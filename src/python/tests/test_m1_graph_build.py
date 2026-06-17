"""Coverage test for the M1 (0.8.2 Slice 10) per-question graph build.

Load-bearing assertions (no real extraction — a tiny hand-built fixture so the test
is fast and offline):
  - ``build_question_graph_engine`` loads per-question entities + **body-less**
    fact-edges into ``canonical_nodes`` / ``canonical_edges``;
  - ``verify_coverage`` confirms **every sampled question has a non-empty graph**
    (≥1 entity node) and the node/edge tables are populated — the way
    ``verify_embed_db`` verifies embeds (drain/terminal status can lie);
  - the **body-less edge invariant** holds (no edge carries a ``body``);
  - the partial-coverage failure mode is DETECTED (a question with an empty graph
    makes the report ``ok == False`` and ``assert_coverage`` raise) — the
    verify_embed_db "partial embed passes naive checks" lesson;
  - ``sample_questions`` is deterministic + hop-stratified.
"""

from __future__ import annotations

import pytest

from eval.m1_graph_build import (
    CoverageIncompleteError,
    assert_coverage,
    build_question_graph_engine,
    sample_questions,
    verify_coverage,
)


def _q(qid: str, n_para: int, hop: int, answerable: bool = True) -> dict:
    return {
        "id": qid,
        "hop_count": hop,
        "answerable": answerable,
        "paragraphs": [
            {"idx": i, "title": f"T{i}", "text": f"body of paragraph {i} for {qid}"}
            for i in range(n_para)
        ],
    }


# Two real questions with non-empty extracted graphs.
_Q1 = _q("2hop__1_1", 2, 2)
_Q2 = _q("3hop1__2_2", 1, 3)
# A third question whose extraction yielded NOTHING (the partial-coverage failure).
_Q_EMPTY = _q("2hop__9_9", 1, 2)

_PARA_GRAPHS = {
    "2hop__1_1#0": {
        "entities": [{"name": "Alice", "type": "person"}, {"name": "Paris", "type": "city"}],
        "relations": [{"subject": "Alice", "predicate": "lives in", "object": "Paris"}],
    },
    "2hop__1_1#1": {
        "entities": [{"name": "Paris", "type": "city"}, {"name": "France", "type": "country"}],
        "relations": [{"subject": "Paris", "predicate": "capital of", "object": "France"}],
    },
    "3hop1__2_2#0": {
        "entities": [{"name": "Bob", "type": "person"}, {"name": "London", "type": "city"}],
        "relations": [{"subject": "Bob", "predicate": "lives in", "object": "London"}],
    },
    # _Q_EMPTY contributes no entities/relations.
    "2hop__9_9#0": {"entities": [], "relations": []},
}


@pytest.fixture()
def built_db(tmp_path):
    db = tmp_path / "graph.sqlite"
    questions = [_Q1, _Q2, _Q_EMPTY]
    engine, stats = build_question_graph_engine(questions, _PARA_GRAPHS, db, log=lambda *_: None)
    engine.close()
    return db, [q["id"] for q in questions], stats


def test_build_populates_canonical_tables_bodyless(built_db):
    db, _qids, stats = built_db
    # Q1: {alice, paris, france} = 3 entities, 2 edges; Q2: {bob, london} = 2 entities, 1 edge.
    assert stats["entities"] == 5
    assert stats["edges"] == 3
    assert stats["docs"] == 4  # 2 + 1 + 1 paragraphs
    rep = verify_coverage(db, [_Q1["id"], _Q2["id"]])
    assert rep.n_entity_nodes == 5
    assert rep.n_edges_total == 3
    # BODY-LESS invariant: not a single edge may carry a body.
    assert rep.n_edges_with_body == 0


def test_every_sampled_question_has_nonempty_graph(built_db):
    db, _qids, _stats = built_db
    rep = verify_coverage(db, [_Q1["id"], _Q2["id"]])
    assert rep.ok
    assert rep.coverage == 1.0
    assert rep.n_questions_nonempty == 2
    assert rep.per_question[_Q1["id"]]["entities"] == 3
    assert rep.per_question[_Q1["id"]]["edges"] == 2
    assert rep.per_question[_Q2["id"]]["entities"] == 2
    assert rep.per_question[_Q2["id"]]["edges"] == 1
    assert_coverage(db, [_Q1["id"], _Q2["id"]])  # does not raise


def test_partial_coverage_is_detected(built_db):
    db, _qids, _stats = built_db
    # Include the empty-graph question: coverage must drop below 1.0 and fail.
    rep = verify_coverage(db, [_Q1["id"], _Q2["id"], _Q_EMPTY["id"]])
    assert not rep.ok
    assert rep.coverage == pytest.approx(2 / 3, rel=1e-6)
    assert _Q_EMPTY["id"] in rep.empty_questions
    with pytest.raises(CoverageIncompleteError):
        assert_coverage(db, [_Q1["id"], _Q2["id"], _Q_EMPTY["id"]])


def test_sample_questions_deterministic_and_stratified():
    rows = (
        [_q(f"2hop__{i}_{i}", 3, 2) for i in range(40)]
        + [_q(f"3hop1__{i}_{i}", 3, 3) for i in range(20)]
        + [_q(f"4hop3__{i}_{i}", 3, 4) for i in range(10)]
    )
    a = sample_questions(rows, n=14, seed=7, log=lambda *_: None)
    b = sample_questions(rows, n=14, seed=7, log=lambda *_: None)
    assert [q["id"] for q in a] == [q["id"] for q in b]  # determinism
    hops = {int(q["hop_count"]) for q in a}
    assert hops == {2, 3, 4}  # every stratum represented
    # different seed ⇒ (generally) a different selection, still all strata
    c = sample_questions(rows, n=14, seed=99, log=lambda *_: None)
    assert {int(q["hop_count"]) for q in c} == {2, 3, 4}
