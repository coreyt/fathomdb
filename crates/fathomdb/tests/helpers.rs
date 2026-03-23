#![allow(dead_code, clippy::expect_used, clippy::missing_panics_doc, clippy::must_use_candidate, clippy::doc_markdown)]

use std::path::Path;

/// Count all rows in a named table.
pub fn count_rows(db_path: &Path, table: &str) -> i64 {
    let conn = rusqlite::Connection::open(db_path).expect("open db");
    conn.query_row(
        &format!("SELECT count(*) FROM {table}"),
        [],
        |row| row.get(0),
    )
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
