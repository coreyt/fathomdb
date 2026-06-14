"""Slice 25 — R2 parity eval harness contract tests (RED → GREEN).

These exercise the harness *internals* with stubs only: no DB, no live LLM, no
``R2_RUN``. They pin the load-bearing measurement-neutrality properties the codex
§9 review checks (ADR-0.8.1-ir-measure-eval-design §3):

* RED-1 identical-answerer constraint (the same answerer / same prompt template
  serves all three systems; adapters cannot build their own prompt);
* RED-2 per-class scorer (all five R2 classes present; abstention counts as a miss);
* RED-3 corpus-hash pin (COR-2) is asserted before any number is produced;
* RED-4 output artifact carries the required keys.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from eval.r2_parity_eval import (
    CORPUS_HASH_PREFIX,
    R2_CLASSES,
    Hit,
    PerClassScorer,
    R2Harness,
    RecordingAnswerer,
    StubAdapter,
    StubAnswerer,
)

# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------


def _write_stub_gold(path: Path, *, corpus_hash: str) -> Path:
    """Write a tiny gold file (one query per class we exercise) with the given
    ``corpus_hash`` so the harness can be constructed without the frozen corpus."""

    queries = [
        {
            "query_id": "q-factoid-1",
            "query": "What is the capital of France?",
            "query_class": "exact_fact",  # mapped → factoid by the harness
            "answers": ["Paris"],
            "required_evidence": [{"doc_id": "doc-fr", "locator": {"kind": "whole_body"}}],
        },
        {
            "query_id": "q-explore-1",
            "query": "Summarize the meeting outcomes.",
            "query_class": "exploratory",
            "answers": ["a plan was drafted"],
            "required_evidence": [{"doc_id": "doc-mtg", "locator": {"kind": "whole_body"}}],
        },
        {
            "query_id": "q-neg-1",
            "query": "Who won the 3019 world cup?",
            "query_class": "negative",
            "answers": [],
            "required_evidence": [],
        },
    ]
    payload = {
        "corpus_hash": corpus_hash,
        "qrels_version": "ir-c-reused-v1",
        "note": "slice-25 unit-test stub gold",
        "queries": queries,
    }
    path.write_text(json.dumps(payload), encoding="utf-8")
    return path


def _stub_systems() -> dict[str, StubAdapter]:
    """Three adapters that each retrieve a fixed (correct) hit for every query."""

    hits_by_query = {
        "What is the capital of France?": [Hit(doc_id="doc-fr", body="Paris is the capital.", score=1.0)],
        "Summarize the meeting outcomes.": [Hit(doc_id="doc-mtg", body="The team drafted a plan.", score=1.0)],
        "Who won the 3019 world cup?": [],  # nothing to find → answerer must abstain
    }
    return {
        "fathomdb": StubAdapter(name="fathomdb", hits_by_query=hits_by_query),
        "mem0_oss": StubAdapter(name="mem0_oss", hits_by_query=hits_by_query),
        "naive_rag": StubAdapter(name="naive_rag", hits_by_query=hits_by_query),
    }


# ---------------------------------------------------------------------------
# RED-1 — identical-answerer constraint
# ---------------------------------------------------------------------------


def test_identical_answerer_constraint_enforced(tmp_path: Path) -> None:
    gold = _write_stub_gold(tmp_path / "stub.gold.json", corpus_hash="fe973fcd49fb_stub")
    answerer = RecordingAnswerer()
    systems = _stub_systems()

    harness = R2Harness(gold_path=gold, answerer=answerer)
    harness.run(systems, k=5)

    # The one answerer object served every system (≥ one call per system × query).
    assert answerer.records, "answerer was never invoked by the harness"

    # Adapters expose ONLY retrieval — they cannot build a prompt or answer, so a
    # per-system prompt divergence is structurally impossible.
    for adapter in systems.values():
        assert hasattr(adapter, "retrieve")
        assert not hasattr(adapter, "answer")
        assert not hasattr(adapter, "build_prompt")

    # Every prompt template the answerer saw is byte-identical (same skeleton for
    # all three systems): the load-bearing identical-answerer property.
    templates = {rec.template for rec in answerer.records}
    assert len(templates) == 1, f"prompt template diverged across systems: {templates!r}"

    # And for the SAME question routed through all three systems, the template is
    # identical while only the retrieved context may differ.
    by_question: dict[str, set[str]] = {}
    for rec in answerer.records:
        by_question.setdefault(rec.question, set()).add(rec.template)
    for question, tmpls in by_question.items():
        assert len(tmpls) == 1, f"question {question!r} got divergent templates {tmpls!r}"


# ---------------------------------------------------------------------------
# RED-2 — per-class scorer
# ---------------------------------------------------------------------------


def test_per_class_scoring_has_all_five_classes() -> None:
    scorer = PerClassScorer()
    required = {"factoid", "temporal", "multi_hop", "knowledge_update", "multi_session"}
    assert required <= scorer.classes
    assert required <= set(R2_CLASSES)


def test_abstention_counted_as_miss() -> None:
    scorer = PerClassScorer()
    # ground truth exists but the system returned no answer → a miss (0.0), NOT skipped.
    acc = scorer.score_answer(ground_truth=["X"], system_answer=None)
    assert acc == 0.0


def test_answering_a_negative_query_is_a_false_positive() -> None:
    scorer = PerClassScorer()
    # No answer exists (negative class) but the system answered → false positive (0.0).
    assert scorer.score_answer(ground_truth=[], system_answer="some confident guess") == 0.0
    # Correctly abstaining on a negative query scores 1.0.
    assert scorer.score_answer(ground_truth=[], system_answer=None) == 1.0


def test_correct_answer_scores_one() -> None:
    scorer = PerClassScorer()
    assert scorer.score_answer(ground_truth=["Paris"], system_answer="The capital is Paris.") == 1.0
    assert scorer.score_answer(ground_truth=["Paris"], system_answer="London") == 0.0


# ---------------------------------------------------------------------------
# RED-3 — corpus-hash pin (COR-2)
# ---------------------------------------------------------------------------


def test_harness_rejects_wrong_corpus_hash(tmp_path: Path) -> None:
    gold = _write_stub_gold(tmp_path / "bad.gold.json", corpus_hash="deadbeefdeadbeef")
    with pytest.raises(ValueError, match=CORPUS_HASH_PREFIX):
        R2Harness(gold_path=gold, answerer=StubAnswerer())


def test_harness_accepts_pinned_corpus_hash(tmp_path: Path) -> None:
    gold = _write_stub_gold(tmp_path / "ok.gold.json", corpus_hash="fe973fcd49fb_stub")
    harness = R2Harness(gold_path=gold, answerer=StubAnswerer())
    assert harness.corpus_hash.startswith(CORPUS_HASH_PREFIX)


# ---------------------------------------------------------------------------
# RED-4 — output artifact schema
# ---------------------------------------------------------------------------


def test_output_json_has_required_keys(tmp_path: Path) -> None:
    gold = _write_stub_gold(tmp_path / "stub.gold.json", corpus_hash="fe973fcd49fb_stub")
    harness = R2Harness(gold_path=gold, answerer=StubAnswerer())
    out = harness.run(_stub_systems(), k=5)

    for key in ("r2_per_class_deltas", "corpus_hash", "answerer_model", "n_queries_per_class"):
        assert key in out, f"missing required output key: {key}"

    # The delta table MUST carry the three R3 go/no-go classes (even if null).
    for cls in ("temporal", "multi_hop", "knowledge_update"):
        assert cls in out["r2_per_class_deltas"], f"delta table missing class {cls}"
        row = out["r2_per_class_deltas"][cls]
        assert "fathomdb_minus_mem0" in row
        assert "fathomdb_minus_naive_rag" in row

    # n_queries_per_class is keyed by the five R2 classes.
    for cls in R2_CLASSES:
        assert cls in out["n_queries_per_class"]

    assert out["corpus_hash"].startswith(CORPUS_HASH_PREFIX)
