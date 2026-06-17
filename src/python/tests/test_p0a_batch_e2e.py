"""P0-A base e2e via the airlock Batch API — pure-logic TDD (no network, no DB).

Binding spec: ``dev/design/0.8.1-graph-experiment-plan.md`` (P0-A base-retrieval,
the identical-answerer invariant, recall@K-vs-accuracy caveat §1) and the airlock
batch recipe (`docs/guide/batch.md`). These pin the load-bearing rules of
``eval.p0a_batch_e2e`` *without* the dataset, the engine, or the airlock:

1. **custom_id round-trip** — every (variant, question) maps to a stable
   ``"{variant}||{qid}"`` id present in both the JSONL and the sidecar.
2. **identical-answerer prompt** — each request uses the shared
   :class:`BaseAnswerer` template (no per-variant prompt drift).
3. **abstention normalization** — empty / "I don't know" answers become ``None``;
   malformed batch rows are counted, not crashed.
4. **scoring + the scorer-strictness lock** — ``_match`` is a normalized-substring
   check, so a correct-but-rephrased answer that omits the gold string scores 0.
   This test makes that caveat explicit (high recall can coexist with 0 accuracy).
5. **run_batch orchestration** — upload→create→poll→download against a *mock*
   client, with the provider threaded through and no real sleeping.
"""

from __future__ import annotations

import json
from dataclasses import dataclass

from eval.p0a_base_retrieval import SmokeQuestion
from eval.p0a_batch_e2e import (
    build_batch_jsonl,
    build_judge_jsonl,
    parse_batch_output,
    parse_judge_output,
    run_batch,
    score_e2e,
)
from eval.r2_parity_eval import BaseAnswerer

# --------------------------------------------------------------------------- #
# Fixtures (in-memory; no dataset / engine)
# --------------------------------------------------------------------------- #


@dataclass
class _Hit:
    doc_id: str
    body: str


@dataclass
class _FakeAdapter:
    hits: list[_Hit]

    def retrieve(self, question: str, k: int) -> list[_Hit]:
        return self.hits[:k]


@dataclass
class _FakeSmoke:
    questions: list[SmokeQuestion]


def _q(qid, cls, question, answer, gold=("s1",)):
    return SmokeQuestion(
        qid=qid, reporting_class=cls, question=question, answer=answer,
        gold_sessions=tuple(gold), haystack_session_ids=("s1", "s2"),
    )


# --------------------------------------------------------------------------- #
# 1. build_batch_jsonl — custom_id round-trip + structure
# --------------------------------------------------------------------------- #


def test_build_batch_jsonl_custom_id_roundtrip():
    smoke = _FakeSmoke([_q("q1", "factoid", "What is X?", "42"),
                        _q("q2", "temporal", "When Y?", "1985")])
    systems = {
        "bm25": _FakeAdapter([_Hit("s1", "body one"), _Hit("s2", "body two")]),
        "fts": _FakeAdapter([_Hit("s1", "body one")]),
    }
    jsonl, sidecar = build_batch_jsonl(
        smoke, systems, context_k=10, reader_model="gpt-5.4-nano", max_tokens=64,
    )
    reqs = [json.loads(ln) for ln in jsonl.splitlines() if ln.strip()]

    assert len(reqs) == 4  # 2 variants x 2 questions
    cids = {r["custom_id"] for r in reqs}
    assert cids == {"bm25||q1", "bm25||q2", "fts||q1", "fts||q2"}
    assert set(sidecar.keys()) == cids  # sidecar mirrors the JSONL exactly

    one = next(r for r in reqs if r["custom_id"] == "bm25||q1")
    assert one["method"] == "POST" and one["url"] == "/v1/chat/completions"
    assert one["body"]["model"] == "gpt-5.4-nano"
    assert one["body"]["max_completion_tokens"] == 64
    assert one["body"]["messages"][0]["role"] == "user"
    assert sidecar["bm25||q1"]["answer"] == "42"
    assert sidecar["bm25||q1"]["is_abstention"] is False


def test_build_batch_jsonl_prompt_is_identical_answerer_template():
    smoke = _FakeSmoke([_q("q1", "factoid", "What is X?", "42")])
    ctx = ["body one", "body two"]
    systems = {"bm25": _FakeAdapter([_Hit("s1", "body one"), _Hit("s2", "body two")])}
    jsonl, _ = build_batch_jsonl(
        smoke, systems, context_k=10, reader_model="m", max_tokens=8,
    )
    req = json.loads(jsonl.strip())
    expected = BaseAnswerer().build_prompt("What is X?", ctx)
    assert req["body"]["messages"][0]["content"] == expected
    # sanity: the shared template joins context with the fixed separator
    assert "body one\n---\nbody two" in req["body"]["messages"][0]["content"]


def test_build_batch_jsonl_empty_systems_is_empty():
    jsonl, sidecar = build_batch_jsonl(
        _FakeSmoke([_q("q1", "factoid", "Q", "A")]), {},
        context_k=5, reader_model="m", max_tokens=8,
    )
    assert jsonl == "" and sidecar == {}


# --------------------------------------------------------------------------- #
# 2. parse_batch_output — mapping, abstention, malformed rows
# --------------------------------------------------------------------------- #


def _row(cid, content):
    body = {} if content is None else {"choices": [{"message": {"content": content}}]}
    return json.dumps({"custom_id": cid, "response": {"body": body}})


def test_parse_batch_output_maps_normalizes_and_counts_errors():
    text = "\n".join([
        _row("a", "42"),
        _row("b", "I don't know"),   # abstention -> None
        _row("c", "   "),            # whitespace -> None
        _row("d", None),             # malformed (no choices) -> None + error
        "",                          # blank line ignored
    ]) + "\n"
    answers, parse_errors = parse_batch_output(text)

    assert answers["a"] == "42"
    assert answers["b"] is None
    assert answers["c"] is None
    assert answers["d"] is None
    assert parse_errors == 1  # only the malformed row counts


def test_parse_batch_output_empty():
    answers, parse_errors = parse_batch_output("")
    assert answers == {} and parse_errors == 0


def test_parse_batch_output_corrupt_line_is_counted_not_raised():
    # A truncated/corrupt JSONL line must not discard an already-paid batch.
    text = "\n".join([_row("a", "42"), "{not valid json", _row("b", "ok")]) + "\n"
    answers, parse_errors = parse_batch_output(text)
    assert answers == {"a": "42", "b": "ok"}
    assert parse_errors == 1


# --------------------------------------------------------------------------- #
# 3. score_e2e — aggregation, abstention, scorer-strictness lock
# --------------------------------------------------------------------------- #


def _meta(variant, cls, answer, *, abstain=False):
    return {"variant": variant, "qid": "x", "class": cls,
            "answer": answer, "is_abstention": abstain}


def test_score_e2e_aggregation_and_n_answered():
    sidecar = {
        "bm25||q1": _meta("bm25", "factoid", "42"),
        "bm25||q2": _meta("bm25", "temporal", "1985"),
    }
    answers = {"bm25||q1": "the answer is 42", "bm25||q2": "nineteen eighty five"}
    out = score_e2e(sidecar, answers)

    assert out["bm25"]["per_class_accuracy"]["factoid"] == 1.0   # "42" is a substring
    assert out["bm25"]["per_class_accuracy"]["temporal"] == 0.0  # gold string absent
    assert out["bm25"]["overall_accuracy"] == 0.5
    assert out["bm25"]["n_answered"] == 2


def test_score_e2e_abstention_rules():
    sidecar = {"v||qa": _meta("v", "factoid", "", abstain=True)}
    # reader correctly abstains (no answer present) -> correct
    assert score_e2e(sidecar, {})["v"]["per_class_accuracy"]["factoid"] == 1.0
    # reader answers an abstention question -> false positive (0.0)
    assert score_e2e(sidecar, {"v||qa": "something"})["v"]["per_class_accuracy"]["factoid"] == 0.0


def test_score_e2e_locks_substring_scorer_strictness():
    """High retrieval recall can coexist with 0 accuracy purely from the crude
    normalized-substring scorer — pin both directions so the caveat is explicit."""
    sidecar = {"v||q": _meta("v", "temporal", "1985")}
    # rephrased answer that omits the gold token -> scored WRONG (strictness)
    assert score_e2e(sidecar, {"v||q": "born in nineteen eighty-five"})[
        "v"]["per_class_accuracy"]["temporal"] == 0.0
    # answer containing the gold token -> scored right
    assert score_e2e(sidecar, {"v||q": "it happened in 1985."})[
        "v"]["per_class_accuracy"]["temporal"] == 1.0
    # an unanswered (None) positive question is a miss, not skipped
    assert score_e2e(sidecar, {})["v"]["per_class_accuracy"]["temporal"] == 0.0


# --------------------------------------------------------------------------- #
# 4. run_batch — mock client, provider threading, no real sleep
# --------------------------------------------------------------------------- #


class _FakeClient:
    def __init__(self, statuses, output="OUT", output_file_id: str | None = "out-z"):
        self._statuses = list(statuses)
        self._output = output
        self._ofid = output_file_id
        self.provider_seen: list[tuple[str, str]] = []
        self.downloaded: list[str] = []

    def upload(self, jsonl, provider):
        self.provider_seen.append(("upload", provider))
        return "file-x"

    def create(self, file_id, provider, model=None):
        self.provider_seen.append(("create", provider))
        self.create_model = model
        return "batch-y"

    def status(self, batch_id, provider):
        self.provider_seen.append(("status", provider))
        return {"status": self._statuses.pop(0), "output_file_id": self._ofid,
                "request_counts": {}}

    def download(self, file_id, provider="openai"):
        self.downloaded.append(file_id)
        return self._output


def test_run_batch_completes_and_threads_provider():
    fake = _FakeClient(["validating", "in_progress", "completed"], output="RESULT")
    slept: list[float] = []
    bid, status, out = run_batch(
        fake, "jsonl", "vertex_ai", poll_secs=7, max_polls=10,
        sleep=slept.append,
    )
    assert (bid, status, out) == ("batch-y", "completed", "RESULT")
    # every call carried the chosen provider, not a hard-coded one
    assert {p for _, p in fake.provider_seen} == {"vertex_ai"}
    # slept once per non-terminal poll (validating, in_progress), not after completion
    assert slept == [7, 7]


def test_run_batch_terminal_failure_returns_no_output():
    fake = _FakeClient(["failed"])
    bid, status, out = run_batch(fake, "j", "openai", poll_secs=0, max_polls=5)
    assert status == "failed" and out is None


def test_run_batch_exhausts_polls_without_completion():
    fake = _FakeClient(["in_progress", "in_progress"])
    bid, status, out = run_batch(fake, "j", "openai", poll_secs=0, max_polls=2)
    assert status == "in_progress" and out is None


def test_run_batch_completed_without_output_file_id_does_not_call_download():
    # Defensive: a "completed" with no output_file_id must not GET /files/None/content.
    fake = _FakeClient(["completed"], output_file_id=None)
    bid, status, out = run_batch(fake, "j", "openai", poll_secs=0, max_polls=3)
    assert status == "completed" and out is None
    assert fake.downloaded == []


# --------------------------------------------------------------------------- #
# 5. LLM judge — build_judge_jsonl / parse_judge_output / verdict plumbing
# --------------------------------------------------------------------------- #


def _jmeta(variant, cls, question, answer, *, abstain=False):
    return {"variant": variant, "qid": "x", "class": cls, "question": question,
            "answer": answer, "is_abstention": abstain}


def test_build_judge_jsonl_only_judges_positive_answered():
    sidecar = {
        "v||p": _jmeta("v", "factoid", "When?", "1985"),       # positive, answered -> judged
        "v||a": _jmeta("v", "factoid", "When?", "", abstain=True),  # abstention -> skipped
        "v||u": _jmeta("v", "factoid", "Who?", "x"),           # positive, unanswered -> skipped
    }
    answers = {"v||p": "nineteen eighty five", "v||u": None}
    jsonl, judged = build_judge_jsonl(sidecar, answers, judge_model="gpt-5.4-nano")
    reqs = [json.loads(ln) for ln in jsonl.splitlines() if ln.strip()]

    assert {r["custom_id"] for r in reqs} == {"v||p"}  # only the answered positive
    assert set(judged.keys()) == {"v||p"}
    body = reqs[0]["body"]
    assert body["model"] == "gpt-5.4-nano"
    assert body["temperature"] == 0
    content = body["messages"][0]["content"]
    # the prompt carries question, reference (gold) and candidate
    assert "When?" in content and "1985" in content and "nineteen eighty five" in content


def test_build_judge_jsonl_empty_when_nothing_to_judge():
    sidecar = {"v||a": _jmeta("v", "factoid", "Q", "", abstain=True)}
    jsonl, judged = build_judge_jsonl(sidecar, {}, judge_model="m")
    assert jsonl == "" and judged == {}


def _jrow(cid, content):
    body = {"choices": [{"message": {"content": content}}]}
    return json.dumps({"custom_id": cid, "response": {"body": body}})


def test_parse_judge_output_true_false():
    text = "\n".join([
        _jrow("a", '{"correct": true}'),
        _jrow("b", '{"correct": false}'),
    ]) + "\n"
    verdicts, errs = parse_judge_output(text)
    assert verdicts == {"a": True, "b": False}
    assert errs == 0


def test_parse_judge_output_tolerates_fences_and_prose():
    text = "\n".join([
        _jrow("a", '```json\n{"correct": true}\n```'),
        _jrow("b", 'Verdict: {"correct": false}.'),
        _jrow("c", '{"correct": "yes"}'),  # stringy boolean
    ]) + "\n"
    verdicts, errs = parse_judge_output(text)
    assert verdicts == {"a": True, "b": False, "c": True}
    assert errs == 0


def test_parse_judge_output_malformed_counts_error_and_omits():
    text = "\n".join([
        _jrow("a", '{"correct": true}'),
        _jrow("b", "maybe?"),       # no parseable verdict -> omitted + error
        _jrow("c", None),           # no content -> omitted + error
    ]) + "\n"
    verdicts, errs = parse_judge_output(text)
    assert verdicts == {"a": True}   # b, c omitted (NOT defaulted)
    assert errs == 2


def test_score_e2e_judge_overrides_substring():
    # candidate omits the gold token: _match=0, but a True verdict -> 1.0
    sidecar = {"v||q": _jmeta("v", "temporal", "When?", "1985")}
    answers = {"v||q": "nineteen eighty five"}
    assert score_e2e(sidecar, answers)["v"]["per_class_accuracy"]["temporal"] == 0.0
    assert score_e2e(sidecar, answers, verdicts={"v||q": True})[
        "v"]["per_class_accuracy"]["temporal"] == 1.0
    # a substring-true candidate the judge rejects -> 0.0
    sidecar2 = {"v||q": _jmeta("v", "factoid", "X?", "42")}
    answers2 = {"v||q": "it is 42"}
    assert score_e2e(sidecar2, answers2)["v"]["per_class_accuracy"]["factoid"] == 1.0
    assert score_e2e(sidecar2, answers2, verdicts={"v||q": False})[
        "v"]["per_class_accuracy"]["factoid"] == 0.0


def test_score_e2e_missing_verdict_falls_back_to_match():
    # verdicts present but this cid absent -> graceful fallback to _match (substring)
    sidecar = {"v||q": _jmeta("v", "factoid", "X?", "42")}
    answers = {"v||q": "the answer is 42"}
    out = score_e2e(sidecar, answers, verdicts={"other||z": True})
    assert out["v"]["per_class_accuracy"]["factoid"] == 1.0
