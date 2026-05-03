use fathomdb_schema::{
    check_migration_accretion, migrate, migrate_with_event_sink, migrate_with_steps, Migration,
    MigrationAccretionError, MigrationError, SCHEMA_VERSION,
};
use rusqlite::Connection;

fn user_version(conn: &Connection) -> u32 {
    conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0)).unwrap()
}

fn set_user_version(conn: &Connection, version: u32) {
    conn.pragma_update(None, "user_version", version).unwrap();
}

#[test]
fn ac_046a_applies_ordered_migrations_to_current_version() {
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);

    let report = migrate(&conn).unwrap();

    assert_eq!(report.schema_version_before, 1);
    assert_eq!(report.schema_version_after, SCHEMA_VERSION);
    assert_eq!(user_version(&conn), SCHEMA_VERSION);
    assert_eq!(report.migration_steps.len(), 3);
    assert!(report.migration_steps.iter().all(|step| !step.failed));
}

#[test]
fn ac_046b_success_report_contains_step_ids_and_durations() {
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);

    let report = migrate(&conn).unwrap();

    let step_ids: Vec<u32> = report.migration_steps.iter().map(|step| step.step_id).collect();
    assert_eq!(step_ids, vec![2, 3, 4]);
    assert!(report.migration_steps.iter().all(|step| step.duration_ms.is_some()));
}

#[test]
fn ac_046b_success_emits_structured_step_events() {
    let conn = Connection::open_in_memory().unwrap();
    set_user_version(&conn, 1);
    let mut events = Vec::new();

    migrate_with_event_sink(&conn, fathomdb_schema::MIGRATIONS, |event| {
        events.push(event.clone());
    })
    .unwrap();

    let step_ids: Vec<u32> = events.iter().map(|step| step.step_id).collect();
    assert_eq!(step_ids, vec![2, 3, 4]);
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
