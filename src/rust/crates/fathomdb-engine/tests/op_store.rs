use std::time::Instant;

use fathomdb_engine::{Engine, EngineError, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn open_fixture(name: &str) -> (TempDir, std::path::PathBuf, fathomdb_engine::OpenedEngine) {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, name);
    let opened = Engine::open(&path).unwrap();
    (dir, path, opened)
}

fn register_collection(
    engine: &Engine,
    name: &str,
    kind: &str,
    schema_json: &str,
) -> Result<(), EngineError> {
    engine.write(&[PreparedWrite::AdminSchema {
        name: name.to_string(),
        kind: kind.to_string(),
        schema_json: schema_json.to_string(),
        retention_json: "{}".to_string(),
    }])?;
    Ok(())
}

#[test]
fn ac_061a_append_only_log_preserves_history() {
    let (_dir, path, opened) = open_fixture("append");
    register_collection(&opened.engine, "events", "append_only_log", "{}").unwrap();

    opened
        .engine
        .write(&[
            PreparedWrite::OpStore {
                collection: "events".to_string(),
                record_key: "same".to_string(),
                schema_id: None,
                body: r#"{"n":1}"#.to_string(),
            },
            PreparedWrite::OpStore {
                collection: "events".to_string(),
                record_key: "same".to_string(),
                schema_id: None,
                body: r#"{"n":2}"#.to_string(),
            },
        ])
        .unwrap();

    opened.engine.close().unwrap();
    let conn = Connection::open(&path).unwrap();
    let rows: Vec<String> = conn
        .prepare(
            "SELECT payload_json FROM operational_mutations
             WHERE collection_name = 'events'
             ORDER BY id",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows, vec![r#"{"n":1}"#, r#"{"n":2}"#]);
}

#[test]
fn ac_060b_schema_validation_failure_leaves_no_batch_residue() {
    let (_dir, path, opened) = open_fixture("validation");
    register_collection(
        &opened.engine,
        "validated",
        "append_only_log",
        r#"{"type":"string","pattern":"^(a|a)*$"}"#,
    )
    .unwrap();

    let err = opened
        .engine
        .write(&[
            PreparedWrite::Node { kind: "doc".to_string(), body: "must not commit".to_string() },
            PreparedWrite::OpStore {
                collection: "validated".to_string(),
                record_key: "bad".to_string(),
                schema_id: Some("validated".to_string()),
                body: r#""aaaaaaaaaaaaaaaaaaaaaaaaaaaaaab""#.to_string(),
            },
        ])
        .expect_err("schema failure must reject the whole batch");
    assert_eq!(err, EngineError::SchemaValidation);

    opened.engine.close().unwrap();
    let conn = Connection::open(&path).unwrap();
    let node_count: u32 = conn
        .query_row(
            "SELECT count(*) FROM canonical_nodes WHERE body = 'must not commit'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let op_count: u32 = conn
        .query_row(
            "SELECT count(*) FROM operational_mutations WHERE collection_name = 'validated'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(node_count, 0);
    assert_eq!(op_count, 0);
}

#[test]
fn ac_061b_latest_state_keeps_one_current_row_per_key() {
    let (_dir, path, opened) = open_fixture("latest");
    register_collection(&opened.engine, "state", "latest_state", "{}").unwrap();

    opened
        .engine
        .write(&[
            PreparedWrite::OpStore {
                collection: "state".to_string(),
                record_key: "k".to_string(),
                schema_id: None,
                body: r#"{"n":1}"#.to_string(),
            },
            PreparedWrite::OpStore {
                collection: "state".to_string(),
                record_key: "k".to_string(),
                schema_id: None,
                body: r#"{"n":2}"#.to_string(),
            },
        ])
        .unwrap();

    opened.engine.close().unwrap();
    let conn = Connection::open(&path).unwrap();
    let (count, payload): (u32, String) = conn
        .query_row(
            "SELECT count(*), payload_json FROM operational_state
             WHERE collection_name = 'state' AND record_key = 'k'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(payload, r#"{"n":2}"#);
}

#[test]
fn ac_061c_and_ac_062_schema_has_authoritative_op_store_tables_only() {
    let (_dir, path, opened) = open_fixture("schema");
    opened.engine.close().unwrap();
    let conn = Connection::open(&path).unwrap();

    for table in ["operational_collections", "operational_mutations", "operational_state"] {
        let exists: u32 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "{table} must exist");
    }
    let current_exists: u32 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name = 'operational_current'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(current_exists, 0);

    let columns: Vec<String> = conn
        .prepare("PRAGMA table_info(operational_collections)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        columns,
        vec!["name", "kind", "schema_json", "retention_json", "format_version", "created_at"]
    );
    for forbidden in ["disabled_at", "renamed_from", "retired_at", "status"] {
        assert!(!columns.iter().any(|column| column == forbidden));
    }
}

#[test]
fn ac_063a_b_projection_failures_are_durable_across_restart() {
    let (_dir, path, opened) = open_fixture("projection");

    let cursor = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "force_projection_failure".to_string(),
            body: "doc".to_string(),
        }])
        .unwrap()
        .cursor;
    opened.engine.close().unwrap();

    let reopened = Engine::open(&path).unwrap();
    reopened.engine.close().unwrap();
    let conn = Connection::open(&path).unwrap();
    let count: u32 = conn
        .query_row(
            "SELECT count(*) FROM operational_mutations
             WHERE collection_name = 'projection_failures'
               AND json_extract(payload_json, '$.write_cursor') = ?1",
            [cursor],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn ac_064_schema_validation_rejects_redos_pattern_quickly_and_writer_recovers() {
    let (_dir, _path, opened) = open_fixture("redos");
    register_collection(
        &opened.engine,
        "validated",
        "append_only_log",
        r#"{"type":"string","pattern":"^(a|a)*$"}"#,
    )
    .unwrap();

    let started = Instant::now();
    let err = opened
        .engine
        .write(&[PreparedWrite::OpStore {
            collection: "validated".to_string(),
            record_key: "bad".to_string(),
            schema_id: Some("validated".to_string()),
            body: r#""aaaaaaaaaaaaaaaaaaaaaaaaaaaaaab""#.to_string(),
        }])
        .expect_err("bad payload must fail schema validation");
    assert_eq!(err, EngineError::SchemaValidation);
    assert!(started.elapsed().as_millis() <= 100);

    opened
        .engine
        .write(&[PreparedWrite::OpStore {
            collection: "validated".to_string(),
            record_key: "ok".to_string(),
            schema_id: Some("validated".to_string()),
            body: r#""aaaa""#.to_string(),
        }])
        .expect("writer must accept a benign follow-up write");
}

#[test]
fn ac_065_schema_registration_rejects_external_refs() {
    let (_dir, _path, opened) = open_fixture("refs");

    for uri in ["http://example/", "https://example/", "file:///etc/passwd"] {
        let err = register_collection(
            &opened.engine,
            "bad_ref",
            "append_only_log",
            &format!(r#"{{"$ref":"{uri}"}}"#),
        )
        .expect_err("external ref must be rejected");
        assert_eq!(err, EngineError::SchemaValidation);
    }
}
