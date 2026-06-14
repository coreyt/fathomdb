"""P0-A base-retrieval ablation scorer — TDD (RED first).

Binding spec: ``dev/design/0.8.1-graph-experiment-plan.md`` §1 (Recall@K validity
caveats) and §3.5 (graded metrics cut variance). These tests pin the three
load-bearing scorer rules that make the instrument trustworthy *before* any
graph work:

1. **multi_session full-gold-set rule** — a "hit" requires the *entire* gold
   session set in top-K, not ``>=1`` (the naive any-hit definition mis-measures
   exactly the class the graph arm is meant to help).
2. **abstention exclusion** — questions with no gold session are excluded from
   Recall@K and counted separately.
3. **graded metric** (MRR / nDCG) — a continuous complement that lowers variance.

These tests need neither the database nor an LLM (plan §1 / AGENTS.md): the
scorer is pure functions over id lists.
"""

from __future__ import annotations

import math

import pytest

from eval.p0a_base_retrieval import (
    RetrievalRecord,
    aggregate,
    hit_at_k,
    ndcg_at_k,
    reciprocal_rank,
)


# --------------------------------------------------------------------------- #
# Rule 1 — multi_session requires the FULL gold set in top-K
# --------------------------------------------------------------------------- #


def test_multi_session_requires_full_gold_set() -> None:
    gold = ("g1", "g2")
    # Only one of the two gold sessions in top-K -> MISS for multi_session.
    partial = ("x", "g1", "y", "z")
    assert hit_at_k(gold, partial, k=5, reporting_class="multi_session") == 0.0
    # Both gold sessions present -> HIT.
    full = ("x", "g1", "g2", "z")
    assert hit_at_k(gold, full, k=5, reporting_class="multi_session") == 1.0


def test_non_multi_session_uses_any_hit() -> None:
    gold = ("g1", "g2")
    partial = ("x", "g1", "y", "z")
    # Same partial retrieval is a HIT for the any-hit classes.
    for cls in ("factoid", "temporal", "knowledge_update"):
        assert hit_at_k(gold, partial, k=5, reporting_class=cls) == 1.0
    # No gold in top-K -> MISS.
    none_present = ("x", "y", "z")
    assert hit_at_k(gold, none_present, k=5, reporting_class="factoid") == 0.0


def test_hit_respects_k_cutoff() -> None:
    gold = ("g1",)
    retrieved = ("a", "b", "c", "d", "g1")  # g1 at rank 5
    assert hit_at_k(gold, retrieved, k=4, reporting_class="factoid") == 0.0
    assert hit_at_k(gold, retrieved, k=5, reporting_class="factoid") == 1.0


def test_multi_session_full_set_respects_k_cutoff() -> None:
    gold = ("g1", "g2")
    retrieved = ("g1", "a", "b", "c", "g2")  # g2 only reachable at k>=5
    assert hit_at_k(gold, retrieved, k=4, reporting_class="multi_session") == 0.0
    assert hit_at_k(gold, retrieved, k=5, reporting_class="multi_session") == 1.0


# --------------------------------------------------------------------------- #
# Rule 2 — abstention questions (no gold) are EXCLUDED, not scored 0
# --------------------------------------------------------------------------- #


def test_abstention_returns_none_not_zero() -> None:
    # Empty gold set => abstention => None (excluded), distinct from a 0.0 miss.
    assert hit_at_k((), ("a", "b"), k=5, reporting_class="negative") is None
    assert reciprocal_rank((), ("a", "b")) is None
    assert ndcg_at_k((), ("a", "b"), k=5) is None


def test_aggregate_excludes_abstention_from_recall() -> None:
    records = [
        RetrievalRecord("factoid", ("g1",), ("g1", "x")),       # hit
        RetrievalRecord("factoid", ("g2",), ("x", "y")),         # miss
        RetrievalRecord("factoid", (), ("x", "y")),              # abstention
    ]
    out = aggregate(records, ks=(5,), graded_k=5)
    fac = out["per_class"]["factoid"]
    assert fac["n_total"] == 3
    assert fac["n_scored"] == 2          # abstention excluded from the denominator
    assert fac["n_abstention"] == 1
    # recall = 1 hit / 2 scored = 0.5 (NOT 1/3)
    assert fac["recall_at_5"] == pytest.approx(0.5)
    assert out["abstention_total"] == 1


def test_aggregate_all_abstention_class_has_null_recall() -> None:
    records = [RetrievalRecord("negative", (), ("a",))]
    out = aggregate(records, ks=(5,), graded_k=5)
    neg = out["per_class"]["negative"]
    assert neg["n_scored"] == 0
    assert neg["recall_at_5"] is None
    assert neg["mrr"] is None
    assert neg["ndcg_at_5"] is None


# --------------------------------------------------------------------------- #
# Rule 3 — graded metrics (MRR + nDCG)
# --------------------------------------------------------------------------- #


def test_reciprocal_rank_uses_first_gold() -> None:
    gold = ("g1", "g2")
    assert reciprocal_rank(gold, ("a", "b", "g2", "g1")) == pytest.approx(1 / 3)
    assert reciprocal_rank(gold, ("g1", "a")) == pytest.approx(1.0)
    assert reciprocal_rank(gold, ("a", "b", "c")) == 0.0


def test_ndcg_rewards_higher_ranking() -> None:
    gold = ("g1",)
    top = ndcg_at_k(gold, ("g1", "a", "b"), k=3)
    lower = ndcg_at_k(gold, ("a", "b", "g1"), k=3)
    assert top is not None and lower is not None
    assert top == pytest.approx(1.0)
    assert lower == pytest.approx(1.0 / math.log2(4))  # rank 3 -> 1/log2(3+1)
    assert top > lower


def test_ndcg_multi_gold_ideal_normalisation() -> None:
    gold = ("g1", "g2")
    # Both gold at ranks 1,2 => perfect nDCG.
    assert ndcg_at_k(gold, ("g1", "g2", "x"), k=3) == pytest.approx(1.0)
    # One of two gold retrieved at rank 1: dcg=1, idcg=1+1/log2(3)
    val = ndcg_at_k(gold, ("g1", "x", "y"), k=3)
    idcg = 1.0 + 1.0 / math.log2(3)
    assert val == pytest.approx(1.0 / idcg)


def test_aggregate_reports_graded_per_class() -> None:
    records = [
        RetrievalRecord("temporal", ("g1",), ("g1", "x", "y")),
        RetrievalRecord("temporal", ("g2",), ("x", "g2", "y")),
    ]
    out = aggregate(records, ks=(5,), graded_k=5)
    temp = out["per_class"]["temporal"]
    assert temp["mrr"] == pytest.approx((1.0 + 0.5) / 2)
    assert temp["ndcg_at_5"] is not None
    assert temp["recall_at_5"] == pytest.approx(1.0)
