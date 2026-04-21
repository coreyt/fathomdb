use rusqlite::{OptionalExtension, TransactionBehavior};

use super::{
    AdminService, EngineError, FtsProfile, FtsPropertyPathMode, FtsPropertyPathSpec,
    FtsPropertySchemaRecord, RebuildMode, RebuildRequest, resolve_tokenizer_preset,
};

impl AdminService {
    /// Persist or update the FTS tokenizer profile for a node kind.
    ///
    /// `tokenizer_str` may be a preset name (see [`TOKENIZER_PRESETS`]) or a
    /// raw FTS5 tokenizer string.  The resolved string is validated before
    /// being written to `projection_profiles`.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the tokenizer string contains disallowed
    /// characters, or if the database write fails.
    pub fn set_fts_profile(
        &self,
        kind: &str,
        tokenizer_str: &str,
    ) -> Result<FtsProfile, EngineError> {
        let resolved = resolve_tokenizer_preset(tokenizer_str);
        // Allowed chars: alphanumeric, space, apostrophe, dot, underscore, hyphen, dollar, at
        if !resolved
            .chars()
            .all(|c| c.is_alphanumeric() || "'._-$@ ".contains(c))
        {
            return Err(EngineError::Bridge(format!(
                "invalid tokenizer string: {resolved:?}"
            )));
        }
        let conn = self.connect()?;
        conn.execute(
            r"INSERT INTO projection_profiles (kind, facet, config_json, active_at, created_at)
              VALUES (?1, 'fts', json_object('tokenizer', ?2), unixepoch(), unixepoch())
              ON CONFLICT(kind, facet) DO UPDATE SET
                  config_json = json_object('tokenizer', ?2),
                  active_at   = unixepoch()",
            rusqlite::params![kind, resolved],
        )?;
        let row = conn.query_row(
            "SELECT kind, json_extract(config_json, '$.tokenizer'), active_at, created_at \
             FROM projection_profiles WHERE kind = ?1 AND facet = 'fts'",
            rusqlite::params![kind],
            |row| {
                Ok(FtsProfile {
                    kind: row.get(0)?,
                    tokenizer: row.get(1)?,
                    active_at: row.get(2)?,
                    created_at: row.get(3)?,
                })
            },
        )?;
        Ok(row)
    }

    /// Retrieve the FTS tokenizer profile for a node kind.
    ///
    /// Returns `None` if no profile has been set for `kind`.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn get_fts_profile(&self, kind: &str) -> Result<Option<FtsProfile>, EngineError> {
        let conn = self.connect()?;
        let result = conn
            .query_row(
                "SELECT kind, json_extract(config_json, '$.tokenizer'), active_at, created_at \
                 FROM projection_profiles WHERE kind = ?1 AND facet = 'fts'",
                rusqlite::params![kind],
                |row| {
                    Ok(FtsProfile {
                        kind: row.get(0)?,
                        tokenizer: row.get(1)?,
                        active_at: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                },
            )
            .optional()?;
        Ok(result)
    }

    /// Register (or update) an FTS property projection schema for the given node kind.
    ///
    /// After registration, any node of this kind will have the declared JSON property
    /// paths extracted, concatenated, and indexed in the per-kind `fts_props_<kind>` FTS5 table.
    ///
    /// # Errors
    /// Returns [`EngineError`] if `property_paths` is empty, contains duplicates,
    /// or if the database write fails.
    pub fn register_fts_property_schema(
        &self,
        kind: &str,
        property_paths: &[String],
        separator: Option<&str>,
    ) -> Result<FtsPropertySchemaRecord, EngineError> {
        let specs: Vec<FtsPropertyPathSpec> = property_paths
            .iter()
            .map(|p| FtsPropertyPathSpec::scalar(p.clone()))
            .collect();
        self.register_fts_property_schema_with_entries(
            kind,
            &specs,
            separator,
            &[],
            RebuildMode::Eager,
        )
    }

    /// Register (or update) an FTS property projection schema with
    /// per-path modes and optional exclude paths.
    ///
    /// Under `RebuildMode::Eager` (the legacy mode), the full rebuild runs
    /// inside the registration transaction — same behavior as before Pack 7.
    ///
    /// Under `RebuildMode::Async` (the 0.4.1 default), the schema row is
    /// persisted in a short IMMEDIATE transaction, a rebuild-state row is
    /// upserted, and the actual rebuild is handed off to the background
    /// `RebuildActor`.  The register call returns in <100ms even for large
    /// kinds.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the paths are invalid, the JSON
    /// serialization fails, or the (schema-persist / rebuild) transaction fails.
    pub fn register_fts_property_schema_with_entries(
        &self,
        kind: &str,
        entries: &[FtsPropertyPathSpec],
        separator: Option<&str>,
        exclude_paths: &[String],
        mode: RebuildMode,
    ) -> Result<FtsPropertySchemaRecord, EngineError> {
        let paths: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
        validate_fts_property_paths(&paths)?;
        for p in exclude_paths {
            if !p.starts_with("$.") {
                return Err(EngineError::InvalidWrite(format!(
                    "exclude_paths entries must start with '$.' but got: {p}"
                )));
            }
        }
        for e in entries {
            if let Some(w) = e.weight
                && !(w > 0.0 && w <= 1000.0)
            {
                return Err(EngineError::Bridge(format!(
                    "weight out of range: {w} (must satisfy 0.0 < weight <= 1000.0)"
                )));
            }
        }
        let separator = separator.unwrap_or(" ");
        let paths_json = serialize_property_paths_json(entries, exclude_paths)?;

        match mode {
            RebuildMode::Eager => self.register_fts_property_schema_eager(
                kind,
                entries,
                separator,
                exclude_paths,
                &paths,
                &paths_json,
            ),
            RebuildMode::Async => self.register_fts_property_schema_async(
                kind,
                entries,
                separator,
                &paths,
                &paths_json,
            ),
        }
    }

    /// Eager path: existing transactional behavior unchanged.
    fn register_fts_property_schema_eager(
        &self,
        kind: &str,
        entries: &[FtsPropertyPathSpec],
        separator: &str,
        exclude_paths: &[String],
        paths: &[String],
        paths_json: &str,
    ) -> Result<FtsPropertySchemaRecord, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        // Determine whether the registration introduces a recursive path
        // that was not present in the previously-registered schema for
        // this kind. If so, we must eagerly rebuild property FTS rows and
        // position map for every active node of this kind within the same
        // transaction.
        let previous_row: Option<(String, String)> = tx
            .query_row(
                "SELECT property_paths_json, separator FROM fts_property_schemas WHERE kind = ?1",
                [kind],
                |row| {
                    let json: String = row.get(0)?;
                    let sep: String = row.get(1)?;
                    Ok((json, sep))
                },
            )
            .optional()?;
        let had_previous_schema = previous_row.is_some();
        let previous_recursive_paths: Vec<String> = previous_row
            .map(|(json, sep)| crate::writer::parse_property_schema_json(&json, &sep))
            .map_or(Vec::new(), |schema| {
                schema
                    .paths
                    .into_iter()
                    .filter(|p| p.mode == crate::writer::PropertyPathMode::Recursive)
                    .map(|p| p.path)
                    .collect()
            });
        let new_recursive_paths: Vec<&str> = entries
            .iter()
            .filter(|e| e.mode == FtsPropertyPathMode::Recursive)
            .map(|e| e.path.as_str())
            .collect();
        let introduces_new_recursive = new_recursive_paths
            .iter()
            .any(|p| !previous_recursive_paths.iter().any(|prev| prev == p));

        tx.execute(
            "INSERT INTO fts_property_schemas (kind, property_paths_json, separator) \
             VALUES (?1, ?2, ?3) \
             ON CONFLICT(kind) DO UPDATE SET property_paths_json = ?2, separator = ?3",
            rusqlite::params![kind, paths_json, separator],
        )?;

        // Eager transactional rebuild: always fire on any registration or update.
        // First-time registrations must populate the per-kind FTS table from any
        // existing nodes; updates must clear and re-populate so stale rows don't
        // linger. This covers recursive-path additions AND scalar-only
        // re-registrations where only the path or separator changed. (P4-P2-1)
        let _ = (introduces_new_recursive, had_previous_schema);
        let needs_rebuild = true;
        if needs_rebuild {
            let any_weight = entries.iter().any(|e| e.weight.is_some());
            let tok = fathomdb_schema::resolve_fts_tokenizer(&tx, kind)
                .map_err(|e| EngineError::Bridge(e.to_string()))?;
            if any_weight {
                // Per-spec column mode: drop and recreate the table with one column
                // per spec. Data population into per-spec columns is future work;
                // the table is left empty after recreation.
                create_or_replace_fts_kind_table(&tx, kind, entries, &tok)?;
                tx.execute(
                    "DELETE FROM fts_node_property_positions WHERE kind = ?1",
                    [kind],
                )?;
                // Skip insert_property_fts_rows_for_kind — it uses text_content
                // which is not present in the per-spec column layout.
            } else {
                // Legacy text_content mode: drop and recreate the table to ensure
                // the correct single-column layout (handles weighted-to-unweighted
                // downgrade where a stale per-spec table might otherwise remain).
                create_or_replace_fts_kind_table(&tx, kind, &[], &tok)?;
                tx.execute(
                    "DELETE FROM fts_node_property_positions WHERE kind = ?1",
                    [kind],
                )?;
                // Scope the rebuild to `kind` only. The multi-kind
                // `insert_property_fts_rows` iterates over every registered
                // schema and would re-insert rows for siblings that were not
                // deleted above, duplicating their FTS entries.
                crate::projection::insert_property_fts_rows_for_kind(&tx, kind)?;
            }
        }

        super::persist_simple_provenance_event(
            &tx,
            "fts_property_schema_registered",
            kind,
            Some(serde_json::json!({
                "property_paths": paths,
                "separator": separator,
                "exclude_paths": exclude_paths,
                "eager_rebuild": needs_rebuild,
            })),
        )?;
        tx.commit()?;

        self.describe_fts_property_schema(kind)?.ok_or_else(|| {
            EngineError::Bridge("registered FTS property schema missing after commit".to_owned())
        })
    }

    /// Async path: schema persisted in a short tx; rebuild handed to actor.
    fn register_fts_property_schema_async(
        &self,
        kind: &str,
        entries: &[FtsPropertyPathSpec],
        separator: &str,
        paths: &[String],
        paths_json: &str,
    ) -> Result<FtsPropertySchemaRecord, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        // Detect first-registration vs re-registration.
        let had_previous_schema: bool = tx
            .query_row(
                "SELECT count(*) FROM fts_property_schemas WHERE kind = ?1",
                rusqlite::params![kind],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;

        // Upsert schema row (fast — just a metadata write).
        tx.execute(
            "INSERT INTO fts_property_schemas (kind, property_paths_json, separator) \
             VALUES (?1, ?2, ?3) \
             ON CONFLICT(kind) DO UPDATE SET property_paths_json = ?2, separator = ?3",
            rusqlite::params![kind, paths_json, separator],
        )?;

        // Preserve the live per-kind FTS table when the new schema is
        // shape-compatible with the existing one. Readers arriving during
        // PENDING/BUILDING then continue to see the pre-registration rows
        // until the rebuild actor's step 5 atomic swap commits. Only drop
        // when the new schema is shape-incompatible (column set or
        // tokenizer change) — the live table's columns cannot service the
        // new schema in that case. First registration (existing = None)
        // leaves the table alone; the actor's defensive CREATE IF NOT
        // EXISTS in step 5 creates it.
        let any_weight = entries.iter().any(|e| e.weight.is_some());
        let tok = fathomdb_schema::resolve_fts_tokenizer(&tx, kind)
            .map_err(|e| EngineError::Bridge(e.to_string()))?;
        let desired = desired_fts_shape(entries, &tok);
        let existing = fts_kind_table_shape(&tx, kind)?;
        let must_drop = match &existing {
            None => false,
            Some(existing) => !shape_compatible(existing, &desired),
        };
        if must_drop {
            if any_weight {
                create_or_replace_fts_kind_table(&tx, kind, entries, &tok)?;
            } else {
                // Legacy text_content layout — pass empty specs so
                // create_or_replace_fts_kind_table uses the single text_content column.
                create_or_replace_fts_kind_table(&tx, kind, &[], &tok)?;
            }
        }

        // Retrieve the rowid of the schema row as schema_id.
        let schema_id: i64 = tx.query_row(
            "SELECT rowid FROM fts_property_schemas WHERE kind = ?1",
            rusqlite::params![kind],
            |r| r.get(0),
        )?;

        let now_ms = crate::rebuild_actor::now_unix_ms_pub();
        let is_first = i64::from(!had_previous_schema);

        // Upsert rebuild state row.
        tx.execute(
            "INSERT INTO fts_property_rebuild_state \
             (kind, schema_id, state, rows_done, started_at, is_first_registration) \
             VALUES (?1, ?2, 'PENDING', 0, ?3, ?4) \
             ON CONFLICT(kind) DO UPDATE SET \
                 schema_id = excluded.schema_id, \
                 state = 'PENDING', \
                 rows_total = NULL, \
                 rows_done = 0, \
                 started_at = excluded.started_at, \
                 last_progress_at = NULL, \
                 error_message = NULL, \
                 is_first_registration = excluded.is_first_registration",
            rusqlite::params![kind, schema_id, now_ms, is_first],
        )?;

        super::persist_simple_provenance_event(
            &tx,
            "fts_property_schema_registered",
            kind,
            Some(serde_json::json!({
                "property_paths": paths,
                "separator": separator,
                "mode": "async",
            })),
        )?;
        tx.commit()?;

        // Enqueue the rebuild request if the actor is available.
        // try_send is non-blocking: if the channel is full (capacity 64), the
        // request is dropped. The state row stays PENDING and the caller can
        // observe this via get_property_fts_rebuild_state. No automatic retry
        // in 0.4.1 — caller must re-invoke register to re-enqueue.
        if let Some(sender) = &self.rebuild_sender
            && sender
                .try_send(RebuildRequest {
                    kind: kind.to_owned(),
                    schema_id,
                })
                .is_err()
        {
            trace_warn!(
                kind = %kind,
                "rebuild channel full; rebuild request dropped — state remains PENDING"
            );
        }

        self.describe_fts_property_schema(kind)?.ok_or_else(|| {
            EngineError::Bridge("registered FTS property schema missing after commit".to_owned())
        })
    }

    /// Return the rebuild state row for a kind, if one exists.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn get_property_fts_rebuild_state(
        &self,
        kind: &str,
    ) -> Result<Option<crate::rebuild_actor::RebuildStateRow>, EngineError> {
        let conn = self.connect()?;
        let row = conn
            .query_row(
                "SELECT kind, schema_id, state, rows_total, rows_done, \
                 started_at, is_first_registration, error_message \
                 FROM fts_property_rebuild_state WHERE kind = ?1",
                rusqlite::params![kind],
                |r| {
                    Ok(crate::rebuild_actor::RebuildStateRow {
                        kind: r.get(0)?,
                        schema_id: r.get(1)?,
                        state: r.get(2)?,
                        rows_total: r.get(3)?,
                        rows_done: r.get(4)?,
                        started_at: r.get(5)?,
                        is_first_registration: r.get::<_, i64>(6)? != 0,
                        error_message: r.get(7)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// Return the count of rows in `fts_property_rebuild_staging` for a kind.
    /// Used by tests to verify the staging table was populated.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn count_staging_rows(&self, kind: &str) -> Result<i64, EngineError> {
        let conn = self.connect()?;
        let count: i64 = conn.query_row(
            "SELECT count(*) FROM fts_property_rebuild_staging WHERE kind = ?1",
            rusqlite::params![kind],
            |r| r.get(0),
        )?;
        Ok(count)
    }

    /// Return whether a specific node is present in `fts_property_rebuild_staging`.
    /// Used by tests to verify the double-write path.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn staging_row_exists(
        &self,
        kind: &str,
        node_logical_id: &str,
    ) -> Result<bool, EngineError> {
        let conn = self.connect()?;
        let count: i64 = conn.query_row(
            "SELECT count(*) FROM fts_property_rebuild_staging WHERE kind = ?1 AND node_logical_id = ?2",
            rusqlite::params![kind, node_logical_id],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    /// Return the FTS property schema for a single node kind, if registered.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn describe_fts_property_schema(
        &self,
        kind: &str,
    ) -> Result<Option<FtsPropertySchemaRecord>, EngineError> {
        let conn = self.connect()?;
        load_fts_property_schema_record(&conn, kind)
    }

    /// Return all registered FTS property schemas.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn list_fts_property_schemas(&self) -> Result<Vec<FtsPropertySchemaRecord>, EngineError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT kind, property_paths_json, separator, format_version \
             FROM fts_property_schemas ORDER BY kind",
        )?;
        let records = stmt
            .query_map([], |row| {
                let kind: String = row.get(0)?;
                let paths_json: String = row.get(1)?;
                let separator: String = row.get(2)?;
                let format_version: i64 = row.get(3)?;
                Ok(build_fts_property_schema_record(
                    kind,
                    &paths_json,
                    separator,
                    format_version,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    /// Remove the FTS property schema for a node kind.
    ///
    /// This does **not** delete existing FTS rows for this kind;
    /// call `rebuild_projections(Fts)` to clean up stale rows.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the kind is not registered or the delete fails.
    pub fn remove_fts_property_schema(&self, kind: &str) -> Result<(), EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let deleted = tx.execute("DELETE FROM fts_property_schemas WHERE kind = ?1", [kind])?;
        if deleted == 0 {
            return Err(EngineError::InvalidWrite(format!(
                "FTS property schema for kind '{kind}' is not registered"
            )));
        }
        // Delete all FTS rows from the per-kind table (if it exists).
        let table = fathomdb_schema::fts_kind_table_name(kind);
        let table_exists: bool = tx
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name = ?1 \
                 AND sql LIKE 'CREATE VIRTUAL TABLE%'",
                rusqlite::params![table],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if table_exists {
            tx.execute_batch(&format!("DELETE FROM {table}"))?;
        }
        super::persist_simple_provenance_event(&tx, "fts_property_schema_removed", kind, None)?;
        tx.commit()?;
        Ok(())
    }
}

pub(super) fn serialize_property_paths_json(
    entries: &[FtsPropertyPathSpec],
    exclude_paths: &[String],
) -> Result<String, EngineError> {
    // Scalar-only schemas with no exclude_paths and no weights are
    // serialised in the legacy shape (bare array of strings) for full
    // backwards compatibility with earlier schema versions.
    let all_scalar = entries
        .iter()
        .all(|e| e.mode == FtsPropertyPathMode::Scalar);
    let any_weight = entries.iter().any(|e| e.weight.is_some());
    if all_scalar && exclude_paths.is_empty() && !any_weight {
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        return serde_json::to_string(&paths).map_err(|e| {
            EngineError::InvalidWrite(format!("failed to serialize property paths: {e}"))
        });
    }

    let mut obj = serde_json::Map::new();
    let paths_json: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            let mode_str = match e.mode {
                FtsPropertyPathMode::Scalar => "scalar",
                FtsPropertyPathMode::Recursive => "recursive",
            };
            let mut entry = serde_json::json!({ "path": e.path, "mode": mode_str });
            if let Some(w) = e.weight {
                entry["weight"] = serde_json::json!(w);
            }
            entry
        })
        .collect();
    obj.insert("paths".to_owned(), serde_json::Value::Array(paths_json));
    if !exclude_paths.is_empty() {
        obj.insert("exclude_paths".to_owned(), serde_json::json!(exclude_paths));
    }
    serde_json::to_string(&serde_json::Value::Object(obj))
        .map_err(|e| EngineError::InvalidWrite(format!("failed to serialize property paths: {e}")))
}

/// Shape of the per-kind FTS5 virtual table — tokenizer string and the
/// sorted set of non-metadata indexed column names.
///
/// Used by `register_fts_property_schema_async` to decide whether a
/// re-registration can preserve the existing live table (shape-compatible)
/// or must drop and recreate (shape-incompatible).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FtsTableShape {
    pub tokenizer: String,
    /// Sorted list of indexed (non-`UNINDEXED`, non-`node_logical_id`) columns.
    pub columns: Vec<String>,
}

/// Read the current shape of the per-kind FTS5 virtual table, if it exists.
///
/// Returns `None` when the table is absent. Parses columns via
/// `PRAGMA table_info` and the tokenizer clause from the
/// `CREATE VIRTUAL TABLE` SQL stored in `sqlite_master`.
pub(super) fn fts_kind_table_shape(
    conn: &rusqlite::Connection,
    kind: &str,
) -> Result<Option<FtsTableShape>, EngineError> {
    let table = fathomdb_schema::fts_kind_table_name(kind);
    let create_sql: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1 \
             AND sql LIKE 'CREATE VIRTUAL TABLE%'",
            rusqlite::params![table],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
    let Some(create_sql) = create_sql else {
        return Ok(None);
    };

    // Extract the tokenizer= clause: tokenize='...'
    let tokenizer = extract_tokenizer_clause(&create_sql).unwrap_or_default();

    // Read columns via PRAGMA table_info.
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
    let mut columns: Vec<String> = rows
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|c| c != "node_logical_id")
        .collect();
    columns.sort();

    Ok(Some(FtsTableShape { tokenizer, columns }))
}

/// Compute the shape that `create_or_replace_fts_kind_table` would
/// produce for the given specs and tokenizer.
pub(super) fn desired_fts_shape(specs: &[FtsPropertyPathSpec], tokenizer: &str) -> FtsTableShape {
    // Mirror the branch in `register_fts_property_schema_async`:
    // if any spec carries a weight the table uses per-spec columns; otherwise
    // it uses the single legacy `text_content` column.
    let any_weight = specs.iter().any(|s| s.weight.is_some());
    let mut columns: Vec<String> = if any_weight {
        specs
            .iter()
            .map(|s| {
                let is_recursive = matches!(s.mode, FtsPropertyPathMode::Recursive);
                fathomdb_schema::fts_column_name(&s.path, is_recursive)
            })
            .collect()
    } else {
        vec!["text_content".to_owned()]
    };
    columns.sort();
    FtsTableShape {
        tokenizer: tokenizer.to_owned(),
        columns,
    }
}

/// Return true iff two FTS table shapes have identical tokenizer and
/// identical (sorted) column sets. The `tokenizer` comparison is a
/// plain string equality after extracting the value from the
/// `tokenize='...'` clause.
pub(super) fn shape_compatible(existing: &FtsTableShape, desired: &FtsTableShape) -> bool {
    existing.tokenizer == desired.tokenizer && existing.columns == desired.columns
}

/// Parse the value of a `tokenize='...'` clause from a CREATE VIRTUAL
/// TABLE SQL statement. Returns `None` if no such clause is present.
fn extract_tokenizer_clause(sql: &str) -> Option<String> {
    let lower = sql.to_lowercase();
    let key_idx = lower.find("tokenize")?;
    let after_key = &sql[key_idx..];
    // Advance past "tokenize", optional spaces, '=', optional spaces.
    let eq_rel = after_key.find('=')?;
    let rest = &after_key[eq_rel + 1..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('\'')?;
    // Find the closing single quote, respecting doubled-single-quote escape.
    let bytes = rest.as_bytes();
    let mut i = 0;
    let mut out = String::new();
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '\'' {
            if i + 1 < bytes.len() && bytes[i + 1] as char == '\'' {
                out.push('\'');
                i += 2;
                continue;
            }
            return Some(out);
        }
        out.push(c);
        i += 1;
    }
    None
}

/// Drop and recreate the per-kind FTS5 virtual table with one column per spec.
///
/// The tokenizer string is validated before interpolation into DDL to
/// prevent SQL injection.  If `specs` is empty a single `text_content`
/// column is used (matching the migration-21 baseline shape).
pub(super) fn create_or_replace_fts_kind_table(
    conn: &rusqlite::Connection,
    kind: &str,
    specs: &[FtsPropertyPathSpec],
    tokenizer: &str,
) -> Result<(), EngineError> {
    let table = fathomdb_schema::fts_kind_table_name(kind);

    // Validate tokenizer string: alphanumeric plus the set used by all known presets.
    // Must match the allowlist in `set_fts_profile` so that profiles written by one
    // function are accepted by the other.  The source-code preset
    // (`"unicode61 tokenchars '._-$@'"`) requires `.`, `-`, `$`, `@`.
    if !tokenizer
        .chars()
        .all(|c| c.is_alphanumeric() || "'._-$@ ".contains(c))
    {
        return Err(EngineError::Bridge(format!(
            "invalid tokenizer string: {tokenizer:?}"
        )));
    }

    let cols: Vec<String> = if specs.is_empty() {
        vec![
            "node_logical_id UNINDEXED".to_owned(),
            "text_content".to_owned(),
        ]
    } else {
        std::iter::once("node_logical_id UNINDEXED".to_owned())
            .chain(specs.iter().map(|s| {
                let is_recursive = matches!(s.mode, FtsPropertyPathMode::Recursive);
                fathomdb_schema::fts_column_name(&s.path, is_recursive)
            }))
            .collect()
    };

    // Escape inner apostrophes so the SQL single-quoted tokenize= clause is valid.
    // "unicode61 tokenchars '._-$@'" → "unicode61 tokenchars ''._-$@''"
    let tokenizer_sql = tokenizer.replace('\'', "''");
    conn.execute_batch(&format!(
        "DROP TABLE IF EXISTS {table}; \
         CREATE VIRTUAL TABLE {table} USING fts5({cols}, tokenize='{tokenizer_sql}');",
        cols = cols.join(", "),
    ))?;

    Ok(())
}

pub(super) fn validate_fts_property_paths(paths: &[String]) -> Result<(), EngineError> {
    if paths.is_empty() {
        return Err(EngineError::InvalidWrite(
            "FTS property paths must not be empty".to_owned(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for path in paths {
        if !path.starts_with("$.") {
            return Err(EngineError::InvalidWrite(format!(
                "FTS property path must start with '$.' but got: {path}"
            )));
        }
        let after_prefix = &path[2..]; // safe: already validated "$." prefix
        let segments: Vec<&str> = after_prefix.split('.').collect();
        if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
            return Err(EngineError::InvalidWrite(format!(
                "FTS property path has empty segment(s): {path}"
            )));
        }
        for seg in &segments {
            if !seg.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Err(EngineError::InvalidWrite(format!(
                    "FTS property path segment contains invalid characters: {path}"
                )));
            }
        }
        if !seen.insert(path) {
            return Err(EngineError::InvalidWrite(format!(
                "duplicate FTS property path: {path}"
            )));
        }
    }
    Ok(())
}

pub(super) fn load_fts_property_schema_record(
    conn: &rusqlite::Connection,
    kind: &str,
) -> Result<Option<FtsPropertySchemaRecord>, EngineError> {
    let row = conn
        .query_row(
            "SELECT kind, property_paths_json, separator, format_version \
             FROM fts_property_schemas WHERE kind = ?1",
            [kind],
            |row| {
                let kind: String = row.get(0)?;
                let paths_json: String = row.get(1)?;
                let separator: String = row.get(2)?;
                let format_version: i64 = row.get(3)?;
                Ok(build_fts_property_schema_record(
                    kind,
                    &paths_json,
                    separator,
                    format_version,
                ))
            },
        )
        .optional()?;
    Ok(row)
}

/// Build an [`FtsPropertySchemaRecord`] from a raw
/// `fts_property_schemas` row. Delegates JSON parsing to
/// [`crate::writer::parse_property_schema_json`] — the same parser the
/// recursive walker uses at rebuild time — so both the legacy bare-array
/// shape and the Phase 4 object-shaped envelope round-trip correctly.
pub(super) fn build_fts_property_schema_record(
    kind: String,
    paths_json: &str,
    separator: String,
    format_version: i64,
) -> FtsPropertySchemaRecord {
    let schema = crate::writer::parse_property_schema_json(paths_json, &separator);
    let entries: Vec<FtsPropertyPathSpec> = schema
        .paths
        .into_iter()
        .map(|entry| FtsPropertyPathSpec {
            path: entry.path,
            mode: match entry.mode {
                crate::writer::PropertyPathMode::Scalar => FtsPropertyPathMode::Scalar,
                crate::writer::PropertyPathMode::Recursive => FtsPropertyPathMode::Recursive,
            },
            weight: entry.weight,
        })
        .collect();
    let property_paths: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
    FtsPropertySchemaRecord {
        kind,
        property_paths,
        entries,
        exclude_paths: schema.exclude_paths,
        separator,
        format_version,
    }
}
