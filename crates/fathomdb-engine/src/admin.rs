use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use fathomdb_schema::SchemaManager;
use rusqlite::{DatabaseName, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    EngineError, ProjectionRepairReport, ProjectionService, ids::new_id,
    projection::ProjectionTarget, sqlite,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IntegrityReport {
    pub physical_ok: bool,
    pub foreign_keys_ok: bool,
    pub missing_fts_rows: usize,
    pub duplicate_active_logical_ids: usize,
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
    pub node_logical_ids: Vec<String>,
    pub action_ids: Vec<String>,
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
    /// Vec rows whose node has been superseded or retired.
    pub vec_rows_for_superseded_nodes: usize,
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
    #[serde(default = "default_warn_executable_path")]
    pub warn_executable_path: bool,
}

impl Default for VectorGeneratorPolicy {
    fn default() -> Self {
        Self {
            timeout_ms: 300_000,
            max_stdout_bytes: 64 * 1024 * 1024,
            max_stderr_bytes: 1024 * 1024,
            max_input_bytes: 64 * 1024 * 1024,
            max_chunks: 1_000_000,
            warn_executable_path: true,
        }
    }
}

const fn default_warn_executable_path() -> bool {
    true
}

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

        let mut warnings = Vec::new();
        if missing_fts_rows > 0 {
            warnings.push("missing FTS projections detected".to_owned());
        }
        if duplicate_active > 0 {
            warnings.push("duplicate active logical_ids detected".to_owned());
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
                WHERE n.logical_id = c.node_logical_id AND n.superseded_at IS NULL
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
            JOIN nodes n  ON n.logical_id = c.node_logical_id
            WHERE n.superseded_at IS NOT NULL
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

        let mut warnings = Vec::new();
        if orphaned_chunks > 0 {
            warnings.push(format!(
                "{orphaned_chunks} orphaned chunk(s) with no active node"
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
                "{vec_rows_for_superseded_nodes} vec row(s) for superseded node(s)"
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
            warnings,
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
        if config.table_name != "vec_nodes_active" {
            return Err(EngineError::Bridge(format!(
                "unsupported vector table name: {}",
                config.table_name
            )));
        }
        if config.generator_command.is_empty() {
            return Err(EngineError::Bridge(
                "generator_command must contain at least one executable".to_owned(),
            ));
        }
        if config.dimension == 0 {
            return Err(EngineError::Bridge(
                "vector embedding dimension must be greater than zero".to_owned(),
            ));
        }

        let conn = self.connect()?;
        let chunks = collect_regeneration_chunks(&conn)?;
        let payload = build_regeneration_input(config, chunks.clone());
        let snapshot_hash = compute_snapshot_hash(&payload)?;
        let notes = generator_policy_notes(config, policy);
        let generated = run_vector_generator_bounded(config, &payload, policy)?;
        let mut embedding_map = validate_generated_embeddings(config, &chunks, generated)?;

        let mut conn = conn;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        self.schema_manager
            .ensure_vector_profile(&tx, &config.profile, &config.table_name, config.dimension)
            .map_err(EngineError::Schema)?;
        let apply_chunks = collect_regeneration_chunks(&tx)?;
        let apply_payload = build_regeneration_input(config, apply_chunks.clone());
        let apply_hash = compute_snapshot_hash(&apply_payload)?;
        if apply_hash != snapshot_hash {
            return Err(EngineError::Bridge(
                "vector regeneration snapshot changed during generation; retry".to_owned(),
            ));
        }
        persist_vector_contract(&tx, config, &snapshot_hash)?;
        tx.execute("DELETE FROM vec_nodes_active", [])?;
        let mut stmt = tx
            .prepare_cached("INSERT INTO vec_nodes_active (chunk_id, embedding) VALUES (?1, ?2)")?;
        let mut regenerated_rows = 0usize;
        for chunk in &apply_chunks {
            let embedding = embedding_map.remove(&chunk.chunk_id).ok_or_else(|| {
                EngineError::Bridge(format!(
                    "generator did not return embedding for chunk '{}'",
                    chunk.chunk_id
                ))
            })?;
            stmt.execute(rusqlite::params![chunk.chunk_id.as_str(), embedding])?;
            regenerated_rows += 1;
        }
        drop(stmt);
        tx.execute(
            "INSERT INTO provenance_events (id, event_type, subject) VALUES (?1, 'vector_regeneration_apply', ?2)",
            rusqlite::params![new_id(), config.profile.as_str()],
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

        Ok(TraceReport {
            source_ref: source_ref.to_owned(),
            node_rows: count_source_ref(&conn, "nodes", source_ref)?,
            edge_rows: count_source_ref(&conn, "edges", source_ref)?,
            action_rows: count_source_ref(&conn, "actions", source_ref)?,
            node_logical_ids,
            action_ids,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails, the transaction cannot be
    /// started, or any SQL statement fails.
    pub fn excise_source(&self, source_ref: &str) -> Result<TraceReport, EngineError> {
        let mut conn = self.connect()?;

        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

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
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, unixepoch(), ?10, unixepoch())
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
        ],
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
) -> Result<std::collections::HashMap<String, Vec<u8>>, EngineError> {
    if generated.embeddings.len() != chunks.len() {
        return Err(EngineError::Bridge(format!(
            "generator returned {} embedding(s) for {} chunk(s)",
            generated.embeddings.len(),
            chunks.len()
        )));
    }

    let mut embedding_map = std::collections::HashMap::new();
    for embedding in generated.embeddings {
        if embedding.embedding.len() != config.dimension {
            return Err(EngineError::Bridge(format!(
                "embedding for chunk '{}' has dimension {}, expected {}",
                embedding.chunk_id,
                embedding.embedding.len(),
                config.dimension
            )));
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
            return Err(EngineError::Bridge(format!(
                "duplicate embedding returned for chunk '{}'",
                embedding.chunk_id
            )));
        }
    }

    Ok(embedding_map)
}

fn generator_policy_notes(
    config: &VectorRegenerationConfig,
    policy: &VectorGeneratorPolicy,
) -> Vec<String> {
    let mut notes = vec!["vector embeddings regenerated from application contract".to_owned()];
    if policy.warn_executable_path {
        if let Some(executable) = config.generator_command.first() {
            if let Some(note) = executable_path_warning(executable) {
                notes.push(note);
            }
        }
    }
    notes
}

fn executable_path_warning(executable: &str) -> Option<String> {
    let path = Path::new(executable);
    if !path.is_absolute() {
        return Some(format!(
            "vector generator executable '{}' is not an absolute path",
            executable
        ));
    }
    let safe_roots = ["/usr/bin", "/usr/local/bin", "/bin", "/opt/homebrew/bin"];
    if safe_roots.iter().any(|root| path.starts_with(root)) {
        return None;
    }
    Some(format!(
        "vector generator executable '{}' is outside common safe roots",
        executable
    ))
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

fn run_vector_generator_bounded(
    config: &VectorRegenerationConfig,
    payload: &VectorRegenerationInput,
    policy: &VectorGeneratorPolicy,
) -> Result<GeneratedEmbeddings, EngineError> {
    if payload.chunks.len() > policy.max_chunks {
        return Err(EngineError::Bridge(format!(
            "vector generator input exceeded max_chunks: {} > {}",
            payload.chunks.len(),
            policy.max_chunks
        )));
    }

    let input =
        serde_json::to_vec(payload).map_err(|error| EngineError::Bridge(error.to_string()))?;
    if input.len() > policy.max_input_bytes {
        return Err(EngineError::Bridge(format!(
            "vector generator input exceeded max_input_bytes: {} > {}",
            input.len(),
            policy.max_input_bytes
        )));
    }

    let mut command = Command::new(
        config
            .generator_command
            .first()
            .ok_or_else(|| EngineError::Bridge("missing generator executable".to_owned()))?,
    );
    command.args(config.generator_command.iter().skip(1));
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&input)?;
    } else {
        return Err(EngineError::Bridge(
            "failed to open generator stdin".to_owned(),
        ));
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| EngineError::Bridge("failed to open generator stdout".to_owned()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| EngineError::Bridge("failed to open generator stderr".to_owned()))?;

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
    let mut stream_error: Option<EngineError> = None;

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
                    stream_error = Some(EngineError::Bridge(format!(
                        "vector generator stdout exceeded max_stdout_bytes ({})",
                        policy.max_stdout_bytes
                    )));
                }
                (GeneratorStream::Stderr, StreamReadResult::Overflow) => {
                    stream_error = Some(EngineError::Bridge(format!(
                        "vector generator stderr exceeded max_stderr_bytes ({})",
                        policy.max_stderr_bytes
                    )));
                }
                (_, StreamReadResult::Io(error)) => stream_error = Some(EngineError::Io(error)),
            }
        }

        if stream_error.is_some() {
            let _ = child.kill();
            break;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            stream_error = Some(EngineError::Bridge(format!(
                "vector generator timed out after {}ms",
                policy.timeout_ms
            )));
            break;
        }
        status = child.try_wait()?;
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
                stream_error = Some(EngineError::Bridge(format!(
                    "vector generator stdout exceeded max_stdout_bytes ({})",
                    policy.max_stdout_bytes
                )));
            }
            (GeneratorStream::Stderr, StreamReadResult::Overflow) => {
                stream_error = Some(EngineError::Bridge(format!(
                    "vector generator stderr exceeded max_stderr_bytes ({})",
                    policy.max_stderr_bytes
                )));
            }
            (_, StreamReadResult::Io(error)) => stream_error = Some(EngineError::Io(error)),
        }
    }

    if let Some(error) = stream_error {
        return Err(error);
    }

    let status = status.ok_or_else(|| {
        EngineError::Bridge("vector generator exited without a status".to_owned())
    })?;
    if !status.success() {
        let stderr = truncate_error_text(stderr_bytes.unwrap_or_default(), policy.max_stderr_bytes);
        return Err(EngineError::Bridge(format!(
            "vector generator failed: {stderr}"
        )));
    }

    let stdout = stdout_bytes.unwrap_or_default();
    serde_json::from_slice(&stdout)
        .map_err(|error| EngineError::Bridge(format!("decode generator output: {error}")))
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
        other => return Err(EngineError::Bridge(format!("unknown table: {other}"))),
    };
    let count: i64 = conn.query_row(sql, [source_ref], |row| row.get(0))?;
    // FIX(review): was `count as usize` — unsound cast.
    // Chose option (C) here: propagate error since this is a user-facing helper.
    usize::try_from(count)
        .map_err(|_| EngineError::Bridge(format!("count overflow for table {table}: {count}")))
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;

    use fathomdb_schema::SchemaManager;
    use tempfile::NamedTempFile;

    use super::{
        AdminService, SafeExportOptions, VectorGeneratorPolicy, VectorRegenerationConfig,
        load_vector_regeneration_config,
    };
    use crate::sqlite;

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
        assert!(report.warnings.is_empty());
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
        let mut perms = fs::metadata(&script_path)
            .expect("script metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod");

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
        let audit_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM provenance_events WHERE event_type = 'vector_regeneration_apply' AND subject = 'default'",
                [],
                |row| row.get(0),
            )
            .expect("audit count");
        assert_eq!(audit_count, 1);
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
        let mut perms = fs::metadata(&script_path)
            .expect("script metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod");

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

        assert!(error.to_string().contains("vector generator failed"));

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
        let mut perms = fs::metadata(&script_path)
            .expect("script metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod");

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
                .contains("vector regeneration snapshot changed during generation; retry")
        );

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
        let mut perms = fs::metadata(&script_path)
            .expect("script metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod");

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
                    warn_executable_path: false,
                },
            )
            .expect_err("generator should time out");
        assert!(error.to_string().contains("vector generator timed out"));
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
        let mut perms = fs::metadata(&script_path)
            .expect("script metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod");

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
                    warn_executable_path: false,
                },
            )
            .expect_err("generator stdout should overflow");
        assert!(
            error
                .to_string()
                .contains("vector generator stdout exceeded")
        );
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
        let mut perms = fs::metadata(&script_path)
            .expect("script metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod");

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
                    warn_executable_path: false,
                },
            )
            .expect_err("generator stderr should overflow");
        assert!(
            error
                .to_string()
                .contains("vector generator stderr exceeded")
        );
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
                    warn_executable_path: false,
                },
            )
            .expect_err("input size should be rejected before spawn");
        assert!(
            error
                .to_string()
                .contains("vector generator input exceeded")
        );
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
                    warn_executable_path: false,
                },
            )
            .expect_err("chunk count should be rejected before spawn");
        assert!(
            error
                .to_string()
                .contains("vector generator input exceeded max_chunks")
        );
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
        let mut perms = fs::metadata(&script_path)
            .expect("script metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod");

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
        assert_eq!(manifest.schema_version, 4, "schema_version should be 4");
        assert_eq!(manifest.protocol_version, 1, "protocol_version should be 1");
        assert!(manifest.page_count > 0, "page_count should be positive");
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
        assert_eq!(manifest.schema_version, 4);
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
    fn excise_source_repairs_fts_after_excision() {
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
            assert_eq!(fts_count, 1, "FTS should be rebuilt after excise");
        }
    }
}
