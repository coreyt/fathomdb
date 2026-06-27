//! 0.8.9 R-PG-2 — demonstrate-the-catch for the AC-075 recall-floor gate.
//!
//! Background (the masked-gate it replaces): before 0.8.0 Slice 40, the recall
//! verdict was the synthetic `perf_gates::ac_013b_recall_at_10_floor`, which
//! hard-asserted the 0.90 product floor against an *isotropic* `VaryingEmbedder`
//! (whose recall is noise-limited ~0.35–0.89 < 0.90). That gate was also
//! `AGENT_LONG`-gated, so it never ran per-push: a latent contradiction hidden
//! by never executing (the "vacuous-green" trap,
//! `perf-recall-gates-masked-and-ac013b-conflation`). Slice 40 (AC-075) made
//! the synthetic gate **report-only** and moved the asserting verdict to the
//! real-embedder eu7 path, whose pass/fail is the one-sided CI predicate
//! `recall_gate::recall_ci_clears_floor` (`tests/support/recall_gate.rs`).
//!
//! The eu7 gate itself is necessarily `AGENT_LONG` (a real BGE embed pass is
//! minutes–hours of wall-clock), so it cannot run per-push. This always-run
//! unit test pins the **catch logic** instead: it proves the predicate FAILS
//! when recall is significantly below the floor and PASSES on the
//! within-uncertainty case — so a regression that silently turns the predicate
//! into a tautology (e.g. `|_, _| true`) RED-fails here on every `cargo test`,
//! not once-per-release. This is the cheap, per-push half of the two-tier
//! recall posture documented in `dev/design/perf-gates.md`.

#[path = "support/recall_gate.rs"]
mod recall_gate;

use recall_gate::recall_ci_clears_floor;

/// The HITL-locked production floor (mirrors `AC013B_RECALL_FLOOR` /
/// `eu7_real_corpus_ac::CURRENT_FLOOR`). Kept local so this fast test has no
/// dependency on the AGENT_LONG eu7 module.
const FLOOR: f64 = 0.90;

#[test]
fn below_floor_ci_fails_the_gate() {
    // A recall whose 95% CI upper bound is below the floor must FAIL — this is
    // the regression the gate exists to catch. (e.g. the conservative 0.871
    // measurement, CI well under 0.90.)
    assert!(
        !recall_ci_clears_floor(0.88, FLOOR),
        "ci_hi=0.88 is significantly below the 0.90 floor and MUST fail the gate"
    );
    assert!(!recall_ci_clears_floor(0.50, FLOOR), "a collapsed recall MUST fail the gate");
}

#[test]
fn within_uncertainty_or_above_passes() {
    // The shipped N=7667 result: point 0.8960, CI [0.8640, 0.9250]. ci_hi 0.925
    // is not significantly below the floor → PASS (the HITL "rounding-error
    // territory" acceptance, one-sided).
    assert!(
        recall_ci_clears_floor(0.925, FLOOR),
        "ci_hi=0.925 is within measurement uncertainty of the floor → PASS"
    );
    // A comfortably-high recall whose entire CI clears the floor passes too —
    // the predicate is deliberately one-sided, not a two-sided membership test.
    assert!(recall_ci_clears_floor(0.97, FLOOR));
}

#[test]
fn exact_floor_is_a_pass() {
    // Boundary: ci_hi exactly at the floor passes (`>=`, not `>`).
    assert!(recall_ci_clears_floor(FLOOR, FLOOR));
}
