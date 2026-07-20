//! Slice 30 (R3) — graph-retrieval arm tests.
//!
//! RED-3: temporal filter tests for the graph arm.
//!   - `graph_arm_drops_invalidated_edges`: edges with `t_invalid` in the past
//!     must NOT contribute their reachable nodes to the graph arm.
//!   - `graph_arm_temporal_fallback_excluded_or_downweighted`: IGNORED (schema gate).
//!   - `graph_arm_disabled_is_byte_identical_to_baseline`: `use_graph_arm=false` must
//!     produce byte-identical results to the two-arm baseline.
//!
//! RED-4: factoid Recall@K no-regress schema pin.
//!   - `graph_arm_factoid_recall_cdf_artifact_pinned`: asserts the CDF artifact
//!     exists and contains the expected rrf_fused exact_fact K=200 entry.
//!
//! All RED-3 tests FAIL before `use_graph_arm` is wired into `Engine::search_reranked`.

use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn node_write(kind: &str, body: &str, logical_id: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: kind.to_string(),
        body: body.to_string(),
        source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
        logical_id: Some(logical_id.to_string()),
        state: fathomdb_engine::InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

fn edge_with_t_invalid(from: &str, to: &str, logical_id: &str, t_invalid: &str) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
        logical_id: Some(logical_id.to_string()),
        body: Some(format!("{from} links to {to}")),
        t_valid: None,
        t_invalid: Some(t_invalid.to_string()),
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

fn live_edge(from: &str, to: &str, logical_id: &str) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
        logical_id: Some(logical_id.to_string()),
        body: Some(format!("{from} links to {to}")),
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

fn temporal_fallback_edge(from: &str, to: &str, logical_id: &str) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
        logical_id: Some(logical_id.to_string()),
        body: Some(format!("{from} links to {to}")),
        t_valid: Some("2024-01-01T00:00:00Z".to_string()),
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: Some(true),
    }
}

/// Build a simple BYO-LLM stub harness inline that returns a single edge.
#[allow(dead_code)]
fn write_inline_stub(dir: &TempDir, doc_id: &str, result_json: &str) -> String {
    use std::io::Write as _;
    let src = format!(
        "import json,sys\nRES='{}'\n\
         for line in sys.stdin:\n  \
           line=line.strip()\n  \
           if not line: continue\n  \
           msg=json.loads(line)\n  \
           t=msg.get('type')\n  \
           if t=='hello': print(json.dumps({{'protocol':'fathomdb.extract.v1','type':'ready','schema_version':1,'model':'stub','max_docs_per_request':1}}),flush=True)\n  \
           elif t=='extract':\n    \
             r=json.loads(RES)\n    \
             r['request_id']=msg.get('request_id')\n    \
             print(json.dumps(r),flush=True)\n",
        result_json.replace('\'', "\\'"),
    );
    let path = dir.path().join(format!("stub_{doc_id}.py"));
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(src.as_bytes()).unwrap();
    path.to_string_lossy().to_string()
}

// ---------------------------------------------------------------------------
// RED-3a: Invalidated edge must not contribute graph arm candidates
// ---------------------------------------------------------------------------

/// An edge whose `t_invalid` is in the past (year 2000) must NOT yield
/// reachable nodes in the graph arm.
///
/// Setup:
///   - Node A ("alice anchor text for search")
///   - Node B ("bob target node unreachable via dead edge")
///   - Edge A->B with t_invalid = "2000-01-01T00:00:00Z" (expired in the past)
///
/// With use_graph_arm=true, searching for "alice anchor" should find node A
/// (via text/vector) but should NOT produce B in the graph arm (the edge is
/// invalidated at t_invalid = 2000, which is in the past).
///
/// FAILS at RED because `Engine::search_reranked` does not yet accept `use_graph_arm`.
#[test]
fn graph_arm_drops_invalidated_edges() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "invalidated_edge")).expect("open");

    opened
        .engine
        .write(&[
            node_write("doc", "alice anchor text for search", "alice"),
            node_write("doc", "bob target node unreachable via dead edge", "bob"),
            // Edge expired 25 years ago — must be excluded from graph arm traversal.
            edge_with_t_invalid("alice", "bob", "edge-ab", "2000-01-01T00:00:00Z"),
        ])
        .expect("write");

    // Search "alice anchor" — finds alice via text/vector.
    // Graph arm: alice is seed; the only edge (alice->bob) has t_invalid in the
    // past, so BFS should NOT traverse it, so bob should NOT appear in graph arm.
    //
    // FAILS at RED because use_graph_arm param doesn't exist yet.
    let result = opened
        .engine
        .search_reranked("alice anchor", None, 0, true, 0.3, 0)
        .expect("search with graph arm");

    let bob_in_results = result.results.iter().any(|h| h.body.contains("bob target"));
    assert!(
        !bob_in_results,
        "graph arm must NOT surface bob via an expired edge (t_invalid=2000); \
         got results: {:?}",
        result.results.iter().map(|h| &h.body).collect::<Vec<_>>()
    );

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// RED-3b: temporal_fallback — SCHEMA GATE (test is #[ignore])
// ---------------------------------------------------------------------------

/// SCHEMA-GATE-1 resolved (HITL-SIGNED 2026-06-13): SCHEMA_VERSION 15 adds
/// `canonical_edges.temporal_fallback INTEGER`. BFS now filters
/// `AND (e.temporal_fallback IS NULL OR e.temporal_fallback = 0)`.
///
/// Setup:
///   - Node A ("carol anchor text for search")
///   - Node B ("dave reachable via live edge") — reachable via a live edge
///   - Node C ("eve unreachable via fallback edge") — only edge A->C has temporal_fallback=true
///
/// With use_graph_arm=true:
///   - B must appear (live edge A->B traversable)
///   - C must NOT appear (edge A->C has temporal_fallback=true → excluded from BFS)
#[test]
fn graph_arm_temporal_fallback_excluded_or_downweighted() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "temporal_fallback_edge")).expect("open");

    opened
        .engine
        .write(&[
            node_write("doc", "carol anchor text for search", "carol"),
            node_write("doc", "dave reachable via live edge", "dave"),
            node_write("doc", "eve unreachable via fallback edge", "eve"),
            // Live edge — BFS must traverse this.
            live_edge("carol", "dave", "edge-carol-dave"),
            // temporal_fallback edge — BFS must NOT traverse this.
            temporal_fallback_edge("carol", "eve", "edge-carol-eve"),
        ])
        .expect("write");

    // C1 seeding: query "anchor text" matches ONLY carol's node body, not the edge
    // body ("carol links to dave"). So carol is the seed and dave is graph-REACHED
    // via the live edge (not co-seeded as an edge-fact endpoint), keeping this a
    // clean test of temporal traversal filtering: dave (live) in, eve (fallback) out.
    let result = opened
        .engine
        .search_reranked("anchor text", None, 0, true, 0.3, 0)
        .expect("search with graph arm");

    let bodies: Vec<&str> = result.results.iter().map(|h| h.body.as_str()).collect();

    // Dave is reachable via the live edge.
    assert!(
        result.results.iter().any(|h| h.body.contains("dave reachable")),
        "dave (reachable via live edge) must appear in graph arm results; got: {bodies:?}"
    );

    // Eve is NOT reachable — her only path is through a temporal_fallback edge.
    assert!(
        !result.results.iter().any(|h| h.body.contains("eve unreachable")),
        "eve must NOT appear (only edge has temporal_fallback=true → excluded from BFS); \
         got: {bodies:?}"
    );

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// RED-3c: use_graph_arm=false must be byte-identical to baseline
// ---------------------------------------------------------------------------

/// With `use_graph_arm=false` (the default), `Engine::search_reranked`
/// must return results byte-identical to the pre-Slice-30 two-arm fused order.
///
/// FAILS at RED because `Engine::search_reranked` does not yet accept `use_graph_arm`.
#[test]
fn graph_arm_disabled_is_byte_identical_to_baseline() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "baseline_parity")).expect("open");

    // Ingest a small set of nodes so there are search results to compare.
    opened
        .engine
        .write(&[
            node_write("doc", "baseline search doc alpha", "n1"),
            node_write("doc", "baseline search doc beta", "n2"),
            node_write("doc", "baseline search doc gamma", "n3"),
            // Add a live edge so the graph arm would have candidates if enabled.
            live_edge("n1", "n2", "e12"),
            live_edge("n2", "n3", "e23"),
        ])
        .expect("write");

    // With use_graph_arm=false, must match the baseline (no-graph-arm) search.
    // We test this by calling search_reranked twice: once with use_graph_arm=false,
    // once with use_graph_arm=true, and asserting false==baseline.
    //
    // The baseline is the search with no graph arm (pre-Slice-30 behavior).
    // FAILS at RED because use_graph_arm param doesn't exist yet.
    let without_arm = opened
        .engine
        .search_reranked("baseline search", None, 0, false, 0.3, 0)
        .expect("search without graph arm");
    let with_arm = opened
        .engine
        .search_reranked("baseline search", None, 0, true, 0.3, 0)
        .expect("search with graph arm");

    // The two-arm result (use_graph_arm=false) must match the pre-Slice-30 output.
    // We verify by calling search() (which uses the old path) and comparing.
    let classic = opened.engine.search("baseline search").expect("classic search");

    assert_eq!(
        without_arm.results, classic.results,
        "use_graph_arm=false must produce byte-identical results to Engine::search()"
    );

    // The with_arm result may differ (graph arm can add candidates) — we just
    // verify it compiles and runs. The important assertion is the false==baseline check.
    let _ = with_arm;

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// RED-4: Factoid Recall@K no-regress schema pin
// ---------------------------------------------------------------------------

/// Asserts that `dev/plans/runs/IR-C-recall-cdf.json` exists and contains the
/// required factoid no-regress anchor:
///   arm="rrf_fused", query_class="exact_fact", k=200, found_at_k >= 0.9695.
///
/// This test GREENs immediately (the artifact exists from Slice 5/10).
/// It is included here to anchor the factoid no-regress requirement.
#[test]
fn graph_arm_factoid_recall_cdf_artifact_pinned() {
    let cdf_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../dev/plans/runs/IR-C-recall-cdf.json");

    assert!(
        cdf_path.exists(),
        "IR-C-recall-cdf.json must exist at {}: run Slice 5 to generate it",
        cdf_path.display()
    );

    let content = std::fs::read_to_string(&cdf_path).expect("read IR-C-recall-cdf.json");
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("IR-C-recall-cdf.json must be valid JSON");

    let recall_cdf =
        json["recall_cdf"].as_array().expect("IR-C-recall-cdf.json must have a 'recall_cdf' array");

    let entry = recall_cdf.iter().find(|e| {
        e["arm"].as_str() == Some("rrf_fused")
            && e["query_class"].as_str() == Some("exact_fact")
            && e["k"].as_u64() == Some(200)
    });

    let entry = entry.expect(
        "IR-C-recall-cdf.json must contain an entry with \
         arm='rrf_fused', query_class='exact_fact', k=200",
    );

    let found_at_k = entry["found_at_k"].as_f64().expect("found_at_k must be a number");

    assert!(
        found_at_k >= 0.9695,
        "factoid Recall@K=200 rrf_fused must be >= 0.9695 (got {found_at_k:.4}); \
         this is the no-regress floor from Slice 5"
    );

    println!(
        "graph_arm_factoid_recall_cdf_artifact_pinned: found_at_k={found_at_k:.4} >= 0.9695 OK"
    );
}
