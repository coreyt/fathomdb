"""P0-A answer-accuracy scorer unification — TDD (§5.6).

Binding spec: ``dev/plans/prompts/0.8.1-graph-track-HANDOFF-2.md`` §5.6 (one shared
scorer so the batch and sync e2e paths cannot drift; back-port the apostrophe-safe
abstention fix to ``*Answerer._complete``). These tests pin:

1. **apostrophe-safe abstention** — ``normalize_answer`` maps "I don't know" (and
   empties) to ``None``; ``_normalize`` turns the apostrophe into a space, so the
   legacy literal set ``{"i dont know"}`` silently missed the exact phrase the
   answerer prompt instructs the model to emit.
2. **scorer agreement (the drift lock)** — the batch ``score_e2e`` and the shared
   ``score_answers`` (which the sync ``run_e2e_loop`` now also calls) return a
   byte-identical scoring block for the same logical inputs, including the
   "I don't know" abstention case that was previously mis-scored.
3. **answerer back-port** — ``AirlockAnswerer._complete`` abstains (returns
   ``None``) on the exact "I don't know" output.

These tests need neither the database nor an LLM (the answerer test mocks urlopen).
"""

from __future__ import annotations

import json

import pytest

from eval.p0a_base_retrieval import (
    AirlockAnswerer,
    AnswerRecord,
    score_answer,
    score_answers,
)
from eval.p0a_batch_e2e import score_e2e
from eval.r2_parity_eval import normalize_answer


# --------------------------------------------------------------------------- #
# 1. normalize_answer — apostrophe-safe abstention
# --------------------------------------------------------------------------- #


def test_normalize_answer_apostrophe_abstention() -> None:
    # The exact phrase the prompt asks for — previously slipped through as a
    # non-None answer because `_normalize("I don't know") == "i don t know"`.
    assert normalize_answer("I don't know") is None
    assert normalize_answer("I don't know.") is None
    assert normalize_answer("I dont know") is None
    assert normalize_answer("idk") is None
    assert normalize_answer("  ") is None
    assert normalize_answer("") is None
    assert normalize_answer(None) is None
    # A real answer survives intact (not lower-cased / stripped of content).
    assert normalize_answer("  42  ") == "42"
    assert normalize_answer("Paris") == "Paris"


# --------------------------------------------------------------------------- #
# 2. score_answer — abstention vs positive, verdict override
# --------------------------------------------------------------------------- #


def _rec(cls, gold, answer, *, abstain=False, cid="v||q"):
    return AnswerRecord(
        cid=cid, variant="v", reporting_class=cls,
        is_abstention=abstain, gold_answer=gold, answer=answer,
    )


def test_score_answer_idk_on_positive_is_miss() -> None:
    # "I don't know" -> None upstream; on a positive query that is a miss.
    assert score_answer(_rec("factoid", "42", normalize_answer("I don't know"))) == 0.0


def test_score_answer_idk_is_correct_abstention() -> None:
    assert score_answer(_rec("negative", "", normalize_answer("I don't know"), abstain=True)) == 1.0
    # answering an abstention query is a false positive
    assert score_answer(_rec("negative", "", "it's 7", abstain=True)) == 0.0


def test_score_answer_verdict_overrides_substring() -> None:
    # rephrased answer that omits the gold token: _match=0 but a True verdict=1.0
    rec = _rec("temporal", "1985", "nineteen eighty five")
    assert score_answer(rec) == 0.0                       # strict substring
    assert score_answer(rec, verdict=True) == 1.0         # judge says equivalent
    # a substring-true candidate the judge rejects -> 0.0
    assert score_answer(_rec("factoid", "42", "it is 42"), verdict=False) == 0.0


# --------------------------------------------------------------------------- #
# 3. The drift lock — batch score_e2e == shared score_answers on fixed input
# --------------------------------------------------------------------------- #


def test_sync_and_batch_paths_agree_on_fixed_input() -> None:
    """Pin that the batch sidecar->record mapping and the sync record mapping feed
    the SAME shared scorer to the SAME result — so the two paths cannot drift."""
    # (variant, class, gold, raw_reader_output, is_abstention)
    rows = [
        ("bm25", "factoid", "42", "the answer is 42", False),     # hit
        ("bm25", "temporal", "1985", "I don't know", False),      # abstain phrase -> miss
        ("bm25", "factoid", "", "I don't know", True),            # correct abstention
        ("bm25", "factoid", "", "actually it's 7", True),         # abstention false positive
    ]
    sidecar: dict = {}
    answers: dict = {}
    sync_records: list[AnswerRecord] = []
    for i, (variant, cls, gold, raw, abstain) in enumerate(rows):
        cid = f"{variant}||q{i}"
        # batch path: parse_batch_output normalizes the raw content
        sidecar[cid] = {"variant": variant, "qid": f"q{i}", "class": cls,
                        "answer": gold, "is_abstention": abstain}
        answers[cid] = normalize_answer(raw)
        # sync path: run_e2e_loop's answerer.answer() already returns normalized
        sync_records.append(AnswerRecord(
            cid=cid, variant=variant, reporting_class=cls,
            is_abstention=abstain, gold_answer=gold, answer=normalize_answer(raw),
        ))

    batch_out = score_e2e(sidecar, answers)
    sync_out = score_answers(sync_records)

    assert batch_out == sync_out  # the drift lock
    # and the values are what we expect (the "I don't know" rows scored correctly)
    fac = batch_out["bm25"]["per_class_accuracy"]["factoid"]
    assert fac == round(2 / 3, 4)  # hit + correct-abstention + abstention-FP (_mean rounds 4dp)
    assert batch_out["bm25"]["per_class_accuracy"]["temporal"] == 0.0  # IDK on positive
    assert batch_out["bm25"]["n_answered"] == 2  # only the two non-abstaining outputs


def test_score_answers_verdicts_plumb_through() -> None:
    recs = [
        AnswerRecord("v||a", "v", "temporal", False, "1985", "nineteen eighty five"),
        AnswerRecord("v||b", "v", "temporal", False, "2001", "in 2001"),
    ]
    # no verdicts -> strict _match: a=0 (rephrased), b=1 (substring)
    assert score_answers(recs)["v"]["per_class_accuracy"]["temporal"] == pytest.approx(0.5)
    # judge says a is equivalent; b missing from verdicts -> falls back to _match (1)
    out = score_answers(recs, verdicts={"v||a": True})
    assert out["v"]["per_class_accuracy"]["temporal"] == pytest.approx(1.0)


# --------------------------------------------------------------------------- #
# 4. Answerer back-port — AirlockAnswerer abstains on "I don't know"
# --------------------------------------------------------------------------- #


class _FakeResp:
    def __init__(self, payload: str) -> None:
        self._payload = payload

    def __enter__(self) -> "_FakeResp":
        return self

    def __exit__(self, *_a) -> bool:
        return False

    def read(self) -> bytes:
        return self._payload.encode("utf-8")


def _fake_urlopen_returning(content: str):
    payload = json.dumps({"choices": [{"message": {"content": content}}]})

    def _open(req, timeout=None):  # noqa: ARG001
        return _FakeResp(payload)

    return _open


def test_airlock_answerer_abstains_on_i_dont_know(monkeypatch) -> None:
    monkeypatch.setattr("urllib.request.urlopen", _fake_urlopen_returning("I don't know"))
    ans = AirlockAnswerer("test-model")._complete("prompt", "question", [])
    assert ans is None  # the back-port: previously returned the raw string


def test_airlock_answerer_returns_real_answer(monkeypatch) -> None:
    monkeypatch.setattr("urllib.request.urlopen", _fake_urlopen_returning("  Paris  "))
    ans = AirlockAnswerer("test-model")._complete("prompt", "question", [])
    assert ans == "Paris"
