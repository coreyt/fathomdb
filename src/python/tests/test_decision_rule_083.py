"""Slice 0 (0.8.3 / Mem0-parity) — frozen RESOLUTION decision rule + pre-reg lint.

Pre-registration is only credible if the REACHED/NOT_REACHED computation is frozen
as *code* before any number is seen, so a downstream slice (10/20/25/30) cannot
post-hoc switch the endpoint. This module pins the full truth table of
``eval.decision_rule_083.decide_083`` at its boundaries, the two $0-probe pass
criteria (15a embedder-ceiling; 15b D2 content-at-scale proxy), and lints that
``dev/design/0.8.3-mem0-parity.md`` carries the required frozen, dated
pre-registration fields.

The rule consumes already-computed summary statistics (the parity harness runs the
paired bootstrap; ``decide_083`` stays pure/deterministic — no RNG, no I/O):

    decide_083(external_per_class, eu7_recall, latency_ok) -> Resolution

REACHED iff (1) NOT blocked by a hard gate (eu7 ≥ 0.90 and latency intact),
(2) no class is under-powered (``mde ≤ ε`` per class), and (3) *every* class
clears the near-parity band (external paired-CI lower bound ``ci_lo >= -ε``).
``surpass_candidates`` = the classes whose ``ci_lo > 0`` (strictly better). All
floats must be finite — a malformed endpoint raises ``ValueError``.

Pure stdlib only — **no ``fathomdb`` / ``scipy`` / ``networkx`` import** — so this
test runs anywhere, independent of the native-extension build or the ``.venv``
binding.
"""

from __future__ import annotations

import re as _re
from pathlib import Path

import pytest

from eval.decision_rule_083 import (
    EPS_NEAR_PARITY,
    EU7_FLOOR,
    MEMORY_CLASSES,
    REQUIRED_FROZEN_FIELDS_083,
    decide_083,
    lint_preregistration_083,
    probe_15a_pass,
    probe_15b_pass,
)

# --------------------------------------------------------------------------- #
# Locate the frozen design doc (repo-relative; no fathomdb import).
# tests/ -> src/python -> src -> repo-root
# --------------------------------------------------------------------------- #
_REPO_ROOT = Path(__file__).resolve().parents[3]
_DESIGN_DOC = _REPO_ROOT / "dev" / "design" / "0.8.3-mem0-parity.md"


# --------------------------------------------------------------------------- #
# Class-delta builders for the decide_083 signature.
# --------------------------------------------------------------------------- #
def _cd(
    *,
    point: float = 0.02,
    ci_lo: float = 0.01,
    ci_hi: float = 0.05,
    mde: float = 0.04,
    n: int = 200,
) -> dict[str, float]:
    return {"point": point, "ci_lo": ci_lo, "ci_hi": ci_hi, "mde": mde, "n": n}


def _all(**overrides: dict[str, float]) -> dict[str, dict[str, float]]:
    """Build the per-class mapping, defaulting every class to a passing delta."""
    table: dict[str, dict[str, float]] = {cls: _cd() for cls in MEMORY_CLASSES}
    table.update(overrides)
    return table


# --------------------------------------------------------------------------- #
# Frozen constants are auditable (the rule must be inspectable).
# --------------------------------------------------------------------------- #
def test_frozen_eps_near_parity() -> None:
    assert EPS_NEAR_PARITY == pytest.approx(0.05)


def test_frozen_eu7_floor() -> None:
    assert EU7_FLOOR == pytest.approx(0.90)


def test_frozen_memory_classes() -> None:
    assert MEMORY_CLASSES == (
        "factoid",
        "knowledge_update",
        "multi_session",
        "temporal",
    )


def test_required_frozen_fields_set() -> None:
    assert set(REQUIRED_FROZEN_FIELDS_083) == {
        "near-parity-band",
        "power-sizing rule",
        "decision-rule",
        "$0-probe pass/fail",
        "eu7-break fork",
        "surpass-option protocol",
    }


# --------------------------------------------------------------------------- #
# Truth table — REACHED only when not blocked, not under-powered, all near-parity.
# --------------------------------------------------------------------------- #
def test_all_classes_near_parity_is_reached() -> None:
    res = decide_083(_all(), eu7_recall=0.92, latency_ok=True)
    assert res["verdict"] == "REACHED"
    assert res["blocked_by"] is None
    assert res["binding_constraint"] is None


def test_reached_lists_strictly_better_classes_as_surpass_candidates() -> None:
    # factoid strictly better (ci_lo>0); the other three near-parity but ci_lo<=0.
    table = _all(
        knowledge_update=_cd(point=-0.02, ci_lo=-0.04, ci_hi=0.0),
        multi_session=_cd(point=-0.02, ci_lo=-0.04, ci_hi=0.0),
        temporal=_cd(point=-0.02, ci_lo=-0.04, ci_hi=0.0),
    )
    res = decide_083(table, eu7_recall=0.92, latency_ok=True)
    assert res["verdict"] == "REACHED"
    assert res["surpass_candidates"] == ["factoid"]


def test_ci_lo_exactly_neg_eps_is_near_parity_reached() -> None:
    # Boundary: ci_lo == -EPS clears the band (>=, not >).
    table = _all(temporal=_cd(point=-0.04, ci_lo=-EPS_NEAR_PARITY, ci_hi=-0.02))
    res = decide_083(table, eu7_recall=0.92, latency_ok=True)
    assert res["verdict"] == "REACHED"


def test_one_class_below_band_is_not_reached() -> None:
    table = _all(temporal=_cd(point=-0.08, ci_lo=-0.10, ci_hi=-0.05))
    res = decide_083(table, eu7_recall=0.92, latency_ok=True)
    assert res["verdict"] == "NOT_REACHED"
    assert res["binding_constraint"] == "below_parity:temporal"
    assert res["blocked_by"] is None


def test_multiple_classes_below_band_names_all_in_order() -> None:
    table = _all(
        knowledge_update=_cd(ci_lo=-0.20),
        temporal=_cd(ci_lo=-0.20),
    )
    res = decide_083(table, eu7_recall=0.92, latency_ok=True)
    assert res["verdict"] == "NOT_REACHED"
    assert res["binding_constraint"] == "below_parity:knowledge_update,temporal"


# --- hard-gate BLOCK (checked first) ---------------------------------------- #
def test_eu7_below_floor_blocks() -> None:
    res = decide_083(_all(), eu7_recall=0.89, latency_ok=True)
    assert res["verdict"] == "NOT_REACHED"
    assert res["blocked_by"] == "eu7"
    assert res["binding_constraint"] is None


def test_eu7_exactly_floor_does_not_block() -> None:
    res = decide_083(_all(), eu7_recall=EU7_FLOOR, latency_ok=True)
    assert res["verdict"] == "REACHED"
    assert res["blocked_by"] is None


def test_latency_breach_blocks() -> None:
    res = decide_083(_all(), eu7_recall=0.92, latency_ok=False)
    assert res["verdict"] == "NOT_REACHED"
    assert res["blocked_by"] == "latency"


def test_eu7_block_precedes_latency_block() -> None:
    # Both fail: eu7 is reported (checked first).
    res = decide_083(_all(), eu7_recall=0.50, latency_ok=False)
    assert res["blocked_by"] == "eu7"


def test_block_precedes_below_parity() -> None:
    # A blocked run reports blocked_by, not a below_parity binding constraint.
    table = _all(temporal=_cd(ci_lo=-0.30))
    res = decide_083(table, eu7_recall=0.10, latency_ok=True)
    assert res["blocked_by"] == "eu7"
    assert res["binding_constraint"] is None


# --- power guard ------------------------------------------------------------ #
def test_underpowered_class_is_not_reached() -> None:
    table = _all(multi_session=_cd(mde=0.06))  # mde > EPS
    res = decide_083(table, eu7_recall=0.92, latency_ok=True)
    assert res["verdict"] == "NOT_REACHED"
    assert res["binding_constraint"] == "underpowered:multi_session"
    assert res["blocked_by"] is None


def test_mde_exactly_eps_is_adequately_powered() -> None:
    # Boundary: mde == EPS is powered (the guard fires on mde > EPS).
    table = _all(factoid=_cd(mde=EPS_NEAR_PARITY))
    res = decide_083(table, eu7_recall=0.92, latency_ok=True)
    assert res["verdict"] == "REACHED"


def test_power_guard_reports_first_underpowered_class_in_order() -> None:
    table = _all(
        knowledge_update=_cd(mde=0.07),
        temporal=_cd(mde=0.07),
    )
    res = decide_083(table, eu7_recall=0.92, latency_ok=True)
    assert res["binding_constraint"] == "underpowered:knowledge_update"


def test_block_precedes_power_guard() -> None:
    table = _all(factoid=_cd(mde=0.50))
    res = decide_083(table, eu7_recall=0.10, latency_ok=True)
    assert res["blocked_by"] == "eu7"
    assert res["binding_constraint"] is None


# --- result shape ----------------------------------------------------------- #
def test_result_carries_per_class_echo_with_derived_flags() -> None:
    res = decide_083(_all(), eu7_recall=0.92, latency_ok=True)
    assert set(res) == {
        "verdict",
        "per_class",
        "binding_constraint",
        "surpass_candidates",
        "blocked_by",
    }
    assert set(res["per_class"]) == set(MEMORY_CLASSES)
    pc = res["per_class"]["factoid"]
    assert pc["near_parity"] is True
    assert pc["better"] is True
    assert pc["underpowered"] is False


# --- missing keys / non-finite floats fail LOUDLY --------------------------- #
def test_missing_class_raises_key_error() -> None:
    table = _all()
    del table["temporal"]
    with pytest.raises(KeyError):
        decide_083(table, eu7_recall=0.92, latency_ok=True)


@pytest.mark.parametrize("bad", [float("nan"), float("inf"), float("-inf")])
def test_non_finite_ci_lo_raises(bad: float) -> None:
    table = _all(factoid=_cd(ci_lo=bad))
    with pytest.raises(ValueError):
        decide_083(table, eu7_recall=0.92, latency_ok=True)


@pytest.mark.parametrize("bad", [float("nan"), float("inf"), float("-inf")])
def test_non_finite_eu7_raises(bad: float) -> None:
    with pytest.raises(ValueError):
        decide_083(_all(), eu7_recall=bad, latency_ok=True)


@pytest.mark.parametrize("bad", [float("nan"), float("inf"), float("-inf")])
def test_non_finite_mde_raises(bad: float) -> None:
    table = _all(factoid=_cd(mde=bad))
    with pytest.raises(ValueError):
        decide_083(table, eu7_recall=0.92, latency_ok=True)


# --- determinism ------------------------------------------------------------ #
def test_determinism_same_input_same_result() -> None:
    a = decide_083(_all(), eu7_recall=0.92, latency_ok=True)
    b = decide_083(_all(), eu7_recall=0.92, latency_ok=True)
    assert a == b


# --------------------------------------------------------------------------- #
# Probe 15a — embedder-ceiling (PRIMARY). Clears iff beats CLS-corrected
# bge-small on BOTH eu8 and the hard subset by a CI-lower-> 0 margin, is
# CPU-feasible, AND is 1-bit-survivable (projected eu7 >= 0.90).
# --------------------------------------------------------------------------- #
def _cand_15a(**overrides: float) -> dict[str, float]:
    cand = {
        "eu8": 0.60,
        "hard": 0.50,
        "eu8_margin_ci_lo": 0.01,
        "hard_margin_ci_lo": 0.01,
        "cpu_feasible": True,
        "projected_eu7": 0.91,
    }
    cand.update(overrides)
    return cand


_BASE_15A = {"eu8": 0.571, "hard": 0.45}


def test_probe_15a_all_criteria_pass() -> None:
    assert probe_15a_pass(_cand_15a(), _BASE_15A) is True


def test_probe_15a_fails_when_eu8_margin_ci_not_above_zero() -> None:
    assert probe_15a_pass(_cand_15a(eu8_margin_ci_lo=0.0), _BASE_15A) is False


def test_probe_15a_fails_when_hard_margin_ci_not_above_zero() -> None:
    assert probe_15a_pass(_cand_15a(hard_margin_ci_lo=-0.01), _BASE_15A) is False


def test_probe_15a_point_tie_with_positive_margin_ci_passes() -> None:
    # Frozen design §5 gates 15a on the paired margin CI lower bound > 0 on BOTH
    # eu8 and the hard subset (plus cpu_feasible + 1-bit-survivable) — NOT on a raw
    # candidate-point > base comparison. A candidate whose point estimate TIES the
    # base (c_eu8 == base.eu8, c_hard == base.hard) but whose margin CI lower bounds
    # are both > 0 must CLEAR (the unregistered raw-point check is removed; the old
    # code returned False here = a rounding-induced false negative).
    cand = _cand_15a(eu8=_BASE_15A["eu8"], hard=_BASE_15A["hard"])
    assert probe_15a_pass(cand, _BASE_15A) is True


def test_probe_15a_fails_when_not_cpu_feasible() -> None:
    assert probe_15a_pass(_cand_15a(cpu_feasible=False), _BASE_15A) is False


def test_probe_15a_fails_when_not_one_bit_survivable() -> None:
    assert probe_15a_pass(_cand_15a(projected_eu7=0.89), _BASE_15A) is False


def test_probe_15a_projected_eu7_exactly_floor_survives() -> None:
    assert probe_15a_pass(_cand_15a(projected_eu7=EU7_FLOOR), _BASE_15A) is True


# --------------------------------------------------------------------------- #
# Probe 15b — D2 content-at-scale proxy. Passes iff fielded content beats the
# length-matched placebo at power (margin CI lower > 0) AND fielding removes the
# FTS length-norm penalty.
# --------------------------------------------------------------------------- #
def _enriched_15b(**overrides: float) -> dict[str, float]:
    enriched = {
        "recall": 0.55,
        "margin_ci_lo": 0.02,
        "removes_length_norm_penalty": True,
    }
    enriched.update(overrides)
    return enriched


_PLACEBO_15B = {"recall": 0.50}


def test_probe_15b_all_criteria_pass() -> None:
    assert probe_15b_pass(_enriched_15b(), _PLACEBO_15B) is True


def test_probe_15b_fails_when_margin_ci_not_above_zero() -> None:
    assert probe_15b_pass(_enriched_15b(margin_ci_lo=0.0), _PLACEBO_15B) is False


def test_probe_15b_point_tie_with_positive_margin_ci_passes() -> None:
    # Frozen design §5 gates 15b on the paired enriched-placebo margin CI lower
    # bound > 0 AND removes_length_norm_penalty — NOT on a raw enriched-point >
    # placebo-point comparison. Enriched recall that TIES the placebo point but with
    # a margin CI lower bound > 0 (and penalty removed) must PASS; the old code
    # returned False here = a rounding-induced false negative.
    enriched = _enriched_15b(recall=_PLACEBO_15B["recall"])
    assert probe_15b_pass(enriched, _PLACEBO_15B) is True


def test_probe_15b_fails_when_length_norm_penalty_not_removed() -> None:
    assert (
        probe_15b_pass(_enriched_15b(removes_length_norm_penalty=False), _PLACEBO_15B)
        is False
    )


# --------------------------------------------------------------------------- #
# Pre-registration schema lint — raises if any frozen/dated field is missing.
# --------------------------------------------------------------------------- #
def test_design_doc_exists() -> None:
    assert _DESIGN_DOC.is_file(), f"design doc not found: {_DESIGN_DOC}"


def test_prereg_083_lint_passes_on_real_design_doc() -> None:
    # No raise == clean. The doc is frozen at the Slice-0 gate: it began
    # `decision-ready` and is now HITL-`SIGNED` (a stronger freeze); the lint
    # accepts either (ACCEPTED_STATUS_TOKENS).
    lint_preregistration_083(_DESIGN_DOC.read_text(encoding="utf-8"))


def test_prereg_083_lint_flags_downgraded_status() -> None:
    # Downgrade whichever frozen status the doc currently carries (SIGNED today,
    # decision-ready historically) to a non-frozen `draft`; the lint must flag it.
    text = (
        _DESIGN_DOC.read_text(encoding="utf-8")
        .replace("status: SIGNED", "status: draft")
        .replace("status: decision-ready", "status: draft")
    )
    with pytest.raises(AssertionError):
        lint_preregistration_083(text)


def test_prereg_083_lint_accepts_either_frozen_status() -> None:
    # Non-vacuity: BOTH frozen tokens satisfy the lint — the real (SIGNED) doc and
    # the historical decision-ready form. Swapping SIGNED -> decision-ready keeps
    # the doc frozen, so the lint must still pass (proves the OR is real, not a
    # SIGNED-only accept that would reject a decision-ready pre-registration).
    real = _DESIGN_DOC.read_text(encoding="utf-8")
    lint_preregistration_083(real)  # SIGNED — clean
    decision_ready = real.replace("status: SIGNED", "status: decision-ready")
    assert "status: decision-ready" in decision_ready
    lint_preregistration_083(decision_ready)  # decision-ready — also clean


def test_prereg_083_lint_flags_missing_frozen_field() -> None:
    text = _DESIGN_DOC.read_text(encoding="utf-8").replace(
        "frozen-field: surpass-option protocol", "xxx-removed-field"
    )
    with pytest.raises(AssertionError):
        lint_preregistration_083(text)


def test_prereg_083_lint_flags_missing_decision_rule_field() -> None:
    text = _DESIGN_DOC.read_text(encoding="utf-8").replace(
        "frozen-field: decision-rule", "xxx-removed-field"
    )
    with pytest.raises(AssertionError):
        lint_preregistration_083(text)


def test_prereg_083_lint_flags_undated_field() -> None:
    # A required field present but stripped of its date must be flagged.
    text = _DESIGN_DOC.read_text(encoding="utf-8")
    out: list[str] = []
    for ln in text.splitlines():
        if "frozen-field: eu7-break fork" in ln:
            out.append(_re.sub(r"\b20\d\d-\d\d-\d\d\b", "REDACTED", ln))
        else:
            out.append(ln)
    with pytest.raises(AssertionError):
        lint_preregistration_083("\n".join(out))
