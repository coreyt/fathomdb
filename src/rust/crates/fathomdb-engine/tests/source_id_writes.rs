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
                source_id: fathomdb_engine::SourceId::new("S1").expect("test source id"),
                logical_id: None,
                state: fathomdb_engine::InitialState::Active,
                reason: None,
                valid_from: None,
                valid_until: None,
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
                source_id: fathomdb_engine::SourceId::new("S2").expect("test source id"),
                logical_id: None,
                body: None,
                t_valid: None,
                t_invalid: None,
                confidence: None,
                extractor_model_id: None,
                temporal_fallback: None,
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

/// 0.8.20 Slice 5c (R-20-E3) — REPLACES `phase9_pack_b_source_id_default_none_
/// persists_as_null`, which asserted the pre-0.8.20 contract that an omitted
/// `source_id` persists as NULL. That contract WAS THE DEFECT: `excise_source`
/// addresses rows by `source_id`, so a NULL-provenance row is reachable by no
/// erasure call at all. The behaviour is now the inverse — every write stores a
/// provenance, and "no provenance" cannot be expressed on `PreparedWrite`.
#[test]
fn every_write_stores_a_non_null_source_id() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "source_id_never_null");
    {
        let opened = Engine::open(&path).expect("open");
        opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: "no source".to_string(),
                source_id: fathomdb_engine::SourceId::new("doc-provenanced")
                    .expect("test source id"),
                logical_id: None,
                state: fathomdb_engine::InitialState::Active,
                reason: None,
                valid_from: None,
                valid_until: None,
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
    assert_eq!(
        stored.as_deref(),
        Some("doc-provenanced"),
        "a canonical row must never be stored with NULL provenance"
    );
}

/// 0.8.20 Slice 5c (R-20-E3) — REPLACES `phase9_pack_b_empty_source_id_is_
/// validation_error`. The rejection did not disappear, it MOVED: an empty
/// `source_id` used to be caught by the write path (bypassable, since the facade
/// re-exports `PreparedWrite` and `Engine::write` is public), and is now caught
/// by the only constructor that can produce the value. Same error, enforced one
/// layer earlier and unbypassably.
#[test]
fn empty_or_reserved_source_id_is_rejected_by_the_constructor() {
    use fathomdb_engine::{EngineError, SourceId};

    assert!(
        matches!(SourceId::new(String::new()), Err(EngineError::WriteValidation)),
        "an empty id names no source"
    );
    assert!(
        matches!(SourceId::new("   "), Err(EngineError::WriteValidation)),
        "a whitespace-only id names no source"
    );
    assert!(
        matches!(SourceId::new("_engine:coverage"), Err(EngineError::WriteValidation)),
        "the `_engine:` namespace is reserved for engine-derived rows"
    );
    assert!(
        matches!(SourceId::new(SourceId::LEGACY_PRE_0_8_20), Err(EngineError::WriteValidation)),
        "a caller must not be able to hide rows among the step-21 backfill"
    );

    let ok = SourceId::new("doc-1").expect("an ordinary document id is accepted");
    assert_eq!(ok.as_str(), "doc-1");
}
