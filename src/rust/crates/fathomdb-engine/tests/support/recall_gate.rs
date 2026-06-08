//! GA-3 (0.8.0 Slice-40) — the AC-075 recall-floor gate predicate.
//!
//! The AC-075 verdict (real-embedder eu7, vector-stage recall@10) is gated by a
//! **one-sided, CI-based** test against the HITL-locked 0.90 floor, per the
//! ◆ HITL ruling 2026-06-08 (`dev/plans/runs/STATUS-0.8.0.md` § 7):
//!
//!   PASS iff `recall_ci_hi >= floor`
//!
//! i.e. the 95% bootstrap CI of the recall point estimate is **not
//! significantly below** the floor — we cannot reject "recall ≥ floor" at 95%
//! confidence. This is the "within measurement uncertainty of the floor"
//! reading that matches the HITL "rounding-error territory" acceptance of the
//! measured N=7667 result (point 0.8960, CI [0.8640, 0.9250] → ci_hi 0.925 ≥
//! 0.90 ⇒ PASS).
//!
//! **It is deliberately ONE-SIDED (`ci_hi >= floor`), NOT a two-sided
//! "floor ∈ [ci_lo, ci_hi]" test:** a two-sided membership test would wrongly
//! FAIL a comfortably-high recall whose entire CI clears the floor
//! (`ci_lo > floor`). The one-sided form passes exactly when the CI is not
//! significantly below the floor, which is the statistically-honest gate.
//!
//! **The 0.90 floor constant is UNCHANGED.** This CI form is a 0.8.0-scoped
//! reconciliation of the asserting gate to the HITL acceptance; it is to be
//! revisited after 0.8.0 (the point-estimate-≥0.90 recovery + the ~4pt
//! 0.7.x→0.8.0 vector-stage drop diagnosis are 0.8.1 items).

/// AC-075 gate predicate: the recall 95% CI is **not significantly below** the
/// floor (one-sided). PASS iff the upper CI bound clears the floor.
#[allow(dead_code)] // included via #[path] in multiple test binaries
pub fn recall_ci_clears_floor(ci_hi: f64, floor: f64) -> bool {
    ci_hi >= floor
}
