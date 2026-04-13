#![allow(clippy::expect_used, clippy::missing_panics_doc)]

//! Pack P7.6a integration tests for the admin FFI JSON roundtrip.
//!
//! These tests exercise the `register_fts_property_schema_with_entries_json`
//! entry point that Python and TypeScript SDKs call into via pyo3 / napi
//! wrappers. Each test constructs the request as a JSON string, invokes
//! the FFI, parses the resulting `FtsPropertySchemaRecord` JSON, and
//! verifies the engine-side schema state plus a follow-on search to
//! confirm recursive-mode rows are written to `fts_node_property_positions`.

use fathomdb::admin_ffi::register_fts_property_schema_with_entries_json;
use fathomdb::search_ffi::{PySearchHitSource, PySearchRows, execute_search_json};
use fathomdb::{ChunkPolicy, Engine, EngineOptions, NodeInsert, WriteRequest};
use rusqlite::Connection;
use serde_json::Value;
use tempfile::NamedTempFile;

fn parse_record(json: &str) -> Value {
    serde_json::from_str(json).expect("parse record JSON")
}

fn open_engine() -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    (db, engine)
}

fn insert_knowledge_item(engine: &Engine, logical_id: &str, properties: &str) {
    engine
        .writer()
        .submit(WriteRequest {
            label: format!("seed-{logical_id}"),
            nodes: vec![NodeInsert {
                row_id: format!("{logical_id}-row"),
                logical_id: logical_id.to_owned(),
                kind: "KnowledgeItem".to_owned(),
                properties: properties.to_owned(),
                source_ref: Some("seed".to_owned()),
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            }],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("seed knowledge item");
}

#[test]
fn register_fts_property_schema_with_entries_json_accepts_recursive_path() {
    let (db, engine) = open_engine();

    let request = r#"{
        "kind": "KnowledgeItem",
        "entries": [
            {"path": "$.title", "mode": "scalar"},
            {"path": "$.payload", "mode": "recursive"}
        ],
        "separator": " ",
        "exclude_paths": []
    }"#;
    let response =
        register_fts_property_schema_with_entries_json(&engine, request).expect("register");
    let record = parse_record(&response);
    assert_eq!(record["kind"].as_str(), Some("KnowledgeItem"));
    assert_eq!(record["separator"].as_str(), Some(" "));
    // Pack P7.7-fix: the load path now calls parse_property_schema_json
    // on the stored JSON, so recursive-bearing schemas round-trip via
    // both the `entries` array (mode-accurate) and `property_paths`
    // (flat display list).
    let paths: Vec<String> = record["property_paths"]
        .as_array()
        .expect("property_paths array")
        .iter()
        .map(|v| v.as_str().expect("path string").to_owned())
        .collect();
    assert_eq!(paths, vec!["$.title".to_owned(), "$.payload".to_owned()]);
    let entries = record["entries"]
        .as_array()
        .expect("entries array populated");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["path"].as_str(), Some("$.title"));
    assert_eq!(entries[0]["mode"].as_str(), Some("scalar"));
    assert_eq!(entries[1]["path"].as_str(), Some("$.payload"));
    assert_eq!(entries[1]["mode"].as_str(), Some("recursive"));
    assert!(
        record["exclude_paths"]
            .as_array()
            .expect("exclude_paths array")
            .is_empty(),
        "exclude_paths should be empty for this request"
    );

    insert_knowledge_item(
        &engine,
        "ki-alpha",
        r#"{"title":"Alpha doc","payload":{"body":"quarterly rollup summary"}}"#,
    );

    // Raw SQL: the recursive walk should have populated
    // fts_node_property_positions for this node.
    let conn = Connection::open(db.path()).expect("open raw conn");
    let pos_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM fts_node_property_positions WHERE kind = ?1",
            ["KnowledgeItem"],
            |row| row.get(0),
        )
        .expect("count positions");
    assert!(
        pos_count >= 1,
        "recursive walk should populate position map (got {pos_count})"
    );

    // And a property-backed search should find the node via the recursive
    // leaf, with attribution populated.
    let search_request = r#"{
        "mode": "text_search",
        "root_kind": "KnowledgeItem",
        "strict_query": "quarterly",
        "relaxed_query": null,
        "limit": 10,
        "filters": [],
        "attribution_requested": true
    }"#;
    let search_response =
        execute_search_json(&engine, search_request).expect("execute_search_json");
    let rows: PySearchRows = serde_json::from_str(&search_response).expect("parse rows");
    assert!(!rows.hits.is_empty(), "expected at least one hit");
    let property_hits: Vec<_> = rows
        .hits
        .iter()
        .filter(|h| matches!(h.source, PySearchHitSource::Property))
        .collect();
    assert!(
        !property_hits.is_empty(),
        "expected at least one property-backed hit"
    );
    let attributed = property_hits
        .iter()
        .find(|h| h.attribution.is_some())
        .expect("attribution payload present");
    let att = attributed.attribution.as_ref().expect("attribution");
    assert!(
        att.matched_paths.iter().any(|p| p.starts_with("$.payload")),
        "at least one matched path should be under $.payload (got {:?})",
        att.matched_paths
    );
}

#[test]
fn register_fts_property_schema_with_entries_json_round_trips_scalar_only() {
    let (_db, engine) = open_engine();

    let request = r#"{
        "kind": "Goal",
        "entries": [
            {"path": "$.name", "mode": "scalar"},
            {"path": "$.description", "mode": "scalar"}
        ]
    }"#;
    let response = register_fts_property_schema_with_entries_json(&engine, request)
        .expect("register scalar-only");
    let record = parse_record(&response);
    assert_eq!(record["kind"].as_str(), Some("Goal"));
    assert_eq!(record["separator"].as_str(), Some(" "));
    let paths: Vec<String> = record["property_paths"]
        .as_array()
        .expect("property_paths array")
        .iter()
        .map(|v| v.as_str().expect("path string").to_owned())
        .collect();
    assert_eq!(paths.len(), 2);
    assert!(paths.contains(&"$.name".to_owned()));
    assert!(paths.contains(&"$.description".to_owned()));
    // Pack P7.7-fix: scalar-only schemas also round-trip their entries,
    // each with mode "scalar".
    let entries = record["entries"]
        .as_array()
        .expect("entries array populated");
    assert_eq!(entries.len(), 2);
    for entry in entries {
        assert_eq!(entry["mode"].as_str(), Some("scalar"));
    }
}

#[test]
fn register_fts_property_schema_with_entries_json_rejects_bad_json() {
    let (_db, engine) = open_engine();
    let err = register_fts_property_schema_with_entries_json(&engine, "{not json}")
        .expect_err("parse should fail");
    let msg = format!("{err}");
    assert!(msg.contains("parse"), "message should mention parse: {msg}");
}
