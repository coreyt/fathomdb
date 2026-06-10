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
    RRF_K, RRF_WEIGHT_TEXT, RRF_WEIGHT_VECTOR,
};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

fn hit(id: u64, body: &str, branch: SoftFallbackBranch) -> SearchHit {
    SearchHit { id, kind: "doc".to_string(), body: body.to_string(), score: 0.0, branch }
}

#[test]
fn rrf_formula_single_branch_ranks() {
    // Vector branch only: "a" rank 1, "b" rank 2. Text branch empty. The vector
    // weight is 1.0, so the bare-rank formula still holds.
    let fused = fuse_rrf(
        vec![hit(1, "a", SoftFallbackBranch::Vector), hit(2, "b", SoftFallbackBranch::Vector)],
        Vec::new(),
    );
    assert_eq!(fused.iter().map(|h| h.body.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    assert!(
        (fused[0].score - RRF_WEIGHT_VECTOR / (RRF_K + 1.0)).abs() < 1e-12,
        "rank-1 = w_vec/(K+1)"
    );
    assert!(
        (fused[1].score - RRF_WEIGHT_VECTOR / (RRF_K + 2.0)).abs() < 1e-12,
        "rank-2 = w_vec/(K+2)"
    );
}

#[test]
fn rrf_agreement_outranks_single_branch() {
    // "agree" is rank 1 in BOTH branches => (w_vec + w_text)/(K+1); single-branch
    // hits get one weighted term only. Agreement must win — the point of fusion.
    let vector = vec![
        hit(1, "agree", SoftFallbackBranch::Vector),
        hit(2, "vonly", SoftFallbackBranch::Vector),
    ];
    let text =
        vec![hit(1, "agree", SoftFallbackBranch::Text), hit(3, "tonly", SoftFallbackBranch::Text)];
    let fused = fuse_rrf(vector, text);

    assert_eq!(fused[0].body, "agree", "both-branch hit ranks first");
    assert!(
        (fused[0].score - (RRF_WEIGHT_VECTOR + RRF_WEIGHT_TEXT) / (RRF_K + 1.0)).abs() < 1e-12,
        "agree = (w_vec + w_text)/(K+1)"
    );
    assert!(fused[0].score > fused[1].score, "agreement strictly outranks single-branch");
    // Representative of a both-branch body is the VECTOR hit (vector-first id).
    assert_eq!(fused[0].branch, SoftFallbackBranch::Vector);
    assert_eq!(fused[0].id, 1);
}

#[test]
fn rrf_text_weighted_outranks_vector_at_equal_rank() {
    // IR-C text-dominant weighting (3:1): "tonly" (text rank 2, score
    // w_text/(K+2)) now strictly outranks "vonly" (vector rank 2, w_vec/(K+2)).
    // Order: agree (both) > tonly (text) > vonly (vector). Also pins dedup-on-body.
    let vector = vec![
        hit(1, "agree", SoftFallbackBranch::Vector),
        hit(2, "vonly", SoftFallbackBranch::Vector),
    ];
    let text =
        vec![hit(1, "agree", SoftFallbackBranch::Text), hit(3, "tonly", SoftFallbackBranch::Text)];
    let fused = fuse_rrf(vector, text);

    assert_eq!(
        fused.iter().map(|h| h.body.as_str()).collect::<Vec<_>>(),
        vec!["agree", "tonly", "vonly"],
        "score desc; text weight (3:1) lifts the rank-2 text hit above the rank-2 vector hit"
    );
    assert_eq!(fused.iter().filter(|h| h.body == "agree").count(), 1, "dedup on body");
}

#[test]
fn rrf_vector_first_on_exact_score_tie() {
    // The vector-first tiebreak fires only on an EXACT score tie. Under the 3:1
    // weighting a vector rank-1 hit (w_vec/(K+1)) ties a text hit at the rank r
    // where w_text/(K+r) == w_vec/(K+1) ⇒ r = (w_text/w_vec)*(K+1) - K. Construct
    // that exact tie and assert the vector hit sorts first.
    let r = ((RRF_WEIGHT_TEXT / RRF_WEIGHT_VECTOR) * (RRF_K + 1.0) - RRF_K) as usize;
    assert!(r >= 1, "constructed tie rank must be valid");
    let mut text: Vec<SearchHit> = (1..r)
        .map(|i| hit(1000 + i as u64, &format!("filler{i}"), SoftFallbackBranch::Text))
        .collect();
    text.push(hit(2, "tie", SoftFallbackBranch::Text)); // text rank r
    let vector = vec![hit(1, "vtie", SoftFallbackBranch::Vector)]; // vector rank 1
    let fused = fuse_rrf(vector, text);

    let vpos = fused.iter().position(|h| h.body == "vtie").expect("vtie present");
    let tpos = fused.iter().position(|h| h.body == "tie").expect("tie present");
    assert!((fused[vpos].score - fused[tpos].score).abs() < 1e-12, "scores are exactly tied");
    assert!(vpos < tpos, "vector-first orders the vector hit ahead of the tied text hit");
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
                logical_id: None,
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
            logical_id: None,
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
