#![allow(dead_code, clippy::expect_used, clippy::missing_panics_doc, clippy::doc_markdown)]

use std::path::Path;

fn open(db_path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(db_path).expect("open db")
}

/// Delete all fts_nodes rows — simulates full projection loss.
pub fn delete_all_fts_rows(db_path: &Path) {
    open(db_path)
        .execute_batch("DELETE FROM fts_nodes")
        .expect("delete_all_fts_rows failed");
}

/// Delete a single fts_nodes row by chunk_id — minimal projection gap.
pub fn delete_fts_row(db_path: &Path, chunk_id: &str) {
    open(db_path)
        .execute("DELETE FROM fts_nodes WHERE chunk_id = ?1", rusqlite::params![chunk_id])
        .expect("delete_fts_row failed");
}

/// Delete a chunk row while leaving its FTS row intact — creates stale_fts_rows.
pub fn delete_chunk_keep_fts(db_path: &Path, chunk_id: &str) {
    open(db_path)
        .execute("DELETE FROM chunks WHERE id = ?1", rusqlite::params![chunk_id])
        .expect("delete_chunk_keep_fts failed");
}

/// Delete an active node row without cleaning up its chunks/FTS rows — creates orphaned_chunks.
pub fn delete_node_leave_chunks(db_path: &Path, logical_id: &str) {
    open(db_path)
        .execute(
            "DELETE FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL",
            rusqlite::params![logical_id],
        )
        .expect("delete_node_leave_chunks failed");
}

/// Supersede a node (set superseded_at) without deleting its FTS rows — creates fts_rows_for_superseded_nodes.
pub fn supersede_node_leave_fts(db_path: &Path, logical_id: &str) {
    open(db_path)
        .execute(
            "UPDATE nodes SET superseded_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE logical_id = ?1 AND superseded_at IS NULL",
            rusqlite::params![logical_id],
        )
        .expect("supersede_node_leave_fts failed");
}

/// Insert a second active row for the same logical_id.
/// Creates: duplicate active logical_ids detectable by check_integrity.
pub fn inject_duplicate_active(db_path: &Path, logical_id: &str, row_id: &str) {
    let conn = open(db_path);
    let (kind, properties, source_ref): (String, String, Option<String>) = conn
        .query_row(
            "SELECT kind, properties, source_ref FROM nodes WHERE logical_id = ?1 AND superseded_at IS NULL",
            rusqlite::params![logical_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("inject_duplicate_active: no active row for logical_id");
    conn.execute(
        "INSERT INTO nodes (row_id, logical_id, kind, properties, source_ref, superseded_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
        rusqlite::params![row_id, logical_id, kind, properties, source_ref],
    )
    .expect("inject_duplicate_active failed");
}
