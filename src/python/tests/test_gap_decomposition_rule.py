"""Frozen gap-decomposition rule — truth table (design §3).

Pure stdlib (no fathomdb / numpy / backend): the rule is the executable
pre-registration, so this test pins the verdict for every branch BEFORE any number
is seen. Covers: each component DOMINANT, the power-safe dominance (point-largest
loses while a rival's ci_hi overlaps the leader's ci_lo), the underpowered guard,
the missing/None/non-finite-stat guard, and the fit-coverage floor.
"""

from __future__ import annotations

import math
from typing import Any

from eval.gap_decomposition_rule import (
    EPS,
    FIT_COVERAGE_MIN,
    decide_gap_decomposition,
)


def _comp(point: float, ci_lo: float, ci_hi: float, mde: float, n: int = 100) -> dict[str, Any]:
    return {"point": point, "ci_lo": ci_lo, "ci_hi": ci_hi, "mde": mde, "n": n}


def _small() -> dict[str, Any]:
    """A small, non-dominant component well inside the band."""
    return _comp(0.01, -0.02, 0.04, 0.02)


# --------------------------------------------------------------------------- #
# Each component DOMINANT (powered + beats every rival's ci_hi).
# --------------------------------------------------------------------------- #
def test_retrieval_dominant() -> None:
    comps = {
        "RETRIEVAL": _comp(0.20, 0.15, 0.25, 0.03),
        "DISTILLED_FORM": _comp(0.02, -0.01, 0.05, 0.02),
        "MEM0_RESIDUAL": _small(),
    }
    d = decide_gap_decomposition(comps, fit_coverage=1.0)
    assert d["verdict"] == "RETRIEVAL_DOMINANT"
    assert d["reason"] is None


def test_distilled_form_dominant() -> None:
    comps = {
        "RETRIEVAL": _comp(0.02, -0.01, 0.05, 0.02),
        "DISTILLED_FORM": _comp(0.20, 0.15, 0.25, 0.03),
        "MEM0_RESIDUAL": _small(),
    }
    assert decide_gap_decomposition(comps, fit_coverage=1.0)["verdict"] == "DISTILLED_FORM_DOMINANT"


def test_mem0_residual_dominant() -> None:
    comps = {
        "RETRIEVAL": _comp(0.02, -0.01, 0.05, 0.02),
        "DISTILLED_FORM": _small(),
        "MEM0_RESIDUAL": _comp(0.20, 0.15, 0.25, 0.03),
    }
    assert decide_gap_decomposition(comps, fit_coverage=1.0)["verdict"] == "MEM0_RESIDUAL_DOMINANT"


# --------------------------------------------------------------------------- #
# Power-safe dominance: point-largest is NOT enough when a rival's ci_hi
# exceeds the leader's ci_lo (codex Q3).
# --------------------------------------------------------------------------- #
def test_point_largest_but_ci_overlap_is_inconclusive() -> None:
    comps = {
        # R has the largest point (0.20) but a wide CI: its ci_lo (0.03) is below
        # F's ci_hi (0.10) → no dominance.
        "RETRIEVAL": _comp(0.20, 0.03, 0.30, 0.04),
        "DISTILLED_FORM": _comp(0.07, 0.01, 0.10, 0.02),
        "MEM0_RESIDUAL": _small(),
    }
    d = decide_gap_decomposition(comps, fit_coverage=1.0)
    assert d["verdict"] == "INCONCLUSIVE"
    assert d["reason"] == "no_dominant"


def test_underpowered_leader_is_inconclusive() -> None:
    # R point-largest + ci_lo beats rivals' ci_hi, BUT R.mde > EPS (underpowered).
    comps = {
        "RETRIEVAL": _comp(0.20, 0.15, 0.25, EPS + 0.03),
        "DISTILLED_FORM": _comp(0.02, -0.01, 0.05, 0.02),
        "MEM0_RESIDUAL": _small(),
    }
    d = decide_gap_decomposition(comps, fit_coverage=1.0)
    assert d["verdict"] == "INCONCLUSIVE"
    assert d["reason"] == "no_dominant"


def test_ci_lo_must_be_strictly_positive() -> None:
    # Leader beats rivals' ci_hi and is powered, but ci_lo == 0 (not > 0).
    comps = {
        "RETRIEVAL": _comp(0.10, 0.0, 0.20, 0.03),
        "DISTILLED_FORM": _comp(-0.05, -0.10, -0.02, 0.02),
        "MEM0_RESIDUAL": _comp(-0.06, -0.12, -0.03, 0.02),
    }
    assert decide_gap_decomposition(comps, fit_coverage=1.0)["verdict"] == "INCONCLUSIVE"


# --------------------------------------------------------------------------- #
# Missing / None / non-finite stat → INCONCLUSIVE (never raise).
# --------------------------------------------------------------------------- #
def test_none_stat_forces_inconclusive() -> None:
    comps = {
        "RETRIEVAL": {"point": 0.2, "ci_lo": None, "ci_hi": 0.25, "mde": 0.03, "n": 100},
        "DISTILLED_FORM": _small(),
        "MEM0_RESIDUAL": _small(),
    }
    d = decide_gap_decomposition(comps, fit_coverage=1.0)
    assert d["verdict"] == "INCONCLUSIVE"
    assert d["reason"] == "missing_stat:RETRIEVAL"


def test_missing_component_forces_inconclusive() -> None:
    comps = {"RETRIEVAL": _comp(0.2, 0.15, 0.25, 0.03), "DISTILLED_FORM": _small()}
    d = decide_gap_decomposition(comps, fit_coverage=1.0)
    assert d["verdict"] == "INCONCLUSIVE"
    assert d["reason"] == "missing_stat:MEM0_RESIDUAL"


def test_non_finite_stat_forces_inconclusive() -> None:
    comps = {
        "RETRIEVAL": _comp(0.2, float("nan"), 0.25, 0.03),
        "DISTILLED_FORM": _small(),
        "MEM0_RESIDUAL": _small(),
    }
    assert decide_gap_decomposition(comps, fit_coverage=1.0)["verdict"] == "INCONCLUSIVE"


def test_none_mde_from_degenerate_n_forces_inconclusive() -> None:
    # class_delta returns mde=None for n<=1; the rule must not crash or dominate.
    comps = {
        "RETRIEVAL": {"point": 0.2, "ci_lo": 0.2, "ci_hi": 0.2, "mde": None, "n": 1},
        "DISTILLED_FORM": _small(),
        "MEM0_RESIDUAL": _small(),
    }
    assert decide_gap_decomposition(comps, fit_coverage=1.0)["verdict"] == "INCONCLUSIVE"


# --------------------------------------------------------------------------- #
# Fit-coverage floor (codex round-4 BLOCK): below 0.80 → INCONCLUSIVE even when a
# component would otherwise dominate (selective exclusion changes the estimand).
# --------------------------------------------------------------------------- #
def test_fit_coverage_below_floor_forces_inconclusive() -> None:
    comps = {
        "RETRIEVAL": _comp(0.20, 0.15, 0.25, 0.03),  # would be RETRIEVAL_DOMINANT
        "DISTILLED_FORM": _comp(0.02, -0.01, 0.05, 0.02),
        "MEM0_RESIDUAL": _small(),
    }
    d = decide_gap_decomposition(comps, fit_coverage=FIT_COVERAGE_MIN - 0.01)
    assert d["verdict"] == "INCONCLUSIVE"
    assert d["reason"].startswith("fit_coverage:")


def test_fit_coverage_at_floor_allows_dominance() -> None:
    comps = {
        "RETRIEVAL": _comp(0.20, 0.15, 0.25, 0.03),
        "DISTILLED_FORM": _comp(0.02, -0.01, 0.05, 0.02),
        "MEM0_RESIDUAL": _small(),
    }
    assert decide_gap_decomposition(comps, fit_coverage=FIT_COVERAGE_MIN)["verdict"] == "RETRIEVAL_DOMINANT"


def test_non_finite_fit_coverage_forces_inconclusive() -> None:
    comps = {
        "RETRIEVAL": _comp(0.20, 0.15, 0.25, 0.03),
        "DISTILLED_FORM": _small(),
        "MEM0_RESIDUAL": _small(),
    }
    d = decide_gap_decomposition(comps, fit_coverage=math.nan)
    assert d["verdict"] == "INCONCLUSIVE"
    assert d["reason"] == "fit_coverage:none"


def test_deterministic() -> None:
    comps = {
        "RETRIEVAL": _comp(0.20, 0.15, 0.25, 0.03),
        "DISTILLED_FORM": _comp(0.02, -0.01, 0.05, 0.02),
        "MEM0_RESIDUAL": _small(),
    }
    assert decide_gap_decomposition(comps, 1.0) == decide_gap_decomposition(comps, 1.0)
