#![allow(clippy::expect_used, clippy::missing_panics_doc)]

//! Pack B integration tests for profile FFI functions.
//!
//! These tests exercise the five new FFI functions added to admin_ffi:
//! set_fts_profile_json, get_fts_profile_json, set_vec_profile_json,
//! get_vec_profile_json, preview_projection_impact_json.

use fathomdb::admin_ffi::{
    get_fts_profile_json, get_vec_profile_json, preview_projection_impact_json,
    set_fts_profile_json, set_vec_profile_json,
};
use fathomdb::{ChunkPolicy, Engine, EngineOptions, NodeInsert, WriteRequest};
use serde_json::Value;
use tempfile::NamedTempFile;

fn open_engine() -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    (db, engine)
}

fn parse_json(s: &str) -> Value {
    serde_json::from_str(s).expect("parse JSON")
}

#[test]
fn set_get_fts_profile_roundtrip() {
    let (_db, engine) = open_engine();

    let request = r#"{"kind":"KnowledgeItem","tokenizer":"precision-optimized"}"#;
    let response = set_fts_profile_json(&engine, request).expect("set_fts_profile_json");
    let record = parse_json(&response);
    assert_eq!(record["kind"].as_str(), Some("KnowledgeItem"));
    // precision-optimized resolves to the unicode61 tokenizer
    let tokenizer = record["tokenizer"].as_str().expect("tokenizer field");
    assert!(!tokenizer.is_empty(), "tokenizer should not be empty");

    let get_response =
        get_fts_profile_json(&engine, "KnowledgeItem").expect("get_fts_profile_json");
    let get_record = parse_json(&get_response);
    assert_eq!(
        get_record["kind"].as_str(),
        Some("KnowledgeItem"),
        "kind should round-trip"
    );
    assert_eq!(
        get_record["tokenizer"].as_str(),
        record["tokenizer"].as_str(),
        "tokenizer should round-trip"
    );
}

#[test]
fn get_fts_profile_returns_null_when_unset() {
    let (_db, engine) = open_engine();

    let response = get_fts_profile_json(&engine, "NonExistentKind").expect("get_fts_profile_json");
    assert_eq!(response, "null", "unset profile should serialize as null");
}

#[test]
fn preview_fts_count() {
    let (_db, engine) = open_engine();

    // Insert some nodes
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "node-1-row".to_owned(),
                    logical_id: "node-1".to_owned(),
                    kind: "Article".to_owned(),
                    properties: r#"{"title":"First article"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "node-2-row".to_owned(),
                    logical_id: "node-2".to_owned(),
                    kind: "Article".to_owned(),
                    properties: r#"{"title":"Second article"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
            ],
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
        .expect("insert nodes");

    let response = preview_projection_impact_json(&engine, "Article", "fts")
        .expect("preview_projection_impact_json");
    let record = parse_json(&response);
    let rows_to_rebuild = record["rows_to_rebuild"]
        .as_u64()
        .expect("rows_to_rebuild field");
    assert!(
        rows_to_rebuild > 0,
        "rows_to_rebuild should be > 0 after inserting nodes (got {rows_to_rebuild})"
    );
}

#[test]
fn set_get_vec_profile_roundtrip() {
    let (_db, engine) = open_engine();

    let config = r#"{"model_identity":"test-model","model_version":"v1","dimensions":384,"normalization_policy":"l2"}"#;
    let set_response = set_vec_profile_json(&engine, config).expect("set_vec_profile_json");
    let set_record = parse_json(&set_response);
    assert_eq!(
        set_record["model_identity"].as_str(),
        Some("test-model"),
        "model_identity should be set"
    );
    assert_eq!(
        set_record["dimensions"].as_u64(),
        Some(384),
        "dimensions should be 384"
    );

    let get_response = get_vec_profile_json(&engine).expect("get_vec_profile_json");
    let get_record = parse_json(&get_response);
    assert_eq!(
        get_record["model_identity"].as_str(),
        Some("test-model"),
        "model_identity should round-trip"
    );
    assert_eq!(
        get_record["dimensions"].as_u64(),
        Some(384),
        "dimensions should round-trip"
    );
}

#[test]
fn invalid_fts_request_returns_error() {
    let (_db, engine) = open_engine();

    let err =
        set_fts_profile_json(&engine, "{not valid json}").expect_err("should fail on bad JSON");
    let msg = format!("{err}");
    assert!(
        msg.contains("parse"),
        "error message should mention parse: {msg}"
    );
}
