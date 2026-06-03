//! Slice 10 / G9 — Reciprocal Rank Fusion as the unconditional new ranking.
//!
//! Pins the RRF contract: `Σ 1/(RRF_K + rank)` keyed on `SearchHit.body`,
//! agreement (a body in both branches) outranks single-branch hits,
//! vector-first tiebreak, dedup-on-body, and a `rerank_fused` identity stub.
//! There is **no** legacy-union-ordering reproduction (HITL Q3 — no knob); the
//! pinned property is **determinism**, not legacy reproducibility.
//!
//! The formula/tiebreak/dedup are unit-tested directly on the pure `fuse_rrf`
//! function (no embedder, fully deterministic); a second e2e test asserts that
//! repeated `Engine::search` calls return byte-identical order + scores, and
//! that the vector-empty `soft_fallback` signal is computed BEFORE the branches
//! collapse into the fused list. No mocking of the database.

use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    fuse_rrf, rerank_fused, Engine, PreparedWrite, SearchHit, SoftFallback, SoftFallbackBranch,
    RRF_K,
};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

fn hit(id: u64, body: &str, branch: SoftFallbackBranch) -> SearchHit {
    SearchHit { id, kind: "doc".to_string(), body: body.to_string(), score: 0.0, branch }
}

#[test]
fn rrf_formula_single_branch_ranks() {
    // Vector branch only: "a" rank 1, "b" rank 2. Text branch empty.
    let fused = fuse_rrf(
        vec![hit(1, "a", SoftFallbackBranch::Vector), hit(2, "b", SoftFallbackBranch::Vector)],
        Vec::new(),
    );
    assert_eq!(fused.iter().map(|h| h.body.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    assert!((fused[0].score - 1.0 / (RRF_K + 1.0)).abs() < 1e-12, "rank-1 = 1/(K+1)");
    assert!((fused[1].score - 1.0 / (RRF_K + 2.0)).abs() < 1e-12, "rank-2 = 1/(K+2)");
}

#[test]
fn rrf_agreement_outranks_single_branch() {
    // "agree" is rank 1 in BOTH branches => 2/(K+1); single-branch hits get
    // one term only. Agreement must win — the entire point of fusion.
    let vector = vec![
        hit(1, "agree", SoftFallbackBranch::Vector),
        hit(2, "vonly", SoftFallbackBranch::Vector),
    ];
    let text =
        vec![hit(1, "agree", SoftFallbackBranch::Text), hit(3, "tonly", SoftFallbackBranch::Text)];
    let fused = fuse_rrf(vector, text);

    assert_eq!(fused[0].body, "agree", "both-branch hit ranks first");
    assert!((fused[0].score - 2.0 / (RRF_K + 1.0)).abs() < 1e-12, "agree = 2/(K+1)");
    assert!(fused[0].score > fused[1].score, "agreement strictly outranks single-branch");
    // Representative of a both-branch body is the VECTOR hit (vector-first id).
    assert_eq!(fused[0].branch, SoftFallbackBranch::Vector);
    assert_eq!(fused[0].id, 1);
}

#[test]
fn rrf_vector_first_tiebreak_and_dedup_on_body() {
    // "vonly" (vector rank 2) and "tonly" (text rank 2) have equal RRF score
    // 1/(K+2): vector-first tiebreak puts vonly before tonly.
    let vector = vec![
        hit(1, "agree", SoftFallbackBranch::Vector),
        hit(2, "vonly", SoftFallbackBranch::Vector),
    ];
    let text =
        vec![hit(1, "agree", SoftFallbackBranch::Text), hit(3, "tonly", SoftFallbackBranch::Text)];
    let fused = fuse_rrf(vector, text);

    assert_eq!(
        fused.iter().map(|h| h.body.as_str()).collect::<Vec<_>>(),
        vec!["agree", "vonly", "tonly"],
        "score desc, then vector-first on the equal-score tail"
    );
    assert_eq!(fused.iter().filter(|h| h.body == "agree").count(), 1, "dedup on body");
}

#[test]
fn rerank_fused_is_identity_stub() {
    // The G9 rerank seam is present but returns its input unchanged for now.
    let hits = vec![hit(1, "a", SoftFallbackBranch::Vector), hit(2, "b", SoftFallbackBranch::Text)];
    assert_eq!(rerank_fused(hits.clone()), hits);
}

/// Deterministic embedder so the e2e ordering is a pure function of the corpus.
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

#[test]
fn rrf_end_to_end_order_is_deterministic() {
    let (_dir, path) = fixture("g9_determinism");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
    for body in ["hybrid retrieval alpha", "hybrid retrieval beta", "hybrid retrieval gamma"] {
        opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: body.to_string(),
                source_id: None,
            }])
            .expect("write");
    }
    opened.engine.drain(10_000).expect("drain");

    let first = opened.engine.search("hybrid").expect("search");
    assert!(!first.results.is_empty(), "expected fused hits");
    // Repeated identical searches must produce byte-identical order + scores.
    for _ in 0..5 {
        let again = opened.engine.search("hybrid").expect("search");
        assert_eq!(again, first, "RRF fused order + scores must be deterministic");
    }
    // Every fused score is finite and the list is sorted descending.
    for w in first.results.windows(2) {
        assert!(w[0].score >= w[1].score, "fused list sorted by score desc");
    }
    opened.engine.close().unwrap();
}

#[test]
fn vector_empty_soft_fallback_signal_survives_fusion() {
    // Vector branch empty (projection frozen) but the text branch matches a
    // vector-kind row: the soft-fallback signal is computed BEFORE the branches
    // collapse, so fusion must not erase it.
    let (_dir, path) = fixture("g9_soft_fallback");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
    opened.engine.set_projection_scheduler_frozen_for_test(true);

    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "phase nine hybrid search".to_string(),
            source_id: None,
        }])
        .expect("write");

    // The FTS `search_index` is written synchronously in `commit_batch`; only
    // the vector projection is frozen. So the text branch matches immediately
    // while the vector branch stays empty.
    let result = opened.engine.search("hybrid").expect("search");
    assert_eq!(
        result.soft_fallback,
        Some(SoftFallback { branch: SoftFallbackBranch::Vector }),
        "vector-empty signal must survive the fusion collapse"
    );
    opened.engine.close().unwrap();
}
