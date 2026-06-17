"""Slice 0 (0.8.2 / M1) — frozen GO/NO-GO decision rule + pre-registration lint.

Pre-registration is only credible if the GO/NO-GO computation is frozen as *code*
now, so Slice 20 cannot post-hoc switch the endpoint. This module pins the full
truth table of ``eval.m1_decision_rule.decide`` at its boundaries, and lints that
``dev/design/0.8.2-m1-multihop-harness.md`` carries the required frozen, dated
pre-registration fields.

Pure stdlib only — **no ``fathomdb`` / ``scipy`` / ``networkx`` import** — so this
test runs anywhere, independent of the native-extension build or the ``.venv``
binding.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from eval.m1_decision_rule import (
    EM_MIN_LIFT_3PLUS,
    MATERIAL_F1_LIFT,
    REQUIRED_HOPS,
    decide,
    lint_preregistration,
)

# --------------------------------------------------------------------------- #
# Locate the frozen design doc (repo-relative; no fathomdb import).
# tests/ -> src/python -> src -> repo-root
# --------------------------------------------------------------------------- #
_REPO_ROOT = Path(__file__).resolve().parents[3]
_DESIGN_DOC = _REPO_ROOT / "dev" / "design" / "0.8.2-m1-multihop-harness.md"


def _deltas(
    em2: float, f12: float, em3: float, f13: float, em4: float, f14: float
) -> dict[int, dict[str, float]]:
    """Build a ``deltas_by_hop`` map: hop -> {'em': ΔEM, 'f1': ΔF1}."""
    return {
        2: {"em": em2, "f1": f12},
        3: {"em": em3, "f1": f13},
        4: {"em": em4, "f1": f14},
    }


# --------------------------------------------------------------------------- #
# Constants are frozen (the rule must be auditable).
# --------------------------------------------------------------------------- #
def test_frozen_constants() -> None:
    assert REQUIRED_HOPS == (2, 3, 4)
    assert MATERIAL_F1_LIFT == pytest.approx(0.02)
    assert EM_MIN_LIFT_3PLUS == pytest.approx(0.0)


# --------------------------------------------------------------------------- #
# Truth table — pin every row from the plan §4 / design §4.1.
# --------------------------------------------------------------------------- #
def test_flat_lift_on_3plus_is_no_go() -> None:
    # ≥3-hop F1 lift is flat (zero) → NO_GO, regardless of power.
    d = _deltas(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
    assert decide(d, power_ok=True) == "NO_GO"


def test_negative_lift_on_3plus_is_no_go() -> None:
    # ≥3-hop F1 lift is negative (graph hurts) → NO_GO.
    d = _deltas(0.0, -0.01, -0.02, -0.03, -0.03, -0.05)
    assert decide(d, power_ok=True) == "NO_GO"


def test_positive_below_material_on_3plus_is_no_go() -> None:
    # Positive but below the material threshold on the ≥3-hop strata → NO_GO.
    tiny = MATERIAL_F1_LIFT / 2.0
    d = _deltas(0.0, 0.0, 0.01, tiny, 0.01, tiny)
    assert decide(d, power_ok=True) == "NO_GO"


def test_positive_material_but_flat_dose_is_no_go() -> None:
    # Material positive ≥3-hop lift but NOT dose-responsive (flat across hops) → NO_GO.
    d = _deltas(0.05, 0.05, 0.05, 0.05, 0.05, 0.05)
    assert decide(d, power_ok=True) == "NO_GO"


def test_positive_material_but_decreasing_dose_is_no_go() -> None:
    # Material positive but the lift SHRINKS with hops (anti-dose-response) → NO_GO.
    d = _deltas(0.05, 0.09, 0.05, 0.06, 0.05, 0.03)
    assert decide(d, power_ok=True) == "NO_GO"


def test_dose_responsive_but_underpowered_is_no_go() -> None:
    # Dose-responsive, material ≥3-hop lift, EM non-regressing, but power_ok=False → NO_GO.
    d = _deltas(0.01, 0.02, 0.03, 0.05, 0.05, 0.09)
    assert decide(d, power_ok=False) == "NO_GO"


def test_dose_responsive_and_powered_is_go() -> None:
    # The ONLY GO row: dose-responsive (2<3<4), material on hop-3 & hop-4,
    # EM non-regressing on ≥3-hop, and adequately powered.
    d = _deltas(0.01, 0.02, 0.03, 0.05, 0.05, 0.09)
    assert decide(d, power_ok=True) == "GO"


def test_dose_responsive_powered_but_em_regresses_on_3plus_is_no_go() -> None:
    # Confident-wrong guard: F1 dose-responsive + powered, but ΔEM negative on a
    # ≥3-hop stratum → NO_GO.
    d = _deltas(0.01, 0.02, -0.01, 0.05, 0.02, 0.09)
    assert decide(d, power_ok=True) == "NO_GO"


def test_material_lift_must_hold_on_both_hop3_and_hop4() -> None:
    # Hop-3 material but hop-4 below material → NO_GO (the ≥3-hop region must ALL clear).
    sub = MATERIAL_F1_LIFT / 2.0
    d = _deltas(0.0, 0.01, 0.02, MATERIAL_F1_LIFT + 0.01, 0.02, sub)
    assert decide(d, power_ok=True) == "NO_GO"


def test_determinism_same_input_same_verdict() -> None:
    d = _deltas(0.01, 0.02, 0.03, 0.05, 0.05, 0.09)
    first = decide(d, power_ok=True)
    second = decide(d, power_ok=True)
    assert first == second == "GO"
    neg = _deltas(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
    assert decide(neg, power_ok=True) == decide(neg, power_ok=True) == "NO_GO"


def test_decide_returns_only_go_or_no_go() -> None:
    d = _deltas(0.01, 0.02, 0.03, 0.05, 0.05, 0.09)
    assert decide(d, power_ok=True) in ("GO", "NO_GO")


def test_missing_hop_raises() -> None:
    # A malformed deltas map (missing a required hop) must fail loudly, not
    # silently return a verdict.
    bad = {2: {"em": 0.0, "f1": 0.0}, 3: {"em": 0.0, "f1": 0.05}}
    with pytest.raises((KeyError, ValueError)):
        decide(bad, power_ok=True)  # type: ignore[arg-type]


# --------------------------------------------------------------------------- #
# Non-finite (NaN / ±inf) endpoints must fail LOUDLY, not silently return a
# verdict. A NaN slips past every ``<`` comparison (``nan < x`` is False), so
# without an explicit isfinite guard ``decide`` would return GO on malformed
# data — see codex §9 [P2]. The exact codex repro is pinned below.
# --------------------------------------------------------------------------- #
def test_nan_em_at_hop3_with_otherwise_go_inputs_raises() -> None:
    # Codex [P2] repro: otherwise-GO inputs, but ΔEM at hop-3 is NaN. The
    # confident-wrong guard (nan < 0.0 is False) would silently pass and the
    # rule would return GO; a non-finite endpoint must fail loudly instead.
    nan = float("nan")
    d = _deltas(0.01, 0.02, nan, 0.05, 0.05, 0.09)
    with pytest.raises(ValueError):
        decide(d, power_ok=True)


@pytest.mark.parametrize("bad", [float("nan"), float("inf"), float("-inf")])
@pytest.mark.parametrize("metric", ["em", "f1"])
@pytest.mark.parametrize("hop", [2, 3, 4])
def test_non_finite_metric_at_any_required_hop_raises(
    bad: float, metric: str, hop: int
) -> None:
    # Any non-finite EM or F1 at any required hop must raise loudly, regardless
    # of which gate it would otherwise reach. Start from the GO row and poison
    # one cell.
    d = _deltas(0.01, 0.02, 0.03, 0.05, 0.05, 0.09)
    d[hop][metric] = bad
    with pytest.raises(ValueError):
        decide(d, power_ok=True)


# --------------------------------------------------------------------------- #
# Pre-registration schema lint — fails RED if any frozen/dated field is missing.
# --------------------------------------------------------------------------- #
def test_design_doc_exists() -> None:
    assert _DESIGN_DOC.is_file(), f"design doc not found: {_DESIGN_DOC}"


def test_preregistration_is_complete_and_dated() -> None:
    text = _DESIGN_DOC.read_text(encoding="utf-8")
    problems = lint_preregistration(text)
    assert problems == [], f"pre-registration lint failures: {problems}"


def test_preregistration_lint_flags_missing_field() -> None:
    # A doc missing a required frozen field must be flagged.
    text = _DESIGN_DOC.read_text(encoding="utf-8")
    mutated = text.replace("frozen-field: decision-rule", "xxx-removed-field")
    problems = lint_preregistration(mutated)
    assert any("decision-rule" in p for p in problems)


def test_preregistration_lint_flags_undated_field() -> None:
    # A required field present but stripped of its date must be flagged.
    text = _DESIGN_DOC.read_text(encoding="utf-8")
    lines = text.splitlines()
    out: list[str] = []
    for ln in lines:
        if "frozen-field: primary-endpoint" in ln:
            # remove every YYYY-MM-DD token on that line
            import re as _re

            out.append(_re.sub(r"\b20\d\d-\d\d-\d\d\b", "REDACTED", ln))
        else:
            out.append(ln)
    mutated = "\n".join(out)
    problems = lint_preregistration(mutated)
    assert any("primary-endpoint" in p for p in problems)


def test_preregistration_lint_requires_status_decision_ready() -> None:
    text = _DESIGN_DOC.read_text(encoding="utf-8").replace(
        "status: decision-ready", "status: draft"
    )
    problems = lint_preregistration(text)
    assert any("decision-ready" in p for p in problems)
