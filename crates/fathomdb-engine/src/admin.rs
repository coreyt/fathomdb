use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::SyncSender;
use std::time::SystemTime;

use fathomdb_schema::{SchemaError, SchemaManager};
use rusqlite::{DatabaseName, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::rebuild_actor::{RebuildMode, RebuildRequest, RebuildStateRow};

use crate::{
    EngineError, ProjectionRepairReport, ProjectionService,
    embedder::{BatchEmbedder, QueryEmbedder, QueryEmbedderIdentity},
    ids::new_id,
    operational::{
        OperationalCollectionKind, OperationalCollectionRecord, OperationalCompactionReport,
        OperationalCurrentRow, OperationalFilterClause, OperationalFilterField,
        OperationalFilterFieldType, OperationalFilterMode, OperationalFilterValue,
        OperationalHistoryValidationIssue, OperationalHistoryValidationReport,
        OperationalMutationRow, OperationalPurgeReport, OperationalReadReport,
        OperationalReadRequest, OperationalRegisterRequest, OperationalRepairReport,
        OperationalRetentionActionKind, OperationalRetentionPlanItem,
        OperationalRetentionPlanReport, OperationalRetentionRunItem, OperationalRetentionRunReport,
        OperationalSecondaryIndexDefinition, OperationalSecondaryIndexRebuildReport,
        OperationalTraceReport, extract_secondary_index_entries_for_current,
        extract_secondary_index_entries_for_mutation, parse_operational_secondary_indexes_json,
        parse_operational_validation_contract, validate_operational_payload_against_contract,
    },
    projection::ProjectionTarget,
    sqlite,
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
    database_path: PathBuf,
    schema_manager: Arc<SchemaManager>,
    projections: ProjectionService,
    /// Sender side of the rebuild actor's channel.  `None` when the engine
    /// was opened without a rebuild actor (e.g. in tests that use
    /// [`AdminService::new`] directly).
    rebuild_sender: Option<SyncSender<RebuildRequest>>,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct VectorRegenerationConfig {
    pub profile: String,
    pub table_name: String,
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

const CURRENT_VECTOR_CONTRACT_FORMAT_VERSION: i64 = 1;
const MAX_PROFILE_LEN: usize = 128;
const MAX_POLICY_LEN: usize = 128;
const MAX_CONTRACT_JSON_BYTES: usize = 32 * 1024;
const MAX_AUDIT_METADATA_BYTES: usize = 2048;
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

    fn connect(&self) -> Result<rusqlite::Connection, EngineError> {
        #[cfg(feature = "sqlite-vec")]
        let conn = sqlite::open_connection_with_vec(&self.database_path)?;
        #[cfg(not(feature = "sqlite-vec"))]
        let conn = sqlite::open_connection(&self.database_path)?;
        self.schema_manager.bootstrap(&conn)?;
        Ok(conn)
    }

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

    /// Retrieve the global vector embedding profile.
    ///
    /// Returns `None` if no vector profile has been persisted yet.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn get_vec_profile(&self) -> Result<Option<VecProfile>, EngineError> {
        let conn = self.connect()?;
        let result = conn
            .query_row(
                "SELECT \
                   json_extract(config_json, '$.model_identity'), \
                   json_extract(config_json, '$.model_version'), \
                   CAST(json_extract(config_json, '$.dimensions') AS INTEGER), \
                   active_at, \
                   created_at \
                 FROM projection_profiles WHERE kind = '*' AND facet = 'vec'",
                [],
                |row| {
                    Ok(VecProfile {
                        model_identity: row.get(0)?,
                        model_version: row.get(1)?,
                        dimensions: {
                            let d: i64 = row.get(2)?;
                            u32::try_from(d).unwrap_or(0)
                        },
                        active_at: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(result)
    }

    /// Write or update the global vector profile from a JSON identity string.
    ///
    /// This is a private helper called after a successful vector regeneration.
    /// Errors are logged as warnings and not propagated to the caller.
    #[allow(dead_code)]
    fn set_vec_profile_inner(
        conn: &rusqlite::Connection,
        identity_json: &str,
    ) -> Result<VecProfile, rusqlite::Error> {
        conn.execute(
            r"INSERT INTO projection_profiles (kind, facet, config_json, active_at, created_at)
              VALUES ('*', 'vec', ?1, unixepoch(), unixepoch())
              ON CONFLICT(kind, facet) DO UPDATE SET
                  config_json = ?1,
                  active_at   = unixepoch()",
            rusqlite::params![identity_json],
        )?;
        conn.query_row(
            "SELECT \
               json_extract(config_json, '$.model_identity'), \
               json_extract(config_json, '$.model_version'), \
               CAST(json_extract(config_json, '$.dimensions') AS INTEGER), \
               active_at, \
               created_at \
             FROM projection_profiles WHERE kind = '*' AND facet = 'vec'",
            [],
            |row| {
                Ok(VecProfile {
                    model_identity: row.get(0)?,
                    model_version: row.get(1)?,
                    dimensions: {
                        let d: i64 = row.get(2)?;
                        u32::try_from(d).unwrap_or(0)
                    },
                    active_at: row.get(3)?,
                    created_at: row.get(4)?,
                })
            },
        )
    }

    /// Persist or update the global vector profile from a JSON config string.
    ///
    /// `config_json` must be valid JSON with at least a `model_identity`
    /// field and `dimensions`.  The JSON is stored verbatim in the
    /// `projection_profiles` table under `kind='*'`, `facet='vec'`.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database write fails.
    pub fn set_vec_profile(&self, config_json: &str) -> Result<VecProfile, EngineError> {
        let conn = self.connect()?;
        Self::set_vec_profile_inner(&conn, config_json).map_err(EngineError::Sqlite)
    }

    /// Estimate the cost of rebuilding a projection.
    ///
    /// For facet `"fts"`: counts active nodes of `kind`.
    /// For facet `"vec"`: counts all chunks.
    ///
    /// # Errors
    /// Returns [`EngineError`] for unknown facets or database errors.
    pub fn preview_projection_impact(
        &self,
        kind: &str,
        facet: &str,
    ) -> Result<ProjectionImpact, EngineError> {
        let conn = self.connect()?;
        match facet {
            "fts" => {
                let rows: u64 = conn
                    .query_row(
                        "SELECT count(*) FROM nodes WHERE kind = ?1 AND superseded_at IS NULL",
                        rusqlite::params![kind],
                        |row| row.get::<_, i64>(0),
                    )
                    .map(i64::cast_unsigned)?;
                let current_tokenizer = self.get_fts_profile(kind)?.map(|p| p.tokenizer);
                Ok(ProjectionImpact {
                    rows_to_rebuild: rows,
                    estimated_seconds: rows / 5000,
                    temp_db_size_bytes: rows * 200,
                    current_tokenizer,
                    target_tokenizer: None,
                })
            }
            "vec" => {
                let rows: u64 = conn
                    .query_row("SELECT count(*) FROM chunks", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .map(i64::cast_unsigned)?;
                Ok(ProjectionImpact {
                    rows_to_rebuild: rows,
                    estimated_seconds: rows / 100,
                    temp_db_size_bytes: rows * 1536,
                    current_tokenizer: None,
                    target_tokenizer: None,
                })
            }
            other => Err(EngineError::Bridge(format!(
                "unknown projection facet: {other:?}"
            ))),
        }
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

        // Vec stale row detection — degrades to 0 when the vec profile is absent.
        #[cfg(feature = "sqlite-vec")]
        let stale_vec_rows: i64 = match conn.query_row(
            r"
            SELECT count(*) FROM vec_nodes_active v
            WHERE NOT EXISTS (SELECT 1 FROM chunks c WHERE c.id = v.chunk_id)
            ",
            [],
            |row| row.get(0),
        ) {
            Ok(n) => n,
            Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                if msg.contains("vec_nodes_active") || msg.contains("no such module: vec0") =>
            {
                0
            }
            Err(e) => return Err(EngineError::Sqlite(e)),
        };
        #[cfg(not(feature = "sqlite-vec"))]
        let stale_vec_rows: i64 = 0;

        #[cfg(feature = "sqlite-vec")]
        let vec_rows_for_superseded_nodes: i64 = match conn.query_row(
            r"
            SELECT count(*) FROM vec_nodes_active v
            JOIN chunks c ON c.id = v.chunk_id
            WHERE NOT EXISTS (
                SELECT 1 FROM nodes n
                WHERE n.logical_id = c.node_logical_id
            )
            ",
            [],
            |row| row.get(0),
        ) {
            Ok(n) => n,
            Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                if msg.contains("vec_nodes_active") || msg.contains("no such module: vec0") =>
            {
                0
            }
            Err(e) => return Err(EngineError::Sqlite(e)),
        };
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

    /// # Errors
    /// Returns [`EngineError`] if the collection metadata is invalid or the insert fails.
    pub fn register_operational_collection(
        &self,
        request: &OperationalRegisterRequest,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        if request.name.trim().is_empty() {
            return Err(EngineError::InvalidWrite(
                "operational collection name must not be empty".to_owned(),
            ));
        }
        if request.schema_json.is_empty() {
            return Err(EngineError::InvalidWrite(
                "operational collection schema_json must not be empty".to_owned(),
            ));
        }
        if request.retention_json.is_empty() {
            return Err(EngineError::InvalidWrite(
                "operational collection retention_json must not be empty".to_owned(),
            ));
        }
        if request.filter_fields_json.is_empty() {
            return Err(EngineError::InvalidWrite(
                "operational collection filter_fields_json must not be empty".to_owned(),
            ));
        }
        parse_operational_validation_contract(&request.validation_json)
            .map_err(EngineError::InvalidWrite)?;
        parse_operational_secondary_indexes_json(&request.secondary_indexes_json, request.kind)
            .map_err(EngineError::InvalidWrite)?;
        if request.format_version <= 0 {
            return Err(EngineError::InvalidWrite(
                "operational collection format_version must be positive".to_owned(),
            ));
        }
        parse_operational_filter_fields(&request.filter_fields_json)
            .map_err(EngineError::InvalidWrite)?;

        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO operational_collections \
             (name, kind, schema_json, retention_json, filter_fields_json, validation_json, secondary_indexes_json, format_version, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, unixepoch())",
            rusqlite::params![
                request.name.as_str(),
                request.kind.as_str(),
                request.schema_json.as_str(),
                request.retention_json.as_str(),
                request.filter_fields_json.as_str(),
                request.validation_json.as_str(),
                request.secondary_indexes_json.as_str(),
                request.format_version,
            ],
        )?;
        persist_simple_provenance_event(
            &tx,
            "operational_collection_registered",
            request.name.as_str(),
            Some(serde_json::json!({
                "kind": request.kind.as_str(),
                "format_version": request.format_version,
            })),
        )?;
        tx.commit()?;

        self.describe_operational_collection(&request.name)?
            .ok_or_else(|| {
                EngineError::Bridge("registered collection missing after commit".to_owned())
            })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn describe_operational_collection(
        &self,
        name: &str,
    ) -> Result<Option<OperationalCollectionRecord>, EngineError> {
        let conn = self.connect()?;
        load_operational_collection_record(&conn, name)
    }

    /// # Errors
    /// Returns [`EngineError`] if the collection is missing, the filter contract is invalid,
    /// or existing mutation backfill fails.
    pub fn update_operational_collection_filters(
        &self,
        name: &str,
        filter_fields_json: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        if filter_fields_json.is_empty() {
            return Err(EngineError::InvalidWrite(
                "operational collection filter_fields_json must not be empty".to_owned(),
            ));
        }
        let declared_fields = parse_operational_filter_fields(filter_fields_json)
            .map_err(EngineError::InvalidWrite)?;

        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        tx.execute(
            "UPDATE operational_collections SET filter_fields_json = ?2 WHERE name = ?1",
            rusqlite::params![name, filter_fields_json],
        )?;
        tx.execute(
            "DELETE FROM operational_filter_values WHERE collection_name = ?1",
            [name],
        )?;

        let mut mutation_stmt = tx.prepare(
            "SELECT id, payload_json FROM operational_mutations \
             WHERE collection_name = ?1 ORDER BY mutation_order",
        )?;
        let mutations = mutation_stmt
            .query_map([name], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(mutation_stmt);

        let mut insert_filter_value = tx.prepare_cached(
            "INSERT INTO operational_filter_values \
             (mutation_id, collection_name, field_name, string_value, integer_value) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut inserted_values = 0usize;
        for (mutation_id, payload_json) in &mutations {
            for filter_value in
                extract_operational_filter_values(&declared_fields, payload_json.as_str())
            {
                insert_filter_value.execute(rusqlite::params![
                    mutation_id,
                    name,
                    filter_value.field_name,
                    filter_value.string_value,
                    filter_value.integer_value,
                ])?;
                inserted_values += 1;
            }
        }
        drop(insert_filter_value);

        persist_simple_provenance_event(
            &tx,
            "operational_collection_filter_fields_updated",
            name,
            Some(serde_json::json!({
                "field_count": declared_fields.len(),
                "mutations_backfilled": mutations.len(),
                "inserted_filter_values": inserted_values,
            })),
        )?;
        let updated = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::Bridge("operational collection missing after filter update".to_owned())
        })?;
        tx.commit()?;
        Ok(updated)
    }

    /// # Errors
    /// Returns [`EngineError`] if the collection is missing or the validation contract is invalid.
    pub fn update_operational_collection_validation(
        &self,
        name: &str,
        validation_json: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        parse_operational_validation_contract(validation_json)
            .map_err(EngineError::InvalidWrite)?;

        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        tx.execute(
            "UPDATE operational_collections SET validation_json = ?2 WHERE name = ?1",
            rusqlite::params![name, validation_json],
        )?;
        persist_simple_provenance_event(
            &tx,
            "operational_collection_validation_updated",
            name,
            Some(serde_json::json!({
                "has_validation": !validation_json.is_empty(),
            })),
        )?;
        let updated = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::Bridge("operational collection missing after validation update".to_owned())
        })?;
        tx.commit()?;
        Ok(updated)
    }

    /// # Errors
    /// Returns [`EngineError`] if the collection is missing, the contract is invalid,
    /// or derived index rebuild fails.
    pub fn update_operational_collection_secondary_indexes(
        &self,
        name: &str,
        secondary_indexes_json: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        let indexes = parse_operational_secondary_indexes_json(secondary_indexes_json, record.kind)
            .map_err(EngineError::InvalidWrite)?;
        tx.execute(
            "UPDATE operational_collections SET secondary_indexes_json = ?2 WHERE name = ?1",
            rusqlite::params![name, secondary_indexes_json],
        )?;
        let (mutation_entries_rebuilt, current_entries_rebuilt) =
            rebuild_operational_secondary_index_entries(&tx, &record.name, record.kind, &indexes)?;
        persist_simple_provenance_event(
            &tx,
            "operational_collection_secondary_indexes_updated",
            name,
            Some(serde_json::json!({
                "index_count": indexes.len(),
                "mutation_entries_rebuilt": mutation_entries_rebuilt,
                "current_entries_rebuilt": current_entries_rebuilt,
            })),
        )?;
        let updated = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::Bridge(
                "operational collection missing after secondary index update".to_owned(),
            )
        })?;
        tx.commit()?;
        Ok(updated)
    }

    /// # Errors
    /// Returns [`EngineError`] if the collection is missing or rebuild fails.
    pub fn rebuild_operational_secondary_indexes(
        &self,
        name: &str,
    ) -> Result<OperationalSecondaryIndexRebuildReport, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        let indexes =
            parse_operational_secondary_indexes_json(&record.secondary_indexes_json, record.kind)
                .map_err(EngineError::InvalidWrite)?;
        let (mutation_entries_rebuilt, current_entries_rebuilt) =
            rebuild_operational_secondary_index_entries(&tx, &record.name, record.kind, &indexes)?;
        persist_simple_provenance_event(
            &tx,
            "operational_secondary_indexes_rebuilt",
            name,
            Some(serde_json::json!({
                "index_count": indexes.len(),
                "mutation_entries_rebuilt": mutation_entries_rebuilt,
                "current_entries_rebuilt": current_entries_rebuilt,
            })),
        )?;
        tx.commit()?;
        Ok(OperationalSecondaryIndexRebuildReport {
            collection_name: name.to_owned(),
            mutation_entries_rebuilt,
            current_entries_rebuilt,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the collection is missing or its validation contract is invalid.
    pub fn validate_operational_collection_history(
        &self,
        name: &str,
    ) -> Result<OperationalHistoryValidationReport, EngineError> {
        let conn = self.connect()?;
        let record = load_operational_collection_record(&conn, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        let Some(contract) = parse_operational_validation_contract(&record.validation_json)
            .map_err(EngineError::InvalidWrite)?
        else {
            return Err(EngineError::InvalidWrite(format!(
                "operational collection '{name}' has no validation_json configured"
            )));
        };

        let mut stmt = conn.prepare(
            "SELECT id, record_key, op_kind, payload_json FROM operational_mutations \
             WHERE collection_name = ?1 ORDER BY mutation_order",
        )?;
        let rows = stmt
            .query_map([name], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut checked_rows = 0usize;
        let mut issues = Vec::new();
        for (mutation_id, record_key, op_kind, payload_json) in rows {
            if op_kind == "delete" {
                continue;
            }
            checked_rows += 1;
            if let Err(message) =
                validate_operational_payload_against_contract(&contract, payload_json.as_str())
            {
                issues.push(OperationalHistoryValidationIssue {
                    mutation_id,
                    record_key,
                    op_kind,
                    message,
                });
            }
        }

        Ok(OperationalHistoryValidationReport {
            collection_name: name.to_owned(),
            checked_rows,
            invalid_row_count: issues.len(),
            issues,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn disable_operational_collection(
        &self,
        name: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        let changed = if record.disabled_at.is_none() {
            tx.execute(
                "UPDATE operational_collections SET disabled_at = unixepoch() WHERE name = ?1",
                [name],
            )?;
            true
        } else {
            false
        };
        let record = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::Bridge("operational collection missing after disable".to_owned())
        })?;
        persist_simple_provenance_event(
            &tx,
            "operational_collection_disabled",
            name,
            Some(serde_json::json!({
                "disabled_at": record.disabled_at,
                "changed": changed,
            })),
        )?;
        tx.commit()?;
        Ok(record)
    }

    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn compact_operational_collection(
        &self,
        name: &str,
        dry_run: bool,
    ) -> Result<OperationalCompactionReport, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let collection = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        validate_append_only_operational_collection(&collection, "compact")?;
        let (mutation_ids, before_timestamp) =
            operational_compaction_candidates(&tx, &collection.retention_json, name)?;
        if dry_run {
            drop(tx);
            return Ok(OperationalCompactionReport {
                collection_name: name.to_owned(),
                deleted_mutations: mutation_ids.len(),
                dry_run: true,
                before_timestamp,
            });
        }
        let mut delete_stmt =
            tx.prepare_cached("DELETE FROM operational_mutations WHERE id = ?1")?;
        for mutation_id in &mutation_ids {
            delete_stmt.execute([mutation_id.as_str()])?;
        }
        drop(delete_stmt);
        persist_simple_provenance_event(
            &tx,
            "operational_collection_compacted",
            name,
            Some(serde_json::json!({
                "deleted_mutations": mutation_ids.len(),
                "before_timestamp": before_timestamp,
            })),
        )?;
        tx.commit()?;
        Ok(OperationalCompactionReport {
            collection_name: name.to_owned(),
            deleted_mutations: mutation_ids.len(),
            dry_run: false,
            before_timestamp,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn purge_operational_collection(
        &self,
        name: &str,
        before_timestamp: i64,
    ) -> Result<OperationalPurgeReport, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let collection = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        validate_append_only_operational_collection(&collection, "purge")?;
        let deleted_mutations = tx.execute(
            "DELETE FROM operational_mutations WHERE collection_name = ?1 AND created_at < ?2",
            rusqlite::params![name, before_timestamp],
        )?;
        persist_simple_provenance_event(
            &tx,
            "operational_collection_purged",
            name,
            Some(serde_json::json!({
                "deleted_mutations": deleted_mutations,
                "before_timestamp": before_timestamp,
            })),
        )?;
        tx.commit()?;
        Ok(OperationalPurgeReport {
            collection_name: name.to_owned(),
            deleted_mutations,
            before_timestamp,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if collection selection or policy parsing fails.
    pub fn plan_operational_retention(
        &self,
        now_timestamp: i64,
        collection_names: Option<&[String]>,
        max_collections: Option<usize>,
    ) -> Result<OperationalRetentionPlanReport, EngineError> {
        let conn = self.connect()?;
        let records = load_operational_retention_records(&conn, collection_names, max_collections)?;
        let mut items = Vec::with_capacity(records.len());
        for record in records {
            items.push(plan_operational_retention_item(
                &conn,
                &record,
                now_timestamp,
            )?);
        }
        Ok(OperationalRetentionPlanReport {
            planned_at: now_timestamp,
            collections_examined: items.len(),
            items,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if collection selection, policy parsing, or execution fails.
    pub fn run_operational_retention(
        &self,
        now_timestamp: i64,
        collection_names: Option<&[String]>,
        max_collections: Option<usize>,
        dry_run: bool,
    ) -> Result<OperationalRetentionRunReport, EngineError> {
        let mut conn = self.connect()?;
        let records = load_operational_retention_records(&conn, collection_names, max_collections)?;
        let mut items = Vec::with_capacity(records.len());
        let mut collections_acted_on = 0usize;

        for record in records {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let item = run_operational_retention_item(&tx, &record, now_timestamp, dry_run)?;
            if item.deleted_mutations > 0 {
                collections_acted_on += 1;
            }
            if dry_run || item.action_kind == OperationalRetentionActionKind::Noop {
                drop(tx);
            } else {
                tx.commit()?;
            }
            items.push(item);
        }

        Ok(OperationalRetentionRunReport {
            executed_at: now_timestamp,
            collections_examined: items.len(),
            collections_acted_on,
            dry_run,
            items,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn trace_operational_collection(
        &self,
        collection_name: &str,
        record_key: Option<&str>,
    ) -> Result<OperationalTraceReport, EngineError> {
        let conn = self.connect()?;
        ensure_operational_collection_registered(&conn, collection_name)?;
        let mutations = if let Some(record_key) = record_key {
            let mut stmt = conn.prepare(
                "SELECT id, collection_name, record_key, op_kind, payload_json, source_ref, created_at \
                 FROM operational_mutations \
                 WHERE collection_name = ?1 AND record_key = ?2 \
                 ORDER BY mutation_order",
            )?;
            stmt.query_map([collection_name, record_key], map_operational_mutation_row)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, collection_name, record_key, op_kind, payload_json, source_ref, created_at \
                 FROM operational_mutations \
                 WHERE collection_name = ?1 \
                 ORDER BY mutation_order",
            )?;
            stmt.query_map([collection_name], map_operational_mutation_row)?
                .collect::<Result<Vec<_>, _>>()?
        };
        let current_rows = if let Some(record_key) = record_key {
            let mut stmt = conn.prepare(
                "SELECT collection_name, record_key, payload_json, updated_at, last_mutation_id \
                 FROM operational_current \
                 WHERE collection_name = ?1 AND record_key = ?2 \
                 ORDER BY updated_at, record_key",
            )?;
            stmt.query_map([collection_name, record_key], map_operational_current_row)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            let mut stmt = conn.prepare(
                "SELECT collection_name, record_key, payload_json, updated_at, last_mutation_id \
                 FROM operational_current \
                 WHERE collection_name = ?1 \
                 ORDER BY updated_at, record_key",
            )?;
            stmt.query_map([collection_name], map_operational_current_row)?
                .collect::<Result<Vec<_>, _>>()?
        };

        Ok(OperationalTraceReport {
            collection_name: collection_name.to_owned(),
            record_key: record_key.map(str::to_owned),
            mutation_count: mutations.len(),
            current_count: current_rows.len(),
            mutations,
            current_rows,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the collection contract is invalid or the filtered read fails.
    pub fn read_operational_collection(
        &self,
        request: &OperationalReadRequest,
    ) -> Result<OperationalReadReport, EngineError> {
        if request.collection_name.trim().is_empty() {
            return Err(EngineError::InvalidWrite(
                "operational read collection_name must not be empty".to_owned(),
            ));
        }
        if request.filters.is_empty() {
            return Err(EngineError::InvalidWrite(
                "operational read requires at least one filter clause".to_owned(),
            ));
        }

        let conn = self.connect()?;
        let record = load_operational_collection_record(&conn, &request.collection_name)?
            .ok_or_else(|| {
                EngineError::InvalidWrite(format!(
                    "operational collection '{}' is not registered",
                    request.collection_name
                ))
            })?;
        validate_append_only_operational_collection(&record, "read")?;
        let declared_fields = parse_operational_filter_fields(&record.filter_fields_json)
            .map_err(EngineError::InvalidWrite)?;
        let secondary_indexes =
            parse_operational_secondary_indexes_json(&record.secondary_indexes_json, record.kind)
                .map_err(EngineError::InvalidWrite)?;
        let applied_limit = operational_read_limit(request.limit)?;
        let filters = compile_operational_read_filters(&request.filters, &declared_fields)?;
        if let Some(report) = execute_operational_secondary_index_read(
            &conn,
            &request.collection_name,
            &filters,
            &secondary_indexes,
            applied_limit,
        )? {
            return Ok(report);
        }
        execute_operational_filtered_read(&conn, &request.collection_name, &filters, applied_limit)
    }

    /// # Errors
    /// Returns [`EngineError`] if the database query fails or collection validation fails.
    pub fn rebuild_operational_current(
        &self,
        collection_name: Option<&str>,
    ) -> Result<OperationalRepairReport, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let collections = if let Some(name) = collection_name {
            let maybe_kind: Option<String> = tx
                .query_row(
                    "SELECT kind FROM operational_collections WHERE name = ?1",
                    [name],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(kind) = maybe_kind else {
                return Err(EngineError::InvalidWrite(format!(
                    "operational collection '{name}' is not registered"
                )));
            };
            if kind != OperationalCollectionKind::LatestState.as_str() {
                return Err(EngineError::InvalidWrite(format!(
                    "operational collection '{name}' is not latest_state"
                )));
            }
            vec![name.to_owned()]
        } else {
            let mut stmt = tx.prepare(
                "SELECT name FROM operational_collections WHERE kind = 'latest_state' ORDER BY name",
            )?;
            stmt.query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };

        let rebuilt_rows = rebuild_operational_current_rows(&tx, &collections)?;
        for collection in &collections {
            let record = load_operational_collection_record(&tx, collection)?.ok_or_else(|| {
                EngineError::Bridge(format!(
                    "operational collection '{collection}' missing during current rebuild"
                ))
            })?;
            let indexes = parse_operational_secondary_indexes_json(
                &record.secondary_indexes_json,
                record.kind,
            )
            .map_err(EngineError::InvalidWrite)?;
            if !indexes.is_empty() {
                rebuild_operational_secondary_index_entries(
                    &tx,
                    &record.name,
                    record.kind,
                    &indexes,
                )?;
            }
        }

        persist_simple_provenance_event(
            &tx,
            "operational_current_rebuilt",
            collection_name.unwrap_or("*"),
            Some(serde_json::json!({
                "collections_rebuilt": collections.len(),
                "current_rows_rebuilt": rebuilt_rows,
            })),
        )?;
        tx.commit()?;

        Ok(OperationalRepairReport {
            collections_rebuilt: collections.len(),
            current_rows_rebuilt: rebuilt_rows,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails or the projection rebuild fails.
    pub fn rebuild_projections(
        &self,
        target: ProjectionTarget,
    ) -> Result<ProjectionRepairReport, EngineError> {
        self.projections.rebuild_projections(target)
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails or the projection rebuild fails.
    pub fn rebuild_missing_projections(&self) -> Result<ProjectionRepairReport, EngineError> {
        self.projections.rebuild_missing_projections()
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

        persist_simple_provenance_event(
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

        // Always drop and recreate the per-kind FTS table to ensure the schema
        // matches the registered spec layout. This handles weighted-to-unweighted
        // downgrade where a stale per-spec table would otherwise remain.
        let any_weight = entries.iter().any(|e| e.weight.is_some());
        let tok = fathomdb_schema::resolve_fts_tokenizer(&tx, kind)
            .map_err(|e| EngineError::Bridge(e.to_string()))?;
        if any_weight {
            create_or_replace_fts_kind_table(&tx, kind, entries, &tok)?;
        } else {
            // Legacy text_content layout — pass empty specs so
            // create_or_replace_fts_kind_table uses the single text_content column.
            create_or_replace_fts_kind_table(&tx, kind, &[], &tok)?;
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

        persist_simple_provenance_event(
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
    ) -> Result<Option<RebuildStateRow>, EngineError> {
        let conn = self.connect()?;
        let row = conn
            .query_row(
                "SELECT kind, schema_id, state, rows_total, rows_done, \
                 started_at, is_first_registration, error_message \
                 FROM fts_property_rebuild_state WHERE kind = ?1",
                rusqlite::params![kind],
                |r| {
                    Ok(RebuildStateRow {
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
        persist_simple_provenance_event(&tx, "fts_property_schema_removed", kind, None)?;
        tx.commit()?;
        Ok(())
    }

    /// Recreate enabled vector profiles from persisted `vector_profiles` metadata.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, reading metadata fails,
    /// or sqlite-vec support is unavailable while enabled profiles are present.
    pub fn restore_vector_profiles(&self) -> Result<ProjectionRepairReport, EngineError> {
        let conn = self.connect()?;
        let profiles: Vec<(String, String, i64)> = {
            let mut stmt = conn.prepare(
                "SELECT profile, table_name, dimension \
                 FROM vector_profiles WHERE enabled = 1 ORDER BY profile",
            )?;
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
        };

        for (profile, table_name, dimension) in &profiles {
            let dimension = usize::try_from(*dimension).map_err(|_| {
                EngineError::Bridge(format!("invalid vector profile dimension: {dimension}"))
            })?;
            self.schema_manager
                .ensure_vector_profile(&conn, profile, table_name, dimension)?;
        }

        Ok(ProjectionRepairReport {
            targets: vec![ProjectionTarget::Vec],
            rebuilt_rows: profiles.len(),
            notes: vec![],
        })
    }

    /// Rebuild vector embeddings using an application-supplied regeneration
    /// contract and generator command.
    ///
    /// The config is persisted in `vector_embedding_contracts` so the metadata
    /// required for recovery survives future repair runs.
    ///
    /// Vector identity is stamped from [`QueryEmbedder::identity`] — the
    /// caller supplies the embedder and cannot override its identity. This
    /// makes drift between the read-path and write-path identity stories
    /// structurally impossible.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the config is
    /// invalid, the embedder fails, or the regenerated embeddings are
    /// malformed.
    #[allow(clippy::too_many_lines)]
    pub fn regenerate_vector_embeddings(
        &self,
        embedder: &dyn QueryEmbedder,
        config: &VectorRegenerationConfig,
    ) -> Result<VectorRegenerationReport, EngineError> {
        let conn = self.connect()?;
        let identity = embedder.identity();
        let config = validate_vector_regeneration_config(&conn, config, &identity)
            .map_err(|failure| failure.to_engine_error())?;
        let chunks = collect_regeneration_chunks(&conn)?;
        let payload = build_regeneration_input(&config, &identity, chunks.clone());
        let snapshot_hash = compute_snapshot_hash(&payload)?;
        let audit_metadata = VectorRegenerationAuditMetadata {
            profile: config.profile.clone(),
            model_identity: identity.model_identity.clone(),
            model_version: identity.model_version.clone(),
            chunk_count: chunks.len(),
            snapshot_hash: snapshot_hash.clone(),
            failure_class: None,
        };
        persist_vector_regeneration_event(
            &conn,
            "vector_regeneration_requested",
            &config.profile,
            &audit_metadata,
        )?;
        let notes = vec!["vector embeddings regenerated via configured embedder".to_owned()];

        let mut embedding_map: std::collections::HashMap<String, Vec<u8>> =
            std::collections::HashMap::with_capacity(chunks.len());
        for chunk in &chunks {
            let vector = match embedder.embed_query(&chunk.text_content) {
                Ok(vector) => vector,
                Err(error) => {
                    let failure = VectorRegenerationFailure::new(
                        VectorRegenerationFailureClass::EmbedderFailure,
                        format!("embedder failed for chunk '{}': {error}", chunk.chunk_id),
                    );
                    self.persist_vector_regeneration_failure_best_effort(
                        &config.profile,
                        &audit_metadata,
                        &failure,
                    );
                    return Err(failure.to_engine_error());
                }
            };
            if vector.len() != identity.dimension {
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::InvalidEmbedderOutput,
                    format!(
                        "embedder produced {} values for chunk '{}', expected {}",
                        vector.len(),
                        chunk.chunk_id,
                        identity.dimension
                    ),
                );
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
            if vector.iter().any(|value| !value.is_finite()) {
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::InvalidEmbedderOutput,
                    format!(
                        "embedder returned non-finite values for chunk '{}'",
                        chunk.chunk_id
                    ),
                );
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
            let bytes: Vec<u8> = vector
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect();
            embedding_map.insert(chunk.chunk_id.clone(), bytes);
        }

        let mut conn = conn;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        match self.schema_manager.ensure_vector_profile(
            &tx,
            &config.profile,
            &config.table_name,
            identity.dimension,
        ) {
            Ok(()) => {}
            Err(SchemaError::MissingCapability(message)) => {
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::UnsupportedVecCapability,
                    message,
                );
                drop(tx);
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
            Err(error) => return Err(EngineError::Schema(error)),
        }
        let apply_chunks = collect_regeneration_chunks(&tx)?;
        let apply_payload = build_regeneration_input(&config, &identity, apply_chunks.clone());
        let apply_hash = compute_snapshot_hash(&apply_payload)?;
        if apply_hash != snapshot_hash {
            let failure = VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::SnapshotDrift,
                "chunk snapshot changed during generation; retry".to_owned(),
            );
            drop(tx);
            self.persist_vector_regeneration_failure_best_effort(
                &config.profile,
                &audit_metadata,
                &failure,
            );
            return Err(failure.to_engine_error());
        }
        persist_vector_contract(&tx, &config, &identity, &snapshot_hash)?;
        tx.execute("DELETE FROM vec_nodes_active", [])?;
        let mut stmt = tx
            .prepare_cached("INSERT INTO vec_nodes_active (chunk_id, embedding) VALUES (?1, ?2)")?;
        let mut regenerated_rows = 0usize;
        for chunk in &apply_chunks {
            let Some(embedding) = embedding_map.remove(&chunk.chunk_id) else {
                drop(stmt);
                drop(tx);
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::InvalidEmbedderOutput,
                    format!(
                        "embedder did not produce a vector for chunk '{}'",
                        chunk.chunk_id
                    ),
                );
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            };
            stmt.execute(rusqlite::params![chunk.chunk_id.as_str(), embedding])?;
            regenerated_rows += 1;
        }
        drop(stmt);
        persist_vector_regeneration_event(
            &tx,
            "vector_regeneration_apply",
            &config.profile,
            &audit_metadata,
        )?;
        tx.commit()?;

        Ok(VectorRegenerationReport {
            profile: config.profile.clone(),
            table_name: config.table_name.clone(),
            dimension: identity.dimension,
            total_chunks: chunks.len(),
            regenerated_rows,
            contract_persisted: true,
            notes,
        })
    }

    /// Regenerate vector embeddings in-process using a [`BatchEmbedder`].
    ///
    /// Functionally equivalent to [`regenerate_vector_embeddings`] but uses
    /// `BatchEmbedder::batch_embed` to process all chunks in one call. This
    /// is the intended path for [`BuiltinBgeSmallEmbedder`] — it keeps the
    /// forward pass in-process without requiring an external subprocess.
    ///
    /// The subprocess-based path ([`regenerate_vector_embeddings`]) remains
    /// intact for callers who supply their own generator binary.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the config is
    /// invalid, the embedder fails, or the regenerated embeddings are malformed.
    #[allow(clippy::too_many_lines)]
    pub fn regenerate_vector_embeddings_in_process(
        &self,
        embedder: &dyn BatchEmbedder,
        config: &VectorRegenerationConfig,
    ) -> Result<VectorRegenerationReport, EngineError> {
        let conn = self.connect()?;
        let identity = embedder.identity();
        let config = validate_vector_regeneration_config(&conn, config, &identity)
            .map_err(|failure| failure.to_engine_error())?;
        let chunks = collect_regeneration_chunks(&conn)?;
        let payload = build_regeneration_input(&config, &identity, chunks.clone());
        let snapshot_hash = compute_snapshot_hash(&payload)?;
        let audit_metadata = VectorRegenerationAuditMetadata {
            profile: config.profile.clone(),
            model_identity: identity.model_identity.clone(),
            model_version: identity.model_version.clone(),
            chunk_count: chunks.len(),
            snapshot_hash: snapshot_hash.clone(),
            failure_class: None,
        };
        persist_vector_regeneration_event(
            &conn,
            "vector_regeneration_requested",
            &config.profile,
            &audit_metadata,
        )?;
        let notes = vec!["vector embeddings regenerated via in-process batch embedder".to_owned()];

        // Collect texts and call batch_embed once for all chunks.
        let chunk_texts: Vec<String> = chunks.iter().map(|c| c.text_content.clone()).collect();
        let batch_vectors = match embedder.batch_embed(&chunk_texts) {
            Ok(vecs) => vecs,
            Err(error) => {
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::EmbedderFailure,
                    format!("batch embedder failed: {error}"),
                );
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
        };
        if batch_vectors.len() != chunks.len() {
            let failure = VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::InvalidEmbedderOutput,
                format!(
                    "batch embedder returned {} vectors for {} chunks",
                    batch_vectors.len(),
                    chunks.len()
                ),
            );
            self.persist_vector_regeneration_failure_best_effort(
                &config.profile,
                &audit_metadata,
                &failure,
            );
            return Err(failure.to_engine_error());
        }

        let mut embedding_map: std::collections::HashMap<String, Vec<u8>> =
            std::collections::HashMap::with_capacity(chunks.len());
        for (chunk, vector) in chunks.iter().zip(batch_vectors.into_iter()) {
            if vector.len() != identity.dimension {
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::InvalidEmbedderOutput,
                    format!(
                        "embedder produced {} values for chunk '{}', expected {}",
                        vector.len(),
                        chunk.chunk_id,
                        identity.dimension
                    ),
                );
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
            if vector.iter().any(|value| !value.is_finite()) {
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::InvalidEmbedderOutput,
                    format!(
                        "embedder returned non-finite values for chunk '{}'",
                        chunk.chunk_id
                    ),
                );
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
            let bytes: Vec<u8> = vector
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect();
            embedding_map.insert(chunk.chunk_id.clone(), bytes);
        }

        let mut conn = conn;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        match self.schema_manager.ensure_vector_profile(
            &tx,
            &config.profile,
            &config.table_name,
            identity.dimension,
        ) {
            Ok(()) => {}
            Err(SchemaError::MissingCapability(message)) => {
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::UnsupportedVecCapability,
                    message,
                );
                drop(tx);
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
            Err(error) => return Err(EngineError::Schema(error)),
        }
        let apply_chunks = collect_regeneration_chunks(&tx)?;
        let apply_payload = build_regeneration_input(&config, &identity, apply_chunks.clone());
        let apply_hash = compute_snapshot_hash(&apply_payload)?;
        if apply_hash != snapshot_hash {
            let failure = VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::SnapshotDrift,
                "chunk snapshot changed during generation; retry".to_owned(),
            );
            drop(tx);
            self.persist_vector_regeneration_failure_best_effort(
                &config.profile,
                &audit_metadata,
                &failure,
            );
            return Err(failure.to_engine_error());
        }
        persist_vector_contract(&tx, &config, &identity, &snapshot_hash)?;
        tx.execute("DELETE FROM vec_nodes_active", [])?;
        let mut stmt = tx
            .prepare_cached("INSERT INTO vec_nodes_active (chunk_id, embedding) VALUES (?1, ?2)")?;
        let mut regenerated_rows = 0usize;
        for chunk in &apply_chunks {
            let Some(embedding) = embedding_map.remove(&chunk.chunk_id) else {
                drop(stmt);
                drop(tx);
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::InvalidEmbedderOutput,
                    format!(
                        "embedder did not produce a vector for chunk '{}'",
                        chunk.chunk_id
                    ),
                );
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            };
            stmt.execute(rusqlite::params![chunk.chunk_id.as_str(), embedding])?;
            regenerated_rows += 1;
        }
        drop(stmt);
        persist_vector_regeneration_event(
            &tx,
            "vector_regeneration_apply",
            &config.profile,
            &audit_metadata,
        )?;
        tx.commit()?;

        Ok(VectorRegenerationReport {
            profile: config.profile.clone(),
            table_name: config.table_name.clone(),
            dimension: identity.dimension,
            total_chunks: chunks.len(),
            regenerated_rows,
            contract_persisted: true,
            notes,
        })
    }

    fn persist_vector_regeneration_failure_best_effort(
        &self,
        profile: &str,
        metadata: &VectorRegenerationAuditMetadata,
        failure: &VectorRegenerationFailure,
    ) {
        let Ok(conn) = self.connect() else {
            return;
        };
        let failure_metadata = VectorRegenerationAuditMetadata {
            profile: metadata.profile.clone(),
            model_identity: metadata.model_identity.clone(),
            model_version: metadata.model_version.clone(),
            chunk_count: metadata.chunk_count,
            snapshot_hash: metadata.snapshot_hash.clone(),
            failure_class: Some(failure.failure_class_label().to_owned()),
        };
        let _ = persist_vector_regeneration_event(
            &conn,
            "vector_regeneration_failed",
            profile,
            &failure_metadata,
        );
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails or any SQL query fails.
    pub fn trace_source(&self, source_ref: &str) -> Result<TraceReport, EngineError> {
        let conn = self.connect()?;

        let node_logical_ids = collect_strings(
            &conn,
            "SELECT logical_id FROM nodes WHERE source_ref = ?1 ORDER BY created_at",
            source_ref,
        )?;
        let action_ids = collect_strings(
            &conn,
            "SELECT id FROM actions WHERE source_ref = ?1 ORDER BY created_at",
            source_ref,
        )?;
        let operational_mutation_ids = collect_strings(
            &conn,
            "SELECT id FROM operational_mutations WHERE source_ref = ?1 ORDER BY mutation_order",
            source_ref,
        )?;

        Ok(TraceReport {
            source_ref: source_ref.to_owned(),
            node_rows: count_source_ref(&conn, "nodes", source_ref)?,
            edge_rows: count_source_ref(&conn, "edges", source_ref)?,
            action_rows: count_source_ref(&conn, "actions", source_ref)?,
            operational_mutation_rows: count_source_ref(
                &conn,
                "operational_mutations",
                source_ref,
            )?,
            node_logical_ids,
            action_ids,
            operational_mutation_ids,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the transaction cannot be
    /// started, or lifecycle restoration prerequisites are missing.
    #[allow(clippy::too_many_lines)]
    pub fn restore_logical_id(
        &self,
        logical_id: &str,
    ) -> Result<LogicalRestoreReport, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let active_count: i64 = tx.query_row(
            "SELECT count(*) FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL",
            [logical_id],
            |row| row.get(0),
        )?;
        if active_count > 0 {
            return Ok(LogicalRestoreReport {
                logical_id: logical_id.to_owned(),
                was_noop: true,
                restored_node_rows: 0,
                restored_edge_rows: 0,
                restored_chunk_rows: 0,
                restored_fts_rows: 0,
                restored_property_fts_rows: 0,
                restored_vec_rows: 0,
                skipped_edges: Vec::new(),
                notes: vec!["logical_id already active".to_owned()],
            });
        }

        let restored_node: Option<(String, String)> = tx
            .query_row(
                "SELECT row_id, kind FROM nodes \
                 WHERE logical_id = ?1 AND superseded_at IS NOT NULL \
                 ORDER BY superseded_at DESC, created_at DESC, rowid DESC LIMIT 1",
                [logical_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (restored_node_row_id, restored_kind) = restored_node.ok_or_else(|| {
            EngineError::InvalidWrite(format!("logical_id '{logical_id}' is not retired"))
        })?;

        tx.execute(
            "UPDATE nodes SET superseded_at = NULL WHERE row_id = ?1",
            [restored_node_row_id.as_str()],
        )?;

        let retire_scope: Option<(i64, Option<String>, i64)> = tx
            .query_row(
                "SELECT rowid, source_ref, created_at FROM provenance_events \
                 WHERE event_type = 'node_retire' AND subject = ?1 \
                 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                [logical_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (restored_edge_rows, skipped_edges) = if let Some((
            retire_event_rowid,
            retire_source_ref,
            retire_created_at,
        )) = retire_scope
        {
            restore_validated_edges(
                &tx,
                logical_id,
                retire_source_ref.as_deref(),
                retire_created_at,
                retire_event_rowid,
            )?
        } else {
            (0, Vec::new())
        };

        let restored_chunk_rows: usize = tx
            .query_row(
                "SELECT count(*) FROM chunks WHERE node_logical_id = ?1",
                [logical_id],
                |row| row.get::<_, i64>(0),
            )
            .map(i64_to_usize)?;
        tx.execute(
            "DELETE FROM fts_nodes WHERE node_logical_id = ?1",
            [logical_id],
        )?;
        let restored_fts_rows = tx.execute(
            "INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content) \
             SELECT id, node_logical_id, ?2, text_content \
             FROM chunks WHERE node_logical_id = ?1",
            rusqlite::params![logical_id, restored_kind],
        )?;
        let restored_vec_rows = count_vec_rows_for_logical_id(&tx, logical_id)?;

        // Rebuild property FTS for the restored node.
        // Delete from the per-kind FTS table for this node (if the table exists).
        let table = fathomdb_schema::fts_kind_table_name(&restored_kind);
        let fts_table_exists: bool = tx
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name = ?1 \
                 AND sql LIKE 'CREATE VIRTUAL TABLE%'",
                rusqlite::params![table],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if fts_table_exists {
            tx.execute(
                &format!("DELETE FROM {table} WHERE node_logical_id = ?1"),
                [logical_id],
            )?;
        }
        let restored_property_fts_rows =
            rebuild_single_node_property_fts(&tx, logical_id, &restored_kind)?;

        persist_simple_provenance_event(
            &tx,
            "restore_logical_id",
            logical_id,
            Some(serde_json::json!({
                "restored_node_rows": 1,
                "restored_edge_rows": restored_edge_rows,
                "restored_chunk_rows": restored_chunk_rows,
                "restored_fts_rows": restored_fts_rows,
                "restored_property_fts_rows": restored_property_fts_rows,
                "restored_vec_rows": restored_vec_rows,
            })),
        )?;
        tx.commit()?;

        Ok(LogicalRestoreReport {
            logical_id: logical_id.to_owned(),
            was_noop: false,
            restored_node_rows: 1,
            restored_edge_rows,
            restored_chunk_rows,
            restored_fts_rows,
            restored_property_fts_rows,
            restored_vec_rows,
            skipped_edges,
            notes: Vec::new(),
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the transaction cannot be
    /// started, or the purge mutation fails.
    pub fn purge_logical_id(&self, logical_id: &str) -> Result<LogicalPurgeReport, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let active_count: i64 = tx.query_row(
            "SELECT count(*) FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL",
            [logical_id],
            |row| row.get(0),
        )?;
        if active_count > 0 {
            return Ok(LogicalPurgeReport {
                logical_id: logical_id.to_owned(),
                was_noop: true,
                deleted_node_rows: 0,
                deleted_edge_rows: 0,
                deleted_chunk_rows: 0,
                deleted_fts_rows: 0,
                deleted_vec_rows: 0,
                notes: vec!["logical_id is active; purge skipped".to_owned()],
            });
        }

        let node_rows: i64 = tx.query_row(
            "SELECT count(*) FROM nodes WHERE logical_id = ?1",
            [logical_id],
            |row| row.get(0),
        )?;
        if node_rows == 0 {
            return Err(EngineError::InvalidWrite(format!(
                "logical_id '{logical_id}' does not exist"
            )));
        }

        let deleted_vec_rows = delete_vec_rows_for_logical_id(&tx, logical_id)?;
        let deleted_fts_rows = tx.execute(
            "DELETE FROM fts_nodes WHERE node_logical_id = ?1",
            [logical_id],
        )?;
        let deleted_edge_rows = tx.execute(
            "DELETE FROM edges WHERE source_logical_id = ?1 OR target_logical_id = ?1",
            [logical_id],
        )?;
        let deleted_chunk_rows = tx.execute(
            "DELETE FROM chunks WHERE node_logical_id = ?1",
            [logical_id],
        )?;
        let deleted_node_rows =
            tx.execute("DELETE FROM nodes WHERE logical_id = ?1", [logical_id])?;
        tx.execute(
            "DELETE FROM node_access_metadata WHERE logical_id = ?1",
            [logical_id],
        )?;

        persist_simple_provenance_event(
            &tx,
            "purge_logical_id",
            logical_id,
            Some(serde_json::json!({
                "deleted_node_rows": deleted_node_rows,
                "deleted_edge_rows": deleted_edge_rows,
                "deleted_chunk_rows": deleted_chunk_rows,
                "deleted_fts_rows": deleted_fts_rows,
                "deleted_vec_rows": deleted_vec_rows,
            })),
        )?;
        tx.commit()?;

        Ok(LogicalPurgeReport {
            logical_id: logical_id.to_owned(),
            was_noop: false,
            deleted_node_rows,
            deleted_edge_rows,
            deleted_chunk_rows,
            deleted_fts_rows,
            deleted_vec_rows,
            notes: Vec::new(),
        })
    }

    /// Purge provenance events older than `before_timestamp`.
    ///
    /// By default, `excise` and `purge_logical_id` event types are preserved so that
    /// data-deletion audit trails survive. Pass an explicit
    /// `preserve_event_types` list to override this default.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the transaction
    /// cannot be started, or any SQL statement fails.
    pub fn purge_provenance_events(
        &self,
        before_timestamp: i64,
        options: &ProvenancePurgeOptions,
    ) -> Result<ProvenancePurgeReport, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let preserved_types: Vec<&str> = if options.preserve_event_types.is_empty() {
            vec!["excise", "purge_logical_id"]
        } else {
            options
                .preserve_event_types
                .iter()
                .map(String::as_str)
                .collect()
        };

        // Build the NOT IN clause dynamically based on preserved types.
        let placeholders: String = (0..preserved_types.len())
            .map(|i| format!("?{}", i + 2))
            .collect::<Vec<_>>()
            .join(", ");
        let count_query = format!(
            "SELECT count(*) FROM provenance_events \
             WHERE created_at < ?1 AND event_type NOT IN ({placeholders})"
        );
        let delete_query = format!(
            "DELETE FROM provenance_events WHERE rowid IN (\
             SELECT rowid FROM provenance_events \
             WHERE created_at < ?1 AND event_type NOT IN ({placeholders}) \
             LIMIT 10000)"
        );

        let bind_params = |stmt: &mut rusqlite::Statement<'_>| -> Result<(), rusqlite::Error> {
            stmt.raw_bind_parameter(1, before_timestamp)?;
            for (i, event_type) in preserved_types.iter().enumerate() {
                stmt.raw_bind_parameter(i + 2, *event_type)?;
            }
            Ok(())
        };

        let events_deleted = if options.dry_run {
            let mut stmt = tx.prepare(&count_query)?;
            bind_params(&mut stmt)?;
            stmt.raw_query()
                .next()?
                .map_or(0, |row| row.get::<_, u64>(0).unwrap_or(0))
        } else {
            let mut total_deleted: u64 = 0;
            loop {
                let mut stmt = tx.prepare(&delete_query)?;
                bind_params(&mut stmt)?;
                let deleted = stmt.raw_execute()?;
                if deleted == 0 {
                    break;
                }
                total_deleted += deleted as u64;
            }
            total_deleted
        };

        let total_after: u64 =
            tx.query_row("SELECT count(*) FROM provenance_events", [], |row| {
                row.get(0)
            })?;

        let oldest_remaining: Option<i64> = tx
            .query_row("SELECT MIN(created_at) FROM provenance_events", [], |row| {
                row.get(0)
            })
            .optional()?
            .flatten();

        if !options.dry_run {
            tx.commit()?;
        }

        // In dry_run mode nothing was deleted, so total_after includes the
        // would-be-deleted rows; subtract to get the preserved count.
        let events_preserved = if options.dry_run {
            total_after - events_deleted
        } else {
            total_after
        };

        Ok(ProvenancePurgeReport {
            events_deleted,
            events_preserved,
            oldest_remaining,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the transaction cannot be
    /// started, or any SQL statement fails.
    #[allow(clippy::too_many_lines)]
    pub fn excise_source(&self, source_ref: &str) -> Result<TraceReport, EngineError> {
        let mut conn = self.connect()?;

        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let affected_operational_collections = collect_strings_tx(
            &tx,
            "SELECT DISTINCT m.collection_name \
             FROM operational_mutations m \
             JOIN operational_collections c ON c.name = m.collection_name \
             WHERE m.source_ref = ?1 AND c.kind = 'latest_state' \
             ORDER BY m.collection_name",
            source_ref,
        )?;

        // Collect (row_id, logical_id) for active rows that will be excised.
        let pairs: Vec<(String, String)> = {
            let mut stmt = tx.prepare(
                "SELECT row_id, logical_id FROM nodes \
                 WHERE source_ref = ?1 AND superseded_at IS NULL",
            )?;
            stmt.query_map([source_ref], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
        };
        let affected_logical_ids: Vec<String> = pairs
            .iter()
            .map(|(_, logical_id)| logical_id.clone())
            .collect();

        // Supersede bad rows in all tables.
        tx.execute(
            "UPDATE nodes SET superseded_at = unixepoch() \
             WHERE source_ref = ?1 AND superseded_at IS NULL",
            [source_ref],
        )?;
        tx.execute(
            "UPDATE edges SET superseded_at = unixepoch() \
             WHERE source_ref = ?1 AND superseded_at IS NULL",
            [source_ref],
        )?;
        tx.execute(
            "UPDATE actions SET superseded_at = unixepoch() \
             WHERE source_ref = ?1 AND superseded_at IS NULL",
            [source_ref],
        )?;
        clear_operational_current_rows(&tx, &affected_operational_collections)?;
        tx.execute(
            "DELETE FROM operational_mutations WHERE source_ref = ?1",
            [source_ref],
        )?;
        for logical_id in &affected_logical_ids {
            delete_vec_rows_for_logical_id(&tx, logical_id)?;
            tx.execute(
                "DELETE FROM chunks WHERE node_logical_id = ?1",
                [logical_id.as_str()],
            )?;
        }

        // Restore the most recent prior version for each affected logical_id.
        for (excised_row_id, logical_id) in &pairs {
            let prior: Option<String> = tx
                .query_row(
                    "SELECT row_id FROM nodes \
                     WHERE logical_id = ?1 AND row_id != ?2 \
                     ORDER BY created_at DESC LIMIT 1",
                    [logical_id.as_str(), excised_row_id.as_str()],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(prior_id) = prior {
                tx.execute(
                    "UPDATE nodes SET superseded_at = NULL WHERE row_id = ?1",
                    [prior_id.as_str()],
                )?;
            }
        }

        for logical_id in &affected_logical_ids {
            let has_active_node = tx
                .query_row(
                    "SELECT 1 FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL LIMIT 1",
                    [logical_id.as_str()],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .is_some();
            if !has_active_node {
                tx.execute(
                    "DELETE FROM node_access_metadata WHERE logical_id = ?1",
                    [logical_id.as_str()],
                )?;
            }
        }

        rebuild_operational_current_rows(&tx, &affected_operational_collections)?;

        // Rebuild FTS atomically within the same transaction so readers never
        // observe a post-excise node state with a stale FTS index.
        tx.execute("DELETE FROM fts_nodes", [])?;
        tx.execute(
            r"
            INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
            SELECT c.id, n.logical_id, n.kind, c.text_content
            FROM chunks c
            JOIN nodes n
              ON n.logical_id = c.node_logical_id
             AND n.superseded_at IS NULL
            ",
            [],
        )?;

        // Rebuild property FTS in the same transaction.
        rebuild_property_fts_in_tx(&tx)?;

        // Record the audit event inside the same transaction so the excision and its
        // audit record are committed atomically — no window where the excision is
        // durable but unaudited.
        tx.execute(
            "INSERT INTO provenance_events (id, event_type, subject, source_ref) \
             VALUES (?1, 'excise_source', ?2, ?2)",
            rusqlite::params![new_id(), source_ref],
        )?;

        tx.commit()?;

        self.trace_source(source_ref)
    }

    /// # Errors
    /// Returns [`EngineError`] if the WAL checkpoint fails, the `SQLite` backup fails,
    /// the SHA-256 digest cannot be computed, or the manifest file cannot be written.
    pub fn safe_export(
        &self,
        destination_path: impl AsRef<Path>,
        options: SafeExportOptions,
    ) -> Result<SafeExportManifest, EngineError> {
        let destination_path = destination_path.as_ref();

        // 1. Optionally checkpoint WAL before exporting. This keeps the on-disk file tidy for
        // callers that want a fully checkpointed export, but export correctness does not depend
        // on it because the backup API copies from the live SQLite connection state.
        let conn = self.connect()?;

        if options.force_checkpoint {
            trace_info!("safe_export: wal checkpoint started");
            let (busy, log, checkpointed): (i64, i64, i64) =
                conn.query_row("PRAGMA wal_checkpoint(FULL)", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
            if busy != 0 {
                trace_warn!(
                    busy,
                    log_frames = log,
                    checkpointed_frames = checkpointed,
                    "safe_export: wal checkpoint blocked by active readers"
                );
                return Err(EngineError::Bridge(format!(
                    "WAL checkpoint blocked: {busy} active reader(s) prevented a full checkpoint; \
                     log frames={log}, checkpointed={checkpointed}; \
                     retry export when no readers are active"
                )));
            }
            trace_info!(
                log_frames = log,
                checkpointed_frames = checkpointed,
                "safe_export: wal checkpoint completed"
            );
        }

        let schema_version: u32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM fathom_schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // 2. Export the database through SQLite's online backup API so committed data in the WAL
        // is included even when `force_checkpoint` is false.
        if let Some(parent) = destination_path.parent() {
            fs::create_dir_all(parent)?;
        }
        conn.backup(DatabaseName::Main, destination_path, None)?;

        drop(conn);

        // 2b. Query page_count from the EXPORTED file so the manifest reflects what was
        // actually backed up, not the source (which may have changed between the PRAGMA
        // and the backup call).
        let page_count: u64 = {
            let export_conn = rusqlite::Connection::open_with_flags(
                destination_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            export_conn.query_row("PRAGMA page_count", [], |row| row.get(0))?
        };

        // 3. Compute SHA-256 of the exported file.
        // FIX(review): was fs::read loading entire DB into memory; use streaming hash.
        let sha256 = {
            let mut file = fs::File::open(destination_path)?;
            let mut hasher = Sha256::new();
            io::copy(&mut file, &mut hasher)?;
            format!("{:x}", hasher.finalize())
        };

        // 4. Record when the export was created.
        let exported_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| EngineError::Bridge(format!("system clock error: {e}")))?
            .as_secs();

        let manifest = SafeExportManifest {
            exported_at,
            sha256,
            schema_version,
            protocol_version: EXPORT_PROTOCOL_VERSION,
            page_count,
        };

        // 5. Write manifest alongside the exported file, using Path API for the name.
        let manifest_path = {
            let mut p = destination_path.to_path_buf();
            let stem = p
                .file_name()
                .map(|n| format!("{}.export-manifest.json", n.to_string_lossy()))
                .ok_or_else(|| {
                    EngineError::Bridge("destination path has no filename".to_owned())
                })?;
            p.set_file_name(stem);
            p
        };
        let manifest_json =
            serde_json::to_string(&manifest).map_err(|e| EngineError::Bridge(e.to_string()))?;

        // Atomic manifest write: write to a temp file then rename so readers never
        // observe a partially-written manifest.
        let manifest_tmp = manifest_path.with_extension("json.tmp");
        if let Err(e) = fs::write(&manifest_tmp, &manifest_json)
            .and_then(|()| fs::rename(&manifest_tmp, &manifest_path))
        {
            let _ = fs::remove_file(&manifest_tmp);
            return Err(e.into());
        }

        Ok(manifest)
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct VectorEmbeddingContractRecord {
    profile: String,
    table_name: String,
    model_identity: String,
    model_version: String,
    dimension: usize,
    normalization_policy: String,
    chunking_policy: String,
    preprocessing_policy: String,
    generator_command_json: String,
    applied_at: i64,
    snapshot_hash: String,
    contract_format_version: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct VectorRegenerationInputChunk {
    chunk_id: String,
    node_logical_id: String,
    kind: String,
    text_content: String,
    byte_start: Option<i64>,
    byte_end: Option<i64>,
    source_ref: Option<String>,
    created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct VectorRegenerationInput {
    profile: String,
    table_name: String,
    model_identity: String,
    model_version: String,
    dimension: usize,
    normalization_policy: String,
    chunking_policy: String,
    preprocessing_policy: String,
    chunks: Vec<VectorRegenerationInputChunk>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VectorRegenerationFailureClass {
    InvalidContract,
    EmbedderFailure,
    InvalidEmbedderOutput,
    SnapshotDrift,
    UnsupportedVecCapability,
}

impl VectorRegenerationFailureClass {
    fn label(self) -> &'static str {
        match self {
            Self::InvalidContract => "invalid contract",
            Self::EmbedderFailure => "embedder failure",
            Self::InvalidEmbedderOutput => "invalid embedder output",
            Self::SnapshotDrift => "snapshot drift",
            Self::UnsupportedVecCapability => "unsupported vec capability",
        }
    }

    fn retryable(self) -> bool {
        matches!(self, Self::SnapshotDrift)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VectorRegenerationFailure {
    class: VectorRegenerationFailureClass,
    detail: String,
}

impl VectorRegenerationFailure {
    pub(crate) fn new(class: VectorRegenerationFailureClass, detail: impl Into<String>) -> Self {
        Self {
            class,
            detail: detail.into(),
        }
    }

    fn to_engine_error(&self) -> EngineError {
        let retry_suffix = if self.class.retryable() {
            " [retryable]"
        } else {
            ""
        };
        EngineError::Bridge(format!(
            "vector regeneration {}: {}{}",
            self.class.label(),
            self.detail,
            retry_suffix
        ))
    }

    fn failure_class_label(&self) -> &'static str {
        self.class.label()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct VectorRegenerationAuditMetadata {
    profile: String,
    model_identity: String,
    model_version: String,
    chunk_count: usize,
    snapshot_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure_class: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum OperationalRetentionPolicy {
    KeepAll,
    PurgeBeforeSeconds { max_age_seconds: i64 },
    KeepLast { max_rows: usize },
}

/// # Errors
/// Returns [`EngineError`] if the file cannot be read or the config is invalid.
pub fn load_vector_regeneration_config(
    path: impl AsRef<Path>,
) -> Result<VectorRegenerationConfig, EngineError> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path)?;
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("toml") => {
            toml::from_str(&raw).map_err(|error| EngineError::Bridge(error.to_string()))
        }
        Some("json") | None => {
            serde_json::from_str(&raw).map_err(|error| EngineError::Bridge(error.to_string()))
        }
        Some(other) => Err(EngineError::Bridge(format!(
            "unsupported vector regeneration config extension: {other}"
        ))),
    }
}

fn validate_vector_regeneration_config(
    conn: &rusqlite::Connection,
    config: &VectorRegenerationConfig,
    identity: &QueryEmbedderIdentity,
) -> Result<VectorRegenerationConfig, VectorRegenerationFailure> {
    let profile = validate_bounded_text("profile", &config.profile, MAX_PROFILE_LEN)?;
    let table_name = validate_bounded_text("table_name", &config.table_name, MAX_PROFILE_LEN)?;
    if table_name != "vec_nodes_active" {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!("table_name must be vec_nodes_active, got '{table_name}'"),
        ));
    }
    if identity.dimension == 0 {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            "embedder reports dimension 0".to_owned(),
        ));
    }
    let chunking_policy =
        validate_bounded_text("chunking_policy", &config.chunking_policy, MAX_POLICY_LEN)?;
    let preprocessing_policy = validate_bounded_text(
        "preprocessing_policy",
        &config.preprocessing_policy,
        MAX_POLICY_LEN,
    )?;

    if let Some(existing_dimension) = current_vector_profile_dimension(conn, &profile)?
        && existing_dimension != identity.dimension
    {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!(
                "embedder dimension {} does not match existing vector profile dimension {}",
                identity.dimension, existing_dimension
            ),
        ));
    }

    validate_existing_contract_version(conn, &profile)?;

    let normalized = VectorRegenerationConfig {
        profile,
        table_name,
        chunking_policy,
        preprocessing_policy,
    };
    let serialized = serde_json::to_vec(&normalized).map_err(|error| {
        VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            error.to_string(),
        )
    })?;
    if serialized.len() > MAX_CONTRACT_JSON_BYTES {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!("serialized contract exceeds {MAX_CONTRACT_JSON_BYTES} bytes"),
        ));
    }

    Ok(normalized)
}

#[allow(clippy::cast_possible_wrap)]
fn persist_vector_contract(
    conn: &rusqlite::Connection,
    config: &VectorRegenerationConfig,
    identity: &QueryEmbedderIdentity,
    snapshot_hash: &str,
) -> Result<(), EngineError> {
    conn.execute(
        r"
        INSERT OR REPLACE INTO vector_embedding_contracts (
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
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, unixepoch(), ?10, ?11, unixepoch())
        ",
        rusqlite::params![
            config.profile.as_str(),
            config.table_name.as_str(),
            identity.model_identity.as_str(),
            identity.model_version.as_str(),
            identity.dimension as i64,
            identity.normalization_policy.as_str(),
            config.chunking_policy.as_str(),
            config.preprocessing_policy.as_str(),
            "[]",
            snapshot_hash,
            CURRENT_VECTOR_CONTRACT_FORMAT_VERSION,
        ],
    )?;
    Ok(())
}

fn persist_vector_regeneration_event(
    conn: &rusqlite::Connection,
    event_type: &str,
    subject: &str,
    metadata: &VectorRegenerationAuditMetadata,
) -> Result<(), EngineError> {
    let metadata_json = serialize_audit_metadata(metadata)?;
    conn.execute(
        "INSERT INTO provenance_events (id, event_type, subject, metadata_json) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![new_id(), event_type, subject, metadata_json],
    )?;
    Ok(())
}

fn persist_simple_provenance_event(
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

/// Count property FTS rows whose `text_content` has drifted from the current canonical
/// value computed by `compute_property_fts_text(...)`. This catches:
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
        let mut stmt = conn.prepare(&format!(
            "SELECT fp.node_logical_id, fp.text_content, n.properties \
             FROM {table} fp \
             JOIN nodes n ON n.logical_id = fp.node_logical_id AND n.superseded_at IS NULL \
             WHERE n.kind = ?1"
        ))?;
        let rows = stmt.query_map([kind.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (_logical_id, stored_text, properties_str) = row?;
            let props: serde_json::Value =
                serde_json::from_str(&properties_str).unwrap_or_default();
            let (expected, _positions, _stats) =
                crate::writer::extract_property_fts(&props, schema);
            match expected {
                Some(text) if text == stored_text => {}
                _ => drifted += 1,
            }
        }
    }
    Ok(drifted)
}

/// Rebuild property FTS rows from canonical state within an existing transaction.
fn rebuild_property_fts_in_tx(conn: &rusqlite::Connection) -> Result<usize, EngineError> {
    // Delete from ALL per-kind FTS virtual tables (including orphaned ones without schemas).
    // Filter by sql LIKE 'CREATE VIRTUAL TABLE%' to exclude FTS5 shadow tables.
    let all_per_kind_tables: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'fts_props_%' \
             AND sql LIKE 'CREATE VIRTUAL TABLE%'",
        )?;
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    for table in &all_per_kind_tables {
        conn.execute_batch(&format!("DELETE FROM {table}"))?;
    }
    conn.execute("DELETE FROM fts_node_property_positions", [])?;
    let inserted = crate::projection::insert_property_fts_rows(
        conn,
        "SELECT logical_id, properties FROM nodes WHERE kind = ?1 AND superseded_at IS NULL",
    )?;
    Ok(inserted)
}

/// Rebuild property FTS for a single node. Returns 1 if a row was inserted, 0 otherwise.
/// The caller must delete any existing per-kind FTS row for this node first.
fn rebuild_single_node_property_fts(
    conn: &rusqlite::Connection,
    logical_id: &str,
    kind: &str,
) -> Result<usize, EngineError> {
    let schema: Option<(String, String)> = conn
        .query_row(
            "SELECT property_paths_json, separator FROM fts_property_schemas WHERE kind = ?1",
            [kind],
            |row| {
                let paths_json: String = row.get(0)?;
                let separator: String = row.get(1)?;
                Ok((paths_json, separator))
            },
        )
        .optional()?;
    let Some((paths_json, separator)) = schema else {
        return Ok(0);
    };
    let parsed = crate::writer::parse_property_schema_json(&paths_json, &separator);
    let properties_str: Option<String> = conn
        .query_row(
            "SELECT properties FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL",
            [logical_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(properties_str) = properties_str else {
        return Ok(0);
    };
    let props: serde_json::Value = serde_json::from_str(&properties_str).unwrap_or_default();
    let (text, positions, _stats) = crate::writer::extract_property_fts(&props, &parsed);
    let Some(text) = text else {
        return Ok(0);
    };
    conn.execute(
        "DELETE FROM fts_node_property_positions WHERE node_logical_id = ?1",
        rusqlite::params![logical_id],
    )?;
    let table = fathomdb_schema::fts_kind_table_name(kind);
    let tok = fathomdb_schema::DEFAULT_FTS_TOKENIZER;
    conn.execute_batch(&format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS {table} \
         USING fts5(node_logical_id UNINDEXED, text_content, tokenize = '{tok}')"
    ))?;
    conn.execute(
        &format!("INSERT INTO {table} (node_logical_id, text_content) VALUES (?1, ?2)"),
        rusqlite::params![logical_id, text],
    )?;
    for pos in &positions {
        conn.execute(
            "INSERT INTO fts_node_property_positions \
             (node_logical_id, kind, start_offset, end_offset, leaf_path) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                logical_id,
                kind,
                i64::try_from(pos.start_offset).unwrap_or(i64::MAX),
                i64::try_from(pos.end_offset).unwrap_or(i64::MAX),
                pos.leaf_path,
            ],
        )?;
    }
    Ok(1)
}

fn serialize_property_paths_json(
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

/// Drop and recreate the per-kind FTS5 virtual table with one column per spec.
///
/// The tokenizer string is validated before interpolation into DDL to
/// prevent SQL injection.  If `specs` is empty a single `text_content`
/// column is used (matching the migration-21 baseline shape).
fn create_or_replace_fts_kind_table(
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

fn validate_fts_property_paths(paths: &[String]) -> Result<(), EngineError> {
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

fn load_fts_property_schema_record(
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
fn build_fts_property_schema_record(
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

fn build_regeneration_input(
    config: &VectorRegenerationConfig,
    identity: &QueryEmbedderIdentity,
    chunks: Vec<VectorRegenerationInputChunk>,
) -> VectorRegenerationInput {
    VectorRegenerationInput {
        profile: config.profile.clone(),
        table_name: config.table_name.clone(),
        model_identity: identity.model_identity.clone(),
        model_version: identity.model_version.clone(),
        dimension: identity.dimension,
        normalization_policy: identity.normalization_policy.clone(),
        chunking_policy: config.chunking_policy.clone(),
        preprocessing_policy: config.preprocessing_policy.clone(),
        chunks,
    }
}

fn compute_snapshot_hash(payload: &VectorRegenerationInput) -> Result<String, EngineError> {
    let bytes =
        serde_json::to_vec(payload).map_err(|error| EngineError::Bridge(error.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_regeneration_chunks(
    conn: &rusqlite::Connection,
) -> Result<Vec<VectorRegenerationInputChunk>, EngineError> {
    let mut stmt = conn.prepare(
        r"
        SELECT c.id, c.node_logical_id, n.kind, c.text_content, c.byte_start, c.byte_end, n.source_ref, c.created_at
        FROM chunks c
        JOIN nodes n
          ON n.logical_id = c.node_logical_id
         AND n.superseded_at IS NULL
        ORDER BY c.created_at, c.id
        ",
    )?;
    let chunks = stmt
        .query_map([], |row| {
            Ok(VectorRegenerationInputChunk {
                chunk_id: row.get(0)?,
                node_logical_id: row.get(1)?,
                kind: row.get(2)?,
                text_content: row.get(3)?,
                byte_start: row.get(4)?,
                byte_end: row.get(5)?,
                source_ref: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(chunks)
}

fn validate_bounded_text(
    field: &str,
    value: &str,
    max_len: usize,
) -> Result<String, VectorRegenerationFailure> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!("{field} must not be empty"),
        ));
    }
    if trimmed.len() > max_len {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!("{field} exceeds max length {max_len}"),
        ));
    }
    Ok(trimmed.to_owned())
}

fn current_vector_profile_dimension(
    conn: &rusqlite::Connection,
    profile: &str,
) -> Result<Option<usize>, VectorRegenerationFailure> {
    let dimension: Option<i64> = conn
        .query_row(
            "SELECT dimension FROM vector_profiles WHERE profile = ?1 AND enabled = 1",
            [profile],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::InvalidContract,
                error.to_string(),
            )
        })?;
    dimension
        .map(|value| {
            usize::try_from(value).map_err(|_| {
                VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::InvalidContract,
                    format!("stored vector profile dimension is invalid: {value}"),
                )
            })
        })
        .transpose()
}

fn validate_existing_contract_version(
    conn: &rusqlite::Connection,
    profile: &str,
) -> Result<(), VectorRegenerationFailure> {
    let version: Option<i64> = conn
        .query_row(
            "SELECT contract_format_version FROM vector_embedding_contracts WHERE profile = ?1",
            [profile],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::InvalidContract,
                error.to_string(),
            )
        })?;
    if let Some(version) = version
        && version > CURRENT_VECTOR_CONTRACT_FORMAT_VERSION
    {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!(
                "persisted contract format version {version} is unsupported; supported version is {CURRENT_VECTOR_CONTRACT_FORMAT_VERSION}"
            ),
        ));
    }
    Ok(())
}

fn serialize_audit_metadata(
    metadata: &VectorRegenerationAuditMetadata,
) -> Result<String, EngineError> {
    let json =
        serde_json::to_string(metadata).map_err(|error| EngineError::Bridge(error.to_string()))?;
    if json.len() > MAX_AUDIT_METADATA_BYTES {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!("audit metadata exceeds {MAX_AUDIT_METADATA_BYTES} bytes"),
        )
        .to_engine_error());
    }
    Ok(json)
}

fn count_source_ref(
    conn: &rusqlite::Connection,
    table: &str,
    source_ref: &str,
) -> Result<usize, EngineError> {
    let sql = match table {
        "nodes" => "SELECT count(*) FROM nodes WHERE source_ref = ?1",
        "edges" => "SELECT count(*) FROM edges WHERE source_ref = ?1",
        "actions" => "SELECT count(*) FROM actions WHERE source_ref = ?1",
        "operational_mutations" => {
            "SELECT count(*) FROM operational_mutations WHERE source_ref = ?1"
        }
        other => return Err(EngineError::Bridge(format!("unknown table: {other}"))),
    };
    let count: i64 = conn.query_row(sql, [source_ref], |row| row.get(0))?;
    // FIX(review): was `count as usize` — unsound cast.
    // Chose option (C) here: propagate error since this is a user-facing helper.
    usize::try_from(count)
        .map_err(|_| EngineError::Bridge(format!("count overflow for table {table}: {count}")))
}

fn rebuild_operational_current_rows(
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
        let rows = stmt.query_map([collection], map_operational_mutation_row)?;
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

fn clear_operational_current_rows(
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

fn clear_operational_secondary_index_entries(
    tx: &rusqlite::Transaction<'_>,
    collection_name: &str,
) -> Result<(), EngineError> {
    tx.execute(
        "DELETE FROM operational_secondary_index_entries WHERE collection_name = ?1",
        [collection_name],
    )?;
    Ok(())
}

fn insert_operational_secondary_index_entry(
    tx: &rusqlite::Transaction<'_>,
    collection_name: &str,
    subject_kind: &str,
    mutation_id: &str,
    record_key: &str,
    entry: &crate::operational::OperationalSecondaryIndexEntry,
) -> Result<(), EngineError> {
    tx.execute(
        "INSERT INTO operational_secondary_index_entries \
         (collection_name, index_name, subject_kind, mutation_id, record_key, sort_timestamp, \
          slot1_text, slot1_integer, slot2_text, slot2_integer, slot3_text, slot3_integer) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            collection_name,
            entry.index_name,
            subject_kind,
            mutation_id,
            record_key,
            entry.sort_timestamp,
            entry.slot1_text,
            entry.slot1_integer,
            entry.slot2_text,
            entry.slot2_integer,
            entry.slot3_text,
            entry.slot3_integer,
        ],
    )?;
    Ok(())
}

fn rebuild_operational_secondary_index_entries(
    tx: &rusqlite::Transaction<'_>,
    collection_name: &str,
    collection_kind: OperationalCollectionKind,
    indexes: &[OperationalSecondaryIndexDefinition],
) -> Result<(usize, usize), EngineError> {
    clear_operational_secondary_index_entries(tx, collection_name)?;

    let mut mutation_entries_rebuilt = 0usize;
    if collection_kind == OperationalCollectionKind::AppendOnlyLog {
        let mut stmt = tx.prepare(
            "SELECT id, record_key, payload_json FROM operational_mutations \
             WHERE collection_name = ?1 ORDER BY mutation_order",
        )?;
        let rows = stmt
            .query_map([collection_name], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for (mutation_id, record_key, payload_json) in rows {
            for entry in extract_secondary_index_entries_for_mutation(indexes, &payload_json) {
                insert_operational_secondary_index_entry(
                    tx,
                    collection_name,
                    "mutation",
                    &mutation_id,
                    &record_key,
                    &entry,
                )?;
                mutation_entries_rebuilt += 1;
            }
        }
    }

    let mut current_entries_rebuilt = 0usize;
    if collection_kind == OperationalCollectionKind::LatestState {
        let mut stmt = tx.prepare(
            "SELECT record_key, payload_json, updated_at, last_mutation_id FROM operational_current \
             WHERE collection_name = ?1 ORDER BY updated_at DESC, record_key",
        )?;
        let rows = stmt
            .query_map([collection_name], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for (record_key, payload_json, updated_at, last_mutation_id) in rows {
            for entry in
                extract_secondary_index_entries_for_current(indexes, &payload_json, updated_at)
            {
                insert_operational_secondary_index_entry(
                    tx,
                    collection_name,
                    "current",
                    &last_mutation_id,
                    &record_key,
                    &entry,
                )?;
                current_entries_rebuilt += 1;
            }
        }
    }

    Ok((mutation_entries_rebuilt, current_entries_rebuilt))
}

fn collect_strings_tx(
    tx: &rusqlite::Transaction<'_>,
    sql: &str,
    value: &str,
) -> Result<Vec<String>, EngineError> {
    let mut stmt = tx.prepare(sql)?;
    let rows = stmt.query_map([value], |row| row.get::<_, String>(0))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(EngineError::from)
}

/// Convert a non-negative i64 count to usize, panicking on negative values
/// which would indicate data corruption.
#[allow(clippy::expect_used)]
fn i64_to_usize(val: i64) -> usize {
    usize::try_from(val).expect("count(*) must be non-negative")
}

/// Runs a parameterized query and collects the first column as strings.
///
/// NOTE(review): sql parameter must be a hardcoded query string, never user input.
/// Options: (A) doc comment, (B) whitelist refactor like `count_source_ref`, (C) leave as-is.
/// Chose (A): function is private, only called with hardcoded SQL from `trace_source`.
/// Whitelist refactor not practical — queries have different SELECT/ORDER BY per table.
fn collect_strings(
    conn: &rusqlite::Connection,
    sql: &str,
    param: &str,
) -> Result<Vec<String>, EngineError> {
    let mut stmt = conn.prepare(sql)?;
    let values = stmt
        .query_map([param], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(values)
}

fn collect_edge_logical_ids_for_restore(
    tx: &rusqlite::Transaction<'_>,
    logical_id: &str,
    retire_source_ref: Option<&str>,
    retire_created_at: i64,
    retire_event_rowid: i64,
) -> Result<Vec<String>, EngineError> {
    let mut stmt = tx.prepare(
        "SELECT DISTINCT e.logical_id \
         FROM edges e \
         JOIN provenance_events p \
           ON p.subject = e.logical_id \
          AND p.event_type = 'edge_retire' \
          AND ( \
                p.created_at > ?3 \
                OR (p.created_at = ?3 AND p.rowid >= ?4) \
          ) \
          AND ((?2 IS NULL AND p.source_ref IS NULL) OR p.source_ref = ?2) \
         WHERE e.superseded_at IS NOT NULL \
           AND (e.source_logical_id = ?1 OR e.target_logical_id = ?1) \
           AND NOT EXISTS ( \
                SELECT 1 FROM edges active \
                WHERE active.logical_id = e.logical_id \
                  AND active.superseded_at IS NULL \
           ) \
         ORDER BY e.logical_id",
    )?;
    let edge_ids = stmt
        .query_map(
            rusqlite::params![
                logical_id,
                retire_source_ref,
                retire_created_at,
                retire_event_rowid
            ],
            |row| row.get::<_, String>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(edge_ids)
}

/// Restores edges for a node being restored, skipping any whose counterpart
/// endpoint is not active (e.g. still retired or purged).
fn restore_validated_edges(
    tx: &rusqlite::Transaction<'_>,
    logical_id: &str,
    retire_source_ref: Option<&str>,
    retire_created_at: i64,
    retire_event_rowid: i64,
) -> Result<(usize, Vec<SkippedEdge>), EngineError> {
    let edge_logical_ids = collect_edge_logical_ids_for_restore(
        tx,
        logical_id,
        retire_source_ref,
        retire_created_at,
        retire_event_rowid,
    )?;
    let mut restored = 0usize;
    let mut skipped = Vec::new();
    for edge_logical_id in &edge_logical_ids {
        let edge_detail: Option<(String, String, String)> = tx
            .query_row(
                "SELECT row_id, source_logical_id, target_logical_id FROM edges \
                 WHERE logical_id = ?1 AND superseded_at IS NOT NULL \
                 ORDER BY superseded_at DESC, created_at DESC, rowid DESC LIMIT 1",
                [edge_logical_id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((edge_row_id, source_lid, target_lid)) = edge_detail else {
            continue;
        };
        let other_endpoint = if source_lid == logical_id {
            &target_lid
        } else {
            &source_lid
        };
        let endpoint_active: bool = tx
            .query_row(
                "SELECT 1 FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL LIMIT 1",
                [other_endpoint.as_str()],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if !endpoint_active {
            skipped.push(SkippedEdge {
                edge_logical_id: edge_logical_id.clone(),
                missing_endpoint: other_endpoint.clone(),
            });
            continue;
        }
        restored += tx.execute(
            "UPDATE edges SET superseded_at = NULL WHERE row_id = ?1",
            [edge_row_id.as_str()],
        )?;
    }
    Ok((restored, skipped))
}

#[cfg(feature = "sqlite-vec")]
fn count_vec_rows_for_logical_id(
    tx: &rusqlite::Transaction<'_>,
    logical_id: &str,
) -> Result<usize, EngineError> {
    match tx.query_row(
        "SELECT count(*) FROM vec_nodes_active v \
         JOIN chunks c ON c.id = v.chunk_id \
         WHERE c.node_logical_id = ?1",
        [logical_id],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(count) => Ok(i64_to_usize(count)),
        Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
            if msg.contains("vec_nodes_active") || msg.contains("no such module: vec0") =>
        {
            Ok(0)
        }
        Err(error) => Err(EngineError::Sqlite(error)),
    }
}

#[cfg(not(feature = "sqlite-vec"))]
#[allow(clippy::unnecessary_wraps)]
fn count_vec_rows_for_logical_id(
    _tx: &rusqlite::Transaction<'_>,
    _logical_id: &str,
) -> Result<usize, EngineError> {
    Ok(0)
}

#[cfg(feature = "sqlite-vec")]
fn delete_vec_rows_for_logical_id(
    tx: &rusqlite::Transaction<'_>,
    logical_id: &str,
) -> Result<usize, EngineError> {
    match tx.execute(
        "DELETE FROM vec_nodes_active \
         WHERE chunk_id IN (SELECT id FROM chunks WHERE node_logical_id = ?1)",
        [logical_id],
    ) {
        Ok(count) => Ok(count),
        Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
            if msg.contains("vec_nodes_active") || msg.contains("no such module: vec0") =>
        {
            Ok(0)
        }
        Err(error) => Err(EngineError::Sqlite(error)),
    }
}

#[cfg(not(feature = "sqlite-vec"))]
#[allow(clippy::unnecessary_wraps)]
fn delete_vec_rows_for_logical_id(
    _tx: &rusqlite::Transaction<'_>,
    _logical_id: &str,
) -> Result<usize, EngineError> {
    Ok(0)
}

fn ensure_operational_collection_registered(
    conn: &rusqlite::Connection,
    collection_name: &str,
) -> Result<(), EngineError> {
    if load_operational_collection_record(conn, collection_name)?.is_none() {
        return Err(EngineError::InvalidWrite(format!(
            "operational collection '{collection_name}' is not registered"
        )));
    }
    Ok(())
}

fn load_operational_collection_record(
    conn: &rusqlite::Connection,
    name: &str,
) -> Result<Option<OperationalCollectionRecord>, EngineError> {
    conn.query_row(
        "SELECT name, kind, schema_json, retention_json, filter_fields_json, validation_json, secondary_indexes_json, format_version, created_at, disabled_at \
         FROM operational_collections WHERE name = ?1",
        [name],
        map_operational_collection_row,
    )
    .optional()
    .map_err(EngineError::Sqlite)
}

fn validate_append_only_operational_collection(
    record: &OperationalCollectionRecord,
    operation: &str,
) -> Result<(), EngineError> {
    if record.kind != OperationalCollectionKind::AppendOnlyLog {
        return Err(EngineError::InvalidWrite(format!(
            "operational collection '{}' must be append_only_log to {operation}",
            record.name
        )));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CompiledOperationalReadFilter {
    field: String,
    condition: OperationalReadCondition,
}

#[derive(Clone, Debug)]
struct MatchedAppendOnlySecondaryIndexRead<'a> {
    index_name: &'a str,
    value_filter: &'a CompiledOperationalReadFilter,
    time_range: Option<&'a CompiledOperationalReadFilter>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum OperationalReadCondition {
    ExactString(String),
    ExactInteger(i64),
    Prefix(String),
    Range {
        lower: Option<i64>,
        upper: Option<i64>,
    },
}

fn operational_read_limit(limit: Option<usize>) -> Result<usize, EngineError> {
    let applied_limit = limit.unwrap_or(DEFAULT_OPERATIONAL_READ_LIMIT);
    if applied_limit == 0 {
        return Err(EngineError::InvalidWrite(
            "operational read limit must be greater than zero".to_owned(),
        ));
    }
    Ok(applied_limit.min(MAX_OPERATIONAL_READ_LIMIT))
}

fn parse_operational_filter_fields(
    filter_fields_json: &str,
) -> Result<Vec<OperationalFilterField>, String> {
    let fields: Vec<OperationalFilterField> = serde_json::from_str(filter_fields_json)
        .map_err(|error| format!("invalid filter_fields_json: {error}"))?;
    let mut seen = std::collections::HashSet::new();
    for field in &fields {
        if field.name.trim().is_empty() {
            return Err("filter_fields_json field names must not be empty".to_owned());
        }
        if !seen.insert(field.name.as_str()) {
            return Err(format!(
                "filter_fields_json contains duplicate field '{}'",
                field.name
            ));
        }
        if field.modes.is_empty() {
            return Err(format!(
                "filter_fields_json field '{}' must declare at least one mode",
                field.name
            ));
        }
        if field.modes.contains(&OperationalFilterMode::Prefix)
            && field.field_type != OperationalFilterFieldType::String
        {
            return Err(format!(
                "filter field '{}' only supports prefix for string types",
                field.name
            ));
        }
    }
    Ok(fields)
}

fn compile_operational_read_filters(
    filters: &[OperationalFilterClause],
    declared_fields: &[OperationalFilterField],
) -> Result<Vec<CompiledOperationalReadFilter>, EngineError> {
    let field_map = declared_fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect::<std::collections::HashMap<_, _>>();
    filters
        .iter()
        .map(|filter| match filter {
            OperationalFilterClause::Exact { field, value } => {
                let declared = field_map.get(field.as_str()).ok_or_else(|| {
                    EngineError::InvalidWrite(format!(
                        "operational read filter uses undeclared field '{field}'"
                    ))
                })?;
                if !declared.modes.contains(&OperationalFilterMode::Exact) {
                    return Err(EngineError::InvalidWrite(format!(
                        "operational read field '{field}' does not allow exact filters"
                    )));
                }
                let condition = match (declared.field_type, value) {
                    (OperationalFilterFieldType::String, OperationalFilterValue::String(value)) => {
                        OperationalReadCondition::ExactString(value.clone())
                    }
                    (
                        OperationalFilterFieldType::Integer | OperationalFilterFieldType::Timestamp,
                        OperationalFilterValue::Integer(value),
                    ) => OperationalReadCondition::ExactInteger(*value),
                    _ => {
                        return Err(EngineError::InvalidWrite(format!(
                            "operational read field '{field}' received a value with the wrong type"
                        )));
                    }
                };
                Ok(CompiledOperationalReadFilter {
                    field: field.clone(),
                    condition,
                })
            }
            OperationalFilterClause::Prefix { field, value } => {
                let declared = field_map.get(field.as_str()).ok_or_else(|| {
                    EngineError::InvalidWrite(format!(
                        "operational read filter uses undeclared field '{field}'"
                    ))
                })?;
                if !declared.modes.contains(&OperationalFilterMode::Prefix) {
                    return Err(EngineError::InvalidWrite(format!(
                        "operational read field '{field}' does not allow prefix filters"
                    )));
                }
                if declared.field_type != OperationalFilterFieldType::String {
                    return Err(EngineError::InvalidWrite(format!(
                        "operational read field '{field}' only supports prefix filters for strings"
                    )));
                }
                Ok(CompiledOperationalReadFilter {
                    field: field.clone(),
                    condition: OperationalReadCondition::Prefix(value.clone()),
                })
            }
            OperationalFilterClause::Range {
                field,
                lower,
                upper,
            } => {
                let declared = field_map.get(field.as_str()).ok_or_else(|| {
                    EngineError::InvalidWrite(format!(
                        "operational read filter uses undeclared field '{field}'"
                    ))
                })?;
                if !declared.modes.contains(&OperationalFilterMode::Range) {
                    return Err(EngineError::InvalidWrite(format!(
                        "operational read field '{field}' does not allow range filters"
                    )));
                }
                if !matches!(
                    declared.field_type,
                    OperationalFilterFieldType::Integer | OperationalFilterFieldType::Timestamp
                ) {
                    return Err(EngineError::InvalidWrite(format!(
                        "operational read field '{field}' only supports range filters for integer/timestamp fields"
                    )));
                }
                if lower.is_none() && upper.is_none() {
                    return Err(EngineError::InvalidWrite(format!(
                        "operational read range filter for '{field}' must specify a lower or upper bound"
                    )));
                }
                Ok(CompiledOperationalReadFilter {
                    field: field.clone(),
                    condition: OperationalReadCondition::Range {
                        lower: *lower,
                        upper: *upper,
                    },
                })
            }
        })
        .collect()
}

fn match_append_only_secondary_index_read<'a>(
    filters: &'a [CompiledOperationalReadFilter],
    indexes: &'a [OperationalSecondaryIndexDefinition],
) -> Option<MatchedAppendOnlySecondaryIndexRead<'a>> {
    indexes.iter().find_map(|index| {
        let OperationalSecondaryIndexDefinition::AppendOnlyFieldTime {
            name,
            field,
            value_type,
            time_field,
        } = index
        else {
            return None;
        };
        if !(1..=2).contains(&filters.len()) {
            return None;
        }

        let mut value_filter = None;
        let mut time_range = None;
        for filter in filters {
            if filter.field == *field {
                let supported = matches!(
                    (&filter.condition, value_type),
                    (
                        OperationalReadCondition::ExactString(_)
                            | OperationalReadCondition::Prefix(_),
                        crate::operational::OperationalSecondaryIndexValueType::String
                    ) | (
                        OperationalReadCondition::ExactInteger(_),
                        crate::operational::OperationalSecondaryIndexValueType::Integer
                            | crate::operational::OperationalSecondaryIndexValueType::Timestamp
                    )
                );
                if !supported || value_filter.is_some() {
                    return None;
                }
                value_filter = Some(filter);
                continue;
            }
            if filter.field == *time_field {
                if !matches!(filter.condition, OperationalReadCondition::Range { .. })
                    || time_range.is_some()
                {
                    return None;
                }
                time_range = Some(filter);
                continue;
            }
            return None;
        }

        value_filter.map(|value_filter| MatchedAppendOnlySecondaryIndexRead {
            index_name: name.as_str(),
            value_filter,
            time_range,
        })
    })
}

fn execute_operational_secondary_index_read(
    conn: &rusqlite::Connection,
    collection_name: &str,
    filters: &[CompiledOperationalReadFilter],
    indexes: &[OperationalSecondaryIndexDefinition],
    applied_limit: usize,
) -> Result<Option<OperationalReadReport>, EngineError> {
    use rusqlite::types::Value;

    let Some(matched) = match_append_only_secondary_index_read(filters, indexes) else {
        return Ok(None);
    };

    let mut sql = String::from(
        "SELECT m.id, m.collection_name, m.record_key, m.op_kind, m.payload_json, m.source_ref, m.created_at \
         FROM operational_secondary_index_entries s \
         JOIN operational_mutations m ON m.id = s.mutation_id \
         WHERE s.collection_name = ?1 AND s.index_name = ?2 AND s.subject_kind = 'mutation' ",
    );
    let mut params = vec![
        Value::from(collection_name.to_owned()),
        Value::from(matched.index_name.to_owned()),
    ];

    match &matched.value_filter.condition {
        OperationalReadCondition::ExactString(value) => {
            let _ = write!(sql, "AND s.slot1_text = ?{} ", params.len() + 1);
            params.push(Value::from(value.clone()));
        }
        OperationalReadCondition::Prefix(value) => {
            let _ = write!(sql, "AND s.slot1_text GLOB ?{} ", params.len() + 1);
            params.push(Value::from(glob_prefix_pattern(value)));
        }
        OperationalReadCondition::ExactInteger(value) => {
            let _ = write!(sql, "AND s.slot1_integer = ?{} ", params.len() + 1);
            params.push(Value::from(*value));
        }
        OperationalReadCondition::Range { .. } => return Ok(None),
    }

    if let Some(time_range) = matched.time_range
        && let OperationalReadCondition::Range { lower, upper } = &time_range.condition
    {
        if let Some(lower) = lower {
            let _ = write!(sql, "AND s.sort_timestamp >= ?{} ", params.len() + 1);
            params.push(Value::from(*lower));
        }
        if let Some(upper) = upper {
            let _ = write!(sql, "AND s.sort_timestamp <= ?{} ", params.len() + 1);
            params.push(Value::from(*upper));
        }
    }

    let _ = write!(
        sql,
        "ORDER BY s.sort_timestamp DESC, m.mutation_order DESC LIMIT ?{}",
        params.len() + 1
    );
    params.push(Value::from(i64::try_from(applied_limit + 1).map_err(
        |_| EngineError::Bridge("operational read limit overflow".to_owned()),
    )?));

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt
        .query_map(
            rusqlite::params_from_iter(params),
            map_operational_mutation_row,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let was_limited = rows.len() > applied_limit;
    if was_limited {
        rows.truncate(applied_limit);
    }

    Ok(Some(OperationalReadReport {
        collection_name: collection_name.to_owned(),
        row_count: rows.len(),
        applied_limit,
        was_limited,
        rows,
    }))
}

fn execute_operational_filtered_read(
    conn: &rusqlite::Connection,
    collection_name: &str,
    filters: &[CompiledOperationalReadFilter],
    applied_limit: usize,
) -> Result<OperationalReadReport, EngineError> {
    use rusqlite::types::Value;

    let mut sql = String::from(
        "SELECT m.id, m.collection_name, m.record_key, m.op_kind, m.payload_json, m.source_ref, m.created_at \
         FROM operational_mutations m ",
    );
    let mut params = vec![Value::from(collection_name.to_owned())];
    for (index, filter) in filters.iter().enumerate() {
        let _ = write!(
            sql,
            "JOIN operational_filter_values f{index} \
             ON f{index}.mutation_id = m.id \
            AND f{index}.collection_name = m.collection_name "
        );
        match &filter.condition {
            OperationalReadCondition::ExactString(value) => {
                let _ = write!(
                    sql,
                    "AND f{index}.field_name = ?{} AND f{index}.string_value = ?{} ",
                    params.len() + 1,
                    params.len() + 2
                );
                params.push(Value::from(filter.field.clone()));
                params.push(Value::from(value.clone()));
            }
            OperationalReadCondition::ExactInteger(value) => {
                let _ = write!(
                    sql,
                    "AND f{index}.field_name = ?{} AND f{index}.integer_value = ?{} ",
                    params.len() + 1,
                    params.len() + 2
                );
                params.push(Value::from(filter.field.clone()));
                params.push(Value::from(*value));
            }
            OperationalReadCondition::Prefix(value) => {
                let _ = write!(
                    sql,
                    "AND f{index}.field_name = ?{} AND f{index}.string_value GLOB ?{} ",
                    params.len() + 1,
                    params.len() + 2
                );
                params.push(Value::from(filter.field.clone()));
                params.push(Value::from(glob_prefix_pattern(value)));
            }
            OperationalReadCondition::Range { lower, upper } => {
                let _ = write!(sql, "AND f{index}.field_name = ?{} ", params.len() + 1);
                params.push(Value::from(filter.field.clone()));
                if let Some(lower) = lower {
                    let _ = write!(sql, "AND f{index}.integer_value >= ?{} ", params.len() + 1);
                    params.push(Value::from(*lower));
                }
                if let Some(upper) = upper {
                    let _ = write!(sql, "AND f{index}.integer_value <= ?{} ", params.len() + 1);
                    params.push(Value::from(*upper));
                }
            }
        }
    }
    let _ = write!(
        sql,
        "WHERE m.collection_name = ?1 ORDER BY m.mutation_order DESC LIMIT ?{}",
        params.len() + 1
    );
    params.push(Value::from(i64::try_from(applied_limit + 1).map_err(
        |_| EngineError::Bridge("operational read limit overflow".to_owned()),
    )?));

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt
        .query_map(
            rusqlite::params_from_iter(params),
            map_operational_mutation_row,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let was_limited = rows.len() > applied_limit;
    if was_limited {
        rows.truncate(applied_limit);
    }
    Ok(OperationalReadReport {
        collection_name: collection_name.to_owned(),
        row_count: rows.len(),
        applied_limit,
        was_limited,
        rows,
    })
}

fn glob_prefix_pattern(value: &str) -> String {
    let mut pattern = String::with_capacity(value.len() + 1);
    for ch in value.chars() {
        match ch {
            '*' => pattern.push_str("[*]"),
            '?' => pattern.push_str("[?]"),
            '[' => pattern.push_str("[[]"),
            _ => pattern.push(ch),
        }
    }
    pattern.push('*');
    pattern
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExtractedOperationalFilterValue {
    field_name: String,
    string_value: Option<String>,
    integer_value: Option<i64>,
}

fn extract_operational_filter_values(
    filter_fields: &[OperationalFilterField],
    payload_json: &str,
) -> Vec<ExtractedOperationalFilterValue> {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(payload_json) else {
        return Vec::new();
    };
    let Some(object) = parsed.as_object() else {
        return Vec::new();
    };

    filter_fields
        .iter()
        .filter_map(|field| {
            let value = object.get(&field.name)?;
            match field.field_type {
                OperationalFilterFieldType::String => {
                    value
                        .as_str()
                        .map(|string_value| ExtractedOperationalFilterValue {
                            field_name: field.name.clone(),
                            string_value: Some(string_value.to_owned()),
                            integer_value: None,
                        })
                }
                OperationalFilterFieldType::Integer | OperationalFilterFieldType::Timestamp => {
                    value
                        .as_i64()
                        .map(|integer_value| ExtractedOperationalFilterValue {
                            field_name: field.name.clone(),
                            string_value: None,
                            integer_value: Some(integer_value),
                        })
                }
            }
        })
        .collect()
}

fn operational_compaction_candidates(
    conn: &rusqlite::Connection,
    retention_json: &str,
    collection_name: &str,
) -> Result<(Vec<String>, Option<i64>), EngineError> {
    operational_compaction_candidates_at(
        conn,
        retention_json,
        collection_name,
        current_unix_timestamp()?,
    )
}

fn operational_compaction_candidates_at(
    conn: &rusqlite::Connection,
    retention_json: &str,
    collection_name: &str,
    now_timestamp: i64,
) -> Result<(Vec<String>, Option<i64>), EngineError> {
    let policy = parse_operational_retention_policy(retention_json)?;
    match policy {
        OperationalRetentionPolicy::KeepAll => Ok((Vec::new(), None)),
        OperationalRetentionPolicy::PurgeBeforeSeconds { max_age_seconds } => {
            let before_timestamp = now_timestamp - max_age_seconds;
            let mut stmt = conn.prepare(
                "SELECT id FROM operational_mutations \
                 WHERE collection_name = ?1 AND created_at < ?2 \
                 ORDER BY mutation_order",
            )?;
            let mutation_ids = stmt
                .query_map(
                    rusqlite::params![collection_name, before_timestamp],
                    |row| row.get::<_, String>(0),
                )?
                .collect::<Result<Vec<_>, _>>()?;
            Ok((mutation_ids, Some(before_timestamp)))
        }
        OperationalRetentionPolicy::KeepLast { max_rows } => {
            let mut stmt = conn.prepare(
                "SELECT id FROM operational_mutations \
                 WHERE collection_name = ?1 \
                 ORDER BY mutation_order DESC",
            )?;
            let ordered_ids = stmt
                .query_map([collection_name], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok((ordered_ids.into_iter().skip(max_rows).collect(), None))
        }
    }
}

fn parse_operational_retention_policy(
    retention_json: &str,
) -> Result<OperationalRetentionPolicy, EngineError> {
    let policy: OperationalRetentionPolicy = serde_json::from_str(retention_json)
        .map_err(|error| EngineError::InvalidWrite(format!("invalid retention_json: {error}")))?;
    match policy {
        OperationalRetentionPolicy::KeepAll => Ok(policy),
        OperationalRetentionPolicy::PurgeBeforeSeconds { max_age_seconds } => {
            if max_age_seconds <= 0 {
                return Err(EngineError::InvalidWrite(
                    "retention_json max_age_seconds must be greater than zero".to_owned(),
                ));
            }
            Ok(policy)
        }
        OperationalRetentionPolicy::KeepLast { max_rows } => {
            if max_rows == 0 {
                return Err(EngineError::InvalidWrite(
                    "retention_json max_rows must be greater than zero".to_owned(),
                ));
            }
            Ok(policy)
        }
    }
}

fn load_operational_retention_records(
    conn: &rusqlite::Connection,
    collection_names: Option<&[String]>,
    max_collections: Option<usize>,
) -> Result<Vec<OperationalCollectionRecord>, EngineError> {
    let limit = max_collections.unwrap_or(usize::MAX);
    if limit == 0 {
        return Err(EngineError::InvalidWrite(
            "max_collections must be greater than zero".to_owned(),
        ));
    }

    let mut records = Vec::new();
    if let Some(collection_names) = collection_names {
        for name in collection_names.iter().take(limit) {
            let record = load_operational_collection_record(conn, name)?.ok_or_else(|| {
                EngineError::InvalidWrite(format!(
                    "operational collection '{name}' is not registered"
                ))
            })?;
            records.push(record);
        }
        return Ok(records);
    }

    let mut stmt = conn.prepare(
        "SELECT name, kind, schema_json, retention_json, filter_fields_json, validation_json, secondary_indexes_json, format_version, created_at, disabled_at \
         FROM operational_collections ORDER BY name",
    )?;
    let rows = stmt
        .query_map([], map_operational_collection_row)?
        .take(limit)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn last_operational_retention_run_at(
    conn: &rusqlite::Connection,
    collection_name: &str,
) -> Result<Option<i64>, EngineError> {
    conn.query_row(
        "SELECT MAX(executed_at) FROM operational_retention_runs WHERE collection_name = ?1",
        [collection_name],
        |row| row.get(0),
    )
    .optional()
    .map_err(EngineError::Sqlite)
    .map(Option::flatten)
}

fn count_operational_mutations_for_collection(
    conn: &rusqlite::Connection,
    collection_name: &str,
) -> Result<usize, EngineError> {
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM operational_mutations WHERE collection_name = ?1",
        [collection_name],
        |row| row.get(0),
    )?;
    usize::try_from(count).map_err(|_| {
        EngineError::Bridge(format!("count overflow for collection {collection_name}"))
    })
}

fn retention_action_kind_and_limit(
    policy: &OperationalRetentionPolicy,
) -> (OperationalRetentionActionKind, Option<usize>) {
    match policy {
        OperationalRetentionPolicy::KeepAll => (OperationalRetentionActionKind::Noop, None),
        OperationalRetentionPolicy::PurgeBeforeSeconds { .. } => {
            (OperationalRetentionActionKind::PurgeBeforeSeconds, None)
        }
        OperationalRetentionPolicy::KeepLast { max_rows } => {
            (OperationalRetentionActionKind::KeepLast, Some(*max_rows))
        }
    }
}

fn plan_operational_retention_item(
    conn: &rusqlite::Connection,
    record: &OperationalCollectionRecord,
    now_timestamp: i64,
) -> Result<OperationalRetentionPlanItem, EngineError> {
    let last_run_at = last_operational_retention_run_at(conn, &record.name)?;
    if record.kind != OperationalCollectionKind::AppendOnlyLog {
        return Ok(OperationalRetentionPlanItem {
            collection_name: record.name.clone(),
            action_kind: OperationalRetentionActionKind::Noop,
            candidate_deletions: 0,
            before_timestamp: None,
            max_rows: None,
            last_run_at,
        });
    }
    let policy = parse_operational_retention_policy(&record.retention_json)?;
    let (action_kind, max_rows) = retention_action_kind_and_limit(&policy);
    let (candidate_ids, before_timestamp) = operational_compaction_candidates_at(
        conn,
        &record.retention_json,
        &record.name,
        now_timestamp,
    )?;
    Ok(OperationalRetentionPlanItem {
        collection_name: record.name.clone(),
        action_kind,
        candidate_deletions: candidate_ids.len(),
        before_timestamp,
        max_rows,
        last_run_at,
    })
}

fn run_operational_retention_item(
    tx: &rusqlite::Transaction<'_>,
    record: &OperationalCollectionRecord,
    now_timestamp: i64,
    dry_run: bool,
) -> Result<OperationalRetentionRunItem, EngineError> {
    let plan = plan_operational_retention_item(tx, record, now_timestamp)?;
    let mut deleted_mutations = 0usize;
    if record.kind == OperationalCollectionKind::AppendOnlyLog
        && plan.action_kind != OperationalRetentionActionKind::Noop
        && plan.candidate_deletions > 0
        && !dry_run
    {
        let (candidate_ids, _) = operational_compaction_candidates_at(
            tx,
            &record.retention_json,
            &record.name,
            now_timestamp,
        )?;
        let mut delete_stmt =
            tx.prepare_cached("DELETE FROM operational_mutations WHERE id = ?1")?;
        for mutation_id in &candidate_ids {
            delete_stmt.execute([mutation_id.as_str()])?;
            deleted_mutations += 1;
        }
        drop(delete_stmt);

        persist_simple_provenance_event(
            tx,
            "operational_retention_run",
            &record.name,
            Some(serde_json::json!({
                "action_kind": plan.action_kind,
                "deleted_mutations": deleted_mutations,
                "before_timestamp": plan.before_timestamp,
                "max_rows": plan.max_rows,
                "executed_at": now_timestamp,
            })),
        )?;
    }

    let live_rows_remaining = count_operational_mutations_for_collection(tx, &record.name)?;
    let effective_deleted_mutations = if dry_run {
        plan.candidate_deletions
    } else {
        deleted_mutations
    };
    let rows_remaining = if dry_run {
        live_rows_remaining.saturating_sub(effective_deleted_mutations)
    } else {
        live_rows_remaining
    };
    if !dry_run && plan.action_kind != OperationalRetentionActionKind::Noop {
        tx.execute(
            "INSERT INTO operational_retention_runs \
             (id, collection_name, executed_at, action_kind, dry_run, deleted_mutations, rows_remaining, metadata_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                new_id(),
                record.name,
                now_timestamp,
                serde_json::to_string(&plan.action_kind)
                    .unwrap_or_else(|_| "\"noop\"".to_owned())
                    .trim_matches('"')
                    .to_owned(),
                i32::from(dry_run),
                deleted_mutations,
                rows_remaining,
                serde_json::json!({
                    "before_timestamp": plan.before_timestamp,
                    "max_rows": plan.max_rows,
                })
                .to_string(),
            ],
        )?;
    }

    Ok(OperationalRetentionRunItem {
        collection_name: plan.collection_name,
        action_kind: plan.action_kind,
        deleted_mutations: effective_deleted_mutations,
        before_timestamp: plan.before_timestamp,
        max_rows: plan.max_rows,
        rows_remaining,
    })
}

fn current_unix_timestamp() -> Result<i64, EngineError> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|error| EngineError::Bridge(format!("system clock error: {error}")))?;
    i64::try_from(now.as_secs())
        .map_err(|_| EngineError::Bridge("unix timestamp overflow".to_owned()))
}

fn map_operational_collection_row(
    row: &rusqlite::Row<'_>,
) -> Result<OperationalCollectionRecord, rusqlite::Error> {
    let kind_text: String = row.get(1)?;
    let kind = OperationalCollectionKind::try_from(kind_text.as_str()).map_err(|message| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(io::Error::new(io::ErrorKind::InvalidData, message)),
        )
    })?;
    Ok(OperationalCollectionRecord {
        name: row.get(0)?,
        kind,
        schema_json: row.get(2)?,
        retention_json: row.get(3)?,
        filter_fields_json: row.get(4)?,
        validation_json: row.get(5)?,
        secondary_indexes_json: row.get(6)?,
        format_version: row.get(7)?,
        created_at: row.get(8)?,
        disabled_at: row.get(9)?,
    })
}

fn map_operational_mutation_row(
    row: &rusqlite::Row<'_>,
) -> Result<OperationalMutationRow, rusqlite::Error> {
    Ok(OperationalMutationRow {
        id: row.get(0)?,
        collection_name: row.get(1)?,
        record_key: row.get(2)?,
        op_kind: row.get(3)?,
        payload_json: row.get(4)?,
        source_ref: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn map_operational_current_row(
    row: &rusqlite::Row<'_>,
) -> Result<OperationalCurrentRow, rusqlite::Error> {
    Ok(OperationalCurrentRow {
        collection_name: row.get(0)?,
        record_key: row.get(1)?,
        payload_json: row.get(2)?,
        updated_at: row.get(3)?,
        last_mutation_id: row.get(4)?,
    })
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
                .ensure_vector_profile(&conn, "default", "vec_nodes_active", 4)
                .expect("ensure vec profile");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('chunk-1', 'ghost-doc', 'budget narrative', 100)",
                [],
            )
            .expect("insert orphaned chunk");
            conn.execute(
                "INSERT INTO vec_nodes_active (chunk_id, embedding) VALUES ('chunk-1', zeroblob(16))",
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
                .ensure_vector_profile(&conn, "default", "vec_nodes_active", 4)
                .expect("ensure vec profile");
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
                "INSERT INTO vec_nodes_active (chunk_id, embedding) VALUES ('chunk-1', zeroblob(16))",
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
                .ensure_vector_profile(&conn, "default", "vec_nodes_active", 4)
                .expect("ensure vec profile");
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
                "INSERT INTO vec_nodes_active (chunk_id, embedding) VALUES ('chunk-1', zeroblob(16))",
                [],
            )
            .expect("insert vec row");
        }

        let report = service.purge_logical_id("doc-1").expect("purge");
        assert_eq!(report.deleted_vec_rows, 1);

        let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
        let vec_count: i64 = conn
            .query_row("SELECT count(*) FROM vec_nodes_active", [], |row| {
                row.get(0)
            })
            .expect("vec count");
        assert_eq!(vec_count, 0);
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn restore_logical_id_restores_visibility_of_regenerated_vectors() {
        let (db, service) = setup();

        {
            let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
            service
                .schema_manager
                .ensure_vector_profile(&conn, "default", "vec_nodes_active", 4)
                .expect("ensure vec profile");
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
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
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
                .ensure_vector_profile(&conn, "default", "vec_nodes_active", 3)
                .expect("vec profile");
            // Insert a vec row whose chunk does not exist.
            let bytes: Vec<u8> = [0.1f32, 0.2f32, 0.3f32]
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();
            conn.execute(
                "INSERT INTO vec_nodes_active (chunk_id, embedding) VALUES ('ghost-chunk', ?1)",
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
            profile: "default".to_owned(),
            table_name: "vec_nodes_active".to_owned(),
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
        let legacy_json = r#"{
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
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
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
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                },
            )
            .expect("regenerate vectors");

        assert_eq!(report.profile, "default");
        assert_eq!(report.table_name, "vec_nodes_active");
        assert_eq!(report.dimension, 4);
        assert_eq!(report.total_chunks, 2);
        assert_eq!(report.regenerated_rows, 2);
        assert!(report.contract_persisted);

        let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
        let vec_count: i64 = conn
            .query_row("SELECT count(*) FROM vec_nodes_active", [], |row| {
                row.get(0)
            })
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
                .ensure_vector_profile(&conn, "default", "vec_nodes_active", 4)
                .expect("ensure vec profile");
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
                    "vec_nodes_active",
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
                "INSERT INTO vec_nodes_active (chunk_id, embedding) VALUES ('chunk-1', zeroblob(16))",
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
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
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
            .query_row("SELECT count(*) FROM vec_nodes_active", [], |row| {
                row.get(0)
            })
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
                    profile: "   ".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
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
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
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
                .ensure_vector_profile(&conn, "default", "vec_nodes_active", 4)
                .expect("ensure vec profile");
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
                "INSERT INTO vec_nodes_active (chunk_id, embedding) VALUES ('ck1', zeroblob(16))",
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
            .query_row("SELECT count(*) FROM vec_nodes_active", [], |row| {
                row.get(0)
            })
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

    /// Item 5 integration test: a stub embedder with `max_tokens=8192` can
    /// process a single chunk whose text exceeds 512 words. The pre-written
    /// chunk is stored as one unit; `regenerate_vector_embeddings` embeds it
    /// as one row, not two.
    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn max_tokens_8192_embedder_processes_long_chunk_as_single_unit() {
        // Build a text with ~600 words — exceeds 512 but fits within 8192.
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

        // Embedder with max_tokens=8192 — should handle the 600-word chunk.
        let embedder = LargeContextTestEmbedder::new("long-context-model", 4, 8192);
        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let report = service
            .regenerate_vector_embeddings(
                &embedder,
                &VectorRegenerationConfig {
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
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
    #[derive(Debug)]
    struct LargeContextTestEmbedder {
        identity: QueryEmbedderIdentity,
        vector: Vec<f32>,
        max_tokens: usize,
    }

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
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r1', 'node-1', 'Doc', '{}', 100, 'src1')",
                [],
            )
            .expect("insert node 1");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r2', 'node-2', 'Doc', '{}', 101, 'src2')",
                [],
            )
            .expect("insert node 2");
            conn.execute(
                "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
                 VALUES ('r3', 'node-3', 'Doc', '{}', 102, 'src3')",
                [],
            )
            .expect("insert node 3");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('c1', 'node-1', 'first document text', 100)",
                [],
            )
            .expect("insert chunk 1");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('c2', 'node-2', 'second document text', 101)",
                [],
            )
            .expect("insert chunk 2");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) \
                 VALUES ('c3', 'node-3', 'third document text', 102)",
                [],
            )
            .expect("insert chunk 3");
        }

        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let embedder = TestEmbedder::new("batch-test-model", 4);
        let config = VectorRegenerationConfig {
            profile: "default".to_owned(),
            table_name: "vec_nodes_active".to_owned(),
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
            .query_row("SELECT count(*) FROM vec_nodes_active", [], |row| {
                row.get(0)
            })
            .expect("vec count");
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
}
