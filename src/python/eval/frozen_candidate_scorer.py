"""0.8.8 Slice 20 (OPP-9) offline frozen-candidate scorer.

Binding spec: ``dev/design/0.8.8-explain-and-telemetry-adr.md §B.3`` +
``dev/plans/runs/0.8.8-explanation-fieldset-ratification.md §3d``.

WHY THIS EXISTS (§B.3). A :class:`eval.gold_capture.GoldRecord` CANNOT drive the
live re-run harness (:mod:`eval.r2_parity_eval`): that harness re-runs retrieval
from query *text*, which is forbidden by §C (text is never captured) and which
:class:`~eval.r2_parity_eval.GoldQuery` has no slot for 0/1 negatives anyway.
So the ratified consumer of telemetry gold is THIS offline scorer: it does NOT
re-run search — it scores the FROZEN captured ``candidate_ids`` against the
agent-supplied ``labels`` over that frozen pool.

METRICS (kept simple, documented, deterministic). Over the LABELED subset of a
record's frozen candidate pool:

- ``precision`` — of the candidates that carry a label, the fraction labeled
  relevant (1). Measures: when the engine returned a candidate the agent had an
  opinion about, how often was that opinion "relevant"? Undefined (``None``)
  when no returned candidate is labeled.
- ``recall`` — of all ids the agent labeled relevant, the fraction that appear
  in the frozen ``candidate_ids``. Measures: did the returned pool actually
  contain the relevant ids? Undefined (``None``) when there are no relevant
  labels.

These are computed purely over ids — no bodies, no scores, no re-ranking — so
the result is deterministic given a record.

EVAL-ONLY: test-infra under ``eval/`` (NOT shipped in the wheel).
"""

from __future__ import annotations

from collections.abc import Iterable

from eval.gold_capture import GoldRecord


def score_gold_record(rec: GoldRecord) -> dict:
    """Score one :class:`GoldRecord`'s frozen candidate pool against its labels.

    Returns a dict with:

    - ``query_id`` — the record's join key (echoed for aggregation/audit).
    - ``n_candidates`` — size of the frozen returned pool.
    - ``n_relevant_labels`` — number of ids labeled relevant (1).
    - ``n_labeled_returned`` — number of returned candidates that carry a label.
    - ``precision`` — labeled-relevant / labeled-returned, or ``None`` when no
      returned candidate is labeled.
    - ``recall`` — relevant-and-returned / total-relevant-labels, or ``None``
      when there are no relevant labels.

    Deterministic: depends only on the record's frozen ids and labels.
    """
    candidates = rec.candidate_ids
    candidate_set = set(candidates)
    labels = rec.labels

    relevant_ids = {i for i, lab in labels.items() if lab == 1}
    # Of the returned candidates, those the agent labeled at all / labeled relevant.
    labeled_returned = [i for i in candidates if i in labels]
    relevant_returned = [i for i in candidates if i in relevant_ids]

    n_labeled_returned = len(labeled_returned)
    precision = (
        len(relevant_returned) / n_labeled_returned if n_labeled_returned else None
    )
    recall = (
        len(relevant_ids & candidate_set) / len(relevant_ids) if relevant_ids else None
    )

    return {
        "query_id": rec.query_id,
        "n_candidates": len(candidates),
        "n_relevant_labels": len(relevant_ids),
        "n_labeled_returned": n_labeled_returned,
        "precision": precision,
        "recall": recall,
    }


def score_gold(records: Iterable[GoldRecord]) -> dict:
    """Aggregate :func:`score_gold_record` over many records.

    Returns a dict with:

    - ``n_records`` — number of scored records.
    - ``mean_precision`` — mean of the per-record ``precision`` over records
      where precision is defined (not ``None``), or ``None`` when none are.
    - ``mean_recall`` — mean of the per-record ``recall`` over records where
      recall is defined, or ``None`` when none are.
    - ``per_record`` — the list of per-record dicts from
      :func:`score_gold_record`.

    Deterministic; means skip ``None`` (undefined) components rather than
    treating them as 0 (the null-vs-zero distinction).
    """
    per_record = [score_gold_record(r) for r in records]

    def _mean(key: str) -> float | None:
        vals = [m[key] for m in per_record if m[key] is not None]
        return sum(vals) / len(vals) if vals else None

    return {
        "n_records": len(per_record),
        "mean_precision": _mean("precision"),
        "mean_recall": _mean("recall"),
        "per_record": per_record,
    }
