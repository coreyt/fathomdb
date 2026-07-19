//! 0.8.20 Slice 5c (R-20-E8) — step-21 legacy-provenance backfill migration tests.
//!
//! Covers `dev/design/0.8.20-slice0-erasure-design.md` §4 work item 7 +
//! `dev/plans/plan-0.8.20.md` R-20-E8. Pre-0.8.20 writes could land
//! `source_id = NULL`, and a NULL-provenance row is unreachable by
//! `excise_source` — un-erasable. Step 21 back-fills those rows with the
//! reserved `source_id = '_legacy:pre-0.8.20'`.
//!
//! **The gate is EXACT and load-bearing: `WHERE logical_id IS NULL` ONLY.**
//! It comes from the TC-11 pin (CLOSED — not re-openable): a **governed** row
//! (non-NULL `logical_id`) keeps its NULL `source_id` and stays
//! `purge`-addressable BY `logical_id`. The pin's enforcing invariant is that no
//! migration, backfill or verb shall ever populate `logical_id` on an existing
//! canonical row, and a stored row's id-space is NEVER re-derived — so this
//! migration reads `logical_id` and never writes it.
//!
//! `SCHEMA_VERSION` advances 20 → 21. One migration per release (I-6).

use fathomdb_schema::{check_migration_accretion, migrate_with_steps, MIGRATIONS, SCHEMA_VERSION};
use rusqlite::Connection;
use std::sync::Once;

/// The reserved provenance step-21 stamps onto legacy **ungoverned** rows.
const LEGACY_SOURCE_ID: &str = "_legacy:pre-0.8.20";

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

fn source_id_of(conn: &Connection, table: &str, cursor: i64) -> Option<String> {
    conn.query_row(
        &format!("SELECT source_id FROM {table} WHERE write_cursor = ?1"),
        [cursor],
        |row| row.get::<_, Option<String>>(0),
    )
    .unwrap_or_else(|e| panic!("{table} row {cursor} must survive the migration: {e}"))
}

fn logical_id_of(conn: &Connection, table: &str, cursor: i64) -> Option<String> {
    conn.query_row(
        &format!("SELECT logical_id FROM {table} WHERE write_cursor = ?1"),
        [cursor],
        |row| row.get::<_, Option<String>>(0),
    )
    .unwrap_or_else(|e| panic!("{table} row {cursor} must survive the migration: {e}"))
}

/// Migrate to v20 and seed the four-way matrix the backfill has to discriminate:
/// (ungoverned | governed) × (NULL provenance | already provenanced), on BOTH
/// `canonical_nodes` and `canonical_edges`.
fn seed_v20_matrix() -> Connection {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    let steps_to_20: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id <= 20).cloned().collect();
    migrate_with_steps(&conn, &steps_to_20).expect("migrate to v20");
    assert_eq!(user_version(&conn), 20, "precondition: DB at the pre-slice head 20");

    conn.execute_batch(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, source_id, logical_id)
             VALUES(1, 'doc', 'ungoverned null-provenance', NULL, NULL);
         INSERT INTO canonical_nodes(write_cursor, kind, body, source_id, logical_id)
             VALUES(2, 'doc', 'governed null-provenance', NULL, 'lid-node-governed');
         INSERT INTO canonical_nodes(write_cursor, kind, body, source_id, logical_id)
             VALUES(3, 'doc', 'ungoverned provenanced', 'doc-caller-1', NULL);
         INSERT INTO canonical_nodes(write_cursor, kind, body, source_id, logical_id)
             VALUES(4, 'doc', 'governed provenanced', 'doc-caller-2', 'lid-node-both');
         INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id, source_id, logical_id)
             VALUES(5, 'rel', 'a', 'b', NULL, NULL);
         INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id, source_id, logical_id)
             VALUES(6, 'rel', 'c', 'd', NULL, 'lid-edge-governed');
         INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id, source_id, logical_id)
             VALUES(7, 'rel', 'e', 'f', 'doc-caller-3', NULL);",
    )
    .expect("seed the v20 provenance matrix");

    conn
}

/// **R-20-E8 keystone.** Step 21 back-fills `_legacy:pre-0.8.20` onto legacy
/// **ungoverned** rows and spares every governed one — the TC-11 pin's gate.
/// It must also never touch a row that already carries provenance.
#[test]
fn legacy_backfill_spares_governed_rows() {
    let conn = seed_v20_matrix();

    let step21_only: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id == 21).cloned().collect();
    assert_eq!(step21_only.len(), 1, "step-21 (legacy provenance backfill) must exist");
    let report = migrate_with_steps(&conn, &step21_only).expect("forward migrate to v21");
    assert_eq!(report.schema_version_before, 20);
    assert_eq!(report.schema_version_after, 21);

    // Ungoverned, NULL provenance → back-filled. This is the whole point: the row
    // becomes reachable by `excise_source('_legacy:pre-0.8.20')`.
    assert_eq!(
        source_id_of(&conn, "canonical_nodes", 1).as_deref(),
        Some(LEGACY_SOURCE_ID),
        "an ungoverned NULL-provenance node must be back-filled (it is otherwise un-erasable)"
    );
    assert_eq!(
        source_id_of(&conn, "canonical_edges", 5).as_deref(),
        Some(LEGACY_SOURCE_ID),
        "an ungoverned NULL-provenance edge must be back-filled"
    );

    // Governed, NULL provenance → SPARED. TC-11 pin: it stays `purge`-addressable
    // by `logical_id`, and `excise_source('_legacy:…')` must never reach it.
    assert_eq!(
        source_id_of(&conn, "canonical_nodes", 2),
        None,
        "TC-11 pin: a GOVERNED row keeps NULL source_id — the backfill gate is \
         `WHERE logical_id IS NULL` ONLY"
    );
    assert_eq!(
        source_id_of(&conn, "canonical_edges", 6),
        None,
        "TC-11 pin: a GOVERNED edge keeps NULL source_id"
    );

    // Already-provenanced rows are untouched in both id-spaces.
    assert_eq!(
        source_id_of(&conn, "canonical_nodes", 3).as_deref(),
        Some("doc-caller-1"),
        "the backfill must not overwrite caller-supplied provenance"
    );
    assert_eq!(
        source_id_of(&conn, "canonical_nodes", 4).as_deref(),
        Some("doc-caller-2"),
        "the backfill must not overwrite caller-supplied provenance on a governed row"
    );
    assert_eq!(
        source_id_of(&conn, "canonical_edges", 7).as_deref(),
        Some("doc-caller-3"),
        "the backfill must not overwrite caller-supplied edge provenance"
    );
}

/// **TC-11 pin, enforcing invariant (R-20-SUR).** The step-21 backfill READS
/// `logical_id` as its gate; it must never WRITE one. Asserted over stored rows:
/// the count of rows transitioning `logical_id` NULL → NOT NULL across the
/// migration must be exactly 0, and no governed row's `logical_id` may change.
#[test]
fn s21_backfill_populates_no_logical_id() {
    let conn = seed_v20_matrix();

    let before: Vec<(i64, Option<String>)> = conn
        .prepare("SELECT write_cursor, logical_id FROM canonical_nodes ORDER BY write_cursor")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    let step21_only: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id == 21).cloned().collect();
    // Non-vacuity guard: an empty step list makes `migrate_with_steps` a no-op, so
    // without this the whole test would pass trivially on a ladder lacking step 21.
    assert_eq!(step21_only.len(), 1, "step-21 must exist for this guard to mean anything");
    migrate_with_steps(&conn, &step21_only).expect("forward migrate to v21");

    let after: Vec<(i64, Option<String>)> = conn
        .prepare("SELECT write_cursor, logical_id FROM canonical_nodes ORDER BY write_cursor")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    assert_eq!(
        before, after,
        "TC-11 pin: no migration may populate or re-derive `logical_id` on an existing \
         canonical row"
    );

    let transitioned =
        before.iter().zip(&after).filter(|((_, b), (_, a))| b.is_none() && a.is_some()).count();
    assert_eq!(transitioned, 0, "rows transitioning logical_id NULL → NOT NULL must be 0");

    // The ungoverned rows are still ungoverned (id-space unchanged), even though
    // they now carry provenance.
    assert_eq!(logical_id_of(&conn, "canonical_nodes", 1), None);
    assert_eq!(logical_id_of(&conn, "canonical_edges", 5), None);
}

/// ADR §6 — the head advances to 21 and step-21 is last, with a contiguous ladder.
#[test]
fn s21_is_head_and_schema_version_is_21() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, MIGRATIONS).expect("migration must succeed");

    assert_eq!(user_version(&conn), SCHEMA_VERSION);
    assert_eq!(
        SCHEMA_VERSION, 21,
        "SCHEMA_VERSION must be 21 (step-21 legacy provenance backfill)"
    );
    assert_eq!(
        MIGRATIONS.last().expect("at least one migration").step_id,
        21,
        "step-21 must be the last (head) migration"
    );
}

/// Step-21 is a pure data `UPDATE` — no `CREATE TABLE` / `ADD COLUMN` — so, like
/// step-13, it passes the accretion guard WITHOUT an exemption marker. Asserting
/// this pins the shape: if someone later folds a schema change into step 21, this
/// test forces them to justify it with a marker.
#[test]
fn s21_passes_accretion_guard_without_marker() {
    let step21 = MIGRATIONS
        .iter()
        .find(|m| m.step_id == 21)
        .expect("step 21 (legacy provenance backfill) must exist");

    assert!(
        !step21.sql.contains("MIGRATION-ACCRETION-EXEMPTION"),
        "step-21 is a pure data backfill and needs no accretion exemption"
    );
    check_migration_accretion("021_legacy_provenance_backfill.sql", step21.sql)
        .expect("a pure UPDATE migration must pass the accretion guard unmarked");
}
