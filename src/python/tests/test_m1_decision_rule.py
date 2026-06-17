"""Slice 0-rev (0.8.2 / M1) — AMENDED frozen GO/NO-GO decision rule + pre-reg lint.

Pre-registration is only credible if the GO/NO-GO computation is frozen as *code*
now, so Slice 20 cannot post-hoc switch the endpoint. This module pins the full
truth table of the **amended** ``eval.m1_decision_rule.decide`` at its boundaries
(6 HITL methodology amendments; see
``dev/plans/runs/0.8.2-slice-0-prereg-methodology-review.md``), and lints that
``dev/design/0.8.2-m1-multihop-harness.md`` carries the required AMENDED frozen,
dated pre-registration fields.

The amended rule consumes already-computed summary statistics (the harness runs
the paired bootstrap; ``decide()`` stays pure/deterministic — no RNG, no I/O):

    decide(material, em, trend, confident_wrong, power_ok) -> "GO" | "NO_GO"

GO iff ALL: (1) ``material.f1_delta >= MATERIAL_F1_LIFT`` AND ``material.f1_ci_low
> 0``; (2) ``not trend.neg_significant`` (veto only on a *significantly negative*
ΔF1-vs-hop slope — flat/positive passes, NO strict-monotonic requirement);
(3) ``em.ci_high >= 0``; (4) ``not confident_wrong.increase_significant``;
(5) ``power_ok``. Non-finite floats raise ``ValueError``.

Pure stdlib only — **no ``fathomdb`` / ``scipy`` / ``networkx`` import** — so this
test runs anywhere, independent of the native-extension build or the ``.venv``
binding.
"""

from __future__ import annotations

from collections.abc import Mapping
from pathlib import Path

import pytest

from eval.m1_decision_rule import (
    MATERIAL_F1_LIFT,
    REQUIRED_FROZEN_FIELDS,
    decide,
    lint_preregistration,
)

# --------------------------------------------------------------------------- #
# Locate the frozen design doc (repo-relative; no fathomdb import).
# tests/ -> src/python -> src -> repo-root
# --------------------------------------------------------------------------- #
_REPO_ROOT = Path(__file__).resolve().parents[3]
_DESIGN_DOC = _REPO_ROOT / "dev" / "design" / "0.8.2-m1-multihop-harness.md"


# --------------------------------------------------------------------------- #
# Summary-statistic builders for the amended signature.
# --------------------------------------------------------------------------- #
def _material(f1_delta: float, f1_ci_low: float) -> dict[str, float]:
    return {"f1_delta": f1_delta, "f1_ci_low": f1_ci_low}


def _em(ci_high: float) -> dict[str, float]:
    return {"ci_high": ci_high}


def _trend(neg_significant: bool) -> dict[str, bool]:
    return {"neg_significant": neg_significant}


def _cw(increase_significant: bool) -> dict[str, bool]:
    return {"increase_significant": increase_significant}


def _decide(
    *,
    material: Mapping[str, float] | None = None,
    em: Mapping[str, float] | None = None,
    trend: Mapping[str, bool] | None = None,
    confident_wrong: Mapping[str, bool] | None = None,
    power_ok: bool = True,
) -> str:
    """Call ``decide`` from a fully-passing (GO) default set, overriding one field.

    The defaults are the canonical GO set: pooled ≥3-hop ΔF1 material with a CI
    above 0, EM not significantly worse, no significantly-negative trend, no
    confident-wrong increase, powered. Each test poisons exactly one field.
    """
    return decide(
        material=material if material is not None else _material(0.05, 0.01),
        em=em if em is not None else _em(0.02),
        trend=trend if trend is not None else _trend(False),
        confident_wrong=(
            confident_wrong if confident_wrong is not None else _cw(False)
        ),
        power_ok=power_ok,
    )


# --------------------------------------------------------------------------- #
# Constants + frozen-field set are auditable (the rule must be inspectable).
# --------------------------------------------------------------------------- #
def test_frozen_material_threshold() -> None:
    assert MATERIAL_F1_LIFT == pytest.approx(0.02)


def test_amended_required_frozen_fields() -> None:
    # The amendment replaced per-hop-strata with comparator + baseline-arms and
    # re-scoped mde-power-plan to the whole-rule power sim.
    assert set(REQUIRED_FROZEN_FIELDS) == {
        "primary-endpoint",
        "comparator",
        "decision-rule",
        "baseline-arms",
        "mde-power-plan",
    }


# --------------------------------------------------------------------------- #
# Truth table — GO only when ALL five gates hold.
# --------------------------------------------------------------------------- #
def test_all_gates_pass_is_go() -> None:
    assert _decide() == "GO"


def test_f1_delta_exactly_material_with_ci_above_zero_is_go() -> None:
    # Boundary: f1_delta == MATERIAL_F1_LIFT is material (>=, not >).
    assert _decide(material=_material(MATERIAL_F1_LIFT, 0.001)) == "GO"


def test_em_ci_high_exactly_zero_is_go() -> None:
    # Boundary: ci_high == 0 passes (EM not significantly worse; >= 0).
    assert _decide(em=_em(0.0)) == "GO"


# --- single-gate failures ---------------------------------------------------- #
def test_f1_delta_below_material_is_no_go() -> None:
    assert _decide(material=_material(MATERIAL_F1_LIFT - 0.001, 0.005)) == "NO_GO"


def test_f1_ci_low_zero_is_no_go() -> None:
    # CI lower bound must be strictly > 0 (the CI must EXCLUDE 0).
    assert _decide(material=_material(0.05, 0.0)) == "NO_GO"


def test_f1_ci_low_negative_is_no_go() -> None:
    assert _decide(material=_material(0.05, -0.01)) == "NO_GO"


def test_significantly_negative_trend_is_no_go() -> None:
    assert _decide(trend=_trend(True)) == "NO_GO"


def test_em_ci_high_negative_is_no_go() -> None:
    assert _decide(em=_em(-0.01)) == "NO_GO"


def test_confident_wrong_increase_is_no_go() -> None:
    assert _decide(confident_wrong=_cw(True)) == "NO_GO"


def test_underpowered_is_no_go() -> None:
    assert _decide(power_ok=False) == "NO_GO"


# --------------------------------------------------------------------------- #
# LOAD-BEARING regression test — the whole point of the amendment.
#
# A FLAT-POSITIVE case: per-hop ΔF1 are equal and positive (so NOT strictly
# increasing 2<3<4), the pooled >=3-hop ΔF1 is material with CI>0, EM ok, no
# confident-wrong, powered. The OLD strict-monotonic rule returned NO_GO here
# (its `f1[2] < f1[3] < f1[4]` gate fails on equal values). The amended trend
# gate vetoes ONLY on a significantly NEGATIVE slope, so a flat slope passes and
# the verdict is GO.
# --------------------------------------------------------------------------- #
def test_flat_positive_non_growing_returns_go_old_rule_said_no_go() -> None:
    verdict = _decide(
        material=_material(0.04, 0.015),  # equal positive per-hop -> pooled material
        em=_em(0.01),
        trend=_trend(False),  # flat (==), so NOT significantly negative
        confident_wrong=_cw(False),
        power_ok=True,
    )
    assert verdict == "GO"


# --------------------------------------------------------------------------- #
# Determinism + return-domain.
# --------------------------------------------------------------------------- #
def test_determinism_same_input_same_verdict() -> None:
    assert _decide() == _decide() == "GO"
    assert _decide(power_ok=False) == _decide(power_ok=False) == "NO_GO"


def test_decide_returns_only_go_or_no_go() -> None:
    assert _decide() in ("GO", "NO_GO")


# --------------------------------------------------------------------------- #
# Non-finite (NaN / ±inf) floats must fail LOUDLY (kept fix-1 guard). A NaN
# slips past every ``<`` / ``>=`` comparison and would silently return a verdict.
# --------------------------------------------------------------------------- #
def test_nan_f1_delta_with_otherwise_go_inputs_raises() -> None:
    with pytest.raises(ValueError):
        _decide(material=_material(float("nan"), 0.01))


@pytest.mark.parametrize("bad", [float("nan"), float("inf"), float("-inf")])
def test_non_finite_f1_delta_raises(bad: float) -> None:
    with pytest.raises(ValueError):
        _decide(material=_material(bad, 0.01))


@pytest.mark.parametrize("bad", [float("nan"), float("inf"), float("-inf")])
def test_non_finite_f1_ci_low_raises(bad: float) -> None:
    with pytest.raises(ValueError):
        _decide(material=_material(0.05, bad))


@pytest.mark.parametrize("bad", [float("nan"), float("inf"), float("-inf")])
def test_non_finite_em_ci_high_raises(bad: float) -> None:
    with pytest.raises(ValueError):
        _decide(em=_em(bad))


# --------------------------------------------------------------------------- #
# Pre-registration schema lint — fails RED if any frozen/dated field is missing.
# --------------------------------------------------------------------------- #
def test_design_doc_exists() -> None:
    assert _DESIGN_DOC.is_file(), f"design doc not found: {_DESIGN_DOC}"


def test_preregistration_is_complete_and_dated() -> None:
    text = _DESIGN_DOC.read_text(encoding="utf-8")
    problems = lint_preregistration(text)
    assert problems == [], f"pre-registration lint failures: {problems}"


def test_preregistration_lint_flags_missing_amended_field() -> None:
    # The amended `comparator` frozen field is load-bearing (fixed fused+rerank).
    text = _DESIGN_DOC.read_text(encoding="utf-8")
    mutated = text.replace("frozen-field: comparator", "xxx-removed-field")
    problems = lint_preregistration(mutated)
    assert any("comparator" in p for p in problems)


def test_preregistration_lint_flags_missing_baseline_arms_field() -> None:
    text = _DESIGN_DOC.read_text(encoding="utf-8")
    mutated = text.replace("frozen-field: baseline-arms", "xxx-removed-field")
    problems = lint_preregistration(mutated)
    assert any("baseline-arms" in p for p in problems)


def test_preregistration_lint_flags_undated_field() -> None:
    # A required field present but stripped of its date must be flagged.
    import re as _re

    text = _DESIGN_DOC.read_text(encoding="utf-8")
    out: list[str] = []
    for ln in text.splitlines():
        if "frozen-field: primary-endpoint" in ln:
            out.append(_re.sub(r"\b20\d\d-\d\d-\d\d\b", "REDACTED", ln))
        else:
            out.append(ln)
    problems = lint_preregistration("\n".join(out))
    assert any("primary-endpoint" in p for p in problems)


def test_preregistration_lint_requires_status_decision_ready() -> None:
    text = _DESIGN_DOC.read_text(encoding="utf-8").replace(
        "status: decision-ready", "status: draft"
    )
    problems = lint_preregistration(text)
    assert any("decision-ready" in p for p in problems)


def test_preregistration_lint_flags_missing_trend_test_field() -> None:
    # trend-test is load-bearing: it is the ONLY frozen spec for how
    # trend.neg_significant (gate 2) is computed.  The lint must flag its
    # absence so a post-hoc deletion cannot silently pass.
    text = _DESIGN_DOC.read_text(encoding="utf-8")
    mutated = text.replace("frozen-field: trend-test", "xxx-removed-field")
    problems = lint_preregistration(mutated)
    assert any("trend-test" in p for p in problems)


def test_preregistration_lint_flags_undated_trend_test_field() -> None:
    # A trend-test frozen-field line present but stripped of its date must be
    # flagged (mirrors the undated-field test for primary-endpoint).
    import re as _re

    text = _DESIGN_DOC.read_text(encoding="utf-8")
    out: list[str] = []
    for ln in text.splitlines():
        if "frozen-field: trend-test" in ln:
            out.append(_re.sub(r"\b20\d\d-\d\d-\d\d\b", "REDACTED", ln))
        else:
            out.append(ln)
    problems = lint_preregistration("\n".join(out))
    assert any("trend-test" in p for p in problems)
