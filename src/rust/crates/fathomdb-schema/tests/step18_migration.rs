//! 0.8.16 Slice 5 (F9 KEYSTONE) — step-18 `importance` migration tests.
//!
//! Covers plan-0.8.16 §2 R-F9-1 (the node-level `importance REAL` column lands,
//! nullable, with the 3-way sentinel `NULL`=absent) and R-F9-4 (absent-importance
//! is the graceful-neutral state — a plain additive REAL with no data migration).
//! ADR authority: `dev/adr/ADR-0.8.16-f9-importance-confidence-ranking.md` §2.1 +
//! `dev/design/0.8.16-slice-0-f9-onnx-design.md` §3.
//!
//! `importance` is a caller-supplied ranking scalar on `canonical_nodes`,
//! symmetric with the existing genuine-NULL `canonical_edges.confidence`
//! (step-14). Additive `ADD COLUMN` with a leading `MIGRATION-ACCRETION-EXEMPTION`
//! marker; pre-existing rows read `NULL` in-place (no data migration). The step
//! does NOT rewrite vec0 / vector rows (eu7 no-op basis — ADR §4).

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

/// R-F9-1 — after the full migration set, `canonical_nodes` has `importance`
/// and the head is 18 / step-18 is last.
#[test]
fn s18_importance_column_present_and_schema_version_is_18() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, MIGRATIONS).expect("migration must succeed");

    let cols = column_names(&conn, "canonical_nodes");
    assert!(
        cols.contains(&"importance".to_string()),
        "canonical_nodes must have importance after step-18, got: {cols:?}"
    );

    assert_eq!(user_version(&conn), SCHEMA_VERSION);
    // 0.8.18 Slice 5 moved the head to step-19 (#5 vector-equivalence probe);
    // step-18 (F9 importance) is no longer the head but MUST still be present.
    assert_eq!(
        SCHEMA_VERSION, 22,
        "SCHEMA_VERSION must be 22 (step-22 node validity window, R-20-NV)"
    );
    assert!(
        MIGRATIONS.iter().any(|m| m.step_id == 18),
        "step-18 (F9 importance) must still be present in the migration set"
    );
}

/// R-F9-1 (forward-only) + R-F9-4 (graceful-absent) — open a DB at the OLD schema
/// (version 17) with a pre-existing row, run ONLY step-18, and assert: it reaches
/// 18, ONLY step-18 ran, the legacy row's `importance` back-fills to `NULL`
/// (never-assigned = graceful-absent), and NO vec0 / vector rows are touched
/// (eu7 no-op basis).
#[test]
fn s18_forward_only_from_v17_importance_null_and_no_vector_rewrite() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);

    // Bring the DB up to the OLD head (version 17) — everything before step-18.
    let steps_to_17: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id <= 17).cloned().collect();
    migrate_with_steps(&conn, &steps_to_17).expect("migrate to v17");
    assert_eq!(user_version(&conn), 17, "precondition: DB is at the old schema version 17");

    // A pre-existing (pre-importance) canonical node.
    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, row_kind)
         VALUES(1, 'note', 'legacy body about widgets', 'leaf')",
        [],
    )
    .expect("legacy node insert at v17");

    // Seed a vec0/vector-row bookkeeping row so we can prove step-18 does NOT
    // rewrite the vector corpus (eu7 no-op basis — ADR §4).
    conn.execute(
        "INSERT INTO _fathomdb_vector_rows(rowid, kind, write_cursor) VALUES(1, 'note', 1)",
        [],
    )
    .expect("seed vector-row bookkeeping");
    let vrows_before: i64 =
        conn.query_row("SELECT COUNT(*) FROM _fathomdb_vector_rows", [], |r| r.get(0)).unwrap();

    // Apply ONLY the forward step-18 migration.
    let step18_only: Vec<_> = MIGRATIONS.iter().filter(|m| m.step_id == 18).cloned().collect();
    let report = migrate_with_steps(&conn, &step18_only).expect("forward migrate to v18");

    // Forward-only: exactly the single new step ran; nothing before it re-ran.
    assert_eq!(report.schema_version_before, 17);
    assert_eq!(report.schema_version_after, 18);
    let ran: Vec<u32> = report.migration_steps.iter().map(|s| s.step_id).collect();
    assert_eq!(ran, vec![18], "only step-18 must run when starting from v17");
    assert_eq!(user_version(&conn), 18);

    // R-F9-4 graceful-absent: the legacy row reads NULL importance (never-assigned).
    let importance: Option<f64> = conn
        .query_row("SELECT importance FROM canonical_nodes WHERE write_cursor = 1", [], |r| {
            r.get(0)
        })
        .expect("legacy row must be readable after step-18");
    assert!(importance.is_none(), "legacy importance must back-fill to NULL (graceful-absent)");

    // eu7 no-op basis: the vector-row bookkeeping is byte-untouched (step-18 is a
    // pure ADD COLUMN — it does not rewrite / re-embed / re-quantize any vec0 row).
    let vrows_after: i64 =
        conn.query_row("SELECT COUNT(*) FROM _fathomdb_vector_rows", [], |r| r.get(0)).unwrap();
    assert_eq!(vrows_before, vrows_after, "step-18 must NOT rewrite the vector corpus");
    assert_eq!(vrows_after, 1, "the seeded vector row must survive step-18 untouched");
}

/// R-F9-1 (roundtrip at the SQL layer) — the 3-way sentinel is storable and
/// reads back exact: `NULL` (absent), `0.0` (floor), and an explicit `(0.0,1.0]`.
#[test]
fn s18_importance_three_way_sentinel_roundtrips() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, MIGRATIONS).expect("migrate to head");

    conn.execute(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, row_kind, importance)
         VALUES(1, 'note', 'absent', 'leaf', NULL),
                (2, 'note', 'floor',  'leaf', 0.0),
                (3, 'note', 'high',   'leaf', 1.0)",
        [],
    )
    .expect("insert 3-way sentinel rows");

    let read = |wc: i64| -> Option<f64> {
        conn.query_row(
            "SELECT importance FROM canonical_nodes WHERE write_cursor = ?1",
            [wc],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert!(read(1).is_none(), "NULL = never assigned (graceful-absent)");
    assert_eq!(read(2), Some(0.0), "0.0 = explicit floor/de-weight");
    assert_eq!(read(3), Some(1.0), "1.0 = explicit importance");
}

/// R-F9-1 (guarded) — step-18 is a pure additive `ADD COLUMN` (no DROP), so the
/// accretion guard REJECTS it unless the exemption marker is present.
#[test]
fn s18_passes_accretion_guard_only_with_marker() {
    let step18 =
        MIGRATIONS.iter().find(|m| m.step_id == 18).expect("step 18 (F9 importance) must exist");

    check_migration_accretion("018_f9_importance.sql", step18.sql)
        .expect("step 18 must pass the accretion guard with its exemption marker");

    let without_marker: String = step18
        .sql
        .lines()
        .filter(|line| !line.contains("MIGRATION-ACCRETION-EXEMPTION"))
        .collect::<Vec<_>>()
        .join("\n");
    check_migration_accretion("018_f9_importance.sql", &without_marker)
        .expect_err("step 18 without the exemption marker must be rejected");
}
