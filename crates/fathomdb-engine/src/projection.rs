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

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails or the projection rebuild fails.
    pub fn rebuild_projections(
        &self,
        target: ProjectionTarget,
    ) -> Result<ProjectionRepairReport, EngineError> {
        let mut conn = self.connect()?;

        let mut notes = Vec::new();
        let rebuilt_rows = match target {
            ProjectionTarget::Fts => rebuild_fts(&mut conn)?,
            ProjectionTarget::Vec => rebuild_vec(&mut conn, &mut notes)?,
            ProjectionTarget::All => {
                let rebuilt_fts = rebuild_fts(&mut conn)?;
                let rebuilt_vec = rebuild_vec(&mut conn, &mut notes)?;
                rebuilt_fts + rebuilt_vec
            }
        };

        Ok(ProjectionRepairReport {
            targets: expand_targets(target),
            rebuilt_rows,
            notes,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database connection fails or the INSERT query fails.
    pub fn rebuild_missing_projections(&self) -> Result<ProjectionRepairReport, EngineError> {
        // FIX(review): was bare execute without explicit transaction.
        // Options: (A) IMMEDIATE tx matching rebuild_fts(), (B) DEFERRED tx, (C) leave as-is
        // (autocommit wraps single statements atomically). Chose (A): explicit transaction
        // communicates intent, matches sibling rebuild_fts(), and protects against future
        // refactoring that might add additional statements.
        let mut conn = self.connect()?;

        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let inserted = tx.execute(
            r"
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
            ",
            [],
        )?;
        tx.commit()?;

        Ok(ProjectionRepairReport {
            targets: vec![ProjectionTarget::Fts],
            rebuilt_rows: inserted,
            notes: vec![],
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
    tx.commit()?;
    Ok(inserted)
}

/// Remove stale vec rows: entries whose chunk no longer exists or whose node has been
/// superseded/retired.  When the `sqlite-vec` feature is disabled or the
/// `vec_nodes_active` table is absent, degrades gracefully to a no-op and appends a note.
#[allow(clippy::unnecessary_wraps, unused_variables)]
fn rebuild_vec(
    conn: &mut rusqlite::Connection,
    notes: &mut Vec<String>,
) -> Result<usize, rusqlite::Error> {
    #[cfg(feature = "sqlite-vec")]
    {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let deleted = match tx.execute(
            r"
            DELETE FROM vec_nodes_active WHERE chunk_id IN (
                SELECT v.chunk_id FROM vec_nodes_active v
                LEFT JOIN chunks c ON c.id = v.chunk_id
                LEFT JOIN nodes  n ON n.logical_id = c.node_logical_id
                WHERE c.id IS NULL OR n.superseded_at IS NOT NULL
            )
            ",
            [],
        ) {
            Ok(n) => n,
            Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                if msg.contains("vec_nodes_active") || msg.contains("vec0") =>
            {
                notes.push("vec_nodes_active table absent; vec rebuild skipped".to_owned());
                tx.rollback()?;
                return Ok(0);
            }
            Err(e) => return Err(e),
        };
        tx.commit()?;
        Ok(deleted)
    }
    #[cfg(not(feature = "sqlite-vec"))]
    {
        notes.push("vector projection rebuild skipped: sqlite-vec feature not enabled".to_owned());
        Ok(0)
    }
}

fn expand_targets(target: ProjectionTarget) -> Vec<ProjectionTarget> {
    match target {
        ProjectionTarget::Fts => vec![ProjectionTarget::Fts],
        ProjectionTarget::Vec => vec![ProjectionTarget::Vec],
        ProjectionTarget::All => vec![ProjectionTarget::Fts, ProjectionTarget::Vec],
    }
}

#[cfg(all(test, feature = "sqlite-vec"))]
#[allow(clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use fathomdb_schema::SchemaManager;
    use tempfile::NamedTempFile;

    use crate::sqlite::open_connection_with_vec;

    use super::{ProjectionService, ProjectionTarget};

    #[test]
    fn rebuild_vec_removes_stale_vec_rows_for_superseded_nodes() {
        let db = NamedTempFile::new().expect("temp db");
        let schema = Arc::new(SchemaManager::new());

        {
            let conn = open_connection_with_vec(db.path()).expect("vec conn");
            schema.bootstrap(&conn).expect("bootstrap");
            schema
                .ensure_vector_profile(&conn, "default", "vec_nodes_active", 3)
                .expect("vec profile");

            // Insert a superseded node + chunk + vec row (stale state).
            conn.execute_batch(
                r#"
                INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, superseded_at)
                VALUES ('row-old', 'lg-stale', 'Doc', '{}', 100, 200);
                INSERT INTO chunks (id, node_logical_id, text_content, created_at)
                VALUES ('chunk-stale', 'lg-stale', 'old text', 100);
                "#,
            )
            .expect("seed stale data");

            let bytes: Vec<u8> = [0.1f32, 0.2f32, 0.3f32]
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();
            conn.execute(
                "INSERT INTO vec_nodes_active (chunk_id, embedding) VALUES ('chunk-stale', ?1)",
                rusqlite::params![bytes],
            )
            .expect("insert stale vec row");
        }

        let service = ProjectionService::new(db.path(), Arc::clone(&schema));
        let report = service
            .rebuild_projections(ProjectionTarget::Vec)
            .expect("rebuild vec");

        assert_eq!(report.rebuilt_rows, 1, "one stale vec row must be removed");
        assert!(report.notes.is_empty(), "no notes expected on success");

        let conn = rusqlite::Connection::open(db.path()).expect("conn");
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM vec_nodes_active WHERE chunk_id = 'chunk-stale'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 0, "stale vec row must be gone after rebuild");
    }
}
