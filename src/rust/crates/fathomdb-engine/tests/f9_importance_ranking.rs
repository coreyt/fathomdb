//! 0.8.16 Slice 5 (F9 KEYSTONE) — importance/confidence ranking mechanism.
//!
//! Ships F9 as an OFF-by-default, observable, OPP-12-`rankable`-forward-compatible
//! MECHANISM (no eval-quality claim). Covers plan-0.8.16 §2:
//!   - R-F9-1: `canonical_nodes.importance` write/read round-trip + [0,1]
//!     validation + the 3-way sentinel (`NULL` absent / `0.0` floor / `(0.0,1.0]`).
//!   - R-F9-2: importance (node) / confidence (edge) reweight reorders vs OFF, and
//!     the contribution is observable through `PerHitExplain`.
//!   - R-F9-4: reweight-ON with all-absent (NULL) importance == reweight-OFF
//!     (graceful-neutral identity — the OPP-12 Q6a graceful-absent state).
//!
//! The reweight is multiplicative-on-fused (ADR §2.2, HITL-SIGNED 2026-07-08):
//! `score *= importance(node) * confidence(edge)`, with `NULL ⇒ neutral (1.0)`.
//! Mirrors the `apply_recency_reweight` precedent (pure fn + OFF-by-default flag +
//! `_for_test` seam). No vec0 rewrite (eu7 no-op basis).

use std::collections::HashMap;
use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    apply_importance_reweight, Engine, EngineError, PreparedWrite, SearchHit, SoftFallbackBranch,
};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

fn hit(id: u64, body: &str, score: f64) -> SearchHit {
    SearchHit {
        id,
        kind: "doc".to_string(),
        body: body.to_string(),
        score,
        branch: SoftFallbackBranch::Vector,
        source_id: None,
        ce_score: None,
        stable_id: None,
    }
}

// ---- pure-function unit tests (mirror pr_g12_recency.rs) --------------------

#[test]
fn importance_disabled_is_a_no_op() {
    let hits = vec![hit(1, "a", 0.02), hit(2, "b", 0.01)];
    let mut imp = HashMap::new();
    imp.insert(1u64, 0.1_f64); // would de-weight if enabled
    let out = apply_importance_reweight(hits.clone(), &imp, &HashMap::new(), false);
    assert_eq!(out, hits, "flag off => order + scores unchanged");
}

#[test]
fn importance_all_absent_equals_disabled_r_f9_4() {
    // R-F9-4 graceful-neutral identity: enabled but every hit is NULL/absent
    // (empty maps) must be byte-identical to the disabled result.
    let hits = vec![hit(1, "a", 0.02), hit(2, "b", 0.01)];
    let disabled = apply_importance_reweight(hits.clone(), &HashMap::new(), &HashMap::new(), false);
    let enabled_absent =
        apply_importance_reweight(hits.clone(), &HashMap::new(), &HashMap::new(), true);
    assert_eq!(enabled_absent, disabled, "all-absent reweight-ON must equal reweight-OFF");
    assert_eq!(enabled_absent, hits, "and preserve the input order/scores");
}

#[test]
fn importance_enabled_deweights_and_reorders() {
    // Node 1 outranks node 2 on raw score, but a low importance (0.1) de-weights
    // it below the neutral (absent => 1.0) node 2 => non-vacuous reorder.
    let hits = vec![hit(1, "high-raw", 0.02), hit(2, "neutral", 0.015)];
    let mut imp = HashMap::new();
    imp.insert(1u64, 0.1_f64);
    let out = apply_importance_reweight(hits, &imp, &HashMap::new(), true);
    assert_eq!(out[0].id, 2, "de-weighted node drops below the neutral node");
    assert_eq!(out[1].id, 1);
    assert!(out[0].score >= out[1].score, "reweighted list stays sorted by score desc");
}

#[test]
fn confidence_scales_contribution_pure_fn() {
    // Pure-fn coverage retained: a confidence map entry multiplies the hit's
    // contribution. (The REAL graph-arm path is proven end-to-end by
    // `confidence_on_graph_arm_reorders_and_surfaces_in_explain` below.)
    let hits = vec![hit(1, "edge", 0.02)];
    let mut conf = HashMap::new();
    conf.insert(1u64, 0.5_f64);
    let out = apply_importance_reweight(hits, &HashMap::new(), &conf, true);
    assert!((out[0].score - 0.01).abs() < 1e-9, "confidence 0.5 halves the contribution");
}

#[test]
fn importance_floor_zero_zeroes_contribution() {
    // Sentinel 0.0 = explicit floor => full de-weight (score -> 0.0).
    let hits = vec![hit(1, "floored", 0.02), hit(2, "kept", 0.015)];
    let mut imp = HashMap::new();
    imp.insert(1u64, 0.0_f64);
    let out = apply_importance_reweight(hits, &imp, &HashMap::new(), true);
    assert_eq!(out[0].id, 2, "floored node ranks last");
    assert_eq!(out[1].score, 0.0, "floor 0.0 zeroes the contribution");
}

// ---- end-to-end engine tests -----------------------------------------------

#[derive(Clone, Debug)]
struct FixedEmbedder;

impl Embedder for FixedEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new("deterministic", "rev-a", 8)
    }
    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        let mut v = vec![0.0_f32; 8];
        v[0] = 1.0;
        Ok(v)
    }
}

fn fixture(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    (dir, path)
}

/// R-F9-1 — importance write/read round-trips exact for the 3-way sentinel, and
/// an out-of-[0,1] value is rejected (mirrors the edge-confidence validation).
#[test]
fn importance_write_read_roundtrip_and_range_validation() {
    let (_dir, path) = fixture("f9_roundtrip");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    let engine = &opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");

    let receipt = engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "importance subject".to_string(),
            source_id: None,
            logical_id: None,
        }])
        .expect("write");
    let cursor = receipt.cursor;

    // Absent by default => NULL => graceful-absent.
    assert_eq!(engine.node_importance(cursor).expect("read"), None, "absent => NULL");

    for v in [0.0_f64, 0.5, 1.0] {
        engine.write_node_importance(cursor, v).expect("set importance");
        assert_eq!(engine.node_importance(cursor).expect("read"), Some(v), "round-trips exact");
    }

    // Out-of-range rejected on the write path (mirror canonical_edges.confidence).
    for bad in [-0.1_f64, 1.1] {
        assert!(
            matches!(engine.write_node_importance(cursor, bad), Err(EngineError::WriteValidation)),
            "importance {bad} out of [0,1] must be rejected"
        );
    }
    // The last good value survived the rejected writes.
    assert_eq!(engine.node_importance(cursor).expect("read"), Some(1.0));

    opened.engine.close().unwrap();
}

fn seed_two_docs(engine: &Engine) -> (u64, u64) {
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    let a = engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "importance alpha widget".to_string(),
            source_id: None,
            logical_id: None,
        }])
        .expect("write a")
        .cursor;
    let b = engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "importance beta widget".to_string(),
            source_id: None,
            logical_id: None,
        }])
        .expect("write b")
        .cursor;
    engine.drain(10_000).expect("drain");
    (a, b)
}

/// R-F9-2 — an importance reweight (ON) reorders vs the OFF baseline: de-weighting
/// the baseline top hit drops it below the neutral hit. Non-vacuous.
#[test]
fn importance_reweight_reorders_vs_off() {
    let (_dir, path) = fixture("f9_reorder");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    let engine = &opened.engine;
    seed_two_docs(engine);

    let baseline = engine.search("importance").expect("search");
    assert_eq!(baseline.results.len(), 2, "both docs retrieved");
    let top_id = baseline.results[0].id;
    let second_id = baseline.results[1].id;

    // De-weight the baseline top hit hard; enable the reweight.
    engine.write_node_importance(top_id, 0.01).expect("set importance");
    engine.set_importance_reweight_enabled_for_test(true);

    let reweighted = engine.search("importance").expect("search");
    let rw_bodies: std::collections::BTreeSet<&str> =
        reweighted.results.iter().map(|h| h.body.as_str()).collect();
    let base_bodies: std::collections::BTreeSet<&str> =
        baseline.results.iter().map(|h| h.body.as_str()).collect();
    assert_eq!(base_bodies, rw_bodies, "reweight preserves the result SET");
    assert_eq!(reweighted.results[0].id, second_id, "de-weighted top hit is now second");
    assert_eq!(reweighted.results[1].id, top_id);
    for w in reweighted.results.windows(2) {
        assert!(w[0].score >= w[1].score, "reweighted list stays sorted by score desc");
    }

    opened.engine.close().unwrap();
}

/// R-F9-4 (end-to-end) — reweight ON with all-NULL importance yields the exact
/// same ordering + scores as reweight OFF (graceful-neutral identity).
#[test]
fn importance_reweight_on_all_null_equals_off_e2e() {
    let (_dir, path) = fixture("f9_identity");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    let engine = &opened.engine;
    seed_two_docs(engine);

    let off = engine.search("importance").expect("search off");
    engine.set_importance_reweight_enabled_for_test(true);
    let on_all_null = engine.search("importance").expect("search on");

    assert_eq!(
        off.results, on_all_null.results,
        "reweight-ON with all-absent importance must equal reweight-OFF"
    );

    opened.engine.close().unwrap();
}

/// R-F9-2 (observability) — `explain=True` surfaces the importance contribution
/// on `PerHitExplain`; a node hit carries `confidence: None` (edge-only signal).
#[test]
fn explain_surfaces_importance_contribution() {
    let (_dir, path) = fixture("f9_explain");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    let engine = &opened.engine;
    let (a, _b) = seed_two_docs(engine);

    engine.write_node_importance(a, 0.5).expect("set importance");
    engine.set_importance_reweight_enabled_for_test(true);

    let explained =
        engine.search_explained("importance", None, 0, false, 0.3, 0).expect("search_explained");
    let exp = explained.explanation.expect("explanation sidecar present");

    let entry =
        exp.per_hit.iter().find(|p| p.id == a).expect("per_hit entry for the weighted node");
    assert_eq!(entry.importance, Some(0.5), "explain surfaces the node importance");
    assert_eq!(entry.confidence, None, "a node hit carries no edge confidence");

    opened.engine.close().unwrap();
}

// ---- graph-arm edge-confidence e2e (codex §9 fix-1, FINDING 1) --------------

/// Entity node with an explicit `logical_id` (so it can be a graph seed / edge
/// endpoint — mirrors `pr_c1_graph_seeding::entity_node`).
fn entity_node(body: &str, logical_id: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body.to_string(),
        source_id: None,
        logical_id: Some(logical_id.to_string()),
    }
}

/// Edge with an explicit `confidence` and a query-NON-matching `body` (so the
/// endpoints are reached by BFS traversal, not co-seeded by edge-fact FTS).
fn conf_edge(from: &str, to: &str, logical_id: &str, body: &str, confidence: f64) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: None,
        logical_id: Some(logical_id.to_string()),
        body: Some(body.to_string()),
        t_valid: None,
        t_invalid: None,
        confidence: Some(confidence),
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

/// FINDING 1 (codex §9) — edge confidence must actually reach a graph-arm NODE
/// hit and drive ranking END-TO-END through the real retrieval path (not a
/// hand-built confidence map). Two entities are reached from the same seed by
/// BFS-traversed edges of very different confidence; the reweight reorders them
/// and the confidence surfaces in `PerHitExplain`.
///
/// Setup (no vector kind configured ⇒ vector arm empty; only the seed body
/// matches the FTS query, so `beta`/`gamma` are reached ONLY via graph traversal
/// and are NOT excluded as two-arm hits):
///
/// - seed `zephyr` (matched by the query)
/// - `zephyr --(conf 0.10)--> gamma`  (edge written FIRST ⇒ lower write_cursor ⇒
///   traversed first ⇒ better graph `bfs_rank`)
/// - `zephyr --(conf 0.90)--> beta`   (edge written SECOND)
///
/// So with the reweight OFF, `gamma` (better rank) outranks `beta`; with it ON,
/// `beta` (0.90) leaps above `gamma` (0.10) — a confidence-driven FLIP.
#[test]
fn confidence_on_graph_arm_reorders_and_surfaces_in_explain() {
    let (_dir, path) = fixture("f9_graph_conf");
    let opened = Engine::open(&path).expect("open");
    let engine = &opened.engine;
    engine
        .write(&[
            entity_node("zephyr anchor entity", "zephyr"),
            entity_node("beta reachable payload node", "beta"),
            entity_node("gamma reachable payload node", "gamma"),
            // Edge bodies do NOT contain "zephyr" ⇒ endpoints are reached by BFS,
            // not co-seeded by edge-fact FTS. gamma-edge is written FIRST.
            conf_edge("zephyr", "gamma", "e-zg", "collaboration record one", 0.10),
            conf_edge("zephyr", "beta", "e-zb", "collaboration record two", 0.90),
        ])
        .expect("write");

    // Helper: 0-based index of a body in the result list.
    let pos = |results: &[SearchHit], needle: &str| -> usize {
        results
            .iter()
            .position(|h| h.body.contains(needle))
            .unwrap_or_else(|| panic!("{needle} must be present in results"))
    };

    // Reweight OFF baseline (graph arm ON): both endpoints are graph-arm hits;
    // gamma (better bfs_rank) outranks beta.
    let off = engine.search_reranked("zephyr", None, 0, true, 0.3, 0).expect("search off");
    for name in ["gamma reachable", "beta reachable"] {
        let h = off.results.iter().find(|h| h.body.contains(name)).expect("graph hit present");
        assert_eq!(h.branch, SoftFallbackBranch::GraphArm, "{name} is a graph-arm hit");
    }
    assert!(
        pos(&off.results, "gamma reachable") < pos(&off.results, "beta reachable"),
        "OFF: gamma (better bfs_rank) outranks beta — {:?}",
        off.results.iter().map(|h| (h.body.as_str(), h.score)).collect::<Vec<_>>()
    );

    // Reweight ON: the high-confidence edge (beta, 0.90) must overtake the
    // low-confidence edge (gamma, 0.10) — a confidence-driven end-to-end FLIP.
    engine.set_importance_reweight_enabled_for_test(true);
    let on = engine.search_reranked("zephyr", None, 0, true, 0.3, 0).expect("search on");
    assert!(
        pos(&on.results, "beta reachable") < pos(&on.results, "gamma reachable"),
        "ON: beta (edge conf 0.90) overtakes gamma (edge conf 0.10) — {:?}",
        on.results.iter().map(|h| (h.body.as_str(), h.score)).collect::<Vec<_>>()
    );

    // (b) Observability: the traversing edge's confidence shows in PerHitExplain.
    let explained =
        engine.search_explained("zephyr", None, 0, true, 0.3, 0).expect("search_explained");
    let exp = explained.explanation.expect("explanation sidecar present");
    let beta_id = on.results.iter().find(|h| h.body.contains("beta reachable")).unwrap().id;
    let gamma_id = on.results.iter().find(|h| h.body.contains("gamma reachable")).unwrap().id;
    let beta_exp =
        exp.per_hit.iter().find(|p| p.id == beta_id).expect("per_hit entry for beta graph hit");
    let gamma_exp =
        exp.per_hit.iter().find(|p| p.id == gamma_id).expect("per_hit entry for gamma graph hit");
    assert_eq!(
        beta_exp.confidence,
        Some(0.90),
        "explain surfaces the traversing edge confidence for beta"
    );
    assert_eq!(
        gamma_exp.confidence,
        Some(0.10),
        "explain surfaces the traversing edge confidence for gamma"
    );

    opened.engine.close().unwrap();
}
