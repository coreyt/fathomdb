use std::path::{Path, PathBuf};
use std::sync::Arc;

use fathomdb_schema::SchemaManager;
use rusqlite::{OptionalExtension, TransactionBehavior};
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
        trace_info!(target = ?target, "projection rebuild started");
        #[cfg(feature = "tracing")]
        let start = std::time::Instant::now();
        let mut conn = self.connect()?;

        let mut notes = Vec::new();
        let rebuilt_rows = match target {
            ProjectionTarget::Fts => {
                let fts = rebuild_fts(&mut conn)?;
                let prop_fts = rebuild_property_fts(&mut conn)?;
                fts + prop_fts
            }
            ProjectionTarget::Vec => rebuild_vec(&mut conn, &mut notes)?,
            ProjectionTarget::All => {
                let rebuilt_fts = rebuild_fts(&mut conn)?;
                let rebuilt_prop_fts = rebuild_property_fts(&mut conn)?;
                let rebuilt_vec = rebuild_vec(&mut conn, &mut notes)?;
                rebuilt_fts + rebuilt_prop_fts + rebuilt_vec
            }
        };

        trace_info!(
            target = ?target,
            rebuilt_rows,
            duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
            "projection rebuild completed"
        );
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
        let inserted_chunk_fts = tx.execute(
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
        let inserted_prop_fts = rebuild_missing_property_fts_in_tx(&tx)?;
        tx.commit()?;

        Ok(ProjectionRepairReport {
            targets: vec![ProjectionTarget::Fts],
            rebuilt_rows: inserted_chunk_fts + inserted_prop_fts,
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

/// Atomically rebuild the property FTS index from registered schemas and active nodes.
fn rebuild_property_fts(conn: &mut rusqlite::Connection) -> Result<usize, rusqlite::Error> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    // Delete from ALL per-kind FTS virtual tables (including orphaned ones without schemas).
    // Filter by sql LIKE 'CREATE VIRTUAL TABLE%' to exclude FTS5 shadow tables.
    let all_per_kind_tables: Vec<String> = {
        let mut stmt = tx.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'fts_props_%' \
             AND sql LIKE 'CREATE VIRTUAL TABLE%'",
        )?;
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    for table in &all_per_kind_tables {
        tx.execute_batch(&format!("DELETE FROM {table}"))?;
    }
    tx.execute("DELETE FROM fts_node_property_positions", [])?;

    let total = insert_property_fts_rows(
        &tx,
        "SELECT logical_id, properties FROM nodes WHERE kind = ?1 AND superseded_at IS NULL",
    )?;

    tx.commit()?;
    Ok(total)
}

/// Insert missing property FTS rows within an existing transaction.
///
/// Two repair passes run inside the caller's transaction:
///
/// 1. Nodes of a registered kind that have no row in the per-kind FTS tables are
///    re-extracted from canonical state and inserted (blob + positions).
/// 2. Nodes of a recursive-mode kind that *do* have a row in the per-kind FTS tables
///    but no `fts_node_property_positions` rows have their positions
///    regenerated in place. This repairs orphaned position map rows caused
///    by partial drift without requiring a full `rebuild_projections(Fts)`.
///    (P4-P2-2)
fn rebuild_missing_property_fts_in_tx(
    conn: &rusqlite::Connection,
) -> Result<usize, rusqlite::Error> {
    // The per-kind table is parameterized: the SQL is built per-kind in
    // insert_property_fts_rows_missing (below), which passes the table name inline.
    let inserted = insert_property_fts_rows_missing(conn)?;
    let repaired = repair_orphaned_position_map_in_tx(conn)?;
    Ok(inserted + repaired)
}

/// Repair recursive-mode nodes whose per-kind FTS row exists but
/// whose position-map rows have been dropped. For each such node the
/// property FTS is re-extracted from canonical state and the position rows
/// are re-inserted. The blob row is left untouched — callers that deleted
/// positions without touching the blob keep the original blob rowid, which
/// matters because `projection_row_id` in search hits is the blob rowid.
fn repair_orphaned_position_map_in_tx(
    conn: &rusqlite::Connection,
) -> Result<usize, rusqlite::Error> {
    let schemas = crate::writer::load_fts_property_schemas(conn)?;
    if schemas.is_empty() {
        return Ok(0);
    }
    let mut total = 0usize;
    let mut ins_positions = conn.prepare(
        "INSERT INTO fts_node_property_positions \
         (node_logical_id, kind, start_offset, end_offset, leaf_path) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for (kind, schema) in &schemas {
        let has_recursive = schema
            .paths
            .iter()
            .any(|p| p.mode == crate::writer::PropertyPathMode::Recursive);
        if !has_recursive {
            continue;
        }
        let table = fathomdb_schema::fts_kind_table_name(kind);
        // Nodes that have an FTS row in the per-kind table but no position-map rows.
        let mut stmt = conn.prepare(&format!(
            "SELECT n.logical_id, n.properties FROM nodes n \
             WHERE n.kind = ?1 AND n.superseded_at IS NULL \
               AND EXISTS (SELECT 1 FROM {table} fp \
                           WHERE fp.node_logical_id = n.logical_id) \
               AND NOT EXISTS (SELECT 1 FROM fts_node_property_positions p \
                               WHERE p.node_logical_id = n.logical_id AND p.kind = ?1)"
        ))?;
        let rows: Vec<(String, String)> = stmt
            .query_map([kind.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (logical_id, properties_str) in &rows {
            let props: serde_json::Value = serde_json::from_str(properties_str).unwrap_or_default();
            let (_text, positions, _stats) = crate::writer::extract_property_fts(&props, schema);
            for pos in &positions {
                ins_positions.execute(rusqlite::params![
                    logical_id,
                    kind,
                    i64::try_from(pos.start_offset).unwrap_or(i64::MAX),
                    i64::try_from(pos.end_offset).unwrap_or(i64::MAX),
                    pos.leaf_path,
                ])?;
            }
            if !positions.is_empty() {
                total += 1;
            }
        }
    }
    Ok(total)
}

/// Rebuild property FTS rows for exactly one kind from its just-registered
/// schema. Unlike [`insert_property_fts_rows`], this helper does NOT iterate
/// over every registered schema — so callers that delete rows for a single
/// kind won't duplicate rows for sibling kinds on the subsequent insert.
///
/// The caller is responsible for transaction management and for deleting
/// stale rows for `kind` before calling this function.
pub(crate) fn insert_property_fts_rows_for_kind(
    conn: &rusqlite::Connection,
    kind: &str,
) -> Result<usize, rusqlite::Error> {
    let schemas = crate::writer::load_fts_property_schemas(conn)?;
    let Some(schema) = schemas
        .iter()
        .find(|(k, _)| k == kind)
        .map(|(_, s)| s.clone())
    else {
        return Ok(0);
    };

    let table = fathomdb_schema::fts_kind_table_name(kind);
    ensure_property_fts_table(conn, kind, &schema)?;
    let has_weights = schema.paths.iter().any(|p| p.weight.is_some());
    let mut ins_positions = conn.prepare(
        "INSERT INTO fts_node_property_positions \
         (node_logical_id, kind, start_offset, end_offset, leaf_path) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;

    let mut stmt = conn.prepare(
        "SELECT logical_id, properties FROM nodes \
         WHERE kind = ?1 AND superseded_at IS NULL",
    )?;
    let rows: Vec<(String, String)> = stmt
        .query_map([kind], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut total = 0usize;
    for (logical_id, properties_str) in &rows {
        let props: serde_json::Value = serde_json::from_str(properties_str).unwrap_or_default();
        let (text, positions, _stats) = crate::writer::extract_property_fts(&props, &schema);
        if let Some(text) = text {
            if has_weights {
                let cols = crate::writer::extract_property_fts_columns(&props, &schema);
                let col_names: Vec<&str> = cols.iter().map(|(n, _)| n.as_str()).collect();
                let placeholders: Vec<String> =
                    (2..=cols.len() + 1).map(|i| format!("?{i}")).collect();
                let sql = format!(
                    "INSERT INTO {table}(node_logical_id, {c}) VALUES (?1, {p})",
                    c = col_names.join(", "),
                    p = placeholders.join(", "),
                );
                conn.prepare(&sql)?.execute(rusqlite::params_from_iter(
                    std::iter::once(logical_id.as_str())
                        .chain(cols.iter().map(|(_, v)| v.as_str())),
                ))?;
            } else {
                conn.prepare(&format!(
                    "INSERT INTO {table} (node_logical_id, text_content) VALUES (?1, ?2)"
                ))?
                .execute(rusqlite::params![logical_id, text])?;
            }
            for pos in &positions {
                ins_positions.execute(rusqlite::params![
                    logical_id,
                    kind,
                    i64::try_from(pos.start_offset).unwrap_or(i64::MAX),
                    i64::try_from(pos.end_offset).unwrap_or(i64::MAX),
                    pos.leaf_path,
                ])?;
            }
            total += 1;
        }
    }
    Ok(total)
}

/// Shared loop: load schemas, query nodes with `node_sql` (parameterized by kind),
/// extract property FTS text, and insert into the per-kind FTS table.
/// The caller is responsible for transaction management and for deleting stale rows
/// before calling this function if a full rebuild is intended.
pub(crate) fn insert_property_fts_rows(
    conn: &rusqlite::Connection,
    node_sql: &str,
) -> Result<usize, rusqlite::Error> {
    let schemas = crate::writer::load_fts_property_schemas(conn)?;
    if schemas.is_empty() {
        return Ok(0);
    }

    let mut total = 0usize;
    let mut ins_positions = conn.prepare(
        "INSERT INTO fts_node_property_positions \
         (node_logical_id, kind, start_offset, end_offset, leaf_path) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for (kind, schema) in &schemas {
        let table = fathomdb_schema::fts_kind_table_name(kind);
        ensure_property_fts_table(conn, kind, schema)?;
        let has_weights = schema.paths.iter().any(|p| p.weight.is_some());
        let mut stmt = conn.prepare(node_sql)?;
        let rows: Vec<(String, String)> = stmt
            .query_map([kind.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (logical_id, properties_str) in &rows {
            let props: serde_json::Value = serde_json::from_str(properties_str).unwrap_or_default();
            let (text, positions, _stats) = crate::writer::extract_property_fts(&props, schema);
            if let Some(text) = text {
                if has_weights {
                    let cols = crate::writer::extract_property_fts_columns(&props, schema);
                    let col_names: Vec<&str> = cols.iter().map(|(n, _)| n.as_str()).collect();
                    let placeholders: Vec<String> =
                        (2..=cols.len() + 1).map(|i| format!("?{i}")).collect();
                    let sql = format!(
                        "INSERT INTO {table}(node_logical_id, {c}) VALUES (?1, {p})",
                        c = col_names.join(", "),
                        p = placeholders.join(", "),
                    );
                    conn.prepare(&sql)?.execute(rusqlite::params_from_iter(
                        std::iter::once(logical_id.as_str())
                            .chain(cols.iter().map(|(_, v)| v.as_str())),
                    ))?;
                } else {
                    conn.prepare(&format!(
                        "INSERT INTO {table} (node_logical_id, text_content) VALUES (?1, ?2)"
                    ))?
                    .execute(rusqlite::params![logical_id, text])?;
                }
                for pos in &positions {
                    ins_positions.execute(rusqlite::params![
                        logical_id,
                        kind,
                        i64::try_from(pos.start_offset).unwrap_or(i64::MAX),
                        i64::try_from(pos.end_offset).unwrap_or(i64::MAX),
                        pos.leaf_path,
                    ])?;
                }
                total += 1;
            }
        }
    }
    Ok(total)
}

/// Insert missing property FTS rows: for each registered kind, find nodes that
/// have no row in the per-kind FTS table and insert them.
/// The caller is responsible for transaction management.
fn insert_property_fts_rows_missing(conn: &rusqlite::Connection) -> Result<usize, rusqlite::Error> {
    let schemas = crate::writer::load_fts_property_schemas(conn)?;
    if schemas.is_empty() {
        return Ok(0);
    }

    let mut total = 0usize;
    let mut ins_positions = conn.prepare(
        "INSERT INTO fts_node_property_positions \
         (node_logical_id, kind, start_offset, end_offset, leaf_path) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for (kind, schema) in &schemas {
        let table = fathomdb_schema::fts_kind_table_name(kind);
        ensure_property_fts_table(conn, kind, schema)?;
        let has_weights = schema.paths.iter().any(|p| p.weight.is_some());
        // Find nodes of this kind with no row in the per-kind table.
        let mut stmt = conn.prepare(&format!(
            "SELECT n.logical_id, n.properties FROM nodes n \
             WHERE n.kind = ?1 AND n.superseded_at IS NULL \
               AND NOT EXISTS (SELECT 1 FROM {table} fp WHERE fp.node_logical_id = n.logical_id)"
        ))?;
        let rows: Vec<(String, String)> = stmt
            .query_map([kind.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (logical_id, properties_str) in &rows {
            let props: serde_json::Value = serde_json::from_str(properties_str).unwrap_or_default();
            let (text, positions, _stats) = crate::writer::extract_property_fts(&props, schema);
            if let Some(text) = text {
                if has_weights {
                    let cols = crate::writer::extract_property_fts_columns(&props, schema);
                    let col_names: Vec<&str> = cols.iter().map(|(n, _)| n.as_str()).collect();
                    let placeholders: Vec<String> =
                        (2..=cols.len() + 1).map(|i| format!("?{i}")).collect();
                    let sql = format!(
                        "INSERT INTO {table}(node_logical_id, {c}) VALUES (?1, {p})",
                        c = col_names.join(", "),
                        p = placeholders.join(", "),
                    );
                    conn.prepare(&sql)?.execute(rusqlite::params_from_iter(
                        std::iter::once(logical_id.as_str())
                            .chain(cols.iter().map(|(_, v)| v.as_str())),
                    ))?;
                } else {
                    conn.prepare(&format!(
                        "INSERT INTO {table} (node_logical_id, text_content) VALUES (?1, ?2)"
                    ))?
                    .execute(rusqlite::params![logical_id, text])?;
                }
                for pos in &positions {
                    ins_positions.execute(rusqlite::params![
                        logical_id,
                        kind,
                        i64::try_from(pos.start_offset).unwrap_or(i64::MAX),
                        i64::try_from(pos.end_offset).unwrap_or(i64::MAX),
                        pos.leaf_path,
                    ])?;
                }
                total += 1;
            }
        }
    }
    Ok(total)
}

fn ensure_property_fts_table(
    conn: &rusqlite::Connection,
    kind: &str,
    schema: &crate::writer::PropertyFtsSchema,
) -> Result<(), rusqlite::Error> {
    let table = fathomdb_schema::fts_kind_table_name(kind);
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 \
             AND sql LIKE 'CREATE VIRTUAL TABLE%'",
            rusqlite::params![table],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if exists {
        return Ok(());
    }

    let tokenizer = fathomdb_schema::resolve_fts_tokenizer(conn, kind)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    let tokenizer_sql = tokenizer.replace('\'', "''");
    let has_weights = schema.paths.iter().any(|p| p.weight.is_some());
    let cols: Vec<String> = if has_weights {
        std::iter::once("node_logical_id UNINDEXED".to_owned())
            .chain(schema.paths.iter().map(|p| {
                let is_recursive = matches!(p.mode, crate::writer::PropertyPathMode::Recursive);
                fathomdb_schema::fts_column_name(&p.path, is_recursive)
            }))
            .collect()
    } else {
        vec![
            "node_logical_id UNINDEXED".to_owned(),
            "text_content".to_owned(),
        ]
    };
    conn.execute_batch(&format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS {table} USING fts5({cols}, tokenize='{tokenizer_sql}')",
        cols = cols.join(", "),
    ))?;
    Ok(())
}

/// Remove stale vec rows: entries whose chunk no longer exists or whose node has been
/// superseded/retired.  Iterates all per-kind vec tables registered in
/// `projection_profiles`.  Degrades gracefully when the feature is disabled or tables
/// are absent.
#[allow(clippy::unnecessary_wraps, unused_variables)]
fn rebuild_vec(
    conn: &mut rusqlite::Connection,
    notes: &mut Vec<String>,
) -> Result<usize, rusqlite::Error> {
    #[cfg(feature = "sqlite-vec")]
    {
        let kinds: Vec<String> = {
            let mut stmt =
                match conn.prepare("SELECT kind FROM projection_profiles WHERE facet = 'vec'") {
                    Ok(s) => s,
                    Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                        if msg.contains("no such table: projection_profiles") =>
                    {
                        notes.push("projection_profiles absent; vec rebuild skipped".to_owned());
                        return Ok(0);
                    }
                    Err(e) => return Err(e),
                };
            stmt.query_map([], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?
        };

        if kinds.is_empty() {
            notes.push("no vec profiles registered; vec rebuild skipped".to_owned());
            return Ok(0);
        }

        let mut total = 0;
        for kind in &kinds {
            let table = fathomdb_schema::vec_kind_table_name(kind);
            let sql = format!(
                "DELETE FROM {table} WHERE chunk_id IN (
                    SELECT v.chunk_id FROM {table} v
                    LEFT JOIN chunks c ON c.id = v.chunk_id
                    LEFT JOIN nodes  n ON n.logical_id = c.node_logical_id
                    WHERE c.id IS NULL OR n.superseded_at IS NOT NULL
                )"
            );
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let deleted = match tx.execute(&sql, []) {
                Ok(n) => n,
                Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                    if msg.contains("no such table:") || msg.contains("no such module: vec0") =>
                {
                    notes.push(format!(
                        "{table} absent; vec rebuild for kind '{kind}' skipped"
                    ));
                    tx.rollback()?;
                    continue;
                }
                Err(e) => return Err(e),
            };
            tx.commit()?;
            total += deleted;
        }
        Ok(total)
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
                .ensure_vec_kind_profile(&conn, "Doc", 3)
                .expect("vec kind profile");

            // Insert a superseded node + chunk + vec row (stale state).
            conn.execute_batch(
                r"
                INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, superseded_at)
                VALUES ('row-old', 'lg-stale', 'Doc', '{}', 100, 200);
                INSERT INTO chunks (id, node_logical_id, text_content, created_at)
                VALUES ('chunk-stale', 'lg-stale', 'old text', 100);
                ",
            )
            .expect("seed stale data");

            let bytes: Vec<u8> = [0.1f32, 0.2f32, 0.3f32]
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();
            let vec_table = fathomdb_schema::vec_kind_table_name("Doc");
            conn.execute(
                &format!(
                    "INSERT INTO {vec_table} (chunk_id, embedding) VALUES ('chunk-stale', ?1)"
                ),
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
        let vec_table = fathomdb_schema::vec_kind_table_name("Doc");
        let count: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM {vec_table} WHERE chunk_id = 'chunk-stale'"),
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 0, "stale vec row must be gone after rebuild");
    }
}

// --- B-3: projection per-column INSERT for weighted schemas ---

#[cfg(test)]
#[allow(clippy::expect_used)]
mod weighted_schema_tests {
    use fathomdb_schema::SchemaManager;
    use rusqlite::Connection;

    use super::insert_property_fts_rows_for_kind;

    fn bootstrapped_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        let manager = SchemaManager::new();
        manager.bootstrap(&conn).expect("bootstrap");
        conn
    }

    #[test]
    fn projection_inserts_per_column_for_weighted_schema() {
        let conn = bootstrapped_conn();
        let kind = "Article";
        let table = fathomdb_schema::fts_kind_table_name(kind);
        let title_col = fathomdb_schema::fts_column_name("$.title", false);
        let body_col = fathomdb_schema::fts_column_name("$.body", false);

        // Insert a node with two extractable properties.
        conn.execute(
            "INSERT INTO nodes (row_id, logical_id, kind, properties, created_at, source_ref) \
             VALUES ('row-1', 'article-1', ?1, '{\"title\":\"Hello\",\"body\":\"World\"}', 100, 'seed')",
            rusqlite::params![kind],
        )
        .expect("insert node");

        // Register schema with weights.
        let paths_json = r#"[{"path":"$.title","mode":"scalar","weight":2.0},{"path":"$.body","mode":"scalar","weight":1.0}]"#;
        conn.execute(
            "INSERT INTO fts_property_schemas (kind, property_paths_json, separator) \
             VALUES (?1, ?2, ' ')",
            rusqlite::params![kind, paths_json],
        )
        .expect("insert schema");

        // Create the weighted per-kind FTS table.
        conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS {table} USING fts5(\
                node_logical_id UNINDEXED, {title_col}, {body_col}, \
                tokenize = 'porter unicode61 remove_diacritics 2'\
            )"
        ))
        .expect("create weighted per-kind table");

        // Run the projection insert.
        insert_property_fts_rows_for_kind(&conn, kind).expect("insert_property_fts_rows_for_kind");

        // Verify one row was inserted.
        let count: i64 = conn
            .query_row(
                &format!("SELECT count(*) FROM {table} WHERE node_logical_id = 'article-1'"),
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(count, 1, "per-kind table must have the inserted row");

        // Verify per-column values.
        let (title_val, body_val): (String, String) = conn
            .query_row(
                &format!(
                    "SELECT {title_col}, {body_col} FROM {table} \
                     WHERE node_logical_id = 'article-1'"
                ),
                [],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .expect("select per-column");
        assert_eq!(title_val, "Hello", "title column must have correct value");
        assert_eq!(body_val, "World", "body column must have correct value");
    }
}
