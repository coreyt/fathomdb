//! C1 — graph-arm SEEDING slice (the BLOCK-1 fix).
//!
//! Per `dev/plans/runs/0.8.1-c1-seeding-slice-design.md` §A/§B. The graph arm now
//! seeds the BFS frontier from the query's OWN matched FTS surfaces — edge-fact
//! FTS (`search_index_edges`) endpoints + entity-node FTS (`search_index` rows
//! with `logical_id IS NOT NULL`) — instead of doc-node hits (which carry
//! `logical_id = NULL` → empty frontier). These tests prove the 0→>0 frontier
//! flip and the seed filters, reusing the G0 `_graph_frontier_stats_for_test` seam.
//!
//! Does NOT flip `use_graph_arm` defaults — the arm is exercised explicitly.

use fathomdb_engine::{Engine, PreparedWrite, SoftFallbackBranch};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

/// Entity node with explicit logical_id (the seed surface B).
fn entity_node(body: &str, logical_id: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body.to_string(),
        source_id: None,
        logical_id: Some(logical_id.to_string()),
    }
}

/// Edge with an explicit, query-matchable `body` (the seed surface A) + flags.
#[allow(clippy::too_many_arguments)]
fn edge(
    from: &str,
    to: &str,
    logical_id: &str,
    body: &str,
    source_id: Option<&str>,
    t_invalid: Option<&str>,
    temporal_fallback: Option<bool>,
) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: source_id.map(str::to_string),
        logical_id: Some(logical_id.to_string()),
        body: Some(body.to_string()),
        t_valid: None,
        t_invalid: t_invalid.map(str::to_string),
        confidence: None,
        extractor_model_id: None,
        temporal_fallback,
    }
}

// ---------------------------------------------------------------------------
// §B-1: edge-fact FTS seeding flips the frontier rate 0 → >0
// ---------------------------------------------------------------------------

/// The query matches ONLY an edge-fact body (the entity bodies do not contain the
/// query term), so the frontier can ONLY be seeded via edge-fact FTS (source A).
/// Pre-C1 (doc-seeding) this frontier was empty; now both live endpoints seed.
#[test]
fn test_seed_from_edge_fact_fts_flips_rate_zero_to_one() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "edge_fact_seed_flip")).expect("open");
    opened
        .engine
        .write(&[
            entity_node("alice profile record", "alice"),
            entity_node("bob profile record", "bob"),
            // Edge body carries the distinctive query term; entity bodies do NOT.
            edge(
                "alice",
                "bob",
                "edge-ab",
                "quarterly acquisition agreement",
                Some("docZ"),
                None,
                None,
            ),
        ])
        .expect("write");

    // Sanity: a query matching NOTHING seeds nothing (rate 0, no panic).
    let none = opened.engine._graph_frontier_stats_for_test("nonexistent_zzz_term").expect("stats");
    assert_eq!(none.seeds_considered, 0, "no FTS match → no seeds: {none:?}");
    assert_eq!(none.resolved_seed_rate(), 0.0, "{none:?}");

    // The edge-fact FTS query seeds BOTH endpoints (alice, bob) → frontier flips.
    let hit = opened.engine._graph_frontier_stats_for_test("acquisition agreement").expect("stats");
    assert!(
        hit.seeds_considered >= 2,
        "both edge endpoints are seed candidates via edge-fact FTS: {hit:?}"
    );
    assert_eq!(
        hit.seeds_resolved, hit.seeds_considered,
        "both endpoints resolve to active nodes: {hit:?}"
    );
    assert_eq!(hit.resolved_seed_rate(), 1.0, "edge-fact seeding flips rate to 1.0: {hit:?}");
    assert!(hit.frontier_nonempty, "frontier must be non-empty after edge-fact seeding: {hit:?}");

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// codex §9 [P2]: an EDGE-ONLY query match must EMIT the endpoint entity nodes as
// graph-arm candidates (not just seed them as silent BFS roots). Pre-fix, both
// endpoints went into `visited` and neither was emitted, so an edge match returned
// only the TextEdge fact body — never the relevant entities.
// ---------------------------------------------------------------------------

#[test]
fn test_edge_only_match_emits_endpoint_entities_with_source() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "edge_only_emit")).expect("open");
    opened
        .engine
        .write(&[
            // Entity bodies do NOT contain the query terms — only the edge body does.
            entity_node("alice profile record", "alice"),
            entity_node("bob profile record", "bob"),
            edge(
                "alice",
                "bob",
                "edge-ab",
                "quarterly acquisition agreement",
                Some("docZ"),
                None,
                None,
            ),
        ])
        .expect("write");

    let result = opened
        .engine
        .search_reranked("acquisition agreement", None, 0, true)
        .expect("search with graph arm");

    // Both endpoint entities must surface as graph-arm hits carrying the edge source.
    for (lid_body, name) in [("alice profile record", "alice"), ("bob profile record", "bob")] {
        let hit = result
            .results
            .iter()
            .find(|h| h.body == lid_body)
            .unwrap_or_else(|| panic!("endpoint entity {name} must be emitted as a graph hit"));
        assert_eq!(hit.branch, SoftFallbackBranch::GraphArm, "{name} is a graph-arm hit");
        assert_eq!(
            hit.source_id.as_deref(),
            Some("docZ"),
            "{name} must carry the matched edge's source_id"
        );
    }

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// §B-5: overlapping seed endpoints dedup; the meter is deterministic
// ---------------------------------------------------------------------------

#[test]
fn test_seed_dedup_and_deterministic() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "seed_dedup")).expect("open");
    // Two edges that SHARE the `hub` endpoint, both matching the query. `hub` must
    // be considered exactly once (order-preserving dedup), not twice.
    opened
        .engine
        .write(&[
            entity_node("hub central record", "hub"),
            entity_node("spoke one record", "spoke1"),
            entity_node("spoke two record", "spoke2"),
            edge("hub", "spoke1", "e1", "merger synergy alpha", None, None, None),
            edge("hub", "spoke2", "e2", "merger synergy beta", None, None, None),
        ])
        .expect("write");

    let mut runs = Vec::new();
    for _ in 0..3 {
        let s = opened.engine._graph_frontier_stats_for_test("merger synergy").expect("stats");
        runs.push((s.seeds_considered, s.seeds_resolved, s.frontier_nonempty));
    }
    assert!(runs.iter().all(|r| *r == runs[0]), "frontier meter must be deterministic: {runs:?}");
    // hub + spoke1 + spoke2 = 3 distinct candidates (hub deduped across both edges).
    assert_eq!(runs[0].0, 3, "shared endpoint deduped to 3 distinct seeds, got {}", runs[0].0);
    assert_eq!(runs[0].1, 3, "all three resolve to active nodes");

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// §B-6: seeding excludes superseded / temporal_fallback / expired edges
// ---------------------------------------------------------------------------

/// Only a LIVE edge-fact may seed. An edge with `temporal_fallback=true` or a
/// past `t_invalid` matches the FTS but must be excluded from seeding (same
/// temporal filter the BFS traversal uses).
#[test]
fn test_seed_excludes_temporal_fallback_and_expired_edges() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "seed_excludes")).expect("open");
    opened
        .engine
        .write(&[
            entity_node("nodea record", "a"),
            entity_node("nodeb record", "b"),
            entity_node("nodec record", "c"),
            entity_node("noded record", "d"),
            // LIVE edge (a-b) — seeds.
            edge("a", "b", "live", "treaty negotiation summit", None, None, None),
            // temporal_fallback edge (c-?) — must NOT seed.
            edge("c", "cx", "fb", "treaty negotiation summit", None, None, Some(true)),
            // expired edge (d-?) — t_invalid in the past — must NOT seed.
            edge(
                "d",
                "dx",
                "exp",
                "treaty negotiation summit",
                None,
                Some("2000-01-01T00:00:00Z"),
                None,
            ),
        ])
        .expect("write");

    let s = opened.engine._graph_frontier_stats_for_test("treaty negotiation").expect("stats");
    // Only the live edge's endpoints (a, b) are active seeds. `cx`/`dx` are not
    // even written as nodes; `c`/`d` would only seed via their excluded edges.
    // Entity bodies ("... record") do not contain the query terms, so source B is empty.
    assert_eq!(
        s.seeds_resolved, 2,
        "only the live edge endpoints (a, b) seed; fallback/expired edges excluded: {s:?}"
    );
    assert!(s.frontier_nonempty, "{s:?}");

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// §B-3: an entity seed reaches a neighbor and emits it as a graph-arm hit
// ---------------------------------------------------------------------------

#[test]
fn test_graph_arm_emits_reachable_hit_from_entity_seed() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "entity_seed_emit")).expect("open");
    opened
        .engine
        .write(&[
            // Query term "zephyr" lives ONLY in the seed entity body — NOT in the
            // edge body (else the edge-fact FTS would co-seed `neigh` as an endpoint
            // and it would never be emitted as a reached candidate).
            entity_node("zephyr anchor entity", "zephyr"),
            entity_node("neighbor reachable payload node", "neigh"),
            edge("zephyr", "neigh", "e", "collaboration record xyz", Some("docSrc"), None, None),
        ])
        .expect("write");

    // "zephyr" matches only the entity (seed); the neighbor "neigh" is reached via
    // BFS over the edge and emitted as a graph-arm hit (carrying the edge source).
    let result =
        opened.engine.search_reranked("zephyr", None, 0, true).expect("search with graph arm");
    let neigh = result
        .results
        .iter()
        .find(|h| h.body.contains("neighbor reachable payload"))
        .expect("neighbor must be graph-reached from the entity seed");
    assert_eq!(neigh.branch, SoftFallbackBranch::GraphArm);

    opened.engine.close().unwrap();
}
