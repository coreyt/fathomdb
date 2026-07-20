//! Slice 15 (G11) — step-14 schema migration tests.
//!
//! Tests the 5 additive nullable columns on `canonical_edges` and the
//! `search_index_edges` FTS5 virtual table introduced in step-14.
//! ADR authority: `dev/adr/ADR-0.8.1-graph-substrate-g11-migration.md` §6.

use fathomdb_schema::{check_migration_accretion, migrate, SCHEMA_VERSION};
use rusqlite::Connection;
use std::sync::Once;

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

fn migrate_fresh(conn: &Connection) {
    register_sqlite_vec_once();
    set_user_version(conn, 1);
    migrate(conn).expect("migration must succeed");
}

fn column_names(conn: &Connection, table: &str) -> Vec<String> {
    conn.prepare(&format!("PRAGMA table_info({table})"))
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .collect()
}

/// ADR §6 criterion 1 — after step-14, `canonical_edges` has all five G11 columns.
#[test]
fn s14_g11_canonical_edges_has_all_five_columns() {
    let conn = Connection::open_in_memory().unwrap();
    migrate_fresh(&conn);

    let cols = column_names(&conn, "canonical_edges");
    for expected in &["body", "t_valid", "t_invalid", "confidence", "extractor_model_id"] {
        assert!(
            cols.contains(&expected.to_string()),
            "canonical_edges must have column {expected} after step-14, got: {cols:?}"
        );
    }
}

/// ADR §6 criterion 2 — legacy rows (pre-step-14 inserts) read NULL for all five G11 columns.
#[test]
fn s14_legacy_rows_null_safe_for_g11_columns() {
    let conn = Connection::open_in_memory().unwrap();
    register_sqlite_vec_once();
    // Migrate only to step-13, insert a legacy edge, then apply remaining steps.
    set_user_version(&conn, 1);
    {
        use fathomdb_schema::{migrate_with_steps, MIGRATIONS};
        let steps_to_13: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id <= 13).cloned().collect();
        migrate_with_steps(&conn, &steps_to_13).expect("migrate to step-13");
    }

    // Insert a legacy edge (no G11 columns yet).
    conn.execute(
        "INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id) VALUES(1, 'link', 'A', 'B')",
        [],
    )
    .expect("legacy edge insert");

    // Apply the remaining steps (step-14 onwards).
    {
        use fathomdb_schema::{migrate_with_steps, MIGRATIONS};
        let steps_14_plus: Vec<_> =
            MIGRATIONS.iter().filter(|m| m.step_id >= 14).cloned().collect();
        migrate_with_steps(&conn, &steps_14_plus).expect("migrate step-14+");
    }

    // Legacy row must read NULL for all five G11 columns.
    #[allow(clippy::type_complexity)]
    let row: (Option<String>, Option<String>, Option<String>, Option<f64>, Option<String>) = conn
        .query_row(
            "SELECT body, t_valid, t_invalid, confidence, extractor_model_id
             FROM canonical_edges WHERE write_cursor = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .expect("legacy row must be readable after step-14");

    assert!(row.0.is_none(), "body must be NULL for legacy row");
    assert!(row.1.is_none(), "t_valid must be NULL for legacy row");
    assert!(row.2.is_none(), "t_invalid must be NULL for legacy row");
    assert!(row.3.is_none(), "confidence must be NULL for legacy row");
    assert!(row.4.is_none(), "extractor_model_id must be NULL for legacy row");
}

/// ADR §6 criterion 3 — a fresh migrate reaches head `SCHEMA_VERSION` (was 14
/// at step-14; 15 since step-15 added `temporal_fallback`). Asserting the
/// constant keeps `user_version` robust to future bumps; the explicit head pin
/// below is the deliberate "update me on a schema bump" tripwire.
#[test]
fn s14_schema_version_is_14() {
    let conn = Connection::open_in_memory().unwrap();
    migrate_fresh(&conn);
    assert_eq!(user_version(&conn), SCHEMA_VERSION, "fresh migrate must reach head SCHEMA_VERSION");
    assert_eq!(
        SCHEMA_VERSION, 22,
        "SCHEMA_VERSION constant must be 22 (step-22 node validity window, R-20-NV)"
    );
}

/// Step-14 SQL contains the MIGRATION-ACCRETION-EXEMPTION marker (ADD COLUMN requires it).
#[test]
fn s14_passes_accretion_guard_with_exemption_marker() {
    let step14 = fathomdb_schema::MIGRATIONS
        .iter()
        .find(|m| m.step_id == 14)
        .expect("step 14 (G11 edge enrichment) must exist");

    check_migration_accretion("014_g11_edge_enrichment.sql", step14.sql)
        .expect("step 14 must pass the accretion guard with its exemption marker");

    // Strip the marker; must be rejected.
    let without_marker: String = step14
        .sql
        .lines()
        .filter(|line| !line.contains("MIGRATION-ACCRETION-EXEMPTION"))
        .collect::<Vec<_>>()
        .join("\n");
    check_migration_accretion("014_g11_edge_enrichment.sql", &without_marker)
        .expect_err("step 14 without the exemption marker must be rejected");
}

/// ADR §6 criterion 4 — `search_index_edges` FTS5 virtual table exists after step-14.
#[test]
fn s14_search_index_edges_table_exists() {
    let conn = Connection::open_in_memory().unwrap();
    migrate_fresh(&conn);

    let count: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='search_index_edges'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "search_index_edges must exist after step-14");
}

/// ADR §6 — SCHEMA_VERSION constant matches the last migration step_id.
#[test]
fn schema_version_constant_matches_last_migration_step() {
    let last_step = fathomdb_schema::MIGRATIONS.last().expect("at least one migration");
    assert_eq!(
        SCHEMA_VERSION, last_step.step_id,
        "SCHEMA_VERSION must equal the last migration step_id"
    );
}
