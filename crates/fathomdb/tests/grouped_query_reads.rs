#![allow(
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::unreadable_literal
)]

use fathomdb::{
    ChunkInsert, ChunkPolicy, CompileError, EdgeInsert, Engine, EngineError, EngineOptions,
    GroupedQueryRows, NodeInsert, Predicate, ScalarValue, TraverseDirection, WriteRequest,
    new_row_id,
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

#[test]
fn search_builder_expand_execute_grouped_returns_root_plus_expansion() {
    let (_db, engine) = open_engine();
    seed_meeting_graph(&engine);

    let rows: GroupedQueryRows = engine
        .query("Meeting")
        .search("budget", 5)
        .expand("tasks", TraverseDirection::Out, "HAS_TASK", 1, None)
        .expand("decisions", TraverseDirection::Out, "HAS_DECISION", 1, None)
        .execute_grouped()
        .expect("search grouped executes");

    assert_eq!(rows.roots.len(), 1);
    assert_eq!(rows.roots[0].logical_id, "meeting-1");
    assert_eq!(rows.expansions.len(), 2);
    assert_eq!(rows.expansions[0].slot, "tasks");
    assert_eq!(rows.expansions[0].roots[0].nodes.len(), 1);
    assert_eq!(rows.expansions[0].roots[0].nodes[0].logical_id, "task-1");
    assert_eq!(rows.expansions[1].slot, "decisions");
    assert_eq!(rows.expansions[1].roots[0].nodes.len(), 1);
    assert_eq!(
        rows.expansions[1].roots[0].nodes[0].logical_id,
        "decision-1"
    );
}

#[test]
fn node_query_builder_execute_grouped_convenience_terminal() {
    let (_db, engine) = open_engine();
    seed_meeting_graph(&engine);

    let rows: GroupedQueryRows = engine
        .query("Meeting")
        .filter_logical_id_eq("meeting-1")
        .expand("tasks", TraverseDirection::Out, "HAS_TASK", 1, None)
        .execute_grouped()
        .expect("execute_grouped executes");

    assert_eq!(rows.roots.len(), 1);
    assert_eq!(rows.roots[0].logical_id, "meeting-1");
    assert_eq!(rows.expansions.len(), 1);
    assert_eq!(rows.expansions[0].slot, "tasks");
    assert_eq!(rows.expansions[0].roots[0].nodes.len(), 1);
    assert_eq!(rows.expansions[0].roots[0].nodes[0].logical_id, "task-1");
}

// ---------------------------------------------------------------------------
// Shape 4: self-expand with cycle (A→B→C→A)
// ---------------------------------------------------------------------------
//
// Three nodes of the same kind with edges forming a cycle: A→B, B→C, C→A.
// Tests probe what the engine does at depth=1, depth=2, and depth=3 on such
// a cycle, and document the observed behavior verbatim for use in
// docs/reference/query.md.

fn seed_cycle_graph(engine: &Engine) -> (String, String, String) {
    let id_a = "cycle-node-a".to_owned();
    let id_b = "cycle-node-b".to_owned();
    let id_c = "cycle-node-c".to_owned();

    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-cycle-graph".to_owned(),
            nodes: vec![
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: id_a.clone(),
                    kind: "CycleNode".to_owned(),
                    properties: r#"{"label":"A"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: id_b.clone(),
                    kind: "CycleNode".to_owned(),
                    properties: r#"{"label":"B"}"#.to_owned(),
                    source_ref: None,
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                },
                NodeInsert {
                    row_id: new_row_id(),
                    logical_id: id_c.clone(),
                    kind: "CycleNode".to_owned(),
                    properties: r#"{"label":"C"}"#.to_owned(),
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
                    logical_id: "edge-a-b".to_owned(),
                    source_logical_id: id_a.clone(),
                    target_logical_id: id_b.clone(),
                    kind: "HAS_NEXT".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                },
                EdgeInsert {
                    row_id: new_row_id(),
                    logical_id: "edge-b-c".to_owned(),
                    source_logical_id: id_b.clone(),
                    target_logical_id: id_c.clone(),
                    kind: "HAS_NEXT".to_owned(),
                    properties: "{}".to_owned(),
                    source_ref: None,
                    upsert: false,
                },
                EdgeInsert {
                    row_id: new_row_id(),
                    logical_id: "edge-c-a".to_owned(),
                    source_logical_id: id_c.clone(),
                    target_logical_id: id_a.clone(),
                    kind: "HAS_NEXT".to_owned(),
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
        .expect("seed cycle graph");

    (id_a, id_b, id_c)
}

#[test]
fn expand_self_expand_at_depth_1() {
    // Shape 4, Test 1: max_depth=1 on a cyclic graph is cycle-irrelevant.
    // A→B→C→A: querying from A at depth=1 returns exactly 1 result (B).
    // One hop cannot encounter a cycle.
    let (_db, engine) = open_engine();
    let (id_a, id_b, _id_c) = seed_cycle_graph(&engine);

    let compiled = engine
        .query("CycleNode")
        .filter_logical_id_eq(&id_a)
        .expand("loop", TraverseDirection::Out, "HAS_NEXT", 1, None)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.roots.len(), 1);
    assert_eq!(rows.roots[0].logical_id, id_a);
    assert_eq!(rows.expansions.len(), 1);
    assert_eq!(rows.expansions[0].slot, "loop");
    assert_eq!(rows.expansions[0].roots.len(), 1);
    assert_eq!(rows.expansions[0].roots[0].root_logical_id, id_a);

    // depth=1: exactly 1 result (B), no cycle possible in one hop.
    let nodes = &rows.expansions[0].roots[0].nodes;
    assert_eq!(nodes.len(), 1, "depth=1 from A returns exactly 1 hop (B)");
    assert_eq!(nodes[0].logical_id, id_b, "the single hop result is B");
}

#[test]
fn expand_self_expand_depth_2_termination() {
    // Shape 4, Tests 2 and 3: depth>1 on a cyclic graph A→B→C→A.
    //
    // OBSERVED BEHAVIOR (depth=2, originator=A):
    // The recursive CTE uses a visited-string accumulator
    // (`printf(',%s,', logical_id)` concatenated per hop) and the WHERE
    // clause `instr(t.visited, printf(',%s,', next_id)) = 0` prevents
    // revisiting any node already seen on the current path. The root node A
    // is seeded into the visited string at depth=0, so the path C→A at
    // depth=3 is blocked even when max_depth would allow it. With depth=2
    // from A: hop 1 reaches B, hop 2 reaches C. Attempting C→A is blocked
    // because A is already visited. Result: depth=2 returns exactly 2 nodes
    // (B and C).
    //
    // OBSERVED BEHAVIOR (depth=3, limit=10, originator=A):
    // With depth=3 from A: hop 1 = B, hop 2 = C, hop 3 attempt = A (blocked
    // by visited). Result: depth=3 also returns exactly 2 nodes (B and C) —
    // same as depth=2 — because the cycle back to A is blocked at every
    // depth. The test terminates immediately and does not hang or OOM.
    //
    // Summary: fathomdb's recursive CTE has per-path visited-node
    // deduplication via string accumulation. Cycles in the edge graph are
    // safe at any max_depth value: the root node is always pre-seeded as
    // visited, so no walk can loop back to the originator. The O(depth)
    // worst-case bound holds strictly; the hard limit provides an additional
    // safety cap for very long paths in non-cyclic graphs.

    let (_db, engine) = open_engine();
    let (id_a, id_b, id_c) = seed_cycle_graph(&engine);

    // --- depth=2 ---
    let compiled_d2 = engine
        .query("CycleNode")
        .filter_logical_id_eq(&id_a)
        .expand("loop", TraverseDirection::Out, "HAS_NEXT", 2, None)
        .compile_grouped()
        .expect("depth=2 compiles");

    let rows_d2 = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled_d2)
        .expect("depth=2 executes");

    let nodes_d2 = &rows_d2.expansions[0].roots[0].nodes;
    // depth=2 returns exactly B and C. A is blocked (already visited as root).
    let ids_d2: Vec<&str> = nodes_d2.iter().map(|n| n.logical_id.as_str()).collect();
    assert_eq!(
        nodes_d2.len(),
        2,
        "depth=2 from A returns exactly 2 nodes (B, C); got: {:?}",
        ids_d2
    );
    assert!(
        ids_d2.contains(&id_b.as_str()),
        "depth=2 result must contain B; got: {:?}",
        ids_d2
    );
    assert!(
        ids_d2.contains(&id_c.as_str()),
        "depth=2 result must contain C; got: {:?}",
        ids_d2
    );

    // --- depth=3, limit=10: must terminate and not OOM ---
    let compiled_d3 = engine
        .query("CycleNode")
        .filter_logical_id_eq(&id_a)
        .expand("loop", TraverseDirection::Out, "HAS_NEXT", 3, None)
        .limit(10)
        .compile_grouped()
        .expect("depth=3 compiles");

    let rows_d3 = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled_d3)
        .expect("depth=3 executes without hang or OOM");

    let nodes_d3 = &rows_d3.expansions[0].roots[0].nodes;
    let ids_d3: Vec<&str> = nodes_d3.iter().map(|n| n.logical_id.as_str()).collect();

    // depth=3 with limit=10 terminates and returns exactly 2 nodes (B and C).
    // The cycle back to A is blocked by visited-node tracking; the result is
    // identical to depth=2 because no new reachable nodes exist after C.
    assert_eq!(
        nodes_d3.len(),
        2,
        "depth=3 with limit=10 returns exactly 2 nodes (B, C); got: {:?}",
        ids_d3
    );
    assert!(
        ids_d3.contains(&id_b.as_str()),
        "depth=3 result must contain B; got: {:?}",
        ids_d3
    );
    assert!(
        ids_d3.contains(&id_c.as_str()),
        "depth=3 result must contain C; got: {:?}",
        ids_d3
    );
}

// ---------------------------------------------------------------------------
// Shape 1: per-originator limit under skewed fan-out
// ---------------------------------------------------------------------------
//
// 50 originators with heavily skewed child counts:
//   - Originator 0: 500 children
//   - Originators 1-10: 20 children each
//   - Originators 11-49: 2 children each
//
// expand with limit=50. The `final_limit` in the current API applies to
// both the root query (returning all 50 originators, since 50 >= N_ORIG)
// and to per-originator expansion (via ROW_NUMBER OVER PARTITION BY root_id
// which caps each root at LIMIT independently).
//
// Key property: per-originator limit is enforced independently per root.
// A heavily-fan-out originator must not starve low-fan-out ones.
//
// Expected total: min(500,50) + 10*min(20,50) + 39*min(2,50)
//               = 50 + 200 + 78 = 328
// NOT 50 (which would indicate only a global cap of `limit` applied across
// all roots combined, rather than independent per-originator budgets).

#[test]
fn expand_per_originator_limit_under_skewed_fanout() {
    let (_db, engine) = open_engine();

    // LIMIT must be >= N_ORIG so the root query returns all 50 originators,
    // while still capping the per-originator expansion on the big originator.
    const LIMIT: usize = 50;
    const BIG_FANOUT: usize = 500;
    const MED_FANOUT: usize = 20;
    const SMALL_FANOUT: usize = 2;
    const N_MED: usize = 10; // originators 1-10
    const N_SMALL: usize = 39; // originators 11-49
    const N_ORIG: usize = 1 + N_MED + N_SMALL; // 50

    // Build node and edge inserts in one batch.
    let mut nodes: Vec<NodeInsert> = Vec::new();
    let mut edges: Vec<EdgeInsert> = Vec::new();

    // Insert originator nodes.
    for i in 0..N_ORIG {
        nodes.push(NodeInsert {
            row_id: new_row_id(),
            logical_id: format!("skew-orig-{i:03}"),
            kind: "SkewOrig".to_owned(),
            properties: "{}".to_owned(),
            source_ref: None,
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
            content_ref: None,
        });
    }

    // Insert child nodes and edges for each originator.
    let child_count = |i: usize| -> usize {
        if i == 0 {
            BIG_FANOUT
        } else if i <= N_MED {
            MED_FANOUT
        } else {
            SMALL_FANOUT
        }
    };

    for i in 0..N_ORIG {
        let n = child_count(i);
        for j in 0..n {
            let child_lid = format!("skew-child-{i:03}-{j:04}");
            nodes.push(NodeInsert {
                row_id: new_row_id(),
                logical_id: child_lid.clone(),
                kind: "SkewChild".to_owned(),
                properties: format!(r#"{{"orig":{i},"seq":{j}}}"#),
                source_ref: None,
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            });
            edges.push(EdgeInsert {
                row_id: new_row_id(),
                logical_id: format!("skew-edge-{i:03}-{j:04}"),
                source_logical_id: format!("skew-orig-{i:03}"),
                target_logical_id: child_lid,
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
            label: "seed-skew-graph".to_owned(),
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
        .expect("seed skew graph");

    let compiled = engine
        .query("SkewOrig")
        .expand("children", TraverseDirection::Out, "HAS_CHILD", 1, None)
        .limit(LIMIT)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.roots.len(), N_ORIG, "all 50 originators are returned");
    assert_eq!(rows.expansions.len(), 1);
    assert_eq!(rows.expansions[0].slot, "children");

    let expansion = &rows.expansions[0];

    // Build a map from originator logical_id → result count for assertions.
    let orig_result_count: std::collections::HashMap<&str, usize> = expansion
        .roots
        .iter()
        .map(|r| (r.root_logical_id.as_str(), r.nodes.len()))
        .collect();

    // Originator 0: capped at LIMIT=50 (has 500 children).
    let big_count = *orig_result_count
        .get("skew-orig-000")
        .expect("skew-orig-000 present");
    assert_eq!(
        big_count, LIMIT,
        "originator 0 (500 children) capped at per-originator limit={LIMIT}"
    );

    // Originators 1-10: have exactly 20 children each (below LIMIT=50), all returned.
    for i in 1..=N_MED {
        let lid = format!("skew-orig-{i:03}");
        let count = *orig_result_count
            .get(lid.as_str())
            .unwrap_or_else(|| panic!("{lid} present"));
        assert_eq!(
            count, MED_FANOUT,
            "{lid} (20 children) returns all 20 (< limit={LIMIT})"
        );
    }

    // Originators 11-49: have 2 children each (well below limit), all returned.
    for i in (N_MED + 1)..N_ORIG {
        let lid = format!("skew-orig-{i:03}");
        let count = *orig_result_count
            .get(lid.as_str())
            .unwrap_or_else(|| panic!("{lid} present"));
        assert_eq!(
            count, SMALL_FANOUT,
            "{lid} (2 children) returns all 2 (< limit={LIMIT})"
        );
    }

    // Total result count: 50 + 10*20 + 39*2 = 328. NOT 50 (which would
    // indicate only a global cap of LIMIT=50 applied across all roots
    // combined). The per-originator ROW_NUMBER partition ensures that the
    // full budget is available to each root independently.
    let total: usize = expansion.roots.iter().map(|r| r.nodes.len()).sum();
    let expected_total = LIMIT + N_MED * MED_FANOUT + N_SMALL * SMALL_FANOUT;
    assert_eq!(
        total, expected_total,
        "total={total} must equal per-originator sum={expected_total}"
    );

    // Assert no cross-leak: each originator's children belong only to it.
    for root in &expansion.roots {
        let orig_index: usize = root
            .root_logical_id
            .trim_start_matches("skew-orig-")
            .parse()
            .expect("numeric suffix");
        for node in &root.nodes {
            let props: serde_json::Value =
                serde_json::from_str(&node.properties).expect("valid json");
            let node_orig = props["orig"].as_u64().expect("orig field present") as usize;
            assert_eq!(
                node_orig, orig_index,
                "child {} belongs to orig={node_orig} but is in {}'s slot",
                node.logical_id, root.root_logical_id
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Shape 2a: per-slot result order is undefined
// ---------------------------------------------------------------------------
//
// 1 originator with 10 children inserted in reverse lexicographic order.
// Assert all 10 are returned. Do NOT assert order.
// Per docs/reference/query.md: the order of nodes within an expansion slot
// is undefined. Callers must sort client-side if order matters.

#[test]
fn expand_per_slot_order_is_unordered() {
    let (_db, engine) = open_engine();

    const N: usize = 10;

    let mut nodes: Vec<NodeInsert> = Vec::new();
    let mut edges: Vec<EdgeInsert> = Vec::new();

    nodes.push(NodeInsert {
        row_id: new_row_id(),
        logical_id: "order-orig".to_owned(),
        kind: "OrderOrig".to_owned(),
        properties: "{}".to_owned(),
        source_ref: None,
        upsert: false,
        chunk_policy: ChunkPolicy::Preserve,
        content_ref: None,
    });

    // Insert children in reverse logical-id order (z → a suffix).
    for i in (0..N).rev() {
        let child_lid = format!("order-child-{i:02}");
        nodes.push(NodeInsert {
            row_id: new_row_id(),
            logical_id: child_lid.clone(),
            kind: "OrderChild".to_owned(),
            properties: format!(r#"{{"sequence_index":{i}}}"#),
            source_ref: None,
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
            content_ref: None,
        });
        edges.push(EdgeInsert {
            row_id: new_row_id(),
            logical_id: format!("order-edge-{i:02}"),
            source_logical_id: "order-orig".to_owned(),
            target_logical_id: child_lid,
            kind: "HAS_CHILD".to_owned(),
            properties: "{}".to_owned(),
            source_ref: None,
            upsert: false,
        });
    }

    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-order-graph".to_owned(),
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
        .expect("seed order graph");

    let compiled = engine
        .query("OrderOrig")
        .filter_logical_id_eq("order-orig")
        .expand("children", TraverseDirection::Out, "HAS_CHILD", 1, None)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.roots.len(), 1);
    let nodes = &rows.expansions[0].roots[0].nodes;

    // All 10 children must be returned. Order is intentionally not asserted.
    // Per docs/reference/query.md: per-slot order is undefined; sort client-side.
    assert_eq!(nodes.len(), N, "all {N} children returned");
    let mut ids: Vec<&str> = nodes.iter().map(|n| n.logical_id.as_str()).collect();
    ids.sort_unstable();
    let expected: Vec<String> = (0..N).map(|i| format!("order-child-{i:02}")).collect();
    assert_eq!(
        ids,
        expected.iter().map(String::as_str).collect::<Vec<_>>(),
        "all 10 children present (order not checked)"
    );
}

// ---------------------------------------------------------------------------
// Shape 2b: sort client-side by property
// ---------------------------------------------------------------------------
//
// Same graph as Shape 2a. Sort the returned children client-side by
// $.sequence_index and assert ascending order.
// This documents the idiomatic pattern for callers that need deterministic
// order: fetch from fathomdb, then sort on the application side.

#[test]
fn expand_sort_by_property_client_side() {
    let (_db, engine) = open_engine();

    const N: usize = 10;

    let mut nodes: Vec<NodeInsert> = Vec::new();
    let mut edges: Vec<EdgeInsert> = Vec::new();

    nodes.push(NodeInsert {
        row_id: new_row_id(),
        logical_id: "sortprop-orig".to_owned(),
        kind: "SortPropOrig".to_owned(),
        properties: "{}".to_owned(),
        source_ref: None,
        upsert: false,
        chunk_policy: ChunkPolicy::Preserve,
        content_ref: None,
    });

    for i in (0..N).rev() {
        let child_lid = format!("sortprop-child-{i:02}");
        nodes.push(NodeInsert {
            row_id: new_row_id(),
            logical_id: child_lid.clone(),
            kind: "SortPropChild".to_owned(),
            properties: format!(r#"{{"sequence_index":{i}}}"#),
            source_ref: None,
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
            content_ref: None,
        });
        edges.push(EdgeInsert {
            row_id: new_row_id(),
            logical_id: format!("sortprop-edge-{i:02}"),
            source_logical_id: "sortprop-orig".to_owned(),
            target_logical_id: child_lid,
            kind: "HAS_CHILD".to_owned(),
            properties: "{}".to_owned(),
            source_ref: None,
            upsert: false,
        });
    }

    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-sortprop-graph".to_owned(),
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
        .expect("seed sortprop graph");

    let compiled = engine
        .query("SortPropOrig")
        .filter_logical_id_eq("sortprop-orig")
        .expand("children", TraverseDirection::Out, "HAS_CHILD", 1, None)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.roots.len(), 1);
    let mut nodes = rows.expansions[0].roots[0].nodes.clone();

    // Sort client-side by $.sequence_index ascending.
    nodes.sort_by_key(|n| {
        let props: serde_json::Value = serde_json::from_str(&n.properties).expect("valid json");
        props["sequence_index"]
            .as_i64()
            .expect("sequence_index present")
    });

    // Assert sorted order is 0..N ascending.
    for (expected_seq, node) in nodes.iter().enumerate() {
        let props: serde_json::Value = serde_json::from_str(&node.properties).expect("valid json");
        let actual_seq = props["sequence_index"]
            .as_i64()
            .expect("sequence_index present") as usize;
        assert_eq!(
            actual_seq, expected_seq,
            "after client-side sort, position {expected_seq} must have sequence_index={expected_seq}"
        );
    }
}

// ---------------------------------------------------------------------------
// Shape 3: small originator set, large expansion
// ---------------------------------------------------------------------------
//
// 2 originators × 200 children each, limit=50.
// Per-originator budget must not degenerate at small N.
// Each originator gets exactly 50 results from its own child pool.

#[test]
fn expand_small_originator_set_large_expansion() {
    let (_db, engine) = open_engine();

    const LIMIT: usize = 50;
    const N_ORIG: usize = 2;
    const CHILDREN_PER_ORIG: usize = 200;

    let mut nodes: Vec<NodeInsert> = Vec::new();
    let mut edges: Vec<EdgeInsert> = Vec::new();

    for i in 0..N_ORIG {
        nodes.push(NodeInsert {
            row_id: new_row_id(),
            logical_id: format!("small-orig-{i}"),
            kind: "SmallOrig".to_owned(),
            properties: "{}".to_owned(),
            source_ref: None,
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
            content_ref: None,
        });
        for j in 0..CHILDREN_PER_ORIG {
            let child_lid = format!("small-child-{i}-{j:03}");
            nodes.push(NodeInsert {
                row_id: new_row_id(),
                logical_id: child_lid.clone(),
                kind: "SmallChild".to_owned(),
                properties: format!(r#"{{"orig":{i},"seq":{j}}}"#),
                source_ref: None,
                upsert: false,
                chunk_policy: ChunkPolicy::Preserve,
                content_ref: None,
            });
            edges.push(EdgeInsert {
                row_id: new_row_id(),
                logical_id: format!("small-edge-{i}-{j:03}"),
                source_logical_id: format!("small-orig-{i}"),
                target_logical_id: child_lid,
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
            label: "seed-small-orig-graph".to_owned(),
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
        .expect("seed small-orig graph");

    let compiled = engine
        .query("SmallOrig")
        .expand("children", TraverseDirection::Out, "HAS_CHILD", 1, None)
        .limit(LIMIT)
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.roots.len(), N_ORIG, "both originators returned");
    assert_eq!(rows.expansions.len(), 1);
    assert_eq!(rows.expansions[0].slot, "children");

    let expansion = &rows.expansions[0];

    for root in &expansion.roots {
        // Each originator gets exactly LIMIT=50 results.
        assert_eq!(
            root.nodes.len(),
            LIMIT,
            "{} must have exactly {LIMIT} expansion results",
            root.root_logical_id
        );

        // Determine which originator index this is.
        let orig_index: usize = root
            .root_logical_id
            .trim_start_matches("small-orig-")
            .parse()
            .expect("numeric suffix");

        // Assert no cross-leak: every returned child belongs to this originator.
        for node in &root.nodes {
            let props: serde_json::Value =
                serde_json::from_str(&node.properties).expect("valid json");
            let node_orig = props["orig"].as_u64().expect("orig field") as usize;
            assert_eq!(
                node_orig, orig_index,
                "child {} belongs to orig={node_orig} but appears in {}'s slot",
                node.logical_id, root.root_logical_id
            );
        }
    }
}

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
