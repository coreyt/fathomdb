#![allow(
    dead_code,
    clippy::expect_used,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::doc_markdown
)]

use std::path::Path;

/// Count all rows in a named table.
pub fn count_rows(db_path: &Path, table: &str) -> i64 {
    let conn = rusqlite::Connection::open(db_path).expect("open db");
    conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap_or(0)
}

/// Count active rows (superseded_at IS NULL) for a logical_id in nodes or edges.
pub fn active_count(db_path: &Path, table: &str, logical_id: &str) -> i64 {
    let conn = rusqlite::Connection::open(db_path).expect("open db");
    conn.query_row(
        &format!("SELECT count(*) FROM {table} WHERE logical_id = ?1 AND superseded_at IS NULL"),
        rusqlite::params![logical_id],
        |row| row.get(0),
    )
    .unwrap_or(0)
}

/// Count historical rows (superseded_at IS NOT NULL) for a logical_id.
pub fn historical_count(db_path: &Path, table: &str, logical_id: &str) -> i64 {
    let conn = rusqlite::Connection::open(db_path).expect("open db");
    conn.query_row(
        &format!(
            "SELECT count(*) FROM {table} WHERE logical_id = ?1 AND superseded_at IS NOT NULL"
        ),
        rusqlite::params![logical_id],
        |row| row.get(0),
    )
    .unwrap_or(0)
}

/// Fetch the properties JSON of the active row for a logical_id.
pub fn active_properties(db_path: &Path, logical_id: &str) -> Option<String> {
    let conn = rusqlite::Connection::open(db_path).expect("open db");
    conn.query_row(
        "SELECT properties FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL",
        rusqlite::params![logical_id],
        |row| row.get(0),
    )
    .ok()
}

/// Count fts_nodes rows for a given node_logical_id.
pub fn fts_row_count(db_path: &Path, node_logical_id: &str) -> i64 {
    let conn = rusqlite::Connection::open(db_path).expect("open db");
    conn.query_row(
        "SELECT count(*) FROM fts_nodes WHERE node_logical_id = ?1",
        rusqlite::params![node_logical_id],
        |row| row.get(0),
    )
    .unwrap_or(0)
}

/// Count chunks for a given node_logical_id.
pub fn chunk_count(db_path: &Path, node_logical_id: &str) -> i64 {
    let conn = rusqlite::Connection::open(db_path).expect("open db");
    conn.query_row(
        "SELECT count(*) FROM chunks WHERE node_logical_id = ?1",
        rusqlite::params![node_logical_id],
        |row| row.get(0),
    )
    .unwrap_or(0)
}

/// Execute an arbitrary SQL statement (for injection helpers only).
/// Only use in test contexts.
pub fn exec_sql(db_path: &Path, sql: &str) {
    let conn = rusqlite::Connection::open(db_path).expect("open db");
    conn.execute_batch(sql).expect("exec_sql failed");
}

/// Execute an arbitrary SQL statement with one text parameter.
pub fn exec_sql1(db_path: &Path, sql: &str, param: &str) {
    let conn = rusqlite::Connection::open(db_path).expect("open db");
    conn.execute(sql, rusqlite::params![param])
        .expect("exec_sql1 failed");
}

/// Count rows in the `runs` table.
pub fn run_count(db_path: &Path) -> i64 {
    count_rows(db_path, "runs")
}

/// Count rows in the `steps` table.
pub fn step_count(db_path: &Path) -> i64 {
    count_rows(db_path, "steps")
}

/// Count rows in the `actions` table.
pub fn action_count(db_path: &Path) -> i64 {
    count_rows(db_path, "actions")
}

/// All persisted fields for an active node row.
pub struct NodeFields {
    pub row_id: String,
    pub logical_id: String,
    pub kind: String,
    pub properties: String,
    pub source_ref: Option<String>,
    pub created_at: i64,
    pub superseded_at: Option<i64>,
}

/// Fetch all persisted fields for the active node with the given logical_id.
pub fn node_fields(db_path: &Path, logical_id: &str) -> NodeFields {
    let conn = rusqlite::Connection::open(db_path).expect("open db");
    conn.query_row(
        "SELECT row_id, logical_id, kind, properties, source_ref, created_at, superseded_at \
         FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL",
        rusqlite::params![logical_id],
        |row| {
            Ok(NodeFields {
                row_id: row.get(0)?,
                logical_id: row.get(1)?,
                kind: row.get(2)?,
                properties: row.get(3)?,
                source_ref: row.get(4)?,
                created_at: row.get(5)?,
                superseded_at: row.get(6)?,
            })
        },
    )
    .expect("active node not found")
}

/// All persisted fields for a chunk row.
pub struct ChunkFields {
    pub id: String,
    pub node_logical_id: String,
    pub text_content: String,
    pub created_at: i64,
}

/// Fetch all persisted fields for a chunk by its id.
pub fn chunk_fields(db_path: &Path, chunk_id: &str) -> ChunkFields {
    let conn = rusqlite::Connection::open(db_path).expect("open db");
    conn.query_row(
        "SELECT id, node_logical_id, text_content, created_at FROM chunks WHERE id = ?1",
        rusqlite::params![chunk_id],
        |row| {
            Ok(ChunkFields {
                id: row.get(0)?,
                node_logical_id: row.get(1)?,
                text_content: row.get(2)?,
                created_at: row.get(3)?,
            })
        },
    )
    .expect("chunk not found")
}
