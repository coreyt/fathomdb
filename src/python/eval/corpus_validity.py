"""Corpus-validity guard for the 0.8.3 D0a re-pinned gold.

The Slice-25 defect this slice fixes: the frozen COR-2 gold carried **N=0** on the
three non-factoid memory classes, so every ``FathomDB − Mem0`` class delta came back
null and nobody noticed. :func:`validate_repin` is the non-vacuous guard that makes a
silent empty / under-powered class a hard, visible failure.

Pure stdlib (no ``fathomdb`` / no network) so it runs anywhere — the same posture as
:mod:`eval.decision_rule_083`, whose frozen :data:`MEMORY_CLASSES` it consumes.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import Any

from eval.decision_rule_083 import MEMORY_CLASSES


def validate_repin(
    manifest: Mapping[str, Any],
    gold_queries: Sequence[Mapping[str, Any]],
) -> list[str]:
    """Return a list of corpus-validity problems (empty == valid).

    Asserts, against the re-pin manifest:

    * ``n_min`` is present and a positive int;
    * **every** frozen :data:`MEMORY_CLASSES` appears in ``per_class_gold_counts``;
    * **no** memory class is ``N=0`` (the Slice-25 regression);
    * **no** memory class is below ``n_min``;
    * when ``gold_queries`` is non-empty, the per-class manifest counts agree with
      the actual gold (so the manifest cannot drift from the file it pins).

    Non-vacuous by construction: a missing class, an ``N=0`` class, an under-``n_min``
    class, or a manifest/gold count mismatch each yields a distinct problem string.
    """
    problems: list[str] = []

    n_min = manifest.get("n_min")
    if not isinstance(n_min, int) or n_min <= 0:
        problems.append(f"n_min missing or non-positive: {n_min!r}")
        n_min = None

    counts = manifest.get("per_class_gold_counts")
    if not isinstance(counts, Mapping):
        problems.append("per_class_gold_counts missing or not a mapping")
        return problems

    for cls in MEMORY_CLASSES:
        if cls not in counts:
            problems.append(f"missing memory class in per_class_gold_counts: {cls}")
            continue
        c = counts[cls]
        if not isinstance(c, int) or c <= 0:
            problems.append(f"memory class is N=0 (or non-positive): {cls} = {c!r}")
            continue
        if n_min is not None and c < n_min:
            problems.append(f"memory class under n_min: {cls} = {c} < {n_min}")

    # Cross-check the manifest counts against the actual gold file (drift guard).
    if gold_queries:
        observed = _observed_counts(gold_queries)
        for cls in MEMORY_CLASSES:
            declared = counts.get(cls)
            if isinstance(declared, int) and observed.get(cls, 0) != declared:
                problems.append(
                    f"manifest/gold count mismatch for {cls}: "
                    f"manifest={declared} gold={observed.get(cls, 0)}"
                )

    return problems


def _observed_counts(gold_queries: Sequence[Mapping[str, Any]]) -> dict[str, int]:
    """Count gold queries per reporting class, mapping the gold ``query_class``
    through the same map the harness uses (so ``exact_fact`` → ``factoid`` etc.)."""
    from eval.r2_parity_eval import GOLD_CLASS_MAP

    out: dict[str, int] = {}
    for q in gold_queries:
        raw = str(q.get("query_class", "")).strip()
        cls = GOLD_CLASS_MAP.get(raw, raw)
        out[cls] = out.get(cls, 0) + 1
    return out
