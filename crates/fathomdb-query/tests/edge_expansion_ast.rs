//! Pack B: AST + builder scaffold for edge-projecting traversal.
//!
//! Asserts that `QueryBuilder::traverse_edges(...).done()` populates
//! `QueryAst.edge_expansions` with the expected `EdgeExpansionSlot`
//! shape, that edge/endpoint filters flow through, and that slot-name
//! uniqueness is checked across both `expansions` and
//! `edge_expansions` vecs at grouped-compile time.

use fathomdb_query::{
    CompileError, EdgeExpansionSlot, Predicate, QueryBuilder, ScalarValue, TraverseDirection,
};

#[test]
fn traverse_edges_appends_edge_expansion_slot_to_ast() {
    let query = QueryBuilder::nodes("Meeting")
        .traverse_edges("provenance", TraverseDirection::Out, "assigned_to", 2)
        .done();

    let ast = query.ast();
    assert_eq!(ast.expansions.len(), 0);
    assert_eq!(ast.edge_expansions.len(), 1);

    let slot = &ast.edge_expansions[0];
    assert_eq!(slot.slot, "provenance");
    assert_eq!(slot.direction, TraverseDirection::Out);
    assert_eq!(slot.label, "assigned_to");
    assert_eq!(slot.max_depth, 2);
    assert!(slot.edge_filter.is_none());
    assert!(slot.endpoint_filter.is_none());
}

#[test]
fn traverse_edges_carries_edge_and_endpoint_filters_into_ast() {
    let edge_pred = Predicate::EdgePropertyEq {
        path: "$.rel".to_string(),
        value: ScalarValue::Text("primary".to_string()),
    };
    let endpoint_pred = Predicate::KindEq("Person".to_string());

    let query = QueryBuilder::nodes("Meeting")
        .traverse_edges("provenance", TraverseDirection::In, "assigned_to", 1)
        .edge_filter(edge_pred.clone())
        .endpoint_filter(endpoint_pred.clone())
        .done();

    let ast = query.ast();
    assert_eq!(ast.edge_expansions.len(), 1);
    let expected = EdgeExpansionSlot {
        slot: "provenance".to_string(),
        direction: TraverseDirection::In,
        label: "assigned_to".to_string(),
        max_depth: 1,
        endpoint_filter: Some(endpoint_pred),
        edge_filter: Some(edge_pred),
    };
    assert_eq!(ast.edge_expansions[0], expected);
}

#[test]
fn slot_name_collision_across_node_and_edge_expansions_rejected_at_compile() {
    let query = QueryBuilder::nodes("Meeting")
        .expand("foo", TraverseDirection::Out, "has_task", 1, None, None)
        .traverse_edges("foo", TraverseDirection::Out, "assigned_to", 1)
        .done();

    let result = query.compile_grouped();
    assert!(
        matches!(
            result,
            Err(CompileError::DuplicateExpansionSlot(ref slot)) if slot == "foo"
        ),
        "expected DuplicateExpansionSlot(\"foo\"), got {result:?}"
    );
}
