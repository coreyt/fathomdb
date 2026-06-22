"""Frozen interpretation rule for the 0.8.3 gap-decomposition probe.

This module is the *executable core* of the gap-decomposition pre-registration
(design ``dev/design/0.8.3-gap-decomposition-probe.md`` §3). It freezes — before
any number is seen — the mechanical, **power-safe** rule that turns the three
already-computed bounded components into a single lever DIRECTION:

* **R**     = RETRIEVAL bound  ``acc_oracle_raw − acc_fathomdb``
* **F**     = DISTILLED_FORM bound ``acc_oracle_distilled − acc_oracle_raw``
* **Resid** = MEM0_RESIDUAL ``acc_mem0 − acc_oracle_distilled``

:func:`decide_gap_decomposition` consumes the **already-computed** paired
``{point, ci_lo, ci_hi, mde, n}`` for each component (the runner runs the paired
bootstrap; this stays pure/deterministic — no RNG, no I/O, stdlib only) and a
per-class ``fit_coverage`` (the fraction of the class's questions whose oracle
context fit untruncated). It mirrors :mod:`eval.decision_rule_083`'s style but is
a **separate** frozen module — it does NOT edit / import that rule.

Two deliberate design points (codex round-4 CONCERNs):

* **Power-safe dominance (codex Q3):** a component wins only when it beats every
  rival's *optimistic* CI-upper, never on point-order alone — so an underpowered
  rival cannot hide a larger true effect behind the point-largest component.
* **Fail-INCONCLUSIVE, do not raise (codex Q3):** unlike ``decide_083`` (which
  raises on a malformed endpoint), a *missing / None / non-finite* component stat,
  or a per-class ``fit_coverage`` below the floor, forces **INCONCLUSIVE** in
  CODE. A selective oracle-fit exclusion changes the estimand; below the floor the
  surviving subset no longer decomposes the D0b gap, so no DOMINANT claim is
  permitted regardless of the (then unrepresentative) component CIs.
"""

from __future__ import annotations

import math
from collections.abc import Mapping
from typing import Literal, Optional, TypedDict

# --------------------------------------------------------------------------- #
# Frozen constants (the rule must be auditable). See design §3.
# --------------------------------------------------------------------------- #

#: Materiality / underpowered band ε (5 percentage points) — reused from
#: :data:`eval.decision_rule_083.EPS_NEAR_PARITY` but re-pinned here so the
#: gap-decomposition rule stands alone (frozen 2026-06-22; design §3).
EPS: float = 0.05

#: Per-class oracle-fit coverage floor. A class with a smaller fraction of
#: ``oracle_fit_complete`` questions is forced INCONCLUSIVE — selective exclusion
#: of the long / multi-doc cases would mean the surviving subset no longer
#: decomposes the D0b gap (codex round-4 BLOCK; design §2 fit-coverage floor).
FIT_COVERAGE_MIN: float = 0.80

#: The three bounded components, in the pinned tie-break / iteration order.
COMPONENTS: tuple[str, ...] = ("RETRIEVAL", "DISTILLED_FORM", "MEM0_RESIDUAL")

#: Component name → its DOMINANT verdict token.
_DOMINANT_OF: dict[str, str] = {
    "RETRIEVAL": "RETRIEVAL_DOMINANT",
    "DISTILLED_FORM": "DISTILLED_FORM_DOMINANT",
    "MEM0_RESIDUAL": "MEM0_RESIDUAL_DOMINANT",
}

Verdict = Literal[
    "RETRIEVAL_DOMINANT",
    "DISTILLED_FORM_DOMINANT",
    "MEM0_RESIDUAL_DOMINANT",
    "INCONCLUSIVE",
]
_INCONCLUSIVE: Verdict = "INCONCLUSIVE"

#: The required per-component statistic keys.
_STAT_KEYS: tuple[str, ...] = ("point", "ci_lo", "ci_hi", "mde")


class ComponentFacts(TypedDict):
    """Per-component echo of the input stats plus the two derived gate flags."""

    point: float
    ci_lo: float
    ci_hi: float
    mde: float
    n: int
    positive: bool
    powered: bool


class GapDecision(TypedDict):
    """The frozen gap-decomposition verdict (design §3)."""

    verdict: Verdict
    components: dict[str, ComponentFacts]
    fit_coverage: float
    reason: Optional[str]


def _finite(value: object) -> Optional[float]:
    """Return ``float(value)`` when it is a finite real, else ``None``.

    A missing key, ``None``, a non-numeric, or a non-finite (``NaN`` / ``±inf``)
    value all map to ``None`` — the caller forces INCONCLUSIVE rather than raise
    (design §3: a malformed component must not silently yield a DOMINANT verdict,
    and an underpowered/absent component must escalate, not act)."""
    if value is None or isinstance(value, bool):
        return None
    try:
        v = float(value)  # type: ignore[arg-type]
    except (TypeError, ValueError):
        return None
    return v if math.isfinite(v) else None


def decide_gap_decomposition(
    components: Mapping[str, Mapping[str, object]],
    fit_coverage: float,
    *,
    eps: float = EPS,
    fit_coverage_min: float = FIT_COVERAGE_MIN,
) -> GapDecision:
    """Return the frozen gap-decomposition verdict (design §3).

    ``components`` maps **each** of :data:`COMPONENTS` to an already-computed
    ``{point, ci_lo, ci_hi, mde, n}`` (the per-class or pooled paired delta with
    its 95% CI bounds and the paired MDE). ``fit_coverage`` is the per-class
    oracle-fit coverage (fraction of the class's questions with
    ``oracle_fit_complete``); pass ``1.0`` for the pooled / synthetic case.

    The rule, in order (all branches return INCONCLUSIVE — never raise):

    1. **Fit-coverage floor:** ``fit_coverage`` non-finite or ``< fit_coverage_min``
       ⇒ INCONCLUSIVE (``reason="fit_coverage:<v>"``). Selective exclusion changes
       the estimand, so a low-coverage class cannot make a DOMINANT claim.
    2. **Completeness:** any component missing, or any of its
       ``{point, ci_lo, ci_hi, mde}`` ``None`` / non-finite, or ``n`` absent ⇒
       INCONCLUSIVE (``reason="missing_stat:<component>"``). ``n<=1`` yields a
       ``None`` MDE upstream → caught here.
    3. **Power-safe dominance:** the first component ``X`` (in :data:`COMPONENTS`
       order) with ``X.ci_lo > 0`` **AND** ``X.ci_lo >= max(rivals' ci_hi)``
       **AND** ``X.mde <= eps`` ⇒ ``X_DOMINANT``. Even at the rivals' optimistic
       CI-upper, X still wins, so no underpowered rival can hide a larger effect.
    4. Otherwise INCONCLUSIVE (``reason="no_dominant"``).

    Deterministic: same input → same :class:`GapDecision`.
    """
    cov = _finite(fit_coverage)

    # Parse + echo every component up front (no raise; a bad stat → flagged).
    facts: dict[str, ComponentFacts] = {}
    missing: Optional[str] = None
    for name in COMPONENTS:
        raw = components.get(name)
        parsed = _parse_component(raw)
        if parsed is None:
            if missing is None:
                missing = name
            continue
        facts[name] = parsed

    def _result(verdict: Verdict, reason: Optional[str]) -> GapDecision:
        return GapDecision(
            verdict=verdict,
            components=facts,
            fit_coverage=cov if cov is not None else float("nan"),
            reason=reason,
        )

    # 1 — fit-coverage floor (checked first: it gates the estimand itself).
    if cov is None:
        return _result(_INCONCLUSIVE, "fit_coverage:none")
    if cov < fit_coverage_min:
        return _result(_INCONCLUSIVE, f"fit_coverage:{cov:.4f}")

    # 2 — completeness: every component must be present + fully finite (+ powered MDE).
    if missing is not None or len(facts) != len(COMPONENTS):
        return _result(_INCONCLUSIVE, f"missing_stat:{missing or 'unknown'}")

    # 3 — power-safe dominance (first qualifying component in pinned order).
    for name in COMPONENTS:
        x = facts[name]
        rivals_ci_hi = [facts[o]["ci_hi"] for o in COMPONENTS if o != name]
        if (
            x["positive"]
            and x["powered"]
            and x["ci_lo"] >= max(rivals_ci_hi)
        ):
            return _result(_DOMINANT_OF[name], None)  # type: ignore[arg-type]

    # 4 — no component beats every rival's optimistic upper at power.
    return _result(_INCONCLUSIVE, "no_dominant")


def _parse_component(raw: Optional[Mapping[str, object]]) -> Optional[ComponentFacts]:
    """Parse one component's stats → :class:`ComponentFacts`, or ``None`` if any
    required stat is missing / None / non-finite (or ``n`` absent). ``positive`` =
    ``ci_lo > 0``; ``powered`` = ``mde <= EPS``."""
    if raw is None:
        return None
    vals: dict[str, float] = {}
    for key in _STAT_KEYS:
        v = _finite(raw.get(key))
        if v is None:
            return None
        vals[key] = v
    n_raw = raw.get("n")
    if n_raw is None or isinstance(n_raw, bool):
        return None
    try:
        n = int(n_raw)  # type: ignore[arg-type]
    except (TypeError, ValueError):
        return None
    return ComponentFacts(
        point=vals["point"],
        ci_lo=vals["ci_lo"],
        ci_hi=vals["ci_hi"],
        mde=vals["mde"],
        n=n,
        positive=vals["ci_lo"] > 0.0,
        powered=vals["mde"] <= EPS,
    )
