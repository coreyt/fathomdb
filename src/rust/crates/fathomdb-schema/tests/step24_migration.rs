//! 0.8.20 Slice 15d (R-20-PR / R-20-EAV) — step-24 projection-registry EAV +
//! property-FTS substrate migration tests.
//!
//! Step 24 is NET-NEW infrastructure: before it there is no attribute/EAV store
//! and no property-FTS (only `body`-FTS + vector). It adds three objects:
//!
//! - `_fathomdb_projection_registry` — the durable declared-spec store (the
//!   derived-cache source boot re-derive reads).
//! - `canonical_attributes` — the row-owned EAV attribute store (`write_cursor`,
//!   `attr_name`, `attr_value`) + its `(attr_name, attr_value)` composite index.
//! - `property_search_index` — the property-FTS5 shadow (`write_cursor`
//!   UNINDEXED, same shape as `search_index_edges`).
//!
//! Properties:
//!
//! - **P1 shape** — all three objects exist after step 24, with the columns the
//!   engine binds by name.
//! - **P2 NO DATA MIGRATION** — the step defines the shape only; it does not
//!   backfill (the tables come up EMPTY on a migrated DB — `configure_projections`
//!   / boot re-derive populate them).
//! - **P3 crash-safety / idempotence** — a completed step never re-runs (the
//!   runner wraps the batch + version bump in one `BEGIN IMMEDIATE`).
//! - **P4 accretion guard** — additive `CREATE TABLE` requires the exemption
//!   marker, and the marker is present.

use fathomdb_schema::{
    check_migration_accretion, migrate_with_steps, Migration, MIGRATIONS, SCHEMA_VERSION,
};
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

fn steps_through(limit: u32) -> Vec<Migration> {
    MIGRATIONS.iter().filter(|m| m.step_id <= limit).cloned().collect()
}

fn columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})")).unwrap();
    stmt.query_map([], |row| row.get::<_, String>(1)).unwrap().map(|r| r.unwrap()).collect()
}

fn object_exists(conn: &Connection, name: &str) -> bool {
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM sqlite_master WHERE name = ?1", [name], |r| r.get(0))
        .unwrap();
    n > 0
}

fn seed_v23() -> Connection {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, &steps_through(23)).expect("migrate to v23");
    assert_eq!(user_version(&conn), 23, "precondition: DB at the pre-slice head 23");
    conn
}

/// P1 — the three objects exist and carry the columns the engine binds.
#[test]
fn s24_registry_eav_and_property_fts_objects_exist() {
    let conn = seed_v23();
    // v23 has no attribute substrate yet.
    assert!(!object_exists(&conn, "canonical_attributes"), "pre-slice: no EAV store");
    assert!(!object_exists(&conn, "property_search_index"), "pre-slice: no property-FTS");
    assert!(!object_exists(&conn, "_fathomdb_projection_registry"), "pre-slice: no registry");

    migrate_with_steps(&conn, &steps_through(24)).expect("migrate to v24");

    assert!(object_exists(&conn, "_fathomdb_projection_registry"));
    assert!(object_exists(&conn, "canonical_attributes"));
    assert!(object_exists(&conn, "property_search_index"));

    let reg = columns(&conn, "_fathomdb_projection_registry");
    for c in ["name", "roles", "fts_tokenizer", "vector_embedder", "vector_declared"] {
        assert!(reg.contains(&c.to_string()), "_fathomdb_projection_registry.{c} must exist");
    }
    let eav = columns(&conn, "canonical_attributes");
    for c in ["write_cursor", "attr_name", "attr_value"] {
        assert!(eav.contains(&c.to_string()), "canonical_attributes.{c} must exist");
    }
    // property_search_index is FTS5; PRAGMA table_info reports its columns incl.
    // the UNINDEXED write_cursor (same as search_index_edges).
    let pfts = columns(&conn, "property_search_index");
    for c in ["attr_value", "attr_name", "write_cursor"] {
        assert!(pfts.contains(&c.to_string()), "property_search_index.{c} must exist");
    }
}

/// P2 — NO DATA MIGRATION: the step creates EMPTY tables. Even on a DB that has
/// canonical nodes at v23, step 24 must not backfill any attribute rows (that is
/// `configure_projections` / boot re-derive's job, not the migration's).
#[test]
fn s24_no_data_migration_tables_come_up_empty() {
    let conn = seed_v23();
    conn.execute_batch(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, source_id, logical_id, row_kind, state)
             VALUES(1, 'doc', '{\"status\":\"open\"}', 'src:1', 'lid-a', 'leaf', 'active');",
    )
    .expect("seed a v23 node");

    migrate_with_steps(&conn, &steps_through(24)).expect("migrate to v24");

    for table in ["canonical_attributes", "property_search_index", "_fathomdb_projection_registry"]
    {
        let n: i64 =
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "{table} must come up EMPTY — step 24 is shape-only, no backfill");
    }
}

/// P3 — a completed step never re-runs (re-running the whole registry is a
/// no-op, not a "table already exists" error).
#[test]
fn s24_is_idempotent_across_repeated_migrate_calls() {
    let conn = seed_v23();
    migrate_with_steps(&conn, MIGRATIONS).expect("first migrate to head");
    assert_eq!(user_version(&conn), SCHEMA_VERSION);

    let report = migrate_with_steps(&conn, MIGRATIONS).expect("re-running migrate must be a no-op");
    assert!(
        report.migration_steps.is_empty(),
        "no step may re-run once user_version is at head; ran {:?}",
        report.migration_steps
    );
    assert_eq!(user_version(&conn), SCHEMA_VERSION);
}

/// P3 — atomic with its version bump: a poisoned batch leaves the DB at 23 with
/// NONE of the three objects present, so the retry applies the step whole.
#[test]
fn s24_failed_step_rolls_back_objects_and_version_together() {
    let conn = seed_v23();
    let poisoned = vec![Migration {
        step_id: 24,
        sql: "-- MIGRATION-ACCRETION-EXEMPTION: poisoned step-24 stand-in
              CREATE TABLE canonical_attributes(write_cursor INTEGER NOT NULL, attr_name TEXT NOT NULL, attr_value TEXT);
              SELECT this_is_not_valid_sql_and_must_abort_the_step();",
    }];
    migrate_with_steps(&conn, &poisoned).expect_err("poisoned step must fail");

    assert_eq!(user_version(&conn), 23, "a failed step must leave user_version at 23");
    assert!(
        !object_exists(&conn, "canonical_attributes"),
        "a failed step must roll back its CREATE TABLE — otherwise the retry hits \
         'table already exists' and the DB is wedged"
    );

    migrate_with_steps(&conn, &steps_through(24)).expect("retry must succeed");
    assert_eq!(user_version(&conn), 24);
    assert!(object_exists(&conn, "canonical_attributes"));
}

/// P4 — step 24 adds schema without a DROP, so it REQUIRES the exemption marker,
/// and the marker must actually be present in the shipped SQL.
#[test]
fn s24_carries_the_accretion_exemption_marker() {
    let step = MIGRATIONS.iter().find(|m| m.step_id == 24).expect("step-24 must exist");
    check_migration_accretion("step-24", step.sql)
        .expect("step-24 must satisfy the accretion guard");
    assert!(
        step.sql.contains("-- MIGRATION-ACCRETION-EXEMPTION: "),
        "an additive CREATE TABLE step must carry the exemption marker"
    );
}

/// Step 24 is head and `SCHEMA_VERSION` tracks it.
#[test]
fn s24_is_head_and_schema_version_is_24() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, MIGRATIONS).expect("migration must succeed");

    assert_eq!(user_version(&conn), SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 24, "SCHEMA_VERSION must be 24 (step-24 projection-registry EAV)");
    assert_eq!(
        MIGRATIONS.last().expect("at least one migration").step_id,
        24,
        "step-24 (projection-registry EAV + property-FTS) must be the last (head) migration"
    );
}
