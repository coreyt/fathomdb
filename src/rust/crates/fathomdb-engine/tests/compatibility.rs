use fathomdb_embedder_api::EmbedderIdentity;
use fathomdb_engine::{Engine, EngineOpenError};
use fathomdb_schema::{Migration, SCHEMA_VERSION, SQLITE_SUFFIX};
use rusqlite::{params, Connection};
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn set_user_version(conn: &Connection, version: u32) {
    conn.pragma_update(None, "user_version", version).unwrap();
}

fn create_profile(conn: &Connection, name: &str, revision: &str, dimension: u32) {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _fathomdb_embedder_profiles(
            profile TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            revision TEXT NOT NULL,
            dimension INTEGER NOT NULL
        )",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO _fathomdb_embedder_profiles(profile, name, revision, dimension)
         VALUES('default', ?1, ?2, ?3)",
        params![name, revision, dimension],
    )
    .unwrap();
}

#[test]
fn ac_047_rejects_05_shaped_database_before_use() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "legacy");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(include_str!("fixtures/v05_shape.sql")).unwrap();
    drop(conn);

    let err = Engine::open(&path).expect_err("0.5-shaped database must not open");

    match err {
        EngineOpenError::IncompatibleSchemaVersion { seen, supported } => {
            assert_eq!(seen, 1);
            assert_eq!(supported, SCHEMA_VERSION);
            assert!(err.to_string().contains("1"));
        }
        other => panic!("expected IncompatibleSchemaVersion, got {other:?}"),
    }
}

#[test]
fn future_schema_version_is_also_rejected() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "future");
    let conn = Connection::open(&path).unwrap();
    set_user_version(&conn, SCHEMA_VERSION + 1);
    drop(conn);

    let err = Engine::open(&path).expect_err("future schema version must not open");

    match err {
        EngineOpenError::IncompatibleSchemaVersion { seen, supported } => {
            assert_eq!(seen, SCHEMA_VERSION + 1);
            assert_eq!(supported, SCHEMA_VERSION);
        }
        other => panic!("expected IncompatibleSchemaVersion, got {other:?}"),
    }
}

#[test]
fn engine_open_emits_migration_step_events() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "events");
    let conn = Connection::open(&path).unwrap();
    set_user_version(&conn, 1);
    drop(conn);
    let mut events = Vec::new();

    let opened = Engine::open_with_migration_event_sink(&path, |event| events.push(event.clone()))
        .expect("open should migrate");
    opened.engine.close().unwrap();

    let step_ids: Vec<u32> = events.iter().map(|event| event.step_id).collect();
    assert_eq!(step_ids, vec![2, 3, 4, 5, 6, 7, 8]);
    assert!(events.iter().all(|event| event.duration_ms.is_some()));
    assert!(events.iter().all(|event| !event.failed));
    assert_eq!(opened.report.migration_steps, events);
}

#[test]
fn engine_open_poison_migration_reports_failure_and_preserves_user_version() {
    static POISON: &[Migration] = &[Migration {
        step_id: 2,
        sql: "CREATE TABLE _poison(id INTEGER PRIMARY KEY); SELECT * FROM missing_table",
    }];
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "poison");
    let conn = Connection::open(&path).unwrap();
    set_user_version(&conn, 1);
    drop(conn);
    let mut events = Vec::new();

    let err = Engine::open_with_migrations_for_test(&path, POISON, |event| {
        events.push(event.clone());
    })
    .expect_err("poison migration should fail during open");

    match err {
        EngineOpenError::MigrationError {
            schema_version_before,
            schema_version_current,
            step_id,
        } => {
            assert_eq!(schema_version_before, 1);
            assert_eq!(schema_version_current, 1);
            assert_eq!(step_id, 2);
        }
        other => panic!("expected MigrationError, got {other:?}"),
    }
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].step_id, 2);
    assert!(events[0].failed);
    assert!(events[0].duration_ms.is_some());

    let conn = Connection::open(&path).unwrap();
    let version: u32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0)).unwrap();
    assert_eq!(version, 1);
}

#[test]
fn ac_048_rejects_stored_embedder_identity_mismatch() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "identity");
    let conn = Connection::open(&path).unwrap();
    set_user_version(&conn, SCHEMA_VERSION);
    create_profile(&conn, "other-embedder", "rev-b", 384);
    drop(conn);

    let err = Engine::open(&path).expect_err("identity mismatch must fail open");

    match err {
        EngineOpenError::EmbedderIdentityMismatch { stored, supplied } => {
            assert_eq!(stored, EmbedderIdentity::new("other-embedder", "rev-b", 384));
            assert_eq!(supplied, EmbedderIdentity::new("fathomdb-noop", "0.6.0-scaffold", 384));
        }
        other => panic!("expected EmbedderIdentityMismatch, got {other:?}"),
    }
}

#[test]
fn ac_048b_rejects_stored_embedder_dimension_mismatch() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "dimension");
    let conn = Connection::open(&path).unwrap();
    set_user_version(&conn, SCHEMA_VERSION);
    create_profile(&conn, "fathomdb-noop", "0.6.0-scaffold", 768);
    drop(conn);

    let err = Engine::open(&path).expect_err("dimension mismatch must fail open");

    match err {
        EngineOpenError::EmbedderDimensionMismatch { stored, supplied } => {
            assert_eq!(stored, 768);
            assert_eq!(supplied, 384);
        }
        other => panic!("expected EmbedderDimensionMismatch, got {other:?}"),
    }
}

#[test]
fn second_live_open_is_locked_and_close_releases_lock() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "locked");

    let opened = Engine::open(&path).unwrap();
    let err = Engine::open(&path).expect_err("second open must be locked");
    assert!(matches!(err, EngineOpenError::DatabaseLocked { holder_pid: Some(_) }));

    opened.engine.close().unwrap();
    let reopened = Engine::open(&path).expect("close must release lock");
    reopened.engine.close().unwrap();
}

#[test]
fn open_error_display_is_sanitized() {
    let err = EngineOpenError::MigrationError {
        schema_version_before: 1,
        schema_version_current: 1,
        step_id: 2,
    };

    let display = err.to_string();
    assert!(!display.contains("SELECT"));
    assert!(!display.contains(dir_path_fragment()));
    assert!(!display.contains("line "));
    assert!(!display.contains("column "));
}

fn dir_path_fragment() -> &'static str {
    "/home/"
}
