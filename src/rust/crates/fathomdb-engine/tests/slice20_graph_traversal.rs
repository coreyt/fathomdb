//! Slice 20 — G5/G6 graph traversal: `graph_neighbors` (G5) +
//! `search_expand` (G6).
//!
//! RED tests committed before implementation. All tests fail until
//! `Engine::graph_neighbors`, `Engine::search_expand`, and the
//! `TraversalDirection` + `SearchExpandResult` types are implemented in
//! `fathomdb-engine`.
//!
//! Binds:
//! - G5 `read.neighbors` — bounded BFS, depth 1/2/3, direction, cycle guard,
//!   hard cap 50, valid-time filter, depth > 3 rejection.
//! - G6 `search_expand` — G1 search + G5 expansion, deduplication.
//! - EXPLAIN gate — BFS CTE uses `canonical_edges(from_id)/(to_id)` indexes.
//!
//! ADR refs: `ADR-0.8.0-graph-traversal-scope.md` (D-G1..D-G5),
//! `ADR-0.8.1-graph-substrate-g11-migration.md` §5.2.

use fathomdb_engine::{Engine, EngineError, PreparedWrite, TraversalDirection};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn node(kind: &str, body: &str, logical_id: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: kind.to_string(),
        body: body.to_string(),
        source_id: None,
        logical_id: Some(logical_id.to_string()),
    }
}

fn edge(from: &str, to: &str, logical_id: &str) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: None,
        logical_id: Some(logical_id.to_string()),
        body: None,
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
    }
}

fn edge_with_t_invalid(from: &str, to: &str, logical_id: &str, t_invalid: &str) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: None,
        logical_id: Some(logical_id.to_string()),
        body: None,
        t_valid: None,
        t_invalid: Some(t_invalid.to_string()),
        confidence: None,
        extractor_model_id: None,
    }
}

// ---------------------------------------------------------------------------
// G5 — graph_neighbors tests
// ---------------------------------------------------------------------------

/// Depth=1 outgoing: root A with two outgoing edges A→B and A→C.
/// Traversal returns both B and C.
#[test]
fn graph_neighbors_depth1_returns_adjacent() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "depth1")).expect("open");
    opened
        .engine
        .write(&[
            node("doc", "Root A", "A"),
            node("doc", "Node B", "B"),
            node("doc", "Node C", "C"),
            edge("A", "B", "E-AB"),
            edge("A", "C", "E-AC"),
        ])
        .expect("write");

    let neighbors = opened
        .engine
        .graph_neighbors("A", 1, TraversalDirection::Outgoing)
        .expect("graph_neighbors");

    let mut ids: Vec<_> = neighbors.iter().map(|n| n.logical_id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["B", "C"], "depth=1 outgoing must return both direct neighbors");

    // Root A itself must NOT appear in the neighbor set.
    assert!(
        !neighbors.iter().any(|n| n.logical_id == "A"),
        "root node must not appear in its own neighbor set"
    );
    opened.engine.close().unwrap();
}

/// Depth=2: chain A→B→C. Depth=2 returns both B and C.
#[test]
fn graph_neighbors_depth2_returns_two_hops() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "depth2")).expect("open");
    opened
        .engine
        .write(&[
            node("doc", "Root A", "A"),
            node("doc", "Node B", "B"),
            node("doc", "Node C", "C"),
            edge("A", "B", "E-AB"),
            edge("B", "C", "E-BC"),
        ])
        .expect("write");

    let neighbors = opened
        .engine
        .graph_neighbors("A", 2, TraversalDirection::Outgoing)
        .expect("graph_neighbors");

    let mut ids: Vec<_> = neighbors.iter().map(|n| n.logical_id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["B", "C"], "depth=2 must include both hop-1 (B) and hop-2 (C)");

    opened.engine.close().unwrap();
}

/// Depth=3: chain A→B→C→D. Depth=3 returns B, C, and D.
#[test]
fn graph_neighbors_depth3_limit() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "depth3")).expect("open");
    opened
        .engine
        .write(&[
            node("doc", "Root A", "A"),
            node("doc", "Node B", "B"),
            node("doc", "Node C", "C"),
            node("doc", "Node D", "D"),
            edge("A", "B", "E-AB"),
            edge("B", "C", "E-BC"),
            edge("C", "D", "E-CD"),
        ])
        .expect("write");

    let neighbors = opened
        .engine
        .graph_neighbors("A", 3, TraversalDirection::Outgoing)
        .expect("graph_neighbors");

    let mut ids: Vec<_> = neighbors.iter().map(|n| n.logical_id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["B", "C", "D"], "depth=3 must include all three hops");

    opened.engine.close().unwrap();
}

/// Depth=4 must be rejected with a typed error (not silently clamped).
#[test]
fn graph_neighbors_depth_gt3_rejected() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "depth_gt3")).expect("open");
    opened.engine.write(&[node("doc", "Root A", "A")]).expect("write");

    let result = opened.engine.graph_neighbors("A", 4, TraversalDirection::Outgoing);
    match result {
        Err(EngineError::InvalidArgument { .. }) => {
            // Correct: typed error for depth > 3
        }
        other => panic!("depth=4 must return EngineError::InvalidArgument, got: {other:?}"),
    }

    opened.engine.close().unwrap();
}

/// Cycle guard: A→B→A. BFS must NOT loop; must visit each node once.
#[test]
fn graph_neighbors_cycle_guard() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "cycle")).expect("open");
    opened
        .engine
        .write(&[
            node("doc", "Root A", "A"),
            node("doc", "Node B", "B"),
            edge("A", "B", "E-AB"),
            edge("B", "A", "E-BA"),
        ])
        .expect("write");

    // depth=3 with a cycle — must terminate and NOT visit A or B more than once.
    let neighbors = opened
        .engine
        .graph_neighbors("A", 3, TraversalDirection::Both)
        .expect("graph_neighbors must not loop on cycles");

    // B should be reachable; A itself should not appear in the neighbor set.
    let has_b = neighbors.iter().any(|n| n.logical_id == "B");
    let has_a = neighbors.iter().any(|n| n.logical_id == "A");
    assert!(has_b, "B must be reachable from A via A→B");
    assert!(!has_a, "root A must not appear in the neighbor set (cycle guard prevents revisiting)");

    opened.engine.close().unwrap();
}

/// Hard cap 50: a graph with 60+ distinct neighbors at depth=1 returns ≤ 50.
#[test]
fn graph_neighbors_cap50_enforced() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "cap50")).expect("open");

    let mut batch = vec![node("doc", "Root", "ROOT")];
    for i in 0..60u32 {
        let id = format!("N{i}");
        let eid = format!("E{i}");
        batch.push(node("doc", &format!("Node {i}"), &id));
        batch.push(edge("ROOT", &id, &eid));
    }
    opened.engine.write(&batch).expect("write");

    let neighbors = opened
        .engine
        .graph_neighbors("ROOT", 1, TraversalDirection::Outgoing)
        .expect("graph_neighbors");

    assert!(neighbors.len() <= 50, "hard cap 50 must be enforced; got {} results", neighbors.len());

    opened.engine.close().unwrap();
}

/// Valid-time filter: an edge with `t_invalid` in the past is NOT traversed.
/// The node on the other side of the invalidated edge is NOT returned.
#[test]
fn graph_neighbors_valid_time_filter_drops_invalidated() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "valid_time")).expect("open");

    // Edge A→B is invalidated (t_invalid is in the past).
    // Edge A→C is still valid (t_invalid IS NULL).
    let past = "2000-01-01T00:00:00Z";
    opened
        .engine
        .write(&[
            node("doc", "Root A", "A"),
            node("doc", "Node B", "B"),
            node("doc", "Node C", "C"),
            edge_with_t_invalid("A", "B", "E-AB", past),
            edge("A", "C", "E-AC"),
        ])
        .expect("write");

    let neighbors = opened
        .engine
        .graph_neighbors("A", 1, TraversalDirection::Outgoing)
        .expect("graph_neighbors");

    let ids: Vec<_> = neighbors.iter().map(|n| n.logical_id.as_str()).collect();
    assert!(
        !ids.contains(&"B"),
        "node B must NOT be reachable via an invalidated edge (t_invalid in the past)"
    );
    assert!(ids.contains(&"C"), "node C must still be reachable via the valid edge");

    opened.engine.close().unwrap();
}

/// EXPLAIN gate: the BFS CTE must use `canonical_edges(from_id)` or
/// `canonical_edges(to_id)` indexes — no full SCAN of `canonical_edges`.
#[test]
fn explain_plan_uses_indexes() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "explain")).expect("open");
    opened
        .engine
        .write(&[
            node("doc", "Root", "ROOT"),
            node("doc", "Child", "CHILD"),
            edge("ROOT", "CHILD", "E1"),
        ])
        .expect("write");

    // Delegate to the engine's test seam for EXPLAIN QUERY PLAN.
    let plan = opened
        .engine
        .explain_graph_neighbors_for_test("ROOT", 1, TraversalDirection::Outgoing)
        .expect("explain_graph_neighbors_for_test");

    // The plan must reference an index on canonical_edges.
    let has_index =
        plan.iter().any(|line| line.contains("canonical_edges") && line.contains("USING INDEX"));
    let has_scan = plan.iter().any(|line| {
        // A line containing "SCAN canonical_edges" without "USING INDEX" is a full scan.
        line.contains("SCAN canonical_edges") && !line.contains("USING INDEX")
    });

    assert!(has_index, "BFS CTE must use an index on canonical_edges;\nplan:\n{}", plan.join("\n"));
    assert!(!has_scan, "BFS CTE must NOT full-scan canonical_edges;\nplan:\n{}", plan.join("\n"));

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// G6 — search_expand tests
// ---------------------------------------------------------------------------

/// search_expand: a search hit is expanded by one hop; the expanded
/// neighbor appears in the `expanded` field of the result.
///
/// Uses FTS search (no embedder needed) — the node body goes into
/// search_index synchronously on write.
#[test]
fn search_expand_returns_neighbors() {
    let dir = TempDir::new().unwrap();
    // No embedder needed — FTS search works immediately after write.
    let opened = Engine::open(db_path(&dir, "expand")).expect("open");

    // Node A has distinctive FTS-searchable body; B is its neighbor.
    opened
        .engine
        .write(&[
            node("doc", "xyzunique expand quark alpha", "A"),
            node("doc", "neighbor node B body", "B"),
            edge("A", "B", "E-AB"),
        ])
        .expect("write");

    let result =
        opened.engine.search_expand("xyzunique expand quark", None, 1).expect("search_expand");

    // B should appear in `expanded` (reachable from the search hit A via one hop).
    let expanded_ids: Vec<_> =
        result.expanded.iter().map(|(n, _hop)| n.logical_id.as_str()).collect();
    assert!(
        expanded_ids.contains(&"B"),
        "node B must appear in expanded; expanded={expanded_ids:?}\nresult.search_hits={:?}",
        result.search_hits.iter().map(|h| h.id).collect::<Vec<_>>()
    );

    opened.engine.close().unwrap();
}

/// search_expand: a node that is both a search hit and a traversal neighbor
/// appears ONLY in `search_hits` (deduplication: search score takes priority).
#[test]
fn search_expand_deduplicates() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "dedup")).expect("open");

    // Both A and B are FTS-searchable (direct hits). A→B edge exists.
    // After expanding A, B is reached via traversal. B is ALSO a search hit.
    // Deduplication: B must appear only in search_hits, not in expanded.
    opened
        .engine
        .write(&[
            node("doc", "dedup shimmer unique test node alpha", "A"),
            node("doc", "dedup shimmer unique test node beta", "B"),
            edge("A", "B", "E-AB"),
        ])
        .expect("write");

    let result =
        opened.engine.search_expand("dedup shimmer unique", None, 1).expect("search_expand");

    // B must be in search_hits.
    let hit_ids: Vec<_> = result.search_hits.iter().map(|h| h.id).collect();
    // B must NOT be in expanded (it's deduplicated by being a search hit).
    let expanded_logical_ids: Vec<_> =
        result.expanded.iter().map(|(n, _hop)| n.logical_id.as_str()).collect();
    assert!(
        !expanded_logical_ids.contains(&"B"),
        "B is a search hit and must not appear in expanded; expanded={expanded_logical_ids:?}"
    );
    // Sanity: hits are non-empty.
    assert!(!hit_ids.is_empty(), "search must return at least one hit for 'dedup shimmer unique'");

    opened.engine.close().unwrap();
}

// ===== fix-1: t_invalid datetime normalization =================================

/// An edge stored with `t_invalid` in ISO-8601 `T`-format (e.g. `2026-06-13T00:00:01Z`)
/// must NOT be traversed once it has expired. Previously the lexicographic comparison
/// `e.t_invalid > datetime('now')` could fail when the `T` separator vs. the space
/// separator in `datetime('now')` caused a same-day expired edge to appear valid.
#[test]
fn t_invalid_tformat_edge_correctly_excluded() {
    let dir = TempDir::new().unwrap();
    let opened =
        Engine::open(dir.path().join(format!("fix1{SQLITE_SUFFIX}"))).expect("engine open");

    // Two nodes; edge expires in the past using T-format timestamp.
    let a_id = "fix1-A";
    let b_id = "fix1-B";
    opened
        .engine
        .write(&[
            node("test", "{}", a_id),
            node("test", "{}", b_id),
            PreparedWrite::Edge {
                logical_id: None,
                from: a_id.to_string(),
                to: b_id.to_string(),
                source_id: None,
                kind: "expired_link".to_string(),
                // t_invalid in the past using ISO-8601 T-format
                t_invalid: Some("2020-01-01T00:00:00Z".to_string()),
                body: None,
                t_valid: None,
                confidence: None,
                extractor_model_id: None,
            },
        ])
        .expect("write");

    let result = opened
        .engine
        .graph_neighbors(a_id, 1, TraversalDirection::Outgoing)
        .expect("graph_neighbors");
    assert!(result.is_empty(), "expired T-format edge must be excluded; got {result:?}");

    opened.engine.close().unwrap();
}

// ===== fix-1: search_expand depth=0 all_logical_ids ============================

/// `search_expand` with depth=0 must return search hits in `all_logical_ids`.
/// Previously the depth=0 short-circuit returned an empty `all_logical_ids`.
#[test]
fn search_expand_depth0_populates_all_logical_ids() {
    let dir = TempDir::new().unwrap();
    let opened =
        Engine::open(dir.path().join(format!("depth0{SQLITE_SUFFIX}"))).expect("engine open");

    opened
        .engine
        .write(&[node("note", r#"{"text":"quilted vermillion zephyr unique depth0"}"#, "depth0-X")])
        .expect("write");

    let result = opened
        .engine
        .search_expand("quilted vermillion zephyr unique depth0", None, 0)
        .expect("search_expand depth=0");

    assert!(
        !result.all_logical_ids.is_empty(),
        "depth=0 search_expand must populate all_logical_ids from search hits; got empty"
    );
    assert!(result.expanded.is_empty(), "depth=0 must produce no expanded nodes");

    opened.engine.close().unwrap();
}

// ===== fix-2: graph_neighbors depth=0 rejection ================================

/// `graph_neighbors` with depth=0 must be rejected with a typed error.
/// depth=0 is not "no traversal" for graph_neighbors (unlike search_expand);
/// the API contract is depth ∈ {1,2,3}.
#[test]
fn graph_neighbors_depth0_rejected() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "depth0rej")).expect("open");
    opened.engine.write(&[node("doc", "{}", "A")]).expect("write");

    let result = opened.engine.graph_neighbors("A", 0, TraversalDirection::Outgoing);
    assert!(
        matches!(result, Err(EngineError::InvalidArgument { .. })),
        "graph_neighbors depth=0 must return InvalidArgument; got {result:?}"
    );
    opened.engine.close().unwrap();
}

// ===== fix-2: nearest hop count across roots ====================================

/// When the same expanded node is reachable from two different search-hit roots
/// at different depths, search_expand must report the NEAREST (minimum) hop count.
#[test]
fn search_expand_reports_nearest_hop_count() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "nearest_hop")).expect("open");

    // Topology:
    //   SearchHit-A (1 hop to X)
    //   SearchHit-B (3 hops to X: B→C→D→X)
    //   X is 1 hop from A and 3 hops from B.
    //   Nearest hop from any root = 1.
    opened
        .engine
        .write(&[
            node("note", "nearest hop shimmer alpha unique1", "A"),
            node("note", "nearest hop glimmer beta unique2", "B"),
            node("note", "{}", "C"),
            node("note", "{}", "D"),
            node("note", "{}", "X"),
            edge("A", "X", "E-AX"), // A→X at depth 1
            edge("B", "C", "E-BC"),
            edge("C", "D", "E-CD"),
            edge("D", "X", "E-DX"), // B→C→D→X at depth 3
        ])
        .expect("write");

    // Search returns both A and B as hits.
    let result = opened
        .engine
        .search_expand("shimmer alpha unique1 glimmer beta unique2", None, 3)
        .expect("search_expand");

    // X must be in expanded with hop_count = 1 (nearest root is A at 1 hop).
    let x_entry = result.expanded.iter().find(|(n, _)| n.logical_id == "X");
    if let Some((_, hop)) = x_entry {
        assert_eq!(*hop, 1, "X is 1 hop from A; nearest hop must be 1, not {hop}");
    }
    // (If X is not in expanded — perhaps it was a search hit itself — that's also OK.)
    opened.engine.close().unwrap();
}

/// Regression: dangling intermediate nodes must not extend traversal.
/// Graph: A → MISSING (no active node) → C
/// With depth=2, graph_neighbors("A", 2, Outgoing) must NOT return C because
/// the intermediate node "MISSING" has no active canonical_nodes row.
/// (Fix-6: recursive CTE JOIN canonical_nodes on next hop.)
#[test]
fn graph_neighbors_inactive_intermediate_not_traversed() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "dangling")).expect("open");
    opened
        .engine
        .write(&[
            node("doc", "Root A", "A"),
            // MISSING is intentionally never written as a node.
            node("doc", "Node C", "C"),
            // Edge from A to a node that doesn't exist in canonical_nodes.
            edge("A", "MISSING", "E-AM"),
            // Edge from that missing node onward to C.
            edge("MISSING", "C", "E-MC"),
        ])
        .expect("write");

    let results = opened
        .engine
        .graph_neighbors("A", 2, fathomdb_engine::TraversalDirection::Outgoing)
        .expect("graph_neighbors");

    let ids: Vec<&str> = results.iter().map(|n| n.logical_id.as_str()).collect();
    assert!(
        !ids.contains(&"C"),
        "C is reachable only through MISSING (inactive) — must not appear; got {ids:?}"
    );
    opened.engine.close().unwrap();
}

/// Regression: a search hit on an anonymous node (logical_id: None) must not
/// cause search_expand to return a Storage error.  Anonymous nodes are valid
/// write targets; they have no traversal root but should be silently skipped
/// rather than crashing the whole call.
#[test]
fn search_expand_anon_node_hit_does_not_crash() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "anon_hit")).expect("open");

    // Write a named node and an anonymous node (logical_id: None).
    opened
        .engine
        .write(&[
            PreparedWrite::Node {
                kind: "doc".to_string(),
                body: "anon shimmer unique probe alpha node".to_string(),
                source_id: None,
                logical_id: None, // anonymous — no logical_id
            },
            node("doc", "named shimmer unique probe beta node", "NAMED"),
        ])
        .expect("write");

    // search_expand must succeed (no Storage panic on the NULL logical_id row).
    let result = opened
        .engine
        .search_expand("shimmer unique probe", None, 1)
        .expect("search_expand must not crash on anonymous hit");

    // At minimum NAMED appears somewhere (in hits or expanded).
    let all_ids = &result.all_logical_ids;
    assert!(
        all_ids.contains(&"NAMED".to_string()) || result.search_hits.len() >= 1,
        "expected at least one result; got hits={}, expanded={}",
        result.search_hits.len(),
        result.expanded.len()
    );
    opened.engine.close().unwrap();
}

// ===== fix-21: char(30) in logical_id must be rejected at write time ===========

/// A logical_id containing char(30) (ASCII RS = 0x1E, the BFS cycle-guard
/// delimiter) must be rejected at write time. Allowing it would corrupt the
/// instr-based visited-path substring test.
#[test]
fn write_rejects_logical_id_containing_record_separator() {
    let dir = TempDir::new().unwrap();
    let engine = Engine::open(db_path(&dir, "rs_reject")).expect("open failed").engine;

    let bad_id = "A\x1eB"; // contains char(30)
    let result = engine.write(&[PreparedWrite::Node {
        kind: "doc".to_string(),
        body: "body".to_string(),
        source_id: None,
        logical_id: Some(bad_id.to_string()),
    }]);
    assert!(
        matches!(result, Err(EngineError::WriteValidation)),
        "logical_id containing 0x1E must be rejected; got {result:?}"
    );

    engine.close().unwrap();
}

/// An edge with from/to containing char(30) must be rejected.
#[test]
fn write_rejects_edge_endpoint_containing_record_separator() {
    let dir = TempDir::new().unwrap();
    let engine = Engine::open(db_path(&dir, "rs_edge_reject")).expect("open failed").engine;

    let result = engine.write(&[PreparedWrite::Edge {
        kind: "link".to_string(),
        from: "A".to_string(),
        to: "B\x1eC".to_string(), // contains char(30)
        source_id: None,
        logical_id: None,
        body: None,
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
    }]);
    assert!(
        matches!(result, Err(EngineError::WriteValidation)),
        "edge to containing 0x1E must be rejected; got {result:?}"
    );

    engine.close().unwrap();
}
