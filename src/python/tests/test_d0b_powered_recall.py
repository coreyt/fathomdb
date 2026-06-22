"""Slice 10 (0.8.3 / D0b Phase-B §SECONDARY) — powered LME+LOCOMO strict Recall@K.

Backend-free RED→GREEN for ``eval.d0b_powered_recall`` (no DB, no LLM, no ``mem0``,
no ``fathomdb`` extension): the per-class recall-delta math + the LOCOMO cat-1
**≥2-session predicate** (codex Slice-5 [P1#1]; corpus-adequacy note caveat 2). The
new module must NOT modify the just-reviewed ``d0b_parity_run`` (it reuses its
``per_class_delta_table`` for the paired CI).
"""

from __future__ import annotations

from eval.d0b_powered_recall import (
    RecallItem,
    distinct_sessions,
    filter_min_sessions,
    locomo_items,
    recall_records,
    recall_summary,
    strict_recall_at_k,
)
from eval.r2_parity_eval import Hit


class _FakeAdapter:
    """Returns a fixed ranked doc-id list per question (as Hits); deterministic."""

    def __init__(self, name: str, ranked_by_q: dict[str, list[str]]) -> None:
        self.name = name
        self._r = ranked_by_q

    def retrieve(self, question: str, k: int) -> list[Hit]:
        return [Hit(doc_id=d, body="", score=1.0) for d in self._r.get(question, [])[:k]]


def test_strict_recall_at_k_requires_every_gold_in_topk() -> None:
    # every gold present in top-K → 1.0
    assert strict_recall_at_k(["a", "b", "c"], ["a", "c"], 10) == 1.0
    # one gold missing → 0.0 (strict full-gold rule)
    assert strict_recall_at_k(["a", "b", "c"], ["a", "z"], 10) == 0.0
    # gold present only BELOW the cut → 0.0
    assert strict_recall_at_k(["a", "b", "c", "g"], ["g"], 3) == 0.0
    # empty gold never scores 1 (cannot fabricate a hit)
    assert strict_recall_at_k(["a"], [], 10) == 0.0


def test_distinct_sessions_counts_unique_doc_ids() -> None:
    assert distinct_sessions(["s:session_1", "s:session_1"]) == 1
    assert distinct_sessions(["s:session_1", "s:session_2"]) == 2


def test_filter_min_sessions_drops_single_session_multihop_only() -> None:
    items = [
        RecallItem("q1", "multi_session", ("c:session_1",), "locomo", "Q1"),  # 1 session → drop
        RecallItem("q2", "multi_session", ("c:session_1", "c:session_2"), "locomo", "Q2"),  # keep
        RecallItem("q3", "factoid", ("c:session_1",), "locomo", "Q3"),  # 1 session but factoid → keep
        RecallItem("q4", "temporal", ("c:session_3",), "locomo", "Q4"),  # not in classes → keep
    ]
    kept, dropped = filter_min_sessions(items, min_sessions=2, classes=("multi_session",))
    assert dropped == 1
    kept_ids = {i.query_id for i in kept}
    assert kept_ids == {"q2", "q3", "q4"}


def test_locomo_items_maps_loader_gold_to_recall_items() -> None:
    gold = [
        {
            "query_id": "locomo-0001",
            "query": "Where?",
            "query_class": "multi_session",
            "required_evidence": [{"doc_id": "c:session_1"}, {"doc_id": "c:session_2"}],
            "answers": ["here"],
        }
    ]
    items = locomo_items(gold)
    assert len(items) == 1
    it = items[0]
    assert it.query_id == "locomo-0001"
    assert it.reporting_class == "multi_session"
    assert it.gold_doc_ids == ("c:session_1", "c:session_2")
    assert it.source == "locomo"


def test_recall_records_and_per_class_delta() -> None:
    items = [
        RecallItem("f1", "factoid", ("d1",), "lme", "qf1"),
        RecallItem("f2", "factoid", ("d2",), "lme", "qf2"),
        RecallItem("m1", "multi_session", ("d3", "d4"), "locomo", "qm1"),
    ]
    # fathomdb retrieves every gold; naive_rag misses f2's gold.
    fathomdb = _FakeAdapter("fathomdb", {"qf1": ["d1"], "qf2": ["d2"], "qm1": ["d3", "d4"]})
    naive = _FakeAdapter("naive_rag", {"qf1": ["d1"], "qf2": ["zzz"], "qm1": ["d3", "d4"]})
    records = recall_records(items, {"fathomdb": fathomdb, "naive_rag": naive}, k=10)
    assert len(records) == 3
    # combined-corpus view: factoid n=2, multi_session n=1
    summ = recall_summary(
        records,
        classes=("factoid", "multi_session"),
        treatment="fathomdb",
        comparators=("naive_rag",),
    )
    assert summ["per_class_n"]["factoid"] == 2
    assert summ["per_arm_recall"]["fathomdb"]["factoid"]["mean"] == 1.0
    assert summ["per_arm_recall"]["naive_rag"]["factoid"]["mean"] == 0.5
    # paired fathomdb − naive_rag delta on factoid = mean(1-1, 1-0) = 0.5
    fac = summ["recall_deltas"]["naive_rag"]["factoid"]
    assert fac["point"] == 0.5
    assert fac["n"] == 2

    # LME-only view restricts to source=="lme" (drops the LOCOMO multi_session item)
    lme_only = recall_summary(
        records,
        source_filter="lme",
        classes=("factoid", "multi_session"),
        treatment="fathomdb",
        comparators=("naive_rag",),
    )
    assert lme_only["per_class_n"]["multi_session"] == 0
    assert lme_only["per_class_n"]["factoid"] == 2


class _DupAdapter:
    """Returns duplicate doc-ids in rank order — models Mem0 (several memories per
    session). Honors the requested pool size (codex §9 [P2] regression fixture)."""

    def __init__(self, ranked: list[str]) -> None:
        self._ranked = ranked

    def retrieve(self, question: str, k: int):  # noqa: ANN001 ANN201
        return [Hit(doc_id=d, body="", score=1.0) for d in self._ranked[:k]]


def test_recall_dedupes_before_topk_cut_codex_p2() -> None:
    """A gold doc whose UNIQUE rank is <= k but whose RAW rank is > k must still
    count — the scorer fetches a larger pool, dedupes, THEN cuts (codex §9 [P2]).
    Old behavior (retrieve k raw, then dedupe) scored this 0.0."""
    # First 10 RAW hits hold only 5 unique docs; gold "G" is at raw-rank 11,
    # unique-rank 6 (<= k=10).
    ranked = ["a", "a", "b", "b", "c", "c", "d", "d", "e", "e", "G"]
    items = [
        RecallItem(
            query_id="q1",
            reporting_class="factoid",
            gold_doc_ids=("G",),
            source="lme",
            question="q1",
        )
    ]
    recs = recall_records(items, {"mem0_oss": _DupAdapter(ranked)}, k=10)
    assert recs[0]["recall"]["mem0_oss"] == 1.0, "dedupe-then-cut must surface unique-rank<=k gold"
