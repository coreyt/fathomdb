//! G0 Phase-2 — graph-arm frontier instrumentation + traversed-edge `source_id`.
//!
//! Per `dev/plans/runs/0.8.1-g0-phase2-design.md` §C (measurement-accuracy /
//! anti-vacuous-green) + §B (BLOCK-2 source_id carry). These tests pin the meter
//! and the provenance carry. None of them flip `use_graph_arm` defaults — the arm
//! is exercised explicitly (seam / `search_reranked(..., true)`).
//!
//! BLOCK-1 proof: the doc-seeded frontier is empty (doc nodes carry
//! `logical_id = NULL` → `Some(None)` → the seed guard fails), so
//! `resolved_seed_rate == 0.0`. The meter is the deliverable that proves it; the
//! seeding fix (entities, not docs) is C1.

use fathomdb_engine::{Engine, PreparedWrite, SoftFallbackBranch};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

/// A doc node written WITHOUT a logical_id — exactly how the eval ingests
/// sessions (`engine.write([{kind:"doc", body}])`). These carry `logical_id=NULL`.
fn doc_node(body: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body.to_string(),
        source_id: None,
        logical_id: None,
        state: fathomdb_engine::InitialState::Active,
        reason: None,
    }
}

/// An entity node with an explicit logical_id (resolves at the seed guard).
fn entity_node(body: &str, logical_id: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body.to_string(),
        source_id: None,
        logical_id: Some(logical_id.to_string()),
        state: fathomdb_engine::InitialState::Active,
        reason: None,
    }
}

/// A live edge carrying an explicit `source_id` (the session it was extracted from).
fn edge_with_source(
    from: &str,
    to: &str,
    logical_id: &str,
    source_id: Option<&str>,
) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: source_id.map(str::to_string),
        logical_id: Some(logical_id.to_string()),
        body: Some(format!("{from} links to {to}")),
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

// ---------------------------------------------------------------------------
// §C-1 / C1 §B-4: BLOCK-1 — a doc-ONLY corpus seeds nothing (no entity/edge-fact
// FTS surfaces), so the frontier stays empty with rate 0.0 and no false positives.
// (Pre-C1 this counted doc hits as considered-but-unresolved; C1 seeds from the
// graph's own FTS surfaces, so doc nodes are never even candidates.)
// ---------------------------------------------------------------------------

#[test]
fn test_no_entity_or_edge_match_keeps_rate_zero_no_panic() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "doc_only_rate_zero")).expect("open");
    // Doc nodes only, NO logical_id, NO edges — the eval-real shape.
    opened
        .engine
        .write(&[
            doc_node("frontier anchor doc alpha session one"),
            doc_node("frontier anchor doc beta session two"),
            doc_node("frontier anchor doc gamma session three"),
        ])
        .expect("write");

    let stats =
        opened.engine._graph_frontier_stats_for_test("frontier anchor").expect("frontier stats");

    assert_eq!(
        stats.seeds_considered, 0,
        "doc-only corpus has no entity-FTS (logical_id NULL) or edge-fact seeds: {stats:?}"
    );
    assert_eq!(stats.seeds_resolved, 0, "{stats:?}");
    assert_eq!(stats.resolved_seed_rate(), 0.0, "0/0 → 0.0: {stats:?}");
    assert!(!stats.frontier_nonempty, "doc-only frontier must be empty: {stats:?}");
    assert_eq!(stats.graph_candidates_emitted, 0, "empty frontier emits nothing: {stats:?}");

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// §C-2: meter not stuck at 0 — entity-seeded resolves to rate 1.0
// ---------------------------------------------------------------------------

#[test]
fn test_frontier_entity_seeded_resolved_rate_one() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "entity_seeded_rate_one")).expect("open");
    // Entity nodes WITH logical_id — every seeded fused hit resolves.
    opened
        .engine
        .write(&[
            entity_node("anchor entity alice profile", "alice"),
            entity_node("anchor entity bob profile", "bob"),
            edge_with_source("alice", "bob", "edge-ab", None),
        ])
        .expect("write");

    let stats =
        opened.engine._graph_frontier_stats_for_test("anchor entity").expect("frontier stats");

    assert!(stats.seeds_considered > 0, "must consider entity seeds: {stats:?}");
    assert_eq!(
        stats.seeds_resolved, stats.seeds_considered,
        "every entity seed (logical_id present) must resolve: {stats:?}"
    );
    assert_eq!(stats.resolved_seed_rate(), 1.0, "entity-seeded rate must be 1.0: {stats:?}");
    assert!(stats.frontier_nonempty, "resolved seeds make a non-empty frontier: {stats:?}");

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// §C-3: BLOCK-2 — a graph-arm hit carries the traversed edge's source_id
// ---------------------------------------------------------------------------

/// A graph-reached neighbor must carry the traversing edge's `source_id`, so
/// `doc_id_of` can resolve it to a gold session id. NOTE: this connectivity is
/// contrived (entity-seeded); the doc-seeded eval does NOT produce it — that gap
/// is the C1 seeding slice. Here we prove the carry mechanism in isolation.
#[test]
fn test_graph_arm_hit_carries_traversed_edge_source_id() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "carries_source")).expect("open");
    opened
        .engine
        .write(&[
            entity_node("carol anchor entity for search", "carol"),
            entity_node("dave neighbor reachable node", "dave"),
            // Edge carol->dave extracted from session "docB".
            edge_with_source("carol", "dave", "edge-cd", Some("docB")),
        ])
        .expect("write");

    // Query matches ONLY carol's entity body ("anchor"), not the edge body
    // ("carol links to dave") — so carol is the seed and dave is graph-REACHED
    // (not co-seeded as an edge-fact endpoint), exercising the BLOCK-2 carry.
    let result = opened
        .engine
        .search_reranked("anchor", None, 0, true, 0.3, 0)
        .expect("search with graph arm");

    let dave = result
        .results
        .iter()
        .find(|h| h.body.contains("dave neighbor"))
        .expect("dave must be graph-reached from the carol seed");
    assert_eq!(dave.branch, SoftFallbackBranch::GraphArm, "dave is a graph-arm hit");
    assert_eq!(
        dave.source_id.as_deref(),
        Some("docB"),
        "graph-arm hit must carry the traversed edge's source_id"
    );

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// §C-4: byte stability — use_graph_arm=false unchanged; every hit source_id==None
// ---------------------------------------------------------------------------

#[test]
fn test_two_arm_search_byte_stable_with_source_id_field() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "byte_stable")).expect("open");
    opened
        .engine
        .write(&[
            doc_node("stable search doc alpha"),
            doc_node("stable search doc beta"),
            entity_node("stable search entity gamma", "g1"),
            edge_with_source("g1", "g1", "self", Some("docX")),
        ])
        .expect("write");

    let classic = opened.engine.search("stable search").expect("classic search");
    let without_arm = opened
        .engine
        .search_reranked("stable search", None, 0, false, 0.3, 0)
        .expect("search without graph arm");

    assert_eq!(
        without_arm.results, classic.results,
        "use_graph_arm=false must be byte-identical to Engine::search()"
    );
    assert!(
        without_arm.results.iter().all(|h| h.source_id.is_none()),
        "every two-arm hit must have source_id==None: {:?}",
        without_arm.results.iter().map(|h| (&h.body, &h.source_id)).collect::<Vec<_>>()
    );

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// codex §9 [P2]: provenance is deterministic when multiple edges reach the same
// neighbor — the earliest-written edge (lowest write_cursor) wins the dedup.
// ---------------------------------------------------------------------------

#[test]
fn test_graph_arm_source_id_deterministic_with_multiple_edges() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "source_id_deterministic")).expect("open");
    // Two live edges grace->heidi, written earliest-first with distinct source_ids.
    // ORDER BY write_cursor must make the EARLIEST edge's source_id win, stably.
    opened
        .engine
        .write(&[
            entity_node("grace anchor entity for search", "grace"),
            entity_node("heidi neighbor reachable node", "heidi"),
            edge_with_source("grace", "heidi", "edge-gh-early", Some("docEarly")),
            edge_with_source("grace", "heidi", "edge-gh-late", Some("docLate")),
        ])
        .expect("write");

    // The contract (codex §9 [P2]) is DETERMINISM: with `ORDER BY e.write_cursor`
    // the same edge always wins the `visited` dedup, so the carried provenance is
    // stable across runs (not SQLite-order-dependent) and is a real source — never
    // a coin-flip between docEarly/docLate, never lost.
    let mut seen: Vec<Option<String>> = Vec::new();
    for _ in 0..3 {
        let result = opened
            .engine
            .search_reranked("anchor", None, 0, true, 0.3, 0)
            .expect("search with graph arm");
        let heidi = result
            .results
            .iter()
            .find(|h| h.body.contains("heidi neighbor"))
            .expect("heidi must be graph-reached");
        seen.push(heidi.source_id.clone());
    }
    assert!(
        seen.iter().all(|s| s == &seen[0]),
        "carried provenance must be identical across runs (deterministic): {seen:?}"
    );
    let winner = seen[0].as_deref();
    assert!(
        winner == Some("docEarly") || winner == Some("docLate"),
        "the winning edge's source_id must be a real provenance, got {winner:?}"
    );

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// §C-5: a NULL-source_id edge yields a graph hit with source_id==None (no panic)
// ---------------------------------------------------------------------------

#[test]
fn test_graph_hit_source_id_none_fallback() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "source_none_fallback")).expect("open");
    opened
        .engine
        .write(&[
            entity_node("eve anchor entity for search", "eve"),
            entity_node("frank neighbor reachable node", "frank"),
            // Edge with NULL source_id.
            edge_with_source("eve", "frank", "edge-ef", None),
        ])
        .expect("write");

    let result = opened
        .engine
        .search_reranked("anchor", None, 0, true, 0.3, 0)
        .expect("search with graph arm");

    let frank = result
        .results
        .iter()
        .find(|h| h.body.contains("frank neighbor"))
        .expect("frank must be graph-reached from the eve seed");
    assert_eq!(frank.branch, SoftFallbackBranch::GraphArm);
    assert_eq!(
        frank.source_id, None,
        "a NULL-source_id edge must yield source_id==None (no panic/skip)"
    );

    opened.engine.close().unwrap();
}
