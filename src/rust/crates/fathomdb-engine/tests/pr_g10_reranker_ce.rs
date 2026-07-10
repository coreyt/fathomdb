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

use fathomdb_engine::{rerank_fused, IdSpace, SearchHit, SoftFallbackBranch};

fn hit(id: u64, body: &str, score: f64) -> SearchHit {
    SearchHit {
        // C-2 (0.8.19): typed id-space carrier; positional cursor in `write_cursor`.
        id: IdSpace::content(id.to_string()),
        write_cursor: id,
        kind: "doc".to_string(),
        body: body.to_string(),
        score,
        branch: SoftFallbackBranch::Vector,
        source_id: None,
        // 0.8.5 — additive CE score; None until set inside `ce_rerank` for in-pool hits.
        ce_score: None,
    }
}

/// 0.8.5 — the canonical Berlin reorder fixture shared by the α/pool_n tests.
/// A (id=1) is spuriously top of RRF but off-topic; B (id=2) is the relevant
/// population fact just below A; C (id=3) is a far-down span-widening filler.
fn berlin_fixture() -> (&'static str, Vec<SearchHit>) {
    let query = "How many people live in Berlin?";
    let a = hit(
        1,
        "Berlin is famous for its vibrant art scene, nightlife, and historic architecture.",
        0.500,
    );
    let b = hit(2, "Berlin has a population of about 3.7 million inhabitants, making it the most populous city in Germany.", 0.499);
    let c = hit(3, "The quick brown fox jumps over the lazy dog near the river.", 0.001);
    (query, vec![a, b, c])
}

/// min-max RRF normalization over a pool's raw scores (mirrors `ce_rerank`).
fn rrf_norm(scores: &[f64], s: f64) -> f64 {
    let lo = scores.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = hi - lo;
    if span > 0.0 {
        (s - lo) / span
    } else {
        1.0
    }
}

/// Skip-guard: returns true when the CE model produced real scores (some in-pool
/// `ce_score` is `Some`). When the model is absent (cold cache + no network) the
/// soft-fallback path leaves every `ce_score == None` — a model-dependent test
/// must then bail rather than assert a reorder it cannot get.
fn model_scored(out: &[SearchHit]) -> bool {
    out.iter().any(|h| h.ce_score.is_some())
}

// =================== 0.8.5 — α / pool_n / ce_score exposure ===================

/// REGRESSION PIN (C6 guard). The default config (`alpha=0.3`, `pool_n=rerank_depth`)
/// must reproduce the pre-slice blend EXACTLY: order `[B, A, C]` and each in-pool
/// `score == 0.3*ce_norm + 0.7*rrf_norm`. `ce_score` is the now-exposed `ce_norm`,
/// so the blend formula is checked against it directly — this locks the default α to
/// 0.3 (a drift to any other default flips this assertion).
#[test]
fn default_alpha_0_3_preserves_blend_and_order() {
    let (query, input) = berlin_fixture();
    let raw: Vec<f64> = input.iter().map(|h| h.score).collect();
    let out = rerank_fused(query, input.clone(), 3, 0.3, 3);
    if !model_scored(&out) {
        eprintln!("[SKIP] CE model unavailable — default-blend pin needs the cached reranker");
        return;
    }
    assert_eq!(
        out.iter().map(|h| h.id).collect::<Vec<_>>(),
        vec![2, 1, 3],
        "α=0.3 default order must stay [B, A, C] (C6 guard)"
    );
    for h in &out {
        let ce = h.ce_score.expect("in-pool hit must carry ce_score at depth==pool==len");
        // recover the raw score for this id from the input to compute rrf_norm.
        let original = raw[(h.id - 1) as usize];
        let expected = 0.3 * ce + 0.7 * rrf_norm(&raw, original);
        assert!(
            (h.score - expected).abs() < 1e-9,
            "id={} blended score {} != 0.3*ce({}) + 0.7*rrf; expected {}",
            h.id,
            h.score,
            ce,
            expected
        );
    }
}

/// α=1.0 → pure CE: in-pool order == sort-by-`ce_norm` desc, and each in-pool
/// `score == ce_score` exactly (the RRF term drops out at α=1.0).
#[test]
fn alpha_1_0_is_pure_ce_order() {
    let (query, input) = berlin_fixture();
    let out = rerank_fused(query, input.clone(), 3, 1.0, 3);
    if !model_scored(&out) {
        eprintln!("[SKIP] CE model unavailable — α=1.0 ordering needs the cached reranker");
        return;
    }
    // score == ce_norm exactly at α=1.0.
    for h in &out {
        let ce = h.ce_score.expect("in-pool ce_score");
        assert!((h.score - ce).abs() < 1e-12, "α=1.0: score must equal ce_norm");
    }
    // monotonically non-increasing by score (== ce_norm desc).
    for w in out.windows(2) {
        assert!(w[0].score >= w[1].score, "α=1.0 pool must be sorted by ce_norm desc");
    }
    assert_eq!(out[0].id, 2, "the population fact (B) must top a pure-CE ranking");
}

/// α=0.0 → pure RRF: the CE weight vanishes so the order is the input RRF order
/// (input is already score-descending → unchanged ids [1,2,3]). ce_score is still
/// computed (`Some`) for in-pool hits.
#[test]
fn alpha_0_0_is_pure_rrf_order() {
    let (query, input) = berlin_fixture();
    let out = rerank_fused(query, input.clone(), 3, 0.0, 3);
    if !model_scored(&out) {
        eprintln!("[SKIP] CE model unavailable — α=0.0 ordering needs the cached reranker");
        return;
    }
    assert_eq!(
        out.iter().map(|h| h.id).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "α=0.0 must reproduce the input RRF order"
    );
}

/// α out of `[0,1]` is clamped silently: α=-0.5 behaves as α=0.0, α=2.0 as α=1.0.
#[test]
fn alpha_is_clamped_to_unit_interval() {
    let (query, input) = berlin_fixture();
    let lo = rerank_fused(query, input.clone(), 3, -0.5, 3);
    let lo0 = rerank_fused(query, input.clone(), 3, 0.0, 3);
    let hi = rerank_fused(query, input.clone(), 3, 2.0, 3);
    let hi1 = rerank_fused(query, input.clone(), 3, 1.0, 3);
    assert_eq!(lo, lo0, "α=-0.5 must behave exactly as α=0.0 (clamp)");
    assert_eq!(hi, hi1, "α=2.0 must behave exactly as α=1.0 (clamp)");
}

/// pool_n clamps to `hits.len()`; pool_n < rerank_depth reranks only the top
/// `pool_n` and leaves the remainder untouched with `ce_score == None`.
#[test]
fn pool_n_clamps_and_bounds_the_reranked_pool() {
    let (query, input) = berlin_fixture();
    let raw_c = input[2].score;

    // pool_n=2 < depth=3 → only A,B reranked; C is the untouched remainder.
    let out = rerank_fused(query, input.clone(), 3, 1.0, 2);
    if !model_scored(&out) {
        eprintln!("[SKIP] CE model unavailable — pool_n bound needs the cached reranker");
        return;
    }
    assert_eq!(out.len(), 3);
    let last = out.last().unwrap();
    assert_eq!(last.id, 3, "the out-of-pool filler stays last");
    assert_eq!(last.ce_score, None, "remainder hit must keep ce_score == None");
    assert!((last.score - raw_c).abs() < 1e-12, "remainder keeps its original RRF score");
    assert!(out[0].ce_score.is_some() && out[1].ce_score.is_some(), "in-pool hits carry ce_score");

    // pool_n > len clamps to len → identical to pool_n == len.
    let wide = rerank_fused(query, input.clone(), 3, 1.0, 10);
    let exact = rerank_fused(query, input.clone(), 3, 1.0, 3);
    assert_eq!(wide, exact, "pool_n > len must clamp to len");
}

/// ce_score is `Some(ce_norm) ∈ [0,1]` for in-pool hits and `None` for the
/// remainder (D1). depth=0 is the identity gate → `ce_score == None` everywhere.
#[test]
fn ce_score_population_scope() {
    let (query, input) = berlin_fixture();

    // depth=0 → identity, no model load, ce_score None on every hit.
    let identity = rerank_fused(query, input.clone(), 0, 1.0, 3);
    assert_eq!(identity, input, "depth=0 stays byte-identical incl. ce_score == None");
    assert!(identity.iter().all(|h| h.ce_score.is_none()));

    let out = rerank_fused(query, input.clone(), 3, 1.0, 3);
    if !model_scored(&out) {
        eprintln!("[SKIP] CE model unavailable — ce_score values need the cached reranker");
        return;
    }
    for h in &out {
        let ce = h.ce_score.expect("all three hits are in-pool at depth==pool==len");
        assert!((0.0..=1.0).contains(&ce), "ce_norm must be in [0,1], got {ce}");
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

    let out = rerank_fused(query, input.clone(), 3, 0.3, 3);

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
    let out = rerank_fused("anything", hits.clone(), 0, 0.3, 0);
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
    let out = rerank_fused("any query", hits.clone(), 10, 0.3, 10);
    assert!(
        out.is_empty(),
        "rerank_fused with rerank_depth>0 and empty input must return empty immediately"
    );
}

/// 0.8.5 (codex §9 P2-1) — a non-finite α (NaN / ±inf) must NOT poison the blend.
/// `f64::clamp(NaN)` returns NaN, which would make every blended score NaN and
/// destroy the ranking. The engine falls back to the documented default α=0.3,
/// so the result is byte-identical to the α=0.3 order with all-finite scores.
#[test]
fn nonfinite_alpha_falls_back_to_default() {
    let (query, input) = berlin_fixture();
    let out_default = rerank_fused(query, input.clone(), 3, 0.3, 3);
    if !model_scored(&out_default) {
        eprintln!("[SKIP] CE model unavailable — NaN-α fallback pin needs the cached reranker");
        return;
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let out = rerank_fused(query, input.clone(), 3, bad, 3);
        assert!(
            out.iter().all(|h| h.score.is_finite()),
            "α={bad} produced a non-finite blended score"
        );
        assert_eq!(
            out.iter().map(|h| h.id).collect::<Vec<_>>(),
            out_default.iter().map(|h| h.id).collect::<Vec<_>>(),
            "α={bad} must fall back to the α=0.3 default order"
        );
    }
}
