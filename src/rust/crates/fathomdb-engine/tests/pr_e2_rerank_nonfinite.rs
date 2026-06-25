//! 0.8.2 Slice E2 fix-1 [P2] — rerank_passages must reject non-finite scores.
//!
//! Codex §9 finding: `fathomdb.rerank` accepted NaN/±inf → blended NaN scores
//! and unstable sort order. The fix: `rerank_passages` returns `Err(String)` when
//! any input passage carries a non-finite score, mirroring the malformed-passage
//! loud-fail contract.
//!
//! This target has NO `required-features` gate because the is_finite guard fires
//! before the CE model path (depth is irrelevant; the check precedes the
//! `rerank_fused` call regardless of feature state).

use fathomdb_engine::rerank_passages;

/// NaN score must be rejected before reaching the normalization/sort.
#[test]
fn rerank_passages_rejects_nan_score() {
    let passages = vec![(1u64, "body text".to_string(), f64::NAN)];
    let result = rerank_passages("query", passages, 3, 0.3, 3);
    assert!(
        result.is_err(),
        "NaN score must return Err — got Ok (is_finite guard not yet in place)"
    );
    let msg = result.unwrap_err();
    assert!(msg.contains("non-finite"), "error message must mention 'non-finite'; got: {msg:?}");
}

/// +Inf score must be rejected.
#[test]
fn rerank_passages_rejects_pos_inf_score() {
    let passages = vec![(2u64, "another passage".to_string(), f64::INFINITY)];
    let result = rerank_passages("query", passages, 1, 0.3, 1);
    assert!(
        result.is_err(),
        "+Inf score must return Err — got Ok (is_finite guard not yet in place)"
    );
}

/// −Inf score must be rejected.
#[test]
fn rerank_passages_rejects_neg_inf_score() {
    let passages = vec![(3u64, "yet another passage".to_string(), f64::NEG_INFINITY)];
    let result = rerank_passages("query", passages, 1, 0.3, 1);
    assert!(
        result.is_err(),
        "-Inf score must return Err — got Ok (is_finite guard not yet in place)"
    );
}

/// Non-finite in a mixed pool (first passage is finite, second is NaN) — still rejected.
#[test]
fn rerank_passages_rejects_nonfinite_in_mixed_pool() {
    let passages = vec![
        (10u64, "fine passage".to_string(), 0.9_f64),
        (11u64, "bad passage".to_string(), f64::NAN),
    ];
    let result = rerank_passages("query", passages, 2, 0.3, 2);
    assert!(result.is_err(), "NaN in a mixed pool must reject the whole call");
}

/// All-finite scores are still accepted (guard must not fire on valid input).
#[test]
fn rerank_passages_accepts_finite_scores() {
    let passages =
        vec![(20u64, "alpha".to_string(), 0.8_f64), (21u64, "beta".to_string(), 0.5_f64)];
    // depth=0 → identity path; no model load.
    let result = rerank_passages("query", passages, 0, 0.3, 0);
    assert!(result.is_ok(), "finite scores must not be rejected; got: {:?}", result.err());
}
