"""M1 strong-baseline harness — TDD (RED first), Slice 5.

Binding spec: ``dev/plans/plan-0.8.2.md`` §4 (Slice 5) + ``dev/design/
0.8.2-m1-multihop-harness.md`` §2/§3/§5. These tests pin the four load-bearing
contracts of THE BAR, none of which need a priced LLM call (a ``StubAnswerer`` /
``RecordingAnswerer`` stands in):

(a) the pipeline runs over the **pinned** ``musique_hash`` (and refuses an
    unpinned corpus);
(b) EM/F1 + per-hop(2/3/4) + pooled ≥3-hop + unanswerable metrics emit a
    structured artifact;
(c) the **identical-answerer parity** — the same answerer/top-K is used across
    all four arms (asserted structurally, à la ``r2_parity_eval``);
(d) the four arms are distinct, RRF is k=60 pinned, the comparator is
    ``fused_rerank``.

No DB, no model: the dense encoder + engine reranker are injected as
deterministic fakes so the suite is fast and LLM-free.
"""

from __future__ import annotations

import hashlib
from pathlib import Path

import numpy as np
import pytest

from eval.m1_baseline import (
    ARM_NAMES,
    COMPARATOR_ARM,
    MUSIQUE_HASH,
    RERANK_DEPTH,
    RRF_K,
    Paragraph,
    Question,
    bm25_rank,
    corpus_hash,
    dense_rank,
    em_score,
    f1_score,
    is_confident_answer,
    load_musique,
    normalize_squad,
    retrieve_arms,
    rrf_fuse,
    run_baseline,
)
from eval.r2_parity_eval import RecordingAnswerer, StubAnswerer

_CORPUS = Path(__file__).resolve().parents[3] / "data" / "corpus-data" / "raw" / "musique_dev.jsonl"


# --------------------------------------------------------------------------- #
# Fakes (deterministic; no model / no engine)
# --------------------------------------------------------------------------- #


class FakeEncoder:
    """Deterministic hash-based embedding so the dense arm runs without bge."""

    def encode(self, text: str) -> np.ndarray:
        h = hashlib.sha256(text.encode()).digest()
        v = np.frombuffer(h, dtype=np.uint8).astype(np.float32)[:16]
        return v / (np.linalg.norm(v) + 1e-9)


class FakeReranker:
    """Reranks the fused pool by bm25 order (deterministic; no CE/model).

    Implements the fused-pool seam: receives the in-harness fused(bm25+dense)
    scored pool and returns a full ranking + the reranked-pool size (n_pool)."""

    def rank_fused(self, query, passages, fused_scored, *, depth=None):  # noqa: ANN001
        order = bm25_rank(query, passages)
        return order, len(passages)


def _mini_question(qid: str, hop: int, answerable: bool, answer: str) -> Question:
    paras = tuple(
        Paragraph(idx=i, title=f"T{i}", text=f"body text number {i} about {answer if i == 0 else 'distractor'}",
                  is_supporting=(i == 0))
        for i in range(20)
    )
    return Question(
        id=qid, question=f"what is {answer}?", hop_count=hop, answer=answer,
        answer_aliases=(), answerable=answerable, paragraphs=paras,
    )


def _mini_set() -> list[Question]:
    qs = []
    for hop in (2, 3, 4):
        qs.append(_mini_question(f"{hop}hop__a", hop, True, f"ans{hop}"))
        qs.append(_mini_question(f"{hop}hop__b", hop, True, f"ANS{hop}"))
    qs.append(_mini_question("3hop__u", 3, False, "noans"))
    qs.append(_mini_question("4hop__u", 4, False, "noans2"))
    return qs


# --------------------------------------------------------------------------- #
# (a) pinned musique_hash
# --------------------------------------------------------------------------- #


@pytest.mark.skipif(not _CORPUS.exists(), reason="musique corpus not reproduced")
def test_load_asserts_pinned_hash() -> None:
    assert corpus_hash(_CORPUS) == MUSIQUE_HASH
    qs = load_musique(_CORPUS)
    assert len(qs) == 4834
    assert {q.hop_count for q in qs} == {2, 3, 4}
    assert any(not q.answerable for q in qs) and any(q.answerable for q in qs)


def test_load_refuses_unpinned_corpus(tmp_path: Path) -> None:
    bad = tmp_path / "bad.jsonl"
    bad.write_text('{"id":"2hop__x","question":"q","hop_count":2,"answer":"a",'
                   '"answer_aliases":[],"answerable":true,"paragraphs":[]}\n', encoding="utf-8")
    with pytest.raises(ValueError, match="musique_hash"):
        load_musique(bad)


# --------------------------------------------------------------------------- #
# (b) structured metrics artifact (per-hop + pooled ≥3-hop + unanswerable)
# --------------------------------------------------------------------------- #


def test_pipeline_emits_structured_artifact() -> None:
    art = run_baseline(
        _mini_set(), StubAnswerer(), k=10,
        encoder=FakeEncoder(), reranker=FakeReranker(),
    )
    assert art["musique_hash"] == MUSIQUE_HASH
    assert art["rrf_k"] == RRF_K == 60
    assert art["rerank_depth"] == RERANK_DEPTH == 200
    assert art["comparator_arm"] == COMPARATOR_ARM == "fused_rerank"
    assert list(art["arms"]) == list(ARM_NAMES)
    # per-hop strata 2/3/4 each carry every arm
    for hop in ("2", "3", "4"):
        assert set(art["per_hop"][hop].keys()) == set(ARM_NAMES)
        assert art["per_hop"][hop][COMPARATOR_ARM]["n"] == 2
    # pooled ≥3-hop is the primary cell (4 answerable: two 3-hop + two 4-hop)
    assert art["primary_cell_pooled_ge3hop"][COMPARATOR_ARM]["n"] == 4
    for arm in ARM_NAMES:
        cell = art["primary_cell_pooled_ge3hop"][arm]
        assert cell["em"] is not None and cell["f1"] is not None
    # unanswerable contrast set carries a confident-answer rate per arm
    assert art["n_unanswerable"] == 2
    for arm in ARM_NAMES:
        assert art["unanswerable_contrast"][arm]["confident_answer_rate"] is not None
    # per-question paired records exist for paired-bootstrap (Slice 20)
    assert len(art["paired_records"]) == len(_mini_set())
    rec = art["paired_records"][0]
    assert set(rec["f1"].keys()) <= set(ARM_NAMES)
    assert "n_pool" in rec


# --------------------------------------------------------------------------- #
# (c) identical-answerer parity — same answerer/template/top-K across arms
# --------------------------------------------------------------------------- #


def test_identical_answerer_parity_across_arms() -> None:
    rec = RecordingAnswerer()
    qs = _mini_set()
    run_baseline(qs, rec, k=7, encoder=FakeEncoder(), reranker=FakeReranker())
    # one call per (question, arm)
    assert len(rec.records) == len(qs) * len(ARM_NAMES)
    # every call used the identical prompt template (no per-arm divergence)
    templates = {r.template for r in rec.records}
    assert templates == {RecordingAnswerer.PROMPT_TEMPLATE}
    # every call respected the same top-K budget
    assert all(len(r.context) <= 7 for r in rec.records)


# --------------------------------------------------------------------------- #
# (d) arms are distinct + pure-function correctness
# --------------------------------------------------------------------------- #


def test_arms_are_distinct_and_pinned() -> None:
    q = _mini_question("3hop__x", 3, True, "green bay")
    arms = retrieve_arms(q, FakeEncoder(), FakeReranker())
    for a in ARM_NAMES:
        assert isinstance(arms[a], list) and len(arms[a]) == 20
    assert arms["n_pool"] == 20


def test_fused_rerank_reranks_the_fused_pool() -> None:
    # [P1] regression: the fused_rerank arm must CE-rerank the IN-HARNESS
    # fused(bm25+dense) pool via fathomdb.rerank — NOT the engine's own capped
    # text pool. Construct a pool where the CE flips a middle pair: a distractor
    # the CE demotes sits just ABOVE the answer in RRF, with scores close enough
    # that the 0.3·CE term overcomes the 0.7·RRF_norm gap (engine Decision 5).
    # Local import so RED (symbols absent) fails THIS test alone, not collection.
    from eval.m1_baseline import FusedPoolReranker, rrf_fuse_scored  # noqa: PLC0415
    import fathomdb  # noqa: PLC0415

    paras = [
        Paragraph(0, "Wall", "The Great Wall of China stretches thousands of kilometres across northern China.", False),
        Paragraph(1, "Banana", "Bananas are a yellow tropical fruit rich in potassium and fibre.", False),
        Paragraph(2, "Paris", "Paris is the capital and most populous city of France.", True),
        Paragraph(3, "Ocean", "The Pacific Ocean is the largest and deepest of Earth's oceans.", False),
    ]
    query = "what is the capital of France?"
    # Fused order ranks the answer (idx 2) THIRD, just below an irrelevant idx 1.
    fused_scored = [(0, 1.0), (1, 0.55), (2, 0.50), (3, 0.0)]
    fused_order = [i for i, _ in fused_scored]

    reranker = FusedPoolReranker()
    ranked, n_pool = reranker.rank_fused(query, paras, fused_scored, depth=200)

    # (1) reranks the WHOLE fused pool (rerank_depth clamps to the pool size).
    assert n_pool == 4
    # (2) matches fathomdb.rerank applied to the SAME fused pool (id/body/score).
    payload = [{"id": i, "body": paras[i].body, "score": s} for i, s in fused_scored]
    expected = [int(d["id"]) for d in fathomdb.rerank(query, payload, 4)]
    assert ranked == expected
    # (3) the CE actually reordered the pool — differs from the raw fused order,
    #     and the answer passage moved UP above the demoted distractor.
    assert ranked != fused_order
    assert ranked.index(2) < fused_order.index(2)
    # rrf_fuse_scored is the in-harness pool the arm consumes (id, score) desc.
    scored = rrf_fuse_scored([[2, 0, 1, 3], [2, 1, 0, 3]])
    assert scored[0][0] == 2 and scored[-1][0] == 3
    assert all(scored[i][1] >= scored[i + 1][1] for i in range(len(scored) - 1))


def test_rrf_is_k60_and_fuses() -> None:
    # an item ranked top by both arms must beat one ranked low by both
    r1 = [3, 1, 2, 0]
    r2 = [3, 2, 1, 0]
    fused = rrf_fuse([r1, r2], k=RRF_K)
    assert fused[0] == 3 and fused[-1] == 0


def test_bm25_ranks_lexical_match_first() -> None:
    paras = [
        Paragraph(0, "A", "the capital of france is paris", False),
        Paragraph(1, "B", "bananas are yellow fruit", False),
    ]
    assert bm25_rank("what is the capital of france", paras)[0] == 0


def test_dense_rank_returns_full_permutation() -> None:
    paras = [Paragraph(i, f"T{i}", f"text {i}", False) for i in range(5)]
    order = dense_rank("query", paras, FakeEncoder())
    assert sorted(order) == [0, 1, 2, 3, 4]


def test_scorer_em_f1_and_confident() -> None:
    assert em_score("Green Bay", ["green bay"]) == 1.0
    assert em_score("Green Bay, WI", ["green bay"]) == 0.0
    assert f1_score("green bay wisconsin", ["green bay"]) > 0.5
    assert f1_score(None, ["green bay"]) == 0.0
    assert normalize_squad("The Green Bay!") == "green bay"
    assert is_confident_answer("paris") is True
    assert is_confident_answer(None) is False


# --------------------------------------------------------------------------- #
# [S5fix2] all_bridges_present@K — RED test (must fail before metric exists)
# --------------------------------------------------------------------------- #


def test_bridges_present_at_k_correctness() -> None:
    """[S5fix2] bridges_present_at_k: all gold in top-K → 1.0; one missing → 0.0.

    RED: this test imports ``bridges_present_at_k`` from ``eval.m1_baseline``;
    the symbol does not exist before the fix-2 GREEN commit, so collection fails.
    """
    from eval.m1_baseline import bridges_present_at_k  # noqa: PLC0415

    # All gold passages present in the top-K → 1.0
    assert bridges_present_at_k([0, 1, 2, 3, 4], {0, 2}, k=3) == 1.0
    # One gold passage missing from the top-K → 0.0
    assert bridges_present_at_k([0, 1, 2, 3, 4], {0, 4}, k=3) == 0.0
    # No gold passages → None (excluded from mean)
    assert bridges_present_at_k([0, 1, 2], set(), k=3) is None
    # Both gold passages exactly at boundary (K=3, positions 0 and 2) → 1.0
    assert bridges_present_at_k([0, 2, 1, 3, 4], {0, 2}, k=2) == 1.0
    # Gold passage at position 3 excluded when K=2 → 0.0
    assert bridges_present_at_k([0, 1, 2, 3, 4], {0, 3}, k=2) == 0.0
