"""Frozen GO/NO-GO decision rule for the 0.8.2 / M1 multi-hop adjudication.

This module is the *executable core* of the Slice-0 pre-registration. It freezes
two things as code, before any data is seen, so a downstream slice cannot post-hoc
switch the endpoint:

1. :func:`decide` — the GO/NO-GO computation over the **amended** primary endpoint
   (pooled ≥3-hop ΔF1 vs the fixed ``fused-RRF`` comparator (k=60; AMENDED
   2026-06-19 from ``fused+rerank`` — data shows fused-RRF best composition), with a
   trend gate, a CI-banded EM gate, and an unanswerable-set confident-wrong guard).
   **Slice 20 imports this; it may not redefine the rule.**
2. :func:`lint_preregistration` — the schema lint asserting the design doc
   ``dev/design/0.8.2-m1-multihop-harness.md`` carries the required frozen, dated
   pre-registration fields.

**Amended 2026-06-16 (HITL — all 6 pre-freeze methodology-review amendments;
``dev/plans/runs/0.8.2-slice-0-prereg-methodology-review.md``).** The original
strict-monotonic ``f1[2] < f1[3] < f1[4]`` dose-response gate and per-hop-max
baseline biased the rule toward the expected NO_GO. They are replaced by a trend
gate (veto only on a *significantly negative* slope), a single fixed comparator,
a pooled ≥3-hop material gate with a paired-bootstrap CI lower bound > 0, a
whole-rule power simulation, and a CI-banded EM gate. ``decide`` now consumes
**already-computed summary statistics** (the Slice-20 harness runs the paired
bootstrap; this function stays pure/deterministic — no RNG, no I/O).

Binding spec: ``dev/design/0.8.2-m1-multihop-harness.md`` §4 (pre-registration),
``dev/plans/plan-0.8.2.md`` §4 (Slice 0 contract, AMENDED 2026-06-16).

Pure stdlib — **no ``fathomdb`` / ``scipy`` / ``networkx`` import** — so it (and
its test) run anywhere, independent of the native-extension build or the ``.venv``
binding. Deterministic: no clock, no RNG, no I/O.
"""

from __future__ import annotations

import math
import re
from collections.abc import Mapping
from typing import Literal

# --------------------------------------------------------------------------- #
# Frozen constants (the rule must be auditable). See design §4 (frozen-field:
# decision-rule / mde-power-plan).
# --------------------------------------------------------------------------- #

#: Smallest pooled ≥3-hop ΔF1 lift worth a GO (4 F1 points). Frozen **at/above**
#: the Slice-5 pooled ≥3-hop MDE — the material threshold must sit at or above the
#: MDE (the old "sized below MDE" wording was backwards; amendment 3).
#: **AMENDED 2026-06-19 (HITL):** 0.04 = ceil(conservative ρ=0.5 MDE 0.037),
#: corpus-feasible at N=1165 (`runs/0.8.2-m1-bridge-vs-answer-diagnostic.md`).
MATERIAL_F1_LIFT: float = 0.04

Verdict = Literal["GO", "NO_GO"]
_GO: Verdict = "GO"
_NO_GO: Verdict = "NO_GO"


def _require_finite(value: float, name: str) -> float:
    """Return ``float(value)``; raise ``ValueError`` if it is non-finite.

    A ``NaN`` slips past every ``<`` / ``>=`` comparison (``nan < x`` and
    ``nan >= x`` are both False), so without this guard a malformed endpoint could
    silently yield a verdict. A malformed endpoint must fail loudly instead (the
    Slice-0 fix-1 guard; kept across the amendment).
    """
    v = float(value)
    if not math.isfinite(v):
        raise ValueError(
            f"non-finite {name}: {value!r} (a malformed endpoint must fail loudly)"
        )
    return v


def decide(
    material: Mapping[str, float],
    em: Mapping[str, float],
    trend: Mapping[str, bool],
    confident_wrong: Mapping[str, bool],
    power_ok: bool,
) -> Verdict:
    """Return the frozen GO/NO-GO verdict for the **amended** M1 primary endpoint.

    All arguments are **already-computed summary statistics** (the Slice-20
    harness runs the question-level paired bootstrap; this function is a pure,
    deterministic gate — no RNG, no I/O):

    * ``material`` = ``{"f1_delta", "f1_ci_low"}`` — the pooled ≥3-hop (hops 3+4)
      ΔF1 of ``ppr-fusion`` vs the fixed ``fused-RRF (k=60)`` comparator (AMENDED
      2026-06-19), and its paired-bootstrap CI **lower** bound.
    * ``em`` = ``{"ci_high"}`` — the pooled ≥3-hop ΔEM CI **upper** bound.
    * ``trend`` = ``{"neg_significant"}`` — is the ΔF1-vs-hop slope *significantly
      negative*?
    * ``confident_wrong`` = ``{"increase_significant"}`` — did ppr-fusion
      *significantly* raise the unanswerable-set confident-answer rate?
    * ``power_ok`` — did the whole-rule power simulation clear ≥0.8 P(GO) under the
      flat-positive +0.03 shape (set by Slice 5)?

    The rule (design §4.1 truth table) — **GO** iff *all* of:

    * **material** — ``f1_delta >=`` :data:`MATERIAL_F1_LIFT` **and**
      ``f1_ci_low > 0`` (material lift *and* the CI excludes 0);
    * **trend not significantly negative** — ``not trend["neg_significant"]``
      (veto **only** on a significantly negative ΔF1-vs-hop slope; a flat or
      positive slope passes — there is **no** strict-monotonic requirement);
    * **EM not significantly worse** — ``em["ci_high"] >= 0`` (CI-banded, not a
      point-estimate veto);
    * **no confident-wrong increase** — ``not confident_wrong["increase_significant"]``
      (the unanswerable-set confident-answer rate carries this role, not EM);
    * **adequately powered** — ``power_ok`` is True.

    Otherwise **NO_GO**. F1 is the primary continuous signal. Deterministic: same
    input → same verdict.

    Raises :class:`KeyError` if a required key is missing, and :class:`ValueError`
    if any float in ``material`` / ``em`` is non-finite (``NaN`` / ``±inf``) — a
    malformed endpoint must fail loudly, never silently return a verdict.
    """
    # Validate the float-bearing fields up front — fail loudly on a malformed
    # endpoint (KeyError if a key is missing; ValueError if a value is non-finite).
    f1_delta = _require_finite(material["f1_delta"], "material.f1_delta")
    f1_ci_low = _require_finite(material["f1_ci_low"], "material.f1_ci_low")
    em_ci_high = _require_finite(em["ci_high"], "em.ci_high")

    # The boolean signals (KeyError if missing). Coerced to bool for safety.
    trend_neg_significant = bool(trend["neg_significant"])
    confident_wrong_increase = bool(confident_wrong["increase_significant"])

    # Gate 1 — material pooled ≥3-hop F1 lift AND its CI excludes 0.
    if f1_delta < MATERIAL_F1_LIFT or f1_ci_low <= 0.0:
        return _NO_GO

    # Gate 2 — trend gate: veto ONLY on a significantly negative ΔF1-vs-hop slope.
    if trend_neg_significant:
        return _NO_GO

    # Gate 3 — EM not significantly worse (CI-banded, not a point-estimate veto).
    if em_ci_high < 0.0:
        return _NO_GO

    # Gate 4 — confident-wrong guard (unanswerable-set confident-answer rate).
    if confident_wrong_increase:
        return _NO_GO

    # Gate 5 — adequate whole-rule power.
    if not power_ok:
        return _NO_GO

    return _GO


# --------------------------------------------------------------------------- #
# Pre-registration schema lint (frozen as code, imported by the Slice-0 test).
# --------------------------------------------------------------------------- #

#: The required frozen, dated fields the design doc must carry (design §4, AMENDED
#: 2026-06-16). Each must appear as a ``frozen-field: <key>`` line bearing a
#: YYYY-MM-DD date. The amendment replaced ``per-hop-strata`` with the fixed
#: ``comparator`` (= fused+rerank) and ``baseline-arms`` (incl. RRF k=60), and
#: re-scoped ``mde-power-plan`` to the whole-rule power simulation.
REQUIRED_FROZEN_FIELDS: tuple[str, ...] = (
    "primary-endpoint",
    "comparator",
    "decision-rule",
    "baseline-arms",
    "mde-power-plan",
    "trend-test",
)

#: The design must self-declare it is decision-ready.
REQUIRED_STATUS_TOKEN: str = "status: decision-ready"

_DATE_RE = re.compile(r"\b20\d\d-\d\d-\d\d\b")


def lint_preregistration(doc_text: str) -> list[str]:
    """Return a list of pre-registration problems (empty == clean).

    Fails if the doc lacks ``status: decision-ready`` or any required
    ``frozen-field: <key>`` line, or if such a line is present but undated. The
    Slice-0 test asserts the list is empty for the real design doc and non-empty
    for mutated copies (missing / undated field, downgraded status).
    """
    problems: list[str] = []

    if REQUIRED_STATUS_TOKEN not in doc_text:
        problems.append(f"missing or downgraded status (expected '{REQUIRED_STATUS_TOKEN}')")

    lines = doc_text.splitlines()
    for field in REQUIRED_FROZEN_FIELDS:
        marker = f"frozen-field: {field}"
        line = next((ln for ln in lines if marker in ln), None)
        if line is None:
            problems.append(f"missing frozen field: {field}")
        elif not _DATE_RE.search(line):
            problems.append(f"frozen field undated: {field}")

    return problems
