#![allow(
    clippy::doc_markdown,
    clippy::expect_used,
    clippy::missing_panics_doc,
    clippy::too_many_lines
)]

//! Integration tests for `filter_json_fused_text_in` and `filter_json_text_in`.

use fathomdb::{
    ChunkInsert, ChunkPolicy, EdgeInsert, Engine, EngineOptions, NodeInsert, Predicate,
    ScalarValue, TraverseDirection, WriteRequest, new_row_id,
};
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

// --- Grouped query and expansion slot IN filter tests ---

/// Seed parent Items with category and child Tasks connected by HAS_TASK edges.
fn seed_categorized_items_with_children(engine: &Engine) {
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-categorized-items-with-children".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "parent-alpha".to_owned(),
                    kind: "Item".to_owned(),
                    properties: r#"{"category":"alpha"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "child-a-1".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"name":"task-a-1"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "child-a-2".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"name":"task-a-2"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "parent-beta".to_owned(),
                    kind: "Item".to_owned(),
                    properties: r#"{"category":"beta"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "child-b-1".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"name":"task-b-1"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "parent-gamma".to_owned(),
                    kind: "Item".to_owned(),
                    properties: r#"{"category":"gamma"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "child-g-1".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"name":"task-g-1"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
            ],
            node_retires: vec![],
            edges: vec![
                EdgeInsert {
                    row_id: new_row_id(),
                    logical_id: "edge-alpha-a1".to_owned(),
                    source_logical_id: "parent-alpha".to_owned(),
                    target_logical_id: "child-a-1".to_owned(),
                    kind: "HAS_TASK".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                },
                EdgeInsert {
                    row_id: new_row_id(),
                    logical_id: "edge-alpha-a2".to_owned(),
                    source_logical_id: "parent-alpha".to_owned(),
                    target_logical_id: "child-a-2".to_owned(),
                    kind: "HAS_TASK".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                },
                EdgeInsert {
                    row_id: new_row_id(),
                    logical_id: "edge-beta-b1".to_owned(),
                    source_logical_id: "parent-beta".to_owned(),
                    target_logical_id: "child-b-1".to_owned(),
                    kind: "HAS_TASK".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                },
                EdgeInsert {
                    row_id: new_row_id(),
                    logical_id: "edge-gamma-g1".to_owned(),
                    source_logical_id: "parent-gamma".to_owned(),
                    target_logical_id: "child-g-1".to_owned(),
                    kind: "HAS_TASK".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                },
            ],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("seed categorized items with children");
}

#[test]
fn filter_json_text_in_on_grouped_query_filters_roots() {
    // JsonPathIn on a grouped query root: only alpha and beta roots should be returned.
    let (_db, engine) = open_engine();
    seed_categorized_items_with_children(&engine);

    let compiled = engine
        .query("Item")
        .filter_json_text_in("$.category", vec!["alpha".to_owned(), "beta".to_owned()])
        .expand("tasks", TraverseDirection::Out, "HAS_TASK", 1, None, None)
        .compile_grouped()
        .expect("compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("executes");

    let root_ids: Vec<&str> = rows.roots.iter().map(|r| r.logical_id.as_str()).collect();
    assert!(
        root_ids.contains(&"parent-alpha"),
        "alpha must be in roots; got {root_ids:?}"
    );
    assert!(
        root_ids.contains(&"parent-beta"),
        "beta must be in roots; got {root_ids:?}"
    );
    assert!(
        !root_ids.contains(&"parent-gamma"),
        "gamma must NOT be in roots; got {root_ids:?}"
    );
}

#[test]
fn json_path_in_as_expansion_slot_filter_filters_expansion_results() {
    // JsonPathIn in expansion filter: only tasks with status in ["active"] should appear.
    let (_db, engine) = open_engine();

    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-meeting-tasks".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "meeting-a".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: r#"{"title":"Planning"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "task-active".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"status":"active"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "task-done".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"status":"done"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
            ],
            node_retires: vec![],
            edges: vec![
                EdgeInsert {
                    row_id: new_row_id(),
                    logical_id: "edge-meeting-active".to_owned(),
                    source_logical_id: "meeting-a".to_owned(),
                    target_logical_id: "task-active".to_owned(),
                    kind: "HAS_TASK".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                },
                EdgeInsert {
                    row_id: new_row_id(),
                    logical_id: "edge-meeting-done".to_owned(),
                    source_logical_id: "meeting-a".to_owned(),
                    target_logical_id: "task-done".to_owned(),
                    kind: "HAS_TASK".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                },
            ],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("seed meeting tasks");

    let node_filter = Predicate::JsonPathIn {
        path: "$.status".to_owned(),
        values: vec![ScalarValue::Text("active".to_owned())],
    };

    let compiled = engine
        .query("Meeting")
        .filter_logical_id_eq("meeting-a")
        .expand(
            "tasks",
            TraverseDirection::Out,
            "HAS_TASK",
            1,
            Some(node_filter),
            None,
        )
        .compile_grouped()
        .expect("compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("executes");

    assert_eq!(rows.roots.len(), 1);
    let nodes = &rows.expansions[0].roots[0].nodes;
    let ids: Vec<&str> = nodes.iter().map(|n| n.logical_id.as_str()).collect();

    assert_eq!(
        nodes.len(),
        1,
        "only task-active passes the IN filter; got {ids:?}"
    );
    assert!(
        ids.contains(&"task-active"),
        "task-active must be in results; got {ids:?}"
    );
    assert!(
        !ids.contains(&"task-done"),
        "task-done must NOT be in results (status=done, not in [active]); got {ids:?}"
    );
}
