use std::path::{Path, PathBuf};
use std::sync::Arc;

use fathomdb_schema::SchemaManager;
use serde::Serialize;

use crate::{sqlite, EngineError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ProjectionTarget {
    Fts,
    Vec,
    All,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProjectionRepairReport {
    pub targets: Vec<ProjectionTarget>,
    pub rebuilt_rows: usize,
    pub notes: Vec<String>,
}

#[derive(Debug)]
pub struct ProjectionService {
    database_path: PathBuf,
    schema_manager: Arc<SchemaManager>,
}

impl ProjectionService {
    pub fn new(path: impl AsRef<Path>, schema_manager: Arc<SchemaManager>) -> Self {
        Self {
            database_path: path.as_ref().to_path_buf(),
            schema_manager,
        }
    }

    pub fn rebuild_projections(&self, target: ProjectionTarget) -> Result<ProjectionRepairReport, EngineError> {
        let conn = sqlite::open_connection(&self.database_path)?;
        self.schema_manager.bootstrap(&conn)?;

        let mut notes = Vec::new();
        let rebuilt_rows = match target {
            ProjectionTarget::Fts => rebuild_fts(&conn)?,
            ProjectionTarget::Vec => {
                notes.push("vector projection rebuild is deferred until sqlite-vec is enabled".to_owned());
                0
            }
            ProjectionTarget::All => {
                let rebuilt_fts = rebuild_fts(&conn)?;
                notes.push("vector projection rebuild is deferred until sqlite-vec is enabled".to_owned());
                rebuilt_fts
            }
        };

        Ok(ProjectionRepairReport {
            targets: expand_targets(target),
            rebuilt_rows,
            notes,
        })
    }

    pub fn rebuild_missing_projections(&self) -> Result<ProjectionRepairReport, EngineError> {
        let conn = sqlite::open_connection(&self.database_path)?;
        self.schema_manager.bootstrap(&conn)?;

        let inserted = conn.execute(
            r#"
            INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
            SELECT c.id, n.logical_id, n.kind, c.text_content
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
        )?;

        Ok(ProjectionRepairReport {
            targets: vec![ProjectionTarget::Fts],
            rebuilt_rows: inserted,
            notes: vec!["vector projection backfill remains deferred".to_owned()],
        })
    }
}

fn rebuild_fts(conn: &rusqlite::Connection) -> Result<usize, rusqlite::Error> {
    conn.execute("DELETE FROM fts_nodes", [])?;
    conn.execute(
        r#"
        INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
        SELECT c.id, n.logical_id, n.kind, c.text_content
        FROM chunks c
        JOIN nodes n
          ON n.logical_id = c.node_logical_id
         AND n.superseded_at IS NULL
        "#,
        [],
    )
}

fn expand_targets(target: ProjectionTarget) -> Vec<ProjectionTarget> {
    match target {
        ProjectionTarget::Fts => vec![ProjectionTarget::Fts],
        ProjectionTarget::Vec => vec![ProjectionTarget::Vec],
        ProjectionTarget::All => vec![ProjectionTarget::Fts, ProjectionTarget::Vec],
    }
}
