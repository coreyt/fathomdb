use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use fathomdb_schema::{SchemaError, SchemaManager};
use rusqlite::{DatabaseName, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    EngineError, ProjectionRepairReport, ProjectionService, executable_trust,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IntegrityReport {
    pub physical_ok: bool,
    pub foreign_keys_ok: bool,
    pub missing_fts_rows: usize,
    pub duplicate_active_logical_ids: usize,
    pub operational_missing_collections: usize,
    pub operational_missing_last_mutations: usize,
    pub warnings: Vec<String>,
}

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LogicalRestoreReport {
    pub logical_id: String,
    pub was_noop: bool,
    pub restored_node_rows: usize,
    pub restored_edge_rows: usize,
    pub restored_chunk_rows: usize,
    pub restored_fts_rows: usize,
    pub restored_vec_rows: usize,
    pub notes: Vec<String>,
}

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

#[derive(Debug)]
pub struct AdminService {
    database_path: PathBuf,
    schema_manager: Arc<SchemaManager>,
    projections: ProjectionService,
}

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
    /// Access metadata rows whose logical_id no longer has any node history.
    pub orphaned_last_access_metadata_rows: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VectorRegenerationConfig {
    pub profile: String,
    pub table_name: String,
    pub model_identity: String,
    pub model_version: String,
    pub dimension: usize,
    pub normalization_policy: String,
    pub chunking_policy: String,
    pub preprocessing_policy: String,
    pub generator_command: Vec<String>,
}

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VectorGeneratorPolicy {
    pub timeout_ms: u64,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
    pub max_input_bytes: usize,
    pub max_chunks: usize,
    #[serde(default = "default_require_absolute_executable")]
    pub require_absolute_executable: bool,
    #[serde(default = "default_reject_world_writable_executable")]
    pub reject_world_writable_executable: bool,
    #[serde(default)]
    pub allowed_executable_roots: Vec<String>,
    #[serde(default)]
    pub preserve_env_vars: Vec<String>,
}

impl Default for VectorGeneratorPolicy {
    fn default() -> Self {
        Self {
            timeout_ms: 300_000,
            max_stdout_bytes: 64 * 1024 * 1024,
            max_stderr_bytes: 1024 * 1024,
            max_input_bytes: 64 * 1024 * 1024,
            max_chunks: 1_000_000,
            require_absolute_executable: true,
            reject_world_writable_executable: true,
            allowed_executable_roots: vec![],
            preserve_env_vars: vec![],
        }
    }
}

const fn default_require_absolute_executable() -> bool {
    true
}

const fn default_reject_world_writable_executable() -> bool {
    true
}

const CURRENT_VECTOR_CONTRACT_FORMAT_VERSION: i64 = 1;
const MAX_PROFILE_LEN: usize = 128;
const MAX_MODEL_IDENTITY_LEN: usize = 256;
const MAX_MODEL_VERSION_LEN: usize = 128;
const MAX_POLICY_LEN: usize = 128;
const MAX_GENERATOR_COMMAND_ARG_LEN: usize = 4096;
const MAX_GENERATOR_COMMAND_TOTAL_LEN: usize = 16 * 1024;
const MAX_CONTRACT_JSON_BYTES: usize = 32 * 1024;
const MAX_AUDIT_METADATA_BYTES: usize = 2048;
const DEFAULT_OPERATIONAL_READ_LIMIT: usize = 100;
const MAX_OPERATIONAL_READ_LIMIT: usize = 1000;

#[derive(Clone, Debug)]
pub struct AdminHandle {
    inner: Arc<AdminService>,
}

impl AdminHandle {
    #[must_use]
    pub fn new(service: AdminService) -> Self {
        Self {
            inner: Arc::new(service),
        }
    }

    #[must_use]
    pub fn service(&self) -> Arc<AdminService> {
        Arc::clone(&self.inner)
    }
}

impl AdminService {
    #[must_use]
    pub fn new(path: impl AsRef<Path>, schema_manager: Arc<SchemaManager>) -> Self {
        let database_path = path.as_ref().to_path_buf();
        let projections = ProjectionService::new(&database_path, Arc::clone(&schema_manager));
        Self {
            database_path,
            schema_manager,
            projections,
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

        let mut warnings = Vec::new();
        if missing_fts_rows > 0 {
            warnings.push("missing FTS projections detected".to_owned());
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
                if msg.contains("vec_nodes_active") || msg.contains("vec0") =>
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
                if msg.contains("vec_nodes_active") || msg.contains("vec0") =>
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
            rebuild_operational_secondary_index_entries(&tx, &record.name, &record.kind, &indexes)?;
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
            rebuild_operational_secondary_index_entries(&tx, &record.name, &record.kind, &indexes)?;
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
                    &record.kind,
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
    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the config is
    /// invalid, the generator command fails, or the regenerated embeddings are
    /// malformed.
    #[allow(clippy::too_many_lines)]
    pub fn regenerate_vector_embeddings(
        &self,
        config: &VectorRegenerationConfig,
    ) -> Result<VectorRegenerationReport, EngineError> {
        self.regenerate_vector_embeddings_with_policy(config, &VectorGeneratorPolicy::default())
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the config is
    /// invalid, the generator command fails, or the regenerated embeddings are
    /// malformed.
    #[allow(clippy::too_many_lines)]
    pub fn regenerate_vector_embeddings_with_policy(
        &self,
        config: &VectorRegenerationConfig,
        policy: &VectorGeneratorPolicy,
    ) -> Result<VectorRegenerationReport, EngineError> {
        let conn = self.connect()?;
        let config = validate_vector_regeneration_config(&conn, config, policy)
            .map_err(|failure| failure.to_engine_error())?;
        let chunks = collect_regeneration_chunks(&conn)?;
        let payload = build_regeneration_input(&config, chunks.clone());
        let snapshot_hash = compute_snapshot_hash(&payload)?;
        let audit_metadata = VectorRegenerationAuditMetadata {
            profile: config.profile.clone(),
            model_identity: config.model_identity.clone(),
            model_version: config.model_version.clone(),
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
        let notes = generator_policy_notes(policy);
        let generated = match run_vector_generator_bounded(&config, &payload, policy) {
            Ok(generated) => generated,
            Err(failure) => {
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
        };
        let mut embedding_map = match validate_generated_embeddings(&config, &chunks, generated) {
            Ok(embedding_map) => embedding_map,
            Err(failure) => {
                self.persist_vector_regeneration_failure_best_effort(
                    &config.profile,
                    &audit_metadata,
                    &failure,
                );
                return Err(failure.to_engine_error());
            }
        };

        let mut conn = conn;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        match self.schema_manager.ensure_vector_profile(
            &tx,
            &config.profile,
            &config.table_name,
            config.dimension,
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
        let apply_payload = build_regeneration_input(&config, apply_chunks.clone());
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
        persist_vector_contract(&tx, &config, &snapshot_hash)?;
        tx.execute("DELETE FROM vec_nodes_active", [])?;
        let mut stmt = tx
            .prepare_cached("INSERT INTO vec_nodes_active (chunk_id, embedding) VALUES (?1, ?2)")?;
        let mut regenerated_rows = 0usize;
        for chunk in &apply_chunks {
            let Some(embedding) = embedding_map.remove(&chunk.chunk_id) else {
                drop(stmt);
                drop(tx);
                let failure = VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::MalformedGeneratorJson,
                    format!(
                        "generator did not return embedding for chunk '{}'",
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
            dimension: config.dimension,
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
                restored_vec_rows: 0,
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
        let restored_edge_rows = if let Some((
            retire_event_rowid,
            retire_source_ref,
            retire_created_at,
        )) = retire_scope
        {
            let edge_logical_ids = collect_edge_logical_ids_for_restore(
                &tx,
                logical_id,
                retire_source_ref.as_deref(),
                retire_created_at,
                retire_event_rowid,
            )?;
            let mut restored = 0usize;
            for edge_logical_id in edge_logical_ids {
                let edge_row_id: Option<String> = tx
                    .query_row(
                        "SELECT row_id FROM edges \
                         WHERE logical_id = ?1 AND superseded_at IS NOT NULL \
                         ORDER BY superseded_at DESC, created_at DESC, rowid DESC LIMIT 1",
                        [edge_logical_id.as_str()],
                        |row| row.get(0),
                    )
                    .optional()?;
                if let Some(edge_row_id) = edge_row_id {
                    restored += tx.execute(
                        "UPDATE edges SET superseded_at = NULL WHERE row_id = ?1",
                        [edge_row_id.as_str()],
                    )?;
                }
            }
            restored
        } else {
            0
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

        persist_simple_provenance_event(
            &tx,
            "restore_logical_id",
            logical_id,
            Some(serde_json::json!({
                "restored_node_rows": 1,
                "restored_edge_rows": restored_edge_rows,
                "restored_chunk_rows": restored_chunk_rows,
                "restored_fts_rows": restored_fts_rows,
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
            restored_vec_rows,
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

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the transaction cannot be
    /// started, or any SQL statement fails.
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
    /// Returns [`EngineError`] if the WAL checkpoint fails, the SQLite backup fails,
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
            let (busy, log, checkpointed): (i64, i64, i64) =
                conn.query_row("PRAGMA wal_checkpoint(FULL)", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
            if busy != 0 {
                return Err(EngineError::Bridge(format!(
                    "WAL checkpoint blocked: {busy} active reader(s) prevented a full checkpoint; \
                     log frames={log}, checkpointed={checkpointed}; \
                     retry export when no readers are active"
                )));
            }
        }

        let schema_version: u32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM fathom_schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let page_count: u64 = conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))
            .unwrap_or(0);

        // 2. Export the database through SQLite's online backup API so committed data in the WAL
        // is included even when `force_checkpoint` is false.
        if let Some(parent) = destination_path.parent() {
            fs::create_dir_all(parent)?;
        }
        conn.backup(DatabaseName::Main, destination_path, None)?;

        drop(conn);

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
        fs::write(&manifest_path, manifest_json)?;

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct GeneratedEmbedding {
    chunk_id: String,
    embedding: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct GeneratedEmbeddings {
    embeddings: Vec<GeneratedEmbedding>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VectorRegenerationFailureClass {
    InvalidContract,
    PayloadTooLarge,
    GeneratorTimeout,
    GeneratorStdoutOverflow,
    GeneratorStderrOverflow,
    GeneratorNonzeroExit,
    MalformedGeneratorJson,
    SnapshotDrift,
    UnsupportedVecCapability,
}

impl VectorRegenerationFailureClass {
    fn label(self) -> &'static str {
        match self {
            Self::InvalidContract => "invalid contract",
            Self::PayloadTooLarge => "payload too large",
            Self::GeneratorTimeout => "generator timeout",
            Self::GeneratorStdoutOverflow => "generator stdout overflow",
            Self::GeneratorStderrOverflow => "generator stderr overflow",
            Self::GeneratorNonzeroExit => "generator nonzero exit",
            Self::MalformedGeneratorJson => "malformed generator json",
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
    policy: &VectorGeneratorPolicy,
) -> Result<VectorRegenerationConfig, VectorRegenerationFailure> {
    let profile = validate_bounded_text("profile", &config.profile, MAX_PROFILE_LEN)?;
    let table_name = validate_bounded_text("table_name", &config.table_name, MAX_PROFILE_LEN)?;
    if table_name != "vec_nodes_active" {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!("table_name must be vec_nodes_active, got '{table_name}'"),
        ));
    }
    let model_identity = validate_bounded_text(
        "model_identity",
        &config.model_identity,
        MAX_MODEL_IDENTITY_LEN,
    )?;
    let model_version = validate_bounded_text(
        "model_version",
        &config.model_version,
        MAX_MODEL_VERSION_LEN,
    )?;
    if config.dimension == 0 {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            "dimension must be greater than zero".to_owned(),
        ));
    }
    let normalization_policy = validate_bounded_text(
        "normalization_policy",
        &config.normalization_policy,
        MAX_POLICY_LEN,
    )?;
    let chunking_policy =
        validate_bounded_text("chunking_policy", &config.chunking_policy, MAX_POLICY_LEN)?;
    let preprocessing_policy = validate_bounded_text(
        "preprocessing_policy",
        &config.preprocessing_policy,
        MAX_POLICY_LEN,
    )?;
    let generator_command = validate_generator_command(&config.generator_command, policy)?;

    if let Some(existing_dimension) = current_vector_profile_dimension(conn, &profile)? {
        if existing_dimension != config.dimension {
            return Err(VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::InvalidContract,
                format!(
                    "dimension {} does not match existing vector profile dimension {}",
                    config.dimension, existing_dimension
                ),
            ));
        }
    }

    validate_existing_contract_version(conn, &profile)?;

    let normalized = VectorRegenerationConfig {
        profile,
        table_name,
        model_identity,
        model_version,
        dimension: config.dimension,
        normalization_policy,
        chunking_policy,
        preprocessing_policy,
        generator_command,
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
            format!(
                "serialized contract exceeds {} bytes",
                MAX_CONTRACT_JSON_BYTES
            ),
        ));
    }

    Ok(normalized)
}

fn persist_vector_contract(
    conn: &rusqlite::Connection,
    config: &VectorRegenerationConfig,
    snapshot_hash: &str,
) -> Result<(), EngineError> {
    let generator_command_json = serde_json::to_string(&config.generator_command)
        .map_err(|error| EngineError::Bridge(error.to_string()))?;
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
            config.model_identity.as_str(),
            config.model_version.as_str(),
            config.dimension as i64,
            config.normalization_policy.as_str(),
            config.chunking_policy.as_str(),
            config.preprocessing_policy.as_str(),
            generator_command_json,
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

fn build_regeneration_input(
    config: &VectorRegenerationConfig,
    chunks: Vec<VectorRegenerationInputChunk>,
) -> VectorRegenerationInput {
    VectorRegenerationInput {
        profile: config.profile.clone(),
        table_name: config.table_name.clone(),
        model_identity: config.model_identity.clone(),
        model_version: config.model_version.clone(),
        dimension: config.dimension,
        normalization_policy: config.normalization_policy.clone(),
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

fn validate_generated_embeddings(
    config: &VectorRegenerationConfig,
    chunks: &[VectorRegenerationInputChunk],
    generated: GeneratedEmbeddings,
) -> Result<std::collections::HashMap<String, Vec<u8>>, VectorRegenerationFailure> {
    if generated.embeddings.len() != chunks.len() {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::MalformedGeneratorJson,
            format!(
                "generator returned {} embedding(s) for {} chunk(s)",
                generated.embeddings.len(),
                chunks.len()
            ),
        ));
    }

    let mut embedding_map = std::collections::HashMap::new();
    for embedding in generated.embeddings {
        if embedding.embedding.len() != config.dimension {
            return Err(VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::MalformedGeneratorJson,
                format!(
                    "embedding for chunk '{}' has dimension {}, expected {}",
                    embedding.chunk_id,
                    embedding.embedding.len(),
                    config.dimension
                ),
            ));
        }
        if embedding.embedding.iter().any(|value| !value.is_finite()) {
            return Err(VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::MalformedGeneratorJson,
                format!(
                    "embedding for chunk '{}' contains non-finite values",
                    embedding.chunk_id
                ),
            ));
        }
        let bytes: Vec<u8> = embedding
            .embedding
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        if embedding_map
            .insert(embedding.chunk_id.clone(), bytes)
            .is_some()
        {
            return Err(VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::MalformedGeneratorJson,
                format!(
                    "duplicate embedding returned for chunk '{}'",
                    embedding.chunk_id
                ),
            ));
        }
    }

    Ok(embedding_map)
}

fn generator_policy_notes(policy: &VectorGeneratorPolicy) -> Vec<String> {
    let mut notes = vec!["vector embeddings regenerated from application contract".to_owned()];
    if !policy.allowed_executable_roots.is_empty() {
        notes.push("generator executable roots enforced by operator policy".to_owned());
    }
    if !policy.preserve_env_vars.is_empty() {
        notes.push("generator environment reduced to preserved variables".to_owned());
    }
    notes
}

enum GeneratorStream {
    Stdout,
    Stderr,
}

enum StreamReadResult {
    Complete(Vec<u8>),
    Overflow,
    Io(io::Error),
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

fn validate_generator_command(
    command: &[String],
    policy: &VectorGeneratorPolicy,
) -> Result<Vec<String>, VectorRegenerationFailure> {
    if command.is_empty() {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            "generator_command must contain at least one element".to_owned(),
        ));
    }
    let mut total_len = 0usize;
    for argument in command {
        if argument.is_empty() {
            return Err(VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::InvalidContract,
                "generator_command entries must not be empty".to_owned(),
            ));
        }
        if argument.len() > MAX_GENERATOR_COMMAND_ARG_LEN {
            return Err(VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::InvalidContract,
                format!(
                    "generator_command argument exceeds max length {}",
                    MAX_GENERATOR_COMMAND_ARG_LEN
                ),
            ));
        }
        total_len += argument.len();
    }
    if total_len > MAX_GENERATOR_COMMAND_TOTAL_LEN {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            format!(
                "generator_command exceeds max serialized length {}",
                MAX_GENERATOR_COMMAND_TOTAL_LEN
            ),
        ));
    }
    executable_trust::validate_generator_executable(&command[0], policy)?;
    Ok(command.to_vec())
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
    if let Some(version) = version {
        if version > CURRENT_VECTOR_CONTRACT_FORMAT_VERSION {
            return Err(VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::InvalidContract,
                format!(
                    "persisted contract format version {} is unsupported; supported version is {}",
                    version, CURRENT_VECTOR_CONTRACT_FORMAT_VERSION
                ),
            ));
        }
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
            format!("audit metadata exceeds {} bytes", MAX_AUDIT_METADATA_BYTES),
        )
        .to_engine_error());
    }
    Ok(json)
}

fn run_vector_generator_bounded(
    config: &VectorRegenerationConfig,
    payload: &VectorRegenerationInput,
    policy: &VectorGeneratorPolicy,
) -> Result<GeneratedEmbeddings, VectorRegenerationFailure> {
    if payload.chunks.len() > policy.max_chunks {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::PayloadTooLarge,
            format!(
                "chunk count {} exceeds max_chunks {}",
                payload.chunks.len(),
                policy.max_chunks
            ),
        ));
    }

    let input = serde_json::to_vec(payload).map_err(|error| {
        VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::MalformedGeneratorJson,
            error.to_string(),
        )
    })?;
    if input.len() > policy.max_input_bytes {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::PayloadTooLarge,
            format!(
                "serialized input {} bytes exceeds max_input_bytes {}",
                input.len(),
                policy.max_input_bytes
            ),
        ));
    }

    let mut command = Command::new(config.generator_command.first().ok_or_else(|| {
        VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::InvalidContract,
            "missing generator executable",
        )
    })?);
    command.args(config.generator_command.iter().skip(1));
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.env_clear();
    for env_var in &policy.preserve_env_vars {
        if let Some(value) = std::env::var_os(env_var) {
            command.env(env_var, value);
        }
    }

    let mut child = command.spawn().map_err(|error| {
        VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::GeneratorNonzeroExit,
            format!("failed to spawn generator: {error}"),
        )
    })?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&input).map_err(|error| {
            VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::GeneratorNonzeroExit,
                format!("failed to write generator stdin: {error}"),
            )
        })?;
    } else {
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::GeneratorNonzeroExit,
            "failed to open generator stdin",
        ));
    }

    let stdout = child.stdout.take().ok_or_else(|| {
        VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::GeneratorNonzeroExit,
            "failed to open generator stdout",
        )
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::GeneratorNonzeroExit,
            "failed to open generator stderr",
        )
    })?;

    let (tx, rx) = mpsc::channel();
    let stdout_handle = spawn_capped_reader(
        stdout,
        policy.max_stdout_bytes,
        GeneratorStream::Stdout,
        tx.clone(),
    );
    let stderr_handle =
        spawn_capped_reader(stderr, policy.max_stderr_bytes, GeneratorStream::Stderr, tx);

    let start = Instant::now();
    let timeout = Duration::from_millis(policy.timeout_ms);
    let mut stdout_bytes: Option<Vec<u8>> = None;
    let mut stderr_bytes: Option<Vec<u8>> = None;
    let mut status = None;
    let mut stream_error: Option<VectorRegenerationFailure> = None;

    while status.is_none() && stream_error.is_none() {
        while let Ok((stream, result)) = rx.try_recv() {
            match (stream, result) {
                (GeneratorStream::Stdout, StreamReadResult::Complete(bytes)) => {
                    stdout_bytes = Some(bytes);
                }
                (GeneratorStream::Stderr, StreamReadResult::Complete(bytes)) => {
                    stderr_bytes = Some(bytes);
                }
                (GeneratorStream::Stdout, StreamReadResult::Overflow) => {
                    stream_error = Some(VectorRegenerationFailure::new(
                        VectorRegenerationFailureClass::GeneratorStdoutOverflow,
                        format!(
                            "stdout exceeded max_stdout_bytes {}",
                            policy.max_stdout_bytes
                        ),
                    ));
                }
                (GeneratorStream::Stderr, StreamReadResult::Overflow) => {
                    stream_error = Some(VectorRegenerationFailure::new(
                        VectorRegenerationFailureClass::GeneratorStderrOverflow,
                        format!(
                            "stderr exceeded max_stderr_bytes {}",
                            policy.max_stderr_bytes
                        ),
                    ));
                }
                (_, StreamReadResult::Io(error)) => {
                    stream_error = Some(VectorRegenerationFailure::new(
                        VectorRegenerationFailureClass::GeneratorNonzeroExit,
                        format!("failed to read generator stream: {error}"),
                    ));
                }
            }
        }

        if stream_error.is_some() {
            let _ = child.kill();
            break;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            stream_error = Some(VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::GeneratorTimeout,
                format!("generator exceeded timeout after {}ms", policy.timeout_ms),
            ));
            break;
        }
        status = child.try_wait().map_err(|error| {
            VectorRegenerationFailure::new(
                VectorRegenerationFailureClass::GeneratorNonzeroExit,
                format!("failed to poll generator status: {error}"),
            )
        })?;
        if status.is_none() {
            thread::sleep(Duration::from_millis(10));
        }
    }

    let _ = child.wait();
    let _ = stdout_handle.join();
    let _ = stderr_handle.join();

    while let Ok((stream, result)) = rx.try_recv() {
        match (stream, result) {
            (GeneratorStream::Stdout, StreamReadResult::Complete(bytes)) => {
                stdout_bytes = Some(bytes);
            }
            (GeneratorStream::Stderr, StreamReadResult::Complete(bytes)) => {
                stderr_bytes = Some(bytes);
            }
            (GeneratorStream::Stdout, StreamReadResult::Overflow) => {
                stream_error = Some(VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::GeneratorStdoutOverflow,
                    format!(
                        "stdout exceeded max_stdout_bytes {}",
                        policy.max_stdout_bytes
                    ),
                ));
            }
            (GeneratorStream::Stderr, StreamReadResult::Overflow) => {
                stream_error = Some(VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::GeneratorStderrOverflow,
                    format!(
                        "stderr exceeded max_stderr_bytes {}",
                        policy.max_stderr_bytes
                    ),
                ));
            }
            (_, StreamReadResult::Io(error)) => {
                stream_error = Some(VectorRegenerationFailure::new(
                    VectorRegenerationFailureClass::GeneratorNonzeroExit,
                    format!("failed to read generator stream: {error}"),
                ));
            }
        }
    }

    if let Some(error) = stream_error {
        return Err(error);
    }

    let status = status.ok_or_else(|| {
        VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::GeneratorNonzeroExit,
            "vector generator exited without a status",
        )
    })?;
    if !status.success() {
        let stderr = truncate_error_text(stderr_bytes.unwrap_or_default(), policy.max_stderr_bytes);
        return Err(VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::GeneratorNonzeroExit,
            stderr,
        ));
    }

    let stdout = stdout_bytes.unwrap_or_default();
    serde_json::from_slice(&stdout).map_err(|error| {
        VectorRegenerationFailure::new(
            VectorRegenerationFailureClass::MalformedGeneratorJson,
            format!("decode generator output: {error}"),
        )
    })
}

fn spawn_capped_reader<R: Read + Send + 'static>(
    mut reader: R,
    max_bytes: usize,
    stream: GeneratorStream,
    tx: mpsc::Sender<(GeneratorStream, StreamReadResult)>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => {
                    let _ = tx.send((stream, StreamReadResult::Complete(buffer)));
                    break;
                }
                Ok(read_bytes) => {
                    if buffer.len() + read_bytes > max_bytes {
                        let _ = tx.send((stream, StreamReadResult::Overflow));
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..read_bytes]);
                }
                Err(error) => {
                    let _ = tx.send((stream, StreamReadResult::Io(error)));
                    break;
                }
            }
        }
    })
}

fn truncate_error_text(bytes: Vec<u8>, max_bytes: usize) -> String {
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if bytes.len() > max_bytes {
        text.push_str(" [truncated]");
    }
    text
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
    collection_kind: &OperationalCollectionKind,
    indexes: &[OperationalSecondaryIndexDefinition],
) -> Result<(usize, usize), EngineError> {
    clear_operational_secondary_index_entries(tx, collection_name)?;

    let mut mutation_entries_rebuilt = 0usize;
    if *collection_kind == OperationalCollectionKind::AppendOnlyLog {
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
    if *collection_kind == OperationalCollectionKind::LatestState {
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
            if msg.contains("vec_nodes_active") || msg.contains("vec0") =>
        {
            Ok(0)
        }
        Err(error) => Err(EngineError::Sqlite(error)),
    }
}

#[cfg(not(feature = "sqlite-vec"))]
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
            if msg.contains("vec_nodes_active") || msg.contains("vec0") =>
        {
            Ok(0)
        }
        Err(error) => Err(EngineError::Sqlite(error)),
    }
}

#[cfg(not(feature = "sqlite-vec"))]
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
                        OperationalReadCondition::ExactString(_),
                        crate::operational::OperationalSecondaryIndexValueType::String
                    ) | (
                        OperationalReadCondition::Prefix(_),
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
            sql.push_str(&format!("AND s.slot1_text = ?{} ", params.len() + 1));
            params.push(Value::from(value.clone()));
        }
        OperationalReadCondition::Prefix(value) => {
            sql.push_str(&format!("AND s.slot1_text GLOB ?{} ", params.len() + 1));
            params.push(Value::from(glob_prefix_pattern(value)));
        }
        OperationalReadCondition::ExactInteger(value) => {
            sql.push_str(&format!("AND s.slot1_integer = ?{} ", params.len() + 1));
            params.push(Value::from(*value));
        }
        OperationalReadCondition::Range { .. } => return Ok(None),
    }

    if let Some(time_range) = matched.time_range
        && let OperationalReadCondition::Range { lower, upper } = &time_range.condition
    {
        if let Some(lower) = lower {
            sql.push_str(&format!("AND s.sort_timestamp >= ?{} ", params.len() + 1));
            params.push(Value::from(*lower));
        }
        if let Some(upper) = upper {
            sql.push_str(&format!("AND s.sort_timestamp <= ?{} ", params.len() + 1));
            params.push(Value::from(*upper));
        }
    }

    sql.push_str(&format!(
        "ORDER BY s.sort_timestamp DESC, m.mutation_order DESC LIMIT ?{}",
        params.len() + 1
    ));
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
        sql.push_str(&format!(
            "JOIN operational_filter_values f{index} \
             ON f{index}.mutation_id = m.id \
            AND f{index}.collection_name = m.collection_name "
        ));
        match &filter.condition {
            OperationalReadCondition::ExactString(value) => {
                sql.push_str(&format!(
                    "AND f{index}.field_name = ?{} AND f{index}.string_value = ?{} ",
                    params.len() + 1,
                    params.len() + 2
                ));
                params.push(Value::from(filter.field.clone()));
                params.push(Value::from(value.clone()));
            }
            OperationalReadCondition::ExactInteger(value) => {
                sql.push_str(&format!(
                    "AND f{index}.field_name = ?{} AND f{index}.integer_value = ?{} ",
                    params.len() + 1,
                    params.len() + 2
                ));
                params.push(Value::from(filter.field.clone()));
                params.push(Value::from(*value));
            }
            OperationalReadCondition::Prefix(value) => {
                sql.push_str(&format!(
                    "AND f{index}.field_name = ?{} AND f{index}.string_value GLOB ?{} ",
                    params.len() + 1,
                    params.len() + 2
                ));
                params.push(Value::from(filter.field.clone()));
                params.push(Value::from(glob_prefix_pattern(value)));
            }
            OperationalReadCondition::Range { lower, upper } => {
                sql.push_str(&format!("AND f{index}.field_name = ?{} ", params.len() + 1));
                params.push(Value::from(filter.field.clone()));
                if let Some(lower) = lower {
                    sql.push_str(&format!(
                        "AND f{index}.integer_value >= ?{} ",
                        params.len() + 1
                    ));
                    params.push(Value::from(*lower));
                }
                if let Some(upper) = upper {
                    sql.push_str(&format!(
                        "AND f{index}.integer_value <= ?{} ",
                        params.len() + 1
                    ));
                    params.push(Value::from(*upper));
                }
            }
        }
    }
    sql.push_str(&format!(
        "WHERE m.collection_name = ?1 ORDER BY m.mutation_order DESC LIMIT ?{}",
        params.len() + 1
    ));
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
    .map(|value| value.flatten())
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
        OperationalRetentionPolicy::KeepLast { max_rows } => (
            OperationalRetentionActionKind::KeepLast,
            Some(*max_rows as usize),
        ),
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
                if dry_run { 1 } else { 0 },
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

    use super::{AdminService, SafeExportOptions, VectorRegenerationConfig};
    use crate::sqlite;
    use crate::{EngineError, OperationalCollectionKind, OperationalRegisterRequest};

    #[cfg(feature = "sqlite-vec")]
    use fathomdb_query::QueryBuilder;

    #[cfg(feature = "sqlite-vec")]
    use super::{VectorGeneratorPolicy, load_vector_regeneration_config};

    #[cfg(feature = "sqlite-vec")]
    use crate::ExecutionCoordinator;

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

        let coordinator =
            ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), Some(4), 1)
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
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let script_path = temp_dir.path().join("vector-generator-restore.sh");
        fs::write(
            &script_path,
            r#"#!/usr/bin/env bash
set -euo pipefail
python3 -c 'import json, sys
payload = json.load(sys.stdin)
json.dump({"embeddings": [{"chunk_id": payload["chunks"][0]["chunk_id"], "embedding": [0.0, 0.0, 0.0, 0.0]}]}, sys.stdout)'
"#,
        )
        .expect("write script");
        set_file_mode(&script_path, 0o755);

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

        service
            .regenerate_vector_embeddings(&VectorRegenerationConfig {
                profile: "default".to_owned(),
                table_name: "vec_nodes_active".to_owned(),
                model_identity: "model".to_owned(),
                model_version: "1.0.0".to_owned(),
                dimension: 4,
                normalization_policy: "l2".to_owned(),
                chunking_policy: "per_chunk".to_owned(),
                preprocessing_policy: "trim".to_owned(),
                generator_command: vec![script_path.to_string_lossy().to_string()],
            })
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

        let coordinator =
            ExecutionCoordinator::open(db.path(), Arc::new(SchemaManager::new()), Some(4), 1)
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
        assert_eq!(report.was_limited, false);
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
            model_identity: "model-a".to_owned(),
            model_version: "1.0".to_owned(),
            dimension: 4,
            normalization_policy: "l2".to_owned(),
            chunking_policy: "per_chunk".to_owned(),
            preprocessing_policy: "trim".to_owned(),
            generator_command: vec!["/bin/echo".to_owned()],
        };

        fs::write(&json_path, serde_json::to_string(&config).expect("json")).expect("write json");
        fs::write(&toml_path, toml::to_string(&config).expect("toml")).expect("write toml");

        let parsed_json = load_vector_regeneration_config(&json_path).expect("json parse");
        let parsed_toml = load_vector_regeneration_config(&toml_path).expect("toml parse");

        assert_eq!(parsed_json, config);
        assert_eq!(parsed_toml, config);
    }

    #[cfg(all(not(feature = "sqlite-vec"), unix))]
    #[test]
    fn regenerate_vector_embeddings_unsupported_vec_capability_writes_request_and_failed_audit() {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let script_path = temp_dir.path().join("vector-generator-no-vec.sh");

        fs::write(
            &script_path,
            r#"#!/usr/bin/env bash
set -euo pipefail
python3 -c 'import json, sys
payload = json.load(sys.stdin)
embeddings = [{"chunk_id": chunk["chunk_id"], "embedding": [1.0, 0.0, 0.0, 0.0]} for chunk in payload["chunks"]]
json.dump({"embeddings": embeddings}, sys.stdout)'
"#,
        )
        .expect("write generator script");
        set_file_mode(&script_path, 0o755);

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
        let error = service
            .regenerate_vector_embeddings(&VectorRegenerationConfig {
                profile: "default".to_owned(),
                table_name: "vec_nodes_active".to_owned(),
                model_identity: "test-model".to_owned(),
                model_version: "1.0.0".to_owned(),
                dimension: 4,
                normalization_policy: "l2".to_owned(),
                chunking_policy: "per_chunk".to_owned(),
                preprocessing_policy: "trim".to_owned(),
                generator_command: vec![script_path.to_string_lossy().to_string()],
            })
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
    fn regenerate_vector_embeddings_rebuilds_embeddings_from_generator() {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let script_path = temp_dir.path().join("vector-generator.sh");

        fs::write(
            &script_path,
            r#"#!/usr/bin/env bash
set -euo pipefail
python3 -c 'import json, sys
payload = json.load(sys.stdin)
embeddings = []
for chunk in payload["chunks"]:
    text = chunk["text_content"].lower()
    if "budget" in text:
        embedding = [1.0, 0.0, 0.0, 0.0]
    else:
        embedding = [0.0, 1.0, 0.0, 0.0]
    embeddings.append({"chunk_id": chunk["chunk_id"], "embedding": embedding})
json.dump({"embeddings": embeddings}, sys.stdout)'
"#,
        )
        .expect("write generator script");
        set_file_mode(&script_path, 0o755);

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
        let report = service
            .regenerate_vector_embeddings(&VectorRegenerationConfig {
                profile: "default".to_owned(),
                table_name: "vec_nodes_active".to_owned(),
                model_identity: "test-model".to_owned(),
                model_version: "1.0.0".to_owned(),
                dimension: 4,
                normalization_policy: "l2".to_owned(),
                chunking_policy: "per_chunk".to_owned(),
                preprocessing_policy: "trim".to_owned(),
                generator_command: vec![script_path.to_string_lossy().to_string()],
            })
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

        let contract_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM vector_embedding_contracts WHERE profile = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("contract count");
        assert_eq!(contract_count, 1);
        let applied_at: i64 = conn
            .query_row(
                "SELECT applied_at FROM vector_embedding_contracts WHERE profile = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("applied_at");
        assert!(applied_at > 0);
        let snapshot_hash: String = conn
            .query_row(
                "SELECT snapshot_hash FROM vector_embedding_contracts WHERE profile = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("snapshot_hash");
        assert!(!snapshot_hash.is_empty());
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
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn regenerate_vector_embeddings_failure_leaves_contract_and_vec_rows_unchanged() {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let script_path = temp_dir.path().join("vector-generator-fail.sh");

        fs::write(
            &script_path,
            "#!/usr/bin/env bash\nset -euo pipefail\necho 'generator boom' >&2\nexit 17\n",
        )
        .expect("write failing script");
        set_file_mode(&script_path, 0o755);

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
                    "[\"/bin/echo\"]",
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
        let error = service
            .regenerate_vector_embeddings_with_policy(
                &VectorRegenerationConfig {
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
                    model_identity: "new-model".to_owned(),
                    model_version: "1.0.0".to_owned(),
                    dimension: 4,
                    normalization_policy: "l2".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                    generator_command: vec![script_path.to_string_lossy().to_string()],
                },
                &VectorGeneratorPolicy::default(),
            )
            .expect_err("generator should fail");

        assert!(error.to_string().contains("generator nonzero exit"));

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
        assert!(failure_metadata.contains("\"failure_class\":\"generator nonzero exit\""));
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn regenerate_vector_embeddings_snapshot_drift_is_retryable_and_non_mutating() {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let script_path = temp_dir.path().join("vector-generator-drift.sh");
        let db_path = db.path().to_string_lossy().to_string();

        fs::write(
            &script_path,
            format!(
                r#"#!/usr/bin/env bash
set -euo pipefail
python3 -c 'import json, sqlite3, sys
payload = json.load(sys.stdin)
conn = sqlite3.connect({db_path:?})
conn.execute("INSERT INTO chunks (id, node_logical_id, text_content, created_at) VALUES (?, ?, ?, ?)", ("chunk-2", "doc-1", "late arriving text", 101))
conn.commit()
conn.close()
embeddings = [{{"chunk_id": chunk["chunk_id"], "embedding": [1.0, 0.0, 0.0, 0.0]}} for chunk in payload["chunks"]]
json.dump({{"embeddings": embeddings}}, sys.stdout)'
"#,
            ),
        )
        .expect("write drift script");
        set_file_mode(&script_path, 0o755);

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
        }

        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let error = service
            .regenerate_vector_embeddings_with_policy(
                &VectorRegenerationConfig {
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
                    model_identity: "test-model".to_owned(),
                    model_version: "1.0.0".to_owned(),
                    dimension: 4,
                    normalization_policy: "l2".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                    generator_command: vec![script_path.to_string_lossy().to_string()],
                },
                &VectorGeneratorPolicy::default(),
            )
            .expect_err("snapshot drift should fail");

        assert!(
            error
                .to_string()
                .contains("vector regeneration snapshot drift:")
        );
        assert!(error.to_string().contains("[retryable]"));

        let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
        let contract_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM vector_embedding_contracts",
                [],
                |row| row.get(0),
            )
            .expect("contract count");
        assert_eq!(contract_count, 0);
        let vec_count: i64 = conn
            .query_row("SELECT count(*) FROM vec_nodes_active", [], |row| {
                row.get(0)
            })
            .expect("vec count");
        assert_eq!(vec_count, 0);
        let failure_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM provenance_events WHERE event_type = 'vector_regeneration_failed' AND subject = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("failure count");
        assert_eq!(failure_count, 1);
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn regenerate_vector_embeddings_times_out_and_kills_generator() {
        let (_db, service) = setup();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let script_path = temp_dir.path().join("vector-generator-timeout.sh");

        fs::write(
            &script_path,
            "#!/usr/bin/env bash\nset -euo pipefail\nsleep 1\nprintf '{\"embeddings\":[]}'\n",
        )
        .expect("write timeout script");
        set_file_mode(&script_path, 0o755);

        let error = service
            .regenerate_vector_embeddings_with_policy(
                &VectorRegenerationConfig {
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
                    model_identity: "model".to_owned(),
                    model_version: "1.0.0".to_owned(),
                    dimension: 4,
                    normalization_policy: "l2".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                    generator_command: vec![script_path.to_string_lossy().to_string()],
                },
                &VectorGeneratorPolicy {
                    timeout_ms: 50,
                    max_stdout_bytes: 1024,
                    max_stderr_bytes: 1024,
                    max_input_bytes: 1024,
                    max_chunks: 10,
                    require_absolute_executable: true,
                    reject_world_writable_executable: true,
                    allowed_executable_roots: vec![],
                    preserve_env_vars: vec![],
                },
            )
            .expect_err("generator should time out");
        assert!(error.to_string().contains("generator timeout"));
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn regenerate_vector_embeddings_rejects_oversized_stdout() {
        let (_db, service) = setup();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let script_path = temp_dir.path().join("vector-generator-stdout.sh");

        fs::write(
            &script_path,
            "#!/usr/bin/env bash\nset -euo pipefail\npython3 -c 'import sys; sys.stdout.write(\"x\" * 5000)'\n",
        )
        .expect("write stdout script");
        set_file_mode(&script_path, 0o755);

        let error = service
            .regenerate_vector_embeddings_with_policy(
                &VectorRegenerationConfig {
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
                    model_identity: "model".to_owned(),
                    model_version: "1.0.0".to_owned(),
                    dimension: 4,
                    normalization_policy: "l2".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                    generator_command: vec![script_path.to_string_lossy().to_string()],
                },
                &VectorGeneratorPolicy {
                    timeout_ms: 1000,
                    max_stdout_bytes: 128,
                    max_stderr_bytes: 1024,
                    max_input_bytes: 1024,
                    max_chunks: 10,
                    require_absolute_executable: true,
                    reject_world_writable_executable: true,
                    allowed_executable_roots: vec![],
                    preserve_env_vars: vec![],
                },
            )
            .expect_err("generator stdout should overflow");
        assert!(error.to_string().contains("stdout overflow"));
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn regenerate_vector_embeddings_rejects_oversized_stderr() {
        let (_db, service) = setup();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let script_path = temp_dir.path().join("vector-generator-stderr.sh");

        fs::write(
            &script_path,
            "#!/usr/bin/env bash\nset -euo pipefail\npython3 -c 'import sys; sys.stderr.write(\"e\" * 5000); sys.exit(7)'\n",
        )
        .expect("write stderr script");
        set_file_mode(&script_path, 0o755);

        let error = service
            .regenerate_vector_embeddings_with_policy(
                &VectorRegenerationConfig {
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
                    model_identity: "model".to_owned(),
                    model_version: "1.0.0".to_owned(),
                    dimension: 4,
                    normalization_policy: "l2".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                    generator_command: vec![script_path.to_string_lossy().to_string()],
                },
                &VectorGeneratorPolicy {
                    timeout_ms: 1000,
                    max_stdout_bytes: 1024,
                    max_stderr_bytes: 128,
                    max_input_bytes: 1024,
                    max_chunks: 10,
                    require_absolute_executable: true,
                    reject_world_writable_executable: true,
                    allowed_executable_roots: vec![],
                    preserve_env_vars: vec![],
                },
            )
            .expect_err("generator stderr should overflow");
        assert!(error.to_string().contains("stderr overflow"));
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn regenerate_vector_embeddings_rejects_oversized_input_before_spawn() {
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
                 VALUES ('chunk-1', 'doc-1', 'this chunk is intentionally long to exceed the configured input limit', 100)",
                [],
            )
            .expect("insert chunk");
        }

        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let error = service
            .regenerate_vector_embeddings_with_policy(
                &VectorRegenerationConfig {
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
                    model_identity: "model".to_owned(),
                    model_version: "1.0.0".to_owned(),
                    dimension: 4,
                    normalization_policy: "l2".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                    generator_command: vec!["/bin/echo".to_owned()],
                },
                &VectorGeneratorPolicy {
                    timeout_ms: 1000,
                    max_stdout_bytes: 1024,
                    max_stderr_bytes: 1024,
                    max_input_bytes: 32,
                    max_chunks: 10,
                    require_absolute_executable: true,
                    reject_world_writable_executable: true,
                    allowed_executable_roots: vec![],
                    preserve_env_vars: vec![],
                },
            )
            .expect_err("input size should be rejected before spawn");
        assert!(error.to_string().contains("payload too large"));
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn regenerate_vector_embeddings_rejects_excessive_chunk_count_before_spawn() {
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
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) VALUES ('chunk-1', 'doc-1', 'a', 100)",
                [],
            )
            .expect("insert chunk 1");
            conn.execute(
                "INSERT INTO chunks (id, node_logical_id, text_content, created_at) VALUES ('chunk-2', 'doc-1', 'b', 101)",
                [],
            )
            .expect("insert chunk 2");
        }

        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let error = service
            .regenerate_vector_embeddings_with_policy(
                &VectorRegenerationConfig {
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
                    model_identity: "model".to_owned(),
                    model_version: "1.0.0".to_owned(),
                    dimension: 4,
                    normalization_policy: "l2".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                    generator_command: vec!["/bin/echo".to_owned()],
                },
                &VectorGeneratorPolicy {
                    timeout_ms: 1000,
                    max_stdout_bytes: 1024,
                    max_stderr_bytes: 1024,
                    max_input_bytes: 2048,
                    max_chunks: 1,
                    require_absolute_executable: true,
                    reject_world_writable_executable: true,
                    allowed_executable_roots: vec![],
                    preserve_env_vars: vec![],
                },
            )
            .expect_err("chunk count should be rejected before spawn");
        assert!(error.to_string().contains("payload too large"));
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn regenerate_vector_embeddings_malformed_json_leaves_contract_and_vec_rows_unchanged() {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let script_path = temp_dir.path().join("vector-generator-bad-json.sh");

        fs::write(
            &script_path,
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf 'not-json'\n",
        )
        .expect("write bad json script");
        set_file_mode(&script_path, 0o755);

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
                    "[\"/bin/echo\"]",
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
        let error = service
            .regenerate_vector_embeddings_with_policy(
                &VectorRegenerationConfig {
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
                    model_identity: "new-model".to_owned(),
                    model_version: "1.0.0".to_owned(),
                    dimension: 4,
                    normalization_policy: "l2".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                    generator_command: vec![script_path.to_string_lossy().to_string()],
                },
                &VectorGeneratorPolicy::default(),
            )
            .expect_err("bad json should fail");

        assert!(error.to_string().contains("decode generator output"));

        let conn = crate::sqlite::open_connection_with_vec(db.path()).expect("vec conn");
        let model_identity: String = conn
            .query_row(
                "SELECT model_identity FROM vector_embedding_contracts WHERE profile = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("model identity");
        assert_eq!(model_identity, "old-model");
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
    }

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
        let error = service
            .regenerate_vector_embeddings(&VectorRegenerationConfig {
                profile: "   ".to_owned(),
                table_name: "vec_nodes_active".to_owned(),
                model_identity: "test-model".to_owned(),
                model_version: "1.0.0".to_owned(),
                dimension: 4,
                normalization_policy: "l2".to_owned(),
                chunking_policy: "per_chunk".to_owned(),
                preprocessing_policy: "trim".to_owned(),
                generator_command: vec!["/bin/echo".to_owned()],
            })
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
    fn regenerate_vector_embeddings_rejects_world_writable_executable_when_policy_requires_it() {
        let (_db, service) = setup();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let script_path = temp_dir.path().join("vector-generator-world-writable.sh");

        fs::write(
            &script_path,
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf '{\"embeddings\":[]}'\n",
        )
        .expect("write script");
        set_file_mode(&script_path, 0o777);

        let error = service
            .regenerate_vector_embeddings_with_policy(
                &VectorRegenerationConfig {
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
                    model_identity: "model".to_owned(),
                    model_version: "1.0.0".to_owned(),
                    dimension: 4,
                    normalization_policy: "l2".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                    generator_command: vec![script_path.to_string_lossy().to_string()],
                },
                &VectorGeneratorPolicy::default(),
            )
            .expect_err("world-writable executable should be rejected");

        assert!(error.to_string().contains("world-writable executable"));
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn regenerate_vector_embeddings_rejects_executable_outside_allowlisted_roots() {
        let (_db, service) = setup();
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let allowed_dir = tempfile::tempdir().expect("allowed dir");
        let script_path = temp_dir.path().join("vector-generator-outside-root.sh");

        fs::write(
            &script_path,
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf '{\"embeddings\":[]}'\n",
        )
        .expect("write script");
        set_file_mode(&script_path, 0o755);

        let error = service
            .regenerate_vector_embeddings_with_policy(
                &VectorRegenerationConfig {
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
                    model_identity: "model".to_owned(),
                    model_version: "1.0.0".to_owned(),
                    dimension: 4,
                    normalization_policy: "l2".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                    generator_command: vec![script_path.to_string_lossy().to_string()],
                },
                &VectorGeneratorPolicy {
                    timeout_ms: 1000,
                    max_stdout_bytes: 1024,
                    max_stderr_bytes: 1024,
                    max_input_bytes: 1024,
                    max_chunks: 10,
                    require_absolute_executable: true,
                    reject_world_writable_executable: true,
                    allowed_executable_roots: vec![
                        allowed_dir.path().to_string_lossy().to_string(),
                    ],
                    preserve_env_vars: vec![],
                },
            )
            .expect_err("disallowed root should be rejected");

        assert!(
            error
                .to_string()
                .contains("outside allowed executable roots")
        );
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
                    "[\"/bin/echo\"]",
                    111,
                    "old-snapshot",
                    99,
                    111,
                ],
            )
            .expect("seed future contract");
        }

        let service = AdminService::new(db.path(), Arc::clone(&schema));
        let error = service
            .regenerate_vector_embeddings(&VectorRegenerationConfig {
                profile: "default".to_owned(),
                table_name: "vec_nodes_active".to_owned(),
                model_identity: "test-model".to_owned(),
                model_version: "1.0.0".to_owned(),
                dimension: 4,
                normalization_policy: "l2".to_owned(),
                chunking_policy: "per_chunk".to_owned(),
                preprocessing_policy: "trim".to_owned(),
                generator_command: vec!["/bin/echo".to_owned()],
            })
            .expect_err("future contract version should be rejected");

        assert!(error.to_string().contains("unsupported"));
        assert!(error.to_string().contains("format version"));
    }

    #[cfg(feature = "sqlite-vec")]
    #[test]
    fn regenerate_vector_embeddings_clears_environment_except_preserved_vars() {
        let db = NamedTempFile::new().expect("temp file");
        let schema = Arc::new(SchemaManager::new());
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let script_path = temp_dir.path().join("vector-generator-env.sh");
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

        fs::write(
            &script_path,
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${VECTOR_TEST_SECRET:-}" != "expected" ]]; then
  echo "missing secret" >&2
  exit 9
fi
python3 -c 'import json, sys
payload = json.load(sys.stdin)
json.dump({"embeddings": [{"chunk_id": payload["chunks"][0]["chunk_id"], "embedding": [1.0, 0.0, 0.0, 0.0]}]}, sys.stdout)'
"#,
        )
        .expect("write script");
        set_file_mode(&script_path, 0o755);

        let service = AdminService::new(db.path(), Arc::clone(&schema));
        unsafe {
            std::env::set_var("VECTOR_TEST_SECRET", "expected");
        }
        let missing_env = service
            .regenerate_vector_embeddings_with_policy(
                &VectorRegenerationConfig {
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
                    model_identity: "model".to_owned(),
                    model_version: "1.0.0".to_owned(),
                    dimension: 4,
                    normalization_policy: "l2".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                    generator_command: vec![script_path.to_string_lossy().to_string()],
                },
                &VectorGeneratorPolicy::default(),
            )
            .expect_err("non-preserved env var should be dropped");
        assert!(missing_env.to_string().contains("nonzero exit"));

        let report = service
            .regenerate_vector_embeddings_with_policy(
                &VectorRegenerationConfig {
                    profile: "default".to_owned(),
                    table_name: "vec_nodes_active".to_owned(),
                    model_identity: "model".to_owned(),
                    model_version: "1.0.0".to_owned(),
                    dimension: 4,
                    normalization_policy: "l2".to_owned(),
                    chunking_policy: "per_chunk".to_owned(),
                    preprocessing_policy: "trim".to_owned(),
                    generator_command: vec![script_path.to_string_lossy().to_string()],
                },
                &VectorGeneratorPolicy {
                    timeout_ms: 1000,
                    max_stdout_bytes: 1024,
                    max_stderr_bytes: 1024,
                    max_input_bytes: 4096,
                    max_chunks: 10,
                    require_absolute_executable: true,
                    reject_world_writable_executable: true,
                    allowed_executable_roots: vec![],
                    preserve_env_vars: vec!["VECTOR_TEST_SECRET".to_owned()],
                },
            )
            .expect("preserved env var should allow success");
        assert_eq!(report.regenerated_rows, 1);
        unsafe {
            std::env::remove_var("VECTOR_TEST_SECRET");
        }
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
}
