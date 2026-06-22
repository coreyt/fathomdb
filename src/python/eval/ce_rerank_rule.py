"""Frozen pure rule for the 0.8.3 CE-rerank precision probe (design §3).

This is the executable, auditable core of the CE-rerank go/no-go. It mirrors the
:mod:`eval.decision_rule_083` style — pure stdlib, **no ``fathomdb`` / ``numpy`` /
``scipy`` import** — so it (and its test) run anywhere, independent of the native
extension build or the ``.venv`` binding. Deterministic: no clock, no RNG, no I/O.

It consumes the **already-computed** paired margin summary (the probe runner runs
the bootstrap; this rule stays pure) and answers two things:

1. :func:`probe_rerank_pass` — the frozen pass criterion: the CE reranker clears
   iff the paired ``(fathomdb_rerank − fathomdb)`` Recall@10 **margin CI lower
   bound > 0** AND the class is **powered** (per-class ``mde ≤`` :data:`EPS_MDE`).
   An under-powered class is **INCONCLUSIVE**, never a silent FAIL (mirrors the
   :func:`eval.decision_rule_083.decide_083` power guard).
2. :func:`headroom_captured` — the **diagnostic, non-gating** fraction of the
   gap-decomposition oracle headroom the realizable CE rerank captures.

Binding spec: ``dev/design/0.8.3-ce-rerank-precision-probe.md`` §3.
"""

from __future__ import annotations

import math
from collections.abc import Mapping
from typing import Literal, Optional

# --------------------------------------------------------------------------- #
# Frozen constants (the rule must be auditable). Date: 2026-06-22 (design §3).
# --------------------------------------------------------------------------- #

#: Per-class power floor ε on the paired MDE. A class whose minimum detectable
#: effect exceeds this cannot support a parity/lift claim → INCONCLUSIVE. Frozen at
#: 0.05 to match :data:`eval.decision_rule_083.EPS_NEAR_PARITY` (design §3).
EPS_MDE: float = 0.05

ProbeVerdict = Literal["PASS", "FAIL", "INCONCLUSIVE"]
_PASS: ProbeVerdict = "PASS"
_FAIL: ProbeVerdict = "FAIL"
_INCONCLUSIVE: ProbeVerdict = "INCONCLUSIVE"


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


def probe_rerank_pass(margin: Mapping[str, object]) -> ProbeVerdict:
    """Return the frozen CE-rerank probe verdict for one paired margin.

    ``margin`` is the already-computed paired ``(fathomdb_rerank − fathomdb)``
    Recall@10 delta on a class (or pooled): ``{"point", "ci_lo", "ci_hi", "mde",
    "n"}`` (the :func:`eval.d0b_parity_run.class_delta` shape). The rule, in order:

    1. **Power guard (checked first):** ``mde`` is ``None`` (n ≤ 1 — degenerate) or
       ``mde >`` :data:`EPS_MDE` ⇒ ``"INCONCLUSIVE"``. No lift/fail claim on an
       under-powered class (mirrors :func:`decide_083`'s power guard).
    2. **Lift:** ``ci_lo > 0`` ⇒ ``"PASS"`` (the reranker lifts recall at power).
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


def headroom_captured(
    rerank_recall: float,
    fathomdb_recall: float,
    oracle_gap: Optional[float],
) -> Optional[float]:
    """Diagnostic fraction of the oracle headroom the CE rerank captures (design §3).

    ``headroom_captured = (rerank_recall − fathomdb_recall) / oracle_gap`` where
    ``oracle_gap`` is the **(oracle_raw − fathomdb)** headroom magnitude taken from
    the gap-decomposition artifact (``component_deltas[class]["RETRIEVAL"]["point"]``
    — the perfect-raw-gold UPPER bound). **Non-gating** and explicitly cross-domain
    (the gap-decomp headroom is an accuracy-delta upper bound; the numerator here is
    a recall lift) — it is a directional diagnostic only, never a pass criterion.

    Returns ``None`` (not a fabricated number) when ``oracle_gap`` is ``None``
    (artifact absent) or ~0 (no measurable headroom to capture against). The
    recall inputs must be finite; a non-finite ``oracle_gap`` raises.
    """
    rr = _require_finite(rerank_recall, "rerank_recall")
    fr = _require_finite(fathomdb_recall, "fathomdb_recall")
    if oracle_gap is None:
        return None
    gap = _require_finite(oracle_gap, "oracle_gap")
    if abs(gap) < 1e-12:
        return None
    return (rr - fr) / gap
