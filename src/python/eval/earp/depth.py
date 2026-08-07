"""Retrieval depth — one rule, one place (S6a, the D-5 successor).

@K is measurable exactly when `k <= limit`, for EVERY retrieval mode: 0.8.22
Slice 18 gave every search verb a public `limit` (default 10, refused outside
1..=100 with a typed engine error), so the old mode table — refuse deep K for
vector/hybrid, treat FTS as unbounded — describes an engine that no longer
exists. Mode still determines cost and semantics; it no longer determines
depth.

This is deliberately NOT inside the metric layer. The reference's
`evidence_recall_at_k` takes any `k` and never refuses; a version carrying an
extra refusal arm would not be the reference's function, so parity could not
be asserted on it. Limit RANGE validation is not this function's job either:
that is a config-shape fact the schema window owns (`earp.config.v1`), and a
second check here would double-report.

S3 calls this at config validation with the resolved limit; S6's
characterization path calls it with the engine maximum to refuse
permanently-unmeasurable ladders.
"""

from __future__ import annotations

from eval.earp.schema.models import (
    ENGINE_MAX_RESULT_LIMIT,
    Blocker,
    BlockerCode,
    RetrievalMode,
)


def check_depth(mode: RetrievalMode, k: int, limit: int) -> Blocker | None:
    """Return a blocker when depth `k` cannot honestly be measured under the
    public result limit in effect.

    Returned, never raised: an unmeasurable depth is a declared-configuration
    problem to be recorded, not an exceptional condition.
    """
    if k <= limit:
        return None
    if k > ENGINE_MAX_RESULT_LIMIT:
        message = (
            f"@{k} is not measurable in any retrieval mode: the engine refuses a result "
            f"`limit` above {ENGINE_MAX_RESULT_LIMIT} with a typed error, so depths beyond "
            f"{ENGINE_MAX_RESULT_LIMIT} are permanently unmeasurable."
        )
    else:
        message = (
            f"@{k} is not measurable for retrieval mode `{mode.value}`: the run's public "
            f"result limit is {limit}, so the engine returns at most {limit} visible hits "
            f"and depths beyond that would score a page it never delivered. Raise `limit` "
            f"(0.8.22 Slice 18's public result limit) up to {ENGINE_MAX_RESULT_LIMIT}."
        )
    return Blocker(
        code=BlockerCode.METRIC_NOT_MEASURABLE,
        message=message,
        stage="metrics.depth",
        detail={"mode": mode.value, "k": k, "limit": limit},
    )


__all__ = ["check_depth"]
