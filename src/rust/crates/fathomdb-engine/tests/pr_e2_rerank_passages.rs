//! 0.8.2 Slice E2 — standalone rerank over an arbitrary passage list.
//!
//! Exercises the pure helper
//! `rerank_passages(query, Vec<(id, body, score)>, depth) -> Vec<(id, score)>`
//! that the `fathomdb.rerank` pyo3 binding is a thin wrapper over. Slice 5's
//! `fused_rerank` comparator needs to CE-rerank its OWN in-harness fused
//! (bm25+dense) pool — a pool the engine's `search()` never sees — so the CE
//! must be reachable over a caller-supplied passage list, not only the engine's
//! capped text pool.
//!
//! Runs ONLY under `--features default-reranker` (the whole target is gated in
//! `Cargo.toml`), mirroring `pr_g10_reranker_ce`: the reorder assertion drives
//! the real TinyBERT-L-2 cross-encoder (one gated, sha256-pinned ~17 MB fetch on
//! a cold cache, then cached). The default build skips this target; its
//! soft-fallback identity contract is covered there by `pr_g10_reranker.rs`.

use fathomdb_engine::rerank_passages;

fn passage(id: u64, body: &str, score: f64) -> (u64, String, f64) {
    (id, body.to_string(), score)
}

/// CE disagrees with the caller's input score order → reorder. Same pool shape
/// as `pr_g10_reranker_ce::ce_rerank_reorders_when_ce_disagrees_with_rrf`:
/// - `1` is spuriously top of the input order but off-topic;
/// - `2` is just below `1` but is the relevant population fact;
/// - `3` is a far-down filler that WIDENS the min-max span so the normalized
///   1↔2 gap is tiny (≈0.002) — small enough for the α=0.3 CE weight to flip it.
///
/// Asserts `2` ranks first after rerank (it was second in the input order).
#[test]
fn rerank_passages_reorders_when_ce_disagrees_with_input_order() {
    let query = "How many people live in Berlin?";

    let input = vec![
        passage(
            1,
            "Berlin is famous for its vibrant art scene, nightlife, and historic architecture.",
            0.500,
        ),
        passage(
            2,
            "Berlin has a population of about 3.7 million inhabitants, making it the most populous city in Germany.",
            0.499,
        ),
        passage(3, "The quick brown fox jumps over the lazy dog near the river.", 0.001),
    ];
    // Sanity: 1 is first, 2 second in the input order we feed in.
    assert_eq!(input[0].0, 1);
    assert_eq!(input[1].0, 2);

    let out = rerank_passages(query, input.clone(), 3);

    let in_ids: Vec<u64> = input.iter().map(|p| p.0).collect();
    let out_ids: Vec<u64> = out.iter().map(|p| p.0).collect();
    assert_ne!(
        out_ids, in_ids,
        "CE rerank must change the order (RED against the 0.0-logit stub, which is identity). \
         If this is the first run on a cold cache and the network is unavailable, the model \
         could not load — re-run with network access to fetch the pinned reranker weights."
    );
    assert_eq!(
        out[0].0, 2,
        "the relevant passage (id=2) must rank first after CE rerank; got id={}",
        out[0].0
    );
}

/// The soft-fallback identity contract holds WITH the feature compiled in:
/// `rerank_depth == 0` returns the input order with the input scores,
/// byte-identical (no model load, no network), even though the CE path is real.
#[test]
fn rerank_passages_depth_0_is_identity() {
    let input =
        vec![passage(10, "alpha", 0.05), passage(20, "beta", 0.04), passage(30, "gamma", 0.03)];
    let out = rerank_passages("anything", input.clone(), 0);
    let expected: Vec<(u64, f64)> = input.iter().map(|p| (p.0, p.2)).collect();
    assert_eq!(out, expected, "depth=0 must preserve input order AND scores byte-identical");
}

/// Empty passage list short-circuits to empty without driving the model load —
/// pins the `rerank_fused`/`ce_rerank` empty guard through the helper.
#[test]
fn rerank_passages_empty_is_identity() {
    let out = rerank_passages("any query", vec![], 10);
    assert!(out.is_empty(), "empty passage list must return empty immediately");
}
