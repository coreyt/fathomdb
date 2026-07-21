//! 0.8.18 Slice 5 (#5 vector-equivalence probe KEYSTONE) — step-19
//! `_fathomdb_embed_probe` migration tests.
//!
//! Covers `dev/design/0.8.18-slice-0-vector-equivalence-publish-design.md` §U1
//! R-VEQ-1 (the `_fathomdb_embed_probe` self-check substrate table lands with the
//! frozen shape — probe text + **UN-centered f32 reference vector** + embedder
//! identity + dim, and **NO bit column**: the Phase-1 mean-centered bits are NEVER
//! persisted, they are recomputed at check time). ADR authority:
//! `dev/adr/ADR-0.8.18-vector-equivalence-self-check.md`.
//!
//! `SCHEMA_VERSION` advances 18 → 19. The migration only CREATEs the empty table;
//! the engine populates the 45 probe rows at first vector-kind registration (open
//! path). Additive `CREATE TABLE` (no DROP) with a leading
//! `MIGRATION-ACCRETION-EXEMPTION` marker; the step does NOT rewrite vec0 rows
//! (eu7 no-op basis).

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

fn column_names(conn: &Connection, table: &str) -> Vec<String> {
    conn.prepare(&format!("PRAGMA table_info({table})"))
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .collect()
}

/// R-VEQ-1 — after the full migration set, the head is 19, step-19 is last, and
/// `_fathomdb_embed_probe` has the frozen shape (probe text + f32 reference +
/// identity + dim), with NO persisted-bits column.
#[test]
fn s19_embed_probe_table_present_and_schema_version_is_19() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, MIGRATIONS).expect("migration must succeed");

    assert_eq!(user_version(&conn), SCHEMA_VERSION);
    assert_eq!(
        SCHEMA_VERSION, 24,
        "SCHEMA_VERSION must be 24 (step-24 projection-registry EAV + property-FTS, Slice 15d)"
    );
    assert_eq!(
        MIGRATIONS.last().expect("at least one migration").step_id,
        24,
        "step-24 (projection-registry EAV + property-FTS, Slice 15d) must be the last (head) migration"
    );

    let cols = column_names(&conn, "_fathomdb_embed_probe");
    for expected in [
        "probe_ordinal",
        "probe_text",
        "reference_vec",
        "embedder_name",
        "embedder_revision",
        "dim",
    ] {
        assert!(
            cols.contains(&expected.to_string()),
            "_fathomdb_embed_probe must have `{expected}`, got: {cols:?}"
        );
    }
    // U1-d — the P1 bits are NEVER persisted; only the UN-centered f32 reference
    // is stored. Guard against a stray bit/embedding-bin column.
    for forbidden in ["embedding_bin", "reference_bits", "bits", "reference_bin"] {
        assert!(
            !cols.iter().any(|c| c == forbidden),
            "_fathomdb_embed_probe must NOT persist a bit column (`{forbidden}`), got: {cols:?}"
        );
    }
    // Fresh table — the engine populates the 45 rows at first vector-kind
    // registration, so the migration alone leaves it empty.
    let rows: i64 =
        conn.query_row("SELECT COUNT(*) FROM _fathomdb_embed_probe", [], |r| r.get(0)).unwrap();
    assert_eq!(rows, 0, "migration creates the empty table; the engine populates it at open");
}

/// R-VEQ-1 (forward-only) — open a DB at the OLD schema (version 18), run ONLY
/// step-19, and assert it reaches 19, ONLY step-19 ran, and NO vec0 / vector rows
/// are touched (eu7 no-op basis — the step creates a fresh sidecar table only).
#[test]
fn s19_forward_only_from_v18_no_vector_rewrite() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);

    let steps_to_18: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id <= 18).cloned().collect();
    migrate_with_steps(&conn, &steps_to_18).expect("migrate to v18");
    assert_eq!(user_version(&conn), 18, "precondition: DB is at the old schema version 18");

    conn.execute(
        "INSERT INTO _fathomdb_vector_rows(rowid, kind, write_cursor) VALUES(1, 'note', 1)",
        [],
    )
    .expect("seed vector-row bookkeeping");
    let vrows_before: i64 =
        conn.query_row("SELECT COUNT(*) FROM _fathomdb_vector_rows", [], |r| r.get(0)).unwrap();

    let step19_only: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id == 19).cloned().collect();
    let report = migrate_with_steps(&conn, &step19_only).expect("forward migrate to v19");

    assert_eq!(report.schema_version_before, 18);
    assert_eq!(report.schema_version_after, 19);
    let ran: Vec<u32> = report.migration_steps.iter().map(|s| s.step_id).collect();
    assert_eq!(ran, vec![19], "only step-19 must run when starting from v18");
    assert_eq!(user_version(&conn), 19);

    let vrows_after: i64 =
        conn.query_row("SELECT COUNT(*) FROM _fathomdb_vector_rows", [], |r| r.get(0)).unwrap();
    assert_eq!(vrows_before, vrows_after, "step-19 must NOT rewrite the vector corpus");
    assert_eq!(vrows_after, 1, "the seeded vector row must survive step-19 untouched");
}

/// R-VEQ-1 (guarded) — step-19 is an additive `CREATE TABLE` (no DROP), so the
/// accretion guard REJECTS it unless the exemption marker is present.
#[test]
fn s19_passes_accretion_guard_only_with_marker() {
    let step19 = MIGRATIONS
        .iter()
        .find(|m| m.step_id == 19)
        .expect("step 19 (#5 vector-equivalence probe) must exist");

    check_migration_accretion("019_embed_probe.sql", step19.sql)
        .expect("step 19 must pass the accretion guard with its exemption marker");

    let without_marker: String = step19
        .sql
        .lines()
        .filter(|line| !line.contains("MIGRATION-ACCRETION-EXEMPTION"))
        .collect::<Vec<_>>()
        .join("\n");
    check_migration_accretion("019_embed_probe.sql", &without_marker)
        .expect_err("step 19 without the exemption marker must be rejected");
}
