//! Slice 5 (EXP-S KEYSTONE) — step-16 `row_kind` migration tests.
//!
//! Covers plan-0.8.14 §2 R-SUB-1 (row_kind column lands) and R-SUB-3
//! (migration is forward-only + guarded). ADR authority:
//! `dev/adr/ADR-0.8.14-exp-s-kind-tagged-coexisting-index-substrate.md` D1/D3/D4.
//!
//! `row_kind` is a SEPARATE structural-role axis on `canonical_nodes`
//! (leaf/coverage/graph), orthogonal to the hard-locked doc-type `kind`
//! vocabulary. `NOT NULL DEFAULT 'leaf'` means every pre-existing row
//! back-fills to `leaf` in-place (== current behavior) — the migration is
//! forward-only and touches no vec0 rows (ADR §D6).

use fathomdb_schema::{
    check_migration_accretion, migrate_with_steps, MigrationAccretionError, MIGRATIONS,
    SCHEMA_VERSION,
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

fn column_names(conn: &Connection, table: &str) -> Vec<String> {
    conn.prepare(&format!("PRAGMA table_info({table})"))
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .collect()
}

/// R-SUB-1 — after the full migration set, `canonical_nodes` has `row_kind`.
/// R-SUB-3 — the migration reaches head `SCHEMA_VERSION`. `row_kind` lands at
/// step 16; the head pin below tracks the current head (bumped to 17 when F5's
/// step-17 `search_index_v2` co-landed, then 18 when F9's step-18 `importance`
/// landed).
#[test]
fn s16_row_kind_column_present_and_schema_version_is_head() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, MIGRATIONS).expect("migration must succeed");

    let cols = column_names(&conn, "canonical_nodes");
    assert!(
        cols.contains(&"row_kind".to_string()),
        "canonical_nodes must have row_kind after step-16, got: {cols:?}"
    );

    assert_eq!(user_version(&conn), SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 18, "SCHEMA_VERSION must be 18 (step-18 F9 importance)");
    assert_eq!(
        MIGRATIONS.last().expect("at least one migration").step_id,
        18,
        "step-18 (F9 importance) must be the last (head) migration"
    );
}

/// R-SUB-3 (forward-only) — open a DB at the OLD schema (version 15) with a
/// pre-existing row, run ONLY the step-16 migration, and assert: it reaches 16,
/// the legacy row defaults to `row_kind='leaf'`, and only the single forward
/// step was applied (no re-run of 2..15).
#[test]
fn s16_forward_only_from_v15_defaults_existing_rows_to_leaf() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);

    // Bring the DB up to the OLD head (version 15) — everything before step-16.
    let steps_to_15: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id <= 15).cloned().collect();
    migrate_with_steps(&conn, &steps_to_15).expect("migrate to v15");
    assert_eq!(user_version(&conn), 15, "precondition: DB is at the old schema version 15");

    // A pre-existing (pre-row_kind) canonical node.
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body) VALUES(1, 'doc', 'legacy-body')",
        [],
    )
    .expect("legacy node insert");

    // Apply ONLY the forward step-16 migration.
    let step16_only: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id == 16).cloned().collect();
    let report = migrate_with_steps(&conn, &step16_only).expect("forward migrate to v16");

    // Forward-only: exactly the single new step ran; nothing before it re-ran.
    assert_eq!(report.schema_version_before, 15);
    assert_eq!(report.schema_version_after, 16);
    let ran: Vec<u32> = report.migration_steps.iter().map(|s| s.step_id).collect();
    assert_eq!(ran, vec![16], "only step-16 must run when starting from v15");
    assert_eq!(user_version(&conn), 16);

    // The legacy row back-fills to the 'leaf' default (== current behavior).
    let row_kind: String = conn
        .query_row("SELECT row_kind FROM canonical_nodes WHERE write_cursor = 1", [], |row| {
            row.get(0)
        })
        .expect("legacy row must be readable after step-16");
    assert_eq!(row_kind, "leaf", "existing rows must default to row_kind='leaf'");
}

/// R-SUB-3 (guarded) — step-16 is a pure additive `ADD COLUMN` (no DROP), so the
/// accretion guard REJECTS it unless the exemption marker is present. This pins
/// that the marker is the only thing letting it pass (mirrors `ac_049` / `s12`).
#[test]
fn s16_passes_accretion_guard_only_with_marker() {
    let step16 =
        MIGRATIONS.iter().find(|m| m.step_id == 16).expect("step 16 (EXP-S row_kind) must exist");

    check_migration_accretion("016_exp_s_row_kind.sql", step16.sql)
        .expect("step 16 must pass the accretion guard with its exemption marker");

    let without_marker: String = step16
        .sql
        .lines()
        .filter(|line| !line.contains("MIGRATION-ACCRETION-EXEMPTION"))
        .collect::<Vec<_>>()
        .join("\n");
    let err = check_migration_accretion("016_exp_s_row_kind.sql", &without_marker)
        .expect_err("step 16 without the exemption marker must be rejected");
    assert_eq!(err, MigrationAccretionError { offender: "016_exp_s_row_kind.sql".to_string() });
}
