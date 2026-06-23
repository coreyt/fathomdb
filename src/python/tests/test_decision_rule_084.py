"""Slice 0 (0.8.4 / GraphRAG-parity) — frozen RESOLUTION decision rule + pre-reg lint.

Pre-registration is only credible if the REACHED/NOT_REACHED computation is frozen
as *code* before any number is seen, so a downstream slice (5/10/15/20) cannot
post-hoc switch the endpoint. This module pins the full truth table of
``eval.decision_rule_084.decide_084`` at its boundaries (on the **pairwise-win-rate
scale** where parity = 0.5), the four **bias-control BLOCK gates**, the Slice-5
kill-early predicate (``strong_baseline_clears``), the honest-prior funding gate
(``honest_prior_cleared``), and lints that
``dev/design/0.8.4-graphrag-sensemaking.md`` carries the required frozen, dated
pre-registration fields.

The rule consumes already-computed summary statistics (the AutoE harness runs the
clustered bootstrap; ``decide_084`` stays pure/deterministic — no RNG, no I/O):

    decide_084(primary_per_metric, bias_controls, length_corroboration) -> Resolution

REACHED iff (1) NOT blocked by a bias-control gate (order swapped, ≥5 runs,
cross-family judge, length corroboration run), (2) no headline metric is
under-powered (``mde ≤ ε_wr``), (3) the length corroboration does not contradict,
and (4) *every* headline metric clears the near-parity band (paired-CI lower bound
``ci_lo >= 0.5 - ε_wr``). ``surpass_candidates`` = the metrics whose ``ci_lo > 0.5``.
All floats must be finite — a malformed endpoint raises ``ValueError``.

Pure stdlib only — **no ``fathomdb`` / ``scipy`` / ``networkx`` import** — so this
test runs anywhere, independent of the native-extension build or the ``.venv``
binding.
"""

from __future__ import annotations

import re as _re
from pathlib import Path

import pytest

from eval.decision_rule_084 import (
    EPS_WIN_RATE,
    HEADLINE_METRICS,
    HONEST_PRIOR_CRITERIA,
    MIN_RUNS,
    PARITY,
    REQUIRED_FROZEN_FIELDS_084,
    decide_084,
    honest_prior_cleared,
    lint_preregistration_084,
    strong_baseline_clears,
)

# --------------------------------------------------------------------------- #
# Locate the frozen design doc (repo-relative; no fathomdb import).
# tests/ -> src/python -> src -> repo-root
# --------------------------------------------------------------------------- #
_REPO_ROOT = Path(__file__).resolve().parents[3]
_DESIGN_DOC = _REPO_ROOT / "dev" / "design" / "0.8.4-graphrag-sensemaking.md"


# --------------------------------------------------------------------------- #
# Builders for the decide_084 signature.
# --------------------------------------------------------------------------- #
def _wr(
    *,
    win_rate: float = 0.55,
    ci_lo: float = 0.47,
    ci_hi: float = 0.63,
    mde: float = 0.04,
    n: int = 200,
) -> dict[str, float]:
    return {"win_rate": win_rate, "ci_lo": ci_lo, "ci_hi": ci_hi, "mde": mde, "n": n}


def _metrics(**overrides: dict[str, float]) -> dict[str, dict[str, float]]:
    """Build the per-metric mapping, defaulting every metric to a passing win-rate."""
    table: dict[str, dict[str, float]] = {m: _wr() for m in HEADLINE_METRICS}
    table.update(overrides)
    return table


def _controls(**overrides: object) -> dict[str, object]:
    controls: dict[str, object] = {
        "order_swapped": True,
        "n_runs": 5,
        "judge_family": "anthropic",
        "system_families": ["openai", "microsoft"],
    }
    controls.update(overrides)
    return controls


def _length(**overrides: object) -> dict[str, object]:
    corr: dict[str, object] = {"ran": True, "contradicts": False}
    corr.update(overrides)
    return corr


def _decide(
    table: dict[str, dict[str, float]] | None = None,
    controls: dict[str, object] | None = None,
    length: dict[str, object] | None = None,
) -> dict:
    return decide_084(
        table if table is not None else _metrics(),
        controls if controls is not None else _controls(),
        length if length is not None else _length(),
    )


# --------------------------------------------------------------------------- #
# Frozen constants are auditable (the rule must be inspectable).
# --------------------------------------------------------------------------- #
def test_frozen_eps_win_rate() -> None:
    assert EPS_WIN_RATE == pytest.approx(0.05)


def test_frozen_parity() -> None:
    assert PARITY == pytest.approx(0.5)


def test_frozen_min_runs() -> None:
    assert MIN_RUNS == 5


def test_frozen_headline_metrics() -> None:
    assert HEADLINE_METRICS == ("comprehensiveness", "diversity", "empowerment")


def test_required_frozen_fields_set() -> None:
    assert set(REQUIRED_FROZEN_FIELDS_084) == {
        "near-parity-band",
        "power-sizing rule",
        "decision-rule",
        "bias-controls",
        "surpass-option protocol",
        "honest-prior gate",
    }


def test_honest_prior_criteria_set() -> None:
    assert set(HONEST_PRIOR_CRITERIA) == {
        "distinct_from_multihop_prior",
        "strong_baseline_planned",
        "kill_early_criteria_frozen",
        "budget_approved",
    }


# --------------------------------------------------------------------------- #
# Truth table — REACHED only when not blocked, not under-powered, not length-
# confounded, and every headline metric clears the near-parity band.
# --------------------------------------------------------------------------- #
def test_all_metrics_near_parity_is_reached() -> None:
    res = _decide()
    assert res["verdict"] == "REACHED"
    assert res["blocked_by"] is None
    assert res["binding_constraint"] is None


def test_reached_lists_strictly_better_metrics_as_surpass_candidates() -> None:
    # comprehensiveness strictly better (ci_lo > 0.5); the other two near-parity.
    table = _metrics(
        comprehensiveness=_wr(win_rate=0.62, ci_lo=0.54, ci_hi=0.70),
        diversity=_wr(win_rate=0.48, ci_lo=0.46, ci_hi=0.52),
        empowerment=_wr(win_rate=0.48, ci_lo=0.46, ci_hi=0.52),
    )
    res = _decide(table)
    assert res["verdict"] == "REACHED"
    assert res["surpass_candidates"] == ["comprehensiveness"]


def test_ci_lo_exactly_parity_minus_eps_is_near_parity_reached() -> None:
    # Boundary: ci_lo == PARITY - EPS clears the band (>=, not >).
    table = _metrics(empowerment=_wr(win_rate=0.47, ci_lo=PARITY - EPS_WIN_RATE, ci_hi=0.52))
    res = _decide(table)
    assert res["verdict"] == "REACHED"


def test_one_metric_below_band_is_not_reached() -> None:
    table = _metrics(empowerment=_wr(win_rate=0.38, ci_lo=0.30, ci_hi=0.44))
    res = _decide(table)
    assert res["verdict"] == "NOT_REACHED"
    assert res["binding_constraint"] == "below_parity:empowerment"
    assert res["blocked_by"] is None


def test_multiple_metrics_below_band_names_all_in_order() -> None:
    table = _metrics(
        comprehensiveness=_wr(ci_lo=0.20),
        empowerment=_wr(ci_lo=0.20),
    )
    res = _decide(table)
    assert res["verdict"] == "NOT_REACHED"
    assert res["binding_constraint"] == "below_parity:comprehensiveness,empowerment"


# --- bias-control BLOCK (checked first) ------------------------------------- #
def test_position_bias_not_controlled_blocks() -> None:
    res = _decide(controls=_controls(order_swapped=False))
    assert res["verdict"] == "NOT_REACHED"
    assert res["blocked_by"] == "bias:position"
    assert res["binding_constraint"] is None


def test_too_few_runs_blocks() -> None:
    res = _decide(controls=_controls(n_runs=4))
    assert res["verdict"] == "NOT_REACHED"
    assert res["blocked_by"] == "bias:stochasticity"


def test_exactly_min_runs_does_not_block() -> None:
    res = _decide(controls=_controls(n_runs=MIN_RUNS))
    assert res["verdict"] == "REACHED"


def test_self_preference_judge_in_system_family_blocks() -> None:
    res = _decide(
        controls=_controls(judge_family="OpenAI", system_families=["openai", "microsoft"])
    )
    assert res["verdict"] == "NOT_REACHED"
    assert res["blocked_by"] == "bias:self_preference"


def test_length_corroboration_not_run_blocks() -> None:
    res = _decide(length=_length(ran=False))
    assert res["verdict"] == "NOT_REACHED"
    assert res["blocked_by"] == "bias:length_missing"


def test_bias_block_order_position_before_stochasticity() -> None:
    res = _decide(controls=_controls(order_swapped=False, n_runs=1))
    assert res["blocked_by"] == "bias:position"


def test_bias_block_precedes_below_parity() -> None:
    table = _metrics(empowerment=_wr(ci_lo=0.10))
    res = _decide(table, controls=_controls(order_swapped=False))
    assert res["blocked_by"] == "bias:position"
    assert res["binding_constraint"] is None


# --- power guard ------------------------------------------------------------ #
def test_underpowered_metric_is_not_reached() -> None:
    table = _metrics(diversity=_wr(mde=0.06))  # mde > EPS
    res = _decide(table)
    assert res["verdict"] == "NOT_REACHED"
    assert res["binding_constraint"] == "underpowered:diversity"
    assert res["blocked_by"] is None


def test_mde_exactly_eps_is_adequately_powered() -> None:
    table = _metrics(comprehensiveness=_wr(mde=EPS_WIN_RATE))
    res = _decide(table)
    assert res["verdict"] == "REACHED"


def test_power_guard_reports_first_underpowered_metric_in_order() -> None:
    table = _metrics(
        diversity=_wr(mde=0.07),
        empowerment=_wr(mde=0.07),
    )
    res = _decide(table)
    assert res["binding_constraint"] == "underpowered:diversity"


def test_bias_block_precedes_power_guard() -> None:
    table = _metrics(comprehensiveness=_wr(mde=0.50))
    res = _decide(table, controls=_controls(n_runs=1))
    assert res["blocked_by"] == "bias:stochasticity"
    assert res["binding_constraint"] is None


# --- length-confound guard -------------------------------------------------- #
def test_length_confound_is_not_reached() -> None:
    res = _decide(length=_length(contradicts=True))
    assert res["verdict"] == "NOT_REACHED"
    assert res["binding_constraint"] == "length_confounded"
    assert res["blocked_by"] is None


def test_power_guard_precedes_length_confound() -> None:
    # An under-powered metric is reported before the length-confound (power first).
    table = _metrics(diversity=_wr(mde=0.06))
    res = _decide(table, length=_length(contradicts=True))
    assert res["binding_constraint"] == "underpowered:diversity"


# --- result shape ----------------------------------------------------------- #
def test_result_carries_per_metric_echo_with_derived_flags() -> None:
    res = _decide()
    assert set(res) == {
        "verdict",
        "per_metric",
        "binding_constraint",
        "surpass_candidates",
        "blocked_by",
    }
    assert set(res["per_metric"]) == set(HEADLINE_METRICS)
    pm = res["per_metric"]["comprehensiveness"]
    assert pm["near_parity"] is True
    assert pm["better"] is False  # default ci_lo=0.47 < 0.5
    assert pm["underpowered"] is False


# --- missing keys / non-finite floats fail LOUDLY --------------------------- #
def test_missing_metric_raises_key_error() -> None:
    table = _metrics()
    del table["empowerment"]
    with pytest.raises(KeyError):
        _decide(table)


@pytest.mark.parametrize("bad", [float("nan"), float("inf"), float("-inf")])
def test_non_finite_ci_lo_raises(bad: float) -> None:
    table = _metrics(comprehensiveness=_wr(ci_lo=bad))
    with pytest.raises(ValueError):
        _decide(table)


@pytest.mark.parametrize("bad", [float("nan"), float("inf"), float("-inf")])
def test_non_finite_mde_raises(bad: float) -> None:
    table = _metrics(comprehensiveness=_wr(mde=bad))
    with pytest.raises(ValueError):
        _decide(table)


def test_malformed_endpoint_raises_even_when_control_also_absent() -> None:
    # The endpoint echo runs BEFORE the block check, so a bad number raises even
    # when a control is also violated (a bad number must not hide behind a block).
    table = _metrics(comprehensiveness=_wr(ci_lo=float("nan")))
    with pytest.raises(ValueError):
        _decide(table, controls=_controls(order_swapped=False))


# --- determinism ------------------------------------------------------------ #
def test_determinism_same_input_same_result() -> None:
    assert _decide() == _decide()


# --------------------------------------------------------------------------- #
# strong_baseline_clears — Slice-5 kill-early predicate (S1 vs long-context).
# --------------------------------------------------------------------------- #
def test_strong_baseline_clears_when_s1_near_parity_vs_long_context() -> None:
    assert strong_baseline_clears({"ci_lo": 0.46}) is True


def test_strong_baseline_does_not_clear_when_long_context_dominates() -> None:
    # S1 below the band vs the long-context control => ESCALATE before funding.
    assert strong_baseline_clears({"ci_lo": 0.30}) is False


def test_strong_baseline_boundary_at_parity_minus_eps_clears() -> None:
    assert strong_baseline_clears({"ci_lo": PARITY - EPS_WIN_RATE}) is True


@pytest.mark.parametrize("bad", [float("nan"), float("inf"), float("-inf")])
def test_strong_baseline_non_finite_raises(bad: float) -> None:
    with pytest.raises(ValueError):
        strong_baseline_clears({"ci_lo": bad})


# --------------------------------------------------------------------------- #
# honest_prior_cleared — the Slice-0 design-review funding gate.
# --------------------------------------------------------------------------- #
def _review(**overrides: object) -> dict[str, object]:
    review: dict[str, object] = {k: True for k in HONEST_PRIOR_CRITERIA}
    review.update(overrides)
    return review


def test_honest_prior_cleared_when_all_criteria_hold() -> None:
    assert honest_prior_cleared(_review()) is True


@pytest.mark.parametrize("missing", HONEST_PRIOR_CRITERIA)
def test_honest_prior_not_cleared_when_any_criterion_false(missing: str) -> None:
    assert honest_prior_cleared(_review(**{missing: False})) is False


def test_honest_prior_missing_key_raises() -> None:
    review = _review()
    del review["budget_approved"]
    with pytest.raises(KeyError):
        honest_prior_cleared(review)


# --------------------------------------------------------------------------- #
# Pre-registration schema lint — raises if any frozen/dated field is missing.
# --------------------------------------------------------------------------- #
def test_design_doc_exists() -> None:
    assert _DESIGN_DOC.is_file(), f"design doc not found: {_DESIGN_DOC}"


def test_prereg_084_lint_passes_on_real_design_doc() -> None:
    # No raise == clean (the doc is finalized to decision-ready in Slice 0).
    lint_preregistration_084(_DESIGN_DOC.read_text(encoding="utf-8"))


def test_prereg_084_lint_flags_downgraded_status() -> None:
    text = _DESIGN_DOC.read_text(encoding="utf-8").replace(
        "status: decision-ready", "status: draft"
    )
    with pytest.raises(AssertionError):
        lint_preregistration_084(text)


def test_prereg_084_lint_flags_missing_frozen_field() -> None:
    text = _DESIGN_DOC.read_text(encoding="utf-8").replace(
        "frozen-field: honest-prior gate", "xxx-removed-field"
    )
    with pytest.raises(AssertionError):
        lint_preregistration_084(text)


def test_prereg_084_lint_flags_missing_bias_controls_field() -> None:
    text = _DESIGN_DOC.read_text(encoding="utf-8").replace(
        "frozen-field: bias-controls", "xxx-removed-field"
    )
    with pytest.raises(AssertionError):
        lint_preregistration_084(text)


def test_prereg_084_lint_flags_undated_field() -> None:
    # A required field present but stripped of its date must be flagged.
    text = _DESIGN_DOC.read_text(encoding="utf-8")
    out: list[str] = []
    for ln in text.splitlines():
        if "frozen-field: surpass-option protocol" in ln:
            out.append(_re.sub(r"\b20\d\d-\d\d-\d\d\b", "REDACTED", ln))
        else:
            out.append(ln)
    with pytest.raises(AssertionError):
        lint_preregistration_084("\n".join(out))
