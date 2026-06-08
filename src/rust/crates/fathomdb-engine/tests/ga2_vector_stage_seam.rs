//! GA-2 / Slice-40 (◆ B-1) — the test-only vector-stage measurement seam.
//!
//! `set_vector_stage_only_for_test(true)` makes `Engine::search` return the
//! pre-fusion VECTOR-branch ranking (bit-KNN K=192 + f32 rerank) instead of the
//! unconditional RRF-fused (vector ⊕ FTS5) result, so the AC-075 recall gate can
//! measure ANN-quantization FIDELITY (vector top-10 vs exact-f32 vector top-10)
//! in isolation — NOT the hybrid `search()` output.
//!
//! Pins: (1) the seam is OFF by default (production search() unchanged); (2) with
//! the seam ON, a text-only-matching body (a non-vector-kind row, present only in
//! the FTS branch) that appears in the fused result is ABSENT from the
//! vector-stage result, and every vector-stage hit carries the Vector branch.
//! This is the load-bearing demonstration that the eu7 repoint changes the SUT.
//! No mocking of the database.

#[path = "support/recall_gate.rs"]
mod recall_gate;

use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite, SoftFallbackBranch};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

/// Deterministic embedder so the ordering is a pure function of the corpus.
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

fn seed(engine: &Engine, kind: &str, body: &str) {
    engine
        .write(&[PreparedWrite::Node {
            kind: kind.to_string(),
            body: body.to_string(),
            source_id: None,
            logical_id: None,
        }])
        .expect("write");
}

#[test]
fn vector_stage_seam_returns_pre_fusion_vector_branch() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("ga2_vector_stage{SQLITE_SUFFIX}"));
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    let engine = &opened.engine;
    // Only "doc" is a vector kind; "memo" is FTS-indexed but NOT vector-indexed,
    // so it can only surface via the text (FTS5) branch of the fusion.
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    seed(engine, "doc", "hybrid retrieval alpha");
    seed(engine, "doc", "hybrid retrieval beta");
    seed(engine, "memo", "hybrid retrieval memo text only");
    engine.drain(10_000).expect("drain");

    // Default (seam OFF) = production RRF-fused search(): the text-only "memo"
    // body is fused in via the FTS branch.
    let fused = engine.search("hybrid").expect("fused search");
    let fused_bodies: Vec<&str> = fused.results.iter().map(|h| h.body.as_str()).collect();
    assert!(
        fused_bodies.iter().any(|b| b.contains("memo text only")),
        "production fused search() must surface the text-only memo via the FTS branch; got {fused_bodies:?}"
    );

    // Seam ON = pre-fusion VECTOR branch only: the text-only "memo" is absent and
    // every hit is from the Vector branch.
    engine.set_vector_stage_only_for_test(true);
    let vector_stage = engine.search("hybrid").expect("vector-stage search");
    let vec_bodies: Vec<&str> = vector_stage.results.iter().map(|h| h.body.as_str()).collect();
    assert!(
        !vec_bodies.iter().any(|b| b.contains("memo text only")),
        "vector-stage SUT must NOT contain the text-only memo (it is not in the vector branch); got {vec_bodies:?}"
    );
    assert!(
        !vector_stage.results.is_empty(),
        "vector branch should return the two vector-kind docs"
    );
    for h in &vector_stage.results {
        assert_eq!(
            h.branch,
            SoftFallbackBranch::Vector,
            "every vector-stage hit must originate from the vector branch"
        );
    }

    // Seam OFF again = production behavior restored (no sticky state).
    engine.set_vector_stage_only_for_test(false);
    let fused_again = engine.search("hybrid").expect("fused search again");
    assert_eq!(
        fused_again, fused,
        "disabling the seam restores byte-identical production fused output"
    );

    engine.close().unwrap();
}

/// GA-3 (0.8.0 Slice-40) — the reconciled AC-075 recall gate is a ONE-SIDED,
/// CI-based test against the UNCHANGED 0.90 floor (`recall_ci_hi >= floor`),
/// per the ◆ HITL ruling 2026-06-08. This unit-tests the gate predicate
/// (`support/recall_gate.rs`) — the same function eu7's verdict loop asserts —
/// WITHOUT the ~2.5 h real-embedder run.
#[test]
fn ga3_recall_ci_gate_passes_recorded_real_result_and_bites_below_floor() {
    const FLOOR: f64 = 0.90;

    // PASSES for the recorded perf-canonical eu7 real result: point estimate
    // 0.8960, CI [0.8640, 0.9250]. The point estimate is BELOW 0.90 (the old
    // point-estimate assert PANICKED here), but ci_hi 0.925 ≥ 0.90, so the
    // recall CI is not significantly below the floor ⇒ the gate PASSES.
    let recorded_mean = 0.896;
    let recorded_ci_hi = 0.925;
    assert!(
        recorded_mean < FLOOR,
        "guard: the recorded point estimate {recorded_mean} is below the {FLOOR} floor \
         (this is exactly why the old point-estimate assert panicked)"
    );
    assert!(
        recall_gate::recall_ci_clears_floor(recorded_ci_hi, FLOOR),
        "GA-3 gate must PASS for the recorded real result (mean={recorded_mean}, \
         ci_hi={recorded_ci_hi}): ci_hi >= {FLOOR} ⇒ CI not significantly below the floor"
    );

    // STILL FAILS (bites) for a CI entirely below the floor: a genuine
    // regression where even the upper CI bound is short of 0.90.
    let regressed_mean = 0.88;
    let regressed_ci_hi = 0.89;
    assert!(
        !recall_gate::recall_ci_clears_floor(regressed_ci_hi, FLOOR),
        "GA-3 gate must FAIL (bite) for a CI entirely below the floor \
         (mean={regressed_mean}, ci_hi={regressed_ci_hi}): ci_hi < {FLOOR}"
    );

    // The floor boundary: ci_hi exactly at the floor PASSES (>= is inclusive).
    assert!(recall_gate::recall_ci_clears_floor(0.90, FLOOR));
    // ...and the smallest miss below it FAILS.
    assert!(!recall_gate::recall_ci_clears_floor(0.8999, FLOOR));

    // The one-sided gate must NOT degenerate into a two-sided "floor within
    // [ci_lo, ci_hi]" test: a comfortably-high recall whose entire CI clears
    // the floor (ci_lo > floor) must still PASS, not be wrongly failed.
    let high_ci_hi = 0.957; // the 0.7.x anchor's ci_hi (ci_lo 0.913 > floor)
    assert!(
        recall_gate::recall_ci_clears_floor(high_ci_hi, FLOOR),
        "one-sided gate must PASS a comfortably-high recall (ci_hi={high_ci_hi}); a \
         two-sided floor-within-CI test would wrongly fail it"
    );
}
