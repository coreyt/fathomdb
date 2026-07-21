use fathomdb_schema::{
    check_migration_accretion, migrate, migrate_with_event_sink, migrate_with_steps, Migration,
    MigrationAccretionError, MigrationError, SCHEMA_VERSION,
};
use rusqlite::Connection;
// Only used by the Unix-gated repo-linter test below.
#[cfg(unix)]
use std::process::Command;
use std::sync::Once;

// Pack 1 (step 9) creates a vec0 virtual table; register sqlite-vec
// once per test-binary so the migration step can execute.
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

#[test]
fn ac_046a_applies_ordered_migrations_to_current_version() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);

    let report = migrate(&conn).unwrap();

    assert_eq!(report.schema_version_before, 1);
    assert_eq!(report.schema_version_after, SCHEMA_VERSION);
    assert_eq!(user_version(&conn), SCHEMA_VERSION);
    assert_eq!(report.migration_steps.len(), 23);
    assert!(report.migration_steps.iter().all(|step| !step.failed));
}

#[test]
fn ac_046b_success_report_contains_step_ids_and_durations() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);

    let report = migrate(&conn).unwrap();

    let step_ids: Vec<u32> = report.migration_steps.iter().map(|step| step.step_id).collect();
    assert_eq!(
        step_ids,
        vec![2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24]
    );
    assert!(report.migration_steps.iter().all(|step| step.duration_ms.is_some()));
}

#[test]
fn ac_046b_success_emits_structured_step_events() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    let mut events = Vec::new();

    migrate_with_event_sink(&conn, fathomdb_schema::MIGRATIONS, |event| {
        events.push(event.clone());
    })
    .unwrap();

    let step_ids: Vec<u32> = events.iter().map(|step| step.step_id).collect();
    assert_eq!(
        step_ids,
        vec![2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24]
    );
    assert!(events.iter().all(|step| step.duration_ms.is_some()));
    assert!(events.iter().all(|step| !step.failed));
}

#[test]
fn ac_046c_and_ac_070_failed_step_reports_failure_and_preserves_user_version() {
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);

    let migrations = [Migration {
        step_id: 2,
        sql: "CREATE TABLE _poison(id INTEGER PRIMARY KEY); SELECT * FROM missing_table",
    }];

    let err = migrate_with_steps(&conn, &migrations).expect_err("poison migration must fail");

    match err {
        MigrationError::MigrationError(report) => {
            assert_eq!(report.schema_version_before, 1);
            assert_eq!(report.schema_version_current, 1);
            assert_eq!(report.migration_steps.len(), 1);
            assert_eq!(report.migration_steps[0].step_id, 2);
            assert!(report.migration_steps[0].failed);
            assert!(report.migration_steps[0].duration_ms.is_some());
        }
        other => panic!("expected MigrationError, got {other:?}"),
    }
    assert_eq!(user_version(&conn), 1);
}

#[test]
fn ac_046c_failed_migration_emits_failed_step_event() {
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    let migrations = [Migration {
        step_id: 2,
        sql: "CREATE TABLE _poison(id INTEGER PRIMARY KEY); SELECT * FROM missing_table",
    }];
    let mut events = Vec::new();

    let err = migrate_with_event_sink(&conn, &migrations, |event| events.push(event.clone()))
        .expect_err("poison migration must fail");

    assert!(matches!(err, MigrationError::MigrationError(_)));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].step_id, 2);
    assert!(events[0].failed);
    assert!(events[0].duration_ms.is_some());
}

#[test]
fn ac_049_accretion_guard_accepts_current_migrations_and_rejects_violator() {
    check_migration_accretion(
        "003_add_profile.sql",
        "CREATE TABLE x(id INTEGER); -- MIGRATION-ACCRETION-EXEMPTION: bootstrap profile table",
    )
    .unwrap();

    let err = check_migration_accretion("004_bad.sql", "ALTER TABLE x ADD COLUMN y TEXT")
        .expect_err("adding a column without removal or exemption must fail");

    assert_eq!(err, MigrationAccretionError { offender: "004_bad.sql".to_string() });
}

#[test]
fn phase9_pack_b_migration_008_adds_source_id_columns_and_indexes() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate(&conn).unwrap();

    let nodes_has_source_id: bool = conn
        .prepare("PRAGMA table_info(canonical_nodes)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .any(|name| name == "source_id");
    assert!(nodes_has_source_id, "canonical_nodes.source_id must be present after migration 8");

    let edges_has_source_id: bool = conn
        .prepare("PRAGMA table_info(canonical_edges)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .any(|name| name == "source_id");
    assert!(edges_has_source_id, "canonical_edges.source_id must be present after migration 8");

    let nodes_idx: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
            ["canonical_nodes_source_id_idx"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(nodes_idx, 1);

    let edges_idx: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
            ["canonical_edges_source_id_idx"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(edges_idx, 1);

    assert_eq!(user_version(&conn), SCHEMA_VERSION);
}

// Slice 15 (G0) — the step-12 substrate migration is pure additive `ALTER …
// ADD COLUMN` (no DROP), so the accretion guard rejects it UNLESS the exemption
// marker is present. This pins that the marker is the only thing letting it pass
// (mirrors `ac_049`).
#[test]
fn s12_g0_substrate_passes_accretion_guard_only_with_marker() {
    let step12 = fathomdb_schema::MIGRATIONS
        .iter()
        .find(|m| m.step_id == 12)
        .expect("step 12 (G0 substrate) must exist");

    // As-authored (with the marker): accepted.
    check_migration_accretion("012_g0_substrate.sql", step12.sql)
        .expect("step 12 must pass the accretion guard with its exemption marker");

    // Strip the exemption-marker line: the same additive-ALTER SQL is now rejected.
    let without_marker: String = step12
        .sql
        .lines()
        .filter(|line| !line.contains("MIGRATION-ACCRETION-EXEMPTION"))
        .collect::<Vec<_>>()
        .join("\n");
    let err = check_migration_accretion("012_g0_substrate.sql", &without_marker)
        .expect_err("step 12 without the exemption marker must be rejected");
    assert_eq!(err, MigrationAccretionError { offender: "012_g0_substrate.sql".to_string() });
}

// Slice 15 (G0) — applying the full migration set lands the transaction-time
// identity substrate: `logical_id` + `superseded_at` on BOTH canonical tables,
// the partial-unique-active index per table, the folded G4/G5 read indexes, and
// `user_version == 12`.
#[test]
fn s12_g0_adds_logical_id_superseded_at_columns_and_partial_unique_index() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate(&conn).unwrap();

    let column_present = |table: &str, column: &str| -> bool {
        conn.prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(Result::ok)
            .any(|name| name == column)
    };
    for table in ["canonical_nodes", "canonical_edges"] {
        assert!(column_present(table, "logical_id"), "{table}.logical_id must be present");
        assert!(column_present(table, "superseded_at"), "{table}.superseded_at must be present");
    }

    let index_present = |name: &str| -> u64 {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
            [name],
            |row| row.get(0),
        )
        .unwrap()
    };
    for idx in [
        "canonical_nodes_logical_active_idx",
        "canonical_edges_logical_active_idx",
        "canonical_nodes_kind_idx",
        "canonical_edges_from_id_idx",
        "canonical_edges_to_id_idx",
    ] {
        assert_eq!(index_present(idx), 1, "index {idx} must exist after step 12");
    }

    // The active-row uniqueness index must be partial (WHERE superseded_at IS NULL)
    // and — per Decision 5 (HITL-SIGNED 2026-06-05) — scoped to `logical_id` ALONE
    // (NOT the pre-Slice-31 compound `(logical_id, kind)`), on BOTH tables.
    for idx_name in ["canonical_nodes_logical_active_idx", "canonical_edges_logical_active_idx"] {
        let active_sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='index' AND name=?1",
                [idx_name],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            active_sql.contains("superseded_at IS NULL"),
            "{idx_name} must be partial on superseded_at IS NULL, got: {active_sql}"
        );
        assert!(
            active_sql.contains("(logical_id)"),
            "{idx_name} must be scoped to logical_id alone (Decision 5), got: {active_sql}"
        );
        assert!(
            !active_sql.contains("kind"),
            "{idx_name} must NOT include kind in its active-uniqueness scope, got: {active_sql}"
        );
    }

    assert_eq!(user_version(&conn), SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 24);
}

// Slice 33 (G3 / F4-READ) — the step-13 additive index makes the op-store
// paginated read-back (`read.collection` / `read.mutations`) index-driven on a
// large multi-collection log. Pure `CREATE INDEX` (no table/column add, no DROP),
// so the accretion guard does not flag it and no exemption marker is required.
#[test]
fn s13_op_store_collection_index_present_after_migrate() {
    register_sqlite_vec_once();
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    migrate(&conn).unwrap();

    let idx_count: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
            ["operational_mutations_collection_id_idx"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(idx_count, 1, "step-13 must create operational_mutations_collection_id_idx");

    // The index is shaped (collection_name, id): leading column collection_name
    // serves the equality predicate, trailing id serves both the after-id cursor
    // range and ORDER BY id (no temp B-tree).
    let idx_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='index' AND name=?1",
            ["operational_mutations_collection_id_idx"],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        idx_sql.contains("operational_mutations"),
        "index must be on operational_mutations, got: {idx_sql}"
    );
    assert!(
        idx_sql.contains("collection_name") && idx_sql.contains("id"),
        "index must be (collection_name, id), got: {idx_sql}"
    );
    let cn = idx_sql.find("collection_name").unwrap();
    let id_pos = idx_sql.rfind("id").unwrap();
    assert!(cn < id_pos, "collection_name must lead id in the composite index, got: {idx_sql}");

    assert_eq!(user_version(&conn), SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 24);
}

// Slice 33 — the step-13 SQL is a pure `CREATE INDEX` (additive index, no table
// or column add). The accretion guard fires only on CREATE TABLE / ADD COLUMN,
// so step-13 passes WITHOUT an exemption marker (contrast step-12's ADD COLUMN,
// which requires the marker).
#[test]
fn s13_op_store_index_passes_accretion_guard_without_marker() {
    let step13 = fathomdb_schema::MIGRATIONS
        .iter()
        .find(|m| m.step_id == 13)
        .expect("step 13 (op-store collection index) must exist");
    assert!(
        !step13.sql.contains("MIGRATION-ACCRETION-EXEMPTION"),
        "an index-only additive step needs no exemption marker"
    );
    check_migration_accretion("013_op_store_collection_index.sql", step13.sql)
        .expect("step 13 (CREATE INDEX only) must pass the accretion guard with no marker");
}

// Exercises the repo's `scripts/agent-lint-migrations.sh` bash linter — Unix dev/CI
// tooling. Windows cannot exec a `.sh` directly (Os 193) and the script is not a Windows
// artifact, so the test is Unix-only (the accretion-guard logic itself is covered by the
// platform-independent `ac_049_accretion_guard_*` test above). (0.8.9 Slice 20, F-9.)
#[cfg(unix)]
#[test]
fn ac_049_repo_linter_accepts_actual_migrations_and_names_violator() {
    let repo =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(4).expect("repo root");
    let script = repo.join("scripts/agent-lint-migrations.sh");

    let ok = Command::new(&script).current_dir(repo).output().expect("run linter");
    assert!(
        ok.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&ok.stdout),
        String::from_utf8_lossy(&ok.stderr)
    );

    let fixture = repo
        .join("src/rust/crates/fathomdb-schema/tests/fixtures/migrations/accretion_violator.sql");
    let failed = Command::new(&script)
        .arg(&fixture)
        .current_dir(repo)
        .output()
        .expect("run linter on fixture");
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("accretion_violator.sql"));
}
