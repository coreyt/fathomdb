use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::SyncSender;

use fathomdb_schema::SchemaManager;
use serde::{Deserialize, Serialize};

use crate::rebuild_actor::{RebuildMode, RebuildRequest};

use crate::{
    EngineError, ProjectionRepairReport, ProjectionService, ids::new_id,
    projection::ProjectionTarget, sqlite,
};

mod fts;
mod operational;
mod provenance;
mod vector;

pub use vector::{ConfigureEmbeddingOutcome, load_vector_regeneration_config};

#[cfg(test)]
use fts::{
    create_or_replace_fts_kind_table, serialize_property_paths_json, validate_fts_property_paths,
};

/// Results of a physical and structural integrity check on the database.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IntegrityReport {
    pub physical_ok: bool,
    pub foreign_keys_ok: bool,
    pub missing_fts_rows: usize,
    pub missing_property_fts_rows: usize,
    pub duplicate_active_logical_ids: usize,
    pub operational_missing_collections: usize,
    pub operational_missing_last_mutations: usize,
    pub warnings: Vec<String>,
}

/// A registered FTS property projection schema for a node kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FtsPropertySchemaRecord {
    /// The node kind this schema applies to.
    pub kind: String,
    /// Flat display list of registered JSON property paths
    /// (e.g. `["$.name", "$.title"]`). For recursive entries this lists
    /// only the root path; mode information is carried by
    /// [`Self::entries`].
    pub property_paths: Vec<String>,
    /// Full per-entry schema shape with mode
    /// ([`FtsPropertyPathMode::Scalar`] | [`FtsPropertyPathMode::Recursive`]).
    /// Read this field for mode-accurate round-trip of the registered
    /// schema.
    pub entries: Vec<FtsPropertyPathSpec>,
    /// Subtree paths excluded from recursive walks. Empty for
    /// scalar-only schemas or recursive schemas with no exclusions.
    pub exclude_paths: Vec<String>,
    /// Separator used when concatenating extracted values.
    pub separator: String,
    /// Schema format version.
    pub format_version: i64,
}

/// Extraction mode for a single registered FTS property path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FtsPropertyPathMode {
    /// Resolve the path and append the scalar value(s). Matches legacy
    /// pre-Phase-4 behaviour.
    #[default]
    Scalar,
    /// Recursively walk every scalar leaf rooted at the path. Each leaf
    /// contributes one entry to the position map.
    Recursive,
}

/// A single registered property-FTS path with its extraction mode.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FtsPropertyPathSpec {
    /// JSON path to the property (must start with `$.`).
    pub path: String,
    /// Whether to treat this path as a scalar or recursively walk it.
    pub mode: FtsPropertyPathMode,
    /// Optional BM25 weight multiplier for this path (1.0 = default).
    /// Must satisfy `0.0 < weight <= 1000.0` when set.
    pub weight: Option<f32>,
}

// f32 does not implement Eq (due to NaN), but weights in practice are
// always finite values set by callers, so reflexivity holds.
impl Eq for FtsPropertyPathSpec {}

impl FtsPropertyPathSpec {
    #[must_use]
    pub fn scalar(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            mode: FtsPropertyPathMode::Scalar,
            weight: None,
        }
    }

    #[must_use]
    pub fn recursive(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            mode: FtsPropertyPathMode::Recursive,
            weight: None,
        }
    }

    /// Set the BM25 weight multiplier for this path.
    ///
    /// The weight must satisfy `0.0 < weight <= 1000.0` at registration
    /// time; this builder method does not validate — validation happens in
    /// `register_fts_property_schema_with_entries`.
    #[must_use]
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);
        self
    }
}

/// Options controlling how a safe database export is performed.
#[derive(Clone, Copy, Debug)]
pub struct SafeExportOptions {
    /// When true, runs `PRAGMA wal_checkpoint(FULL)` before copying and fails if
    /// any WAL frames could not be applied (busy != 0). Set to false only in
    /// tests that seed a database without WAL mode.
    pub force_checkpoint: bool,
}

impl Default for SafeExportOptions {
    fn default() -> Self {
        Self {
            force_checkpoint: true,
        }
    }
}

// Must match PROTOCOL_VERSION in fathomdb-admin-bridge.rs
const EXPORT_PROTOCOL_VERSION: u32 = 1;

/// Manifest describing a completed safe export.
#[derive(Clone, Debug, Serialize)]
pub struct SafeExportManifest {
    /// Unix timestamp (seconds since epoch) when the export was created.
    pub exported_at: u64,
    /// SHA-256 hex digest of the exported database file.
    pub sha256: String,
    /// Schema version recorded in `fathom_schema_migrations` at export time.
    pub schema_version: u32,
    /// Bridge protocol version compiled into this binary.
    pub protocol_version: u32,
    /// Number of `SQLite` pages in the exported database file.
    pub page_count: u64,
}

/// Report from tracing all rows associated with a given `source_ref`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TraceReport {
    pub source_ref: String,
    pub node_rows: usize,
    pub edge_rows: usize,
    pub action_rows: usize,
    pub operational_mutation_rows: usize,
    pub node_logical_ids: Vec<String>,
    pub action_ids: Vec<String>,
    pub operational_mutation_ids: Vec<String>,
}

/// An edge that was skipped during a restore because an endpoint is missing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SkippedEdge {
    pub edge_logical_id: String,
    pub missing_endpoint: String,
}

/// Report from restoring a retired logical ID back to active state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LogicalRestoreReport {
    pub logical_id: String,
    pub was_noop: bool,
    pub restored_node_rows: usize,
    pub restored_edge_rows: usize,
    pub restored_chunk_rows: usize,
    pub restored_fts_rows: usize,
    pub restored_property_fts_rows: usize,
    pub restored_vec_rows: usize,
    pub skipped_edges: Vec<SkippedEdge>,
    pub notes: Vec<String>,
}

/// Report from permanently purging all rows for a logical ID.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LogicalPurgeReport {
    pub logical_id: String,
    pub was_noop: bool,
    pub deleted_node_rows: usize,
    pub deleted_edge_rows: usize,
    pub deleted_chunk_rows: usize,
    pub deleted_fts_rows: usize,
    pub deleted_vec_rows: usize,
    pub notes: Vec<String>,
}

/// Options controlling provenance event purging behavior.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProvenancePurgeOptions {
    pub dry_run: bool,
    #[serde(default)]
    pub preserve_event_types: Vec<String>,
}

/// Report from a provenance event purge operation.
#[derive(Clone, Debug, Serialize)]
pub struct ProvenancePurgeReport {
    pub events_deleted: u64,
    pub events_preserved: u64,
    pub oldest_remaining: Option<i64>,
}

/// Service providing administrative operations (integrity checks, exports, restores, purges).
#[derive(Debug)]
pub struct AdminService {
    pub(super) database_path: PathBuf,
    pub(super) schema_manager: Arc<SchemaManager>,
    pub(super) projections: ProjectionService,
    /// Sender side of the rebuild actor's channel.  `None` when the engine
    /// was opened without a rebuild actor (e.g. in tests that use
    /// [`AdminService::new`] directly).
    pub(super) rebuild_sender: Option<SyncSender<RebuildRequest>>,
}

/// Results of a semantic consistency check on the graph data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SemanticReport {
    /// Chunks whose `node_logical_id` has no active node.
    pub orphaned_chunks: usize,
    /// Active nodes with a NULL `source_ref` (loss of provenance).
    pub null_source_ref_nodes: usize,
    /// Steps referencing a `run_id` that does not exist in the runs table.
    pub broken_step_fk: usize,
    /// Actions referencing a `step_id` that does not exist in the steps table.
    pub broken_action_fk: usize,
    /// FTS rows whose `chunk_id` does not exist in the chunks table.
    pub stale_fts_rows: usize,
    /// FTS rows whose node has been superseded (`superseded_at` IS NOT NULL on all active rows).
    pub fts_rows_for_superseded_nodes: usize,
    /// Property FTS rows whose node has been superseded or does not exist.
    pub stale_property_fts_rows: usize,
    /// Property FTS rows whose kind has no registered FTS property schema.
    pub orphaned_property_fts_rows: usize,
    /// Property FTS rows whose `kind` does not match the active node's actual kind.
    pub mismatched_kind_property_fts_rows: usize,
    /// Active logical IDs with more than one per-kind FTS property row.
    pub duplicate_property_fts_rows: usize,
    /// Property FTS rows whose `text_content` no longer matches the canonical extraction.
    pub drifted_property_fts_rows: usize,
    /// Active edges where at least one endpoint has no active node.
    pub dangling_edges: usize,
    /// `logical_ids` where every version has been superseded (no active row).
    pub orphaned_supersession_chains: usize,
    /// Vec rows whose backing chunk no longer exists in the chunks table.
    pub stale_vec_rows: usize,
    /// Compatibility counter for vec rows whose chunk points at missing node history.
    pub vec_rows_for_superseded_nodes: usize,
    /// Latest-state keys whose latest mutation is a `put` but no current row exists.
    pub missing_operational_current_rows: usize,
    /// Current rows that do not match the latest mutation state.
    pub stale_operational_current_rows: usize,
    /// Mutations written after the owning collection was disabled.
    pub disabled_collection_mutations: usize,
    /// Access metadata rows whose `logical_id` no longer has any node history.
    pub orphaned_last_access_metadata_rows: usize,
    pub warnings: Vec<String>,
}

/// Configuration for regenerating vector embeddings.
///
/// 0.4.0 architectural invariant: vector identity is the embedder's
/// responsibility, not the regeneration config's. This struct carries only
/// WHERE the vectors live and HOW to chunk/preprocess them — never WHAT
/// model produced them. The embedder supplied at regen-call time is the
/// single source of truth for `model_identity`, `model_version`,
/// `dimension`, and `normalization_policy`; the resulting vector profile
/// is stamped directly from [`QueryEmbedder::identity`].
///
/// 0.5.0 breaking change: `table_name` is removed. The vec table name is now
/// derived from `kind` via [`fathomdb_schema::vec_kind_table_name`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct VectorRegenerationConfig {
    pub kind: String,
    pub profile: String,
    pub chunking_policy: String,
    pub preprocessing_policy: String,
}

/// Report from a vector embedding regeneration run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VectorRegenerationReport {
    pub profile: String,
    pub table_name: String,
    pub dimension: usize,
    pub total_chunks: usize,
    pub regenerated_rows: usize,
    pub contract_persisted: bool,
    pub notes: Vec<String>,
}

/// Stored FTS tokenizer profile for a node kind.
///
/// Created and updated by [`AdminService::set_fts_profile`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FtsProfile {
    /// Node kind this profile applies to (e.g. `"Article"`).
    pub kind: String,
    /// FTS5 tokenizer string (e.g. `"porter unicode61 remove_diacritics 2"`).
    pub tokenizer: String,
    /// Unix timestamp when the profile was last activated, or `None` if never.
    pub active_at: Option<i64>,
    /// Unix timestamp when the profile row was first created.
    pub created_at: i64,
}

/// Stored vector embedding profile (global, kind-agnostic).
///
/// Created and updated by [`AdminService::set_vec_profile`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VecProfile {
    /// Identifier for the embedding model (e.g. `"openai/text-embedding-3-small"`).
    pub model_identity: String,
    /// Optional version string for the model.
    pub model_version: Option<String>,
    /// Number of dimensions produced by the model.
    pub dimensions: u32,
    /// Unix timestamp when the profile was last activated, or `None` if never.
    pub active_at: Option<i64>,
    /// Unix timestamp when the profile row was first created.
    pub created_at: i64,
}

/// Estimated cost of rebuilding a projection (FTS table or vector embeddings).
///
/// Returned by [`AdminService::preview_projection_impact`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionImpact {
    /// Number of rows that would be processed during a full rebuild.
    pub rows_to_rebuild: u64,
    /// Rough estimated rebuild time in seconds.
    pub estimated_seconds: u64,
    /// Estimated temporary disk space required during rebuild, in bytes.
    pub temp_db_size_bytes: u64,
    /// The tokenizer currently stored in `projection_profiles`, if any.
    pub current_tokenizer: Option<String>,
    /// Reserved for future use; always `None` currently.
    pub target_tokenizer: Option<String>,
}

/// Well-known tokenizer preset names mapped to their FTS5 tokenizer strings.
pub const TOKENIZER_PRESETS: &[(&str, &str)] = &[
    (
        "recall-optimized-english",
        "porter unicode61 remove_diacritics 2",
    ),
    ("precision-optimized", "unicode61 remove_diacritics 2"),
    ("global-cjk", "icu"),
    ("substring-trigram", "trigram"),
    ("source-code", "unicode61 tokenchars '._-$@'"),
];

/// Resolve a tokenizer preset name to its FTS5 tokenizer string.
///
/// If `input` matches a known preset name the preset value is returned.
/// Otherwise `input` is returned unchanged (treated as a raw tokenizer string).
pub fn resolve_tokenizer_preset(input: &str) -> &str {
    for (name, value) in TOKENIZER_PRESETS {
        if *name == input {
            return value;
        }
    }
    input
}

pub(super) const CURRENT_VECTOR_CONTRACT_FORMAT_VERSION: i64 = 1;
pub(super) const MAX_PROFILE_LEN: usize = 128;
pub(super) const MAX_POLICY_LEN: usize = 128;
pub(super) const MAX_CONTRACT_JSON_BYTES: usize = 32 * 1024;
pub(super) const MAX_AUDIT_METADATA_BYTES: usize = 2048;
const DEFAULT_OPERATIONAL_READ_LIMIT: usize = 100;
const MAX_OPERATIONAL_READ_LIMIT: usize = 1000;

/// Thread-safe handle to the shared [`AdminService`].
#[derive(Clone, Debug)]
pub struct AdminHandle {
    inner: Arc<AdminService>,
}

impl AdminHandle {
    /// Wrap an [`AdminService`] in a shared handle.
    #[must_use]
    pub fn new(service: AdminService) -> Self {
        Self {
            inner: Arc::new(service),
        }
    }

    /// Clone the inner `Arc` to the [`AdminService`].
    #[must_use]
    pub fn service(&self) -> Arc<AdminService> {
        Arc::clone(&self.inner)
    }
}

impl AdminService {
    /// Create a new admin service for the database at the given path.
    #[must_use]
    pub fn new(path: impl AsRef<Path>, schema_manager: Arc<SchemaManager>) -> Self {
        let database_path = path.as_ref().to_path_buf();
        let projections = ProjectionService::new(&database_path, Arc::clone(&schema_manager));
        Self {
            database_path,
            schema_manager,
            projections,
            rebuild_sender: None,
        }
    }

    /// Create a new admin service wired to the background rebuild actor.
    #[must_use]
    pub fn new_with_rebuild(
        path: impl AsRef<Path>,
        schema_manager: Arc<SchemaManager>,
        rebuild_sender: SyncSender<RebuildRequest>,
    ) -> Self {
        let database_path = path.as_ref().to_path_buf();
        let projections = ProjectionService::new(&database_path, Arc::clone(&schema_manager));
        Self {
            database_path,
            schema_manager,
            projections,
            rebuild_sender: Some(rebuild_sender),
        }
    }

    pub(super) fn connect(&self) -> Result<rusqlite::Connection, EngineError> {
        #[cfg(feature = "sqlite-vec")]
        let conn = sqlite::open_connection_with_vec(&self.database_path)?;
        #[cfg(not(feature = "sqlite-vec"))]
        let conn = sqlite::open_connection(&self.database_path)?;
        self.schema_manager.bootstrap(&conn)?;
        Ok(conn)
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails or any SQL query fails.
    pub fn check_integrity(&self) -> Result<IntegrityReport, EngineError> {
        let conn = self.connect()?;

        let physical_result: String =
            conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        let foreign_key_count: i64 =
            conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })?;
        let missing_fts_rows: i64 = conn.query_row(
            r"
            SELECT count(*)
            FROM chunks c
            JOIN nodes n
              ON n.logical_id = c.node_logical_id
             AND n.superseded_at IS NULL
            WHERE NOT EXISTS (
                SELECT 1
                FROM fts_nodes f
                WHERE f.chunk_id = c.id
            )
            ",
            [],
            |row| row.get(0),
        )?;
        let duplicate_active: i64 = conn.query_row(
            r"
            SELECT count(*)
            FROM (
                SELECT logical_id
                FROM nodes
                WHERE superseded_at IS NULL
                GROUP BY logical_id
                HAVING count(*) > 1
            )
            ",
            [],
            |row| row.get(0),
        )?;
        let operational_missing_collections: i64 = conn.query_row(
            r"
            SELECT (
                SELECT count(*)
                FROM operational_mutations m
                LEFT JOIN operational_collections c ON c.name = m.collection_name
                WHERE c.name IS NULL
            ) + (
                SELECT count(*)
                FROM operational_current oc
                LEFT JOIN operational_collections c ON c.name = oc.collection_name
                WHERE c.name IS NULL
            )
            ",
            [],
            |row| row.get(0),
        )?;
        let operational_missing_last_mutations: i64 = conn.query_row(
            r"
            SELECT count(*)
            FROM operational_current oc
            LEFT JOIN operational_mutations m ON m.id = oc.last_mutation_id
            WHERE m.id IS NULL
            ",
            [],
            |row| row.get(0),
        )?;

        // Count missing property FTS rows using the same extraction logic as
        // write/rebuild. A pure-SQL check would overcount: nodes whose declared
        // paths legitimately normalize to no values correctly have no row.
        let missing_property_fts_rows = count_missing_property_fts_rows(&conn)?;

        let mut warnings = Vec::new();
        if missing_fts_rows > 0 {
            warnings.push("missing FTS projections detected".to_owned());
        }
        if missing_property_fts_rows > 0 {
            warnings.push("missing property FTS projections detected".to_owned());
        }
        if duplicate_active > 0 {
            warnings.push("duplicate active logical_ids detected".to_owned());
        }
        if operational_missing_collections > 0 {
            warnings.push("operational rows reference missing collections".to_owned());
        }
        if operational_missing_last_mutations > 0 {
            warnings.push("operational current rows reference missing last mutations".to_owned());
        }

        // FIX(review): was `as usize` — unsound on 32-bit targets, wraps negatives silently.
        // Options: (A) try_from().unwrap_or(0) — masks corruption, (B) try_from().expect() —
        // panics on corruption, (C) propagate error. Chose (B) here: a negative count(*)
        // signals data corruption, and the integrity report would be meaningless anyway.
        Ok(IntegrityReport {
            physical_ok: physical_result == "ok",
            foreign_keys_ok: foreign_key_count == 0,
            missing_fts_rows: i64_to_usize(missing_fts_rows),
            missing_property_fts_rows: i64_to_usize(missing_property_fts_rows),
            duplicate_active_logical_ids: i64_to_usize(duplicate_active),
            operational_missing_collections: i64_to_usize(operational_missing_collections),
            operational_missing_last_mutations: i64_to_usize(operational_missing_last_mutations),
            warnings,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails or any SQL query fails.
    #[allow(clippy::too_many_lines)]
    pub fn check_semantics(&self) -> Result<SemanticReport, EngineError> {
        let conn = self.connect()?;

        let orphaned_chunks: i64 = conn.query_row(
            r"
            SELECT count(*)
            FROM chunks c
            WHERE NOT EXISTS (
                SELECT 1 FROM nodes n
                WHERE n.logical_id = c.node_logical_id
            )
            ",
            [],
            |row| row.get(0),
        )?;

        let null_source_ref_nodes: i64 = conn.query_row(
            "SELECT count(*) FROM nodes WHERE source_ref IS NULL AND superseded_at IS NULL",
            [],
            |row| row.get(0),
        )?;

        let broken_step_fk: i64 = conn.query_row(
            r"
            SELECT count(*) FROM steps s
            WHERE NOT EXISTS (SELECT 1 FROM runs r WHERE r.id = s.run_id)
            ",
            [],
            |row| row.get(0),
        )?;

        let broken_action_fk: i64 = conn.query_row(
            r"
            SELECT count(*) FROM actions a
            WHERE NOT EXISTS (SELECT 1 FROM steps s WHERE s.id = a.step_id)
            ",
            [],
            |row| row.get(0),
        )?;

        let stale_fts_rows: i64 = conn.query_row(
            r"
            SELECT count(*) FROM fts_nodes f
            WHERE NOT EXISTS (SELECT 1 FROM chunks c WHERE c.id = f.chunk_id)
            ",
            [],
            |row| row.get(0),
        )?;

        let fts_rows_for_superseded_nodes: i64 = conn.query_row(
            r"
            SELECT count(*) FROM fts_nodes f
            WHERE NOT EXISTS (
                SELECT 1 FROM nodes n
                WHERE n.logical_id = f.node_logical_id AND n.superseded_at IS NULL
            )
            ",
            [],
            |row| row.get(0),
        )?;

        let (
            stale_property_fts_rows,
            orphaned_property_fts_rows,
            mismatched_kind_property_fts_rows,
            duplicate_property_fts_rows,
        ) = count_per_kind_property_fts_issues(&conn)?;

        let drifted_property_fts_rows = count_drifted_property_fts_rows(&conn)?;

        let dangling_edges: i64 = conn.query_row(
            r"
            SELECT count(*) FROM edges e
            WHERE e.superseded_at IS NULL AND (
                NOT EXISTS (SELECT 1 FROM nodes n WHERE n.logical_id = e.source_logical_id AND n.superseded_at IS NULL)
                OR
                NOT EXISTS (SELECT 1 FROM nodes n WHERE n.logical_id = e.target_logical_id AND n.superseded_at IS NULL)
            )
            ",
            [],
            |row| row.get(0),
        )?;

        let orphaned_supersession_chains: i64 = conn.query_row(
            r"
            SELECT count(*) FROM (
                SELECT logical_id FROM nodes
                GROUP BY logical_id
                HAVING count(*) > 0 AND sum(CASE WHEN superseded_at IS NULL THEN 1 ELSE 0 END) = 0
            )
            ",
            [],
            |row| row.get(0),
        )?;

        // Vec stale row detection — iterates per-kind vec tables from projection_profiles.
        #[cfg(feature = "sqlite-vec")]
        let (stale_vec_rows, vec_rows_for_superseded_nodes): (i64, i64) = {
            let kinds: Vec<String> =
                match conn.prepare("SELECT kind FROM projection_profiles WHERE facet = 'vec'") {
                    Ok(mut stmt) => stmt
                        .query_map([], |row| row.get(0))
                        .map_err(EngineError::Sqlite)?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(EngineError::Sqlite)?,
                    Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                        if msg.contains("no such table: projection_profiles") =>
                    {
                        vec![]
                    }
                    Err(e) => return Err(EngineError::Sqlite(e)),
                };
            let mut stale = 0i64;
            let mut superseded = 0i64;
            for kind in &kinds {
                let table = fathomdb_schema::vec_kind_table_name(kind);
                let stale_sql = format!(
                    "SELECT count(*) FROM {table} v \
                     WHERE NOT EXISTS (SELECT 1 FROM chunks c WHERE c.id = v.chunk_id)"
                );
                let superseded_sql = format!(
                    "SELECT count(*) FROM {table} v \
                     JOIN chunks c ON c.id = v.chunk_id \
                     WHERE NOT EXISTS (SELECT 1 FROM nodes n WHERE n.logical_id = c.node_logical_id)"
                );
                stale += match conn.query_row(&stale_sql, [], |row| row.get(0)) {
                    Ok(n) => n,
                    Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                        if msg.contains("no such table:")
                            || msg.contains("no such module: vec0") =>
                    {
                        0
                    }
                    Err(e) => return Err(EngineError::Sqlite(e)),
                };
                superseded += match conn.query_row(&superseded_sql, [], |row| row.get(0)) {
                    Ok(n) => n,
                    Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                        if msg.contains("no such table:")
                            || msg.contains("no such module: vec0") =>
                    {
                        0
                    }
                    Err(e) => return Err(EngineError::Sqlite(e)),
                };
            }
            (stale, superseded)
        };
        #[cfg(not(feature = "sqlite-vec"))]
        let stale_vec_rows: i64 = 0;
        #[cfg(not(feature = "sqlite-vec"))]
        let vec_rows_for_superseded_nodes: i64 = 0;
        let missing_operational_current_rows: i64 = conn.query_row(
            r"
            SELECT count(*)
            FROM operational_mutations m
            JOIN operational_collections c
              ON c.name = m.collection_name
             AND c.kind = 'latest_state'
            WHERE m.op_kind = 'put'
              AND NOT EXISTS (
                    SELECT 1
                    FROM operational_mutations newer
                    WHERE newer.collection_name = m.collection_name
                      AND newer.record_key = m.record_key
                      AND newer.mutation_order > m.mutation_order
                )
              AND NOT EXISTS (
                    SELECT 1
                    FROM operational_current oc
                    WHERE oc.collection_name = m.collection_name
                      AND oc.record_key = m.record_key
                )
            ",
            [],
            |row| row.get(0),
        )?;
        let stale_operational_current_rows: i64 = conn.query_row(
            r"
            SELECT count(*)
            FROM operational_current oc
            JOIN operational_collections c
              ON c.name = oc.collection_name
             AND c.kind = 'latest_state'
            LEFT JOIN operational_mutations m ON m.id = oc.last_mutation_id
            WHERE m.id IS NULL
               OR m.collection_name != oc.collection_name
               OR m.record_key != oc.record_key
               OR m.op_kind != 'put'
               OR m.payload_json != oc.payload_json
               OR EXISTS (
                    SELECT 1
                    FROM operational_mutations newer
                    WHERE newer.collection_name = oc.collection_name
                      AND newer.record_key = oc.record_key
                      AND newer.mutation_order > m.mutation_order
                )
            ",
            [],
            |row| row.get(0),
        )?;
        let disabled_collection_mutations: i64 = conn.query_row(
            r"
            SELECT count(*)
            FROM operational_mutations m
            JOIN operational_collections c ON c.name = m.collection_name
            WHERE c.disabled_at IS NOT NULL AND m.created_at > c.disabled_at
            ",
            [],
            |row| row.get(0),
        )?;
        let orphaned_last_access_metadata_rows: i64 = conn.query_row(
            r"
            SELECT count(*)
            FROM node_access_metadata am
            WHERE NOT EXISTS (
                SELECT 1 FROM nodes n WHERE n.logical_id = am.logical_id
            )
            ",
            [],
            |row| row.get(0),
        )?;

        let mut warnings = Vec::new();
        if orphaned_chunks > 0 {
            warnings.push(format!(
                "{orphaned_chunks} orphaned chunk(s) with no surviving node history"
            ));
        }
        if null_source_ref_nodes > 0 {
            warnings.push(format!(
                "{null_source_ref_nodes} active node(s) with null source_ref"
            ));
        }
        if broken_step_fk > 0 {
            warnings.push(format!(
                "{broken_step_fk} step(s) referencing non-existent run"
            ));
        }
        if broken_action_fk > 0 {
            warnings.push(format!(
                "{broken_action_fk} action(s) referencing non-existent step"
            ));
        }
        if stale_fts_rows > 0 {
            warnings.push(format!(
                "{stale_fts_rows} stale FTS row(s) referencing missing chunk"
            ));
        }
        if fts_rows_for_superseded_nodes > 0 {
            warnings.push(format!(
                "{fts_rows_for_superseded_nodes} FTS row(s) for superseded node(s)"
            ));
        }
        if stale_property_fts_rows > 0 {
            warnings.push(format!(
                "{stale_property_fts_rows} stale property FTS row(s) for superseded/missing node(s)"
            ));
        }
        if orphaned_property_fts_rows > 0 {
            warnings.push(format!(
                "{orphaned_property_fts_rows} orphaned property FTS row(s) for unregistered kind(s)"
            ));
        }
        if mismatched_kind_property_fts_rows > 0 {
            warnings.push(format!(
                "{mismatched_kind_property_fts_rows} property FTS row(s) whose kind does not match the active node"
            ));
        }
        if duplicate_property_fts_rows > 0 {
            warnings.push(format!(
                "{duplicate_property_fts_rows} active logical ID(s) with duplicate property FTS rows"
            ));
        }
        if drifted_property_fts_rows > 0 {
            warnings.push(format!(
                "{drifted_property_fts_rows} property FTS row(s) with stale text_content"
            ));
        }
        if dangling_edges > 0 {
            warnings.push(format!(
                "{dangling_edges} active edge(s) with missing endpoint node"
            ));
        }
        if orphaned_supersession_chains > 0 {
            warnings.push(format!(
                "{orphaned_supersession_chains} logical_id(s) with all versions superseded"
            ));
        }
        if stale_vec_rows > 0 {
            warnings.push(format!(
                "{stale_vec_rows} stale vec row(s) referencing missing chunk"
            ));
        }
        if vec_rows_for_superseded_nodes > 0 {
            warnings.push(format!(
                "{vec_rows_for_superseded_nodes} vec row(s) whose node history is missing"
            ));
        }
        if missing_operational_current_rows > 0 {
            warnings.push(format!(
                "{missing_operational_current_rows} latest-state key(s) missing operational_current rows"
            ));
        }
        if stale_operational_current_rows > 0 {
            warnings.push(format!(
                "{stale_operational_current_rows} stale operational_current row(s)"
            ));
        }
        if disabled_collection_mutations > 0 {
            warnings.push(format!(
                "{disabled_collection_mutations} mutation(s) were written after collection disable"
            ));
        }
        if orphaned_last_access_metadata_rows > 0 {
            warnings.push(format!(
                "{orphaned_last_access_metadata_rows} last_access metadata row(s) reference missing node history"
            ));
        }

        Ok(SemanticReport {
            orphaned_chunks: i64_to_usize(orphaned_chunks),
            null_source_ref_nodes: i64_to_usize(null_source_ref_nodes),
            broken_step_fk: i64_to_usize(broken_step_fk),
            broken_action_fk: i64_to_usize(broken_action_fk),
            stale_fts_rows: i64_to_usize(stale_fts_rows),
            fts_rows_for_superseded_nodes: i64_to_usize(fts_rows_for_superseded_nodes),
            stale_property_fts_rows: i64_to_usize(stale_property_fts_rows),
            orphaned_property_fts_rows: i64_to_usize(orphaned_property_fts_rows),
            mismatched_kind_property_fts_rows: i64_to_usize(mismatched_kind_property_fts_rows),
            duplicate_property_fts_rows: i64_to_usize(duplicate_property_fts_rows),
            drifted_property_fts_rows: i64_to_usize(drifted_property_fts_rows),
            dangling_edges: i64_to_usize(dangling_edges),
            orphaned_supersession_chains: i64_to_usize(orphaned_supersession_chains),
            stale_vec_rows: i64_to_usize(stale_vec_rows),
            vec_rows_for_superseded_nodes: i64_to_usize(vec_rows_for_superseded_nodes),
            missing_operational_current_rows: i64_to_usize(missing_operational_current_rows),
            stale_operational_current_rows: i64_to_usize(stale_operational_current_rows),
            disabled_collection_mutations: i64_to_usize(disabled_collection_mutations),
            orphaned_last_access_metadata_rows: i64_to_usize(orphaned_last_access_metadata_rows),
            warnings,
        })
    }
}

/// Count per-kind FTS integrity issues across all registered per-kind tables.
/// Returns (stale, orphaned, `mismatched_kind`, duplicate) counts.
///
/// - Stale: rows in a per-kind table whose node is superseded or missing.
/// - Orphaned: rows in a per-kind table for a kind with no registered schema.
/// - Mismatched kind: impossible with per-kind tables (always 0).
/// - Duplicate: same `node_logical_id` appears more than once in any per-kind table.
fn count_per_kind_property_fts_issues(
    conn: &rusqlite::Connection,
) -> Result<(i64, i64, i64, i64), EngineError> {
    // Collect all per-kind virtual tables from sqlite_master.
    // Filter by sql LIKE 'CREATE VIRTUAL TABLE%' to exclude FTS5 shadow tables
    // (e.g. fts_props_goal_data, fts_props_goal_idx) which share the same prefix.
    let per_kind_tables: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master \
             WHERE type='table' AND name LIKE 'fts_props_%' \
             AND sql LIKE 'CREATE VIRTUAL TABLE%'",
        )?;
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };

    let registered_kinds: std::collections::HashSet<String> = {
        let mut stmt = conn.prepare("SELECT kind FROM fts_property_schemas")?;
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<std::collections::HashSet<_>, _>>()?
    };

    let mut stale = 0i64;
    let mut orphaned = 0i64;
    let mut duplicate = 0i64;

    for table in &per_kind_tables {
        // Stale: rows whose node_logical_id has no active node.
        let kind_stale: i64 = conn.query_row(
            &format!(
                "SELECT count(*) FROM {table} fp \
                 WHERE NOT EXISTS (\
                     SELECT 1 FROM nodes n \
                     WHERE n.logical_id = fp.node_logical_id AND n.superseded_at IS NULL\
                 )"
            ),
            [],
            |r| r.get(0),
        )?;
        stale += kind_stale;

        // Duplicate: same node_logical_id more than once.
        let kind_dup: i64 = conn.query_row(
            &format!(
                "SELECT count(*) FROM (\
                     SELECT node_logical_id FROM {table} \
                     GROUP BY node_logical_id HAVING count(*) > 1\
                 )"
            ),
            [],
            |r| r.get(0),
        )?;
        duplicate += kind_dup;

        // Orphaned: this per-kind table has no corresponding schema.
        // Determine which kind this table corresponds to by checking all registered kinds.
        let table_has_schema = registered_kinds
            .iter()
            .any(|k| fathomdb_schema::fts_kind_table_name(k) == *table);
        if !table_has_schema {
            let table_rows: i64 =
                conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
            orphaned += table_rows;
        }
    }

    // Mismatched kind is always 0 with per-kind tables.
    Ok((stale, orphaned, 0, duplicate))
}

/// Count active nodes that should have a property FTS row (extraction yields a value)
/// but don't. Uses the same extraction logic as write/rebuild to avoid false positives
/// for nodes whose declared paths legitimately normalize to no values.
fn count_missing_property_fts_rows(conn: &rusqlite::Connection) -> Result<i64, EngineError> {
    let schemas = crate::writer::load_fts_property_schemas(conn)?;
    if schemas.is_empty() {
        return Ok(0);
    }

    let mut missing = 0i64;
    for (kind, schema) in &schemas {
        let table = fathomdb_schema::fts_kind_table_name(kind);
        // If the per-kind table doesn't exist yet, all nodes with extractable values are missing.
        let table_exists: bool = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                [table.as_str()],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;

        if table_exists {
            let mut stmt = conn.prepare(&format!(
                "SELECT n.logical_id, n.properties FROM nodes n \
                 WHERE n.kind = ?1 AND n.superseded_at IS NULL \
                   AND NOT EXISTS (SELECT 1 FROM {table} fp WHERE fp.node_logical_id = n.logical_id)"
            ))?;
            let rows = stmt.query_map([kind.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (_logical_id, properties_str) = row?;
                let props: serde_json::Value =
                    serde_json::from_str(&properties_str).unwrap_or_default();
                if crate::writer::extract_property_fts(&props, schema)
                    .0
                    .is_some()
                {
                    missing += 1;
                }
            }
        } else {
            // Per-kind table doesn't exist yet — count all nodes with extractable values.
            let mut stmt = conn.prepare(
                "SELECT n.logical_id, n.properties FROM nodes n \
                 WHERE n.kind = ?1 AND n.superseded_at IS NULL",
            )?;
            let rows = stmt.query_map([kind.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (_logical_id, properties_str) = row?;
                let props: serde_json::Value =
                    serde_json::from_str(&properties_str).unwrap_or_default();
                if crate::writer::extract_property_fts(&props, schema)
                    .0
                    .is_some()
                {
                    missing += 1;
                }
            }
        }
    }
    Ok(missing)
}

/// Count property FTS rows whose persisted text has drifted from the current
/// canonical value computed by the FTS extractors. Handles both per-kind
/// table shapes:
///
/// - Non-weighted (legacy / default): single `text_content` column per row.
///   Compared against `extract_property_fts(...)`.
/// - Weighted: one column per registered path (named by `fts_column_name`).
///   Compared against `extract_property_fts_columns(...)`; any per-column
///   mismatch counts the row as drifted exactly once.
///
/// This catches:
/// - rows whose text no longer matches the current node properties and schema
/// - rows that should have been removed (extraction now yields no value)
fn count_drifted_property_fts_rows(conn: &rusqlite::Connection) -> Result<i64, EngineError> {
    let schemas = crate::writer::load_fts_property_schemas(conn)?;
    if schemas.is_empty() {
        return Ok(0);
    }

    let mut drifted = 0i64;
    for (kind, schema) in &schemas {
        let table = fathomdb_schema::fts_kind_table_name(kind);
        // If the per-kind table doesn't exist, no rows to check.
        let table_exists: bool = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name = ?1",
                [table.as_str()],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if !table_exists {
            continue;
        }

        // Dispatch on the persisted schema, not on the live table columns.
        // Registration (`register_fts_property_schema_with_entries` in
        // `admin/fts.rs`) chooses the per-kind table layout by checking
        // whether any entry carries a weight; mirroring that invariant here
        // keeps the two code paths in lockstep. A live-column probe would
        // misclassify a weighted schema whose registered paths happened to
        // include literal `$.text_content` (collapsing to a `text_content`
        // column), silently running the non-weighted comparator against
        // per-column storage.
        drifted += if schema.is_weighted() {
            count_drifted_weighted(conn, kind, &table, schema)?
        } else {
            count_drifted_non_weighted(conn, kind, &table, schema)?
        };
    }
    Ok(drifted)
}

/// Drift count for the non-weighted (single `text_content` column) per-kind
/// FTS table. Preserves the historic query shape.
fn count_drifted_non_weighted(
    conn: &rusqlite::Connection,
    kind: &str,
    table: &str,
    schema: &crate::writer::PropertyFtsSchema,
) -> Result<i64, EngineError> {
    let mut drifted = 0i64;
    let mut stmt = conn.prepare(&format!(
        "SELECT fp.node_logical_id, fp.text_content, n.properties \
         FROM {table} fp \
         JOIN nodes n ON n.logical_id = fp.node_logical_id AND n.superseded_at IS NULL \
         WHERE n.kind = ?1"
    ))?;
    let rows = stmt.query_map([kind], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (_logical_id, stored_text, properties_str) = row?;
        let props: serde_json::Value = serde_json::from_str(&properties_str).unwrap_or_default();
        let (expected, _positions, _stats) = crate::writer::extract_property_fts(&props, schema);
        match expected {
            Some(text) if text == stored_text => {}
            _ => drifted += 1,
        }
    }
    Ok(drifted)
}

/// Drift count for a weighted (per-column) per-kind FTS table. One column per
/// registered path, named via `fts_column_name`. A row counts as drifted if
/// any per-column value differs from the canonical extraction.
fn count_drifted_weighted(
    conn: &rusqlite::Connection,
    kind: &str,
    table: &str,
    schema: &crate::writer::PropertyFtsSchema,
) -> Result<i64, EngineError> {
    // Column names come from `fts_column_name` and are restricted to
    // `[a-zA-Z0-9_]`, so direct interpolation + quoted identifiers are safe.
    let columns: Vec<String> = schema
        .paths
        .iter()
        .map(|entry| {
            let is_recursive = matches!(entry.mode, crate::writer::PropertyPathMode::Recursive);
            fathomdb_schema::fts_column_name(&entry.path, is_recursive)
        })
        .collect();
    if columns.is_empty() {
        // Weighted table with no registered columns is impossible in
        // practice (register always writes specs), but guard defensively:
        // nothing to compare against, so nothing drifts.
        return Ok(0);
    }

    let select_cols: String = columns
        .iter()
        .map(|c| format!("fp.\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT fp.node_logical_id, {select_cols}, n.properties \
         FROM {table} fp \
         JOIN nodes n ON n.logical_id = fp.node_logical_id AND n.superseded_at IS NULL \
         WHERE n.kind = ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    // Column layout in the result set:
    //   [0] node_logical_id
    //   [1..1+columns.len()] per-spec stored text, in `columns` order
    //   [last] properties JSON
    let props_col_idx = columns.len() + 1;
    let rows = stmt.query_map([kind], |row| {
        let mut stored: Vec<String> = Vec::with_capacity(columns.len());
        for i in 0..columns.len() {
            stored.push(row.get::<_, String>(i + 1)?);
        }
        let properties: String = row.get(props_col_idx)?;
        Ok((stored, properties))
    })?;

    let mut drifted = 0i64;
    for row in rows {
        let (stored, properties_str) = row?;
        let props: serde_json::Value = serde_json::from_str(&properties_str).unwrap_or_default();
        let expected = crate::writer::extract_property_fts_columns(&props, schema);
        // `extract_property_fts_columns` returns entries in schema-path order,
        // which matches `columns`. Compare per-column; any mismatch counts
        // the row as drifted exactly once.
        let row_drifted = if expected.len() == stored.len() {
            expected
                .iter()
                .zip(stored.iter())
                .any(|((_name, exp_text), stored_text)| exp_text != stored_text)
        } else {
            true
        };
        if row_drifted {
            drifted += 1;
        }
    }
    Ok(drifted)
}

/// Convert a non-negative i64 count to usize, panicking on negative values
/// which would indicate data corruption.
#[allow(clippy::expect_used)]
pub(super) fn i64_to_usize(val: i64) -> usize {
    usize::try_from(val).expect("count(*) must be non-negative")
}

pub(super) fn persist_simple_provenance_event(
    conn: &rusqlite::Connection,
    event_type: &str,
    subject: &str,
    metadata: Option<serde_json::Value>,
) -> Result<(), EngineError> {
    let metadata_json = metadata.map(|value| value.to_string()).unwrap_or_default();
    conn.execute(
        "INSERT INTO provenance_events (id, event_type, subject, metadata_json) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![new_id(), event_type, subject, metadata_json],
    )?;
    Ok(())
}

pub(super) fn rebuild_operational_current_rows(
    tx: &rusqlite::Transaction<'_>,
    collections: &[String],
) -> Result<usize, EngineError> {
    let mut rebuilt_rows = 0usize;
    clear_operational_current_rows(tx, collections)?;
    let mut ins_current = tx.prepare_cached(
        "INSERT INTO operational_current \
         (collection_name, record_key, payload_json, updated_at, last_mutation_id) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;

    for collection in collections {
        let mut stmt = tx.prepare(
            "SELECT id, collection_name, record_key, op_kind, payload_json, source_ref, created_at \
             FROM operational_mutations \
             WHERE collection_name = ?1 \
             ORDER BY record_key, mutation_order",
        )?;
        let mut latest_by_key: std::collections::HashMap<String, Option<(String, i64, String)>> =
            std::collections::HashMap::new();
        let rows = stmt.query_map([collection], operational::map_operational_mutation_row)?;
        for row in rows {
            let mutation = row?;
            match mutation.op_kind.as_str() {
                "put" => {
                    latest_by_key.insert(
                        mutation.record_key,
                        Some((mutation.payload_json, mutation.created_at, mutation.id)),
                    );
                }
                "delete" => {
                    latest_by_key.insert(mutation.record_key, None);
                }
                _ => {}
            }
        }

        for (record_key, state) in latest_by_key {
            if let Some((payload_json, updated_at, last_mutation_id)) = state {
                ins_current.execute(rusqlite::params![
                    collection,
                    record_key,
                    payload_json,
                    updated_at,
                    last_mutation_id,
                ])?;
                rebuilt_rows += 1;
            }
        }
    }

    drop(ins_current);
    Ok(rebuilt_rows)
}

pub(super) fn clear_operational_current_rows(
    tx: &rusqlite::Transaction<'_>,
    collections: &[String],
) -> Result<(), EngineError> {
    let mut delete_current =
        tx.prepare_cached("DELETE FROM operational_current WHERE collection_name = ?1")?;
    let mut delete_secondary_current = tx.prepare_cached(
        "DELETE FROM operational_secondary_index_entries \
         WHERE collection_name = ?1 AND subject_kind = 'current'",
    )?;
    for collection in collections {
        delete_secondary_current.execute([collection])?;
        delete_current.execute([collection])?;
    }
    drop(delete_secondary_current);
    drop(delete_current);
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use fathomdb_schema::SchemaManager;
    use tempfile::NamedTempFile;

    use super::{
        AdminService, FtsPropertyPathMode, FtsPropertyPathSpec, SafeExportOptions,
        VectorRegenerationConfig,
    };
    use crate::embedder::{BatchEmbedder, EmbedderError, QueryEmbedder, QueryEmbedderIdentity};
    use crate::projection::ProjectionTarget;
    use crate::sqlite;
    use crate::{EngineError, OperationalCollectionKind, OperationalRegisterRequest};

    #[cfg(feature = "sqlite-vec")]
    use crate::{ExecutionCoordinator, TelemetryCounters};

    #[cfg(feature = "sqlite-vec")]
    use fathomdb_query::QueryBuilder;

    #[cfg(feature = "sqlite-vec")]
    use super::load_vector_regeneration_config;

    /// In-process embedder used by the regeneration test suite. The
    /// vector is parameterized so individual tests can distinguish which
    /// embedder produced which profile row.
    #[derive(Debug)]
    #[allow(dead_code)]
    struct TestEmbedder {
        identity: QueryEmbedderIdentity,
        vector: Vec<f32>,
    }

    #[allow(dead_code)]
    impl TestEmbedder {
        fn new(model: &str, dimension: usize) -> Self {
            Self {
                identity: QueryEmbedderIdentity {
                    model_identity: model.to_owned(),
                    model_version: "1.0.0".to_owned(),
                    dimension,
                    normalization_policy: "l2".to_owned(),
                },
                vector: vec![1.0; dimension],
            }
        }
    }

    impl QueryEmbedder for TestEmbedder {
        fn embed_query(&self, _text: &str) -> Result<Vec<f32>, EmbedderError> {
            Ok(self.vector.clone())
        }
        fn identity(&self) -> QueryEmbedderIdentity {
            self.identity.clone()
        }
        fn max_tokens(&self) -> usize {
            512
        }
    }

    impl BatchEmbedder for TestEmbedder {
        fn batch_embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
            Ok(texts.iter().map(|_| self.vector.clone()).collect())
        }
        fn identity(&self) -> QueryEmbedderIdentity {
            self.identity.clone()
        }
        fn max_tokens(&self) -> usize {
            512
        }
    }

    /// Embedder that always fails — used to exercise the post-request
    /// failure audit path without the complexity of subprocess machinery.
    #[derive(Debug)]
    #[allow(dead_code)]
    struct FailingEmbedder {
        identity: QueryEmbedderIdentity,
    }

    impl QueryEmbedder for FailingEmbedder {
        fn embed_query(&self, _text: &str) -> Result<Vec<f32>, EmbedderError> {
            Err(EmbedderError::Failed("test failure".to_owned()))
        }
        fn identity(&self) -> QueryEmbedderIdentity {
            self.identity.clone()
        }
        fn max_tokens(&self) -> usize {
            512
        }
    }

    #[allow(dead_code)]
    #[cfg(unix)]
    fn set_file_mode(path: &std::path::Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path).expect("script metadata").permissions();
        permissions.set_mode(mode);
        fs::set_permissions(path, permissions).expect("chmod");
    }

    #[allow(dead_code)]
    #[cfg(not(unix))]
    fn set_file_mode(_path: &std::path::Path, _mode: u32) {}
    fn setup() -> (NamedTempFile, AdminService) {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());
        {
            let conn = sqlite::open_connection(db.path()).expect("connection");
            schema.bootstrap(&conn).expect("bootstrap");
        }
        let service = AdminService::new(db.path(), Arc::clone(&schema));
        (db, service)
    }

    #[test]
    fn check_integrity_includes_active_uniqueness_count() {
        let (_db, service) = setup();
        let report = service.check_integrity().expect("integrity check");
        assert_eq!(report.duplicate_active_logical_ids, 0);
        assert_eq!(report.operational_missing_collections, 0);
        assert_eq!(report.operational_missing_last_mutations, 0);
    }

    #[test]
    fn trace_source_returns_node_logical_ids() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r1', 'lg1', 'Meeting', '{}', 100, 'source-1')",
                [],
            )
            .expect("insert node");
        }
        let report = service.trace_source("source-1").expect("trace");
        assert_eq!(report.node_rows, 1);
        assert_eq!(report.node_logical_ids, vec!["lg1"]);
    }

    #[test]
    fn trace_source_includes_operational_mutations() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO operational_collections \
                 (name, kind, schema_json, retention_json, format_version, created_at) \
                 VALUES ('connector_health', 'latest_state', '{}', '{}', 1, 100)",
                [],
            )
            .expect("insert collection");
            conn.execute(
                "INSERT INTO operational_mutations \
                 (id, collection_name, record_key, op_kind, payload_json, source_ref, created_at, mutation_order) \
                 VALUES ('m1', 'connector_health', 'gmail', 'put', '{\"status\":\"ok\"}', 'source-1', 100, 1)",
                [],
            )
            .expect("insert mutation");
        }

        let report = service.trace_source("source-1").expect("trace");
        assert_eq!(report.operational_mutation_rows, 1);
        assert_eq!(report.operational_mutation_ids, vec!["m1"]);
    }

    #[test]
    fn excise_source_restores_prior_active_node() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, superseded_at, source_ref) \
                 VALUES ('r1', 'lg1', 'Meeting', '{}', 100, 200, 'source-1')",
                [],
            )
            .expect("insert v1 superseded");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r2', 'lg1', 'Meeting', '{}', 200, 'source-2')",
                [],
            )
            .expect("insert v2 active");
        }
        service.excise_source("source-2").expect("excise");
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            let active_row_id: String = conn
                .query_row(
                    "SELECT row_id FROM nodes WHERE logical_id = 'lg1' AND superseded_at IS NULL",
                    [],
                    |row| row.get(0),
                )
                .expect("active row exists after excise");
            assert_eq!(active_row_id, "r1");
        }
    }

    #[test]
    fn excise_source_deletes_operational_mutations_and_repairs_latest_state_current() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO operational_collections \
                 (name, kind, schema_json, retention_json, format_version, created_at) \
                 VALUES ('connector_health', 'latest_state', '{}', '{}', 1, 100)",
                [],
            )
            .expect("insert collection");
            conn.execute(
                "INSERT INTO operational_mutations \
                 (id, collection_name, record_key, op_kind, payload_json, source_ref, created_at, mutation_order) \
                 VALUES ('m1', 'connector_health', 'gmail', 'put', '{\"status\":\"old\"}', 'source-1', 100, 1)",
                [],
            )
            .expect("insert prior mutation");
            conn.execute(
                "INSERT INTO operational_mutations \
                 (id, collection_name, record_key, op_kind, payload_json, source_ref, created_at, mutation_order) \
                 VALUES ('m2', 'connector_health', 'gmail', 'put', '{\"status\":\"new\"}', 'source-2', 200, 2)",
                [],
            )
            .expect("insert excised mutation");
            conn.execute(
                "INSERT INTO operational_current \
                 (collection_name, record_key, payload_json, updated_at, last_mutation_id) \
                 VALUES ('connector_health', 'gmail', '{\"status\":\"new\"}', 200, 'm2')",
                [],
            )
            .expect("insert current row");
        }

        let traced = service
            .trace_source("source-2")
            .expect("trace before excise");
        assert_eq!(traced.operational_mutation_rows, 1);
        assert_eq!(traced.operational_mutation_ids, vec!["m2"]);

        let excised = service.excise_source("source-2").expect("excise");
        assert_eq!(excised.operational_mutation_rows, 0);
        assert!(excised.operational_mutation_ids.is_empty());

        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            let remaining: i64 = conn
                .query_row(
                    "SELECT count(*) FROM operational_mutations WHERE source_ref = 'source-2'",
                    [],
                    |row| row.get(0),
                )
                .expect("remaining count");
            assert_eq!(remaining, 0);

            let current: (String, String) = conn
                .query_row(
                    "SELECT payload_json, last_mutation_id FROM operational_current \
                     WHERE collection_name = 'connector_health' AND record_key = 'gmail'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("rebuilt current row");
            assert_eq!(current.0, "{\"status\":\"old\"}");
            assert_eq!(current.1, "m1");
        }
    }

    #[test]
    fn restore_logical_id_reestablishes_last_pre_retire_content_and_attached_edges() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('node-row-1', 'doc-1', 'Document', '{\"title\":\"Budget\"}', 100, 'seed')",
                [],
            )
            .expect("insert node");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('node-row-topic', 'topic-1', 'Topic', '{}', 100, 'seed')",
                [],
            )
            .expect("insert target node");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'doc-1', 'budget narrative', 100)",
                [],
            )
            .expect("insert chunk");
            conn.execute(
                "INSERT INTO edges \
                 (row_id, logical_id, source_logical_id, target_logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('edge-row-1', 'edge-1', 'doc-1', 'topic-1', 'TAGGED', '{}', 100, 'seed')",
                [],
            )
            .expect("insert edge");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-node-retire', 'node_retire', 'doc-1', 'forget-1', 200, '')",
                [],
            )
            .expect("insert node retire event");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-edge-retire', 'edge_retire', 'edge-1', 'forget-1', 200, '')",
                [],
            )
            .expect("insert edge retire event");
            conn.execute(
                "UPDATE nodes SET superseded_at = 200 WHERE logical_id = 'doc-1'",
                [],
            )
            .expect("retire node");
            conn.execute(
                "UPDATE edges SET superseded_at = 200 WHERE logical_id = 'edge-1'",
                [],
            )
            .expect("retire edge");
            conn.execute("DELETE FROM fts_nodes", [])
                .expect("clear fts");
        }

        let report = service.restore_logical_id("doc-1").expect("restore");
        assert_eq!(report.logical_id, "doc-1");
        assert!(!report.was_noop);
        assert_eq!(report.restored_node_rows, 1);
        assert_eq!(report.restored_edge_rows, 1);
        assert_eq!(report.restored_chunk_rows, 1);
        assert_eq!(report.restored_fts_rows, 1);

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let active_node_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM nodes WHERE logical_id = 'doc-1' AND superseded_at IS NULL",
                [],
                |row| row.get(0),
            )
            .expect("active node count");
        assert_eq!(active_node_count, 1);
        let active_edge_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM edges WHERE logical_id = 'edge-1' AND superseded_at IS NULL",
                [],
                |row| row.get(0),
            )
            .expect("active edge count");
        assert_eq!(active_edge_count, 1);
        let fts_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM fts_nodes WHERE chunk_id = 'chunk-1'",
                [],
                |row| row.get(0),
            )
            .expect("fts count");
        assert_eq!(fts_count, 1);
    }

    #[test]
    fn restore_logical_id_restores_edges_retired_after_the_node_retire_event() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('node-row-1', 'doc-1', 'Document', '{\"title\":\"Budget\"}', 100, 'seed')",
                [],
            )
            .expect("insert node");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('node-row-topic', 'topic-1', 'Topic', '{}', 100, 'seed')",
                [],
            )
            .expect("insert target node");
            conn.execute(
                "INSERT INTO edges \
                 (row_id, logical_id, source_logical_id, target_logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('edge-row-1', 'edge-1', 'doc-1', 'topic-1', 'TAGGED', '{}', 100, 'seed')",
                [],
            )
            .expect("insert edge");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-node-retire', 'node_retire', 'doc-1', 'forget-1', 200, '')",
                [],
            )
            .expect("insert node retire event");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-edge-retire', 'edge_retire', 'edge-1', 'forget-1', 201, '')",
                [],
            )
            .expect("insert edge retire event");
            conn.execute(
                "UPDATE nodes SET superseded_at = 200 WHERE logical_id = 'doc-1'",
                [],
            )
            .expect("retire node");
            conn.execute(
                "UPDATE edges SET superseded_at = 201 WHERE logical_id = 'edge-1'",
                [],
            )
            .expect("retire edge");
        }

        let report = service.restore_logical_id("doc-1").expect("restore");
        assert_eq!(report.restored_edge_rows, 1);

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let active_edge_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM edges WHERE logical_id = 'edge-1' AND superseded_at IS NULL",
                [],
                |row| row.get(0),
            )
            .expect("active edge count");
        assert_eq!(active_edge_count, 1);
    }

    #[test]
    fn restore_logical_id_prefers_latest_retired_revision_when_timestamps_tie() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO nodes \
                 (row_id, logical_id, kind, properties, created_at, superseded_at, source_ref) \
                 VALUES ('node-row-older', 'doc-1', 'Document', '{\"title\":\"older\"}', 100, 200, 'forget-1')",
                [],
            )
            .expect("insert older retired node");
            conn.execute(
                "INSERT INTO nodes \
                 (row_id, logical_id, kind, properties, created_at, superseded_at, source_ref) \
                 VALUES ('node-row-newer', 'doc-1', 'Document', '{\"title\":\"newer\"}', 100, 200, 'forget-1')",
                [],
            )
            .expect("insert newer retired node");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-retire-older', 'node_retire', 'doc-1', 'forget-1', 200, '')",
                [],
            )
            .expect("insert older retire event");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-retire-newer', 'node_retire', 'doc-1', 'forget-1', 200, '')",
                [],
            )
            .expect("insert newer retire event");
        }

        let report = service.restore_logical_id("doc-1").expect("restore");

        assert!(!report.was_noop);
        let conn = sqlite::open_connection(db.path()).expect("conn");
        let active_row: (String, String) = conn
            .query_row(
                "SELECT row_id, properties FROM nodes \
                 WHERE logical_id = 'doc-1' AND superseded_at IS NULL",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("restored active row");
        assert_eq!(active_row.0, "node-row-newer");
        assert_eq!(active_row.1, "{\"title\":\"newer\"}");
    }

    #[test]
    fn purge_logical_id_removes_retired_content_and_records_tombstone() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, superseded_at, source_ref) \
                 VALUES ('node-row-1', 'doc-1', 'Document', '{\"title\":\"Budget\"}', 100, 200, 'seed')",
                [],
            )
            .expect("insert retired node");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'doc-1', 'budget narrative', 100)",
                [],
            )
            .expect("insert chunk");
            conn.execute(
                "INSERT INTO edges \
                 (row_id, logical_id, source_logical_id, target_logical_id, kind, properties, created_at, superseded_at, source_ref) \
                 VALUES ('edge-row-1', 'edge-1', 'doc-1', 'topic-1', 'TAGGED', '{}', 100, 200, 'seed')",
                [],
            )
            .expect("insert retired edge");
            conn.execute(
                "INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content) \
                 VALUES ('chunk-1', 'doc-1', 'Document', 'budget narrative')",
                [],
            )
            .expect("insert fts");
        }

        let report = service.purge_logical_id("doc-1").expect("purge");
        assert_eq!(report.logical_id, "doc-1");
        assert!(!report.was_noop);
        assert_eq!(report.deleted_node_rows, 1);
        assert_eq!(report.deleted_edge_rows, 1);
        assert_eq!(report.deleted_chunk_rows, 1);
        assert_eq!(report.deleted_fts_rows, 1);

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let remaining_nodes: i64 = conn
            .query_row(
                "SELECT count(*) FROM nodes WHERE logical_id = 'doc-1'",
                [],
                |row| row.get(0),
            )
            .expect("remaining nodes");
        assert_eq!(remaining_nodes, 0);
        let remaining_edges: i64 = conn
            .query_row(
                "SELECT count(*) FROM edges WHERE logical_id = 'edge-1'",
                [],
                |row| row.get(0),
            )
            .expect("remaining edges");
        assert_eq!(remaining_edges, 0);
        let remaining_chunks: i64 = conn
            .query_row(
                "SELECT count(*) FROM chunks WHERE id = 'chunk-1'",
                [],
                |row| row.get(0),
            )
            .expect("remaining chunks");
        assert_eq!(remaining_chunks, 0);
        let purge_events: i64 = conn
            .query_row(
                "SELECT count(*) FROM provenance_events WHERE event_type = 'purge_logical_id' AND subject = 'doc-1'",
                [],
                |row| row.get(0),
            )
            .expect("purge events");
        assert_eq!(purge_events, 1);
    }

    #[test]
    fn check_semantics_accepts_preserved_retired_chunks() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, superseded_at, source_ref) \
                 VALUES ('node-row-1', 'doc-1', 'Document', '{}', 100, 200, 'seed')",
                [],
            )
            .expect("insert retired node");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'doc-1', 'budget narrative', 100)",
                [],
            )
            .expect("insert chunk");
        }

        let report = service.check_semantics().expect("semantics");
        assert_eq!(report.orphaned_chunks, 0);
    }

    #[test]
    fn check_semantics_detects_missing_retired_node_history_for_preserved_chunks() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'ghost-doc', 'budget narrative', 100)",
                [],
            )
            .expect("insert orphaned chunk");
        }

        let report = service.check_semantics().expect("semantics");
        assert_eq!(report.orphaned_chunks, 1);
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn check_semantics_detects_missing_retired_node_history_for_preserved_vec_rows() {
        let (db, service) = setup();
        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            service
                .schema_manager
                .ensure_vec_kind_profile(&conn, "Doc", 4)
                .expect("ensure vec kind profile");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'ghost-doc', 'budget narrative', 100)",
                [],
            )
            .expect("insert orphaned chunk");
            conn.execute(
                "INSERT INTO vec_doc (chunk_id, embedding) VALUES ('chunk-1', zeroblob(16))",
                [],
            )
            .expect("insert vec row");
        }

        let report = service.check_semantics().expect("semantics");
        assert_eq!(report.orphaned_chunks, 1);
        assert_eq!(report.vec_rows_for_superseded_nodes, 1);
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn restore_logical_id_reestablishes_vector_search_without_reingest() {
        let (db, service) = setup();
        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            service
                .schema_manager
                .ensure_vec_kind_profile(&conn, "Document", 4)
                .expect("ensure vec kind profile");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, superseded_at, source_ref) \
                 VALUES ('node-row-1', 'doc-1', 'Document', '{\"title\":\"Budget\"}', 100, 200, 'seed')",
                [],
            )
            .expect("insert retired node");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'doc-1', 'budget narrative', 100)",
                [],
            )
            .expect("insert chunk");
            conn.execute(
                "INSERT INTO vec_document (chunk_id, embedding) VALUES ('chunk-1', zeroblob(16))",
                [],
            )
            .expect("insert vec row");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-node-retire', 'node_retire', 'doc-1', 'forget-1', 200, '')",
                [],
            )
            .expect("insert retire event");
        }

        let report = service.restore_logical_id("doc-1").expect("restore");
        assert_eq!(report.restored_vec_rows, 1);

        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            Some(4),
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");
        let compiled = QueryBuilder::nodes("Document")
            .vector_search("[0.0, 0.0, 0.0, 0.0]", 5)
            .compile()
            .expect("compile");
        let rows = coordinator
            .execute_compiled_read(&compiled)
            .expect("vector read");
        assert!(
            rows.nodes.iter().any(|row| row.logical_id == "doc-1"),
            "restore should make the preserved vec row visible again without re-ingest"
        );
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn purge_logical_id_deletes_vec_rows_for_retired_content() {
        let (db, service) = setup();
        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            service
                .schema_manager
                .ensure_vec_kind_profile(&conn, "Document", 4)
                .expect("ensure vec kind profile");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, superseded_at, source_ref) \
                 VALUES ('node-row-1', 'doc-1', 'Document', '{\"title\":\"Budget\"}', 100, 200, 'seed')",
                [],
            )
            .expect("insert retired node");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'doc-1', 'budget narrative', 100)",
                [],
            )
            .expect("insert chunk");
            conn.execute(
                "INSERT INTO vec_document (chunk_id, embedding) VALUES ('chunk-1', zeroblob(16))",
                [],
            )
            .expect("insert vec row");
        }

        let report = service.purge_logical_id("doc-1").expect("purge");
        assert_eq!(report.deleted_vec_rows, 1);

        let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
        let vec_count: i64 = conn
            .query_row("SELECT count(*) FROM vec_document", [], |row| row.get(0))
            .expect("vec count");
        assert_eq!(vec_count, 0);
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn restore_logical_id_restores_visibility_of_regenerated_vectors() {
        let (db, service) = setup();

        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('node-row-1', 'doc-1', 'Document', '{\"title\":\"Budget\"}', 100, 'seed')",
                [],
            )
            .expect("insert node");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'doc-1', 'budget narrative', 100)",
                [],
            )
            .expect("insert chunk");
        }

        let embedder = TestEmbedder::new("test-model", 4);
        service
            .regenerate_vector_embeddings(
                &embedder,
                &VectorRegenerationConfig {
                    kind: "Document".to_owned(),
                    profile: "default".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                },
            )
            .expect("regenerate");

        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-node-retire', 'node_retire', 'doc-1', 'forget-1', 200, '')",
                [],
            )
            .expect("insert retire event");
            conn.execute(
                "UPDATE nodes SET superseded_at = 200 WHERE logical_id = 'doc-1'",
                [],
            )
            .expect("retire node");
        }

        let report = service.restore_logical_id("doc-1").expect("restore");
        assert_eq!(report.restored_vec_rows, 1);

        let coordinator = ExecutionCoordinator::open(
            db.path(),
            Arc::new(SchemaManager::new()),
            Some(4),
            1,
            Arc::new(TelemetryCounters::default()),
            None,
        )
        .expect("coordinator");
        let compiled = QueryBuilder::nodes("Document")
            .vector_search("[0.0, 0.0, 0.0, 0.0]", 5)
            .compile()
            .expect("compile");
        let rows = coordinator
            .execute_compiled_read(&compiled)
            .expect("vector read");
        assert!(
            rows.nodes.iter().any(|row| row.logical_id == "doc-1"),
            "restored logical_id should become visible through regenerated vectors"
        );
    }

    #[test]
    fn check_semantics_clean_db_returns_zeros() {
        let (_db, service) = setup();
        let report = service.check_semantics().expect("semantics check");
        assert_eq!(report.orphaned_chunks, 0);
        assert_eq!(report.null_source_ref_nodes, 0);
        assert_eq!(report.broken_step_fk, 0);
        assert_eq!(report.broken_action_fk, 0);
        assert_eq!(report.stale_fts_rows, 0);
        assert_eq!(report.fts_rows_for_superseded_nodes, 0);
        assert_eq!(report.dangling_edges, 0);
        assert_eq!(report.orphaned_supersession_chains, 0);
        assert_eq!(report.stale_vec_rows, 0);
        assert_eq!(report.vec_rows_for_superseded_nodes, 0);
        assert_eq!(report.missing_operational_current_rows, 0);
        assert_eq!(report.stale_operational_current_rows, 0);
        assert_eq!(report.disabled_collection_mutations, 0);
        assert_eq!(report.mismatched_kind_property_fts_rows, 0);
        assert_eq!(report.duplicate_property_fts_rows, 0);
        assert_eq!(report.drifted_property_fts_rows, 0);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn register_operational_collection_persists_and_emits_provenance() {
        let (db, service) = setup();
        let record = service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "connector_health".to_owned(),
                kind: OperationalCollectionKind::LatestState,
                schema_json: "{}".to_owned(),
                retention_json: "{}".to_owned(),
                filter_fields_json: "[]".to_owned(),
                validation_json: String::new(),
                secondary_indexes_json: "[]".to_owned(),
                format_version: 1,
            })
            .expect("register collection");

        assert_eq!(record.name, "connector_health");
        assert_eq!(record.kind, OperationalCollectionKind::LatestState);
        assert_eq!(record.schema_json, "{}");
        assert_eq!(record.retention_json, "{}");
        assert_eq!(record.filter_fields_json, "[]");
        assert!(record.created_at > 0);
        assert_eq!(record.disabled_at, None);

        let described = service
            .describe_operational_collection("connector_health")
            .expect("describe collection")
            .expect("collection exists");
        assert_eq!(described, record);

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let provenance_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM provenance_events \
                 WHERE event_type = 'operational_collection_registered' AND subject = 'connector_health'",
                [],
                |row| row.get(0),
            )
            .expect("provenance count");
        assert_eq!(provenance_count, 1);
    }

    #[test]
    fn register_and_update_operational_collection_validation_round_trip() {
        let (db, service) = setup();
        let record = service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "connector_health".to_owned(),
                kind: OperationalCollectionKind::LatestState,
                schema_json: "{}".to_owned(),
                retention_json: "{}".to_owned(),
                filter_fields_json: "[]".to_owned(),
                validation_json: String::new(),
                secondary_indexes_json: "[]".to_owned(),
                format_version: 1,
            })
            .expect("register collection");
        assert_eq!(record.validation_json, "");

        let validation_json = r#"{"format_version":1,"mode":"enforce","additional_properties":false,"fields":[{"name":"status","type":"string","required":true,"enum":["ok","failed"]}]}"#;
        let updated = service
            .update_operational_collection_validation("connector_health", validation_json)
            .expect("update validation");
        assert_eq!(updated.validation_json, validation_json);

        let described = service
            .describe_operational_collection("connector_health")
            .expect("describe collection")
            .expect("collection exists");
        assert_eq!(described.validation_json, validation_json);

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let provenance_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM provenance_events \
                 WHERE event_type = 'operational_collection_validation_updated' \
                   AND subject = 'connector_health'",
                [],
                |row| row.get(0),
            )
            .expect("provenance count");
        assert_eq!(provenance_count, 1);
    }

    #[test]
    fn register_update_and_rebuild_operational_secondary_indexes_round_trip() {
        let (db, service) = setup();
        let record = service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "audit_log".to_owned(),
                kind: OperationalCollectionKind::AppendOnlyLog,
                schema_json: "{}".to_owned(),
                retention_json: r#"{"mode":"keep_all"}"#.to_owned(),
                filter_fields_json: r#"[{"name":"actor","type":"string","modes":["exact","prefix"]},{"name":"ts","type":"timestamp","modes":["range"]}]"#.to_owned(),
                validation_json: String::new(),
                secondary_indexes_json: "[]".to_owned(),
                format_version: 1,
            })
            .expect("register collection");
        assert_eq!(record.secondary_indexes_json, "[]");

        {
            let writer = crate::WriterActor::start(
                db.path(),
                Arc::new(SchemaManager::new()),
                crate::ProvenanceMode::Warn,
                Arc::new(crate::TelemetryCounters::default()),
            )
            .expect("writer");
            writer
                .submit(crate::WriteRequest {
                    label: "secondary-index-seed".to_owned(),
                    nodes: vec![],
                    node_retires: vec![],
                    edges: vec![],
                    edge_retires: vec![],
                    chunks: vec![],
                    runs: vec![],
                    steps: vec![],
                    actions: vec![],
                    optional_backfills: vec![],
                    vec_inserts: vec![],
                    operational_writes: vec![
                        crate::OperationalWrite::Append {
                            collection: "audit_log".to_owned(),
                            record_key: "evt-1".to_owned(),
                            payload_json: r#"{"actor":"alice","ts":100}"#.to_owned(),
                            source_ref: Some("src-1".to_owned()),
                        },
                        crate::OperationalWrite::Append {
                            collection: "audit_log".to_owned(),
                            record_key: "evt-2".to_owned(),
                            payload_json: r#"{"actor":"bob","ts":200}"#.to_owned(),
                            source_ref: Some("src-2".to_owned()),
                        },
                    ],
                })
                .expect("seed writes");
        }

        let secondary_indexes_json = r#"[{"name":"actor_ts","kind":"append_only_field_time","field":"actor","value_type":"string","time_field":"ts"}]"#;
        let updated = service
            .update_operational_collection_secondary_indexes("audit_log", secondary_indexes_json)
            .expect("update secondary indexes");
        assert_eq!(updated.secondary_indexes_json, secondary_indexes_json);

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let entry_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_secondary_index_entries \
                 WHERE collection_name = 'audit_log' AND index_name = 'actor_ts'",
                [],
                |row| row.get(0),
            )
            .expect("secondary index count");
        assert_eq!(entry_count, 2);
        conn.execute(
            "DELETE FROM operational_secondary_index_entries WHERE collection_name = 'audit_log'",
            [],
        )
        .expect("clear index entries");
        drop(conn);

        let rebuild = service
            .rebuild_operational_secondary_indexes("audit_log")
            .expect("rebuild secondary indexes");
        assert_eq!(rebuild.collection_name, "audit_log");
        assert_eq!(rebuild.mutation_entries_rebuilt, 2);
        assert_eq!(rebuild.current_entries_rebuilt, 0);
    }

    #[test]
    fn register_operational_collection_rejects_invalid_validation_contract() {
        let (_db, service) = setup();

        let error = service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "connector_health".to_owned(),
                kind: OperationalCollectionKind::LatestState,
                schema_json: "{}".to_owned(),
                retention_json: "{}".to_owned(),
                filter_fields_json: "[]".to_owned(),
                validation_json: r#"{"format_version":1,"mode":"enforce","fields":[{"name":"status","type":"string","minimum":0}]}"#
                    .to_owned(),
                secondary_indexes_json: "[]".to_owned(),
                format_version: 1,
            })
            .expect_err("invalid validation contract should reject");

        assert!(matches!(error, EngineError::InvalidWrite(_)));
        assert!(error.to_string().contains("minimum/maximum"));
    }

    #[test]
    fn validate_operational_collection_history_reports_invalid_rows_without_mutation() {
        let (db, service) = setup();
        service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "audit_log".to_owned(),
                kind: OperationalCollectionKind::AppendOnlyLog,
                schema_json: "{}".to_owned(),
                retention_json: r#"{"mode":"keep_all"}"#.to_owned(),
                filter_fields_json: "[]".to_owned(),
                validation_json: r#"{"format_version":1,"mode":"disabled","additional_properties":false,"fields":[{"name":"status","type":"string","required":true,"enum":["ok","failed"]}]}"#
                    .to_owned(),
                secondary_indexes_json: "[]".to_owned(),
                format_version: 1,
            })
            .expect("register collection");
        {
            let writer = crate::WriterActor::start(
                db.path(),
                Arc::new(SchemaManager::new()),
                crate::ProvenanceMode::Warn,
                Arc::new(crate::TelemetryCounters::default()),
            )
            .expect("writer");
            writer
                .submit(crate::WriteRequest {
                    label: "history-validation".to_owned(),
                    nodes: vec![],
                    node_retires: vec![],
                    edges: vec![],
                    edge_retires: vec![],
                    chunks: vec![],
                    runs: vec![],
                    steps: vec![],
                    actions: vec![],
                    optional_backfills: vec![],
                    vec_inserts: vec![],
                    operational_writes: vec![
                        crate::OperationalWrite::Append {
                            collection: "audit_log".to_owned(),
                            record_key: "evt-1".to_owned(),
                            payload_json: r#"{"status":"ok"}"#.to_owned(),
                            source_ref: Some("src-1".to_owned()),
                        },
                        crate::OperationalWrite::Append {
                            collection: "audit_log".to_owned(),
                            record_key: "evt-2".to_owned(),
                            payload_json: r#"{"status":"bogus"}"#.to_owned(),
                            source_ref: Some("src-2".to_owned()),
                        },
                    ],
                })
                .expect("write");
        }

        let report = service
            .validate_operational_collection_history("audit_log")
            .expect("validate history");
        assert_eq!(report.collection_name, "audit_log");
        assert_eq!(report.checked_rows, 2);
        assert_eq!(report.invalid_row_count, 1);
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].record_key, "evt-2");
        assert!(report.issues[0].message.contains("must be one of"));

        let trace = service
            .trace_operational_collection("audit_log", None)
            .expect("trace");
        assert_eq!(trace.mutation_count, 2);

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let provenance_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM provenance_events \
                 WHERE event_type = 'operational_collection_history_validated' \
                   AND subject = 'audit_log'",
                [],
                |row| row.get(0),
            )
            .expect("provenance count");
        assert_eq!(provenance_count, 0);
    }

    #[test]
    fn trace_operational_collection_returns_mutations_and_current_rows() {
        let (db, service) = setup();
        service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "connector_health".to_owned(),
                kind: OperationalCollectionKind::LatestState,
                schema_json: "{}".to_owned(),
                retention_json: "{}".to_owned(),
                filter_fields_json: "[]".to_owned(),
                validation_json: String::new(),
                secondary_indexes_json: "[]".to_owned(),
                format_version: 1,
            })
            .expect("register collection");
        {
            let writer = crate::WriterActor::start(
                db.path(),
                Arc::new(SchemaManager::new()),
                crate::ProvenanceMode::Warn,
                Arc::new(crate::TelemetryCounters::default()),
            )
            .expect("writer");
            writer
                .submit(crate::WriteRequest {
                    label: "operational".to_owned(),
                    nodes: vec![],
                    node_retires: vec![],
                    edges: vec![],
                    edge_retires: vec![],
                    chunks: vec![],
                    runs: vec![],
                    steps: vec![],
                    actions: vec![],
                    optional_backfills: vec![],
                    vec_inserts: vec![],
                    operational_writes: vec![crate::OperationalWrite::Put {
                        collection: "connector_health".to_owned(),
                        record_key: "gmail".to_owned(),
                        payload_json: r#"{"status":"ok"}"#.to_owned(),
                        source_ref: Some("src-1".to_owned()),
                    }],
                })
                .expect("write");
        }

        let report = service
            .trace_operational_collection("connector_health", Some("gmail"))
            .expect("trace");
        assert_eq!(report.collection_name, "connector_health");
        assert_eq!(report.record_key.as_deref(), Some("gmail"));
        assert_eq!(report.mutation_count, 1);
        assert_eq!(report.current_count, 1);
        assert_eq!(report.mutations[0].op_kind, "put");
        assert_eq!(report.current_rows[0].payload_json, r#"{"status":"ok"}"#);
    }

    #[test]
    fn trace_operational_collection_rejects_unknown_collection() {
        let (_db, service) = setup();

        let error = service
            .trace_operational_collection("missing_collection", None)
            .expect_err("unknown collection should fail");

        assert!(matches!(error, EngineError::InvalidWrite(_)));
        assert!(error.to_string().contains("is not registered"));
    }

    #[test]
    fn rebuild_operational_current_repairs_missing_latest_state_rows() {
        let (db, service) = setup();
        service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "connector_health".to_owned(),
                kind: OperationalCollectionKind::LatestState,
                schema_json: "{}".to_owned(),
                retention_json: "{}".to_owned(),
                filter_fields_json: "[]".to_owned(),
                validation_json: String::new(),
                secondary_indexes_json: "[]".to_owned(),
                format_version: 1,
            })
            .expect("register collection");
        {
            let writer = crate::WriterActor::start(
                db.path(),
                Arc::new(SchemaManager::new()),
                crate::ProvenanceMode::Warn,
                Arc::new(crate::TelemetryCounters::default()),
            )
            .expect("writer");
            writer
                .submit(crate::WriteRequest {
                    label: "operational".to_owned(),
                    nodes: vec![],
                    node_retires: vec![],
                    edges: vec![],
                    edge_retires: vec![],
                    chunks: vec![],
                    runs: vec![],
                    steps: vec![],
                    actions: vec![],
                    optional_backfills: vec![],
                    vec_inserts: vec![],
                    operational_writes: vec![crate::OperationalWrite::Put {
                        collection: "connector_health".to_owned(),
                        record_key: "gmail".to_owned(),
                        payload_json: r#"{"status":"ok"}"#.to_owned(),
                        source_ref: Some("src-1".to_owned()),
                    }],
                })
                .expect("write");
        }
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "DELETE FROM operational_current WHERE collection_name = 'connector_health' AND record_key = 'gmail'",
                [],
            )
            .expect("delete current row");
        }

        let before = service.check_semantics().expect("semantics before rebuild");
        assert_eq!(before.missing_operational_current_rows, 1);

        let repair = service
            .rebuild_operational_current(Some("connector_health"))
            .expect("rebuild current");
        assert_eq!(repair.collections_rebuilt, 1);
        assert_eq!(repair.current_rows_rebuilt, 1);

        let after = service.check_semantics().expect("semantics after rebuild");
        assert_eq!(after.missing_operational_current_rows, 0);

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let payload: String = conn
            .query_row(
                "SELECT payload_json FROM operational_current \
                 WHERE collection_name = 'connector_health' AND record_key = 'gmail'",
                [],
                |row| row.get(0),
            )
            .expect("restored payload");
        assert_eq!(payload, r#"{"status":"ok"}"#);
    }

    #[test]
    fn rebuild_operational_current_restores_latest_state_secondary_index_entries() {
        let (db, service) = setup();
        service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "connector_health".to_owned(),
                kind: OperationalCollectionKind::LatestState,
                schema_json: "{}".to_owned(),
                retention_json: "{}".to_owned(),
                filter_fields_json: "[]".to_owned(),
                validation_json: String::new(),
                secondary_indexes_json: r#"[{"name":"status_current","kind":"latest_state_field","field":"status","value_type":"string"}]"#.to_owned(),
                format_version: 1,
            })
            .expect("register collection");
        {
            let writer = crate::WriterActor::start(
                db.path(),
                Arc::new(SchemaManager::new()),
                crate::ProvenanceMode::Warn,
                Arc::new(crate::TelemetryCounters::default()),
            )
            .expect("writer");
            writer
                .submit(crate::WriteRequest {
                    label: "operational".to_owned(),
                    nodes: vec![],
                    node_retires: vec![],
                    edges: vec![],
                    edge_retires: vec![],
                    chunks: vec![],
                    runs: vec![],
                    steps: vec![],
                    actions: vec![],
                    optional_backfills: vec![],
                    vec_inserts: vec![],
                    operational_writes: vec![crate::OperationalWrite::Put {
                        collection: "connector_health".to_owned(),
                        record_key: "gmail".to_owned(),
                        payload_json: r#"{"status":"ok"}"#.to_owned(),
                        source_ref: Some("src-1".to_owned()),
                    }],
                })
                .expect("write");
        }
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            let entry_count: i64 = conn
                .query_row(
                    "SELECT count(*) FROM operational_secondary_index_entries \
                     WHERE collection_name = 'connector_health' AND subject_kind = 'current'",
                    [],
                    |row| row.get(0),
                )
                .expect("secondary index count before repair");
            assert_eq!(entry_count, 1);
            conn.execute(
                "DELETE FROM operational_current WHERE collection_name = 'connector_health' AND record_key = 'gmail'",
                [],
            )
            .expect("delete current row");
        }

        service
            .rebuild_operational_current(Some("connector_health"))
            .expect("rebuild current");

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let entry_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_secondary_index_entries \
                 WHERE collection_name = 'connector_health' AND subject_kind = 'current'",
                [],
                |row| row.get(0),
            )
            .expect("secondary index count after repair");
        assert_eq!(entry_count, 1);
    }

    #[test]
    fn operational_current_semantics_and_rebuild_follow_mutation_order() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO operational_collections (name, kind, schema_json, retention_json, format_version, created_at) \
                 VALUES ('connector_health', 'latest_state', '{}', '{}', 1, 100)",
                [],
            )
            .expect("seed collection");
            conn.execute(
                "INSERT INTO operational_mutations \
                 (id, collection_name, record_key, op_kind, payload_json, source_ref, created_at, mutation_order) \
                 VALUES ('m3', 'connector_health', 'gmail', 'put', '{\"status\":\"old\"}', 'src-1', 100, 1)",
                [],
            )
            .expect("seed first put");
            conn.execute(
                "INSERT INTO operational_mutations \
                 (id, collection_name, record_key, op_kind, payload_json, source_ref, created_at, mutation_order) \
                 VALUES ('m2', 'connector_health', 'gmail', 'delete', '', 'src-2', 100, 2)",
                [],
            )
            .expect("seed delete");
            conn.execute(
                "INSERT INTO operational_mutations \
                 (id, collection_name, record_key, op_kind, payload_json, source_ref, created_at, mutation_order) \
                 VALUES ('m1', 'connector_health', 'gmail', 'put', '{\"status\":\"new\"}', 'src-3', 100, 3)",
                [],
            )
            .expect("seed final put");
            conn.execute(
                "INSERT INTO operational_current \
                 (collection_name, record_key, payload_json, updated_at, last_mutation_id) \
                 VALUES ('connector_health', 'gmail', '{\"status\":\"new\"}', 100, 'm1')",
                [],
            )
            .expect("seed current");
        }

        let before = service.check_semantics().expect("semantics before rebuild");
        assert_eq!(before.missing_operational_current_rows, 0);
        assert_eq!(before.stale_operational_current_rows, 0);

        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "DELETE FROM operational_current WHERE collection_name = 'connector_health' AND record_key = 'gmail'",
                [],
            )
            .expect("delete current row");
        }

        let missing = service.check_semantics().expect("semantics after delete");
        assert_eq!(missing.missing_operational_current_rows, 1);
        assert_eq!(missing.stale_operational_current_rows, 0);

        service
            .rebuild_operational_current(Some("connector_health"))
            .expect("rebuild current");

        let after = service.check_semantics().expect("semantics after rebuild");
        assert_eq!(after.missing_operational_current_rows, 0);
        assert_eq!(after.stale_operational_current_rows, 0);

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let payload: String = conn
            .query_row(
                "SELECT payload_json FROM operational_current \
                 WHERE collection_name = 'connector_health' AND record_key = 'gmail'",
                [],
                |row| row.get(0),
            )
            .expect("restored payload");
        assert_eq!(payload, r#"{"status":"new"}"#);
    }

    #[test]
    fn disable_operational_collection_sets_disabled_at_and_emits_provenance() {
        let (db, service) = setup();
        service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "audit_log".to_owned(),
                kind: OperationalCollectionKind::AppendOnlyLog,
                schema_json: "{}".to_owned(),
                retention_json: r#"{"mode":"keep_all"}"#.to_owned(),
                filter_fields_json: "[]".to_owned(),
                validation_json: String::new(),
                secondary_indexes_json: "[]".to_owned(),
                format_version: 1,
            })
            .expect("register collection");

        let record = service
            .disable_operational_collection("audit_log")
            .expect("disable collection");
        assert_eq!(record.name, "audit_log");
        assert!(record.disabled_at.is_some());

        let disabled_at = record.disabled_at.expect("disabled_at");
        let described = service
            .describe_operational_collection("audit_log")
            .expect("describe collection")
            .expect("collection exists");
        assert_eq!(described.disabled_at, Some(disabled_at));

        let writer = crate::WriterActor::start(
            db.path(),
            Arc::new(SchemaManager::new()),
            crate::ProvenanceMode::Warn,
            Arc::new(crate::TelemetryCounters::default()),
        )
        .expect("writer");
        let error = writer
            .submit(crate::WriteRequest {
                label: "disabled-operational".to_owned(),
                nodes: vec![],
                node_retires: vec![],
                edges: vec![],
                edge_retires: vec![],
                chunks: vec![],
                runs: vec![],
                steps: vec![],
                actions: vec![],
                optional_backfills: vec![],
                vec_inserts: vec![],
                operational_writes: vec![crate::OperationalWrite::Append {
                    collection: "audit_log".to_owned(),
                    record_key: "evt-1".to_owned(),
                    payload_json: r#"{"type":"sync"}"#.to_owned(),
                    source_ref: Some("src-1".to_owned()),
                }],
            })
            .expect_err("disabled collection should reject writes");
        assert!(matches!(error, EngineError::InvalidWrite(_)));
        assert!(error.to_string().contains("is disabled"));

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let provenance_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM provenance_events \
                 WHERE event_type = 'operational_collection_disabled' AND subject = 'audit_log'",
                [],
                |row| row.get(0),
            )
            .expect("provenance count");
        assert_eq!(provenance_count, 1);
    }

    #[test]
    fn purge_operational_collection_deletes_append_only_rows_before_cutoff() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO operational_collections (name, kind, schema_json, retention_json, format_version, created_at) \
                 VALUES ('audit_log', 'append_only_log', '{}', '{\"mode\":\"keep_all\"}', 1, 100)",
                [],
            )
            .expect("seed collection");
            conn.execute(
                "INSERT INTO operational_mutations \
                 (id, collection_name, record_key, op_kind, payload_json, source_ref, created_at, mutation_order) \
                 VALUES ('evt-1', 'audit_log', 'evt-1', 'append', '{\"seq\":1}', 'src-1', 100, 1)",
                [],
            )
            .expect("seed event 1");
            conn.execute(
                "INSERT INTO operational_mutations \
                 (id, collection_name, record_key, op_kind, payload_json, source_ref, created_at, mutation_order) \
                 VALUES ('evt-2', 'audit_log', 'evt-2', 'append', '{\"seq\":2}', 'src-2', 200, 2)",
                [],
            )
            .expect("seed event 2");
            conn.execute(
                "INSERT INTO operational_mutations \
                 (id, collection_name, record_key, op_kind, payload_json, source_ref, created_at, mutation_order) \
                 VALUES ('evt-3', 'audit_log', 'evt-3', 'append', '{\"seq\":3}', 'src-3', 300, 3)",
                [],
            )
            .expect("seed event 3");
        }

        let report = service
            .purge_operational_collection("audit_log", 250)
            .expect("purge collection");
        assert_eq!(report.collection_name, "audit_log");
        assert_eq!(report.deleted_mutations, 2);
        assert_eq!(report.before_timestamp, 250);

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let remaining: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT id FROM operational_mutations \
                     WHERE collection_name = 'audit_log' ORDER BY mutation_order",
                )
                .expect("stmt");
            stmt.query_map([], |row| row.get(0))
                .expect("rows")
                .collect::<Result<_, _>>()
                .expect("collect")
        };
        assert_eq!(remaining, vec!["evt-3".to_owned()]);
        let provenance_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM provenance_events \
                 WHERE event_type = 'operational_collection_purged' AND subject = 'audit_log'",
                [],
                |row| row.get(0),
            )
            .expect("provenance count");
        assert_eq!(provenance_count, 1);
    }

    #[test]
    fn compact_operational_collection_dry_run_reports_without_mutation() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO operational_collections (name, kind, schema_json, retention_json, format_version, created_at) \
                 VALUES ('audit_log', 'append_only_log', '{}', '{\"mode\":\"keep_last\",\"max_rows\":2}', 1, 100)",
                [],
            )
            .expect("seed collection");
            for (index, created_at) in [(1_i64, 100_i64), (2, 200), (3, 300)] {
                conn.execute(
                    "INSERT INTO operational_mutations \
                     (id, collection_name, record_key, op_kind, payload_json, source_ref, created_at, mutation_order) \
                     VALUES (?1, 'audit_log', ?1, 'append', ?2, 'src', ?3, ?4)",
                    rusqlite::params![
                        format!("evt-{index}"),
                        format!("{{\"seq\":{index}}}"),
                        created_at,
                        index,
                    ],
                )
                .expect("seed event");
            }
        }

        let report = service
            .compact_operational_collection("audit_log", true)
            .expect("compact collection");
        assert_eq!(report.collection_name, "audit_log");
        assert_eq!(report.deleted_mutations, 1);
        assert!(report.dry_run);
        assert_eq!(report.before_timestamp, None);

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let remaining_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_mutations WHERE collection_name = 'audit_log'",
                [],
                |row| row.get(0),
            )
            .expect("remaining count");
        assert_eq!(remaining_count, 3);
        let provenance_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM provenance_events \
                 WHERE event_type = 'operational_collection_compacted' AND subject = 'audit_log'",
                [],
                |row| row.get(0),
            )
            .expect("provenance count");
        assert_eq!(provenance_count, 0);
    }

    #[test]
    fn compact_operational_collection_keep_last_deletes_oldest_rows() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO operational_collections (name, kind, schema_json, retention_json, format_version, created_at) \
                 VALUES ('audit_log', 'append_only_log', '{}', '{\"mode\":\"keep_last\",\"max_rows\":2}', 1, 100)",
                [],
            )
            .expect("seed collection");
            for (index, created_at) in [(1_i64, 100_i64), (2, 200), (3, 300)] {
                conn.execute(
                    "INSERT INTO operational_mutations \
                     (id, collection_name, record_key, op_kind, payload_json, source_ref, created_at, mutation_order) \
                     VALUES (?1, 'audit_log', ?1, 'append', ?2, 'src', ?3, ?4)",
                    rusqlite::params![
                        format!("evt-{index}"),
                        format!("{{\"seq\":{index}}}"),
                        created_at,
                        index,
                    ],
                )
                .expect("seed event");
            }
        }

        let report = service
            .compact_operational_collection("audit_log", false)
            .expect("compact collection");
        assert_eq!(report.deleted_mutations, 1);
        assert!(!report.dry_run);

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let remaining: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT id FROM operational_mutations \
                     WHERE collection_name = 'audit_log' ORDER BY mutation_order",
                )
                .expect("stmt");
            stmt.query_map([], |row| row.get(0))
                .expect("rows")
                .collect::<Result<_, _>>()
                .expect("collect")
        };
        assert_eq!(remaining, vec!["evt-2".to_owned(), "evt-3".to_owned()]);
        let provenance_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM provenance_events \
                 WHERE event_type = 'operational_collection_compacted' AND subject = 'audit_log'",
                [],
                |row| row.get(0),
            )
            .expect("provenance count");
        assert_eq!(provenance_count, 1);
    }

    #[test]
    fn plan_and_run_operational_retention_keep_last() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO operational_collections (name, kind, schema_json, retention_json, format_version, created_at) \
                 VALUES ('audit_log', 'append_only_log', '{}', '{\"mode\":\"keep_last\",\"max_rows\":2}', 1, 100)",
                [],
            )
            .expect("seed collection");
            for (index, created_at) in [(1_i64, 100_i64), (2, 200), (3, 300)] {
                conn.execute(
                    "INSERT INTO operational_mutations \
                     (id, collection_name, record_key, op_kind, payload_json, source_ref, created_at, mutation_order) \
                     VALUES (?1, 'audit_log', ?1, 'append', ?2, 'src', ?3, ?4)",
                    rusqlite::params![
                        format!("evt-{index}"),
                        format!("{{\"seq\":{index}}}"),
                        created_at,
                        index,
                    ],
                )
                .expect("seed event");
            }
        }

        let plan = service
            .plan_operational_retention(1_000, None, Some(10))
            .expect("plan retention");
        assert_eq!(plan.collections_examined, 1);
        assert_eq!(plan.items[0].collection_name, "audit_log");
        assert_eq!(
            plan.items[0].action_kind,
            crate::operational::OperationalRetentionActionKind::KeepLast
        );
        assert_eq!(plan.items[0].candidate_deletions, 1);
        assert_eq!(plan.items[0].max_rows, Some(2));
        assert_eq!(plan.items[0].last_run_at, None);

        let dry_run = service
            .run_operational_retention(1_000, None, Some(10), true)
            .expect("dry-run retention");
        assert!(dry_run.dry_run);
        assert_eq!(dry_run.collections_acted_on, 1);
        assert_eq!(dry_run.items[0].deleted_mutations, 1);
        assert_eq!(dry_run.items[0].rows_remaining, 2);

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let remaining_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_mutations WHERE collection_name = 'audit_log'",
                [],
                |row| row.get(0),
            )
            .expect("remaining count after dry run");
        assert_eq!(remaining_count, 3);
        let retention_run_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operational_retention_runs WHERE collection_name = 'audit_log'",
                [],
                |row| row.get(0),
            )
            .expect("retention run count");
        assert_eq!(retention_run_count, 0);
        drop(conn);

        let executed = service
            .run_operational_retention(1_000, None, Some(10), false)
            .expect("execute retention");
        assert_eq!(executed.collections_acted_on, 1);
        assert_eq!(executed.items[0].deleted_mutations, 1);
        assert_eq!(executed.items[0].rows_remaining, 2);

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let remaining: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT id FROM operational_mutations \
                     WHERE collection_name = 'audit_log' ORDER BY mutation_order",
                )
                .expect("stmt");
            stmt.query_map([], |row| row.get(0))
                .expect("rows")
                .collect::<Result<_, _>>()
                .expect("collect")
        };
        assert_eq!(remaining, vec!["evt-2".to_owned(), "evt-3".to_owned()]);
        let last_run_at: i64 = conn
            .query_row(
                "SELECT executed_at FROM operational_retention_runs \
                 WHERE collection_name = 'audit_log' ORDER BY executed_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("last run at");
        assert_eq!(last_run_at, 1_000);
    }

    #[test]
    fn dry_run_operational_retention_does_not_mark_noop_collection_as_acted_on() {
        let (db, service) = setup();
        let conn = sqlite::open_connection(db.path()).expect("conn");
        conn.execute(
            "INSERT INTO operational_collections (name, kind, schema_json, retention_json, format_version, created_at) \
             VALUES ('audit_log', 'append_only_log', '{}', '{\"mode\":\"keep_last\",\"max_rows\":2}', 1, 100)",
            [],
        )
        .expect("seed collection");
        for (index, created_at) in [(1_i64, 100_i64), (2, 200)] {
            conn.execute(
                "INSERT INTO operational_mutations \
                 (id, collection_name, record_key, op_kind, payload_json, source_ref, created_at, mutation_order) \
                 VALUES (?1, 'audit_log', ?1, 'append', ?2, 'src', ?3, ?4)",
                rusqlite::params![
                    format!("evt-{index}"),
                    format!("{{\"seq\":{index}}}"),
                    created_at,
                    index,
                ],
            )
            .expect("seed event");
        }
        drop(conn);

        let dry_run = service
            .run_operational_retention(1_000, None, Some(10), true)
            .expect("dry-run retention");
        assert!(dry_run.dry_run);
        assert_eq!(dry_run.collections_acted_on, 0);
        assert_eq!(dry_run.items[0].deleted_mutations, 0);
        assert_eq!(dry_run.items[0].rows_remaining, 2);
    }

    #[test]
    fn compact_operational_collection_rejects_latest_state() {
        let (_db, service) = setup();
        service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "connector_health".to_owned(),
                kind: OperationalCollectionKind::LatestState,
                schema_json: "{}".to_owned(),
                retention_json: r#"{"mode":"keep_all"}"#.to_owned(),
                filter_fields_json: "[]".to_owned(),
                validation_json: String::new(),
                secondary_indexes_json: "[]".to_owned(),
                format_version: 1,
            })
            .expect("register collection");

        let error = service
            .compact_operational_collection("connector_health", false)
            .expect_err("latest_state compaction should be rejected");
        assert!(matches!(error, EngineError::InvalidWrite(_)));
        assert!(error.to_string().contains("append_only_log"));
    }

    #[test]
    fn register_operational_collection_persists_filter_fields_json() {
        let (_db, service) = setup();

        let record = service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "audit_log".to_owned(),
                kind: OperationalCollectionKind::AppendOnlyLog,
                schema_json: "{}".to_owned(),
                retention_json: r#"{"mode":"keep_all"}"#.to_owned(),
                filter_fields_json: r#"[{"name":"actor","type":"string","modes":["exact","prefix"]},{"name":"ts","type":"timestamp","modes":["range"]}]"#.to_owned(),
                validation_json: String::new(),
                secondary_indexes_json: "[]".to_owned(),
                format_version: 1,
            })
            .expect("register collection");

        assert_eq!(
            record.filter_fields_json,
            r#"[{"name":"actor","type":"string","modes":["exact","prefix"]},{"name":"ts","type":"timestamp","modes":["range"]}]"#
        );
    }

    #[test]
    fn read_operational_collection_filters_append_only_rows_by_declared_fields() {
        let (db, service) = setup();
        service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "audit_log".to_owned(),
                kind: OperationalCollectionKind::AppendOnlyLog,
                schema_json: "{}".to_owned(),
                retention_json: r#"{"mode":"keep_all"}"#.to_owned(),
                filter_fields_json: r#"[{"name":"actor","type":"string","modes":["exact","prefix"]},{"name":"seq","type":"integer","modes":["exact","range"]},{"name":"ts","type":"timestamp","modes":["exact","range"]}]"#.to_owned(),
                validation_json: String::new(),
                secondary_indexes_json: "[]".to_owned(),
                format_version: 1,
            })
            .expect("register collection");
        {
            let writer = crate::WriterActor::start(
                db.path(),
                Arc::new(SchemaManager::new()),
                crate::ProvenanceMode::Warn,
                Arc::new(crate::TelemetryCounters::default()),
            )
            .expect("writer");
            writer
                .submit(crate::WriteRequest {
                    label: "operational".to_owned(),
                    nodes: vec![],
                    node_retires: vec![],
                    edges: vec![],
                    edge_retires: vec![],
                    chunks: vec![],
                    runs: vec![],
                    steps: vec![],
                    actions: vec![],
                    optional_backfills: vec![],
                    vec_inserts: vec![],
                    operational_writes: vec![
                        crate::OperationalWrite::Append {
                            collection: "audit_log".to_owned(),
                            record_key: "evt-1".to_owned(),
                            payload_json: r#"{"actor":"alice","seq":1,"ts":100}"#.to_owned(),
                            source_ref: Some("src-1".to_owned()),
                        },
                        crate::OperationalWrite::Append {
                            collection: "audit_log".to_owned(),
                            record_key: "evt-2".to_owned(),
                            payload_json: r#"{"actor":"alice-admin","seq":2,"ts":200}"#.to_owned(),
                            source_ref: Some("src-2".to_owned()),
                        },
                        crate::OperationalWrite::Append {
                            collection: "audit_log".to_owned(),
                            record_key: "evt-3".to_owned(),
                            payload_json: r#"{"actor":"bob","seq":3,"ts":300}"#.to_owned(),
                            source_ref: Some("src-3".to_owned()),
                        },
                    ],
                })
                .expect("write");
        }

        let report = service
            .read_operational_collection(&crate::operational::OperationalReadRequest {
                collection_name: "audit_log".to_owned(),
                filters: vec![
                    crate::operational::OperationalFilterClause::Prefix {
                        field: "actor".to_owned(),
                        value: "alice".to_owned(),
                    },
                    crate::operational::OperationalFilterClause::Range {
                        field: "ts".to_owned(),
                        lower: Some(150),
                        upper: Some(250),
                    },
                ],
                limit: Some(10),
            })
            .expect("filtered read");

        assert_eq!(report.collection_name, "audit_log");
        assert_eq!(report.row_count, 1);
        assert!(!report.was_limited);
        assert_eq!(report.rows.len(), 1);
        assert_eq!(report.rows[0].record_key, "evt-2");
        assert_eq!(
            report.rows[0].payload_json,
            r#"{"actor":"alice-admin","seq":2,"ts":200}"#
        );
    }

    #[test]
    fn read_operational_collection_uses_secondary_index_when_filter_values_are_missing() {
        let (db, service) = setup();
        service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "audit_log".to_owned(),
                kind: OperationalCollectionKind::AppendOnlyLog,
                schema_json: "{}".to_owned(),
                retention_json: r#"{"mode":"keep_all"}"#.to_owned(),
                filter_fields_json: r#"[{"name":"actor","type":"string","modes":["exact","prefix"]},{"name":"ts","type":"timestamp","modes":["range"]}]"#.to_owned(),
                validation_json: String::new(),
                secondary_indexes_json: r#"[{"name":"actor_ts","kind":"append_only_field_time","field":"actor","value_type":"string","time_field":"ts"}]"#.to_owned(),
                format_version: 1,
            })
            .expect("register collection");
        {
            let writer = crate::WriterActor::start(
                db.path(),
                Arc::new(SchemaManager::new()),
                crate::ProvenanceMode::Warn,
                Arc::new(crate::TelemetryCounters::default()),
            )
            .expect("writer");
            writer
                .submit(crate::WriteRequest {
                    label: "operational".to_owned(),
                    nodes: vec![],
                    node_retires: vec![],
                    edges: vec![],
                    edge_retires: vec![],
                    chunks: vec![],
                    runs: vec![],
                    steps: vec![],
                    actions: vec![],
                    optional_backfills: vec![],
                    vec_inserts: vec![],
                    operational_writes: vec![
                        crate::OperationalWrite::Append {
                            collection: "audit_log".to_owned(),
                            record_key: "evt-1".to_owned(),
                            payload_json: r#"{"actor":"alice","ts":100}"#.to_owned(),
                            source_ref: Some("src-1".to_owned()),
                        },
                        crate::OperationalWrite::Append {
                            collection: "audit_log".to_owned(),
                            record_key: "evt-2".to_owned(),
                            payload_json: r#"{"actor":"alice-admin","ts":200}"#.to_owned(),
                            source_ref: Some("src-2".to_owned()),
                        },
                    ],
                })
                .expect("write");
        }
        let conn = sqlite::open_connection(db.path()).expect("conn");
        conn.execute(
            "DELETE FROM operational_filter_values WHERE collection_name = 'audit_log'",
            [],
        )
        .expect("clear filter values");
        drop(conn);

        let report = service
            .read_operational_collection(&crate::operational::OperationalReadRequest {
                collection_name: "audit_log".to_owned(),
                filters: vec![
                    crate::operational::OperationalFilterClause::Prefix {
                        field: "actor".to_owned(),
                        value: "alice".to_owned(),
                    },
                    crate::operational::OperationalFilterClause::Range {
                        field: "ts".to_owned(),
                        lower: Some(150),
                        upper: Some(250),
                    },
                ],
                limit: Some(10),
            })
            .expect("secondary-index read");

        assert_eq!(report.row_count, 1);
        assert_eq!(report.rows[0].record_key, "evt-2");
    }

    #[test]
    fn read_operational_collection_rejects_undeclared_fields_and_latest_state_collections() {
        let (_db, service) = setup();
        service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "connector_health".to_owned(),
                kind: OperationalCollectionKind::LatestState,
                schema_json: "{}".to_owned(),
                retention_json: "{}".to_owned(),
                filter_fields_json: r#"[{"name":"status","type":"string","modes":["exact"]}]"#
                    .to_owned(),
                validation_json: String::new(),
                secondary_indexes_json: "[]".to_owned(),
                format_version: 1,
            })
            .expect("register collection");

        let latest_state_error = service
            .read_operational_collection(&crate::operational::OperationalReadRequest {
                collection_name: "connector_health".to_owned(),
                filters: vec![crate::operational::OperationalFilterClause::Exact {
                    field: "status".to_owned(),
                    value: crate::operational::OperationalFilterValue::String("ok".to_owned()),
                }],
                limit: Some(10),
            })
            .expect_err("latest_state filtered reads should be rejected");
        assert!(latest_state_error.to_string().contains("append_only_log"));

        service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "audit_log".to_owned(),
                kind: OperationalCollectionKind::AppendOnlyLog,
                schema_json: "{}".to_owned(),
                retention_json: r#"{"mode":"keep_all"}"#.to_owned(),
                filter_fields_json: r#"[{"name":"actor","type":"string","modes":["exact"]}]"#
                    .to_owned(),
                validation_json: String::new(),
                secondary_indexes_json: "[]".to_owned(),
                format_version: 1,
            })
            .expect("register append-only collection");

        let undeclared_error = service
            .read_operational_collection(&crate::operational::OperationalReadRequest {
                collection_name: "audit_log".to_owned(),
                filters: vec![crate::operational::OperationalFilterClause::Exact {
                    field: "missing".to_owned(),
                    value: crate::operational::OperationalFilterValue::String("x".to_owned()),
                }],
                limit: Some(10),
            })
            .expect_err("undeclared field should be rejected");
        assert!(undeclared_error.to_string().contains("undeclared"));
    }

    #[test]
    fn read_operational_collection_applies_limit_and_reports_truncation() {
        let (db, service) = setup();
        service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "audit_log".to_owned(),
                kind: OperationalCollectionKind::AppendOnlyLog,
                schema_json: "{}".to_owned(),
                retention_json: r#"{"mode":"keep_all"}"#.to_owned(),
                filter_fields_json: r#"[{"name":"actor","type":"string","modes":["prefix"]}]"#
                    .to_owned(),
                validation_json: String::new(),
                secondary_indexes_json: "[]".to_owned(),
                format_version: 1,
            })
            .expect("register collection");
        {
            let writer = crate::WriterActor::start(
                db.path(),
                Arc::new(SchemaManager::new()),
                crate::ProvenanceMode::Warn,
                Arc::new(crate::TelemetryCounters::default()),
            )
            .expect("writer");
            writer
                .submit(crate::WriteRequest {
                    label: "operational".to_owned(),
                    nodes: vec![],
                    node_retires: vec![],
                    edges: vec![],
                    edge_retires: vec![],
                    chunks: vec![],
                    runs: vec![],
                    steps: vec![],
                    actions: vec![],
                    optional_backfills: vec![],
                    vec_inserts: vec![],
                    operational_writes: vec![
                        crate::OperationalWrite::Append {
                            collection: "audit_log".to_owned(),
                            record_key: "evt-1".to_owned(),
                            payload_json: r#"{"actor":"alice-1"}"#.to_owned(),
                            source_ref: Some("src-1".to_owned()),
                        },
                        crate::OperationalWrite::Append {
                            collection: "audit_log".to_owned(),
                            record_key: "evt-2".to_owned(),
                            payload_json: r#"{"actor":"alice-2"}"#.to_owned(),
                            source_ref: Some("src-2".to_owned()),
                        },
                    ],
                })
                .expect("write");
        }

        let report = service
            .read_operational_collection(&crate::operational::OperationalReadRequest {
                collection_name: "audit_log".to_owned(),
                filters: vec![crate::operational::OperationalFilterClause::Prefix {
                    field: "actor".to_owned(),
                    value: "alice".to_owned(),
                }],
                limit: Some(1),
            })
            .expect("limited read");

        assert_eq!(report.row_count, 1);
        assert_eq!(report.applied_limit, 1);
        assert!(report.was_limited);
        assert_eq!(report.rows[0].record_key, "evt-2");
    }

    #[test]
    fn preexisting_operational_collection_can_gain_filter_contract_after_upgrade() {
        let db = NamedTempFile::new().expect("temp db");
        let conn = sqlite::open_connection(db.path()).expect("conn");
        conn.execute_batch(
            r#"
            CREATE TABLE operational_collections (
                name TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                schema_json TEXT NOT NULL,
                retention_json TEXT NOT NULL,
                format_version INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL DEFAULT 100,
                disabled_at INTEGER
            );
            CREATE TABLE operational_mutations (
                id TEXT PRIMARY KEY,
                collection_name TEXT NOT NULL,
                record_key TEXT NOT NULL,
                op_kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                source_ref TEXT,
                created_at INTEGER NOT NULL DEFAULT 100,
                mutation_order INTEGER NOT NULL DEFAULT 1
            );
            INSERT INTO operational_collections (name, kind, schema_json, retention_json, format_version, created_at)
            VALUES ('audit_log', 'append_only_log', '{}', '{"mode":"keep_all"}', 1, 100);
            INSERT INTO operational_mutations
                (id, collection_name, record_key, op_kind, payload_json, source_ref, created_at, mutation_order)
            VALUES
                ('evt-1', 'audit_log', 'evt-1', 'append', '{"actor":"alice","ts":0}', 'src-1', 100, 1);
            "#,
        )
        .expect("seed pre-v10 schema");
        drop(conn);

        let service = AdminService::new(db.path(), Arc::new(SchemaManager::new()));
        let pre_update = service
            .read_operational_collection(&crate::operational::OperationalReadRequest {
                collection_name: "audit_log".to_owned(),
                filters: vec![crate::operational::OperationalFilterClause::Exact {
                    field: "actor".to_owned(),
                    value: crate::operational::OperationalFilterValue::String("alice".to_owned()),
                }],
                limit: Some(10),
            })
            .expect_err("read should reject undeclared fields before migration update");
        assert!(pre_update.to_string().contains("undeclared"));

        let updated = service
            .update_operational_collection_filters(
                "audit_log",
                r#"[{"name":"actor","type":"string","modes":["exact"]},{"name":"ts","type":"timestamp","modes":["range"]}]"#,
            )
            .expect("update filter contract");
        assert!(updated.filter_fields_json.contains("\"actor\""));

        let report = service
            .read_operational_collection(&crate::operational::OperationalReadRequest {
                collection_name: "audit_log".to_owned(),
                filters: vec![crate::operational::OperationalFilterClause::Range {
                    field: "ts".to_owned(),
                    lower: Some(0),
                    upper: Some(0),
                }],
                limit: Some(10),
            })
            .expect("read after explicit filter update");
        assert_eq!(report.row_count, 1);
        assert_eq!(report.rows[0].record_key, "evt-1");
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn check_semantics_detects_stale_vec_rows() {
        use crate::sqlite::open_connection_with_vec;

        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());
        {
            let conn = open_connection_with_vec(db.path()).expect("vec conn");
            schema.bootstrap(&conn).expect("bootstrap");
            schema
                .ensure_vec_kind_profile(&conn, "Doc", 3)
                .expect("vec kind profile");
            // Insert a vec row whose chunk does not exist.
            let bytes: Vec<u8> = [0.1f32, 0.2f32, 0.3f32]
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();
            conn.execute(
                "INSERT INTO vec_doc (chunk_id, embedding) VALUES ('ghost-chunk', ?1)",
                rusqlite::params![bytes],
            )
            .expect("insert stale vec row");
        }
        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let report = service.check_semantics().expect("semantics check");
        assert_eq!(report.stale_vec_rows, 1);
        assert!(
            report.warnings.iter().any(|w| w.contains("stale vec")),
            "warning must mention stale vec"
        );
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn restore_vector_profiles_recreates_vec_table_from_metadata() {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());
        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            schema.bootstrap(&conn).expect("bootstrap");
            conn.execute(
                "INSERT INTO vector_profiles (profile, table_name, dimension, enabled) \
                 VALUES ('default', 'vec_nodes_active', 3, 1)",
                [],
            )
            .expect("insert vector profile");
        }

        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let report = service
            .restore_vector_profiles()
            .expect("restore vector profiles");
        assert_eq!(
            report.targets,
            vec![crate::projection::ProjectionTarget::Vec]
        );
        assert_eq!(report.rebuilt_rows, 1);

        let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name = 'vec_nodes_active'",
                [],
                |row| row.get(0),
            )
            .expect("vec schema count");
        assert_eq!(count, 1, "vec table should exist after restore");
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn load_vector_regeneration_config_supports_json_and_toml() {
        let dir = tempfile::tempdir().expect("temp dir");
        let json_path = dir.path().join("regen.json");
        let toml_path = dir.path().join("regen.toml");

        let config = VectorRegenerationConfig {
            kind: "Document".to_owned(),
            profile: "default".to_owned(),
            chunking_policy: "per_chunk".to_owned(),
            preprocessing_policy: "trim".to_owned(),
        };

        fs::write(&json_path, serde_json::to_string(&config).expect("json")).expect("write json");
        fs::write(&toml_path, toml::to_string(&config).expect("toml")).expect("write toml");

        let parsed_json = load_vector_regeneration_config(&json_path).expect("json parse");
        let parsed_toml = load_vector_regeneration_config(&toml_path).expect("toml parse");

        assert_eq!(parsed_json, config);
        assert_eq!(parsed_toml, config);
    }

    /// The 0.4.0 rewrite removed the identity fields from the config.
    /// Any client that still serializes the pre-0.4 fields must be
    /// rejected AT THE SERDE BOUNDARY with a clear error — never
    /// silently accepted.
    #[test]
    fn regenerate_vector_embeddings_config_rejects_old_identity_fields() {
        // Pre-0.5.0 configs that include old fields (table_name, model_identity, etc.)
        // must be rejected at the serde boundary due to deny_unknown_fields.
        let legacy_json = r#"{
            "kind": "Document",
            "profile": "default",
            "table_name": "vec_nodes_active",
            "model_identity": "old-model",
            "model_version": "1.0",
            "dimension": 4,
            "normalization_policy": "l2",
            "chunking_policy": "per_chunk",
            "preprocessing_policy": "trim",
            "generator_command": ["/bin/echo"]
        }"#;
        let result: Result<VectorRegenerationConfig, _> = serde_json::from_str(legacy_json);
        assert!(
            result.is_err(),
            "legacy identity fields must be rejected at deserialization"
        );
    }

    #[cfg(all(not(feature = "sqlite-vec"), unix))]
    #[test]
    fn regenerate_vector_embeddings_unsupported_vec_capability_writes_request_and_failed_audit() {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());

        {
            let conn = sqlite::open_connection(db.path()).expect("connection");
            schema.bootstrap(&conn).expect("bootstrap");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'doc-1', 'Document', '{}', 100, 'source-1')",
                [],
            )
            .expect("insert node");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'doc-1', 'budget discussion', 100)",
                [],
            )
            .expect("insert chunk");
        }

        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let embedder = TestEmbedder::new("test-model", 4);
        let error = service
            .regenerate_vector_embeddings(
                &embedder,
                &VectorRegenerationConfig {
                    kind: "Document".to_owned(),
                    profile: "default".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                },
            )
            .expect_err("sqlite-vec capability should be required");

        assert!(error.to_string().contains("unsupported vec capability"));

        let conn = sqlite::open_connection(db.path()).expect("connection");
        let request_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM provenance_events WHERE event_type = 'vector_regeneration_requested' AND subject = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("request count");
        assert_eq!(request_count, 1);
        let failed_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM provenance_events WHERE event_type = 'vector_regeneration_failed' AND subject = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("failed count");
        assert_eq!(failed_count, 1);
        let metadata_json: String = conn
            .query_row(
                "SELECT metadata_json FROM provenance_events WHERE event_type = 'vector_regeneration_failed' AND subject = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("failed metadata");
        assert!(metadata_json.contains("\"failure_class\":\"unsupported vec capability\""));
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    #[allow(clippy::too_many_lines)]
    fn regenerate_vector_embeddings_rebuilds_embeddings_via_embedder() {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());

        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            schema.bootstrap(&conn).expect("bootstrap");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'doc-1', 'Document', '{}', 100, 'source-1')",
                [],
            )
            .expect("insert node");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'doc-1', 'budget discussion', 100)",
                [],
            )
            .expect("insert chunk 1");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-2', 'doc-1', 'travel plan', 101)",
                [],
            )
            .expect("insert chunk 2");
        }

        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let embedder = TestEmbedder::new("test-model", 4);
        let report = service
            .regenerate_vector_embeddings(
                &embedder,
                &VectorRegenerationConfig {
                    kind: "Document".to_owned(),
                    profile: "default".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                },
            )
            .expect("regenerate vectors");

        assert_eq!(report.profile, "default");
        assert_eq!(report.table_name, "vec_document");
        assert_eq!(report.dimension, 4);
        assert_eq!(report.total_chunks, 2);
        assert_eq!(report.regenerated_rows, 2);
        assert!(report.contract_persisted);

        let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
        let vec_count: i64 = conn
            .query_row("SELECT count(*) FROM vec_document", [], |row| row.get(0))
            .expect("vec count");
        assert_eq!(vec_count, 2);

        // The persisted vector contract must reflect the embedder
        // identity — not any string the caller passed in, because the
        // caller never passes one.
        let (model_identity, model_version, dimension, normalization_policy): (
            String,
            String,
            i64,
            String,
        ) = conn
            .query_row(
                "SELECT model_identity, model_version, dimension, normalization_policy \
                 FROM vector_embedding_contracts WHERE profile = 'default'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("contract row");
        assert_eq!(model_identity, "test-model");
        assert_eq!(model_version, "1.0.0");
        assert_eq!(dimension, 4);
        assert_eq!(normalization_policy, "l2");

        let contract_format_version: i64 = conn
            .query_row(
                "SELECT contract_format_version FROM vector_embedding_contracts WHERE profile = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("contract_format_version");
        assert_eq!(contract_format_version, 1);
        let request_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM provenance_events WHERE event_type = 'vector_regeneration_requested' AND subject = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("request audit count");
        assert_eq!(request_count, 1);
        let apply_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM provenance_events WHERE event_type = 'vector_regeneration_apply' AND subject = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("apply audit count");
        assert_eq!(apply_count, 1);
        let apply_metadata: String = conn
            .query_row(
                "SELECT metadata_json FROM provenance_events WHERE event_type = 'vector_regeneration_apply' AND subject = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("apply metadata");
        assert!(apply_metadata.contains("\"profile\":\"default\""));
        assert!(apply_metadata.contains("\"snapshot_hash\":"));
        assert!(apply_metadata.contains("\"model_identity\":\"test-model\""));
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    #[allow(clippy::too_many_lines)]
    fn regenerate_vector_embeddings_embedder_failure_leaves_contract_and_vec_rows_unchanged() {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());

        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            schema.bootstrap(&conn).expect("bootstrap");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'doc-1', 'Document', '{}', 100, 'source-1')",
                [],
            )
            .expect("insert node");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'doc-1', 'budget discussion', 100)",
                [],
            )
            .expect("insert chunk");
            schema
                .ensure_vec_kind_profile(&conn, "Document", 4)
                .expect("ensure vec kind profile");
            conn.execute(
                r"
                INSERT INTO vector_embedding_contracts (
                    profile,
                    table_name,
                    model_identity,
                    model_version,
                    dimension,
                    normalization_policy,
                    chunking_policy,
                    preprocessing_policy,
                    generator_command_json,
                    applied_at,
                    snapshot_hash
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ",
                rusqlite::params![
                    "default",
                    "vec_document",
                    "old-model",
                    "0.9.0",
                    4,
                    "l2",
                    "per_chunk",
                    "trim",
                    "[]",
                    111,
                    "old-snapshot"
                ],
            )
            .expect("seed contract");
            conn.execute(
                "INSERT INTO vec_document (chunk_id, embedding) VALUES ('chunk-1', zeroblob(16))",
                [],
            )
            .expect("seed vec row");
        }

        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let failing = FailingEmbedder {
            identity: QueryEmbedderIdentity {
                model_identity: "new-model".to_owned(),
                model_version: "1.0.0".to_owned(),
                dimension: 4,
                normalization_policy: "l2".to_owned(),
            },
        };
        let error = service
            .regenerate_vector_embeddings(
                &failing,
                &VectorRegenerationConfig {
                    kind: "Document".to_owned(),
                    profile: "default".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                },
            )
            .expect_err("embedder should fail");

        assert!(error.to_string().contains("embedder failure"));

        let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
        let model_identity: String = conn
            .query_row(
                "SELECT model_identity FROM vector_embedding_contracts WHERE profile = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("model identity");
        assert_eq!(model_identity, "old-model");
        let snapshot_hash: String = conn
            .query_row(
                "SELECT snapshot_hash FROM vector_embedding_contracts WHERE profile = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("snapshot hash");
        assert_eq!(snapshot_hash, "old-snapshot");
        let vec_count: i64 = conn
            .query_row("SELECT count(*) FROM vec_document", [], |row| row.get(0))
            .expect("vec count");
        assert_eq!(vec_count, 1);
        let failure_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM provenance_events WHERE event_type = 'vector_regeneration_failed' AND subject = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("failure count");
        assert_eq!(failure_count, 1);
        let failure_metadata: String = conn
            .query_row(
                "SELECT metadata_json FROM provenance_events WHERE event_type = 'vector_regeneration_failed' AND subject = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("failure metadata");
        assert!(failure_metadata.contains("\"failure_class\":\"embedder failure\""));
    }

    // Subprocess generator tests (snapshot-drift-via-concurrent-writer,
    // timeout, stdout/stderr overflow, oversized input, excessive chunk
    // count, malformed JSON, world-writable executable, disallowed
    // executable root, environment preservation) were removed in 0.4.0
    // along with the subprocess generator pattern itself. The failure
    // modes they exercised belong to the deleted
    // `run_vector_generator_bounded` pipeline and have no equivalent in
    // the direct-embedder path. See
    // `.claude/memory/project_vector_identity_invariant.md`.

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn regenerate_vector_embeddings_rejects_whitespace_only_profile_before_mutation() {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());
        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            schema.bootstrap(&conn).expect("bootstrap");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'doc-1', 'Document', '{}', 100, 'source-1')",
                [],
            )
            .expect("insert node");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'doc-1', 'budget discussion', 100)",
                [],
            )
            .expect("insert chunk");
        }

        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let embedder = TestEmbedder::new("test-model", 4);
        let error = service
            .regenerate_vector_embeddings(
                &embedder,
                &VectorRegenerationConfig {
                    kind: "Document".to_owned(),
                    profile: "   ".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                },
            )
            .expect_err("whitespace profile should be rejected");

        assert!(error.to_string().contains("invalid contract"));
        let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
        let contract_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM vector_embedding_contracts",
                [],
                |row| row.get(0),
            )
            .expect("contract count");
        assert_eq!(contract_count, 0);
        let provenance_count: i64 = conn
            .query_row("SELECT count(*) FROM provenance_events", [], |row| {
                row.get(0)
            })
            .expect("provenance count");
        assert_eq!(provenance_count, 0);
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn regenerate_vector_embeddings_rejects_future_contract_format_version() {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());
        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            schema.bootstrap(&conn).expect("bootstrap");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'doc-1', 'Document', '{}', 100, 'source-1')",
                [],
            )
            .expect("insert node");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'doc-1', 'budget discussion', 100)",
                [],
            )
            .expect("insert chunk");
            conn.execute(
                r"
                INSERT INTO vector_embedding_contracts (
                    profile,
                    table_name,
                    model_identity,
                    model_version,
                    dimension,
                    normalization_policy,
                    chunking_policy,
                    preprocessing_policy,
                    generator_command_json,
                    applied_at,
                    snapshot_hash,
                    contract_format_version,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                ",
                rusqlite::params![
                    "default",
                    "vec_nodes_active",
                    "old-model",
                    "0.9.0",
                    4,
                    "l2",
                    "per_chunk",
                    "trim",
                    "[]",
                    111,
                    "old-snapshot",
                    99,
                    111,
                ],
            )
            .expect("seed future contract");
        }

        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let embedder = TestEmbedder::new("test-model", 4);
        let error = service
            .regenerate_vector_embeddings(
                &embedder,
                &VectorRegenerationConfig {
                    kind: "Document".to_owned(),
                    profile: "default".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                },
            )
            .expect_err("future contract version should be rejected");

        assert!(error.to_string().contains("unsupported"));
        assert!(error.to_string().contains("format version"));
    }

    #[test]
    fn check_semantics_detects_orphaned_chunk() {
        let (db, service) = setup();
        {
            // Open without FK enforcement to insert chunk with no active node.
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('c1', 'ghost-node', 'text', 100)",
                [],
            )
            .expect("insert orphaned chunk");
        }
        let report = service.check_semantics().expect("semantics check");
        assert_eq!(report.orphaned_chunks, 1);
    }

    #[test]
    fn check_semantics_detects_null_source_ref() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at) \
                 VALUES ('r1', 'lg1', 'Meeting', '{}', 100)",
                [],
            )
            .expect("insert node with null source_ref");
        }
        let report = service.check_semantics().expect("semantics check");
        assert_eq!(report.null_source_ref_nodes, 1);
    }

    #[test]
    fn check_semantics_detects_broken_step_fk() {
        let (db, service) = setup();
        {
            // Explicitly disable FK enforcement for this connection so we can insert
            // an orphaned step (ghost run_id) to simulate a partial-write failure.
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute_batch("PRAGMA foreign_keys = OFF;")
                .expect("disable FK");
            conn.execute(
                "INSERT INTO steps (id, run_id, kind, status, properties, created_at) \
                 VALUES ('s1', 'ghost-run', 'llm', 'completed', '{}', 100)",
                [],
            )
            .expect("insert step with ghost run_id");
        }
        let report = service.check_semantics().expect("semantics check");
        assert_eq!(report.broken_step_fk, 1);
    }

    #[test]
    fn check_semantics_detects_broken_action_fk() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute_batch("PRAGMA foreign_keys = OFF;")
                .expect("disable FK");
            conn.execute(
                "INSERT INTO actions (id, step_id, kind, status, properties, created_at) \
                 VALUES ('a1', 'ghost-step', 'emit', 'completed', '{}', 100)",
                [],
            )
            .expect("insert action with ghost step_id");
        }
        let report = service.check_semantics().expect("semantics check");
        assert_eq!(report.broken_action_fk, 1);
    }

    #[test]
    fn check_semantics_detects_stale_fts_rows() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            // FTS virtual tables have no FK constraints; insert a row referencing
            // a chunk_id that does not exist in the chunks table.
            conn.execute(
                "INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content) \
                 VALUES ('ghost-chunk', 'any-node', 'Meeting', 'stale content')",
                [],
            )
            .expect("insert stale FTS row");
        }
        let report = service.check_semantics().expect("semantics check");
        assert_eq!(report.stale_fts_rows, 1);
    }

    #[test]
    fn check_semantics_detects_fts_rows_for_superseded_nodes() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            // Insert a node that has been fully superseded (superseded_at IS NOT NULL).
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, superseded_at, source_ref) \
                 VALUES ('r1', 'lg-sup', 'Meeting', '{}', 100, 200, 'src-1')",
                [],
            )
            .expect("insert superseded node");
            // Insert an FTS row for the superseded node's logical_id.
            conn.execute(
                "INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content) \
                 VALUES ('ck-x', 'lg-sup', 'Meeting', 'superseded content')",
                [],
            )
            .expect("insert FTS row for superseded node");
        }
        let report = service.check_semantics().expect("semantics check");
        assert_eq!(report.fts_rows_for_superseded_nodes, 1);
    }

    #[test]
    fn check_semantics_detects_dangling_edges() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute_batch("PRAGMA foreign_keys = OFF;")
                .expect("disable FK");
            // One active node as source; target does not exist — edge is dangling.
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r1', 'lg-src', 'Meeting', '{}', 100, 'src-1')",
                [],
            )
            .expect("insert source node");
            conn.execute(
                "INSERT INTO edges \
                 (row_id, logical_id, source_logical_id, target_logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('e1', 'edge-1', 'lg-src', 'ghost-target', 'LINKS', '{}', 100, 'src-1')",
                [],
            )
            .expect("insert dangling edge");
        }
        let report = service.check_semantics().expect("semantics check");
        assert_eq!(report.dangling_edges, 1);
    }

    #[test]
    fn check_semantics_detects_orphaned_supersession_chains() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            // Every version of this logical_id is superseded — no active row remains.
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, superseded_at, source_ref) \
                 VALUES ('r1', 'lg-orphaned', 'Meeting', '{}', 100, 200, 'src-1')",
                [],
            )
            .expect("insert fully superseded node");
        }
        let report = service.check_semantics().expect("semantics check");
        assert_eq!(report.orphaned_supersession_chains, 1);
    }

    #[test]
    fn check_semantics_detects_mismatched_kind_property_fts_rows() {
        // With per-kind tables, mismatched_kind is always 0 — rows in fts_props_<kind>
        // must belong to that kind by construction. However, orphaned rows (per-kind table
        // with no registered schema) serve as the equivalent signal and are tested via
        // check_semantics_detects_fts_rows_for_superseded_nodes. This test verifies
        // mismatched_kind is 0 even when per-kind table rows exist for a node.
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO fts_property_schemas (kind, property_paths_json, separator) \
                 VALUES ('Goal', '[\"$.name\"]', ' ')",
                [],
            )
            .expect("register schema");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r1', 'goal-1', 'Goal', '{\"name\":\"Ship v2\"}', 100, 'src-1')",
                [],
            )
            .expect("insert node");
            // Create the per-kind table and insert a correctly-kind row.
            let table = fathomdb_schema::fts_kind_table_name("Goal");
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS {table} \
                 USING fts5(node_logical_id UNINDEXED, text_content, tokenize = 'porter unicode61 remove_diacritics 2')"
            ))
            .expect("create per-kind table");
            conn.execute(
                &format!("INSERT INTO {table} (node_logical_id, text_content) VALUES ('goal-1', 'Ship v2')"),
                [],
            )
            .expect("insert per-kind FTS row");
        }
        let report = service.check_semantics().expect("semantics check");
        // Per-kind tables make mismatched_kind impossible — always 0.
        assert_eq!(report.mismatched_kind_property_fts_rows, 0);
    }

    #[test]
    fn check_semantics_detects_duplicate_property_fts_rows() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r1', 'goal-1', 'Goal', '{\"name\":\"Ship v2\"}', 100, 'src-1')",
                [],
            )
            .expect("insert node");
            // Create the per-kind table and insert two rows for the same logical ID.
            let table = fathomdb_schema::fts_kind_table_name("Goal");
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS {table} \
                 USING fts5(node_logical_id UNINDEXED, text_content, tokenize = 'porter unicode61 remove_diacritics 2')"
            ))
            .expect("create per-kind table");
            conn.execute(
                &format!("INSERT INTO {table} (node_logical_id, text_content) VALUES ('goal-1', 'Ship v2')"),
                [],
            )
            .expect("insert first property FTS row");
            conn.execute(
                &format!("INSERT INTO {table} (node_logical_id, text_content) VALUES ('goal-1', 'Ship v2 duplicate')"),
                [],
            )
            .expect("insert duplicate property FTS row");
        }
        let report = service.check_semantics().expect("semantics check");
        assert_eq!(report.duplicate_property_fts_rows, 1);
    }

    #[test]
    fn check_semantics_detects_drifted_property_fts_text() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO fts_property_schemas (kind, property_paths_json, separator) \
                 VALUES ('Goal', '[\"$.name\"]', ' ')",
                [],
            )
            .expect("register schema");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r1', 'goal-1', 'Goal', '{\"name\":\"Current name\"}', 100, 'src-1')",
                [],
            )
            .expect("insert node");
            // Create per-kind table and insert a row with outdated text content.
            let table = fathomdb_schema::fts_kind_table_name("Goal");
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS {table} \
                 USING fts5(node_logical_id UNINDEXED, text_content, tokenize = 'porter unicode61 remove_diacritics 2')"
            ))
            .expect("create per-kind table");
            conn.execute(
                &format!("INSERT INTO {table} (node_logical_id, text_content) VALUES ('goal-1', 'Old stale name')"),
                [],
            )
            .expect("insert stale property FTS row");
        }
        let report = service.check_semantics().expect("semantics check");
        assert_eq!(report.drifted_property_fts_rows, 1);
    }

    #[test]
    fn check_semantics_detects_property_fts_row_that_should_not_exist() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO fts_property_schemas (kind, property_paths_json, separator) \
                 VALUES ('Goal', '[\"$.searchable\"]', ' ')",
                [],
            )
            .expect("register schema");
            // Node does NOT have $.searchable — extraction yields no value.
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r1', 'goal-1', 'Goal', '{\"other\":\"field\"}', 100, 'src-1')",
                [],
            )
            .expect("insert node");
            // Create per-kind table and insert a phantom row that should not exist.
            let table = fathomdb_schema::fts_kind_table_name("Goal");
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS {table} \
                 USING fts5(node_logical_id UNINDEXED, text_content, tokenize = 'porter unicode61 remove_diacritics 2')"
            ))
            .expect("create per-kind table");
            conn.execute(
                &format!("INSERT INTO {table} (node_logical_id, text_content) VALUES ('goal-1', 'phantom text')"),
                [],
            )
            .expect("insert phantom property FTS row");
        }
        let report = service.check_semantics().expect("semantics check");
        assert_eq!(
            report.drifted_property_fts_rows, 1,
            "row that should not exist must be counted as drifted"
        );
    }

    /// Regression (0.5.2 Pack A): weighted (per-column) FTS property schemas
    /// used to crash `check_semantics` with
    /// `SqliteError: no such column: fp.text_content` because the drift
    /// counter hardcoded the non-weighted column shape. A clean DB with a
    /// weighted schema must now report 0 drift without panicking.
    #[test]
    fn check_semantics_clean_on_weighted_fts_schema_does_not_panic() {
        let (db, service) = setup();
        // Register a weighted schema (two specs: one scalar, one recursive).
        // Eager mode writes the schema row and creates the per-kind weighted
        // FTS table with per-path columns.
        // Weighted schemas (at least one entry with a weight) trigger the
        // per-column FTS table layout in `create_or_replace_fts_kind_table`.
        let entries = vec![
            FtsPropertyPathSpec::scalar("$.title").with_weight(2.0),
            FtsPropertyPathSpec::recursive("$.body").with_weight(1.0),
        ];
        service
            .register_fts_property_schema_with_entries(
                "Article",
                &entries,
                Some(" "),
                &[],
                crate::rebuild_actor::RebuildMode::Eager,
            )
            .expect("register weighted schema");

        // Insert a node and a matching FTS row whose per-column values match
        // the canonical extraction (expected: 0 drift).
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            let properties = r#"{"title":"Hello","body":{"text":"world"}}"#;
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r1', 'article-1', 'Article', ?1, 100, 'src-1')",
                [properties],
            )
            .expect("insert node");

            let schemas = crate::writer::load_fts_property_schemas(&conn).expect("load schemas");
            let (_kind, schema) = schemas
                .iter()
                .find(|(k, _)| k == "Article")
                .expect("weighted schema present");
            let props: serde_json::Value = serde_json::from_str(properties).expect("parse props");
            let cols = crate::writer::extract_property_fts_columns(&props, schema);

            let table = fathomdb_schema::fts_kind_table_name("Article");
            let col_names: Vec<String> = cols.iter().map(|(n, _)| n.clone()).collect();
            let placeholders: Vec<String> =
                (2..=col_names.len() + 1).map(|i| format!("?{i}")).collect();
            let sql = format!(
                "INSERT INTO {table} (node_logical_id, {cols}) VALUES (?1, {placeholders})",
                cols = col_names.join(", "),
                placeholders = placeholders.join(", "),
            );
            let values: Vec<String> = cols.iter().map(|(_, v)| v.clone()).collect();
            let params: Vec<&dyn rusqlite::ToSql> =
                std::iter::once(&"article-1" as &dyn rusqlite::ToSql)
                    .chain(values.iter().map(|v| v as &dyn rusqlite::ToSql))
                    .collect();
            conn.execute(&sql, params.as_slice())
                .expect("insert weighted FTS row");
        }

        let report = service
            .check_semantics()
            .expect("semantics check must not crash on weighted schema");
        assert_eq!(report.drifted_property_fts_rows, 0);
    }

    /// Regression (0.5.2 Pack A): drift detection in weighted FTS tables.
    /// After tampering a per-column value, `check_semantics` must count the
    /// row as drifted (once per row, regardless of how many columns mismatch).
    #[test]
    fn check_semantics_detects_drifted_property_fts_text_weighted() {
        let (db, service) = setup();
        // Weighted schemas (at least one entry with a weight) trigger the
        // per-column FTS table layout in `create_or_replace_fts_kind_table`.
        let entries = vec![
            FtsPropertyPathSpec::scalar("$.title").with_weight(2.0),
            FtsPropertyPathSpec::recursive("$.body").with_weight(1.0),
        ];
        service
            .register_fts_property_schema_with_entries(
                "Article",
                &entries,
                Some(" "),
                &[],
                crate::rebuild_actor::RebuildMode::Eager,
            )
            .expect("register weighted schema");

        let title_col = fathomdb_schema::fts_column_name("$.title", false);

        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            let properties = r#"{"title":"Current","body":{"text":"body"}}"#;
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r1', 'article-1', 'Article', ?1, 100, 'src-1')",
                [properties],
            )
            .expect("insert node");

            let schemas = crate::writer::load_fts_property_schemas(&conn).expect("load schemas");
            let (_kind, schema) = schemas
                .iter()
                .find(|(k, _)| k == "Article")
                .expect("weighted schema present");
            let props: serde_json::Value = serde_json::from_str(properties).expect("parse props");
            let cols = crate::writer::extract_property_fts_columns(&props, schema);

            let table = fathomdb_schema::fts_kind_table_name("Article");
            let col_names: Vec<String> = cols.iter().map(|(n, _)| n.clone()).collect();
            let placeholders: Vec<String> =
                (2..=col_names.len() + 1).map(|i| format!("?{i}")).collect();
            let sql = format!(
                "INSERT INTO {table} (node_logical_id, {cols}) VALUES (?1, {placeholders})",
                cols = col_names.join(", "),
                placeholders = placeholders.join(", "),
            );
            let values: Vec<String> = cols.iter().map(|(_, v)| v.clone()).collect();
            let params: Vec<&dyn rusqlite::ToSql> =
                std::iter::once(&"article-1" as &dyn rusqlite::ToSql)
                    .chain(values.iter().map(|v| v as &dyn rusqlite::ToSql))
                    .collect();
            conn.execute(&sql, params.as_slice())
                .expect("insert weighted FTS row");

            // Tamper the title column so it no longer matches canonical extraction.
            conn.execute(
                &format!("UPDATE {table} SET {title_col} = 'tampered' WHERE node_logical_id = 'article-1'"),
                [],
            )
            .expect("tamper weighted FTS row");
        }

        let report = service.check_semantics().expect("semantics check");
        assert_eq!(report.drifted_property_fts_rows, 1);
    }

    /// Regression (0.5.2 Pack A): a DB with both a weighted and a non-weighted
    /// per-kind FTS table must report 0 drift when both are in sync. This
    /// exercises the dispatcher: weighted path for one kind, non-weighted
    /// path for the other, in a single `check_semantics` call.
    #[test]
    fn check_semantics_mixed_weighted_and_non_weighted_schemas() {
        let (db, service) = setup();

        // Weighted schema for Article (per-column layout — requires weights).
        let weighted_entries = vec![
            FtsPropertyPathSpec::scalar("$.title").with_weight(2.0),
            FtsPropertyPathSpec::recursive("$.body").with_weight(1.0),
        ];
        service
            .register_fts_property_schema_with_entries(
                "Article",
                &weighted_entries,
                Some(" "),
                &[],
                crate::rebuild_actor::RebuildMode::Eager,
            )
            .expect("register weighted schema");

        // Non-weighted (single path) schema for Goal. The legacy JSON shape
        // (bare array of scalar paths) yields a non-weighted per-kind table.
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO fts_property_schemas (kind, property_paths_json, separator) \
                 VALUES ('Goal', '[\"$.name\"]', ' ')",
                [],
            )
            .expect("register non-weighted schema");
            let goal_table = fathomdb_schema::fts_kind_table_name("Goal");
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS {goal_table} \
                 USING fts5(node_logical_id UNINDEXED, text_content, tokenize = 'porter unicode61 remove_diacritics 2')"
            ))
            .expect("create non-weighted per-kind table");

            // Insert Article node + matching weighted FTS row.
            let article_props = r#"{"title":"Hello","body":{"text":"world"}}"#;
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r1', 'article-1', 'Article', ?1, 100, 'src-1')",
                [article_props],
            )
            .expect("insert article");

            let schemas = crate::writer::load_fts_property_schemas(&conn).expect("load schemas");
            let (_k, article_schema) = schemas
                .iter()
                .find(|(k, _)| k == "Article")
                .expect("Article schema present");
            let props: serde_json::Value =
                serde_json::from_str(article_props).expect("parse article props");
            let cols = crate::writer::extract_property_fts_columns(&props, article_schema);
            let article_table = fathomdb_schema::fts_kind_table_name("Article");
            let col_names: Vec<String> = cols.iter().map(|(n, _)| n.clone()).collect();
            let placeholders: Vec<String> =
                (2..=col_names.len() + 1).map(|i| format!("?{i}")).collect();
            let sql = format!(
                "INSERT INTO {article_table} (node_logical_id, {cols}) VALUES (?1, {placeholders})",
                cols = col_names.join(", "),
                placeholders = placeholders.join(", "),
            );
            let values: Vec<String> = cols.iter().map(|(_, v)| v.clone()).collect();
            let params: Vec<&dyn rusqlite::ToSql> =
                std::iter::once(&"article-1" as &dyn rusqlite::ToSql)
                    .chain(values.iter().map(|v| v as &dyn rusqlite::ToSql))
                    .collect();
            conn.execute(&sql, params.as_slice())
                .expect("insert weighted FTS row");

            // Insert Goal node + matching non-weighted FTS row. Canonical
            // extraction for legacy schema on $.name yields the string
            // "Goal One".
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r2', 'goal-1', 'Goal', '{\"name\":\"Goal One\"}', 100, 'src-2')",
                [],
            )
            .expect("insert goal node");
            conn.execute(
                &format!("INSERT INTO {goal_table} (node_logical_id, text_content) VALUES ('goal-1', 'Goal One')"),
                [],
            )
            .expect("insert non-weighted FTS row");
        }

        let report = service
            .check_semantics()
            .expect("semantics check must handle both shapes");
        assert_eq!(
            report.drifted_property_fts_rows, 0,
            "clean mixed weighted + non-weighted DB must report 0 drift"
        );
    }

    /// Regression (0.5.2 follow-up, review note): a weighted schema whose
    /// path set includes literal `$.text_content` collapses via
    /// `fts_column_name` to a `text_content` column. A live-column probe
    /// would then misclassify this weighted table as non-weighted and run
    /// the single-blob comparator against per-column storage — no crash,
    /// but silently incorrect drift counts (a clean DB would report drift).
    /// Dispatching on the persisted schema shape (any entry with
    /// `weight.is_some()` ⇒ weighted) avoids the collision.
    ///
    /// This test writes a CLEAN weighted row (per-column values matching
    /// canonical extraction) and asserts zero drift. Under the old
    /// live-column dispatcher the non-weighted comparator would read the
    /// single-path column value, compare it against the multi-path blob
    /// concatenation produced by `extract_property_fts`, and spuriously
    /// report drift.
    #[test]
    fn check_semantics_weighted_schema_with_text_content_path() {
        let (db, service) = setup();
        let entries = vec![
            FtsPropertyPathSpec::scalar("$.text_content").with_weight(2.0),
            FtsPropertyPathSpec::scalar("$.title").with_weight(1.0),
        ];
        service
            .register_fts_property_schema_with_entries(
                "Article",
                &entries,
                Some(" "),
                &[],
                crate::rebuild_actor::RebuildMode::Eager,
            )
            .expect("register weighted schema with $.text_content path");

        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            // Two distinct, non-empty scalar values. The non-weighted
            // comparator would expect their joined blob ("canonical body
            // Hello") as the single `text_content` column value; the
            // weighted (per-column) layout stores them separately, so on a
            // live-column probe the dispatcher would misclassify and
            // spuriously report drift.
            let properties = r#"{"text_content":"canonical body","title":"Hello"}"#;
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r1', 'article-1', 'Article', ?1, 100, 'src-1')",
                [properties],
            )
            .expect("insert node");

            let schemas = crate::writer::load_fts_property_schemas(&conn).expect("load schemas");
            let (_kind, schema) = schemas
                .iter()
                .find(|(k, _)| k == "Article")
                .expect("weighted schema present");
            let props: serde_json::Value = serde_json::from_str(properties).expect("parse props");
            let cols = crate::writer::extract_property_fts_columns(&props, schema);

            let table = fathomdb_schema::fts_kind_table_name("Article");
            let col_names: Vec<String> = cols.iter().map(|(n, _)| n.clone()).collect();
            let placeholders: Vec<String> =
                (2..=col_names.len() + 1).map(|i| format!("?{i}")).collect();
            let sql = format!(
                "INSERT INTO {table} (node_logical_id, {cols}) VALUES (?1, {placeholders})",
                cols = col_names.join(", "),
                placeholders = placeholders.join(", "),
            );
            let values: Vec<String> = cols.iter().map(|(_, v)| v.clone()).collect();
            let params: Vec<&dyn rusqlite::ToSql> =
                std::iter::once(&"article-1" as &dyn rusqlite::ToSql)
                    .chain(values.iter().map(|v| v as &dyn rusqlite::ToSql))
                    .collect();
            conn.execute(&sql, params.as_slice())
                .expect("insert weighted FTS row");
        }

        let report = service.check_semantics().expect("semantics check");
        assert_eq!(
            report.drifted_property_fts_rows, 0,
            "weighted schema whose path collapses to `text_content` must be \
             dispatched as weighted (per-column comparator); a clean DB \
             must report 0 drift"
        );
    }

    #[test]
    fn safe_export_writes_manifest_with_sha256() {
        let (_db, service) = setup();
        let export_dir = tempfile::TempDir::new().expect("temp dir");
        let export_path = export_dir.path().join("backup.db");

        let manifest = service
            .safe_export(
                &export_path,
                SafeExportOptions {
                    force_checkpoint: false,
                },
            )
            .expect("export");

        assert!(export_path.exists(), "exported db should exist");
        let manifest_path = export_dir.path().join("backup.db.export-manifest.json");
        assert!(
            manifest_path.exists(),
            "manifest file should exist at {}",
            manifest_path.display()
        );
        assert_eq!(manifest.sha256.len(), 64, "sha256 should be 64 hex chars");
        assert!(
            manifest.exported_at > 0,
            "exported_at should be a unix timestamp"
        );
        assert_eq!(
            manifest.schema_version,
            SchemaManager::new().current_version().0,
            "schema_version should match the live schema version"
        );
        assert_eq!(manifest.protocol_version, 1, "protocol_version should be 1");
        assert!(manifest.page_count > 0, "page_count should be positive");
    }

    #[test]
    fn safe_export_preserves_operational_validation_contracts() {
        let (_db, service) = setup();
        let validation_json = r#"{"format_version":1,"mode":"enforce","additional_properties":false,"fields":[{"name":"status","type":"string","required":true,"enum":["ok","failed"]}]}"#;
        service
            .register_operational_collection(&OperationalRegisterRequest {
                name: "connector_health".to_owned(),
                kind: OperationalCollectionKind::LatestState,
                schema_json: "{}".to_owned(),
                retention_json: "{}".to_owned(),
                filter_fields_json: "[]".to_owned(),
                validation_json: validation_json.to_owned(),
                secondary_indexes_json: "[]".to_owned(),
                format_version: 1,
            })
            .expect("register collection");

        let export_dir = tempfile::TempDir::new().expect("temp dir");
        let export_path = export_dir.path().join("backup.db");
        service
            .safe_export(
                &export_path,
                SafeExportOptions {
                    force_checkpoint: false,
                },
            )
            .expect("export");

        let exported = sqlite::open_connection(&export_path).expect("exported conn");
        let exported_validation_json: String = exported
            .query_row(
                "SELECT validation_json FROM operational_collections WHERE name = 'connector_health'",
                [],
                |row| row.get(0),
            )
            .expect("validation_json");
        assert_eq!(exported_validation_json, validation_json);
    }

    #[test]
    fn safe_export_force_checkpoint_false_skips_wal_pragma() {
        let (_db, service) = setup();
        let export_dir = tempfile::TempDir::new().expect("temp dir");
        let export_path = export_dir.path().join("no-wal.db");

        // force_checkpoint: false must not error even on a non-WAL database
        let manifest = service
            .safe_export(
                &export_path,
                SafeExportOptions {
                    force_checkpoint: false,
                },
            )
            .expect("export with no checkpoint");

        assert!(
            manifest.page_count > 0,
            "page_count must be populated regardless of checkpoint mode"
        );
        assert_eq!(
            manifest.schema_version,
            SchemaManager::new().current_version().0
        );
        assert_eq!(manifest.protocol_version, 1);
    }

    #[test]
    fn safe_export_force_checkpoint_false_still_captures_wal_backed_changes() {
        let (db, service) = setup();
        let conn = sqlite::open_connection(db.path()).expect("conn");
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
            .expect("enable wal");
        assert_eq!(journal_mode.to_lowercase(), "wal");
        let auto_checkpoint_pages: i64 = conn
            .query_row("PRAGMA wal_autocheckpoint=0", [], |row| row.get(0))
            .expect("disable auto checkpoint");
        assert_eq!(auto_checkpoint_pages, 0);
        conn.execute(
            "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
             VALUES ('r-wal', 'lg-wal', 'Meeting', '{}', 100, 'src-wal')",
            [],
        )
        .expect("insert wal-backed node");

        let export_dir = tempfile::TempDir::new().expect("temp dir");
        let export_path = export_dir.path().join("wal-backed.db");
        service
            .safe_export(
                &export_path,
                SafeExportOptions {
                    force_checkpoint: false,
                },
            )
            .expect("export wal-backed db");

        let exported = sqlite::open_connection(&export_path).expect("open exported db");
        let exported_count: i64 = exported
            .query_row(
                "SELECT count(*) FROM nodes WHERE logical_id = 'lg-wal'",
                [],
                |row| row.get(0),
            )
            .expect("count exported nodes");
        assert_eq!(
            exported_count, 1,
            "safe_export must include committed rows that are still resident in the WAL"
        );
    }

    #[test]
    fn excise_source_removes_searchable_content_after_excision() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, superseded_at, source_ref) \
                 VALUES ('r1', 'lg1', 'Meeting', '{}', 100, 200, 'source-1')",
                [],
            )
            .expect("insert v1");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r2', 'lg1', 'Meeting', '{}', 200, 'source-2')",
                [],
            )
            .expect("insert v2");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('ck1', 'lg1', 'hello world', 100)",
                [],
            )
            .expect("insert chunk");
        }
        service.excise_source("source-2").expect("excise");
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            let fts_count: i64 = conn
                .query_row(
                    "SELECT count(*) FROM fts_nodes WHERE chunk_id = 'ck1'",
                    [],
                    |row| row.get(0),
                )
                .expect("fts count");
            assert_eq!(
                fts_count, 0,
                "excised content should not remain searchable after excise"
            );
        }
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn excise_source_cleans_chunks_and_vec_rows_for_excised_version() {
        let (db, service) = setup();
        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            service
                .schema_manager
                .ensure_vec_kind_profile(&conn, "Meeting", 4)
                .expect("ensure vec kind profile");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, superseded_at, source_ref) \
                 VALUES ('r1', 'lg1', 'Meeting', '{}', 100, 200, 'source-1')",
                [],
            )
            .expect("insert v1");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r2', 'lg1', 'Meeting', '{}', 200, 'source-2')",
                [],
            )
            .expect("insert v2");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('ck1', 'lg1', 'new content', 200)",
                [],
            )
            .expect("insert chunk");
            conn.execute(
                "INSERT INTO vec_meeting (chunk_id, embedding) VALUES ('ck1', zeroblob(16))",
                [],
            )
            .expect("insert vec row");
        }

        service.excise_source("source-2").expect("excise");

        let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
        let active_row: String = conn
            .query_row(
                "SELECT row_id FROM nodes WHERE logical_id = 'lg1' AND superseded_at IS NULL",
                [],
                |row| row.get(0),
            )
            .expect("restored active row");
        assert_eq!(active_row, "r1");
        let chunk_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM chunks WHERE node_logical_id = 'lg1'",
                [],
                |row| row.get(0),
            )
            .expect("chunk count");
        assert_eq!(
            chunk_count, 0,
            "excised source content must not survive as chunks"
        );
        let vec_count: i64 = conn
            .query_row("SELECT count(*) FROM vec_meeting", [], |row| row.get(0))
            .expect("vec count");
        assert_eq!(vec_count, 0, "excised source vec rows must be removed");
        let fts_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM fts_nodes WHERE node_logical_id = 'lg1'",
                [],
                |row| row.get(0),
            )
            .expect("fts count");
        assert_eq!(
            fts_count, 0,
            "excised source content must not remain searchable"
        );
    }

    #[test]
    fn export_page_count_matches_exported_file() {
        let (_db, service) = setup();
        let export_dir = tempfile::TempDir::new().expect("temp dir");
        let export_path = export_dir.path().join("page-count.db");

        let manifest = service
            .safe_export(
                &export_path,
                SafeExportOptions {
                    force_checkpoint: false,
                },
            )
            .expect("export");

        let exported = sqlite::open_connection(&export_path).expect("open exported db");
        let actual_page_count: u64 = exported
            .query_row("PRAGMA page_count", [], |row| row.get(0))
            .expect("page_count from exported file");

        assert_eq!(
            manifest.page_count, actual_page_count,
            "manifest page_count must match the exported file's PRAGMA page_count"
        );
    }

    #[test]
    fn no_temp_file_after_successful_export() {
        let (_db, service) = setup();
        let export_dir = tempfile::TempDir::new().expect("temp dir");
        let export_path = export_dir.path().join("no-tmp.db");

        service
            .safe_export(
                &export_path,
                SafeExportOptions {
                    force_checkpoint: false,
                },
            )
            .expect("export");

        let tmp_files: Vec<_> = fs::read_dir(export_dir.path())
            .expect("read export dir")
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "tmp"))
            .collect();

        assert!(
            tmp_files.is_empty(),
            "no .tmp files should remain after a successful export, found: {tmp_files:?}"
        );
    }

    #[test]
    fn export_manifest_is_valid_json() {
        let (_db, service) = setup();
        let export_dir = tempfile::TempDir::new().expect("temp dir");
        let export_path = export_dir.path().join("valid-json.db");

        service
            .safe_export(
                &export_path,
                SafeExportOptions {
                    force_checkpoint: false,
                },
            )
            .expect("export");

        let manifest_path = export_dir.path().join("valid-json.db.export-manifest.json");
        let manifest_contents = fs::read_to_string(&manifest_path).expect("read manifest");
        let parsed: serde_json::Value =
            serde_json::from_str(&manifest_contents).expect("manifest must be valid JSON");

        assert!(
            parsed.get("exported_at").is_some(),
            "manifest must contain exported_at"
        );
        assert!(
            parsed.get("sha256").is_some(),
            "manifest must contain sha256"
        );
        assert!(
            parsed.get("schema_version").is_some(),
            "manifest must contain schema_version"
        );
        assert!(
            parsed.get("protocol_version").is_some(),
            "manifest must contain protocol_version"
        );
        assert!(
            parsed.get("page_count").is_some(),
            "manifest must contain page_count"
        );
    }

    #[test]
    fn provenance_purge_dry_run_reports_counts() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at) \
                 VALUES ('p1', 'node_insert', 'lg1', 'src-1', 100)",
                [],
            )
            .expect("insert p1");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at) \
                 VALUES ('p2', 'node_insert', 'lg2', 'src-1', 200)",
                [],
            )
            .expect("insert p2");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at) \
                 VALUES ('p3', 'excise', 'lg3', 'src-1', 300)",
                [],
            )
            .expect("insert p3");
        }

        let options = super::ProvenancePurgeOptions {
            dry_run: true,
            preserve_event_types: Vec::new(),
        };
        let report = service
            .purge_provenance_events(250, &options)
            .expect("dry run purge");

        assert_eq!(report.events_deleted, 2);
        assert_eq!(report.events_preserved, 1);
        assert!(report.oldest_remaining.is_some());

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let total: i64 = conn
            .query_row("SELECT count(*) FROM provenance_events", [], |row| {
                row.get(0)
            })
            .expect("count");
        assert_eq!(total, 3, "dry_run must not delete any events");
    }

    #[test]
    fn provenance_purge_deletes_old_events() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at) \
                 VALUES ('p1', 'node_insert', 'lg1', 'src-1', 100)",
                [],
            )
            .expect("insert p1");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at) \
                 VALUES ('p2', 'node_insert', 'lg2', 'src-1', 200)",
                [],
            )
            .expect("insert p2");
        }

        let options = super::ProvenancePurgeOptions {
            dry_run: false,
            preserve_event_types: Vec::new(),
        };
        let report = service
            .purge_provenance_events(150, &options)
            .expect("purge");

        assert_eq!(report.events_deleted, 1);
        assert_eq!(report.events_preserved, 1);
        assert_eq!(report.oldest_remaining, Some(200));

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let remaining: i64 = conn
            .query_row("SELECT count(*) FROM provenance_events", [], |row| {
                row.get(0)
            })
            .expect("count");
        assert_eq!(remaining, 1);
    }

    #[test]
    fn provenance_purge_preserves_specified_types() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at) \
                 VALUES ('p1', 'excise', 'lg1', 'src-1', 100)",
                [],
            )
            .expect("insert p1");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at) \
                 VALUES ('p2', 'node_insert', 'lg2', 'src-1', 100)",
                [],
            )
            .expect("insert p2");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at) \
                 VALUES ('p3', 'node_insert', 'lg3', 'src-1', 100)",
                [],
            )
            .expect("insert p3");
        }

        let options = super::ProvenancePurgeOptions {
            dry_run: false,
            preserve_event_types: Vec::new(),
        };
        let report = service
            .purge_provenance_events(500, &options)
            .expect("purge");

        assert_eq!(report.events_deleted, 2);
        assert_eq!(report.events_preserved, 1);

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let remaining_type: String = conn
            .query_row("SELECT event_type FROM provenance_events", [], |row| {
                row.get(0)
            })
            .expect("remaining event type");
        assert_eq!(remaining_type, "excise");
    }

    #[test]
    fn provenance_purge_noop_with_zero_timestamp() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at) \
                 VALUES ('p1', 'node_insert', 'lg1', 'src-1', 100)",
                [],
            )
            .expect("insert p1");
        }

        let options = super::ProvenancePurgeOptions {
            dry_run: false,
            preserve_event_types: Vec::new(),
        };
        let report = service.purge_provenance_events(0, &options).expect("purge");

        assert_eq!(report.events_deleted, 0);
        assert_eq!(report.events_preserved, 1);
        assert_eq!(report.oldest_remaining, Some(100));
    }

    #[test]
    fn restore_skips_edge_when_counterpart_purged() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            // Create node A (doc-1) and node B (doc-2)
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('node-row-a', 'doc-1', 'Document', '{}', 100, 'seed')",
                [],
            )
            .expect("insert node A");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('node-row-b', 'doc-2', 'Document', '{}', 100, 'seed')",
                [],
            )
            .expect("insert node B");
            // Create edge between A and B
            conn.execute(
                "INSERT INTO edges \
                 (row_id, logical_id, source_logical_id, target_logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('edge-row-1', 'edge-1', 'doc-1', 'doc-2', 'RELATED', '{}', 100, 'seed')",
                [],
            )
            .expect("insert edge");
            // Retire both A and B, and the edge
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-retire-a', 'node_retire', 'doc-1', 'forget-1', 200, '')",
                [],
            )
            .expect("insert retire event A");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-edge-retire', 'edge_retire', 'edge-1', 'forget-1', 200, '')",
                [],
            )
            .expect("insert edge retire event");
            conn.execute(
                "UPDATE nodes SET superseded_at = 200 WHERE logical_id = 'doc-1'",
                [],
            )
            .expect("retire node A");
            conn.execute(
                "UPDATE nodes SET superseded_at = 200 WHERE logical_id = 'doc-2'",
                [],
            )
            .expect("retire node B");
            conn.execute(
                "UPDATE edges SET superseded_at = 200 WHERE logical_id = 'edge-1'",
                [],
            )
            .expect("retire edge");
            // Simulate purge of B: delete node rows but leave the edge intact
            // to reproduce the dangling-edge scenario the validation guards against.
            conn.execute("DELETE FROM nodes WHERE logical_id = 'doc-2'", [])
                .expect("purge node B rows");
        }

        // Restore A — the edge should be skipped because B has no active node
        let report = service.restore_logical_id("doc-1").expect("restore A");
        assert!(!report.was_noop);
        assert_eq!(report.restored_node_rows, 1);
        assert_eq!(report.restored_edge_rows, 0, "edge should not be restored");
        assert_eq!(report.skipped_edges.len(), 1);
        assert_eq!(report.skipped_edges[0].edge_logical_id, "edge-1");
        assert_eq!(report.skipped_edges[0].missing_endpoint, "doc-2");

        // Verify the edge is still retired in the database
        let conn = sqlite::open_connection(db.path()).expect("conn");
        let active_edge_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM edges WHERE logical_id = 'edge-1' AND superseded_at IS NULL",
                [],
                |row| row.get(0),
            )
            .expect("active edge count");
        assert_eq!(active_edge_count, 0, "edge must remain retired");
    }

    #[test]
    fn restore_restores_edges_to_active_nodes() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            // Create node A and node B (B stays active)
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('node-row-a', 'doc-1', 'Document', '{}', 100, 'seed')",
                [],
            )
            .expect("insert node A");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('node-row-b', 'doc-2', 'Document', '{}', 100, 'seed')",
                [],
            )
            .expect("insert node B");
            // Create edge between A and B
            conn.execute(
                "INSERT INTO edges \
                 (row_id, logical_id, source_logical_id, target_logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('edge-row-1', 'edge-1', 'doc-1', 'doc-2', 'RELATED', '{}', 100, 'seed')",
                [],
            )
            .expect("insert edge");
            // Retire only A
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-retire-a', 'node_retire', 'doc-1', 'forget-1', 200, '')",
                [],
            )
            .expect("insert retire event A");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-edge-retire', 'edge_retire', 'edge-1', 'forget-1', 200, '')",
                [],
            )
            .expect("insert edge retire event");
            conn.execute(
                "UPDATE nodes SET superseded_at = 200 WHERE logical_id = 'doc-1'",
                [],
            )
            .expect("retire node A");
            conn.execute(
                "UPDATE edges SET superseded_at = 200 WHERE logical_id = 'edge-1'",
                [],
            )
            .expect("retire edge");
        }

        // Restore A — B is active, so the edge should be restored normally
        let report = service.restore_logical_id("doc-1").expect("restore A");
        assert!(!report.was_noop);
        assert_eq!(report.restored_node_rows, 1);
        assert!(report.restored_edge_rows > 0, "edge should be restored");
        assert!(
            report.skipped_edges.is_empty(),
            "no edges should be skipped"
        );

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let active_edge_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM edges WHERE logical_id = 'edge-1' AND superseded_at IS NULL",
                [],
                |row| row.get(0),
            )
            .expect("active edge count");
        assert_eq!(active_edge_count, 1, "edge must be active");
    }

    #[test]
    fn restore_restores_edges_when_both_restored() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            // Create node A and node B
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('node-row-a', 'doc-1', 'Document', '{}', 100, 'seed')",
                [],
            )
            .expect("insert node A");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('node-row-b', 'doc-2', 'Document', '{}', 100, 'seed')",
                [],
            )
            .expect("insert node B");
            // Create edge between A and B
            conn.execute(
                "INSERT INTO edges \
                 (row_id, logical_id, source_logical_id, target_logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('edge-row-1', 'edge-1', 'doc-1', 'doc-2', 'RELATED', '{}', 100, 'seed')",
                [],
            )
            .expect("insert edge");
            // Retire both A and B
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-retire-a', 'node_retire', 'doc-1', 'forget-1', 200, '')",
                [],
            )
            .expect("insert retire event A");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-retire-b', 'node_retire', 'doc-2', 'forget-1', 200, '')",
                [],
            )
            .expect("insert retire event B");
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-edge-retire', 'edge_retire', 'edge-1', 'forget-1', 200, '')",
                [],
            )
            .expect("insert edge retire event");
            conn.execute(
                "UPDATE nodes SET superseded_at = 200 WHERE logical_id = 'doc-1'",
                [],
            )
            .expect("retire node A");
            conn.execute(
                "UPDATE nodes SET superseded_at = 200 WHERE logical_id = 'doc-2'",
                [],
            )
            .expect("retire node B");
            conn.execute(
                "UPDATE edges SET superseded_at = 200 WHERE logical_id = 'edge-1'",
                [],
            )
            .expect("retire edge");
        }

        // Restore B first — edge is skipped because A is still retired
        let report_b = service.restore_logical_id("doc-2").expect("restore B");
        assert!(!report_b.was_noop);

        // Restore A — B is now active, so the edge should be restored
        let report_a = service.restore_logical_id("doc-1").expect("restore A");
        assert!(!report_a.was_noop);
        assert_eq!(report_a.restored_node_rows, 1);
        assert!(
            report_a.restored_edge_rows > 0,
            "edge should be restored when both endpoints active"
        );
        assert!(
            report_a.skipped_edges.is_empty(),
            "no edges should be skipped"
        );

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let active_edge_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM edges WHERE logical_id = 'edge-1' AND superseded_at IS NULL",
                [],
                |row| row.get(0),
            )
            .expect("active edge count");
        assert_eq!(
            active_edge_count, 1,
            "edge must be active after both endpoints restored"
        );
    }

    // ── FTS property schema end-to-end tests ──────────────────────────

    #[test]
    fn fts_property_schema_crud_round_trip() {
        let (_db, service) = setup();

        // Register
        let record = service
            .register_fts_property_schema(
                "Meeting",
                &["$.title".to_owned(), "$.summary".to_owned()],
                None,
            )
            .expect("register");
        assert_eq!(record.kind, "Meeting");
        assert_eq!(record.property_paths, vec!["$.title", "$.summary"]);
        assert_eq!(record.separator, " ");
        assert_eq!(record.format_version, 1);

        // Describe
        let described = service
            .describe_fts_property_schema("Meeting")
            .expect("describe")
            .expect("should exist");
        assert_eq!(described, record);

        // Describe missing kind
        let missing = service
            .describe_fts_property_schema("NoSuchKind")
            .expect("describe missing");
        assert!(missing.is_none());

        // List
        let list = service.list_fts_property_schemas().expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, "Meeting");

        // Update (idempotent upsert)
        let updated = service
            .register_fts_property_schema(
                "Meeting",
                &["$.title".to_owned(), "$.notes".to_owned()],
                Some("\n"),
            )
            .expect("update");
        assert_eq!(updated.property_paths, vec!["$.title", "$.notes"]);
        assert_eq!(updated.separator, "\n");

        // Remove
        service
            .remove_fts_property_schema("Meeting")
            .expect("remove");
        let after_remove = service
            .describe_fts_property_schema("Meeting")
            .expect("describe after remove");
        assert!(after_remove.is_none());

        // Remove non-existent is an error
        let err = service.remove_fts_property_schema("Meeting");
        assert!(err.is_err());
    }

    #[test]
    fn describe_fts_property_schema_round_trips_recursive_entries() {
        let (_db, service) = setup();

        let entries = vec![
            FtsPropertyPathSpec::scalar("$.title"),
            FtsPropertyPathSpec::recursive("$.payload"),
        ];
        let exclude = vec!["$.payload.private".to_owned()];
        let registered = service
            .register_fts_property_schema_with_entries(
                "KnowledgeItem",
                &entries,
                Some(" "),
                &exclude,
                crate::rebuild_actor::RebuildMode::Eager,
            )
            .expect("register recursive");

        // The register entry point now echoes back the fully-populated
        // record via the same load helper used by describe/list.
        assert_eq!(registered.entries, entries);
        assert_eq!(registered.exclude_paths, exclude);
        assert_eq!(registered.property_paths, vec!["$.title", "$.payload"]);

        let described = service
            .describe_fts_property_schema("KnowledgeItem")
            .expect("describe")
            .expect("should exist");
        assert_eq!(described.kind, "KnowledgeItem");
        assert_eq!(described.entries, entries);
        assert_eq!(described.exclude_paths, exclude);
        assert_eq!(described.property_paths, vec!["$.title", "$.payload"]);
        assert_eq!(described.separator, " ");
        assert_eq!(described.format_version, 1);
    }

    #[test]
    fn list_fts_property_schemas_round_trips_recursive_entries() {
        let (_db, service) = setup();

        let entries = vec![
            FtsPropertyPathSpec::scalar("$.title"),
            FtsPropertyPathSpec::recursive("$.payload"),
        ];
        let exclude = vec!["$.payload.secret".to_owned()];
        service
            .register_fts_property_schema_with_entries(
                "KnowledgeItem",
                &entries,
                Some(" "),
                &exclude,
                crate::rebuild_actor::RebuildMode::Eager,
            )
            .expect("register recursive");

        let listed = service.list_fts_property_schemas().expect("list");
        assert_eq!(listed.len(), 1);
        let record = &listed[0];
        assert_eq!(record.kind, "KnowledgeItem");
        assert_eq!(record.entries, entries);
        assert_eq!(record.exclude_paths, exclude);
        assert_eq!(record.property_paths, vec!["$.title", "$.payload"]);
    }

    #[test]
    fn describe_fts_property_schema_round_trips_scalar_only_entries() {
        let (_db, service) = setup();

        service
            .register_fts_property_schema(
                "Meeting",
                &["$.title".to_owned(), "$.summary".to_owned()],
                None,
            )
            .expect("register scalar");

        let described = service
            .describe_fts_property_schema("Meeting")
            .expect("describe")
            .expect("should exist");
        assert_eq!(described.property_paths, vec!["$.title", "$.summary"]);
        assert_eq!(described.entries.len(), 2);
        for entry in &described.entries {
            assert_eq!(
                entry.mode,
                FtsPropertyPathMode::Scalar,
                "scalar-only schema should deserialize every entry as Scalar"
            );
        }
        assert!(described.exclude_paths.is_empty());
    }

    #[test]
    fn restore_reestablishes_property_fts_visibility() {
        let (db, service) = setup();
        let doc_table = fathomdb_schema::fts_kind_table_name("Document");
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            // Register a property schema for Document kind.
            conn.execute(
                "INSERT INTO fts_property_schemas (kind, property_paths_json, separator) \
                 VALUES ('Document', '[\"$.title\", \"$.body\"]', ' ')",
                [],
            )
            .expect("register schema");
            // Create the per-kind FTS table.
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS {doc_table} USING fts5(\
                    node_logical_id UNINDEXED, text_content, \
                    tokenize = 'porter unicode61 remove_diacritics 2'\
                )"
            ))
            .expect("create per-kind table");
            // Insert an active node with extractable properties.
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'doc-1', 'Document', '{\"title\":\"Budget\",\"body\":\"Q3 forecast\"}', 100, 'seed')",
                [],
            )
            .expect("insert node");
            // Insert a chunk so restore has something to work with for FTS.
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'doc-1', 'budget text', 100)",
                [],
            )
            .expect("insert chunk");
            // Insert property FTS row into per-kind table (as write path would).
            conn.execute(
                &format!(
                    "INSERT INTO {doc_table} (node_logical_id, text_content) \
                     VALUES ('doc-1', 'Budget Q3 forecast')"
                ),
                [],
            )
            .expect("insert property fts");
            // Simulate retire: supersede node, clear FTS.
            conn.execute(
                "INSERT INTO provenance_events (id, event_type, subject, source_ref, created_at, metadata_json) \
                 VALUES ('evt-retire', 'node_retire', 'doc-1', 'forget-1', 200, '')",
                [],
            )
            .expect("retire event");
            conn.execute(
                "UPDATE nodes SET superseded_at = 200 WHERE logical_id = 'doc-1'",
                [],
            )
            .expect("supersede");
            conn.execute("DELETE FROM fts_nodes", [])
                .expect("clear chunk fts");
            conn.execute(&format!("DELETE FROM {doc_table}"), [])
                .expect("clear property fts");
        }

        let report = service.restore_logical_id("doc-1").expect("restore");
        assert_eq!(report.restored_property_fts_rows, 1);

        // Verify the property FTS row was recreated in the per-kind table.
        let conn = sqlite::open_connection(db.path()).expect("conn");
        let prop_fts_count: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM {doc_table} WHERE node_logical_id = 'doc-1'"),
                [],
                |row| row.get(0),
            )
            .expect("prop fts count");
        assert_eq!(prop_fts_count, 1, "property FTS must be restored");

        let text: String = conn
            .query_row(
                &format!("SELECT text_content FROM {doc_table} WHERE node_logical_id = 'doc-1'"),
                [],
                |row| row.get(0),
            )
            .expect("prop fts text");
        assert_eq!(text, "Budget Q3 forecast");
    }

    #[test]
    fn safe_export_preserves_fts_property_schemas() {
        let (_db, service) = setup();
        service
            .register_fts_property_schema(
                "Goal",
                &["$.name".to_owned(), "$.rationale".to_owned()],
                None,
            )
            .expect("register schema");

        let export_dir = tempfile::TempDir::new().expect("temp dir");
        let export_path = export_dir.path().join("backup.db");
        service
            .safe_export(
                &export_path,
                SafeExportOptions {
                    force_checkpoint: false,
                },
            )
            .expect("export");

        // Open the exported DB and verify the schema survived.
        let exported_conn = rusqlite::Connection::open(&export_path).expect("open exported db");
        let kind: String = exported_conn
            .query_row(
                "SELECT kind FROM fts_property_schemas WHERE kind = 'Goal'",
                [],
                |row| row.get(0),
            )
            .expect("schema must exist in export");
        assert_eq!(kind, "Goal");
        let paths_json: String = exported_conn
            .query_row(
                "SELECT property_paths_json FROM fts_property_schemas WHERE kind = 'Goal'",
                [],
                |row| row.get(0),
            )
            .expect("paths must exist");
        let paths: Vec<String> = serde_json::from_str(&paths_json).expect("valid json");
        assert_eq!(paths, vec!["$.name", "$.rationale"]);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn export_recovery_rebuilds_property_fts_from_canonical_state() {
        let (db, service) = setup();
        let goal_table = fathomdb_schema::fts_kind_table_name("Goal");
        // Register a schema and insert two nodes with extractable properties.
        service
            .register_fts_property_schema("Goal", &["$.name".to_owned()], None)
            .expect("register");
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'goal-1', 'Goal', '{\"name\":\"Ship v2\"}', 100, 'seed')",
                [],
            )
            .expect("insert node 1");
            conn.execute(
                &format!(
                    "INSERT INTO {goal_table} (node_logical_id, text_content) \
                     VALUES ('goal-1', 'Ship v2')"
                ),
                [],
            )
            .expect("insert property FTS row 1");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-2', 'goal-2', 'Goal', '{\"name\":\"Launch redesign\"}', 100, 'seed')",
                [],
            )
            .expect("insert node 2");
            conn.execute(
                &format!(
                    "INSERT INTO {goal_table} (node_logical_id, text_content) \
                     VALUES ('goal-2', 'Launch redesign')"
                ),
                [],
            )
            .expect("insert property FTS row 2");
        }

        // Export.
        let export_dir = tempfile::TempDir::new().expect("temp dir");
        let export_path = export_dir.path().join("backup.db");
        service
            .safe_export(
                &export_path,
                SafeExportOptions {
                    force_checkpoint: false,
                },
            )
            .expect("export");

        // Corrupt the derived rows: replace correct text with wrong text for
        // goal-1, and delete the row for goal-2 entirely. This exercises both
        // corrupted-but-present rows and missing rows in the same recovery.
        {
            let conn = rusqlite::Connection::open(&export_path).expect("open export");
            // Bootstrap the exported DB to get per-kind tables.
            SchemaManager::new()
                .bootstrap(&conn)
                .expect("bootstrap export");
            conn.execute(
                &format!("DELETE FROM {goal_table} WHERE node_logical_id = 'goal-1'"),
                [],
            )
            .expect("delete old row");
            conn.execute(
                &format!(
                    "INSERT INTO {goal_table} (node_logical_id, text_content) \
                     VALUES ('goal-1', 'completely wrong stale text')"
                ),
                [],
            )
            .expect("insert corrupted row");
            conn.execute(
                &format!("DELETE FROM {goal_table} WHERE node_logical_id = 'goal-2'"),
                [],
            )
            .expect("delete goal-2 row");
        }

        // Open the exported DB and rebuild projections from canonical state.
        let schema = Arc::new(SchemaManager::new());
        let exported_service = AdminService::new(&export_path, Arc::clone(&schema));
        exported_service
            .rebuild_projections(ProjectionTarget::Fts)
            .expect("rebuild");

        // Verify the per-kind table has the correct rows after recovery.
        let conn = rusqlite::Connection::open(&export_path).expect("open export for verify");
        let goal1_text: String = conn
            .query_row(
                &format!("SELECT text_content FROM {goal_table} WHERE node_logical_id = 'goal-1'"),
                [],
                |r| r.get(0),
            )
            .expect("goal-1 text after rebuild");
        assert_eq!(
            goal1_text, "Ship v2",
            "goal-1 text must be corrected by rebuild"
        );

        let goal2_count: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM {goal_table} WHERE node_logical_id = 'goal-2'"),
                [],
                |r| r.get(0),
            )
            .expect("goal-2 count");
        assert_eq!(goal2_count, 1, "goal-2 row must be restored by rebuild");

        let stale_count: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM {goal_table} WHERE text_content = 'completely wrong stale text'"),
                [],
                |r| r.get(0),
            )
            .expect("stale count");
        assert_eq!(stale_count, 0, "corrupted text must be gone after rebuild");

        // Verify integrity and semantics are clean after recovery.
        let integrity = exported_service.check_integrity().expect("integrity");
        assert_eq!(integrity.missing_property_fts_rows, 0);
        let semantics = exported_service.check_semantics().expect("semantics");
        assert_eq!(semantics.drifted_property_fts_rows, 0);
        assert_eq!(semantics.orphaned_property_fts_rows, 0);
        assert_eq!(semantics.duplicate_property_fts_rows, 0);
    }

    #[test]
    fn check_integrity_no_false_positives_for_empty_extraction() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            // Register a schema that looks for $.searchable
            conn.execute(
                "INSERT INTO fts_property_schemas (kind, property_paths_json, separator) \
                 VALUES ('Ticket', '[\"$.searchable\"]', ' ')",
                [],
            )
            .expect("register schema");
            // Insert a node whose properties do NOT contain $.searchable —
            // correctly has no property FTS row.
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'ticket-1', 'Ticket', '{\"status\":\"open\"}', 100, 'seed')",
                [],
            )
            .expect("insert node");
        }

        let report = service.check_integrity().expect("integrity");
        assert_eq!(
            report.missing_property_fts_rows, 0,
            "node with no extractable values must not be counted as missing"
        );
    }

    #[test]
    fn check_integrity_detects_genuinely_missing_property_fts_rows() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO fts_property_schemas (kind, property_paths_json, separator) \
                 VALUES ('Ticket', '[\"$.title\"]', ' ')",
                [],
            )
            .expect("register schema");
            // Insert a node WITH an extractable $.title but no property FTS row.
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'ticket-1', 'Ticket', '{\"title\":\"fix login bug\"}', 100, 'seed')",
                [],
            )
            .expect("insert node");
        }

        let report = service.check_integrity().expect("integrity");
        assert_eq!(
            report.missing_property_fts_rows, 1,
            "node with extractable values but no property FTS row must be detected"
        );
    }

    #[test]
    fn rebuild_projections_fts_restores_missing_property_fts_rows() {
        let (db, service) = setup();
        let goal_table = fathomdb_schema::fts_kind_table_name("Goal");
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO fts_property_schemas (kind, property_paths_json, separator) \
                 VALUES ('Goal', '[\"$.name\"]', ' ')",
                [],
            )
            .expect("register schema");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'goal-1', 'Goal', '{\"name\":\"Ship v2\"}', 100, 'seed')",
                [],
            )
            .expect("insert node");
            // Deliberately do NOT insert a property FTS row.
        }

        let report = service
            .rebuild_projections(ProjectionTarget::Fts)
            .expect("rebuild");
        assert!(
            report.rebuilt_rows >= 1,
            "rebuild must insert at least one property FTS row"
        );

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let text: String = conn
            .query_row(
                &format!("SELECT text_content FROM {goal_table} WHERE node_logical_id = 'goal-1'"),
                [],
                |row| row.get(0),
            )
            .expect("property FTS row must exist after rebuild");
        assert_eq!(text, "Ship v2");
    }

    #[test]
    fn rebuild_missing_projections_fills_gap_for_deleted_property_fts_row() {
        let (db, service) = setup();
        let goal_table = fathomdb_schema::fts_kind_table_name("Goal");
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO fts_property_schemas (kind, property_paths_json, separator) \
                 VALUES ('Goal', '[\"$.name\"]', ' ')",
                [],
            )
            .expect("register schema");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'goal-1', 'Goal', '{\"name\":\"Ship v2\"}', 100, 'seed')",
                [],
            )
            .expect("insert node");
            // Create per-kind table and insert then delete to simulate corruption.
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS {goal_table} USING fts5(\
                    node_logical_id UNINDEXED, text_content, \
                    tokenize = 'porter unicode61 remove_diacritics 2'\
                )"
            ))
            .expect("create per-kind table");
            conn.execute(
                &format!(
                    "INSERT INTO {goal_table} (node_logical_id, text_content) \
                     VALUES ('goal-1', 'Ship v2')"
                ),
                [],
            )
            .expect("insert property fts");
            conn.execute(
                &format!("DELETE FROM {goal_table} WHERE node_logical_id = 'goal-1'"),
                [],
            )
            .expect("delete property fts");
        }

        let report = service
            .rebuild_missing_projections()
            .expect("rebuild missing");
        assert!(
            report.rebuilt_rows >= 1,
            "missing rebuild must insert the gap-fill row"
        );

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let count: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM {goal_table} WHERE node_logical_id = 'goal-1'"),
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(
            count, 1,
            "gap-fill must restore exactly one property FTS row"
        );
    }

    #[test]
    fn remove_schema_then_rebuild_cleans_stale_property_fts_rows() {
        // This test verifies that a full FTS rebuild clears per-kind tables whose
        // schema has been removed (orphaned state). We create the orphaned state
        // directly via SQL (bypassing the service API, which now eagerly deletes rows
        // on schema removal) to simulate a table that was left populated from a
        // previous registration cycle.
        let (db, service) = setup();
        let goal_table = fathomdb_schema::fts_kind_table_name("Goal");
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'goal-1', 'Goal', '{\"name\":\"Ship v2\"}', 100, 'seed')",
                [],
            )
            .expect("insert node");
            // Create per-kind table WITHOUT registering a schema — simulates orphaned rows
            // that remain after schema removal (or pre-existing table from a previous cycle).
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS {goal_table} \
                 USING fts5(node_logical_id UNINDEXED, text_content, tokenize = 'porter unicode61 remove_diacritics 2')"
            ))
            .expect("create per-kind table");
            conn.execute(
                &format!(
                    "INSERT INTO {goal_table} (node_logical_id, text_content) \
                     VALUES ('goal-1', 'Ship v2')"
                ),
                [],
            )
            .expect("insert property fts");
        }

        // No schema registered — per-kind table has orphaned rows.
        let semantics = service.check_semantics().expect("semantics");
        assert_eq!(
            semantics.orphaned_property_fts_rows, 1,
            "orphaned property FTS rows must be detected with no registered schema"
        );

        // Full rebuild should clean them (no schema means nothing to rebuild).
        service
            .rebuild_projections(ProjectionTarget::Fts)
            .expect("rebuild");

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let count: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM {goal_table} WHERE node_logical_id = 'goal-1'"),
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(
            count, 0,
            "rebuild must delete rows from per-kind tables with no registered schema"
        );
    }

    mod validate_fts_property_paths_tests {
        use super::super::validate_fts_property_paths;

        #[test]
        fn valid_simple_path() {
            assert!(validate_fts_property_paths(&["$.name".to_owned()]).is_ok());
        }

        #[test]
        fn valid_nested_path() {
            assert!(validate_fts_property_paths(&["$.address.city".to_owned()]).is_ok());
        }

        #[test]
        fn valid_underscore_segment() {
            assert!(validate_fts_property_paths(&["$.a_b".to_owned()]).is_ok());
        }

        #[test]
        fn rejects_bare_prefix() {
            let result = validate_fts_property_paths(&["$.".to_owned()]);
            assert!(result.is_err(), "path '$.' must be rejected");
        }

        #[test]
        fn rejects_double_dot() {
            let result = validate_fts_property_paths(&["$..x".to_owned()]);
            assert!(result.is_err(), "path '$..x' must be rejected");
        }

        #[test]
        fn rejects_trailing_dot() {
            let result = validate_fts_property_paths(&["$.foo.".to_owned()]);
            assert!(result.is_err(), "path '$.foo.' must be rejected");
        }

        #[test]
        fn rejects_space_in_segment() {
            let result = validate_fts_property_paths(&["$.foo bar".to_owned()]);
            assert!(result.is_err(), "path '$.foo bar' must be rejected");
        }

        #[test]
        fn rejects_bracket_syntax() {
            let result = validate_fts_property_paths(&["$.foo[0]".to_owned()]);
            assert!(result.is_err(), "path '$.foo[0]' must be rejected");
        }

        #[test]
        fn rejects_duplicates() {
            let result = validate_fts_property_paths(&["$.name".to_owned(), "$.name".to_owned()]);
            assert!(result.is_err(), "duplicate paths must be rejected");
        }

        #[test]
        fn rejects_empty_list() {
            let result = validate_fts_property_paths(&[]);
            assert!(result.is_err(), "empty path list must be rejected");
        }
    }

    // --- A-6: per-kind FTS table tests ---

    #[test]
    fn register_fts_schema_writes_to_per_kind_table() {
        // After A-6: register_fts_property_schema writes rows to fts_props_<kind>,
        // NOT to fts_node_properties.
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            // Insert a node before registering the schema.
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'goal-1', 'Goal', '{\"name\":\"Ship v2\"}', 100, 'seed')",
                [],
            )
            .expect("insert node");
        }

        // Register schema — this triggers eager rebuild which writes to per-kind table.
        service
            .register_fts_property_schema("Goal", &["$.name".to_owned()], None)
            .expect("register schema");

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let table = fathomdb_schema::fts_kind_table_name("Goal");
        // Per-kind table must have the row.
        let per_kind_count: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM {table} WHERE node_logical_id = 'goal-1'"),
                [],
                |row| row.get(0),
            )
            .expect("per-kind count");
        assert_eq!(
            per_kind_count, 1,
            "per-kind table must have the row after registration"
        );
    }

    #[test]
    fn remove_fts_schema_deletes_from_per_kind_table() {
        // After A-6: remove_fts_property_schema deletes rows from fts_props_<kind>.
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'goal-1', 'Goal', '{\"name\":\"Ship v2\"}', 100, 'seed')",
                [],
            )
            .expect("insert node");
        }

        service
            .register_fts_property_schema("Goal", &["$.name".to_owned()], None)
            .expect("register schema");
        service
            .remove_fts_property_schema("Goal")
            .expect("remove schema");

        let conn = sqlite::open_connection(db.path()).expect("conn");
        let table = fathomdb_schema::fts_kind_table_name("Goal");
        let per_kind_count: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM {table} WHERE node_logical_id = 'goal-1'"),
                [],
                |row| row.get(0),
            )
            .expect("per-kind count");
        assert_eq!(
            per_kind_count, 0,
            "per-kind table must be empty after schema removal"
        );
    }

    // --- B-1: weight field tests ---

    #[test]
    fn fts_path_spec_with_weight_builder() {
        let spec = FtsPropertyPathSpec::scalar("$.title").with_weight(5.0);
        assert_eq!(spec.weight, Some(5.0));
        assert_eq!(spec.path, "$.title");
        assert_eq!(spec.mode, FtsPropertyPathMode::Scalar);
    }

    #[test]
    fn fts_path_spec_serialize_with_weight() {
        use super::serialize_property_paths_json;
        let entries = vec![
            FtsPropertyPathSpec::scalar("$.title").with_weight(2.0),
            FtsPropertyPathSpec::scalar("$.body"),
        ];
        let json = serialize_property_paths_json(&entries, &[]).expect("serialize");
        // Must use rich object format because a weight is present
        let v: serde_json::Value = serde_json::from_str(&json).expect("parse");
        let paths = v
            .get("paths")
            .expect("paths key")
            .as_array()
            .expect("array");
        assert_eq!(paths.len(), 2);
        // First entry has weight
        assert_eq!(
            paths[0].get("path").and_then(serde_json::Value::as_str),
            Some("$.title")
        );
        assert_eq!(
            paths[0].get("weight").and_then(serde_json::Value::as_f64),
            Some(2.0)
        );
        // Second entry has no weight field
        assert!(
            paths[1].get("weight").is_none(),
            "unweighted spec must omit weight field"
        );
    }

    #[test]
    fn fts_path_spec_serialize_no_weights() {
        use super::serialize_property_paths_json;
        let entries = vec![
            FtsPropertyPathSpec::scalar("$.title"),
            FtsPropertyPathSpec::scalar("$.payload"),
        ];
        let json = serialize_property_paths_json(&entries, &[]).expect("serialize");
        // Must use bare string array (backward compat)
        let v: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert!(
            v.is_array(),
            "all-scalar no-weight schema must serialize as bare string array"
        );
        let arr = v.as_array().expect("array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].as_str(), Some("$.title"));
        assert_eq!(arr[1].as_str(), Some("$.payload"));
    }

    #[test]
    fn fts_weight_validation_out_of_range() {
        let (_db, service) = setup();
        // weight = 0.0 must be rejected
        let entries_zero = vec![FtsPropertyPathSpec::scalar("$.title").with_weight(0.0)];
        let result = service.register_fts_property_schema_with_entries(
            "Article",
            &entries_zero,
            None,
            &[],
            crate::rebuild_actor::RebuildMode::Eager,
        );
        assert!(result.is_err(), "weight 0.0 must be rejected");
        let err_msg = result.expect_err("weight 0.0 must be rejected").to_string();
        assert!(
            err_msg.contains("weight"),
            "error must mention weight: {err_msg}"
        );

        // weight = 1001.0 must be rejected
        let entries_big = vec![FtsPropertyPathSpec::scalar("$.title").with_weight(1001.0)];
        let result = service.register_fts_property_schema_with_entries(
            "Article",
            &entries_big,
            None,
            &[],
            crate::rebuild_actor::RebuildMode::Eager,
        );
        assert!(result.is_err(), "weight 1001.0 must be rejected");
    }

    #[test]
    fn fts_weight_validation_valid() {
        let (_db, service) = setup();
        let entries = vec![FtsPropertyPathSpec::scalar("$.title").with_weight(10.0)];
        let result = service.register_fts_property_schema_with_entries(
            "Article",
            &entries,
            None,
            &[],
            crate::rebuild_actor::RebuildMode::Eager,
        );
        assert!(
            result.is_ok(),
            "weight 10.0 must be accepted: {:?}",
            result.err()
        );
    }

    // --- B-2: create_or_replace_fts_kind_table tests ---

    #[test]
    fn create_or_replace_creates_multi_column_table() {
        use super::create_or_replace_fts_kind_table;
        let (db, _service) = setup();
        let conn = sqlite::open_connection(db.path()).expect("conn");
        let specs = vec![
            FtsPropertyPathSpec::scalar("$.title"),
            FtsPropertyPathSpec::recursive("$.payload"),
        ];
        create_or_replace_fts_kind_table(
            &conn,
            "Article",
            &specs,
            fathomdb_schema::DEFAULT_FTS_TOKENIZER,
        )
        .expect("create table");

        // Verify table exists and has the expected columns.
        let table = fathomdb_schema::fts_kind_table_name("Article");
        // node_logical_id column
        let count: i64 = conn
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 0, "new table must be empty");

        // Verify columns exist by inserting a row with named columns
        let title_col = fathomdb_schema::fts_column_name("$.title", false);
        let payload_col = fathomdb_schema::fts_column_name("$.payload", true);
        conn.execute(
            &format!(
                "INSERT INTO {table} (node_logical_id, {title_col}, {payload_col}) VALUES ('id1', 'hello', 'world')"
            ),
            [],
        )
        .expect("insert with per-spec columns must succeed");
    }

    #[test]
    fn create_or_replace_drops_and_recreates() {
        use super::create_or_replace_fts_kind_table;
        let (db, _service) = setup();
        let conn = sqlite::open_connection(db.path()).expect("conn");

        // First call: 1 spec
        let specs_v1 = vec![FtsPropertyPathSpec::scalar("$.title")];
        create_or_replace_fts_kind_table(
            &conn,
            "Post",
            &specs_v1,
            fathomdb_schema::DEFAULT_FTS_TOKENIZER,
        )
        .expect("create v1");

        // Second call: 2 specs (different layout)
        let specs_v2 = vec![
            FtsPropertyPathSpec::scalar("$.title"),
            FtsPropertyPathSpec::scalar("$.summary"),
        ];
        create_or_replace_fts_kind_table(
            &conn,
            "Post",
            &specs_v2,
            fathomdb_schema::DEFAULT_FTS_TOKENIZER,
        )
        .expect("create v2");

        // Verify new layout: summary column must exist
        let table = fathomdb_schema::fts_kind_table_name("Post");
        let summary_col = fathomdb_schema::fts_column_name("$.summary", false);
        conn.execute(
            &format!("INSERT INTO {table} (node_logical_id, {summary_col}) VALUES ('id1', 'text')"),
            [],
        )
        .expect("second layout must allow summary column");
    }

    #[test]
    fn create_or_replace_invalid_tokenizer() {
        use super::create_or_replace_fts_kind_table;
        let (db, _service) = setup();
        let conn = sqlite::open_connection(db.path()).expect("conn");
        let specs = vec![FtsPropertyPathSpec::scalar("$.title")];
        let result = create_or_replace_fts_kind_table(&conn, "Post", &specs, "'; DROP TABLE --");
        assert!(result.is_err(), "invalid tokenizer must be rejected");
        let err_msg = result
            .expect_err("invalid tokenizer must be rejected")
            .to_string();
        assert!(
            err_msg.contains("tokenizer"),
            "error must mention tokenizer: {err_msg}"
        );
    }

    #[test]
    fn register_with_weights_creates_per_column_table() {
        let (db, service) = setup();
        let entries = vec![
            FtsPropertyPathSpec::scalar("$.title").with_weight(2.0),
            FtsPropertyPathSpec::scalar("$.body"),
        ];
        service
            .register_fts_property_schema_with_entries(
                "Article",
                &entries,
                None,
                &[],
                crate::rebuild_actor::RebuildMode::Eager,
            )
            .expect("register");

        // Per-kind table must have per-spec columns, not just text_content
        let conn = sqlite::open_connection(db.path()).expect("conn");
        let table = fathomdb_schema::fts_kind_table_name("Article");
        let title_col = fathomdb_schema::fts_column_name("$.title", false);
        let body_col = fathomdb_schema::fts_column_name("$.body", false);
        // If the columns exist, insert must succeed
        conn.execute(
            &format!(
                "INSERT INTO {table} (node_logical_id, {title_col}, {body_col}) VALUES ('art-1', 'hello', 'world')"
            ),
            [],
        )
        .expect("per-spec columns must exist after registration with weights");
    }

    #[test]
    fn weighted_to_unweighted_downgrade_recreates_table() {
        let (db, service) = setup();

        // First register with weights (creates per-spec column layout).
        let weighted_entries = vec![
            FtsPropertyPathSpec::scalar("$.title").with_weight(2.0),
            FtsPropertyPathSpec::scalar("$.body"),
        ];
        service
            .register_fts_property_schema_with_entries(
                "Article",
                &weighted_entries,
                None,
                &[],
                crate::rebuild_actor::RebuildMode::Eager,
            )
            .expect("register weighted");

        // Re-register the same kind WITHOUT weights.
        let unweighted_entries = vec![
            FtsPropertyPathSpec::scalar("$.title"),
            FtsPropertyPathSpec::scalar("$.body"),
        ];
        service
            .register_fts_property_schema_with_entries(
                "Article",
                &unweighted_entries,
                None,
                &[],
                crate::rebuild_actor::RebuildMode::Eager,
            )
            .expect("re-register unweighted");

        // After downgrade, the table must have the text_content column
        // (legacy single-column layout), not the per-spec columns.
        let conn = sqlite::open_connection(db.path()).expect("conn");
        let table = fathomdb_schema::fts_kind_table_name("Article");
        let result = conn.execute(
            &format!("INSERT INTO {table} (node_logical_id, text_content) VALUES ('art-1', 'hello world')"),
            [],
        );
        assert!(
            result.is_ok(),
            "text_content column must exist after weighted-to-unweighted downgrade"
        );
    }

    // --- Pack A+G: profile CRUD + tokenizer presets ---

    #[test]
    fn set_get_fts_profile_roundtrip() {
        let (_db, service) = setup();
        let profile = service
            .set_fts_profile("book", "unicode61")
            .expect("set_fts_profile");
        assert_eq!(profile.kind, "book");
        assert_eq!(profile.tokenizer, "unicode61");

        let got = service
            .get_fts_profile("book")
            .expect("get_fts_profile")
            .expect("should be Some");
        assert_eq!(got.kind, "book");
        assert_eq!(got.tokenizer, "unicode61");
    }

    #[test]
    fn fts_profile_upsert() {
        let (_db, service) = setup();
        service
            .set_fts_profile("article", "unicode61")
            .expect("first set");
        service
            .set_fts_profile("article", "porter unicode61 remove_diacritics 2")
            .expect("second set");
        let got = service
            .get_fts_profile("article")
            .expect("get")
            .expect("Some");
        assert_eq!(got.tokenizer, "porter unicode61 remove_diacritics 2");
    }

    #[test]
    fn invalid_tokenizer_rejected() {
        let (_db, service) = setup();
        let result = service.set_fts_profile("book", "'; DROP TABLE nodes --");
        assert!(result.is_err(), "invalid tokenizer must be rejected");
        let msg = result.expect_err("must be Err").to_string();
        assert!(
            msg.contains("tokenizer") || msg.contains("invalid"),
            "error must mention tokenizer or invalid: {msg}"
        );
    }

    #[test]
    fn preset_recall_optimized_english() {
        assert_eq!(
            super::resolve_tokenizer_preset("recall-optimized-english"),
            "porter unicode61 remove_diacritics 2"
        );
    }

    #[test]
    fn preset_precision_optimized() {
        assert_eq!(
            super::resolve_tokenizer_preset("precision-optimized"),
            "unicode61 remove_diacritics 2"
        );
    }

    #[test]
    fn preset_global_cjk() {
        assert_eq!(super::resolve_tokenizer_preset("global-cjk"), "icu");
    }

    #[test]
    fn preset_substring_trigram() {
        assert_eq!(
            super::resolve_tokenizer_preset("substring-trigram"),
            "trigram"
        );
    }

    #[test]
    fn preset_source_code() {
        assert_eq!(
            super::resolve_tokenizer_preset("source-code"),
            "unicode61 tokenchars '._-$@'"
        );
    }

    #[test]
    fn preview_fts_row_count() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            for i in 0..5u32 {
                conn.execute(
                    "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                     VALUES (?1, ?2, 'book', '{}', 100, 'src')",
                    rusqlite::params![format!("r{i}"), format!("lg{i}")],
                )
                .expect("insert node");
            }
            // Insert one superseded node that must NOT count
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref, superseded_at) \
                 VALUES ('r99', 'lg99', 'book', '{}', 100, 'src', 200)",
                [],
            )
            .expect("insert superseded");
        }
        let impact = service
            .preview_projection_impact("book", "fts")
            .expect("preview");
        assert_eq!(impact.rows_to_rebuild, 5);
    }

    #[test]
    fn preview_populates_current_tokenizer() {
        let (_db, service) = setup();
        service
            .set_fts_profile("doc", "trigram")
            .expect("set profile");
        let impact = service
            .preview_projection_impact("doc", "fts")
            .expect("preview");
        assert_eq!(impact.current_tokenizer, Some("trigram".to_owned()));
        assert_eq!(impact.target_tokenizer, None);
    }

    // --- Review fix: tokenizer allowlist alignment ---

    #[test]
    fn create_or_replace_source_code_tokenizer_is_accepted() {
        // The source-code preset expands to "unicode61 tokenchars '._-$@'" which
        // contains `.`, `-`, `$`, `@`. The allowlist in create_or_replace_fts_kind_table
        // must accept these characters (matching set_fts_profile's allowlist).
        use super::create_or_replace_fts_kind_table;
        let (db, _service) = setup();
        let conn = sqlite::open_connection(db.path()).expect("conn");
        let specs = vec![FtsPropertyPathSpec::scalar("$.symbol")];
        let source_code_tokenizer = "unicode61 tokenchars '._-$@'";
        let result =
            create_or_replace_fts_kind_table(&conn, "Symbol", &specs, source_code_tokenizer);
        assert!(
            result.is_ok(),
            "source-code tokenizer string must be accepted by create_or_replace_fts_kind_table: {:?}",
            result.err()
        );
    }

    #[test]
    fn source_code_profile_round_trip_through_register_fts_schema() {
        // Verify that set_fts_profile("source-code") followed by
        // register_fts_property_schema succeeds end-to-end.
        // Previously failed because set_fts_profile accepted "unicode61 tokenchars '._-$@'"
        // but create_or_replace_fts_kind_table rejected it (only allowed " '_").
        let db = tempfile::NamedTempFile::new().expect("temp file");
        let schema = Arc::new(fathomdb_schema::SchemaManager::new());

        // Bootstrap the schema (creates projection_profiles table via migration 20).
        {
            let _coord = crate::ExecutionCoordinator::open(
                db.path(),
                Arc::clone(&schema),
                None,
                1,
                Arc::new(crate::TelemetryCounters::default()),
                None,
            )
            .expect("coordinator opens for bootstrap");
        }

        let service = AdminService::new(db.path(), Arc::clone(&schema));

        // Set source-code profile (uses preset resolver, stores "unicode61 tokenchars '._-$@'").
        service
            .set_fts_profile("Symbol", "source-code")
            .expect("set_fts_profile with source-code preset must succeed");

        // Register an FTS schema for this kind — this calls create_or_replace_fts_kind_table
        // with the tokenizer from the profile row.
        let result = service.register_fts_property_schema("Symbol", &["$.name".to_owned()], None);
        assert!(
            result.is_ok(),
            "register_fts_property_schema must succeed when source-code profile is active: {:?}",
            result.err()
        );
    }

    // --- 0.5.0 item 5: max_tokens() capacity ---

    /// A stub embedder with `max_tokens=8192` can embed a pre-written chunk
    /// whose text exceeds 512 words without error. Verifies that `max_tokens()`
    /// advertises the correct capacity and that `regenerate_vector_embeddings`
    /// produces one vector row for one stored chunk, regardless of chunk length.
    /// (The engine does not re-chunk at regen time; splitting is the caller's
    /// responsibility at write time.)
    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn embedder_max_tokens_8192_handles_chunk_exceeding_512_words() {
        let long_text = (0..600u32)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");

        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());

        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            schema.bootstrap(&conn).expect("bootstrap");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'doc-1', 'Document', '{}', 100, 'src-1')",
                [],
            )
            .expect("insert node");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES (?1, 'doc-1', ?2, 100)",
                rusqlite::params!["chunk-long", long_text],
            )
            .expect("insert long chunk");
        }

        let embedder = LargeContextTestEmbedder::new("long-context-model", 4, 8192);
        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let report = service
            .regenerate_vector_embeddings(
                &embedder,
                &VectorRegenerationConfig {
                    kind: "Document".to_owned(),
                    profile: "default".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                },
            )
            .expect("regenerate with long-context embedder");

        assert_eq!(
            report.total_chunks, 1,
            "600-word text pre-written as one chunk must result in exactly one embedded row"
        );
        assert_eq!(report.regenerated_rows, 1);
        assert_eq!(
            embedder.max_tokens(),
            8192,
            "embedder must advertise 8192 token capacity"
        );
    }

    /// Stub embedder with a configurable `max_tokens` for long-context tests.
    #[cfg(feature = "sqlite-vec")]
    #[derive(Debug)]
    struct LargeContextTestEmbedder {
        identity: QueryEmbedderIdentity,
        vector: Vec<f32>,
        max_tokens: usize,
    }

    #[cfg(feature = "sqlite-vec")]
    impl LargeContextTestEmbedder {
        fn new(model: &str, dimension: usize, max_tokens: usize) -> Self {
            Self {
                identity: QueryEmbedderIdentity {
                    model_identity: model.to_owned(),
                    model_version: "1.0.0".to_owned(),
                    dimension,
                    normalization_policy: "l2".to_owned(),
                },
                vector: vec![1.0; dimension],
                max_tokens,
            }
        }
    }

    #[cfg(feature = "sqlite-vec")]
    impl QueryEmbedder for LargeContextTestEmbedder {
        fn embed_query(&self, _text: &str) -> Result<Vec<f32>, EmbedderError> {
            Ok(self.vector.clone())
        }
        fn identity(&self) -> QueryEmbedderIdentity {
            self.identity.clone()
        }
        fn max_tokens(&self) -> usize {
            self.max_tokens
        }
    }

    /// Item 7 integration test: register schema, write nodes, call
    /// `regenerate_vector_embeddings_in_process`, verify contract row and
    /// that vec rows exist for every chunk.
    #[cfg(feature = "sqlite-vec")]
    #[test]
    #[allow(clippy::too_many_lines)]
    fn regenerate_vector_embeddings_in_process_writes_contract_and_vec_rows() {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());

        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            schema.bootstrap(&conn).expect("bootstrap");
            for (row_id, logical_id, created_at, src) in [
                ("r1", "node-1", 100, "src1"),
                ("r2", "node-2", 101, "src2"),
                ("r3", "node-3", 102, "src3"),
            ] {
                conn.execute(
                    "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                     VALUES (?1, ?2, 'Doc', '{}', ?3, ?4)",
                    rusqlite::params![row_id, logical_id, created_at, src],
                )
                .expect("insert node");
            }
            for (chunk_id, node_id, text, created_at) in [
                ("c1", "node-1", "first document text", 100),
                ("c2", "node-2", "second document text", 101),
                ("c3", "node-3", "third document text", 102),
            ] {
                conn.execute(
                    "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![chunk_id, node_id, text, created_at],
                )
                .expect("insert chunk");
            }
        }

        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let embedder = TestEmbedder::new("batch-test-model", 4);
        let config = VectorRegenerationConfig {
            kind: "Doc".to_owned(),
            profile: "default".to_owned(),
            chunking_policy: "per_chunk".to_owned(),
            preprocessing_policy: "trim".to_owned(),
        };
        let report = service
            .regenerate_vector_embeddings_in_process(&embedder, &config)
            .expect("in-process regen must succeed");

        assert_eq!(report.total_chunks, 3);
        assert_eq!(report.regenerated_rows, 3);
        assert!(report.contract_persisted);

        let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
        let vec_count: i64 = conn
            .query_row("SELECT count(*) FROM vec_doc", [], |row| row.get(0))
            .expect("vec_doc count");
        assert_eq!(vec_count, 3, "one vec row per chunk");

        let model_identity: String = conn
            .query_row(
                "SELECT model_identity FROM vector_embedding_contracts WHERE profile = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("contract row");
        assert_eq!(model_identity, "batch-test-model");
    }

    // --- 0.5.0 item 6: per-kind vec regeneration ---

    #[cfg(feature = "sqlite-vec")]
    #[test]
    #[allow(clippy::too_many_lines)]
    fn regenerate_vector_embeddings_targets_per_kind_table() {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());

        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            schema.bootstrap(&conn).expect("bootstrap");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('row-1', 'doc-1', 'Document', '{}', 100, 'source-1')",
                [],
            )
            .expect("insert node");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'doc-1', 'budget discussion', 100)",
                [],
            )
            .expect("insert chunk");
        }

        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let embedder = TestEmbedder::new("test-model", 4);
        let report = service
            .regenerate_vector_embeddings(
                &embedder,
                &VectorRegenerationConfig {
                    kind: "Document".to_owned(),
                    profile: "default".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                },
            )
            .expect("regenerate vectors");

        assert_eq!(report.table_name, "vec_document");
        assert_eq!(report.regenerated_rows, 1);

        let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
        let vec_count: i64 = conn
            .query_row("SELECT count(*) FROM vec_document", [], |row| row.get(0))
            .expect("vec_document count");
        assert_eq!(vec_count, 1, "rows must be in vec_document");

        let old_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='vec_nodes_active'",
                [],
                |r| r.get(0),
            )
            .expect("sqlite_master check");
        assert_eq!(
            old_count, 0,
            "vec_nodes_active must NOT be created for per-kind regen"
        );
    }

    // --- 0.5.0 item 6 step 5: get_vec_profile reads per-kind key ---

    #[test]
    fn get_vec_profile_returns_none_when_no_profile_exists() {
        let (db, service) = setup();
        let _ = db;
        let result = service.get_vec_profile("MyKind").expect("should not error");
        assert!(
            result.is_none(),
            "must return None when no profile registered"
        );
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn get_vec_profile_returns_profile_for_registered_kind() {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());
        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            schema.bootstrap(&conn).expect("bootstrap");
            schema
                .ensure_vec_kind_profile(&conn, "MyKind", 128)
                .expect("ensure_vec_kind_profile");
        }

        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let profile = service.get_vec_profile("MyKind").expect("should not error");
        assert!(profile.is_some(), "must return profile after registration");
        assert_eq!(profile.unwrap().dimensions, 128);
    }

    #[test]
    fn get_vec_profile_does_not_return_global_sentinel_row() {
        let (db, service) = setup();
        {
            let conn = sqlite::open_connection(db.path()).expect("conn");
            conn.execute(
                "INSERT INTO projection_profiles (kind, facet, config_json, active_at, created_at) \
                 VALUES ('*', 'vec', '{\"model_identity\":\"old-model\",\"dimensions\":384}', 0, 0)",
                [],
            )
            .expect("insert global sentinel");
        }
        let result = service
            .get_vec_profile("SomeKind")
            .expect("should not error");
        assert!(
            result.is_none(),
            "per-kind query must not return global ('*', 'vec') row"
        );
    }
}
