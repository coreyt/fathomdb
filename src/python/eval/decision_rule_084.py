"""Frozen RESOLUTION decision rule for the 0.8.4 GraphRAG-parity gate.

This module is the *executable core* of the 0.8.4 Slice-0 pre-registration. It
freezes — before any number is seen, so a downstream slice (5/10/15/20) cannot
post-hoc switch the endpoint — the win-rate resolution rule, the four LLM-judge
bias controls (as hard preconditions), and the pre-registration schema lint.

The 0.8.4 axis is **global sensemaking**, scored by a BenchmarkQED-style **pairwise
LLM-judge win-rate** (reference-free) — NOT a reference-based per-class accuracy
delta (that was 0.8.3 / ``decide_083``). So the rule lives on the **win-rate
scale** where exact parity is ``0.5`` (a coin flip): "near-parity-or-better" on a
metric = the S1-vs-GraphRAG paired win-rate **CI lower bound ≥ 0.5 − ε_wr**;
"better" = lower bound **> 0.5**.

Frozen as code:

1. :func:`decide_084` — the **REACHED / NOT_REACHED** computation over the primary
   ``S1 vs GraphRAG`` per-metric (comprehensiveness / diversity / empowerment)
   pairwise win-rates, with the **bias-control BLOCK gates** (a win-rate without
   the four controls applied is not a valid measurement), a per-metric power guard,
   the length-confound guard, and the near-parity band ε_wr. **Slices 5/15/20
   import this; they may not redefine the rule.**
2. :func:`strong_baseline_clears` — the Slice-5 pilot **kill-early** predicate: if
   S1 does NOT clear near-parity against the **long-context** control (the honest
   upper bar for a corpus that fits the window — the "VectorRAG is almost enough"
   prior), escalate to the HITL **before** funding the community build (design §6).
3. :func:`honest_prior_cleared` — the Slice-0 **design-review gate** as code: the
   cross-graph prior is strongly negative (M1 NO-GO, M2 dropped), so S1 — the
   largest build of the program — only funds if its design is distinct from the
   refuted multi-hop prior, is judged against a strong baseline, freezes kill-early
   criteria, and has an approved budget (design §2 "Honest expectation").
4. :func:`lint_preregistration_084` — the schema lint asserting the design doc
   ``dev/design/0.8.4-graphrag-sensemaking.md`` carries every required frozen,
   dated pre-registration field.

``decide_084`` consumes **already-computed summary statistics** — the AutoE harness
runs the bootstrap over questions (clustered by question, ≥5 runs, order-swapped);
this function stays pure/deterministic (no RNG, no I/O).

Binding spec: ``dev/design/0.8.4-graphrag-sensemaking.md`` §5 (frozen
pre-registration) and §9 (this module's contract); ``dev/plans/plan-0.8.4.md`` §4
(Slice 0).

Mirrors :mod:`eval.decision_rule_083`'s style — pure stdlib, **no ``fathomdb`` /
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
# Frozen constants (the rule must be auditable). See design §5 (frozen fields).
# --------------------------------------------------------------------------- #

#: Exact parity on the pairwise-win-rate scale. A win-rate counts ties as 0.5, so
#: a true coin-flip between S1 and GraphRAG is exactly ``0.5``. Frozen.
PARITY: float = 0.5

#: Near-parity band ε_wr (5 percentage points on the win-rate scale). Frozen
#: 2026-06-23 (design §5; do NOT change downstream). "Near-parity-or-better" on a
#: metric = the S1-vs-GraphRAG paired win-rate **CI lower bound ≥ PARITY − ε_wr**
#: (i.e. ≥ 0.45); "better" = lower bound **> PARITY** (> 0.50).
EPS_WIN_RATE: float = 0.05

#: The stochasticity control: ≥5 independent runs per comparison (arXiv on LLM-judge
#: variance; GraphRAG/BenchmarkQED practice). Fewer ⇒ the win-rate is not a valid
#: measurement → BLOCK. Frozen.
MIN_RUNS: int = 5

#: The three BenchmarkQED / GraphRAG ("From Local to Global", arXiv:2404.16130)
#: headline sensemaking metrics, judged independently. The resolution requires
#: *every* headline metric to clear the near-parity band (the conservative bar — a
#: metric-averaged win-rate would hide a weak dimension). ``directness`` is NOT
#: here: it is the **length-bias corroboration**, not a headline win axis.
HEADLINE_METRICS: tuple[str, ...] = (
    "comprehensiveness",
    "diversity",
    "empowerment",
)

Verdict = Literal["REACHED", "NOT_REACHED"]
_REACHED: Verdict = "REACHED"
_NOT_REACHED: Verdict = "NOT_REACHED"

#: The bias-control BLOCK reasons (the four LLM-judge failure modes + the length
#: corroboration being run). A blocked verdict never makes a parity claim.
BlockedBy = Literal[
    "bias:position",
    "bias:stochasticity",
    "bias:self_preference",
    "bias:length_missing",
]


class MetricFacts(TypedDict):
    """Per-metric echo of the win-rate delta plus the three derived gate flags."""

    win_rate: float
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
    per_metric: dict[str, MetricFacts]
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
        raise ValueError(f"non-finite {name}: {value!r} (a malformed endpoint must fail loudly)")
    return v


def _check_bias_controls(
    bias_controls: Mapping[str, object],
    length_corroboration: Mapping[str, object],
) -> BlockedBy | None:
    """Return the first failing bias-control BLOCK reason, or ``None`` if clean.

    The four LLM-judge failure modes are **preconditions**, not findings — a
    win-rate produced without them is not a measurement (design §1). Order of
    checks is fixed for determinism:

    1. **Position bias** — ``order_swapped`` must be True (answer order
       randomized/swapped and averaged; arXiv:2406.07791).
    2. **Stochasticity** — ``n_runs`` ≥ :data:`MIN_RUNS` (report variance, not a
       point).
    3. **Self-preference** — the ``judge_family`` must NOT be among
       ``system_families`` (the judge is from a different model family than ANY
       system under test; arXiv:2410.21819).
    4. **Length corroboration present** — the directness / claim-count non-judge
       corroboration must have been **run** (``length_corroboration["ran"]``); a
       *contradicting* corroboration is handled later (a power/parity-level
       constraint, not a missing-control block).
    """
    if not bool(bias_controls["order_swapped"]):
        return "bias:position"
    if int(bias_controls["n_runs"]) < MIN_RUNS:
        return "bias:stochasticity"

    judge_family = str(bias_controls["judge_family"]).strip().lower()
    system_families = {str(fam).strip().lower() for fam in bias_controls["system_families"]}
    if judge_family in system_families:
        return "bias:self_preference"

    if not bool(length_corroboration["ran"]):
        return "bias:length_missing"

    return None


def decide_084(
    primary_per_metric: Mapping[str, Mapping[str, float]],
    bias_controls: Mapping[str, object],
    length_corroboration: Mapping[str, object],
) -> Resolution:
    """Return the frozen RESOLUTION verdict for the 0.8.4 GraphRAG-parity gate.

    ``primary_per_metric`` maps **each** of :data:`HEADLINE_METRICS` to an
    already-computed pairwise win-rate ``{"win_rate", "ci_lo", "ci_hi", "mde",
    "n"}`` — the **S1-vs-GraphRAG** paired win-rate on that metric (ties counted as
    0.5; aggregated across ≥5 order-swapped runs, bootstrapped over questions) with
    its 95% CI bounds, the per-metric win-rate MDE, and the comparison N.

    ``bias_controls`` carries ``order_swapped`` (bool), ``n_runs`` (int),
    ``judge_family`` (str), and ``system_families`` (iterable of str — the families
    of EVERY system under test: S1's answerer, GraphRAG's answerer, the baselines).
    ``length_corroboration`` carries ``ran`` (bool) and ``contradicts`` (bool — the
    directness/claim-count control indicates S1's apparent win is a verbosity
    artifact).

    The rule (design §5 decision-rule, §9 truth table) — in order:

    1. **BLOCK (bias controls, checked first):** any of the four LLM-judge controls
       not satisfied ⇒ ``NOT_REACHED`` with the matching ``blocked_by`` (a block
       never makes a parity claim; ``binding_constraint`` stays ``None``).
    2. **Power guard:** any headline metric with ``mde >`` :data:`EPS_WIN_RATE` ⇒
       ``NOT_REACHED`` with ``binding_constraint="underpowered:<metric>"`` (the
       first such metric in :data:`HEADLINE_METRICS` order) — no parity claim on an
       under-powered metric (escalate N, the Slice-0 reserved follow-on).
    3. **Length-confound guard:** ``length_corroboration["contradicts"]`` ⇒
       ``NOT_REACHED`` with ``binding_constraint="length_confounded"`` — a win
       explained by verbosity is not a sensemaking win (the GraphRAG paper's
       non-judge control).
    4. **Resolution:** ``REACHED`` iff *every* headline metric clears the near-parity
       band (``ci_lo >= PARITY - EPS_WIN_RATE``); otherwise ``NOT_REACHED`` with
       ``binding_constraint="below_parity:<metric>,<metric>..."`` (the mechanical
       failing-metric identifier; the HITL names the *semantic* binding constraint —
       judge variance / community-summary quality / extractor ceiling — per design §5).

    ``surpass_candidates`` is always the metrics whose ``ci_lo > PARITY`` (strictly
    better than GraphRAG). Deterministic: same input → same Resolution.

    Raises :class:`KeyError` if a metric, a control, or a win-rate key is missing,
    and :class:`ValueError` if any float is non-finite — a malformed endpoint must
    fail loudly, never silently return a verdict.
    """
    # Echo + derive every metric up front (KeyError if a metric/key is missing;
    # ValueError if any float is non-finite) — fail loudly on a malformed endpoint.
    # Done BEFORE the block check so a malformed endpoint still raises even when a
    # control is also absent (a bad number must never hide behind a block).
    per_metric: dict[str, MetricFacts] = {}
    for metric in HEADLINE_METRICS:
        wr = primary_per_metric[metric]
        win_rate = _require_finite(wr["win_rate"], f"{metric}.win_rate")
        ci_lo = _require_finite(wr["ci_lo"], f"{metric}.ci_lo")
        ci_hi = _require_finite(wr["ci_hi"], f"{metric}.ci_hi")
        mde = _require_finite(wr["mde"], f"{metric}.mde")
        n = int(wr["n"])
        per_metric[metric] = MetricFacts(
            win_rate=win_rate,
            ci_lo=ci_lo,
            ci_hi=ci_hi,
            mde=mde,
            n=n,
            near_parity=ci_lo >= PARITY - EPS_WIN_RATE,
            better=ci_lo > PARITY,
            underpowered=mde > EPS_WIN_RATE,
        )

    surpass_candidates = [m for m in HEADLINE_METRICS if per_metric[m]["better"]]

    def _result(
        verdict: Verdict,
        binding_constraint: str | None,
        blocked_by: BlockedBy | None,
    ) -> Resolution:
        return Resolution(
            verdict=verdict,
            per_metric=per_metric,
            binding_constraint=binding_constraint,
            surpass_candidates=surpass_candidates,
            blocked_by=blocked_by,
        )

    # 1 — bias-control BLOCK (a win-rate without the four controls is not a measurement).
    blocked = _check_bias_controls(bias_controls, length_corroboration)
    if blocked is not None:
        return _result(_NOT_REACHED, None, blocked)

    # 2 — power guard: no parity claim on an under-powered metric.
    for metric in HEADLINE_METRICS:
        if per_metric[metric]["underpowered"]:
            return _result(_NOT_REACHED, f"underpowered:{metric}", None)

    # 3 — length-confound guard: a verbosity-explained win is not a sensemaking win.
    if bool(length_corroboration["contradicts"]):
        return _result(_NOT_REACHED, "length_confounded", None)

    # 4 — resolution: every headline metric must clear the near-parity band.
    below = [m for m in HEADLINE_METRICS if not per_metric[m]["near_parity"]]
    if below:
        return _result(_NOT_REACHED, "below_parity:" + ",".join(below), None)

    return _result(_REACHED, None, None)


def strong_baseline_clears(s1_vs_long_context: Mapping[str, float]) -> bool:
    """Slice-5 **kill-early** predicate (design §6) — the honest upper-bar check.

    The long-context "stuff-it-all-in" control is the honest upper bar for a corpus
    that fits the window (the Samsung "VectorRAG is almost enough" prior). If S1
    does **not** clear near-parity against it in the Slice-5 pilot, the community
    build is likely not worth funding — escalate to the HITL **before** the spend.

    ``s1_vs_long_context`` carries ``ci_lo`` (the S1-vs-long-context paired win-rate
    CI lower bound). Returns ``True`` iff S1 clears the near-parity band against the
    long-context control (``ci_lo >= PARITY - EPS_WIN_RATE``); ``False`` ⇒ ESCALATE.
    Non-finite floats raise.
    """
    ci_lo = _require_finite(s1_vs_long_context["ci_lo"], "s1_vs_long_context.ci_lo")
    return ci_lo >= PARITY - EPS_WIN_RATE


#: The Slice-0 honest-prior design-review checklist (design §2). Each must hold for
#: the largest build of the program (S1) to fund — the cross-graph prior is strongly
#: negative, so a clean PASS here is the load-bearing gate, not a rubber stamp.
HONEST_PRIOR_CRITERIA: tuple[str, ...] = (
    "distinct_from_multihop_prior",  # S1 = community-summary synthesis, a different
    #                                  mechanism+axis than the refuted PPR/BFS traversal.
    "strong_baseline_planned",  # judged vs vector-RAG AND a long-context control.
    "kill_early_criteria_frozen",  # Slice-5 pilot escalation + Slice-15 baseline check.
    "budget_approved",  # the aggregate judged-run budget is HITL-approved before spend.
)


def honest_prior_cleared(design_review: Mapping[str, object]) -> bool:
    """Return ``True`` iff every :data:`HONEST_PRIOR_CRITERIA` flag holds.

    Encodes the Slice-0 design-review gate (design §2 "Honest expectation") as code
    so the funding decision is auditable: S1 funds only if its design is distinct
    from the refuted multi-hop prior, is judged against a strong baseline, freezes
    kill-early criteria, and has an approved budget. A missing key raises
    :class:`KeyError` — every criterion must be explicitly answered.
    """
    return all(bool(design_review[k]) for k in HONEST_PRIOR_CRITERIA)


# --------------------------------------------------------------------------- #
# Pre-registration schema lint (frozen as code, imported by the Slice-0 test).
# --------------------------------------------------------------------------- #

#: The required frozen, dated fields the design doc must carry (design §5). Each
#: must appear as a ``frozen-field: <key>`` line bearing a YYYY-MM-DD date. These
#: are matched as substrings of the bolded field headers in §5.
REQUIRED_FROZEN_FIELDS_084: tuple[str, ...] = (
    "near-parity-band",
    "power-sizing rule",
    "decision-rule",
    "bias-controls",
    "surpass-option protocol",
    "honest-prior gate",
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
        problems.append(f"missing or downgraded status (expected '{REQUIRED_STATUS_TOKEN}')")

    lines = doc_text.splitlines()
    for field in REQUIRED_FROZEN_FIELDS_084:
        marker = f"frozen-field: {field}"
        line = next((ln for ln in lines if marker in ln), None)
        if line is None:
            problems.append(f"missing frozen field: {field}")
        elif not _DATE_RE.search(line):
            problems.append(f"frozen field undated: {field}")

    return problems


def lint_preregistration_084(design_md_text: str) -> None:
    """Assert the 0.8.4 design doc carries every frozen, dated pre-reg field.

    Raises :class:`AssertionError` (with the problem list) if the doc lacks
    ``status: decision-ready`` or any required ``frozen-field: <key>`` line, or if
    such a line is present but undated. Returns ``None`` on a clean doc — mirrors
    :func:`eval.decision_rule_083.lint_preregistration_083` (§9).
    """
    problems = _collect_prereg_problems(design_md_text)
    if problems:
        raise AssertionError("0.8.4 pre-registration lint failed: " + "; ".join(problems))
