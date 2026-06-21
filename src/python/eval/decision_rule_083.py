"""Frozen RESOLUTION decision rule for the 0.8.3 Mem0-parity gate.

This module is the *executable core* of the 0.8.3 Slice-0 pre-registration. It
freezes — before any number is seen, so a downstream slice (10/20/25/30) cannot
post-hoc switch the endpoint — three things as code:

1. :func:`decide_083` — the **REACHED / NOT_REACHED** resolution computation over
   the external per-memory-class ``FathomDB − Mem0`` deltas, with the two hard
   BLOCK gates (eu7 fidelity floor; latency budget), a per-class power guard, and
   the near-parity band ε. **Slices 10/20/25/30 import this; they may not redefine
   the rule.**
2. :func:`probe_15a_pass` / :func:`probe_15b_pass` — the two $0, LLM-free triage
   probe pass criteria (15a embedder-ceiling, PRIMARY; 15b D2 content-at-scale
   proxy) from design §5.
3. :func:`lint_preregistration_083` — the schema lint asserting the design doc
   ``dev/design/0.8.3-mem0-parity.md`` carries every required frozen, dated
   pre-registration field.

The two-level reframe (design §2): the **external** Mem0 delta is THE GATE
(decides *done*); internal lever deltas are attribution only. ``decide_083``
consumes **already-computed summary statistics** — the parity harness runs the
paired bootstrap; this function stays pure/deterministic (no RNG, no I/O).

Binding spec: ``dev/design/0.8.3-mem0-parity.md`` §5 (frozen pre-registration) and
§9 (this module's contract); ``dev/plans/plan-0.8.3.md`` §4 (Slice 0).

Mirrors :mod:`eval.m1_decision_rule`'s style — pure stdlib, **no ``fathomdb`` /
``scipy`` / ``numpy`` / ``networkx`` import** — so it (and its test) run anywhere,
independent of the native-extension build or the ``.venv`` binding. Deterministic:
no clock, no RNG, no I/O.
"""

from __future__ import annotations

import math
import re
from collections.abc import Mapping
from typing import Literal, TypedDict

# --------------------------------------------------------------------------- #
# Frozen constants (the rule must be auditable). See design §5 (frozen-field:
# near-parity-band) + §4 (hard gates).
# --------------------------------------------------------------------------- #

#: Near-parity band ε (5 percentage points). "Near-parity-or-better" on a class =
#: the external ``FathomDB − Mem0`` paired-CI **lower bound ≥ −ε**; "better" =
#: lower bound **> 0**. Frozen 2026-06-21 (design §5; do NOT change downstream).
EPS_NEAR_PARITY: float = 0.05

#: The eu7 ANN-quantization fidelity recall@10 hard floor — any embedder/pooling
#: change must re-clear this once (design §4 BLOCK gate / §5 eu7-break fork).
EU7_FLOOR: float = 0.90

#: The four agentic-memory classes scored independently (design §3). The
#: resolution requires *every* class to clear the near-parity band.
MEMORY_CLASSES: tuple[str, ...] = (
    "factoid",
    "knowledge_update",
    "multi_session",
    "temporal",
)

Verdict = Literal["REACHED", "NOT_REACHED"]
_REACHED: Verdict = "REACHED"
_NOT_REACHED: Verdict = "NOT_REACHED"

BlockedBy = Literal["eu7", "latency"]


class ClassFacts(TypedDict):
    """Per-class echo of the input delta plus the three derived gate flags."""

    point: float
    ci_lo: float
    ci_hi: float
    mde: float
    n: int
    near_parity: bool
    better: bool
    underpowered: bool


class Resolution(TypedDict):
    """The frozen resolution verdict (design §9)."""

    verdict: Verdict
    per_class: dict[str, ClassFacts]
    binding_constraint: str | None
    surpass_candidates: list[str]
    blocked_by: BlockedBy | None


def _require_finite(value: float, name: str) -> float:
    """Return ``float(value)``; raise ``ValueError`` if it is non-finite.

    A ``NaN`` slips past every ``<`` / ``>=`` comparison (``nan < x`` and
    ``nan >= x`` are both False), so without this guard a malformed endpoint could
    silently yield a verdict. A malformed endpoint must fail loudly instead.
    """
    v = float(value)
    if not math.isfinite(v):
        raise ValueError(
            f"non-finite {name}: {value!r} (a malformed endpoint must fail loudly)"
        )
    return v


def decide_083(
    external_per_class: Mapping[str, Mapping[str, float]],
    eu7_recall: float,
    latency_ok: bool,
) -> Resolution:
    """Return the frozen RESOLUTION verdict for the 0.8.3 Mem0-parity gate.

    ``external_per_class`` maps **each** of :data:`MEMORY_CLASSES` to an
    already-computed delta ``{"point", "ci_lo", "ci_hi", "mde", "n"}`` — the
    external ``FathomDB − Mem0`` paired delta on that class with its 95% CI bounds,
    the per-class paired MDE, and the class N. ``eu7_recall`` is the measured eu7
    ANN-fidelity recall@10; ``latency_ok`` is the AC012/AC013/AC020 budget verdict.

    The rule (design §5 decision-rule, §9 truth table) — in order:

    1. **BLOCK (hard gates, checked first):** ``eu7_recall <`` :data:`EU7_FLOOR`
       ⇒ ``NOT_REACHED`` with ``blocked_by="eu7"``; else ``not latency_ok`` ⇒
       ``NOT_REACHED`` with ``blocked_by="latency"``. A block never makes a parity
       claim (``binding_constraint`` stays ``None``; ``blocked_by`` carries it).
    2. **Power guard:** any class with ``mde >`` :data:`EPS_NEAR_PARITY` ⇒
       ``NOT_REACHED`` with ``binding_constraint="underpowered:<class>"`` (the
       first such class in :data:`MEMORY_CLASSES` order) — no parity claim on an
       under-powered class.
    3. **Resolution:** ``REACHED`` iff *every* class clears the near-parity band
       (``ci_lo >= -EPS_NEAR_PARITY``); otherwise ``NOT_REACHED`` with
       ``binding_constraint="below_parity:<class>,<class>..."`` (the mechanical
       failing-class identifier; the HITL names the *semantic* binding constraint —
       embedder ceiling / reader / a class-specific lever — per design §5).

    ``surpass_candidates`` is always the classes whose ``ci_lo > 0`` (strictly
    better than Mem0). Deterministic: same input → same Resolution.

    Raises :class:`KeyError` if a class or a delta key is missing, and
    :class:`ValueError` if any float is non-finite (``NaN`` / ``±inf``) — a
    malformed endpoint must fail loudly, never silently return a verdict.
    """
    eu7 = _require_finite(eu7_recall, "eu7_recall")
    latency_intact = bool(latency_ok)

    # Echo + derive every class up front (KeyError if a class/key is missing;
    # ValueError if any float is non-finite) — fail loudly on a malformed endpoint.
    per_class: dict[str, ClassFacts] = {}
    for cls in MEMORY_CLASSES:
        delta = external_per_class[cls]
        point = _require_finite(delta["point"], f"{cls}.point")
        ci_lo = _require_finite(delta["ci_lo"], f"{cls}.ci_lo")
        ci_hi = _require_finite(delta["ci_hi"], f"{cls}.ci_hi")
        mde = _require_finite(delta["mde"], f"{cls}.mde")
        n = int(delta["n"])
        per_class[cls] = ClassFacts(
            point=point,
            ci_lo=ci_lo,
            ci_hi=ci_hi,
            mde=mde,
            n=n,
            near_parity=ci_lo >= -EPS_NEAR_PARITY,
            better=ci_lo > 0.0,
            underpowered=mde > EPS_NEAR_PARITY,
        )

    surpass_candidates = [cls for cls in MEMORY_CLASSES if per_class[cls]["better"]]

    def _result(
        verdict: Verdict,
        binding_constraint: str | None,
        blocked_by: BlockedBy | None,
    ) -> Resolution:
        return Resolution(
            verdict=verdict,
            per_class=per_class,
            binding_constraint=binding_constraint,
            surpass_candidates=surpass_candidates,
            blocked_by=blocked_by,
        )

    # 1 — hard-gate BLOCK (checked first; eu7 before latency).
    if eu7 < EU7_FLOOR:
        return _result(_NOT_REACHED, None, "eu7")
    if not latency_intact:
        return _result(_NOT_REACHED, None, "latency")

    # 2 — power guard: no parity claim on an under-powered class.
    for cls in MEMORY_CLASSES:
        if per_class[cls]["underpowered"]:
            return _result(_NOT_REACHED, f"underpowered:{cls}", None)

    # 3 — resolution: every class must clear the near-parity band.
    below = [cls for cls in MEMORY_CLASSES if not per_class[cls]["near_parity"]]
    if below:
        return _result(_NOT_REACHED, "below_parity:" + ",".join(below), None)

    return _result(_REACHED, None, None)


def probe_15a_pass(
    cand: Mapping[str, float],
    base: Mapping[str, float],
) -> bool:
    """15a embedder-ceiling probe (PRIMARY) — design §5.

    A candidate embedder *clears* iff it beats the CLS-corrected ``bge-small``
    reference on **both** eu8 relevance recall@10 and the ~596-q hard subset by a
    margin whose paired-CI **lower bound > 0**, **and** is CPU-feasible, **and** is
    1-bit-survivable (projected eu7 ≥ :data:`EU7_FLOOR`).

    ``cand`` carries ``{"eu8", "hard"}`` (candidate point estimates),
    ``{"eu8_margin_ci_lo", "hard_margin_ci_lo"}`` (the paired ``cand − base`` margin
    CI lower bounds, already computed), ``cpu_feasible`` (bool), and
    ``projected_eu7`` (float). ``base`` carries the CLS-corrected ``bge-small``
    reference points ``{"eu8", "hard"}``.

    Returns ``True`` only when every criterion holds. Non-finite floats raise.
    """
    # Validate the carried point estimates for finiteness (a malformed endpoint
    # must fail loudly) but do NOT gate on a raw cand-point > base-point check:
    # frozen design §5 (15a) gates on the paired margin CI lower bound only.
    _require_finite(cand["eu8"], "cand.eu8")
    _require_finite(cand["hard"], "cand.hard")
    _require_finite(base["eu8"], "base.eu8")
    _require_finite(base["hard"], "base.hard")
    eu8_margin_ci_lo = _require_finite(cand["eu8_margin_ci_lo"], "cand.eu8_margin_ci_lo")
    hard_margin_ci_lo = _require_finite(
        cand["hard_margin_ci_lo"], "cand.hard_margin_ci_lo"
    )
    projected_eu7 = _require_finite(cand["projected_eu7"], "cand.projected_eu7")
    cpu_feasible = bool(cand["cpu_feasible"])

    one_bit_survivable = projected_eu7 >= EU7_FLOOR
    return (
        eu8_margin_ci_lo > 0.0
        and hard_margin_ci_lo > 0.0
        and cpu_feasible
        and one_bit_survivable
    )


def probe_15b_pass(
    enriched: Mapping[str, float],
    placebo: Mapping[str, float],
) -> bool:
    """15b D2 content-at-scale proxy probe — design §5.

    Enrichment *passes* iff the fielded content beats the **length-matched
    placebo** at power (margin paired-CI **lower bound > 0**) **and** fielding
    removes the FTS length-norm penalty. FAIL ⇒ Slice 25 (D2 engine) defers/drops.

    ``enriched`` carries ``recall`` (enriched point estimate), ``margin_ci_lo``
    (the paired ``enriched − placebo`` margin CI lower bound, already computed), and
    ``removes_length_norm_penalty`` (bool). ``placebo`` carries the length-matched
    foreign-token placebo ``recall``.

    Returns ``True`` only when both criteria hold. Non-finite floats raise.
    """
    # Validate the carried point estimates for finiteness (a malformed endpoint
    # must fail loudly) but do NOT gate on a raw enriched-point > placebo-point
    # check: frozen design §5 (15b) gates on the paired margin CI lower bound only.
    _require_finite(enriched["recall"], "enriched.recall")
    _require_finite(placebo["recall"], "placebo.recall")
    margin_ci_lo = _require_finite(enriched["margin_ci_lo"], "enriched.margin_ci_lo")
    removes_penalty = bool(enriched["removes_length_norm_penalty"])

    return margin_ci_lo > 0.0 and removes_penalty


# --------------------------------------------------------------------------- #
# Pre-registration schema lint (frozen as code, imported by the Slice-0 test).
# --------------------------------------------------------------------------- #

#: The required frozen, dated fields the design doc must carry (design §5). Each
#: must appear as a ``frozen-field: <key>`` line bearing a YYYY-MM-DD date. These
#: are matched as substrings of the bolded field headers in §5.
REQUIRED_FROZEN_FIELDS_083: tuple[str, ...] = (
    "near-parity-band",
    "power-sizing rule",
    "decision-rule",
    "$0-probe pass/fail",
    "eu7-break fork",
    "surpass-option protocol",
)

#: The design must self-declare it is decision-ready.
REQUIRED_STATUS_TOKEN: str = "status: decision-ready"

_DATE_RE = re.compile(r"\b20\d\d-\d\d-\d\d\b")


def _collect_prereg_problems(doc_text: str) -> list[str]:
    """Return a list of pre-registration problems (empty == clean).

    Flags a missing/downgraded ``status: decision-ready``, any missing
    ``frozen-field: <key>`` line, or such a line present but undated. The lint is
    non-vacuous: every required field is checked for both presence and a date.
    """
    problems: list[str] = []

    if REQUIRED_STATUS_TOKEN not in doc_text:
        problems.append(
            f"missing or downgraded status (expected '{REQUIRED_STATUS_TOKEN}')"
        )

    lines = doc_text.splitlines()
    for field in REQUIRED_FROZEN_FIELDS_083:
        marker = f"frozen-field: {field}"
        line = next((ln for ln in lines if marker in ln), None)
        if line is None:
            problems.append(f"missing frozen field: {field}")
        elif not _DATE_RE.search(line):
            problems.append(f"frozen field undated: {field}")

    return problems


def lint_preregistration_083(design_md_text: str) -> None:
    """Assert the 0.8.3 design doc carries every frozen, dated pre-reg field.

    Raises :class:`AssertionError` (with the problem list) if the doc lacks
    ``status: decision-ready`` or any required ``frozen-field: <key>`` line, or if
    such a line is present but undated. Returns ``None`` on a clean doc — mirrors
    :func:`eval.m1_decision_rule.lint_preregistration` but in assert form (§9).
    """
    problems = _collect_prereg_problems(design_md_text)
    if problems:
        raise AssertionError(
            "0.8.3 pre-registration lint failed: " + "; ".join(problems)
        )
