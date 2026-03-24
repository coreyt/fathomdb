use std::path::{Path, PathBuf};
use std::sync::Arc;

use fathomdb_schema::SchemaManager;
use rusqlite::TransactionBehavior;
use serde::Serialize;

use crate::{EngineError, sqlite};

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

    fn connect(&self) -> Result<rusqlite::Connection, EngineError> {
        let conn = sqlite::open_connection(&self.database_path)?;
        self.schema_manager.bootstrap(&conn)?;
        Ok(conn)
    }

    pub fn rebuild_projections(
        &self,
        target: ProjectionTarget,
    ) -> Result<ProjectionRepairReport, EngineError> {
        let mut conn = self.connect()?;

        let mut notes = Vec::new();
        let rebuilt_rows = match target {
            ProjectionTarget::Fts => rebuild_fts(&mut conn)?,
            ProjectionTarget::Vec => {
                notes.push(
                    "vector projection rebuild is deferred until sqlite-vec is enabled".to_owned(),
                );
                0
            }
            ProjectionTarget::All => {
                let rebuilt_fts = rebuild_fts(&mut conn)?;
                notes.push(
                    "vector projection rebuild is deferred until sqlite-vec is enabled".to_owned(),
                );
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
        // FIX(review): was bare execute without explicit transaction.
        // Options: (A) IMMEDIATE tx matching rebuild_fts(), (B) DEFERRED tx, (C) leave as-is
        // (autocommit wraps single statements atomically). Chose (A): explicit transaction
        // communicates intent, matches sibling rebuild_fts(), and protects against future
        // refactoring that might add additional statements.
        let mut conn = self.connect()?;

        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let inserted = tx.execute(
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
        tx.commit()?;

        Ok(ProjectionRepairReport {
            targets: vec![ProjectionTarget::Fts],
            rebuilt_rows: inserted,
            notes: vec!["vector projection backfill remains deferred".to_owned()],
        })
    }
}

/// Atomically rebuild the FTS index: delete all existing rows and repopulate
/// from the canonical `chunks`/`nodes` join.  The DELETE and INSERT are
/// wrapped in a single `IMMEDIATE` transaction so a mid-rebuild failure
/// cannot leave the index empty.
fn rebuild_fts(conn: &mut rusqlite::Connection) -> Result<usize, rusqlite::Error> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute("DELETE FROM fts_nodes", [])?;
    let inserted = tx.execute(
        r#"
        INSERT INTO fts_nodes (chunk_id, node_logical_id, kind, text_content)
        SELECT c.id, n.logical_id, n.kind, c.text_content
        FROM chunks c
        JOIN nodes n
          ON n.logical_id = c.node_logical_id
         AND n.superseded_at IS NULL
        "#,
        [],
    )?;
    tx.commit()?;
    Ok(inserted)
}

fn expand_targets(target: ProjectionTarget) -> Vec<ProjectionTarget> {
    match target {
        ProjectionTarget::Fts => vec![ProjectionTarget::Fts],
        ProjectionTarget::Vec => vec![ProjectionTarget::Vec],
        ProjectionTarget::All => vec![ProjectionTarget::Fts, ProjectionTarget::Vec],
    }
}
