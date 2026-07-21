//! 0.8.20 Slice 15c (TC-33) fix-6 — the step-23 `canonical_edges` recreate must
//! not ORPHAN the dropped edges' VECTOR projection.
//!
//! **The finding (codex §9 P1).** Step 23 drops every `canonical_edges` row (NO
//! DATA MIGRATION, HITL 2026-07-21). Before fix-6 it cleared the edge FTS
//! projection (`search_index_edges`) and recorded projection terminals (fix-4),
//! but it never removed the dropped edges' rows from the VECTOR projection. That
//! projection has two row-owned halves (engine `ROW_OWNED_PROJECTIONS`, class
//! `Vector`): the `_fathomdb_vector_rows` sidecar/registry table, and the vec0
//! `vector_default` virtual table that actually feeds KNN candidate selection. An
//! orphaned edge vector row (whose `canonical_edges` row is gone) still occupies a
//! top-K candidate slot and is discarded at hydration, so an upgraded DB returns
//! too few / no vector results even though valid node vectors sit just below the
//! cutoff.
//!
//! **This file covers the MIGRATION half** — the `_fathomdb_vector_rows` sidecar
//! delete, which step 23 owns (that table is created by migration step 6 and
//! always exists during migration). The vec0 `vector_default` half cannot be
//! referenced in the migration SQL (it is engine-created and dim-parameterized,
//! and does not exist in the schema crate's migration context), so it is pruned by
//! the ENGINE right after `ensure_vector_partition` and is covered by the
//! `tc33_fix6_edge_vector_prune` engine test.
//!
//! Assertions are on RAW table contents (the property is at-rest): the dropped
//! edge's sidecar row is gone AND the node's sidecar row is untouched (proving the
//! delete is scoped to edge cursors, not an over-broad truncate — nodes and edges
//! share the single `_fathomdb_vector_rows` table).

use fathomdb_schema::{migrate_with_steps, MIGRATIONS};
use rusqlite::Connection;
use std::sync::Once;

// Step 9 references a vec0 virtual table's extension; register sqlite-vec once so
// the migration steps that touch vec0-adjacent state parse.
fn register_sqlite_vec_once() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| unsafe {
        let entrypoint: unsafe extern "C" fn(
            *mut rusqlite::ffi::sqlite3,
            *mut *const std::os::raw::c_char,
            *const rusqlite::ffi::sqlite3_api_routines,
        ) -> std::os::raw::c_int = std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ());
        rusqlite::ffi::sqlite3_auto_extension(Some(entrypoint));
    });
}

fn user_version(conn: &Connection) -> u32 {
    conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0)).unwrap()
}

fn set_user_version(conn: &Connection, version: u32) {
    conn.pragma_update(None, "user_version", version).unwrap();
}

fn steps_through(limit: u32) -> Vec<fathomdb_schema::Migration> {
    MIGRATIONS.iter().filter(|m| m.step_id <= limit).cloned().collect()
}

fn sidecar_has(conn: &Connection, write_cursor: u64) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM _fathomdb_vector_rows WHERE write_cursor = ?1)",
        [write_cursor],
        |row| row.get::<_, i64>(0),
    )
    .unwrap()
        != 0
}

/// Build a v22 DB with a body-bearing, VECTOR-projected edge at cursor 5 (a row
/// in `_fathomdb_vector_rows`, kind `edge_fact`) and a VECTOR-projected node at
/// cursor 6 (kind `doc`). Both cursors are disjoint values of the single shared
/// `write_cursor` sequence.
fn seed_v22_with_vector_projected_edge() -> Connection {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, &steps_through(22)).expect("migrate to v22");
    assert_eq!(user_version(&conn), 22, "precondition: DB at v22 (pre-step-23)");

    // cursor 5 — body-bearing edge with a VECTOR sidecar row.
    conn.execute(
        "INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id, body)
             VALUES(5, 'relates_to', 'a', 'b', 'edge fact with a vector')",
        [],
    )
    .expect("seed edge at cursor 5");
    conn.execute(
        "INSERT INTO _fathomdb_vector_rows(rowid, kind, write_cursor)
             VALUES(5, 'edge_fact', 5)",
        [],
    )
    .expect("seed edge vector sidecar at cursor 5");

    // cursor 6 — node with its own VECTOR sidecar row (MUST survive step 23).
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, source_id, logical_id)
             VALUES(6, 'doc', 'node body', 'doc-1', 'lid-1')",
        [],
    )
    .expect("seed node at cursor 6");
    conn.execute(
        "INSERT INTO _fathomdb_vector_rows(rowid, kind, write_cursor)
             VALUES(6, 'doc', 6)",
        [],
    )
    .expect("seed node vector sidecar at cursor 6");

    conn
}

/// **RED keystone.** After step 23 drops the edge at cursor 5, its
/// `_fathomdb_vector_rows` sidecar row must be gone. Against pre-fix-6 code the
/// migration never touches `_fathomdb_vector_rows`, so the orphan survives and the
/// vec0 row it mirrors goes on consuming a KNN candidate slot on the upgraded DB.
#[test]
fn fix6_dropped_edge_vector_sidecar_is_removed() {
    let conn = seed_v22_with_vector_projected_edge();
    assert!(sidecar_has(&conn, 5), "precondition: edge sidecar exists before step 23");

    migrate_with_steps(&conn, MIGRATIONS).expect("migrate v22 -> head (step 23)");
    assert_eq!(user_version(&conn), 24, "migrations must have applied to head");

    assert!(
        !sidecar_has(&conn, 5),
        "step 23 must delete the dropped edge's _fathomdb_vector_rows sidecar row \
         (cursor 5); leaving it orphans the vec0 candidate on every upgraded DB"
    );
}

/// The delete must be SCOPED to edge cursors, not a truncate: a node's vector
/// sidecar shares the same `_fathomdb_vector_rows` table and MUST survive. Guards
/// against a vacuous "delete everything" fix that would erase node recall.
#[test]
fn fix6_node_vector_sidecar_survives_step_23() {
    let conn = seed_v22_with_vector_projected_edge();

    migrate_with_steps(&conn, MIGRATIONS).expect("migrate v22 -> head (step 23)");

    assert!(
        sidecar_has(&conn, 6),
        "the node's vector sidecar (cursor 6) must survive step 23 — the edge-vector \
         cleanup is scoped to edge cursors, it must not touch node vectors"
    );
    assert!(!sidecar_has(&conn, 5), "edge sidecar (cursor 5) still must be gone");
}
