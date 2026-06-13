//! 0.8.1 Slice 10 / R1 — cross-encoder reranker unit tests.
//!
//! Tests the new `rerank_fused(query, hits, depth)` signature, its
//! soft-fallback contract (`depth=0` or model absent → byte-identical identity),
//! determinism, and the recall-gate schema pin.
//!
//! All tests in this file run in the **default build** (no `default-reranker`
//! feature). The CE inference path is feature-gated; these tests cover the
//! soft-fallback path which must work without the model.

use fathomdb_engine::{rerank_fused, SearchHit, SoftFallbackBranch};

fn hit(id: u64, body: &str, score: f64) -> SearchHit {
    SearchHit {
        id,
        kind: "doc".to_string(),
        body: body.to_string(),
        score,
        branch: SoftFallbackBranch::Vector,
    }
}

/// Soft-fallback: `rerank_depth=0` must return hits in exactly the same order
/// with the same scores — byte-identical to the pre-Slice-10 identity stub.
#[test]
fn rerank_fused_soft_fallback_preserves_fused_order() {
    let hits = vec![hit(1, "alpha", 0.05), hit(2, "beta", 0.04), hit(3, "gamma", 0.03)];
    let out = rerank_fused("what is the meaning of life", hits.clone(), 0);
    assert_eq!(out, hits, "rerank_depth=0 must return hits unchanged (byte-identical)");
}

/// Determinism: two consecutive `rerank_fused` calls with the same inputs must
/// produce byte-identical output. In the default build this is trivially true
/// (identity), but the invariant is pinned here so any future CE path must also
/// satisfy it.
///
/// Note: when `default-reranker` feature is enabled and the model is loaded,
/// determinism is guaranteed by fixed model weights + deterministic tokenization
/// (see design memo Decision 8). This test verifies the soft-fallback path.
#[test]
fn rerank_fused_deterministic() {
    let hits = vec![
        hit(10, "document one", 0.033_333),
        hit(20, "document two", 0.025_000),
        hit(30, "document three", 0.016_667),
    ];
    let first = rerank_fused("document query", hits.clone(), 0);
    let second = rerank_fused("document query", hits.clone(), 0);
    assert_eq!(first, second, "repeated rerank_fused calls must be byte-identical");
}

/// `rerank_depth=0` under the new signature is byte-identical to the old
/// identity-stub behavior. This replaces `rerank_fused_is_identity_stub` in
/// `pr_g9_rrf_fusion.rs` as the canonical expression of the soft-fallback
/// contract under the new 3-argument signature.
///
/// The corresponding test in `pr_g9_rrf_fusion.rs` has been adapted to use
/// `depth=0` (RED-3). Both tests pin the same invariant from different test files.
#[test]
fn rerank_depth_0_is_byte_identical_to_identity_stub() {
    // Mirror of the old `rerank_fused_is_identity_stub` exact input.
    let hits = vec![
        SearchHit {
            id: 1,
            kind: "doc".to_string(),
            body: "a".to_string(),
            score: 0.0,
            branch: SoftFallbackBranch::Vector,
        },
        SearchHit {
            id: 2,
            kind: "doc".to_string(),
            body: "b".to_string(),
            score: 0.0,
            branch: SoftFallbackBranch::Text,
        },
    ];
    let out = rerank_fused("", hits.clone(), 0);
    assert_eq!(out, hits, "depth=0 returns input unchanged — identical to old identity stub");
}

/// Factoid no-regress gate: the CDF artifact must contain an entry for
/// `arm=rrf_fused, query_class=exact_fact, k=200` with `found_at_k >= 0.9695`.
///
/// This is a **schema/artifact-existence gate**, not a live corpus run. The live
/// factoid R@10 >= 0.90 measurement is report-only (AGENT_LONG-gated via the
/// `ir_c_cdf_run.rs` harness). This test pins the Slice 5 CDF artifact so a
/// regression is detectable at the structural level in every push.
///
/// The pin value 0.9695 is the oracle pool measured at K=200 for `rrf_fused`
/// (`IR-C-recall-cdf.json`, generated 2026-06-13).
#[test]
fn reranker_recall_gate_schema_pinned() {
    let artifact_path =
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../dev/plans/runs/IR-C-recall-cdf.json");
    let content = std::fs::read_to_string(artifact_path)
        .unwrap_or_else(|e| panic!("CDF artifact not found at {artifact_path}: {e}"));
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("CDF artifact is not valid JSON");

    let recall_cdf = json
        .get("recall_cdf")
        .and_then(|v| v.as_array())
        .expect("CDF artifact must have a 'recall_cdf' array");

    // Find the rrf_fused / exact_fact / k=200 entry.
    let entry = recall_cdf.iter().find(|e| {
        e.get("arm").and_then(|v| v.as_str()) == Some("rrf_fused")
            && e.get("query_class").and_then(|v| v.as_str()) == Some("exact_fact")
            && e.get("k").and_then(|v| v.as_u64()) == Some(200)
    });

    let entry = entry
        .expect("CDF artifact must contain entry: arm=rrf_fused, query_class=exact_fact, k=200");

    let found_at_k = entry
        .get("found_at_k")
        .and_then(|v| v.as_f64())
        .expect("CDF entry must have numeric 'found_at_k'");

    assert!(
        found_at_k >= 0.9695,
        "rrf_fused exact_fact found@200 must be >= 0.9695 (oracle pool gate); got {found_at_k}"
    );
}
