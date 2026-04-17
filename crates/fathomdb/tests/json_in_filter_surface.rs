#![allow(clippy::expect_used, clippy::missing_panics_doc)]

//! Integration tests for `filter_json_fused_text_in` and `filter_json_text_in`.

use fathomdb::{ChunkInsert, ChunkPolicy, Engine, EngineOptions, NodeInsert, WriteRequest};
use tempfile::NamedTempFile;

fn open_engine() -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    (db, engine)
}

/// Seed nodes for fused IN tests: three Issues with status "open",
/// "pending", "review" and a chunk containing a common keyword.
fn seed_issues_with_status(engine: &Engine) {
    engine
        .register_fts_property_schema(
            "Issue",
            &["$.status".to_owned(), "$.title".to_owned()],
            None,
        )
        .expect("register property schema");

    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-issues".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "issue-open-row".to_owned(),
                    logical_id: "issue-open".to_owned(),
                    kind: "Issue".to_owned(),
                    properties: r#"{"status":"open","title":"Bug in login"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "issue-pending-row".to_owned(),
                    logical_id: "issue-pending".to_owned(),
                    kind: "Issue".to_owned(),
                    properties: r#"{"status":"pending","title":"Optimize queries"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "issue-review-row".to_owned(),
                    logical_id: "issue-review".to_owned(),
                    kind: "Issue".to_owned(),
                    properties: r#"{"status":"review","title":"API contract"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "issue-closed-row".to_owned(),
                    logical_id: "issue-closed".to_owned(),
                    kind: "Issue".to_owned(),
                    properties: r#"{"status":"closed","title":"Old fix"}"#.to_owned(),
                    source_ref: None,
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
                    id: "issue-open-chunk".to_owned(),
                    node_logical_id: "issue-open".to_owned(),
                    text_content: "tracker issue open state".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "issue-pending-chunk".to_owned(),
                    node_logical_id: "issue-pending".to_owned(),
                    text_content: "tracker issue pending state".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "issue-review-chunk".to_owned(),
                    node_logical_id: "issue-review".to_owned(),
                    text_content: "tracker issue review state".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "issue-closed-chunk".to_owned(),
                    node_logical_id: "issue-closed".to_owned(),
                    text_content: "tracker issue closed state".to_owned(),
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
        .expect("seed issues");
}

/// Seed nodes for non-fused IN tests: nodes with `category` field.
fn seed_items_with_category(engine: &Engine) {
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-items".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: "item-alpha-row".to_owned(),
                    logical_id: "item-alpha".to_owned(),
                    kind: "Item".to_owned(),
                    properties: r#"{"category":"alpha","name":"First"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "item-beta-row".to_owned(),
                    logical_id: "item-beta".to_owned(),
                    kind: "Item".to_owned(),
                    properties: r#"{"category":"beta","name":"Second"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "item-gamma-row".to_owned(),
                    logical_id: "item-gamma".to_owned(),
                    kind: "Item".to_owned(),
                    properties: r#"{"category":"gamma","name":"Third"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: "item-delta-row".to_owned(),
                    logical_id: "item-delta".to_owned(),
                    kind: "Item".to_owned(),
                    properties: r#"{"category":"delta","name":"Fourth"}"#.to_owned(),
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
        .expect("seed items");
}

// --- Fused IN tests ---

#[test]
fn filter_json_fused_text_in_returns_matching_nodes_on_text_search() {
    let (_db, engine) = open_engine();
    seed_issues_with_status(&engine);

    let rows = engine
        .query("Issue")
        .text_search("tracker", 20)
        .filter_json_fused_text_in("$.status", vec!["open".to_owned(), "review".to_owned()])
        .expect("fusion gate passes")
        .execute()
        .expect("search executes");

    let ids: Vec<&str> = rows
        .hits
        .iter()
        .map(|h| h.node.logical_id.as_str())
        .collect();
    assert!(
        ids.contains(&"issue-open"),
        "open must be returned; got {ids:?}"
    );
    assert!(
        ids.contains(&"issue-review"),
        "review must be returned; got {ids:?}"
    );
    assert!(
        !ids.contains(&"issue-pending"),
        "pending must NOT be returned; got {ids:?}"
    );
    assert!(
        !ids.contains(&"issue-closed"),
        "closed must NOT be returned; got {ids:?}"
    );
}

#[test]
fn filter_json_fused_text_in_missing_schema_returns_error() {
    let (_db, engine) = open_engine();
    // No FTS schema registered for "Unschema" kind — must return error.
    let result = engine
        .query("Unschema")
        .text_search("foo", 5)
        .filter_json_fused_text_in("$.status", vec!["open".to_owned()]);

    assert!(
        result.is_err(),
        "missing schema must return BuilderValidationError"
    );
}

#[test]
fn filter_json_fused_text_in_unchecked_panics_on_empty_values() {
    use fathomdb::QueryBuilder;
    let result = std::panic::catch_unwind(|| {
        QueryBuilder::nodes("Issue").filter_json_fused_text_in_unchecked("$.status", vec![])
    });
    assert!(result.is_err(), "empty values must panic");
}

// --- Non-fused IN tests ---

#[test]
fn filter_json_text_in_returns_matching_nodes_on_flat_query() {
    let (_db, engine) = open_engine();
    seed_items_with_category(&engine);

    let rows = engine
        .query("Item")
        .filter_json_text_in("$.category", vec!["alpha".to_owned(), "beta".to_owned()])
        .execute()
        .expect("flat query executes");

    let ids: Vec<&str> = rows.nodes.iter().map(|n| n.logical_id.as_str()).collect();
    assert!(
        ids.contains(&"item-alpha"),
        "alpha must be returned; got {ids:?}"
    );
    assert!(
        ids.contains(&"item-beta"),
        "beta must be returned; got {ids:?}"
    );
    assert!(
        !ids.contains(&"item-gamma"),
        "gamma must NOT be returned; got {ids:?}"
    );
    assert!(
        !ids.contains(&"item-delta"),
        "delta must NOT be returned; got {ids:?}"
    );
}

#[test]
fn filter_json_text_in_panics_on_empty_values() {
    use fathomdb::QueryBuilder;
    let result = std::panic::catch_unwind(|| {
        QueryBuilder::nodes("Item").filter_json_text_in("$.category", vec![])
    });
    assert!(result.is_err(), "empty values must panic");
}

#[test]
fn filter_json_text_in_on_text_search_applies_residual_filter() {
    let (_db, engine) = open_engine();
    seed_issues_with_status(&engine);

    // filter_json_text_in on text_search — non-fused, applied as residual.
    let rows = engine
        .query("Issue")
        .text_search("tracker", 20)
        .filter_json_text_in("$.status", vec!["open".to_owned(), "pending".to_owned()])
        .execute()
        .expect("search executes");

    let ids: Vec<&str> = rows
        .hits
        .iter()
        .map(|h| h.node.logical_id.as_str())
        .collect();
    assert!(
        ids.contains(&"issue-open"),
        "open must be returned; got {ids:?}"
    );
    assert!(
        ids.contains(&"issue-pending"),
        "pending must be returned; got {ids:?}"
    );
    assert!(
        !ids.contains(&"issue-review"),
        "review must NOT be returned; got {ids:?}"
    );
    assert!(
        !ids.contains(&"issue-closed"),
        "closed must NOT be returned; got {ids:?}"
    );
}
