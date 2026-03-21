use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fathomdb_schema::SchemaManager;
use rusqlite::OptionalExtension;
use serde::Serialize;

use crate::{
    EngineError, ProjectionRepairReport, ProjectionService, projection::ProjectionTarget, sqlite,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IntegrityReport {
    pub physical_ok: bool,
    pub foreign_keys_ok: bool,
    pub missing_fts_rows: usize,
    pub duplicate_active_logical_ids: usize,
    pub warnings: Vec<String>,
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

        let physical_result: String =
            conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        let foreign_key_count: i64 =
            conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })?;
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
        let duplicate_active: i64 = conn.query_row(
            r#"
            SELECT count(*)
            FROM (
                SELECT logical_id
                FROM nodes
                WHERE superseded_at IS NULL
                GROUP BY logical_id
                HAVING count(*) > 1
            )
            "#,
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

        Ok(IntegrityReport {
            physical_ok: physical_result == "ok",
            foreign_keys_ok: foreign_key_count == 0,
            missing_fts_rows: missing_fts_rows as usize,
            duplicate_active_logical_ids: duplicate_active as usize,
            warnings,
        })
    }

    pub fn rebuild_projections(
        &self,
        target: ProjectionTarget,
    ) -> Result<ProjectionRepairReport, EngineError> {
        self.projections.rebuild_projections(target)
    }

    pub fn rebuild_missing_projections(&self) -> Result<ProjectionRepairReport, EngineError> {
        self.projections.rebuild_missing_projections()
    }

    pub fn trace_source(&self, source_ref: &str) -> Result<TraceReport, EngineError> {
        let conn = sqlite::open_connection(&self.database_path)?;
        self.schema_manager.bootstrap(&conn)?;

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

    pub fn excise_source(&self, source_ref: &str) -> Result<TraceReport, EngineError> {
        let affected: Vec<(String, String)> = {
            let conn = sqlite::open_connection(&self.database_path)?;
            self.schema_manager.bootstrap(&conn)?;

            // Collect (row_id, logical_id) for active rows that will be excised.
            let mut stmt = conn.prepare(
                "SELECT row_id, logical_id FROM nodes \
                 WHERE source_ref = ?1 AND superseded_at IS NULL",
            )?;
            let pairs = stmt
                .query_map([source_ref], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            // Supersede bad rows in all tables.
            conn.execute(
                "UPDATE nodes SET superseded_at = unixepoch() \
                 WHERE source_ref = ?1 AND superseded_at IS NULL",
                [source_ref],
            )?;
            conn.execute(
                "UPDATE edges SET superseded_at = unixepoch() \
                 WHERE source_ref = ?1 AND superseded_at IS NULL",
                [source_ref],
            )?;
            conn.execute(
                "UPDATE actions SET superseded_at = unixepoch() \
                 WHERE source_ref = ?1 AND superseded_at IS NULL",
                [source_ref],
            )?;

            // Restore the most recent prior version for each affected logical_id.
            for (excised_row_id, logical_id) in &pairs {
                let prior: Option<String> = conn
                    .query_row(
                        "SELECT row_id FROM nodes \
                         WHERE logical_id = ?1 AND row_id != ?2 \
                         ORDER BY created_at DESC LIMIT 1",
                        [logical_id.as_str(), excised_row_id.as_str()],
                        |row| row.get(0),
                    )
                    .optional()?;
                if let Some(prior_id) = prior {
                    conn.execute(
                        "UPDATE nodes SET superseded_at = NULL WHERE row_id = ?1",
                        [prior_id.as_str()],
                    )?;
                }
            }

            pairs
        };

        // Rebuild FTS to reflect the restored active state. Uses its own connection.
        self.projections.rebuild_projections(ProjectionTarget::Fts)?;

        let _ = affected;
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
mod tests {
    use std::sync::Arc;

    use fathomdb_schema::SchemaManager;
    use tempfile::NamedTempFile;

    use super::AdminService;
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
