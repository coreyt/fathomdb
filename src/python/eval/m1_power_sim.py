"""M1 whole-``decide()``-rule power simulation (Slice 5, design §4 amendment 4).

Binding spec: ``dev/design/0.8.2-m1-multihop-harness.md`` §4 (frozen-field:
mde-power-plan) and ``dev/plans/plan-0.8.2.md`` §4 (Slice 5 AC-M1-5d). Power is
sized by a **whole-rule power simulation**, NOT a marginal per-hop MDE: from the
measured baseline F1/EM variance, draw question-level paired-bootstrap resamples
under ≥3 effect shapes (**flat-positive +0.04**, **monotonic**, **inverted-U**),
push each through the **real** frozen :func:`m1_decision_rule.decide`, and report
**P(GO)** per shape. N (the pooled ≥3-hop cell) is sized so the rule attains
**P(GO) ≥ 0.8 under flat-positive +0.04**; ``power_ok = True`` only if that holds.

We do **not** redefine ``decide()`` — it is imported and called as the single
source of the GO/NO-GO computation.

Modelling choices (logged, not hidden):
  * The **primary gate** the sim exercises is the pooled ≥3-hop **material** gate
    (``f1_delta >= 0.02`` AND its paired-bootstrap ``f1_ci_low > 0``). Under the
    flat-positive shape the trend gate (slope ≈ 0 ⇒ not significantly negative),
    the EM gate (``ci_high >= 0``) and the confident-wrong gate pass trivially, so
    the binding constraint for required-N is the material gate — a paired-difference
    power calculation, exactly the design's intent.
  * **Paired-difference noise.** Only the *baseline* per-question F1/EM variance is
    measured at Slice 5 (the ppr arm is Slice 15). The per-question effect noise is
    modelled as ``sd_pair = sd_baseline * sqrt(2 * (1 - rho))`` with a documented
    within-question correlation ``rho`` (default 0.5 ⇒ ``sd_pair == sd_baseline``).
    ``rho`` is a logged parameter; a sensitivity sweep is reported.
  * Deterministic given ``seed`` (no clock; numpy ``default_rng``).
"""

from __future__ import annotations

import math
from collections.abc import Sequence
from typing import Any

import numpy as np

from eval.m1_decision_rule import MATERIAL_F1_LIFT, decide

EFFECT_SHAPES: tuple[str, ...] = ("flat_positive", "monotonic", "inverted_u")

#: Default flat-positive lift the power target is sized against (design §4).
#: Must equal MATERIAL_F1_LIFT so a threshold change is automatically reflected
#: in the power target (pwrfix: rev2 left this at 0.03 while MATERIAL_F1_LIFT
#: was bumped to 0.04, making the sim compute P(GO) at a sub-threshold effect).
FLAT_POSITIVE_LIFT: float = MATERIAL_F1_LIFT
#: Default within-question arm correlation for the paired-difference noise model.
DEFAULT_RHO = 0.5
#: Default power target.
TARGET_P_GO = 0.8


#: Closed-form MDE z critical values. The material gate is "paired CI_low > 0"
#: (a 95% two-sided CI ⇒ a one-sided 2.5% lower bound), so z_alpha = Φ⁻¹(0.975);
#: power 0.8 ⇒ z_power = Φ⁻¹(0.8).
_Z_ALPHA_2SIDED_05 = 1.959963984540054
_Z_POWER_80 = 0.8416212335729143


def paired_mde(
    sd_baseline: float,
    n: int,
    *,
    rho: float = DEFAULT_RHO,
    alpha_z: float = _Z_ALPHA_2SIDED_05,
    power_z: float = _Z_POWER_80,
) -> float:
    """Minimum detectable paired ΔF1 at cell size ``n`` and power 0.8.

    Closed-form normal approximation for the one-sample (paired-difference) test
    the material gate implements::

        MDE = (z_alpha + z_power) · sd_diff / sqrt(n),
        sd_diff = sd_baseline · sqrt(2·(1 − rho))

    ``sd_baseline`` is the measured per-question baseline F1 SD; ``rho`` the
    documented within-question arm correlation (ρ=0.5 ⇒ sd_diff == sd_baseline,
    the conservative case)."""
    if n <= 0:
        raise ValueError("n must be positive")
    sd_diff = sd_baseline * math.sqrt(2.0 * (1.0 - rho))
    return (alpha_z + power_z) * sd_diff / math.sqrt(n)


def propose_material_f1_lift(
    sd_baseline: float,
    n: int,
    *,
    rhos: Sequence[float] = (0.5, 0.7),
    round_to: float = 0.01,
) -> dict[str, Any]:
    """Compute the paired MDE at ``n`` for each ρ and PROPOSE a MATERIAL_F1_LIFT.

    The proposed value is the conservative (smallest-ρ ⇒ largest sd_diff ⇒ largest
    MDE) MDE rounded UP to ``round_to``. **Pure compute** — this does NOT touch
    :data:`m1_decision_rule.MATERIAL_F1_LIFT` or :func:`decide`; the re-freeze is
    HITL-confirmed at the next gate. The current frozen value is reported for the
    delta."""
    conservative_rho = min(rhos)
    mde_by_rho = {f"rho_{rho}": round(paired_mde(sd_baseline, n, rho=rho), 6) for rho in rhos}
    conservative = paired_mde(sd_baseline, n, rho=conservative_rho)
    proposed = math.ceil(conservative / round_to) * round_to
    return {
        "n": n,
        "sd_baseline_f1": round(sd_baseline, 6),
        "power": TARGET_P_GO,
        "alpha_two_sided": 0.05,
        "mde_by_rho": mde_by_rho,
        "conservative_rho": conservative_rho,
        "conservative_mde": round(conservative, 6),
        "proposed_material_f1_lift": round(proposed, 4),
        "rounding_rule": f"ceil(conservative ρ={conservative_rho} MDE / {round_to}) · {round_to}",
        "current_frozen_material_f1_lift": MATERIAL_F1_LIFT,
        "note": (
            "PROPOSED ONLY — decide()/MATERIAL_F1_LIFT unchanged in this slice; HITL "
            "confirms the value at the next gate before a tiny decide() re-freeze."
        ),
    }


def effect_delta(shape: str, hop_counts: np.ndarray, *, lift: float = FLAT_POSITIVE_LIFT) -> np.ndarray:
    """Per-question TRUE ΔF1 (ppr − comparator) for an effect ``shape``.

    Each shape is centred so the *pooled* mean lift ≈ ``lift`` (so the shapes are
    comparable at the primary endpoint and only their hop-profile differs):
      * ``flat_positive`` — equal ``lift`` at every hop (not strictly increasing).
      * ``monotonic``     — increasing with hop (2<3<4), pooled mean ≈ ``lift``.
      * ``inverted_u``    — peaks at 3-hop, dips at 2 and 4 (HippoRAG shape).
    """
    hops = np.asarray(hop_counts, dtype=float)
    if shape == "flat_positive":
        d = np.full_like(hops, lift)
    elif shape == "monotonic":
        # linear in hop, centred to mean ~lift over the present hops
        base = (hops - hops.mean()) * lift
        d = lift + base
    elif shape == "inverted_u":
        # peak at hop 3: penalise distance from 3, then CENTER so the pooled mean
        # over the present hops == lift (subtract the mean bump). Without centering
        # the bump rode ON TOP of lift ⇒ pooled mean ≈ lift·(1+0.8·frac_3hop) > lift,
        # inflating P(GO) for this shape relative to flat/monotonic ([P2]).
        bump = (lift * 0.8) * (1.0 - np.abs(hops - 3.0))
        d = lift + bump - bump.mean()
    else:
        raise ValueError(f"unknown effect shape: {shape!r} (expected one of {EFFECT_SHAPES})")
    return d


def _percentile_ci_low(deltas: np.ndarray, rng: np.random.Generator, *, n_boot: int, alpha: float = 0.05) -> float:
    """Lower bound of a percentile paired-bootstrap CI for the mean of ``deltas``."""
    n = len(deltas)
    idx = rng.integers(0, n, size=(n_boot, n))
    boot_means = deltas[idx].mean(axis=1)
    return float(np.quantile(boot_means, alpha / 2))


def _percentile_ci_high(deltas: np.ndarray, rng: np.random.Generator, *, n_boot: int, alpha: float = 0.05) -> float:
    n = len(deltas)
    idx = rng.integers(0, n, size=(n_boot, n))
    boot_means = deltas[idx].mean(axis=1)
    return float(np.quantile(boot_means, 1 - alpha / 2))


def _slope_neg_significant(
    hops: np.ndarray, deltas: np.ndarray, rng: np.random.Generator, *, n_boot: int, alpha: float = 0.05
) -> bool:
    """True iff the OLS slope of ΔF1 on hop is significantly negative (its
    paired-bootstrap CI lies entirely below 0). Needs ≥2 distinct hop values."""
    if len(np.unique(hops)) < 2:
        return False
    n = len(deltas)
    idx = rng.integers(0, n, size=(n_boot, n))
    hb = hops[idx]
    db = deltas[idx]
    hbar = hb.mean(axis=1, keepdims=True)
    dbar = db.mean(axis=1, keepdims=True)
    cov = ((hb - hbar) * (db - dbar)).sum(axis=1)
    var = ((hb - hbar) ** 2).sum(axis=1)
    var = np.where(var == 0, np.nan, var)
    slopes = cov / var
    slopes = slopes[np.isfinite(slopes)]
    if slopes.size == 0:
        return False
    ci_high = float(np.quantile(slopes, 1 - alpha / 2))
    return ci_high < 0.0


def simulate_p_go(
    base_f1: Sequence[float],
    base_em: Sequence[float],
    hop_counts: Sequence[int],
    *,
    shape: str,
    n: int,
    rho: float = DEFAULT_RHO,
    lift: float = FLAT_POSITIVE_LIFT,
    n_trials: int = 300,
    n_boot: int = 400,
    seed: int = 0,
) -> dict[str, Any]:
    """Estimate P(GO) for a pooled ≥3-hop cell of size ``n`` under ``shape``.

    Draws ``n_trials`` paired-bootstrap resamples from the measured baseline
    F1/EM distribution, applies the shape's per-question ΔF1, computes the
    summary statistics the harness would hand ``decide()``, and reports the
    fraction that ``decide()`` calls **GO**. ``decide()`` is the real frozen rule.
    """
    if shape not in EFFECT_SHAPES:
        raise ValueError(f"unknown shape {shape!r}")
    bf = np.asarray(base_f1, dtype=float)
    be = np.asarray(base_em, dtype=float)
    bh = np.asarray(hop_counts, dtype=float)
    if bf.size == 0:
        raise ValueError("base_f1 is empty — need measured baseline F1")
    sd_f1 = float(bf.std(ddof=1)) if bf.size > 1 else 0.0
    sd_em = float(be.std(ddof=1)) if be.size > 1 else 0.0
    sd_pair_f1 = sd_f1 * math.sqrt(2 * (1 - rho))
    sd_pair_em = sd_em * math.sqrt(2 * (1 - rho))

    rng = np.random.default_rng(seed)
    go = 0
    for _ in range(n_trials):
        # Resample a size-n pooled ≥3-hop cell from the measured baseline (its hop
        # mix carries the trend structure).
        sel = rng.integers(0, bf.size, size=n)
        hops = bh[sel]
        true_d = effect_delta(shape, hops, lift=lift)
        # Model the per-question PAIRED DIFFERENCE directly (ppr − comparator):
        # mean = the shape's true ΔF1, SD = the paired-difference SD derived from
        # the measured baseline variance. Per-question F1 ∈ [0,1] ⇒ ΔF1 ∈ [-1,1];
        # clip the *difference*, NOT ppr=base+noise — the latter biases the mean
        # delta negative at large N when the baseline F1 is bimodal (a power
        # artifact that made P(GO) spuriously fall toward 0 as N grew).
        delta_f1 = np.clip(true_d + rng.normal(0, sd_pair_f1, size=n), -1.0, 1.0)
        # EM: no modelled lift (F1 is the primary signal; EM is a CI-banded guard).
        delta_em = np.clip(rng.normal(0, sd_pair_em, size=n), -1.0, 1.0)

        f1_delta = float(delta_f1.mean())
        f1_ci_low = _percentile_ci_low(delta_f1, rng, n_boot=n_boot)
        em_ci_high = _percentile_ci_high(delta_em, rng, n_boot=n_boot)
        neg_sig = _slope_neg_significant(hops, delta_f1, rng, n_boot=n_boot)

        verdict = decide(
            material={"f1_delta": f1_delta, "f1_ci_low": f1_ci_low},
            em={"ci_high": em_ci_high},
            trend={"neg_significant": neg_sig},
            confident_wrong={"increase_significant": False},
            power_ok=True,
        )
        if verdict == "GO":
            go += 1

    return {
        "shape": shape,
        "n": n,
        "p_go": round(go / n_trials, 4),
        "rho": rho,
        "lift": lift,
        "sd_baseline_f1": round(sd_f1, 6),
        "sd_pair_f1": round(sd_pair_f1, 6),
        "n_trials": n_trials,
        "n_boot": n_boot,
        "material_threshold": MATERIAL_F1_LIFT,
        "seed": seed,
    }


def required_n(
    base_f1: Sequence[float],
    base_em: Sequence[float],
    hop_counts: Sequence[int],
    *,
    shape: str = "flat_positive",
    target: float = TARGET_P_GO,
    grid: Sequence[int] = (50, 100, 150, 200, 300, 400, 600, 800, 1200, 1600, 2000),
    rho: float = DEFAULT_RHO,
    lift: float = FLAT_POSITIVE_LIFT,
    n_trials: int = 300,
    n_boot: int = 400,
    seed: int = 0,
) -> dict[str, Any]:
    """Smallest pooled ≥3-hop cell N on ``grid`` with P(GO) ≥ ``target``.

    Returns the required N (``None`` if the grid never clears ``target``) plus the
    full P(GO) curve, so the projection is auditable.
    """
    curve: list[dict[str, Any]] = []
    chosen: int | None = None
    for n in grid:
        res = simulate_p_go(
            base_f1, base_em, hop_counts,
            shape=shape, n=n, rho=rho, lift=lift,
            n_trials=n_trials, n_boot=n_boot, seed=seed,
        )
        curve.append({"n": n, "p_go": res["p_go"]})
        if chosen is None and res["p_go"] >= target:
            chosen = n
    return {
        "shape": shape,
        "target_p_go": target,
        "required_n": chosen,
        "power_ok": chosen is not None,
        "curve": curve,
        "rho": rho,
        "lift": lift,
        "material_threshold": MATERIAL_F1_LIFT,
        "note": (
            "required_n is the pooled >=3-hop cell size; power sized via the whole "
            "decide() rule (imported, not redefined). MATERIAL_F1_LIFT (0.04) sits "
            "at/above the implied pooled >=3-hop MDE at this N."
        ),
    }
