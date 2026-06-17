"""Frozen GO/NO-GO decision rule for the 0.8.2 / M1 multi-hop adjudication.

This module is the *executable core* of the Slice-0 pre-registration. It freezes
two things as code, before any data is seen, so a downstream slice cannot post-hoc
switch the endpoint:

1. :func:`decide` — the GO/NO-GO computation over the per-hop ΔEM/ΔF1 endpoint.
   **Slice 20 imports this; it may not redefine the rule.**
2. :func:`lint_preregistration` — the schema lint asserting the design doc
   ``dev/design/0.8.2-m1-multihop-harness.md`` carries the required frozen, dated
   pre-registration fields.

Binding spec: ``dev/design/0.8.2-m1-multihop-harness.md`` §4 (pre-registration),
``dev/plans/plan-0.8.2.md`` §4 (Slice 0 contract).

Pure stdlib — **no ``fathomdb`` / ``scipy`` / ``networkx`` import** — so it (and
its test) run anywhere, independent of the native-extension build or the ``.venv``
binding. Deterministic: no clock, no RNG, no I/O.
"""

from __future__ import annotations

import re
from collections.abc import Mapping
from typing import Literal

# --------------------------------------------------------------------------- #
# Frozen constants (the rule must be auditable). See design §4 (frozen-field:
# decision-rule / mde-power-plan).
# --------------------------------------------------------------------------- #

#: The MuSiQue hop strata the endpoint is stratified by; the ≥3-hop region
#: (3 and 4) is the graph-favored test region, hop-2 anchors the dose-response.
REQUIRED_HOPS: tuple[int, int, int] = (2, 3, 4)

#: Smallest ≥3-hop F1 lift worth a GO (2 F1 points). Slice 5 sizes N so the
#: per-hop MDE falls below this.
MATERIAL_F1_LIFT: float = 0.02

#: ΔEM non-regression floor on the ≥3-hop strata (confident-wrong guard): the
#: graph arm must not reduce exact-match on the strata it claims to help.
EM_MIN_LIFT_3PLUS: float = 0.0

Verdict = Literal["GO", "NO_GO"]
_GO: Verdict = "GO"
_NO_GO: Verdict = "NO_GO"

#: Hop strata in the graph-favored region (≥3-hop) the rule evaluates for a lift.
_THREE_PLUS_HOPS: tuple[int, ...] = tuple(h for h in REQUIRED_HOPS if h >= 3)


def decide(deltas_by_hop: Mapping[int, Mapping[str, float]], power_ok: bool) -> Verdict:
    """Return the frozen GO/NO-GO verdict for the M1 primary endpoint.

    ``deltas_by_hop`` maps each hop count in :data:`REQUIRED_HOPS` to a mapping
    ``{"em": ΔEM, "f1": ΔF1}``, where Δ = ``(ppr-fusion) − (best baseline)`` on the
    MuSiQue-Ans answerable set for that hop count.

    The rule (design §4.1 truth table) — **GO** iff *all* of:

    * **material** ≥3-hop F1 lift — ``ΔF1 ≥`` :data:`MATERIAL_F1_LIFT` on **every**
      ≥3-hop stratum (hop-3 and hop-4);
    * **dose-responsive** — ΔF1 strictly grows across hops ``2 < 3 < 4``;
    * **EM-non-regressing** — ``ΔEM ≥`` :data:`EM_MIN_LIFT_3PLUS` on every ≥3-hop
      stratum (confident-wrong guard);
    * **adequately powered** — ``power_ok`` is True.

    Otherwise **NO_GO**. F1 is the primary continuous signal; EM is a coarse
    corroborating guard. Deterministic: same input → same verdict.

    Raises :class:`KeyError` if a required hop or metric is missing (a malformed
    endpoint must fail loudly, never silently return a verdict).
    """
    # Validate shape up front — fail loudly on a malformed endpoint.
    f1: dict[int, float] = {}
    em: dict[int, float] = {}
    for hop in REQUIRED_HOPS:
        bucket = deltas_by_hop[hop]  # KeyError if a hop is missing
        f1[hop] = float(bucket["f1"])  # KeyError if a metric is missing
        em[hop] = float(bucket["em"])

    # Gate 1 — material positive F1 lift on every ≥3-hop stratum.
    if any(f1[hop] < MATERIAL_F1_LIFT for hop in _THREE_PLUS_HOPS):
        return _NO_GO

    # Gate 1b — EM non-regression on the ≥3-hop strata (confident-wrong guard).
    if any(em[hop] < EM_MIN_LIFT_3PLUS for hop in _THREE_PLUS_HOPS):
        return _NO_GO

    # Gate 2 — dose-responsive: F1 lift strictly grows 2 < 3 < 4.
    if not (f1[2] < f1[3] < f1[4]):
        return _NO_GO

    # Gate 3 — adequate power.
    if not power_ok:
        return _NO_GO

    return _GO


# --------------------------------------------------------------------------- #
# Pre-registration schema lint (frozen as code, imported by the Slice-0 test).
# --------------------------------------------------------------------------- #

#: The required frozen, dated fields the design doc must carry (design §4). Each
#: must appear as a ``frozen-field: <key>`` line bearing a YYYY-MM-DD date.
REQUIRED_FROZEN_FIELDS: tuple[str, ...] = (
    "primary-endpoint",
    "per-hop-strata",
    "decision-rule",
    "mde-power-plan",
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
