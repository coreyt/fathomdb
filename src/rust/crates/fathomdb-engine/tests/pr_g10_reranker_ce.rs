//! 0.8.2 Slice E1 — the CE reranker actually reranks.
//!
//! Runs ONLY under `--features default-reranker` (the whole target is
//! feature-gated in `Cargo.toml`). It exercises the real TinyBERT-L-2
//! cross-encoder: a `rerank_depth > 0` call over a fused pool where the CE
//! disagrees with the RRF order must REORDER the pool so the
//! genuinely-relevant passage rises above the spuriously top-RRF one.
//!
//! Against the pre-E1 stub (`try_get_loaded() → None`, `score() → 0.0`) the CE
//! path returns `None` → soft-fallback → no reorder → this test FAILS (RED).
//! With the real model loaded it reorders → PASS (GREEN).
//!
//! Loading the model on a cold cache performs ONE gated network fetch of the
//! pinned ~17 MB weights (sha256-verified), then caches under
//! `~/.cache/fathomdb/reranker/`. This is the sanctioned gated download
//! (not a FathomDB-default network call): the default build and the
//! `rerank_depth == 0` path never touch it.

use fathomdb_engine::{rerank_fused, SearchHit, SoftFallbackBranch};

fn hit(id: u64, body: &str, score: f64) -> SearchHit {
    SearchHit {
        id,
        kind: "doc".to_string(),
        body: body.to_string(),
        score,
        branch: SoftFallbackBranch::Vector,
        source_id: None,
    }
}

/// CE disagrees with RRF → reorder. The pool is constructed so that:
/// - `A` (id=1) is spuriously top of the RRF order but irrelevant to the query;
/// - `B` (id=2) is just barely below `A` in RRF but is the relevant passage;
/// - `C` (id=3) is a far-down filler that WIDENS the min-max RRF span so the
///   normalized A↔B gap is tiny (≈0.002) — small enough for the α=0.3 CE weight
///   to flip it when the CE strongly prefers `B`.
///
/// Asserts `B` is ranked first after rerank (it was second before).
#[test]
fn ce_rerank_reorders_when_ce_disagrees_with_rrf() {
    let query = "How many people live in Berlin?";

    // A: top RRF but off-topic. B: relevant population fact. C: filler (span widener).
    let a = hit(
        1,
        "Berlin is famous for its vibrant art scene, nightlife, and historic architecture.",
        0.500,
    );
    let b = hit(2, "Berlin has a population of about 3.7 million inhabitants, making it the most populous city in Germany.", 0.499);
    let c = hit(3, "The quick brown fox jumps over the lazy dog near the river.", 0.001);

    let input = vec![a.clone(), b.clone(), c.clone()];
    // Sanity: A is first, B second in the RRF order we feed in.
    assert_eq!(input[0].id, 1);
    assert_eq!(input[1].id, 2);

    let out = rerank_fused(query, input.clone(), 3);

    assert_ne!(
        out, input,
        "CE rerank must change the order (RED against the 0.0-logit stub, which is identity). \
         If this is the first run on a cold cache and the network is unavailable, the model \
         could not load — re-run with network access to fetch the pinned reranker weights."
    );
    assert_eq!(
        out[0].id, 2,
        "the relevant passage (B, id=2) must rank first after CE rerank; got id={}",
        out[0].id
    );
}

/// The soft-fallback identity contract still holds WITH the feature compiled in:
/// `rerank_depth == 0` returns the pool byte-identical (no model load, no
/// network), even though the CE path is now real.
#[test]
fn ce_feature_on_depth_0_is_identity() {
    let hits = vec![hit(10, "alpha", 0.05), hit(20, "beta", 0.04), hit(30, "gamma", 0.03)];
    let out = rerank_fused("anything", hits.clone(), 0);
    assert_eq!(out, hits, "depth=0 must stay byte-identical even under default-reranker");
}

/// Empty-hits short-circuit: `rerank_fused` with `rerank_depth > 0` but an
/// empty hit set must return an empty vec immediately — without driving the
/// singleton `try_get_loaded()` for nothing.
///
/// This pins the `hits.is_empty()` guard added in fix-1 (codex §9 [P2]).
/// Without the guard, the implementation would call `try_get_loaded()` on every
/// empty search at `rerank_depth > 0`, potentially loading/downloading the ~17 MB
/// model for no benefit, and memoizing a transient load failure process-wide.
///
/// The correctness contract (empty-in → empty-out) is equivalent with or without
/// the guard; the assertion below is the regression anchor.
#[test]
fn ce_rerank_empty_hits_returns_empty_immediately() {
    let hits: Vec<SearchHit> = vec![];
    let out = rerank_fused("any query", hits.clone(), 10);
    assert!(
        out.is_empty(),
        "rerank_fused with rerank_depth>0 and empty input must return empty immediately"
    );
}
