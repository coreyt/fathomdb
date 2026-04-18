/// Maximum depth the recursive property-FTS extraction walk descends before
/// clamping. A walk that reaches this depth will not emit leaves below it.
pub(crate) const MAX_RECURSIVE_DEPTH: usize = 8;

/// Maximum blob size the recursive property-FTS extraction walk will emit.
/// When the next complete leaf would push the blob past this cap, the walk
/// stops on the preceding leaf boundary — the existing blob is indexed, the
/// truncated leaf is not partially emitted.
///
/// This budget applies only to the recursive-walk portion of the blob.
/// Scalar-prefix emissions and the scalar↔recursive [`LEAF_SEPARATOR`] token
/// are not counted against it; the cap governs how much nested-subtree
/// content is serialized, not the final blob length.
pub(crate) const MAX_EXTRACTED_BYTES: usize = 65_536;

/// Hard phrase-break separator inserted between two adjacent recursive
/// leaves in the concatenated blob.
///
/// FTS5 phrase queries match on consecutive token positions, so simply
/// inserting non-token whitespace or control characters does NOT prevent a
/// phrase like `"foo bar"` from matching when `foo` ends one leaf and
/// `bar` starts the next — the tokenizer would discard the control
/// character and the two content tokens would still be adjacent.
///
/// To create a real position gap we embed a sentinel *token*
/// `fathomdbphrasebreaksentinel` between leaves. Under `porter
/// unicode61 remove_diacritics 2` this survives tokenization as its own
/// position, so the content tokens on either side are separated by at
/// least one intervening position and phrase queries spanning the gap
/// cannot match. The sentinel is long, lowercase, and obviously
/// synthetic to minimize the chance of accidental collisions with real
/// content. It remains searchable as a word — callers should avoid
/// passing it to `text_search` directly.
///
/// Validated by the integration test
/// `leaf_separator_is_hard_phrase_break_under_unicode61_porter`.
///
/// Note: because the sentinel is a real token it may appear in FTS5
/// `snippet()` output when the snippet window spans a leaf boundary.
/// Phase 5 presentation-layer snippet post-processing is responsible for
/// stripping it before the snippet is surfaced to callers.
pub(crate) const LEAF_SEPARATOR: &str = " fathomdbphrasebreaksentinel ";

/// Whether a registered property-FTS path extracts a single scalar value
/// or recursively walks the subtree rooted at the path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PropertyPathMode {
    /// Legacy scalar extraction — resolve the path, append the scalar
    /// (flattening arrays of scalars as before Phase 4).
    #[default]
    Scalar,
    /// Recursively walk scalar leaves rooted at the path, emitting one
    /// leaf per scalar with position-map tracking.
    Recursive,
}

/// A single registered property-FTS path with its extraction mode.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PropertyPathEntry {
    pub path: String,
    pub mode: PropertyPathMode,
    /// Optional BM25 weight multiplier parsed from the rich JSON format.
    pub weight: Option<f32>,
}

// f32 does not implement Eq (due to NaN), but weights are always finite in practice.
impl Eq for PropertyPathEntry {}

impl PropertyPathEntry {
    pub(crate) fn scalar(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            mode: PropertyPathMode::Scalar,
            weight: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn recursive(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            mode: PropertyPathMode::Recursive,
            weight: None,
        }
    }
}

/// Parsed property-FTS schema definition for a single kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PropertyFtsSchema {
    /// Property paths to extract. Each entry is matched exactly against
    /// the JSON tree from the node's root; see [`PropertyPathMode`].
    pub paths: Vec<PropertyPathEntry>,
    /// Separator inserted between adjacent leaf values in the extracted
    /// blob. Acts as a hard phrase break under FTS5.
    pub separator: String,
    /// JSON paths to skip during recursive extraction. Matched as an
    /// *exact* path equality — the walk visits each object/array before
    /// descending into it, so listing `$.x.y` suppresses the entire
    /// subtree rooted at `$.x.y`. Prefix matching is NOT supported: an
    /// exclude of `$.x` does not implicitly exclude `$.x.y`, and
    /// `$.payload.priv` does not implicitly exclude `$.payload.priv.inner`
    /// via prefix — the walker will still descend into any subtree that
    /// is not itself listed exactly in `exclude_paths`.
    pub exclude_paths: Vec<String>,
}

/// One position-map entry: a half-open `[start, end)` byte range within the
/// extracted blob plus the `JSONPath` of the scalar leaf whose value occupies
/// that range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PositionEntry {
    pub start_offset: usize,
    pub end_offset: usize,
    pub leaf_path: String,
}

/// Per-kind extraction stats accumulated by a recursive walk. Surface-level
/// Phase 4 logging only; Phase 5 may expose these to callers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExtractStats {
    pub depth_cap_hit: usize,
    /// `true` once the extracted blob has reached `MAX_EXTRACTED_BYTES` and
    /// the walker has stopped accepting additional leaves. Unlike the
    /// depth counter this is a boolean: the walker's `stopped` guard
    /// blocks further `emit_leaf` calls after the first truncation.
    pub byte_cap_reached: bool,
    pub excluded_subtree: usize,
}

impl ExtractStats {
    pub(crate) fn merge(&mut self, other: ExtractStats) {
        self.depth_cap_hit += other.depth_cap_hit;
        self.byte_cap_reached |= other.byte_cap_reached;
        self.excluded_subtree += other.excluded_subtree;
    }
}

pub(super) struct RecursiveWalker {
    pub(super) blob: String,
    pub(super) positions: Vec<PositionEntry>,
    pub(super) stats: ExtractStats,
    pub(super) exclude_paths: Vec<String>,
    /// Once the byte cap is hit, the walker refuses to emit any further
    /// leaves. Subsequent walks still account for depth/exclude stats but
    /// do not append.
    pub(super) stopped: bool,
}

impl RecursiveWalker {
    pub(super) fn walk(&mut self, current_path: &str, value: &serde_json::Value, depth: usize) {
        if self.stopped {
            return;
        }
        if self.exclude_paths.iter().any(|p| p == current_path) {
            self.stats.excluded_subtree += 1;
            return;
        }
        match value {
            serde_json::Value::String(s) => self.emit_leaf(current_path, s),
            serde_json::Value::Number(n) => self.emit_leaf(current_path, &n.to_string()),
            serde_json::Value::Bool(b) => self.emit_leaf(current_path, &b.to_string()),
            serde_json::Value::Null => {}
            serde_json::Value::Object(map) => {
                if depth >= MAX_RECURSIVE_DEPTH {
                    self.stats.depth_cap_hit += 1;
                    return;
                }
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                for key in keys {
                    if self.stopped {
                        return;
                    }
                    let child_path = format!("{current_path}.{key}");
                    if let Some(child) = map.get(key) {
                        self.walk(&child_path, child, depth + 1);
                    }
                }
            }
            serde_json::Value::Array(items) => {
                if depth >= MAX_RECURSIVE_DEPTH {
                    self.stats.depth_cap_hit += 1;
                    return;
                }
                for (idx, item) in items.iter().enumerate() {
                    if self.stopped {
                        return;
                    }
                    let child_path = format!("{current_path}[{idx}]");
                    self.walk(&child_path, item, depth + 1);
                }
            }
        }
    }

    fn emit_leaf(&mut self, leaf_path: &str, value: &str) {
        if self.stopped {
            return;
        }
        if value.is_empty() {
            return;
        }
        // Compute the projected blob size if we accept this leaf.
        // If we already emitted at least one leaf, we must account for
        // the separator that precedes this one.
        let sep_len = if self.blob.is_empty() {
            0
        } else {
            LEAF_SEPARATOR.len()
        };
        let projected_len = self.blob.len() + sep_len + value.len();
        if projected_len > MAX_EXTRACTED_BYTES {
            self.stats.byte_cap_reached = true;
            self.stopped = true;
            return;
        }
        if !self.blob.is_empty() {
            self.blob.push_str(LEAF_SEPARATOR);
        }
        let start_offset = self.blob.len();
        self.blob.push_str(value);
        let end_offset = self.blob.len();
        self.positions.push(PositionEntry {
            start_offset,
            end_offset,
            leaf_path: leaf_path.to_owned(),
        });
    }
}

/// Extract scalar values from a JSON object at a `$.field` path.
/// Supports simple dot-notation paths like `$.name`, `$.address.city`.
/// Returns individual scalars so callers can join them with the configured separator.
pub(crate) fn extract_json_path(value: &serde_json::Value, path: &str) -> Vec<String> {
    let Some(path) = path.strip_prefix("$.") else {
        return Vec::new();
    };
    let mut current = value;
    for segment in path.split('.') {
        match current.get(segment) {
            Some(v) => current = v,
            None => return Vec::new(),
        }
    }
    match current {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Number(n) => vec![n.to_string()],
        serde_json::Value::Bool(b) => vec![b.to_string()],
        serde_json::Value::Null | serde_json::Value::Object(_) => Vec::new(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                serde_json::Value::Bool(b) => Some(b.to_string()),
                _ => None,
            })
            .collect(),
    }
}

/// Core property-FTS extraction. Produces the concatenated blob, the
/// position map identifying which byte range in the blob came from which
/// JSON leaf path, and the guardrail stats for the walk.
///
/// Emission rules:
/// - Path entries are processed in their registered order.
/// - Scalar-mode entries append the scalar value(s) directly (arrays of
///   scalars flatten, matching pre-Phase-4 behavior). Scalar emissions do
///   not produce position-map entries — the position map is only populated
///   when a recursive path contributes a leaf.
/// - Recursive-mode entries walk every descendant scalar leaf in stable
///   lexicographic key order. Each leaf emits a position-map entry whose
///   `[start, end)` spans the leaf value's bytes in the blob (NOT the
///   trailing separator).
/// - Between any two emitted values (whether from the same or different
///   path entries) the walk inserts [`LEAF_SEPARATOR`], a hard phrase
///   break under FTS5's `porter unicode61` tokenizer. Scalar-mode emissions
///   between each other use the schema's configured `separator` for
///   backwards compatibility with Phase 0.
pub(crate) fn extract_property_fts(
    props: &serde_json::Value,
    schema: &PropertyFtsSchema,
) -> (Option<String>, Vec<PositionEntry>, ExtractStats) {
    let mut walker = RecursiveWalker {
        blob: String::new(),
        positions: Vec::new(),
        stats: ExtractStats::default(),
        exclude_paths: schema.exclude_paths.clone(),
        stopped: false,
    };

    let mut scalar_parts: Vec<String> = Vec::new();

    for entry in &schema.paths {
        match entry.mode {
            PropertyPathMode::Scalar => {
                scalar_parts.extend(extract_json_path(props, &entry.path));
            }
            PropertyPathMode::Recursive => {
                let root = resolve_path_root(props, &entry.path);
                if let Some(root) = root {
                    walker.walk(&entry.path, root, 0);
                }
            }
        }
    }

    // Flush scalar parts into the blob. Scalar emissions go first so that
    // their byte offsets are predictable relative to any recursive leaves
    // that follow. Scalars use the configured `separator`; the boundary
    // between scalars and recursive leaves uses `LEAF_SEPARATOR` (so phrase
    // queries cannot cross it).
    let scalar_text = if scalar_parts.is_empty() {
        None
    } else {
        Some(scalar_parts.join(&schema.separator))
    };

    let combined = match (scalar_text, walker.blob.is_empty()) {
        (None, true) => None,
        (None, false) => Some(walker.blob.clone()),
        (Some(s), true) => Some(s),
        (Some(mut s), false) => {
            // Shift recursive positions by the prefix length so they stay
            // correct relative to the combined blob.
            let offset = s.len() + LEAF_SEPARATOR.len();
            for pos in &mut walker.positions {
                pos.start_offset += offset;
                pos.end_offset += offset;
            }
            s.push_str(LEAF_SEPARATOR);
            s.push_str(&walker.blob);
            Some(s)
        }
    };

    (combined, walker.positions, walker.stats)
}

/// Extract per-column FTS text for a node's properties, one entry per spec.
///
/// Returns `Vec<(column_name, text)>` in spec order.
pub(crate) fn extract_property_fts_columns(
    props: &serde_json::Value,
    schema: &PropertyFtsSchema,
) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for entry in &schema.paths {
        let is_recursive = matches!(entry.mode, PropertyPathMode::Recursive);
        let column_name = fathomdb_schema::fts_column_name(&entry.path, is_recursive);
        let text = match entry.mode {
            PropertyPathMode::Scalar => {
                let parts = extract_json_path(props, &entry.path);
                parts.join(&schema.separator)
            }
            PropertyPathMode::Recursive => {
                let mut walker = RecursiveWalker {
                    blob: String::new(),
                    positions: Vec::new(),
                    stats: ExtractStats::default(),
                    exclude_paths: schema.exclude_paths.clone(),
                    stopped: false,
                };
                if let Some(root) = resolve_path_root(props, &entry.path) {
                    walker.walk(&entry.path, root, 0);
                }
                walker.blob
            }
        };
        result.push((column_name, text));
    }
    result
}

pub(crate) fn resolve_path_root<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let stripped = path.strip_prefix("$.")?;
    let mut current = value;
    for segment in stripped.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

/// Load all registered FTS property schemas from the database, tolerating
/// both the legacy JSON shape (array of bare path strings = scalar mode)
/// and the Phase 4 shape (objects carrying `path`, `mode`, optional
/// `exclude_paths`, or a top-level object carrying `paths` + global
/// `exclude_paths`).
pub(crate) fn load_fts_property_schemas(
    conn: &rusqlite::Connection,
) -> Result<Vec<(String, PropertyFtsSchema)>, rusqlite::Error> {
    let mut stmt =
        conn.prepare("SELECT kind, property_paths_json, separator FROM fts_property_schemas")?;
    stmt.query_map([], |row| {
        let kind: String = row.get(0)?;
        let paths_json: String = row.get(1)?;
        let separator: String = row.get(2)?;
        let schema = parse_property_schema_json(&paths_json, &separator);
        Ok((kind, schema))
    })?
    .collect::<Result<Vec<_>, _>>()
}

pub(crate) fn parse_property_schema_json(paths_json: &str, separator: &str) -> PropertyFtsSchema {
    let value: serde_json::Value = serde_json::from_str(paths_json).unwrap_or_default();
    let mut paths = Vec::new();
    let mut exclude_paths: Vec<String> = Vec::new();

    let path_values: Vec<serde_json::Value> = match value {
        serde_json::Value::Array(arr) => arr,
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::Array(excl)) = map.get("exclude_paths") {
                exclude_paths = excl
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
            }
            match map.get("paths") {
                Some(serde_json::Value::Array(arr)) => arr.clone(),
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    };

    for entry in path_values {
        match entry {
            serde_json::Value::String(path) => {
                paths.push(PropertyPathEntry::scalar(path));
            }
            serde_json::Value::Object(map) => {
                let Some(path) = map.get("path").and_then(|v| v.as_str()) else {
                    continue;
                };
                let mode = map.get("mode").and_then(|v| v.as_str()).map_or(
                    PropertyPathMode::Scalar,
                    |m| match m {
                        "recursive" => PropertyPathMode::Recursive,
                        _ => PropertyPathMode::Scalar,
                    },
                );
                #[allow(clippy::cast_possible_truncation)]
                let weight = map
                    .get("weight")
                    .and_then(serde_json::Value::as_f64)
                    .map(|w| w as f32);
                paths.push(PropertyPathEntry {
                    path: path.to_owned(),
                    mode,
                    weight,
                });
                if let Some(serde_json::Value::Array(excl)) = map.get("exclude_paths") {
                    for p in excl {
                        if let Some(s) = p.as_str() {
                            exclude_paths.push(s.to_owned());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    PropertyFtsSchema {
        paths,
        separator: separator.to_owned(),
        exclude_paths,
    }
}
