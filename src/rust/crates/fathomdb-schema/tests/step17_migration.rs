//! Slice 10 (F5 — fielded FTS / BM25F) — step-17 `search_index_v2` migration.
//!
//! Covers plan-0.8.14 §2 R-SUB-3 (migration is forward-only + guarded) and the
//! schema half of R-F5-1 (the multi-column BM25F FTS5 table lands + is
//! O(N)-rebuilt from existing rows). ADR authority:
//! `dev/adr/ADR-0.8.1-deferred-f5-fielded-fts-bm25f.md` §3.1 +
//! `dev/adr/ADR-0.8.14-exp-s-kind-tagged-coexisting-index-substrate.md` §D4/§D6.
//!
//! `search_index_v2` is a NEW multi-column FTS5 index (`kind`/`body`/`status`)
//! that coexists with the RETAINED single-column body-only `search_index`. FTS5
//! has no in-place column-add, so the step CREATEs the new table and re-indexes
//! (`INSERT ... SELECT FROM canonical_nodes`) — an O(N) re-index paid once,
//! co-landed with the step-16 `row_kind` bump. No vec0 rows are touched (ADR
//! §D6), so the eu7 fidelity gate stays a documented no-op at Slice 20.

use fathomdb_schema::{check_migration_accretion, migrate_with_steps, MIGRATIONS, SCHEMA_VERSION};
use rusqlite::Connection;
use std::sync::Once;

// Step 9 creates a vec0 virtual table; register sqlite-vec once per binary.
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

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

/// R-SUB-3 — after the full migration set the head is 17 and step-17 is last;
/// the coexisting `search_index_v2` FTS5 table is present alongside the retained
/// single-column `search_index`.
#[test]
fn s17_search_index_v2_present_and_schema_version_is_17() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, MIGRATIONS).expect("migration must succeed");

    assert!(table_exists(&conn, "search_index_v2"), "search_index_v2 must exist after step-17");
    assert!(table_exists(&conn, "search_index"), "single-column search_index must be RETAINED");

    assert_eq!(user_version(&conn), SCHEMA_VERSION);
    assert_eq!(
        SCHEMA_VERSION, 21,
        "SCHEMA_VERSION must be 21 (step-21 legacy provenance backfill, R-20-E8)"
    );
    assert_eq!(
        MIGRATIONS.last().expect("at least one migration").step_id,
        21,
        "step-21 (legacy provenance backfill, R-20-E8) must be the last (head) migration"
    );

    // The v2 table carries the three BM25F fields kind/body/status (+ the
    // UNINDEXED write_cursor join column).
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(search_index_v2)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    for field in ["kind", "body", "status", "write_cursor"] {
        assert!(
            cols.contains(&field.to_string()),
            "search_index_v2 must have {field}, got {cols:?}"
        );
    }
}

/// R-SUB-3 (forward-only) — open a DB at the OLD schema (version 16) with
/// pre-existing rows, run ONLY step-17, and assert: it reaches 17, ONLY step-17
/// ran, and `search_index_v2` is O(N)-rebuilt from the existing `canonical_nodes`
/// rows — including the JSON `$.status` derivation (guarded by `json_valid`).
#[test]
fn s17_forward_only_from_v16_reindexes_existing_rows() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);

    // Bring the DB up to the OLD head (version 16) — everything before step-17.
    let steps_to_16: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id <= 16).cloned().collect();
    migrate_with_steps(&conn, &steps_to_16).expect("migrate to v16");
    assert_eq!(user_version(&conn), 16, "precondition: DB is at the old schema version 16");

    // Two pre-existing (pre-search_index_v2) canonical nodes: one plain body
    // (non-JSON -> empty status), one JSON body carrying `$.status`.
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, row_kind)
         VALUES(1, 'note', 'plain body about widgets', 'leaf')",
        [],
    )
    .expect("plain node insert");
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, row_kind)
         VALUES(2, 'todo', '{\"status\":\"open\",\"text\":\"buy milk\"}', 'leaf')",
        [],
    )
    .expect("json node insert");

    // Apply ONLY the forward step-17 migration.
    let step17_only: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id == 17).cloned().collect();
    let report = migrate_with_steps(&conn, &step17_only).expect("forward migrate to v17");

    // Forward-only: exactly the single new step ran; nothing before it re-ran.
    assert_eq!(report.schema_version_before, 16);
    assert_eq!(report.schema_version_after, 17);
    let ran: Vec<u32> = report.migration_steps.iter().map(|s| s.step_id).collect();
    assert_eq!(ran, vec![17], "only step-17 must run when starting from v16");
    assert_eq!(user_version(&conn), 17);

    // The O(N) re-index copied both existing rows in, keyed by write_cursor.
    let v2_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM search_index_v2", [], |r| r.get(0)).expect("v2 count");
    assert_eq!(v2_count, 2, "search_index_v2 must be re-indexed from the 2 existing rows");

    // status was derived from `$.status` for the JSON body, and empty for the
    // non-JSON body (json_valid guard).
    let status_plain: String = conn
        .query_row("SELECT status FROM search_index_v2 WHERE write_cursor = 1", [], |r| r.get(0))
        .expect("plain status");
    let status_json: String = conn
        .query_row("SELECT status FROM search_index_v2 WHERE write_cursor = 2", [], |r| r.get(0))
        .expect("json status");
    assert_eq!(status_plain, "", "non-JSON body must index an empty status (json_valid guard)");
    assert_eq!(status_json, "open", "JSON body must index its $.status field");

    // The v2 fields are BM25F-searchable: a `kind`-column match resolves the
    // right canonical row.
    let hit_wc: i64 = conn
        .query_row(
            "SELECT write_cursor FROM search_index_v2 WHERE search_index_v2 MATCH 'kind:todo'",
            [],
            |r| r.get(0),
        )
        .expect("kind-column match must resolve");
    assert_eq!(hit_wc, 2, "kind-column BM25F match must find the 'todo' row");
}

/// R-SUB-3 (guarded) — step-17 carries the exemption marker and passes the
/// accretion guard. `CREATE VIRTUAL TABLE` (like step-5) does not itself trip the
/// guard (it fires on `CREATE TABLE` / `ADD COLUMN`), so the marker is
/// documentation-of-intent, not a gate — this pins that it is present and valid.
#[test]
fn s17_passes_accretion_guard_with_marker() {
    let step17 = MIGRATIONS
        .iter()
        .find(|m| m.step_id == 17)
        .expect("step 17 (F5 search_index_v2) must exist");

    assert!(
        step17.sql.contains("-- MIGRATION-ACCRETION-EXEMPTION: "),
        "step 17 must carry the exemption marker documenting the additive re-index"
    );
    check_migration_accretion("017_f5_search_index_v2.sql", step17.sql)
        .expect("step 17 must pass the accretion guard");
}
