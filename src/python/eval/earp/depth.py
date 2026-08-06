"""Mode-aware retrieval depth (D-5) — one rule, one place.

This is deliberately NOT inside the metric layer. The reference's
`evidence_recall_at_k` takes any `k` and never refuses; a version carrying an
extra refusal arm would not be the reference's function, so parity could not be
asserted on it.

The cap is an SDK-surface fact, not a metric fact. The reference measures vector
@50 perfectly well by raising the fanout through `set_search_limit_for_test`;
what blocks EARP is that the seam has no PyO3 binding and D-5.3 forbids
exporting it. A pure metric function receiving a ranked list cannot know how
that list was obtained, so a check inside it would be advisory at best.

S3 calls this at config validation; the runner calls it before retrieval.
"""

from __future__ import annotations

from eval.earp.schema.models import (
    MAX_MEASURABLE_K,
    PRODUCTION_RERANK_LIMIT,
    Blocker,
    BlockerCode,
    RetrievalMode,
)


def check_depth(mode: RetrievalMode, k: int) -> Blocker | None:
    """Return a blocker when `mode` cannot honestly measure at depth `k`.

    Returned, never raised: an unmeasurable depth is a declared-configuration
    problem to be recorded, not an exceptional condition.
    """
    limit = MAX_MEASURABLE_K[mode]
    if limit is None or k <= limit:
        return None
    return Blocker(
        code=BlockerCode.METRIC_NOT_MEASURABLE,
        message=(
            f"@{k} is not measurable for retrieval mode `{mode.value}`: the production "
            f"phase-2 rerank floor SEARCH_RERANK_LIMIT={PRODUCTION_RERANK_LIMIT} bounds the "
            f"vector path, so depths beyond {limit} would return copies of the @{limit} "
            f"result rather than a deeper one. Raising it needs the commissioned "
            f"cross-binding evaluation-fanout slice (D-5.2); the hidden "
            f"set_search_limit_for_test seam is not exported (D-5.3)."
        ),
        stage="metrics.depth",
        detail={"mode": mode.value, "k": k, "max_measurable_k": limit},
    )


__all__ = ["check_depth"]
