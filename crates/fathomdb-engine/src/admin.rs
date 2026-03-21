use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fathomdb_schema::SchemaManager;
use serde::Serialize;

use crate::{projection::ProjectionTarget, sqlite, EngineError, ProjectionRepairReport, ProjectionService};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IntegrityReport {
    pub physical_ok: bool,
    pub foreign_keys_ok: bool,
    pub missing_fts_rows: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TraceReport {
    pub source_ref: String,
    pub node_rows: usize,
    pub edge_rows: usize,
    pub action_rows: usize,
}

#[derive(Debug)]
pub struct AdminService {
    database_path: PathBuf,
    schema_manager: Arc<SchemaManager>,
    projections: ProjectionService,
}

#[derive(Clone, Debug)]
pub struct AdminHandle {
    inner: Arc<AdminService>,
}

impl AdminHandle {
    pub fn new(service: AdminService) -> Self {
        Self {
            inner: Arc::new(service),
        }
    }

    pub fn service(&self) -> Arc<AdminService> {
        Arc::clone(&self.inner)
    }
}

impl AdminService {
    pub fn new(path: impl AsRef<Path>, schema_manager: Arc<SchemaManager>) -> Self {
        let database_path = path.as_ref().to_path_buf();
        let projections = ProjectionService::new(&database_path, Arc::clone(&schema_manager));
        Self {
            database_path,
            schema_manager,
            projections,
        }
    }

    pub fn check_integrity(&self) -> Result<IntegrityReport, EngineError> {
        let conn = sqlite::open_connection(&self.database_path)?;
        self.schema_manager.bootstrap(&conn)?;

        let physical_result: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        let foreign_key_count: i64 =
            conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| row.get(0))?;
        let missing_fts_rows: i64 = conn.query_row(
            r#"
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
            "#,
            [],
            |row| row.get(0),
        )?;

        let mut warnings = Vec::new();
        if missing_fts_rows > 0 {
            warnings.push("missing FTS projections detected".to_owned());
        }

        Ok(IntegrityReport {
            physical_ok: physical_result == "ok",
            foreign_keys_ok: foreign_key_count == 0,
            missing_fts_rows: missing_fts_rows as usize,
            warnings,
        })
    }

    pub fn rebuild_projections(&self, target: ProjectionTarget) -> Result<ProjectionRepairReport, EngineError> {
        self.projections.rebuild_projections(target)
    }

    pub fn rebuild_missing_projections(&self) -> Result<ProjectionRepairReport, EngineError> {
        self.projections.rebuild_missing_projections()
    }

    pub fn trace_source(&self, source_ref: &str) -> Result<TraceReport, EngineError> {
        let conn = sqlite::open_connection(&self.database_path)?;
        self.schema_manager.bootstrap(&conn)?;

        Ok(TraceReport {
            source_ref: source_ref.to_owned(),
            node_rows: count_source_ref(&conn, "nodes", source_ref)?,
            edge_rows: count_source_ref(&conn, "edges", source_ref)?,
            action_rows: count_source_ref(&conn, "actions", source_ref)?,
        })
    }

    pub fn excise_source(&self, source_ref: &str) -> Result<TraceReport, EngineError> {
        let conn = sqlite::open_connection(&self.database_path)?;
        self.schema_manager.bootstrap(&conn)?;

        conn.execute(
            "UPDATE nodes SET superseded_at = unixepoch() WHERE source_ref = ?1 AND superseded_at IS NULL",
            [source_ref],
        )?;
        conn.execute(
            "UPDATE edges SET superseded_at = unixepoch() WHERE source_ref = ?1 AND superseded_at IS NULL",
            [source_ref],
        )?;
        conn.execute(
            "UPDATE actions SET superseded_at = unixepoch() WHERE source_ref = ?1 AND superseded_at IS NULL",
            [source_ref],
        )?;

        self.trace_source(source_ref)
    }

    pub fn safe_export(&self, destination_path: impl AsRef<Path>) -> Result<(), EngineError> {
        let destination_path = destination_path.as_ref();
        if let Some(parent) = destination_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&self.database_path, destination_path)?;
        Ok(())
    }
}

fn count_source_ref(
    conn: &rusqlite::Connection,
    table: &str,
    source_ref: &str,
) -> Result<usize, EngineError> {
    let sql = format!("SELECT count(*) FROM {table} WHERE source_ref = ?1");
    let count: i64 = conn.query_row(&sql, [source_ref], |row| row.get(0))?;
    Ok(count as usize)
}
