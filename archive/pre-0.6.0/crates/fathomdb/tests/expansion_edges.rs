#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::items_after_statements,
    clippy::needless_raw_string_hashes,
    clippy::panic,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unreadable_literal
)]

use fathomdb::{
    ChunkPolicy, ComparisonOp, EdgeInsert, Engine, EngineOptions, NodeInsert, Predicate,
    QueryBuilder, ScalarValue, TraverseDirection, WriteRequest, new_row_id,
};
use tempfile::NamedTempFile;

fn open_engine() -> (NamedTempFile, Engine) {
    let db = NamedTempFile::new().expect("temporary db");
    let engine = Engine::open(EngineOptions::new(db.path())).expect("engine opens");
    (db, engine)
}

fn node(logical_id: &str, kind: &str, properties: &str) -> NodeInsert {
    NodeInsert {
        row_id: new_row_id(),
        logical_id: logical_id.to_owned(),
        kind: kind.to_owned(),
        properties: properties.to_owned(),
        source_ref: Some(format!("source:{logical_id}")),
        upsert: false,
        chunk_policy: ChunkPolicy::Preserve,
        content_ref: None,
    }
}

fn edge(logical_id: &str, source: &str, target: &str, kind: &str, properties: &str) -> EdgeInsert {
    EdgeInsert {
        row_id: new_row_id(),
        logical_id: logical_id.to_owned(),
        source_logical_id: source.to_owned(),
        target_logical_id: target.to_owned(),
        kind: kind.to_owned(),
        properties: properties.to_owned(),
        source_ref: Some(format!("source:{logical_id}")),
        upsert: false,
    }
}

fn qb(kind: &str, logical_id: &str) -> QueryBuilder {
    QueryBuilder::nodes(kind).filter_logical_id_eq(logical_id)
}

fn submit(engine: &Engine, nodes: Vec<NodeInsert>, edges: Vec<EdgeInsert>) {
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed-edge-expansion".to_owned(),
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
}

#[test]
fn single_hop_out_emits_edge_row_fields() {
    let (_db, engine) = open_engine();
    submit(
        &engine,
        vec![
            node("src-1", "Meeting", r#"{"title":"A"}"#),
            node("dst-1", "Task", r#"{"title":"B"}"#),
        ],
        vec![edge(
            "edge-1",
            "src-1",
            "dst-1",
            "HAS_TASK",
            r#"{"rel":"owns"}"#,
        )],
    );

    let compiled = qb("Meeting", "src-1")
        .traverse_edges("tasks", TraverseDirection::Out, "HAS_TASK", 1)
        .done()
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.edge_expansions.len(), 1);
    assert_eq!(rows.edge_expansions[0].slot, "tasks");
    assert_eq!(rows.edge_expansions[0].roots.len(), 1);
    let root_rows = &rows.edge_expansions[0].roots[0];
    assert_eq!(root_rows.root_logical_id, "src-1");
    assert_eq!(root_rows.pairs.len(), 1);

    let (edge_row, node_row) = &root_rows.pairs[0];
    assert_eq!(edge_row.logical_id, "edge-1");
    assert_eq!(edge_row.source_logical_id, "src-1");
    assert_eq!(edge_row.target_logical_id, "dst-1");
    assert_eq!(edge_row.kind, "HAS_TASK");
    assert_eq!(edge_row.properties, r#"{"rel":"owns"}"#);
    assert_eq!(edge_row.source_ref.as_deref(), Some("source:edge-1"));
    assert_eq!(node_row.logical_id, "dst-1");
    assert_eq!(node_row.kind, "Task");
}

#[test]
fn single_hop_in_mirrors_out() {
    let (_db, engine) = open_engine();
    submit(
        &engine,
        vec![
            node("src-1", "Meeting", r#"{"title":"A"}"#),
            node("dst-1", "Task", r#"{"title":"B"}"#),
        ],
        vec![edge("edge-1", "src-1", "dst-1", "HAS_TASK", r#"{}"#)],
    );

    // Query from dst-1 going In on HAS_TASK; the endpoint should be src-1.
    let compiled = qb("Task", "dst-1")
        .traverse_edges("parents", TraverseDirection::In, "HAS_TASK", 1)
        .done()
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.edge_expansions.len(), 1);
    let root_rows = &rows.edge_expansions[0].roots[0];
    assert_eq!(root_rows.root_logical_id, "dst-1");
    assert_eq!(root_rows.pairs.len(), 1);
    let (edge_row, node_row) = &root_rows.pairs[0];
    assert_eq!(edge_row.logical_id, "edge-1");
    // Edge identity fields are absolute, not reoriented.
    assert_eq!(edge_row.source_logical_id, "src-1");
    assert_eq!(edge_row.target_logical_id, "dst-1");
    // Endpoint on In traversal is the source side.
    assert_eq!(node_row.logical_id, "src-1");
}

#[test]
fn multi_hop_max_depth_2_final_hop_semantics() {
    let (_db, engine) = open_engine();
    submit(
        &engine,
        vec![
            node("a", "Node", r#"{}"#),
            node("b", "Node", r#"{}"#),
            node("c", "Node", r#"{}"#),
        ],
        vec![
            edge("e-ab", "a", "b", "LINK", r#"{"hop":1}"#),
            edge("e-bc", "b", "c", "LINK", r#"{"hop":2}"#),
        ],
    );

    let compiled = qb("Node", "a")
        .traverse_edges("chain", TraverseDirection::Out, "LINK", 2)
        .done()
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    let pairs = &rows.edge_expansions[0].roots[0].pairs;
    assert_eq!(pairs.len(), 2);

    // Find pair whose endpoint is "b" -- edge should be e-ab (final hop leading to b).
    let pair_b = pairs
        .iter()
        .find(|(_, n)| n.logical_id == "b")
        .expect("pair with endpoint b");
    assert_eq!(pair_b.0.logical_id, "e-ab");

    // Pair with endpoint c -- final-hop edge is e-bc (NOT full-path enumeration).
    let pair_c = pairs
        .iter()
        .find(|(_, n)| n.logical_id == "c")
        .expect("pair with endpoint c");
    assert_eq!(pair_c.0.logical_id, "e-bc");
}

#[test]
fn edge_filter_narrows_pairs() {
    let (_db, engine) = open_engine();
    submit(
        &engine,
        vec![
            node("src", "Root", r#"{}"#),
            node("t1", "Target", r#"{}"#),
            node("t2", "Target", r#"{}"#),
        ],
        vec![
            edge("e1", "src", "t1", "LINK", r#"{"tag":"keep"}"#),
            edge("e2", "src", "t2", "LINK", r#"{"tag":"drop"}"#),
        ],
    );

    let compiled = qb("Root", "src")
        .traverse_edges("links", TraverseDirection::Out, "LINK", 1)
        .edge_filter(Predicate::EdgePropertyEq {
            path: "$.tag".to_owned(),
            value: ScalarValue::Text("keep".to_owned()),
        })
        .done()
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    let pairs = &rows.edge_expansions[0].roots[0].pairs;
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].0.logical_id, "e1");
    assert_eq!(pairs[0].1.logical_id, "t1");
}

#[test]
fn endpoint_filter_narrows_pairs() {
    let (_db, engine) = open_engine();
    submit(
        &engine,
        vec![
            node("src", "Root", r#"{}"#),
            node("t1", "Target", r#"{"status":"open"}"#),
            node("t2", "Target", r#"{"status":"closed"}"#),
        ],
        vec![
            edge("e1", "src", "t1", "LINK", r#"{}"#),
            edge("e2", "src", "t2", "LINK", r#"{}"#),
        ],
    );

    let compiled = qb("Root", "src")
        .traverse_edges("links", TraverseDirection::Out, "LINK", 1)
        .endpoint_filter(Predicate::JsonPathEq {
            path: "$.status".to_owned(),
            value: ScalarValue::Text("open".to_owned()),
        })
        .done()
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    let pairs = &rows.edge_expansions[0].roots[0].pairs;
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].1.logical_id, "t1");
}

#[test]
fn mixed_node_and_edge_expansions_in_one_query() {
    let (_db, engine) = open_engine();
    submit(
        &engine,
        vec![node("m", "Meeting", r#"{}"#), node("t", "Task", r#"{}"#)],
        vec![edge("e", "m", "t", "HAS_TASK", r#"{}"#)],
    );

    let compiled = qb("Meeting", "m")
        .expand(
            "tasks_as_nodes",
            TraverseDirection::Out,
            "HAS_TASK",
            1,
            None,
            None,
        )
        .traverse_edges("tasks_as_edges", TraverseDirection::Out, "HAS_TASK", 1)
        .done()
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.expansions.len(), 1);
    assert_eq!(rows.expansions[0].slot, "tasks_as_nodes");
    assert_eq!(rows.expansions[0].roots[0].nodes.len(), 1);
    assert_eq!(rows.expansions[0].roots[0].nodes[0].logical_id, "t");

    assert_eq!(rows.edge_expansions.len(), 1);
    assert_eq!(rows.edge_expansions[0].slot, "tasks_as_edges");
    assert_eq!(rows.edge_expansions[0].roots[0].pairs.len(), 1);
    let (edge_row, node_row) = &rows.edge_expansions[0].roots[0].pairs[0];
    assert_eq!(edge_row.logical_id, "e");
    assert_eq!(node_row.logical_id, "t");
}

#[test]
fn empty_roots_yields_empty_edge_expansions() {
    let (_db, engine) = open_engine();
    submit(&engine, vec![node("src", "Root", r#"{}"#)], vec![]);

    // Filter matches zero nodes.
    let compiled = qb("Root", "does-not-exist")
        .traverse_edges("links", TraverseDirection::Out, "LINK", 1)
        .done()
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert!(rows.roots.is_empty());
    assert_eq!(rows.edge_expansions.len(), 1);
    assert_eq!(rows.edge_expansions[0].slot, "links");
    assert!(rows.edge_expansions[0].roots.is_empty());
}

#[test]
fn edge_filter_pathological_value_does_not_execute_as_sql() {
    let (_db, engine) = open_engine();
    submit(
        &engine,
        vec![node("src", "Root", r#"{}"#), node("t1", "Target", r#"{}"#)],
        vec![edge("e1", "src", "t1", "LINK", r#"{"tag":"safe"}"#)],
    );

    let pathological = "'; DROP TABLE edges; --";

    // Edge filter value is pathological; it must be bound, not interpolated,
    // so the query simply returns zero matches (no SQL injection).
    let compiled = qb("Root", "src")
        .traverse_edges("links", TraverseDirection::Out, "LINK", 1)
        .edge_filter(Predicate::EdgePropertyEq {
            path: "$.tag".to_owned(),
            value: ScalarValue::Text(pathological.to_owned()),
        })
        .done()
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    assert_eq!(rows.edge_expansions[0].roots[0].pairs.len(), 0);

    // Verify the edges table still exists / still has the original edge via
    // a second query without filter.
    let compiled2 = qb("Root", "src")
        .traverse_edges("links", TraverseDirection::Out, "LINK", 1)
        .done()
        .compile_grouped()
        .expect("grouped query compiles");
    let rows2 = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled2)
        .expect("grouped query executes");
    assert_eq!(rows2.edge_expansions[0].roots[0].pairs.len(), 1);
}

#[test]
fn comparison_edge_property_filter() {
    let (_db, engine) = open_engine();
    submit(
        &engine,
        vec![
            node("src", "Root", r#"{}"#),
            node("t1", "Target", r#"{}"#),
            node("t2", "Target", r#"{}"#),
        ],
        vec![
            edge("e1", "src", "t1", "LINK", r#"{"weight":5}"#),
            edge("e2", "src", "t2", "LINK", r#"{"weight":50}"#),
        ],
    );

    let compiled = qb("Root", "src")
        .traverse_edges("links", TraverseDirection::Out, "LINK", 1)
        .edge_filter(Predicate::EdgePropertyCompare {
            path: "$.weight".to_owned(),
            op: ComparisonOp::Gte,
            value: ScalarValue::Integer(10),
        })
        .done()
        .compile_grouped()
        .expect("grouped query compiles");

    let rows = engine
        .coordinator()
        .execute_compiled_grouped_read(&compiled)
        .expect("grouped query executes");

    let pairs = &rows.edge_expansions[0].roots[0].pairs;
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].0.logical_id, "e2");
}
