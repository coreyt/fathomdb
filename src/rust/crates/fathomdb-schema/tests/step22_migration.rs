//! 0.8.20 Slice 10b (R-20-NV) — step-22 node-validity-window migration tests.
//!
//! Covers `dev/plans/plan-0.8.20.md` §3 R-20-NV: `canonical_nodes` gains a
//! world-time validity window as two **INTEGER epoch-second** columns,
//! `valid_from` / `valid_until`, with the HALF-OPEN convention
//! `[valid_from, valid_until)` and NULL meaning UNBOUNDED on that side.
//!
//! Three load-bearing properties:
//!
//! - **P1 shape** — both columns exist, are declared `INTEGER`, and are
//!   NULLABLE with no DEFAULT (so `ADD COLUMN` back-fills NULL in place).
//! - **P2 no-op on existing data** — every row that existed at v21 comes out of
//!   the migration with NULL/NULL, i.e. unbounded ⇒ always valid ⇒ its
//!   default-view visibility is UNCHANGED. This is the assertion the release
//!   contract demands: the migration must not make one shipped row disappear.
//! - **P3 crash-safety / idempotence** — the runner wraps the step and the
//!   `PRAGMA user_version` bump in one `BEGIN IMMEDIATE`, so a failed step
//!   leaves the DB at 21 with the columns absent, and a completed step never
//!   re-runs (load-bearing: `ALTER TABLE … ADD COLUMN` has no `IF NOT EXISTS`).
//!
//! **INTEGER-vs-TEXT divergence (introduced here, RESOLVED by TC-33).** When
//! step 22 shipped, these node columns were INTEGER epoch seconds while the
//! shipped `canonical_edges.t_valid` / `t_invalid` (step 14) were ISO-8601 TEXT.
//! Step 22 was node-only and left the edge columns alone. TC-33's **step 23**
//! converts the edge columns to INTEGER epoch seconds too, closing the
//! divergence; `s22_edge_temporal_columns_text_at_v22_become_integer_at_head`
//! now pins the TRANSITION (TEXT at v22, INTEGER at head) rather than a frozen
//! inconsistency.
//!
//! `SCHEMA_VERSION` advances 21 → 22 for this step. Head is now 23 (step 23,
//! TC-33). One migration per release (I-6).

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

/// `(name, declared_type, notnull, dflt_value)` for one column, via
/// `PRAGMA table_info` — the raw on-disk shape, not an engine projection.
fn column_info(
    conn: &Connection,
    table: &str,
    column: &str,
) -> Option<(String, i64, Option<String>)> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})")).unwrap();
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .unwrap();
    for row in rows.flatten() {
        if row.0 == column {
            return Some((row.1, row.2, row.3));
        }
    }
    None
}

fn steps_through(limit: u32) -> Vec<fathomdb_schema::Migration> {
    MIGRATIONS.iter().filter(|m| m.step_id <= limit).cloned().collect()
}

/// Migrate to v21 and seed rows that predate the validity window.
fn seed_v21() -> Connection {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate_with_steps(&conn, &steps_through(21)).expect("migrate to v21");
    assert_eq!(user_version(&conn), 21, "precondition: DB at the pre-slice head 21");

    conn.execute_batch(
        "INSERT INTO canonical_nodes(write_cursor, kind, body, source_id, logical_id)
             VALUES(1, 'doc', 'pre-existing governed row', 'doc-caller-1', 'lid-a');
         INSERT INTO canonical_nodes(write_cursor, kind, body, source_id, logical_id)
             VALUES(2, 'doc', 'pre-existing anonymous row', 'doc-caller-2', NULL);",
    )
    .expect("seed v21 rows");

    conn
}

/// P1 — both validity columns land on `canonical_nodes`, declared INTEGER,
/// nullable, with no DEFAULT.
#[test]
fn s22_validity_columns_are_nullable_integer_with_no_default() {
    let conn = seed_v21();
    migrate_with_steps(&conn, &steps_through(22)).expect("migrate to v22");

    for column in ["valid_from", "valid_until"] {
        let (decl_type, notnull, default) = column_info(&conn, "canonical_nodes", column)
            .unwrap_or_else(|| panic!("canonical_nodes.{column} must exist after step 22"));
        assert_eq!(
            decl_type.to_ascii_uppercase(),
            "INTEGER",
            "{column} must be INTEGER epoch seconds (R-20-NV release contract), \
             deliberately NOT the ISO-8601 TEXT shape used by canonical_edges"
        );
        assert_eq!(notnull, 0, "{column} must be NULLABLE — NULL means UNBOUNDED on that side");
        assert_eq!(
            default, None,
            "{column} must have NO DEFAULT so ADD COLUMN back-fills NULL in place \
             (unbounded ⇒ always valid ⇒ existing rows stay visible)"
        );
    }
}

/// **P2 keystone.** Every row that existed at v21 comes out of step 22 with
/// NULL/NULL — unbounded on both sides, therefore valid at every instant,
/// therefore visible in the default read view exactly as before the migration.
#[test]
fn s22_preexisting_rows_stay_visible_in_default_view() {
    let conn = seed_v21();
    let before: i64 =
        conn.query_row("SELECT COUNT(*) FROM canonical_nodes", [], |r| r.get(0)).unwrap();

    migrate_with_steps(&conn, &steps_through(22)).expect("migrate to v22");

    let after: i64 =
        conn.query_row("SELECT COUNT(*) FROM canonical_nodes", [], |r| r.get(0)).unwrap();
    assert_eq!(before, after, "step 22 must not add or drop rows");

    let unbounded: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canonical_nodes
             WHERE valid_from IS NULL AND valid_until IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        unbounded, after,
        "EVERY pre-existing row must back-fill to NULL/NULL (unbounded ⇒ always valid); \
         any other value would silently change default-view visibility"
    );

    // The half-open predicate the engine compiles must admit every one of those
    // rows at an ARBITRARY instant — this is the visibility claim, asserted
    // against the raw table rather than through a read verb.
    for instant in [i64::MIN, -1, 0, 1, 1_000, 4_102_444_800, i64::MAX] {
        let visible: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM canonical_nodes
                 WHERE (valid_from IS NULL OR valid_from <= ?1)
                   AND (valid_until IS NULL OR valid_until > ?1)",
                [instant],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            visible, after,
            "unbounded rows must be valid at instant {instant} under the half-open predicate"
        );
    }
}

/// The half-open `[valid_from, valid_until)` convention, asserted at the raw
/// SQL level so the boundary semantics are pinned independently of the engine.
#[test]
fn s22_window_is_half_open_lower_inclusive_upper_exclusive() {
    let conn = seed_v21();
    migrate_with_steps(&conn, &steps_through(22)).expect("migrate to v22");
    conn.execute(
        "UPDATE canonical_nodes SET valid_from = 1000, valid_until = 2000 WHERE write_cursor = 1",
        [],
    )
    .unwrap();

    let visible_at = |t: i64| -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM canonical_nodes
             WHERE write_cursor = 1
               AND (valid_from IS NULL OR valid_from <= ?1)
               AND (valid_until IS NULL OR valid_until > ?1)",
            [t],
            |r| r.get(0),
        )
        .unwrap()
    };

    assert_eq!(visible_at(999), 0, "before the window: invisible");
    assert_eq!(visible_at(1000), 1, "lower bound is INCLUSIVE");
    assert_eq!(visible_at(1999), 1, "inside the window: visible");
    assert_eq!(visible_at(2000), 0, "upper bound is EXCLUSIVE");
    assert_eq!(visible_at(2001), 0, "after the window: invisible");
}

/// **TC-33 update.** This test used to be
/// `s22_edge_temporal_columns_are_untouched_text` and pinned the deliberate
/// TEXT-vs-INTEGER divergence step 22 introduced as a "recorded decision".
/// TC-33 (step 23) RESOLVES that divergence, so the test now pins the
/// TRANSITION instead of a frozen inconsistency:
///
/// - AT v22 the edge columns are still ISO-8601 TEXT and step 22 leaves them
///   alone (a real property of step 22 IN ISOLATION — it is node-only);
/// - AT head (v23) they are INTEGER epoch seconds — the divergence is closed.
#[test]
fn s22_edge_temporal_columns_text_at_v22_become_integer_at_head() {
    // --- at v22: still TEXT, and step 22 is node-only ---
    let conn = seed_v21();
    migrate_with_steps(&conn, &steps_through(22)).expect("migrate to v22");

    for column in ["t_valid", "t_invalid"] {
        let (decl_type, _, _) = column_info(&conn, "canonical_edges", column)
            .unwrap_or_else(|| panic!("canonical_edges.{column} must still exist"));
        assert_eq!(
            decl_type.to_ascii_uppercase(),
            "TEXT",
            "at v22 canonical_edges.{column} is still ISO-8601 TEXT — step 22 is node-only \
             and does not touch the edge columns; TC-33's step 23 converts them"
        );
    }
    assert!(
        column_info(&conn, "canonical_edges", "valid_from").is_none(),
        "step 22 is node-only: canonical_edges must NOT gain valid_from"
    );
    assert!(
        column_info(&conn, "canonical_edges", "valid_until").is_none(),
        "step 22 is node-only: canonical_edges must NOT gain valid_until"
    );

    // --- at head (v23): INTEGER epoch, divergence closed ---
    let head = seed_v21();
    migrate_with_steps(&head, MIGRATIONS).expect("migrate to head");
    for column in ["t_valid", "t_invalid"] {
        let (decl_type, _, _) = column_info(&head, "canonical_edges", column)
            .unwrap_or_else(|| panic!("canonical_edges.{column} must still exist at head"));
        assert_eq!(
            decl_type.to_ascii_uppercase(),
            "INTEGER",
            "TC-33: at head canonical_edges.{column} is INTEGER epoch seconds — the step-22 \
             divergence is resolved, not merely recorded"
        );
    }
}

/// P3 — a completed step never re-runs. Load-bearing because
/// `ALTER TABLE … ADD COLUMN` has no `IF NOT EXISTS` form, so a second
/// application would error with "duplicate column name".
#[test]
fn s22_is_idempotent_across_repeated_migrate_calls() {
    let conn = seed_v21();
    // TC-33: v22 is no longer head (23 is), so migrate the WHOLE registry to
    // head first; then re-running must be the true no-op this test guards.
    migrate_with_steps(&conn, MIGRATIONS).expect("first migrate to head");
    assert_eq!(user_version(&conn), SCHEMA_VERSION);

    // Re-running the whole registry must be a no-op, not a duplicate-column error.
    let report = migrate_with_steps(&conn, MIGRATIONS).expect("re-running migrate must be a no-op");
    assert!(
        report.migration_steps.is_empty(),
        "no step may re-run once user_version is at head; ran {:?}",
        report.migration_steps
    );
    assert_eq!(user_version(&conn), SCHEMA_VERSION);
}

/// P3 — the step is atomic with its version bump: a poisoned batch leaves the
/// DB at 21 with NEITHER column present, so the retry applies the step whole.
#[test]
fn s22_failed_step_rolls_back_columns_and_version_together() {
    let conn = seed_v21();

    let poisoned = vec![fathomdb_schema::Migration {
        step_id: 22,
        sql: "-- MIGRATION-ACCRETION-EXEMPTION: poisoned step-22 stand-in
              ALTER TABLE canonical_nodes ADD COLUMN valid_from INTEGER;
              ALTER TABLE canonical_nodes ADD COLUMN valid_until INTEGER;
              SELECT this_is_not_valid_sql_and_must_abort_the_step();",
    }];
    migrate_with_steps(&conn, &poisoned).expect_err("poisoned step must fail");

    assert_eq!(user_version(&conn), 21, "a failed step must leave user_version at 21");
    assert!(
        column_info(&conn, "canonical_nodes", "valid_from").is_none(),
        "a failed step must roll back its ADD COLUMN — otherwise the retry hits \
         'duplicate column name' and the DB is wedged"
    );
    assert!(column_info(&conn, "canonical_nodes", "valid_until").is_none());

    // The real step then applies cleanly on the rolled-back DB.
    migrate_with_steps(&conn, &steps_through(22)).expect("retry must succeed");
    assert_eq!(user_version(&conn), 22);
    assert!(column_info(&conn, "canonical_nodes", "valid_from").is_some());
}

/// Step 22 is head and `SCHEMA_VERSION` tracks it.
#[test]
fn s22_is_head_and_schema_version_is_22() {
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
}

/// The accretion guard: step 22 adds schema without a DROP, so it REQUIRES the
/// exemption marker, and the marker must actually be present in the shipped SQL.
#[test]
fn s22_carries_the_accretion_exemption_marker() {
    let step = MIGRATIONS.iter().find(|m| m.step_id == 22).expect("step-22 must exist");
    check_migration_accretion("step-22", step.sql)
        .expect("step-22 must satisfy the accretion guard");
    assert!(
        step.sql.contains("-- MIGRATION-ACCRETION-EXEMPTION: "),
        "an additive ADD COLUMN step must carry the exemption marker"
    );
    assert!(!step.sql.to_ascii_uppercase().contains("DROP "), "step 22 must be purely additive");
}

/// R-20-NV explicitly excludes transaction-time. Step 22 must add world-time
/// columns ONLY — no `history_as_of` / transaction-time substrate, even partial.
#[test]
fn s22_adds_no_transaction_time_substrate() {
    let step = MIGRATIONS.iter().find(|m| m.step_id == 22).expect("step-22 must exist");
    let sql = step.sql.to_ascii_lowercase();
    for forbidden in ["history_as_of", "tx_from", "tx_until", "transaction_time", "recorded_at"] {
        assert!(
            !sql.contains(forbidden),
            "history_as_of / transaction-time is explicitly OUT OF SCOPE for R-20-NV; \
             step 22 must not introduce `{forbidden}`"
        );
    }
}
