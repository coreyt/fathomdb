//! Slice 20 (0.8.14 migration verify) — full-path v15 → v17 migration.
//!
//! Covers plan-0.8.14 §2 R-SUB-3 (forward-only + contiguous) and R-GATE, for
//! the WHOLE 0.8.14 schema jump: an OLD database frozen at the pre-0.8.14 head
//! (schema version 15) must migrate cleanly ALL THE WAY to the current head
//! (17 = step-16 EXP-S `row_kind` + step-17 F5 `search_index_v2`) in ONE
//! end-to-end pass, with no data loss and no skipped steps. ADR authority:
//! `dev/adr/ADR-0.8.14-exp-s-kind-tagged-coexisting-index-substrate.md`
//! §D3/§D4 + §D6 (no vec0 rows touched).
//!
//! The two single-step tests (`step16_migration.rs`, `step17_migration.rs`)
//! each prove one hop in isolation. This test proves the two hops *compose* off
//! a real legacy DB: a pre-existing `canonical_nodes` row survives BOTH the
//! step-16 in-place `row_kind` back-fill AND the step-17 O(N) `search_index_v2`
//! re-index, and the v1 `search_index` is retained throughout. Non-vacuous: it
//! fails if `row_kind` defaulting regressed (the legacy row would not read back
//! `'leaf'`), if the v2 rebuild regressed (the legacy row would be missing from
//! `search_index_v2` or its `$.status` would not be derived), if a step were
//! skipped/re-run (the captured step order would not be exactly `[16, 17]`), or
//! if either step lost its accretion exemption.

use fathomdb_schema::{
    check_migration_accretion, migrate_with_event_sink, migrate_with_steps, MigrationStepReport,
    MIGRATIONS, SCHEMA_VERSION,
};
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

/// Stand up a DB at the OLD pre-0.8.14 head (schema version 15) — every step up
/// to and including 15, nothing from the 0.8.14 jump. Mirrors the v15 fixture
/// pattern in `step16_migration.rs`.
fn open_db_at_v15() -> Connection {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    let steps_to_15: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id <= 15).cloned().collect();
    migrate_with_steps(&conn, &steps_to_15).expect("migrate to old head v15");
    assert_eq!(user_version(&conn), 15, "precondition: DB is at the old schema version 15");
    conn
}

/// R-SUB-3 end-to-end — an OLD v15 DB with a realistic legacy row migrates the
/// full path to v17 in one pass: forward-only + contiguous (steps 16 then 17,
/// no skips/re-runs), the legacy row survives both hops (no data loss), and the
/// coexisting search substrates are correct.
#[test]
fn s15_to_17_full_path_migrates_legacy_db_without_loss() {
    let conn = open_db_at_v15();

    // A realistic legacy (pre-0.8.14) canonical node: JSON body carrying a
    // `$.status` field, written BEFORE row_kind and search_index_v2 existed.
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body)
         VALUES(1, 'todo', '{\"status\":\"open\",\"text\":\"legacy ticket about widgets\"}')",
        [],
    )
    .expect("legacy node insert at v15");

    // Run the FULL migration set from v15, capturing each applied step via the
    // event sink (the same driver the engine uses).
    let mut applied: Vec<u32> = Vec::new();
    let report = migrate_with_event_sink(&conn, MIGRATIONS, |step: &MigrationStepReport| {
        assert!(!step.failed, "no step may fail on the v15->v17 path (step {})", step.step_id);
        applied.push(step.step_id);
    })
    .expect("full v15->v17 migration must succeed");

    // Forward-only + contiguous: exactly steps 16 then 17 ran, in order, with no
    // skips and nothing at/below 15 re-run.
    assert_eq!(report.schema_version_before, 15);
    assert_eq!(report.schema_version_after, 18);
    assert_eq!(user_version(&conn), SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 18, "current head must be 18");
    assert_eq!(
        applied,
        vec![16, 17, 18],
        "only steps 16, 17 then 18 may run from v15 (forward-only)"
    );
    let ran_in_report: Vec<u32> = report.migration_steps.iter().map(|s| s.step_id).collect();
    assert_eq!(ran_in_report, vec![16, 17, 18], "report step order must match the emitted order");

    // No data loss — the legacy row is still there and back-filled to the
    // step-16 `row_kind='leaf'` default (in-place, == current behavior).
    let (kind, row_kind): (String, String) = conn
        .query_row("SELECT kind, row_kind FROM canonical_nodes WHERE write_cursor = 1", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("legacy row must survive the full migration");
    assert_eq!(kind, "todo", "legacy row's original data must be intact");
    assert_eq!(row_kind, "leaf", "legacy row must default to row_kind='leaf' after step-16");

    // The step-17 O(N) rebuild indexed the legacy row into search_index_v2,
    // deriving `status` from its JSON `$.status` (json_valid-guarded).
    assert!(table_exists(&conn, "search_index_v2"), "search_index_v2 must exist after step-17");
    let v2_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM search_index_v2", [], |r| r.get(0)).expect("v2 count");
    assert_eq!(v2_count, 1, "the single legacy row must be re-indexed into search_index_v2");
    let status_v2: String = conn
        .query_row("SELECT status FROM search_index_v2 WHERE write_cursor = 1", [], |r| r.get(0))
        .expect("legacy row must be present in search_index_v2");
    assert_eq!(status_v2, "open", "legacy JSON body's $.status must be derived by the v2 rebuild");

    // The v1 single-column body-only index is RETAINED (not dropped by the jump).
    assert!(table_exists(&conn, "search_index"), "single-column search_index must be RETAINED");

    // Accretion-exemption invariants still hold for BOTH steps on this path:
    // each is additive (ADD COLUMN / CREATE) and carries its exemption marker,
    // so the full jump does not trip the accretion guard.
    let step16 = MIGRATIONS.iter().find(|m| m.step_id == 16).expect("step 16 must exist");
    let step17 = MIGRATIONS.iter().find(|m| m.step_id == 17).expect("step 17 must exist");
    check_migration_accretion("016_exp_s_row_kind.sql", step16.sql)
        .expect("step 16 must pass the accretion guard on the full path");
    check_migration_accretion("017_f5_search_index_v2.sql", step17.sql)
        .expect("step 17 must pass the accretion guard on the full path");
}
