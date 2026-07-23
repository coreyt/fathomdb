//! 0.8.19 Slice 5 (OPP-12 record-lifecycle Phase-1 KEYSTONE) — step-20 existence
//! axis migration tests.
//!
//! Covers `dev/design/0.8.19-slice-0-opp12-phase1-design.md` §5 (the ONE 19→20
//! migration) + `dev/plans/plan-0.8.19.md` §2 (R-EX-1/R-MIG-1). The migration adds
//! the two existence columns on `canonical_nodes` — `state` (`NOT NULL DEFAULT
//! 'active'` so every pre-existing row back-fills to `active`) + `reason`
//! (nullable) — and the `canonical_nodes_state_active_idx` partial index. Scoped
//! per F-23 ruling 1a: existence columns ONLY, NO surrogate-`logical_id` backfill.
//!
//! `SCHEMA_VERSION` advances 19 → 20. R-MIG-1: fresh-create at 20 and
//! upgrade-from-19 must both land the IDENTICAL `canonical_nodes` shape + index
//! set (asserted here on BOTH paths). Additive `ADD COLUMN` (no DROP) with a
//! leading `MIGRATION-ACCRETION-EXEMPTION` marker.

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

/// Full `PRAGMA table_info` tuple (name, type, notnull, dflt_value) per column,
/// in declaration order — the byte-stable shape oracle for fresh==upgrade parity.
fn table_shape(conn: &Connection, table: &str) -> Vec<(String, String, i64, Option<String>)> {
    conn.prepare(&format!("PRAGMA table_info({table})"))
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,         // name
                row.get::<_, String>(2)?,         // type
                row.get::<_, i64>(3)?,            // notnull
                row.get::<_, Option<String>>(4)?, // dflt_value
            ))
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect()
}

/// The set of index (name, sql) rows for a table, sorted by name — the second
/// half of the fresh==upgrade shape oracle.
fn index_set(conn: &Connection, table: &str) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = conn
        .prepare("SELECT name, sql FROM sqlite_master WHERE type='index' AND tbl_name=?1")
        .unwrap()
        .query_map([table], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    out.sort();
    out
}

/// R-EX-1 / R-MIG-1 — after the full migration set `canonical_nodes` carries
/// `state` (NOT NULL DEFAULT 'active') + `reason` (nullable) with the
/// active-only partial index. The head pin moved to step-21 in 0.8.20 Slice 5c
/// (legacy provenance backfill, R-20-E8); step-20's columns are unaffected.
#[test]
fn s20_existence_columns_present_and_schema_version_is_head() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, MIGRATIONS).expect("migration must succeed");

    assert_eq!(user_version(&conn), SCHEMA_VERSION);
    assert_eq!(
        MIGRATIONS.last().expect("at least one migration").step_id,
        24,
        "step-24 (projection-registry EAV + property-FTS, Slice 15d) must be the last (head) migration"
    );

    let shape = table_shape(&conn, "canonical_nodes");
    let state = shape.iter().find(|c| c.0 == "state").expect("canonical_nodes must have `state`");
    assert_eq!(state.1, "TEXT", "state must be TEXT");
    assert_eq!(state.2, 1, "state must be NOT NULL");
    assert_eq!(
        state.3.as_deref(),
        Some("'active'"),
        "state must default to 'active' (every existing row back-fills active)"
    );
    let reason =
        shape.iter().find(|c| c.0 == "reason").expect("canonical_nodes must have `reason`");
    assert_eq!(reason.1, "TEXT", "reason must be TEXT");
    assert_eq!(reason.2, 0, "reason must be nullable");

    // The active-only partial index serving the hot path.
    let idx = index_set(&conn, "canonical_nodes");
    let active_idx = idx
        .iter()
        .find(|(name, _)| name == "canonical_nodes_state_active_idx")
        .expect("canonical_nodes_state_active_idx must exist");
    let sql = active_idx.1.as_deref().unwrap_or_default();
    assert!(
        sql.contains("state = 'active'"),
        "canonical_nodes_state_active_idx must be partial on state='active', got: {sql}"
    );
}

/// R-MIG-1 — fresh-create at 20 MUST equal upgrade-from-19: identical
/// `canonical_nodes` `PRAGMA table_info` AND identical index set on BOTH paths.
#[test]
fn s20_fresh_equals_upgrade_from_19() {
    register_sqlite_vec_once();

    // Path A — fresh create at 20. Bounded to steps <= 20 so this stays a
    // fresh-AT-20 vs upgrade-TO-20 comparison as the ladder grows past 20
    // (0.8.20 Slice 5c added step 21); migrating the full set would compare a
    // v21 shape against a v20 one.
    let fresh = Connection::open_in_memory().unwrap();
    set_user_version(&fresh, 1);
    let steps_to_20: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id <= 20).cloned().collect();
    migrate_with_steps(&fresh, &steps_to_20).expect("fresh migrate to 20");
    assert_eq!(user_version(&fresh), 20);

    // Path B — upgrade from 19: migrate to v19, seed a legacy row, then apply
    // ONLY step 20.
    let upgraded = Connection::open_in_memory().unwrap();
    set_user_version(&upgraded, 1);
    let steps_to_19: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id <= 19).cloned().collect();
    migrate_with_steps(&upgraded, &steps_to_19).expect("migrate to v19");
    assert_eq!(user_version(&upgraded), 19, "precondition: DB at old schema version 19");
    upgraded
        .execute(
            "INSERT INTO canonical_nodes(write_cursor, kind, body, row_kind) \
             VALUES(1, 'note', 'legacy body', 'leaf')",
            [],
        )
        .expect("seed legacy row");
    let step20_only: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id == 20).cloned().collect();
    let report = migrate_with_steps(&upgraded, &step20_only).expect("forward migrate to v20");
    assert_eq!(report.schema_version_before, 19);
    assert_eq!(report.schema_version_after, 20);
    let ran: Vec<u32> = report.migration_steps.iter().map(|s| s.step_id).collect();
    assert_eq!(ran, vec![20], "only step-20 must run when starting from v19");

    // The legacy row back-fills to state='active', reason NULL.
    let (state, reason): (String, Option<String>) = upgraded
        .query_row("SELECT state, reason FROM canonical_nodes WHERE write_cursor = 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("legacy row survives");
    assert_eq!(state, "active", "every pre-existing row back-fills to state='active'");
    assert_eq!(reason, None, "reason back-fills to NULL");

    // R-MIG-1 keystone: byte-identical canonical_nodes shape + index set.
    assert_eq!(
        table_shape(&fresh, "canonical_nodes"),
        table_shape(&upgraded, "canonical_nodes"),
        "fresh-create at 20 must equal upgrade-from-19 (PRAGMA table_info)"
    );
    assert_eq!(
        index_set(&fresh, "canonical_nodes"),
        index_set(&upgraded, "canonical_nodes"),
        "fresh-create at 20 must equal upgrade-from-19 (index set)"
    );
}

/// R-MIG-1 (guarded) — step-20 is an additive `ADD COLUMN` (no DROP), so the
/// accretion guard REJECTS it unless the exemption marker is present.
#[test]
fn s20_passes_accretion_guard_only_with_marker() {
    let step20 = MIGRATIONS
        .iter()
        .find(|m| m.step_id == 20)
        .expect("step 20 (OPP-12 Phase-1 existence axis) must exist");

    check_migration_accretion("020_existence_axis.sql", step20.sql)
        .expect("step 20 must pass the accretion guard with its exemption marker");

    let without_marker: String = step20
        .sql
        .lines()
        .filter(|line| !line.contains("MIGRATION-ACCRETION-EXEMPTION"))
        .collect::<Vec<_>>()
        .join("\n");
    check_migration_accretion("020_existence_axis.sql", &without_marker)
        .expect_err("step 20 without the exemption marker must be rejected");
}
