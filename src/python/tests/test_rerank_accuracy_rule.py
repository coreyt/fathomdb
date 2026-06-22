"""0.8.3 Slice-20 CE-rerank ACCURACY rule — RED→GREEN (design §3).

Backend-free unit tests for the frozen pure rule (:mod:`eval.rerank_accuracy_rule`):
the lever-realized truth table (PASS / FAIL-margin / underpowered ⇒ INCONCLUSIVE),
the two non-gating diagnostics, and the GO flag (PASS AND gap_to_mem0_closed ≥ 0.5).
Pure stdlib — no fathomdb / numpy import.
"""

from __future__ import annotations

import pytest

from eval.rerank_accuracy_rule import (
    EPS_MDE,
    GO_GAP_CLOSED_MIN,
    decide_rerank_accuracy,
    gap_to_mem0_closed,
    lever_realized,
    oracle_headroom_captured,
)


# --------------------------------------------------------------------------- #
# (a) lever_realized truth table.
# --------------------------------------------------------------------------- #


def test_lever_realized_truth_table() -> None:
    # PASS: powered (mde <= eps) AND margin CI lower bound > 0.
    assert (
        lever_realized({"point": 0.1, "ci_lo": 0.02, "ci_hi": 0.18, "mde": 0.04, "n": 100})
        == "PASS"
    )
    # FAIL: powered but the margin CI lower bound is <= 0 (no lift at power).
    assert (
        lever_realized({"point": 0.0, "ci_lo": -0.03, "ci_hi": 0.05, "mde": 0.04, "n": 100})
        == "FAIL"
    )
    # ci_lo == 0 exactly is NOT a lift (strict > 0).
    assert (
        lever_realized({"point": 0.05, "ci_lo": 0.0, "ci_hi": 0.1, "mde": 0.04, "n": 80})
        == "FAIL"
    )
    # INCONCLUSIVE: under-powered (mde > eps) even with a positive CI lower bound —
    # the power guard is checked FIRST, never a silent FAIL.
    assert (
        lever_realized({"point": 0.1, "ci_lo": 0.02, "ci_hi": 0.30, "mde": 0.12, "n": 12})
        == "INCONCLUSIVE"
    )
    # INCONCLUSIVE: degenerate n<=1 ⇒ mde is None.
    assert (
        lever_realized({"point": 0.1, "ci_lo": 0.1, "ci_hi": 0.1, "mde": None, "n": 1})
        == "INCONCLUSIVE"
    )
    # The eps boundary is inclusive (mde == eps is powered).
    assert (
        lever_realized({"point": 0.1, "ci_lo": 0.02, "ci_hi": 0.2, "mde": EPS_MDE, "n": 50})
        == "PASS"
    )


def test_lever_realized_rejects_non_finite() -> None:
    with pytest.raises(ValueError):
        lever_realized({"point": 0.1, "ci_lo": float("nan"), "ci_hi": 0.2, "mde": 0.01, "n": 50})


# --------------------------------------------------------------------------- #
# (a) diagnostics: gap_to_mem0_closed + oracle_headroom_captured.
# --------------------------------------------------------------------------- #


def test_gap_to_mem0_closed_diagnostic() -> None:
    # (reranked − fathomdb) / (mem0 − fathomdb) = (0.5 − 0.3)/(0.7 − 0.3) = 0.5
    assert gap_to_mem0_closed(0.5, 0.3, 0.7) == pytest.approx(0.5)
    # any None input ⇒ None (an arm absent from the reused cells).
    assert gap_to_mem0_closed(0.5, 0.3, None) is None
    assert gap_to_mem0_closed(None, 0.3, 0.7) is None
    # ~0 (mem0 == fathomdb) denominator ⇒ None (no measurable gap).
    assert gap_to_mem0_closed(0.5, 0.3, 0.3) is None


def test_oracle_headroom_captured_diagnostic() -> None:
    # (reranked − fathomdb) / (oracle_raw − fathomdb) = (0.6 − 0.4)/(0.8 − 0.4) = 0.5
    assert oracle_headroom_captured(0.6, 0.4, 0.8) == pytest.approx(0.5)
    assert oracle_headroom_captured(0.6, 0.4, None) is None
    assert oracle_headroom_captured(0.6, 0.4, 0.4) is None  # ~0 headroom


# --------------------------------------------------------------------------- #
# (a) decide_rerank_accuracy: GO needs PASS AND gap_to_mem0_closed >= 0.5.
# --------------------------------------------------------------------------- #


def _powered_pass_margin() -> dict[str, object]:
    return {"point": 0.2, "ci_lo": 0.05, "ci_hi": 0.35, "mde": 0.04, "n": 100}


def test_go_requires_pass_and_gap_closed_at_least_half() -> None:
    pass_margin = _powered_pass_margin()

    # PASS + gap closed 0.6 (>= 0.5) ⇒ GO.
    d = decide_rerank_accuracy(
        pass_margin, acc_reranked=0.6, acc_fathomdb=0.4, acc_mem0=0.733, acc_oracle_raw=0.9
    )
    assert d["lever_realized"] == "PASS"
    assert d["gap_to_mem0_closed"] == pytest.approx(0.6, abs=1e-2)
    assert d["go"] is True

    # PASS but the gap is only 0.4-closed (< 0.5) ⇒ NOT GO.
    d2 = decide_rerank_accuracy(
        pass_margin, acc_reranked=0.5, acc_fathomdb=0.4, acc_mem0=0.65, acc_oracle_raw=0.9
    )
    assert d2["lever_realized"] == "PASS"
    assert d2["gap_to_mem0_closed"] == pytest.approx(0.4, abs=1e-2)
    assert d2["go"] is False

    # FAIL margin ⇒ never GO regardless of a large gap-closed.
    fail_margin = {"point": -0.02, "ci_lo": -0.06, "ci_hi": 0.02, "mde": 0.03, "n": 100}
    d3 = decide_rerank_accuracy(
        fail_margin, acc_reranked=0.9, acc_fathomdb=0.4, acc_mem0=0.5, acc_oracle_raw=0.95
    )
    assert d3["lever_realized"] == "FAIL"
    assert d3["go"] is False

    # PASS but mem0 cell absent (gap_to_mem0_closed None) ⇒ NOT GO (never assumed).
    d4 = decide_rerank_accuracy(
        pass_margin, acc_reranked=0.6, acc_fathomdb=0.4, acc_mem0=None, acc_oracle_raw=0.9
    )
    assert d4["lever_realized"] == "PASS"
    assert d4["gap_to_mem0_closed"] is None
    assert d4["go"] is False
    # The oracle diagnostic is still reported (non-gating).
    assert d4["oracle_headroom_captured"] == pytest.approx(0.4, abs=1e-9)


def test_go_threshold_boundary_is_inclusive() -> None:
    # gap_to_mem0_closed == GO_GAP_CLOSED_MIN exactly ⇒ GO (>= boundary).
    d = decide_rerank_accuracy(
        _powered_pass_margin(),
        acc_reranked=0.4 + GO_GAP_CLOSED_MIN * 0.4,  # closes exactly half of a 0.4 gap
        acc_fathomdb=0.4,
        acc_mem0=0.8,
        acc_oracle_raw=0.9,
    )
    assert d["gap_to_mem0_closed"] == pytest.approx(GO_GAP_CLOSED_MIN)
    assert d["go"] is True
