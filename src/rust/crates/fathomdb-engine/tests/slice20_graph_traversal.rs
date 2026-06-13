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

use fathomdb_engine::{Engine, EngineError, PreparedWrite, SearchFilter, TraversalDirection};
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
