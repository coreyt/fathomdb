#![allow(
    clippy::expect_used,
    clippy::missing_panics_doc,
    clippy::too_many_lines
)]

//! Phase 7a integration tests for the search FFI JSON roundtrip.
//!
//! These tests exercise the `execute_search_json` entry point that Python
//! and TypeScript SDKs will call into via pyo3 / napi wrappers. Each test
//! constructs the request as a JSON string, invokes the FFI, parses the
//! resulting `PySearchRows` JSON, and asserts field values. The FFI layer
//! is pure translation — compile plan → coordinator call → serialize — so
//! these tests double as a contract for the wire format consumed by the
//! higher-level SDKs in Packs 7b / 7c.

use fathomdb::search_ffi::{
    PyHitAttribution, PySearchHit, PySearchHitSource, PySearchMatchMode, PySearchRows,
    execute_search_json,
};
use fathomdb::{
    ChunkInsert, ChunkPolicy, Engine, EngineOptions, FtsPropertyPathSpec, NodeInsert, WriteRequest,
};
use tempfile::NamedTempFile;

fn open_engine() -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    (db, engine)
}

fn seed_budget_goals(engine: &Engine) {
    engine
        .register_fts_property_schema(
            "Goal",
            &["$.name".to_owned(), "$.description".to_owned()],
            None,
        )
        .expect("register property schema");

    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-budget".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "budget-alpha-row".to_owned(),
                    logical_id: "budget-alpha".to_owned(),
                    kind: "Goal".to_owned(),
                    properties:
                        r#"{"name":"budget alpha goal","description":"quarterly budget rollup"}"#
                            .to_owned(),
                    source_ref: Some("seed".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "budget-bravo-row".to_owned(),
                    logical_id: "budget-bravo".to_owned(),
                    kind: "Goal".to_owned(),
                    properties:
                        r#"{"name":"budget bravo goal","description":"annual budget summary"}"#
                            .to_owned(),
                    source_ref: Some("seed".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
            ],
            node_retires: vec![],
            edges: vec![],
            edge_retires: vec![],
            chunks: vec![
                ChunkInsert {
                    id: "budget-alpha-chunk".to_owned(),
                    node_logical_id: "budget-alpha".to_owned(),
                    text_content: "alpha budget quarterly review notes".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "budget-bravo-chunk".to_owned(),
                    node_logical_id: "budget-bravo".to_owned(),
                    text_content: "bravo budget annual summary notes".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
            ],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("seed goals");
}

fn seed_recursive_note(engine: &Engine, logical_id: &str, props: &str) {
    engine
        .register_fts_property_schema_with_entries(
            "Note",
            &[FtsPropertyPathSpec::recursive("$.payload")],
            None,
            &[],
        )
        .expect("register recursive schema");
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-note".to_owned(),
            nodes: vec![NodeInsert {
                row_id: format!("{logical_id}-row"),
                logical_id: logical_id.to_owned(),
                kind: "Note".to_owned(),
                properties: props.to_owned(),
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
        .expect("seed note");
}

fn run(engine: &Engine, request_json: &str) -> PySearchRows {
    let response = execute_search_json(engine, request_json).expect("execute_search_json");
    serde_json::from_str::<PySearchRows>(&response).expect("parse PySearchRows")
}

#[test]
fn text_search_strict_hit_populates_all_fields() {
    let (_db, engine) = open_engine();
    seed_budget_goals(&engine);

    let request = r#"{
        "mode": "text_search",
        "root_kind": "Goal",
        "strict_query": "budget",
        "relaxed_query": null,
        "limit": 10,
        "filters": [{"type":"filter_kind_eq","kind":"Goal"}],
        "attribution_requested": false
    }"#;

    let rows = run(&engine, request);
    assert!(!rows.hits.is_empty());
    assert_eq!(rows.strict_hit_count, rows.hits.len());
    assert_eq!(rows.relaxed_hit_count, 0);
    assert!(!rows.fallback_used);
    assert!(!rows.was_degraded);

    let hit: &PySearchHit = &rows.hits[0];
    assert!(hit.score > 0.0, "score must be positive");
    assert!(matches!(hit.match_mode, PySearchMatchMode::Strict));
    assert!(matches!(
        hit.source,
        PySearchHitSource::Chunk | PySearchHitSource::Property
    ));
    assert!(hit.snippet.is_some());
    assert!(hit.written_at > 0);
    assert!(hit.projection_row_id.is_some());
    assert!(hit.attribution.is_none());
    // Node fields present.
    assert_eq!(hit.node.kind, "Goal");
    assert!(hit.node.logical_id.starts_with("budget-"));
}

#[test]
fn text_search_strict_miss_fires_relaxed_branch() {
    let (_db, engine) = open_engine();
    seed_budget_goals(&engine);

    // "budget quarterly nonexistentterm" — strict AND misses because of the
    // dead term; adaptive relaxation strips to "budget OR quarterly OR
    // nonexistentterm" and finds the seeded rows.
    let request = r#"{
        "mode": "text_search",
        "root_kind": "Goal",
        "strict_query": "budget quarterly zzznopeterm",
        "relaxed_query": null,
        "limit": 10,
        "filters": [{"type":"filter_kind_eq","kind":"Goal"}],
        "attribution_requested": false
    }"#;

    let rows = run(&engine, request);
    assert!(rows.fallback_used, "relaxed must fire on strict miss");
    assert!(!rows.hits.is_empty());
    assert_eq!(rows.strict_hit_count, 0);
    assert_eq!(rows.relaxed_hit_count, rows.hits.len());
    assert!(
        rows.hits
            .iter()
            .all(|h| matches!(h.match_mode, PySearchMatchMode::Relaxed))
    );
}

#[test]
fn text_search_with_attribution_populates_matched_paths() {
    let (_db, engine) = open_engine();
    seed_recursive_note(
        &engine,
        "note-attrib",
        r#"{"payload":{"body":"shipping quarterly docs"}}"#,
    );

    let request = r#"{
        "mode": "text_search",
        "root_kind": "Note",
        "strict_query": "shipping",
        "relaxed_query": null,
        "limit": 10,
        "filters": [],
        "attribution_requested": true
    }"#;

    let rows = run(&engine, request);
    assert!(!rows.hits.is_empty());
    let hit = &rows.hits[0];
    let att: &PyHitAttribution = hit
        .attribution
        .as_ref()
        .expect("attribution populated when requested");
    assert_eq!(att.matched_paths, vec!["$.payload.body".to_owned()]);
}

#[test]
fn fallback_search_two_shape_fires_relaxed_branch() {
    let (_db, engine) = open_engine();
    seed_budget_goals(&engine);

    let request = r#"{
        "mode": "fallback_search",
        "root_kind": "Goal",
        "strict_query": "zzznope1 zzznope2",
        "relaxed_query": "budget OR nothing",
        "limit": 10,
        "filters": [{"type":"filter_kind_eq","kind":"Goal"}],
        "attribution_requested": false
    }"#;

    let rows = run(&engine, request);
    assert!(rows.fallback_used);
    assert!(!rows.hits.is_empty());
    assert_eq!(rows.strict_hit_count, 0);
    assert_eq!(rows.relaxed_hit_count, rows.hits.len());
    assert!(
        rows.hits
            .iter()
            .all(|h| matches!(h.match_mode, PySearchMatchMode::Relaxed))
    );
    assert!(!rows.was_degraded);
}

#[test]
fn fallback_search_strict_only_matches_strict_only_text_search() {
    let (_db, engine) = open_engine();
    seed_budget_goals(&engine);

    let fallback_strict = r#"{
        "mode": "fallback_search",
        "root_kind": "Goal",
        "strict_query": "budget",
        "relaxed_query": null,
        "limit": 10,
        "filters": [{"type":"filter_kind_eq","kind":"Goal"}],
        "attribution_requested": false
    }"#;

    let rows_fb = run(&engine, fallback_strict);
    assert!(!rows_fb.hits.is_empty());
    assert!(!rows_fb.fallback_used);
    assert_eq!(rows_fb.relaxed_hit_count, 0);
    assert_eq!(rows_fb.strict_hit_count, rows_fb.hits.len());
    assert!(
        rows_fb
            .hits
            .iter()
            .all(|h| matches!(h.match_mode, PySearchMatchMode::Strict))
    );
}
