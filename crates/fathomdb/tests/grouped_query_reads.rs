#![allow(
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::unreadable_literal
)]

use fathomdb::{
    ChunkInsert, ChunkPolicy, CompileError, EdgeInsert, Engine, EngineError, EngineOptions,
    NodeInsert, Predicate, ScalarValue, TraverseDirection, WriteRequest, new_row_id,
};
use tempfile::NamedTempFile;

fn open_engine() -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    (db, engine)
}

fn seed_meeting_graph(engine: &Engine) {
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-grouped-query".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "meeting-1".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties: r#"{"title":"Budget review","priority":9,"updated_at":1711843200}"#
                        .to_owned(),
                    source_ref: Some("source:meeting-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Replace,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "meeting-2".to_owned(),
                    kind: "Meeting".to_owned(),
                    properties:
                        r#"{"title":"Backlog grooming","priority":2,"updated_at":1700000000}"#
                            .to_owned(),
                    source_ref: Some("source:meeting-2".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Replace,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "task-1".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"title":"Draft memo"}"#.to_owned(),
                    source_ref: Some("source:task-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "task-2".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"title":"Book follow-up"}"#.to_owned(),
                    source_ref: Some("source:task-2".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "decision-1".to_owned(),
                    kind: "Decision".to_owned(),
                    properties: r#"{"title":"Approve budget"}"#.to_owned(),
                    source_ref: Some("source:decision-1".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
            ],
            node_retires: vec![],
            edges: vec![
                EdgeInsert {
                    row_id: new_row_id(),
                    logical_id: "edge-meeting-task-1".to_owned(),
                    source_logical_id: "meeting-1".to_owned(),
                    target_logical_id: "task-1".to_owned(),
                    kind: "HAS_TASK".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("source:edge-1".to_owned()),
                    upsert: false,
                },
                EdgeInsert {
                    row_id: new_row_id(),
                    logical_id: "edge-task-task-2".to_owned(),
                    source_logical_id: "task-1".to_owned(),
                    target_logical_id: "task-2".to_owned(),
                    kind: "HAS_TASK".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("source:edge-2".to_owned()),
                    upsert: false,
                },
                EdgeInsert {
                    row_id: new_row_id(),
                    logical_id: "edge-meeting-decision-1".to_owned(),
                    source_logical_id: "meeting-1".to_owned(),
                    target_logical_id: "decision-1".to_owned(),
                    kind: "HAS_DECISION".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("source:edge-3".to_owned()),
                    upsert: false,
                },
            ],
            edge_retires: vec![],
            chunks: vec![
                ChunkInsert {
                    id: "chunk-meeting-1".to_owned(),
                    node_logical_id: "meeting-1".to_owned(),
                    text_content: "budget review agenda and action items".to_owned(),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                },
                ChunkInsert {
                    id: "chunk-meeting-2".to_owned(),
                    node_logical_id: "meeting-2".to_owned(),
                    text_content: "backlog grooming notes".to_owned(),
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
        .expect("seed write");
}

#[test]
fn grouped_query_returns_root_plus_named_expansion_slots_for_bounded_context() {
    let (_db, engine) = open_engine();
    seed_meeting_graph(&engine);

    let compiled = engine
        .query("Meeting")
        .filter_logical_id_eq("meeting-1")
        .expand("direct_tasks", TraverseDirection::Out, "HAS_TASK", 1, None)
        .expand(
            "task_descendants",
            TraverseDirection::Out,
            "HAS_TASK",
            2,
            None,
        )
        .expand("decisions", TraverseDirection::Out, "HAS_DECISION", 1, None)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.roots.len(), 1);
    assert_eq!(rows.roots[0].logical_id, "meeting-1");
    assert_eq!(rows.expansions.len(), 3);

    assert_eq!(rows.expansions[0].slot, "direct_tasks");
    assert_eq!(rows.expansions[0].roots.len(), 1);
    assert_eq!(rows.expansions[0].roots[0].root_logical_id, "meeting-1");
    assert_eq!(rows.expansions[0].roots[0].nodes.len(), 1);
    assert_eq!(rows.expansions[0].roots[0].nodes[0].logical_id, "task-1");

    assert_eq!(rows.expansions[1].slot, "task_descendants");
    assert_eq!(rows.expansions[1].roots[0].nodes.len(), 2);
    assert_eq!(
        rows.expansions[1].roots[0]
            .nodes
            .iter()
            .map(|node| node.logical_id.as_str())
            .collect::<Vec<_>>(),
        vec!["task-1", "task-2"]
    );

    assert_eq!(rows.expansions[2].slot, "decisions");
    assert_eq!(rows.expansions[2].roots[0].nodes.len(), 1);
    assert_eq!(
        rows.expansions[2].roots[0].nodes[0].logical_id,
        "decision-1"
    );
}

#[test]
fn grouped_query_supports_numeric_and_timestamp_filters_before_enrichment() {
    let (_db, engine) = open_engine();
    seed_meeting_graph(&engine);

    let compiled = engine
        .query("Meeting")
        .filter_json_integer_gte("$.priority", 5)
        .filter_json_timestamp_gte("$.updated_at", 1710000000)
        .expand("tasks", TraverseDirection::Out, "HAS_TASK", 1, None)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.roots.len(), 1);
    assert_eq!(rows.roots[0].logical_id, "meeting-1");
    assert_eq!(rows.expansions[0].slot, "tasks");
    assert_eq!(rows.expansions[0].roots[0].nodes.len(), 1);
    assert_eq!(rows.expansions[0].roots[0].nodes[0].logical_id, "task-1");
}

#[test]
fn grouped_text_search_enrichment_returns_requested_context_in_one_result() {
    let (_db, engine) = open_engine();
    seed_meeting_graph(&engine);

    let compiled = engine
        .query("Meeting")
        .text_search("budget", 5)
        .expand("tasks", TraverseDirection::Out, "HAS_TASK", 1, None)
        .expand("decisions", TraverseDirection::Out, "HAS_DECISION", 1, None)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.roots.len(), 1);
    assert_eq!(rows.roots[0].logical_id, "meeting-1");
    assert_eq!(rows.expansions.len(), 2);
    assert_eq!(rows.expansions[0].roots[0].nodes[0].logical_id, "task-1");
    assert_eq!(
        rows.expansions[1].roots[0].nodes[0].logical_id,
        "decision-1"
    );
}

#[test]
fn grouped_query_rejects_duplicate_expansion_slot_names() {
    let (_db, engine) = open_engine();

    let error = engine
        .query("Meeting")
        .expand("tasks", TraverseDirection::Out, "HAS_TASK", 1, None)
        .expand("tasks", TraverseDirection::Out, "HAS_DECISION", 1, None)
        .compile_grouped()
        .expect_err("duplicate slots must fail");

    assert!(matches!(error, CompileError::DuplicateExpansionSlot(_)));
}

#[test]
fn grouped_query_expansions_honor_the_query_hard_limit() {
    let (_db, engine) = open_engine();
    seed_meeting_graph(&engine);

    engine
        .writer()
        .submit(WriteRequest {
            label: "extend-grouped-query-graph".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "task-3".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"title":"Prepare deck"}"#.to_owned(),
                    source_ref: Some("source:task-3".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "task-4".to_owned(),
                    kind: "Task".to_owned(),
                    properties: r#"{"title":"Send follow-up"}"#.to_owned(),
                    source_ref: Some("source:task-4".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
            ],
            node_retires: vec![],
            edges: vec![
                EdgeInsert {
                    row_id: new_row_id(),
                    logical_id: "edge-task-2-task-3".to_owned(),
                    source_logical_id: "task-2".to_owned(),
                    target_logical_id: "task-3".to_owned(),
                    kind: "HAS_TASK".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("source:edge-4".to_owned()),
                    upsert: false,
                },
                EdgeInsert {
                    row_id: new_row_id(),
                    logical_id: "edge-task-3-task-4".to_owned(),
                    source_logical_id: "task-3".to_owned(),
                    target_logical_id: "task-4".to_owned(),
                    kind: "HAS_TASK".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: Some("source:edge-5".to_owned()),
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
        .expect("extend graph");

    let compiled = engine
        .query("Meeting")
        .filter_logical_id_eq("meeting-1")
        .expand("tasks", TraverseDirection::Out, "HAS_TASK", 4, None)
        .limit(2)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.roots.len(), 1);
    assert_eq!(rows.expansions.len(), 1);
    assert_eq!(rows.expansions[0].roots[0].nodes.len(), 2);
}

// --- Filter-pushdown regression tests ---
//
// These tests verify that JSON property filters, comparison filters, and
// source_ref filters correctly match nodes regardless of insertion order.
//
// Background: the base_candidates CTE previously applied a LIMIT before
// filter predicates ran in the outer WHERE clause, so only the first N
// nodes (by insertion order) had their properties evaluated. Filters are
// now pushed into the CTE so the LIMIT applies after filtering.

#[test]
fn json_text_filter_finds_non_first_node() {
    let (_db, engine) = open_engine();
    seed_meeting_graph(&engine);

    // meeting-2 has title="Backlog grooming", meeting-1 has title="Budget review".
    // With limit(1), the old bug would only evaluate meeting-1.
    let compiled = engine
        .query("Meeting")
        .filter_json_text_eq("$.title", "Backlog grooming")
        .limit(1)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.roots.len(), 1, "must find the non-first node");
    assert_eq!(rows.roots[0].logical_id, "meeting-2");
}

#[test]
fn json_integer_filter_finds_non_first_node() {
    let (_db, engine) = open_engine();
    seed_meeting_graph(&engine);

    // meeting-2 has priority=2, meeting-1 has priority=9.
    // Filter for priority <= 5 should find meeting-2 even with limit(1).
    let compiled = engine
        .query("Meeting")
        .filter_json_integer_lte("$.priority", 5)
        .limit(1)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(
        rows.roots.len(),
        1,
        "must find meeting-2 via integer filter"
    );
    assert_eq!(rows.roots[0].logical_id, "meeting-2");
}

#[test]
fn source_ref_filter_finds_non_first_node() {
    let (_db, engine) = open_engine();
    seed_meeting_graph(&engine);

    let compiled = engine
        .query("Meeting")
        .filter_source_ref_eq("source:meeting-2")
        .limit(1)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(
        rows.roots.len(),
        1,
        "must find meeting-2 via source_ref filter"
    );
    assert_eq!(rows.roots[0].logical_id, "meeting-2");
}

#[test]
fn combined_json_filters_find_correct_node() {
    let (_db, engine) = open_engine();
    seed_meeting_graph(&engine);

    // Both filters must match: priority >= 5 AND updated_at < 1711000000.
    // meeting-1: priority=9, updated_at=1711843200 → fails timestamp
    // meeting-2: priority=2, updated_at=1700000000 → fails priority
    // Neither matches, so result should be empty.
    let compiled = engine
        .query("Meeting")
        .filter_json_integer_gte("$.priority", 5)
        .filter_json_timestamp_lt("$.updated_at", 1711000000)
        .limit(10)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(
        rows.roots.len(),
        0,
        "no node satisfies both filters simultaneously"
    );
}

#[test]
fn json_filter_returns_all_matching_nodes() {
    let (_db, engine) = open_engine();
    seed_meeting_graph(&engine);

    // Both meetings have an updated_at field; filter for updated_at >= 0
    // should return both.
    let compiled = engine
        .query("Meeting")
        .filter_json_timestamp_gte("$.updated_at", 0)
        .limit(10)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.roots.len(), 2, "both meetings satisfy updated_at >= 0");
}

#[test]
fn json_filter_with_expansion_finds_non_first_node() {
    let (_db, engine) = open_engine();
    seed_meeting_graph(&engine);

    // Filter for meeting-2 by title, then expand tasks.
    // meeting-2 has no outgoing HAS_TASK edges, so expansion should be empty.
    let compiled = engine
        .query("Meeting")
        .filter_json_text_eq("$.title", "Backlog grooming")
        .expand("tasks", TraverseDirection::Out, "HAS_TASK", 1, None)
        .limit(1)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.roots.len(), 1);
    assert_eq!(rows.roots[0].logical_id, "meeting-2");
    assert_eq!(rows.expansions.len(), 1);
    assert_eq!(rows.expansions[0].slot, "tasks");
    assert!(
        rows.expansions[0].roots[0].nodes.is_empty(),
        "meeting-2 has no outgoing HAS_TASK edges"
    );
}

// --- Pack 3: target-side filter in grouped expand ---

#[test]
fn expand_with_json_path_eq_filter_returns_only_matching_nodes() {
    let (_db, engine) = open_engine();

    // Create 1 originator with 10 targets: 5 "decision" kind and 5 "action_item" kind.
    let mut nodes = vec![NodeInsert {
        row_id: new_row_id(),
        logical_id: "originator-1".to_owned(),
        kind: "Originator".to_owned(),
        properties: r#"{"title":"meeting"}"#.to_owned(),
        source_ref: None,
        upsert: false,
        chunk_policy: ChunkPolicy::Preserve,
        content_ref: None,
    }];
    let mut edges = vec![];

    for i in 0..5 {
        nodes.push(NodeInsert {
            row_id: new_row_id(),
            logical_id: format!("decision-item-{i}"),
            kind: "Item".to_owned(),
            properties: r#"{"kind":"decision"}"#.to_owned(),
            source_ref: None,
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
            content_ref: None,
        });
        edges.push(EdgeInsert {
            row_id: new_row_id(),
            logical_id: format!("edge-orig-decision-{i}"),
            source_logical_id: "originator-1".to_owned(),
            target_logical_id: format!("decision-item-{i}"),
            kind: "HAS_ITEM".to_owned(),
            properties: "{}".to_owned(),
            source_ref: None,
            upsert: false,
        });
    }
    for i in 0..5 {
        nodes.push(NodeInsert {
            row_id: new_row_id(),
            logical_id: format!("action-item-{i}"),
            kind: "Item".to_owned(),
            properties: r#"{"kind":"action_item"}"#.to_owned(),
            source_ref: None,
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
            content_ref: None,
        });
        edges.push(EdgeInsert {
            row_id: new_row_id(),
            logical_id: format!("edge-orig-action-{i}"),
            source_logical_id: "originator-1".to_owned(),
            target_logical_id: format!("action-item-{i}"),
            kind: "HAS_ITEM".to_owned(),
            properties: "{}".to_owned(),
            source_ref: None,
            upsert: false,
        });
    }

    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-filter-test".to_owned(),
            nodes,
            node_retires: vec![],
            edges,
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("seed write");

    let filter = Predicate::JsonPathEq {
        path: "$.kind".to_owned(),
        value: ScalarValue::Text("decision".to_owned()),
    };

    let compiled = engine
        .query("Originator")
        .filter_logical_id_eq("originator-1")
        .expand(
            "decisions",
            TraverseDirection::Out,
            "HAS_ITEM",
            1,
            Some(filter),
        )
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.roots.len(), 1);
    assert_eq!(rows.expansions.len(), 1);
    assert_eq!(rows.expansions[0].slot, "decisions");
    assert_eq!(rows.expansions[0].roots.len(), 1);
    let decision_nodes = &rows.expansions[0].roots[0].nodes;
    assert_eq!(
        decision_nodes.len(),
        5,
        "must return exactly 5 decision nodes"
    );
    for node in decision_nodes {
        let props: serde_json::Value = serde_json::from_str(&node.properties).expect("valid json");
        assert_eq!(
            props["kind"].as_str(),
            Some("decision"),
            "all returned nodes must match the filter"
        );
    }
}

#[test]
fn expand_filter_applies_before_per_originator_limit() {
    let (_db, engine) = open_engine();

    // 3 originators × 10 children each, 50% match rate (5 "yes", 5 "no").
    // limit=5, filter="matches"=="yes".
    // If filter runs BEFORE limit: each originator gets 5 results.
    // If filter runs AFTER limit (bug): each originator gets only 2-3.
    let mut nodes = vec![];
    let mut edges = vec![];

    for orig in 0..3 {
        nodes.push(NodeInsert {
            row_id: new_row_id(),
            logical_id: format!("orig-{orig}"),
            kind: "Originator".to_owned(),
            properties: format!(r#"{{"id":{orig}}}"#),
            source_ref: None,
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
            content_ref: None,
        });
        // Use "a-nomatch-" prefix for non-matching and "z-match-" prefix for matching
        // so that non-matching nodes sort BEFORE matching nodes alphabetically.
        // The SQL uses ORDER BY logical_id, so with limit=5 and filter-after-limit (bug):
        // the first 5 nodes selected are the "a-nomatch-*" nodes; after filtering for
        // matches="yes" zero results remain. With filter-before-limit (correct):
        // the "a-nomatch-*" nodes are eliminated first and the limit applies to the
        // 5 "z-match-*" nodes, returning 5 results.
        for child in 0..5 {
            nodes.push(NodeInsert {
                row_id: new_row_id(),
                logical_id: format!("a-nomatch-{orig}-{child}"),
                kind: "Child".to_owned(),
                properties: format!(r#"{{"matches":"no","orig":{orig},"child":{child}}}"#),
                source_ref: None,
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            });
            edges.push(EdgeInsert {
                row_id: new_row_id(),
                logical_id: format!("edge-nomatch-{orig}-{child}"),
                source_logical_id: format!("orig-{orig}"),
                target_logical_id: format!("a-nomatch-{orig}-{child}"),
                kind: "HAS_CHILD".to_owned(),
                properties: "{}".to_owned(),
                source_ref: None,
                upsert: false,
            });
        }
        for child in 0..5 {
            nodes.push(NodeInsert {
                row_id: new_row_id(),
                logical_id: format!("z-match-{orig}-{child}"),
                kind: "Child".to_owned(),
                properties: format!(r#"{{"matches":"yes","orig":{orig},"child":{child}}}"#),
                source_ref: None,
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            });
            edges.push(EdgeInsert {
                row_id: new_row_id(),
                logical_id: format!("edge-match-{orig}-{child}"),
                source_logical_id: format!("orig-{orig}"),
                target_logical_id: format!("z-match-{orig}-{child}"),
                kind: "HAS_CHILD".to_owned(),
                properties: "{}".to_owned(),
                source_ref: None,
                upsert: false,
            });
        }
    }

    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-limit-test".to_owned(),
            nodes,
            node_retires: vec![],
            edges,
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("seed write");

    let filter = Predicate::JsonPathEq {
        path: "$.matches".to_owned(),
        value: ScalarValue::Text("yes".to_owned()),
    };

    let compiled = engine
        .query("Originator")
        .expand(
            "children",
            TraverseDirection::Out,
            "HAS_CHILD",
            1,
            Some(filter),
        )
        .limit(5)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.roots.len(), 3, "3 originators");
    assert_eq!(rows.expansions.len(), 1);
    for root_group in &rows.expansions[0].roots {
        assert_eq!(
            root_group.nodes.len(),
            5,
            "each originator must get 5 matching results (filter-before-limit)"
        );
        for node in &root_group.nodes {
            let props: serde_json::Value =
                serde_json::from_str(&node.properties).expect("valid json");
            assert_eq!(
                props["matches"].as_str(),
                Some("yes"),
                "all returned nodes must match the filter"
            );
        }
    }
}

#[test]
fn expand_with_fused_filter_against_kind_without_schema_raises_error() {
    let (_db, engine) = open_engine();

    // Create a node kind "Unschemaed" that has NO property-FTS schema registered.
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-fused-filter-test".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "parent-fused-1".to_owned(),
                    kind: "Parent".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: "unschemaed-1".to_owned(),
                    kind: "Unschemaed".to_owned(),
                    properties: r#"{"tag":"value"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
            ],
            node_retires: vec![],
            edges: vec![EdgeInsert {
                row_id: new_row_id(),
                logical_id: "edge-parent-unschemaed".to_owned(),
                source_logical_id: "parent-fused-1".to_owned(),
                target_logical_id: "unschemaed-1".to_owned(),
                kind: "HAS_UNSCHEMAED".to_owned(),
                properties: "{}".to_owned(),
                source_ref: None,
                upsert: false,
            }],
            edge_retires: vec![],
            chunks: vec![],
            runs: vec![],
            steps: vec![],
            actions: vec![],
            optional_backfills: vec![],
            vec_inserts: vec![],
            operational_writes: vec![],
        })
        .expect("seed write");

    // "Unschemaed" has no property-FTS schema — using a fused filter must raise an error.
    let fused_filter = Predicate::JsonPathFusedEq {
        path: "$.tag".to_owned(),
        value: "value".to_owned(),
    };

    let compiled = engine
        .query("Parent")
        .expand(
            "items",
            TraverseDirection::Out,
            "HAS_UNSCHEMAED",
            1,
            Some(fused_filter),
        )
        .compile_grouped()
        .expect("grouped query compiles");

    // Execute-time validation: fused filter against a kind with no FTS schema must fail.
    // EXECUTE-TIME VALIDATION: target-kind set is discovered at execution time for expand
    // (distinct from main-path which is builder-time). See Pack 12 docs.
    let result = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled);

    let err =
        result.expect_err("fused filter against kind without FTS schema must fail at execute time");
    assert!(
        matches!(err, EngineError::InvalidConfig(_)),
        "expected InvalidConfig error for missing FTS schema, got {err:?}"
    );
}
