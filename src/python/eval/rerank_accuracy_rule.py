"""Frozen pure rule for the 0.8.3 Slice-20 CE-rerank ACCURACY arm (design §3).

This is the executable, auditable core of the Slice-20 go/no-go on the **accuracy**
axis (NOT recall — the gap-decomp showed the lever is retrieval PRECISION, which
strict recall@K is blind to; design §1). It mirrors the
:mod:`eval.decision_rule_083` / :mod:`eval.gap_decomposition_rule` style — pure
stdlib, **no ``fathomdb`` / ``numpy`` / ``scipy`` import** — so it (and its test)
run anywhere, independent of the native extension build or the ``.venv`` binding.
Deterministic: no clock, no RNG, no I/O. It does NOT edit / import either of those
two sibling rules (they keep a 0-line diff).

It consumes the **already-computed** paired margin summary (the runner runs the
bootstrap; this rule stays pure) and answers, in order:

1. :func:`lever_realized` — the frozen pass criterion: the CE rerank clears iff the
   paired ``(fathomdb_reranked − fathomdb)`` **accuracy** margin CI lower bound > 0
   **AND** the class is **powered** (per-class ``mde ≤`` :data:`EPS_MDE`). An
   under-powered class is **INCONCLUSIVE**, never a silent FAIL (mirrors the
   :func:`eval.decision_rule_083.decide_083` / :func:`eval.ce_rerank_rule.probe_rerank_pass`
   power guard).
2. :func:`gap_to_mem0_closed` / :func:`oracle_headroom_captured` — the **diagnostic,
   non-gating** fractions of the Mem0 accuracy gap (resp. the oracle headroom) the
   realizable rerank captures.
3. :func:`decide_rerank_accuracy` — bundles the verdict + diagnostics + the **GO**
   flag: ``GO`` iff ``lever_realized == PASS`` AND ``gap_to_mem0_closed ≥``
   :data:`GO_GAP_CLOSED_MIN` (the rerank closes ≥ half the Mem0 accuracy gap).

Binding spec: ``dev/design/0.8.3-rerank-accuracy-arm.md`` §3.
"""

from __future__ import annotations

import math
from collections.abc import Mapping
from typing import Any, Literal, Optional

# --------------------------------------------------------------------------- #
# Frozen constants (the rule must be auditable). Date: 2026-06-22 (design §3).
# --------------------------------------------------------------------------- #

#: Per-class power floor ε on the paired MDE. A class whose minimum detectable
#: effect exceeds this cannot support a lift claim → INCONCLUSIVE. Frozen at 0.05
#: to match :data:`eval.decision_rule_083.EPS_NEAR_PARITY` /
#: :data:`eval.ce_rerank_rule.EPS_MDE` (design §3).
EPS_MDE: float = 0.05

#: The GO threshold: the rerank must close at least this fraction of the
#: ``(mem0 − fathomdb)`` accuracy gap (design §3). Frozen 2026-06-22.
GO_GAP_CLOSED_MIN: float = 0.5

LeverVerdict = Literal["PASS", "FAIL", "INCONCLUSIVE"]
_PASS: LeverVerdict = "PASS"
_FAIL: LeverVerdict = "FAIL"
_INCONCLUSIVE: LeverVerdict = "INCONCLUSIVE"


def _require_finite(value: float, name: str) -> float:
    """Return ``float(value)``; raise ``ValueError`` if it is non-finite.

    A ``NaN`` slips past every ``<`` / ``>`` comparison, so without this guard a
    malformed endpoint could silently yield a verdict. Fail loudly instead.
    """
    v = float(value)
    if not math.isfinite(v):
        raise ValueError(
            f"non-finite {name}: {value!r} (a malformed endpoint must fail loudly)"
        )
    return v


def lever_realized(margin: Mapping[str, object]) -> LeverVerdict:
    """Return the frozen lever-realized verdict for one paired accuracy margin.

    ``margin`` is the already-computed paired ``(fathomdb_reranked − fathomdb)``
    accuracy delta on a class (or pooled): ``{"point", "ci_lo", "ci_hi", "mde",
    "n"}`` (the :func:`eval.d0b_parity_run.class_delta` shape). The rule, in order:

    1. **Power guard (checked first):** ``mde`` is ``None`` (n ≤ 1 — degenerate) or
       ``mde >`` :data:`EPS_MDE` ⇒ ``"INCONCLUSIVE"``. No lift/fail claim on an
       under-powered class (mirrors :func:`decide_083`'s power guard).
    2. **Lift:** ``ci_lo > 0`` ⇒ ``"PASS"`` (the reranker lifts accuracy at power).
    3. Otherwise ⇒ ``"FAIL"``.

    Deterministic: same input → same verdict. ``ci_lo`` (and ``mde`` when present)
    must be finite or :class:`ValueError` is raised (fail loudly on a malformed
    endpoint). A missing required key raises :class:`KeyError`.
    """
    raw_mde = margin["mde"]
    # Power guard first: a degenerate (n<=1 ⇒ mde None) or under-powered class is
    # INCONCLUSIVE, never a silent FAIL.
    if raw_mde is None:
        return _INCONCLUSIVE
    mde = _require_finite(raw_mde, "margin.mde")  # type: ignore[arg-type]
    if mde > EPS_MDE:
        return _INCONCLUSIVE

    ci_lo = _require_finite(margin["ci_lo"], "margin.ci_lo")  # type: ignore[arg-type]
    return _PASS if ci_lo > 0.0 else _FAIL


def gap_to_mem0_closed(
    acc_reranked: Optional[float],
    acc_fathomdb: Optional[float],
    acc_mem0: Optional[float],
) -> Optional[float]:
    """Diagnostic fraction of the Mem0 accuracy gap the rerank closes (design §3).

    ``gap_to_mem0_closed = (acc_reranked − acc_fathomdb) / (acc_mem0 − acc_fathomdb)``
    — how much of the FathomDB→Mem0 accuracy gap the realizable rerank recovers.
    **Non-gating** input to the GO flag only.

    Returns ``None`` (never a fabricated number) when any input is ``None`` (an arm
    absent from the reused cells) or the ``(mem0 − fathomdb)`` denominator is ~0 (no
    measurable gap to close). The present inputs must be finite; a non-finite value
    raises.
    """
    if acc_reranked is None or acc_fathomdb is None or acc_mem0 is None:
        return None
    rr = _require_finite(acc_reranked, "acc_reranked")
    fb = _require_finite(acc_fathomdb, "acc_fathomdb")
    mm = _require_finite(acc_mem0, "acc_mem0")
    denom = mm - fb
    if abs(denom) < 1e-12:
        return None
    return (rr - fb) / denom


def oracle_headroom_captured(
    acc_reranked: Optional[float],
    acc_fathomdb: Optional[float],
    acc_oracle_raw: Optional[float],
) -> Optional[float]:
    """Diagnostic fraction of the oracle headroom the rerank captures (design §3).

    ``oracle_headroom_captured = (acc_reranked − acc_fathomdb) /
    (acc_oracle_raw − acc_fathomdb)`` — how much of the perfect-raw-gold accuracy
    upper bound the realizable rerank recovers. **Non-gating** (reported only).

    Returns ``None`` when any input is ``None`` (the gap-decomp ``oracle_raw`` cells
    are absent) or the ``(oracle_raw − fathomdb)`` denominator is ~0. The present
    inputs must be finite; a non-finite value raises.
    """
    if acc_reranked is None or acc_fathomdb is None or acc_oracle_raw is None:
        return None
    rr = _require_finite(acc_reranked, "acc_reranked")
    fb = _require_finite(acc_fathomdb, "acc_fathomdb")
    orc = _require_finite(acc_oracle_raw, "acc_oracle_raw")
    denom = orc - fb
    if abs(denom) < 1e-12:
        return None
    return (rr - fb) / denom


def decide_rerank_accuracy(
    margin: Mapping[str, object],
    *,
    acc_reranked: Optional[float],
    acc_fathomdb: Optional[float],
    acc_mem0: Optional[float] = None,
    acc_oracle_raw: Optional[float] = None,
) -> dict[str, Any]:
    """Return the frozen Slice-20 accuracy-arm decision block (design §3).

    Bundles :func:`lever_realized` (the gated verdict) with the two non-gating
    diagnostics and the **GO** flag. ``GO`` is ``True`` iff
    ``lever_realized == "PASS"`` AND ``gap_to_mem0_closed`` is a real number ≥
    :data:`GO_GAP_CLOSED_MIN` (the rerank closes ≥ half the Mem0 accuracy gap). A
    ``None`` ``gap_to_mem0_closed`` (mem0 cells absent / no measurable gap) can never
    be GO — the headroom must be *demonstrably* ≥ half-closed, never assumed.

    Deterministic: same input → same block.
    """
    verdict = lever_realized(margin)
    gap_closed = gap_to_mem0_closed(acc_reranked, acc_fathomdb, acc_mem0)
    oracle_cap = oracle_headroom_captured(acc_reranked, acc_fathomdb, acc_oracle_raw)
    go = verdict == _PASS and gap_closed is not None and gap_closed >= GO_GAP_CLOSED_MIN
    return {
        "lever_realized": verdict,
        "gap_to_mem0_closed": gap_closed,
        "oracle_headroom_captured": oracle_cap,
        "go": go,
    }
