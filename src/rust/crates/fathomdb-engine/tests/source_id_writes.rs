use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

#[test]
fn phase9_pack_b_source_id_round_trips_through_canonical_nodes() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "source_id_node");
    {
        let opened = Engine::open(&path).expect("open");
        opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: "alpha".to_string(),
                source_id: Some("S1".to_string()),
            }])
            .expect("write");
        opened.engine.close().unwrap();
    }

    let conn = Connection::open(&path).expect("open sqlite");
    let stored: String = conn
        .query_row("SELECT source_id FROM canonical_nodes WHERE body = ?1", ["alpha"], |row| {
            row.get(0)
        })
        .expect("source_id row");
    assert_eq!(stored, "S1");
}

#[test]
fn phase9_pack_b_source_id_round_trips_through_canonical_edges() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "source_id_edge");
    {
        let opened = Engine::open(&path).expect("open");
        opened
            .engine
            .write(&[PreparedWrite::Edge {
                kind: "rel".to_string(),
                from: "a".to_string(),
                to: "b".to_string(),
                source_id: Some("S2".to_string()),
            }])
            .expect("write");
        opened.engine.close().unwrap();
    }

    let conn = Connection::open(&path).expect("open sqlite");
    let stored: String = conn
        .query_row("SELECT source_id FROM canonical_edges WHERE from_id = ?1", ["a"], |row| {
            row.get(0)
        })
        .expect("source_id row");
    assert_eq!(stored, "S2");
}

#[test]
fn phase9_pack_b_source_id_default_none_persists_as_null() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "source_id_null");
    {
        let opened = Engine::open(&path).expect("open");
        opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: "no source".to_string(),
                source_id: None,
            }])
            .expect("write");
        opened.engine.close().unwrap();
    }

    let conn = Connection::open(&path).expect("open sqlite");
    let stored: Option<String> = conn
        .query_row("SELECT source_id FROM canonical_nodes WHERE body = ?1", ["no source"], |row| {
            row.get(0)
        })
        .expect("row");
    assert!(stored.is_none(), "expected NULL source_id, got {stored:?}");
}

#[test]
fn phase9_pack_b_empty_source_id_is_validation_error() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "source_id_empty");
    let opened = Engine::open(&path).expect("open");
    let err = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "x".to_string(),
            source_id: Some(String::new()),
        }])
        .expect_err("empty source_id must be rejected");
    assert!(matches!(err, fathomdb_engine::EngineError::WriteValidation));
}
