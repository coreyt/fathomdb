#![allow(clippy::expect_used, clippy::missing_panics_doc)]

use fathomdb::{ChunkInsert, ChunkPolicy, Engine, EngineOptions, NodeInsert, WriteRequest};
use tempfile::NamedTempFile;

fn open_engine() -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    (db, engine)
}

fn seed_tasks(engine: &Engine) {
    engine
        .register_fts_property_schema(
            "Task",
            &["$.title".to_owned(), "$.resolved".to_owned()],
            None,
        )
        .expect("register property schema");

    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-tasks".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "task-open-row".to_owned(),
                    logical_id: "task-open".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"title":"implement login","resolved":false}"#.to_owned(),
                    source_ref: Some("seed-tasks".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "task-done-row".to_owned(),
                    logical_id: "task-done".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"title":"implement logout","resolved":true}"#.to_owned(),
                    source_ref: Some("seed-tasks".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "task-open2-row".to_owned(),
                    logical_id: "task-open2".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"title":"implement dashboard","resolved":false}"#.to_owned(),
                    source_ref: Some("seed-tasks".to_owned()),
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
                    id: "task-open-chunk".to_owned(),
                    node_logical_id: "task-open".to_owned(),
                    text_content: "implement the login feature with OAuth2 support".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "task-done-chunk".to_owned(),
                    node_logical_id: "task-done".to_owned(),
                    text_content: "implement the logout feature with session clearing".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "task-open2-chunk".to_owned(),
                    node_logical_id: "task-open2".to_owned(),
                    text_content: "implement the dashboard with charts and filters".to_owned(),
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
        .expect("seed tasks");
}

#[test]
fn fused_bool_eq_false_filters_to_unresolved_tasks_only() {
    let (_db, engine) = open_engine();
    seed_tasks(&engine);

    let rows = engine
        .query("Task")
        .text_search("implement", 10)
        .filter_json_fused_bool_eq("$.resolved", false)
        .expect("filter_json_fused_bool_eq succeeds")
        .execute()
        .expect("search executes");

    let ids: Vec<&str> = rows
        .hits
        .iter()
        .map(|h| h.node.logical_id.as_str())
        .collect();
    assert_eq!(
        rows.hits.len(),
        2,
        "expected 2 unresolved tasks, got {ids:?}"
    );
    for hit in &rows.hits {
        assert_ne!(
            hit.node.logical_id, "task-done",
            "resolved task must not appear in unresolved filter"
        );
    }
}

#[test]
fn fused_bool_eq_true_filters_to_resolved_tasks_only() {
    let (_db, engine) = open_engine();
    seed_tasks(&engine);

    let rows = engine
        .query("Task")
        .text_search("implement", 10)
        .filter_json_fused_bool_eq("$.resolved", true)
        .expect("filter_json_fused_bool_eq succeeds")
        .execute()
        .expect("search executes");

    let ids: Vec<&str> = rows
        .hits
        .iter()
        .map(|h| h.node.logical_id.as_str())
        .collect();
    assert_eq!(rows.hits.len(), 1, "expected 1 resolved task, got {ids:?}");
    assert_eq!(rows.hits[0].node.logical_id, "task-done");
}

#[test]
fn fused_bool_eq_missing_schema_raises_validation_error() {
    let (_db, engine) = open_engine();

    let result = engine
        .query("UnregisteredKind")
        .text_search("anything", 10)
        .filter_json_fused_bool_eq("$.resolved", false);

    assert!(
        result.is_err(),
        "expected BuilderValidationError for missing schema"
    );
}
