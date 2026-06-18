"""M1 whole-rule power simulation — TDD (RED first), Slice 5 / AC-M1-5d.

Binding spec: ``dev/design/0.8.2-m1-multihop-harness.md`` §4 (frozen-field:
mde-power-plan). These tests pin that the power sim maps (variance, shape) →
P(GO) through the **real** frozen :func:`m1_decision_rule.decide` (imported, not
redefined), over the ≥3 pre-registered effect shapes, and that the required-N
search finds the smallest pooled ≥3-hop cell clearing P(GO) ≥ 0.8 under the
flat-positive +0.03 shape. Pure CPU / deterministic (seeded) — no LLM, no DB.
"""

from __future__ import annotations

import numpy as np
import pytest

from eval.m1_decision_rule import decide
from eval.m1_power_sim import (
    EFFECT_SHAPES,
    effect_delta,
    required_n,
    simulate_p_go,
)


def _baseline(n: int = 400, seed: int = 1) -> tuple[list[float], list[float], list[int]]:
    """A synthetic measured-baseline F1/EM/hop sample (single-digit-F1 regime)."""
    rng = np.random.default_rng(seed)
    f1 = np.clip(rng.normal(0.10, 0.20, n), 0, 1).tolist()
    em = np.clip(rng.normal(0.05, 0.15, n), 0, 1).tolist()
    hops = rng.choice([3, 4], size=n, p=[0.65, 0.35]).tolist()
    return f1, em, hops


def test_three_effect_shapes_registered() -> None:
    assert set(EFFECT_SHAPES) == {"flat_positive", "monotonic", "inverted_u"}


def test_effect_delta_shapes_have_expected_profile() -> None:
    hops = np.array([2, 3, 4])
    flat = effect_delta("flat_positive", hops, lift=0.03)
    assert np.allclose(flat, 0.03)
    mono = effect_delta("monotonic", hops, lift=0.03)
    assert mono[0] < mono[1] < mono[2]
    inv = effect_delta("inverted_u", hops, lift=0.03)
    assert inv[1] > inv[0] and inv[1] > inv[2]  # peak at hop 3


def test_simulate_p_go_uses_real_decide_and_returns_probability() -> None:
    f1, em, hops = _baseline()
    for shape in EFFECT_SHAPES:
        res = simulate_p_go(f1, em, hops, shape=shape, n=200, n_trials=120, n_boot=200, seed=0)
        assert 0.0 <= res["p_go"] <= 1.0
        assert res["shape"] == shape
        assert res["material_threshold"] == 0.02


def test_p_go_is_monotone_increasing_in_n_under_flat_positive() -> None:
    f1, em, hops = _baseline()
    small = simulate_p_go(f1, em, hops, shape="flat_positive", n=50, n_trials=200, n_boot=200, seed=0)
    large = simulate_p_go(f1, em, hops, shape="flat_positive", n=1600, n_trials=200, n_boot=200, seed=0)
    # more questions ⇒ tighter CI ⇒ the material CI-excludes-0 gate fires more often
    assert large["p_go"] >= small["p_go"]
    assert large["p_go"] > 0.5


def test_required_n_finds_smallest_cell_clearing_target() -> None:
    f1, em, hops = _baseline()
    out = required_n(f1, em, hops, shape="flat_positive", target=0.8,
                     n_trials=200, n_boot=200, seed=0)
    assert out["required_n"] is None or isinstance(out["required_n"], int)
    if out["required_n"] is not None:
        assert out["power_ok"] is True
        # the curve is sorted by n and the chosen n is the first to clear target
        clearing = [c["n"] for c in out["curve"] if c["p_go"] >= 0.8]
        assert out["required_n"] == min(clearing)


def test_decide_is_the_real_frozen_rule() -> None:
    # a flat-positive powered case ⇒ GO (the load-bearing amendment regression)
    assert decide(
        material={"f1_delta": 0.03, "f1_ci_low": 0.01},
        em={"ci_high": 0.0},
        trend={"neg_significant": False},
        confident_wrong={"increase_significant": False},
        power_ok=True,
    ) == "GO"
    # CI includes 0 ⇒ NO_GO
    assert decide(
        material={"f1_delta": 0.03, "f1_ci_low": -0.001},
        em={"ci_high": 0.0},
        trend={"neg_significant": False},
        confident_wrong={"increase_significant": False},
        power_ok=True,
    ) == "NO_GO"


def test_unknown_shape_raises() -> None:
    with pytest.raises(ValueError):
        effect_delta("linear", np.array([3, 4]))
