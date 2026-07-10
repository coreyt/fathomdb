//! 0.8.8 EXP-OBS (Slice 5) — the opt-in `explain` retrieval-explanation sidecar.
//!
//! Pins two load-bearing contracts:
//!   * **Byte-stability / zero-cost (R-OBS-2):** the default search paths
//!     (`search`, `search_reranked`) return `explanation == None`, and
//!     `search_explained` returns *byte-identical* `results` (+ cursor +
//!     soft_fallback) to `search_reranked` — the only difference is the populated
//!     sidecar. The explain capture is read-only and must not perturb ranking.
//!   * **Field fidelity (R-OBS-1):** the per-hit breakdown is parallel to (and in
//!     the same order as) `results`; each `PerHitExplain` mirrors the returned
//!     hit (`id`, `arm == branch`, `blended == score`, `ce_score`); the
//!     query-level `QueryTrace` carries the request knobs + embedder identity +
//!     per-arm input counts.
//!
//! Reuses the engine's own fusion machinery (no parallel system, R-OBS-3): the
//! per-arm ranks come from the same lists `fuse_three_arms` consumes, the
//! `fused_score` from the same post-recency / pre-CE intermediate `ce_rerank`
//! normalizes.

use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

/// Deterministic embedder so ordering is a pure function of the corpus.
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

fn seed_corpus(engine: &Engine) {
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    for body in ["hybrid retrieval alpha", "hybrid retrieval beta", "hybrid retrieval gamma"] {
        engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: body.to_string(),
                source_id: None,
                logical_id: None,
                state: fathomdb_engine::InitialState::Active,
                reason: None,
            }])
            .expect("write");
    }
    engine.drain(10_000).expect("drain");
}

#[test]
fn default_search_paths_suppress_the_explanation_sidecar() {
    let (_dir, path) = fixture("exp_obs_off");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    seed_corpus(&opened.engine);

    // `search` and `search_reranked` (explain=false) → no sidecar.
    let plain = opened.engine.search("hybrid").expect("search");
    assert!(!plain.results.is_empty(), "expected fused hits");
    assert!(plain.explanation.is_none(), "default search must suppress the explanation");

    let reranked =
        opened.engine.search_reranked("hybrid", None, 0, false, 0.3, 0).expect("search_reranked");
    assert!(reranked.explanation.is_none(), "search_reranked must suppress the explanation");

    opened.engine.close().unwrap();
}

#[test]
fn search_explained_is_byte_identical_results_plus_a_sidecar() {
    let (_dir, path) = fixture("exp_obs_byte_stable");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    seed_corpus(&opened.engine);

    let plain =
        opened.engine.search_reranked("hybrid", None, 0, false, 0.3, 0).expect("search_reranked");
    let explained =
        opened.engine.search_explained("hybrid", None, 0, false, 0.3, 0).expect("search_explained");

    // The retrieval is identical — only the sidecar differs.
    assert_eq!(
        explained.results, plain.results,
        "explain path must not perturb the fused/reranked results"
    );
    assert_eq!(explained.projection_cursor, plain.projection_cursor);
    assert_eq!(explained.soft_fallback, plain.soft_fallback);
    assert!(plain.explanation.is_none());
    assert!(explained.explanation.is_some(), "explain path must populate the sidecar");

    opened.engine.close().unwrap();
}

#[test]
fn explanation_per_hit_mirrors_results_and_trace_is_populated() {
    let (_dir, path) = fixture("exp_obs_fidelity");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    seed_corpus(&opened.engine);

    let explained =
        opened.engine.search_explained("hybrid", None, 0, false, 0.3, 0).expect("search_explained");
    let exp = explained.explanation.as_ref().expect("sidecar present");

    // Per-hit breakdown is parallel to results, same order.
    assert_eq!(exp.per_hit.len(), explained.results.len(), "one per_hit entry per returned hit");
    for (p, h) in exp.per_hit.iter().zip(&explained.results) {
        assert_eq!(p.id, h.id, "per_hit id mirrors the hit");
        assert_eq!(p.arm, h.branch, "per_hit arm == hit branch");
        assert_eq!(p.blended, h.score, "per_hit blended == hit score");
        assert_eq!(p.ce_score, h.ce_score, "per_hit ce_score == hit ce_score");
        // depth=0 → identity rerank, so the raw fused score equals the blended score.
        assert_eq!(p.fused_score, p.blended, "depth=0: fused_score == blended");
        // Every returned hit was surfaced by at least one arm.
        assert!(
            p.vector_rank.is_some() || p.text_rank.is_some() || p.graph_rank.is_some(),
            "each hit carries at least one arm rank"
        );
    }

    // Query-level trace.
    assert_eq!(exp.trace.query_chars, "hybrid".chars().count() as u32);
    assert_eq!(exp.trace.rerank_depth, 0);
    assert_eq!(exp.trace.alpha, 0.3);
    assert!(!exp.trace.use_graph_arm);
    assert!(!exp.trace.ce_active, "no CE rerank at depth 0");
    assert!(
        exp.trace.embedder_id.contains("deterministic@rev-a"),
        "trace records the active embedder identity, got {:?}",
        exp.trace.embedder_id
    );
    assert!(
        exp.trace.vector_hits > 0 || exp.trace.text_hits > 0,
        "at least one arm produced input hits"
    );

    opened.engine.close().unwrap();
}

#[test]
fn r_obs_1_golden_field_fidelity_at_rerank_depth_gt0() {
    // R-OBS-1 golden — a fixture corpus + a query hitting ≥2 arms with
    // rerank_depth>0. The identity / raw-score / knob assertions are
    // model-INDEPENDENT (they hold whether or not the CE model is loaded), so this
    // runs in the default suite. `ce_score`-presence is model-dependent and only
    // checked conditionally (mirrors pr_g10_reranker_ce's `model_scored` guard).
    let (_dir, path) = fixture("exp_obs_golden");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    seed_corpus(&opened.engine);

    let depth = 3usize;
    let explained = opened
        .engine
        .search_explained("hybrid", None, depth, false, 0.3, 0)
        .expect("search_explained");
    let exp = explained.explanation.as_ref().expect("sidecar present");

    // Order-parallel + the three self-consistency identities.
    assert_eq!(exp.per_hit.len(), explained.results.len());
    for (p, h) in exp.per_hit.iter().zip(&explained.results) {
        assert_eq!(p.id, h.id, "per_hit[i].id == results[i].id");
        assert_eq!(p.arm, h.branch, "per_hit[i].arm == results[i].branch");
        assert_eq!(p.ce_score, h.ce_score, "per_hit[i].ce_score == results[i].ce_score");
        assert_eq!(p.blended, h.score, "per_hit[i].blended == results[i].score");
        // ce_score, when present (CE model loaded), is a sigmoid ∈ [0,1].
        if let Some(ce) = p.ce_score {
            assert!((0.0..=1.0).contains(&ce), "ce_score ∈ [0,1], got {ce}");
        }
        assert!(p.vector_rank.is_some() || p.text_rank.is_some() || p.graph_rank.is_some());
    }

    // `fused_score` is the RAW post-recency / pre-CE RRF value — NOT min-max
    // normalized. A normalized set would have max == 1.0; RRF magnitudes (K=60)
    // are always well under 1.0. Pin that the top fused_score is a raw RRF value.
    let top_fused = exp.per_hit.first().expect("≥1 hit").fused_score;
    assert!(
        top_fused > 0.0 && top_fused < 1.0,
        "fused_score is raw RRF, not normalized: {top_fused}"
    );

    // Config knobs reflected in the trace (guards drift vs harness PROD_ALPHA=0.3).
    assert_eq!(exp.trace.rerank_depth, depth as u32);
    assert_eq!(exp.trace.alpha, 0.3);
    assert_eq!(exp.trace.pool_n, 0, "pool_n echoes the request (0 → defaults applied downstream)");
    // ce_active iff the CE model actually scored the pool (model-dependent).
    let model_scored = exp.per_hit.iter().any(|p| p.ce_score.is_some());
    assert_eq!(exp.trace.ce_active, model_scored, "ce_active reflects whether the CE model ran");

    opened.engine.close().unwrap();
}

#[test]
fn r_obs_2_cov_byte_identity_at_depth_gt0_and_graph_arm() {
    // R-OBS-2-COV — the zero-cost / byte-stability contract on the two most
    // explain-interleaved configs the default test left uncovered: the CE-pool
    // path (rerank_depth>0) and the graph arm (use_graph_arm=true). For each,
    // search_reranked (explain=false) and search_explained (explain=true) must
    // return byte-identical results + cursor + soft_fallback.
    let (_dir, path) = fixture("exp_obs_cov");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    seed_corpus(&opened.engine);

    for (depth, graph) in [(3usize, false), (0usize, true)] {
        let plain = opened
            .engine
            .search_reranked("hybrid", None, depth, graph, 0.3, 0)
            .expect("search_reranked");
        let explained = opened
            .engine
            .search_explained("hybrid", None, depth, graph, 0.3, 0)
            .expect("search_explained");

        assert_eq!(
            explained.results, plain.results,
            "explain must not perturb results (depth={depth}, graph={graph})"
        );
        assert_eq!(explained.projection_cursor, plain.projection_cursor);
        assert_eq!(explained.soft_fallback, plain.soft_fallback);
        assert!(plain.explanation.is_none(), "explain=false suppresses the sidecar");
        assert!(explained.explanation.is_some(), "explain=true populates the sidecar");
    }

    opened.engine.close().unwrap();
}
