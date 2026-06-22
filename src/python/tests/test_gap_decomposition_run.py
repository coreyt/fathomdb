"""Gap-decomposition runner — budget safety, distiller blindness, oracle fit, and
the D0b-checkpoint HARD-STOP (design §4/§6). Backend-light: fakes only (no DB, no
LLM, no mem0, no fathomdb extension build)."""

from __future__ import annotations

import inspect
import json
from pathlib import Path
from typing import Any, Optional

import pytest

from eval.gap_decomposition_run import (
    CHEAP_READER_DEFAULT,
    NEW_ARMS,
    STRONG_READER_DEFAULT,
    BlindDistiller,
    BudgetExceeded,
    BudgetLedger,
    CheckpointMissingRecords,
    UnpinnedPricing,
    answer_retention,
    answer_with_budget,
    component_paired_deltas,
    decide_all_classes,
    distill_corpus,
    load_d0b_cells,
    oracle_context,
    per_component_table,
    price_for,
    resolve_distiller_model,
    run_gap_decomposition,
)
from eval.r2_parity_eval import BaseAnswerer, GoldQuery, Hit


# --------------------------------------------------------------------------- #
# Fakes.
# --------------------------------------------------------------------------- #
class _RecClient:
    """Recording distiller backend: captures every prompt it is asked to complete."""

    model_id = "fake-distiller-v1"

    def __init__(self) -> None:
        self.seen: list[str] = []

    def complete(self, prompt: str) -> str:
        self.seen.append(prompt)
        return "ONE LINE SUMMARY"


class _SpyAnswerer(BaseAnswerer):
    model_id = "spy-answerer-v1"

    def __init__(self) -> None:
        self.calls = 0

    @property
    def available(self) -> bool:
        return True

    def answer(self, question: str, context: list[str]) -> Optional[str]:
        self.calls += 1
        return context[0] if context else None


class _EchoAnswerer(BaseAnswerer):
    """Returns the first context body (so a context containing the gold answer
    string scores 1.0; a distilled/empty context that loses it scores 0.0)."""

    model_id = "echo-answerer-v1"

    @property
    def available(self) -> bool:
        return True

    def answer(self, question: str, context: list[str]) -> Optional[str]:
        return context[0] if context else None


class _FakeAdapter:
    name = "fathomdb"

    def __init__(self, hits_by_q: dict[str, list[Hit]]) -> None:
        self._h = hits_by_q

    def retrieve(self, question: str, k: int) -> list[Hit]:
        return list(self._h.get(question, []))[:k]


# --------------------------------------------------------------------------- #
# (b) fail-closed pricing + pre-call $-cap halts BEFORE the call.
# --------------------------------------------------------------------------- #
def test_price_for_fail_closed_on_unpinned_model() -> None:
    assert price_for("gpt-5.4") == (1.25, 5.00)
    with pytest.raises(UnpinnedPricing):
        price_for("some-unpinned-model")


def test_budget_guard_halts_before_exceeding_cap() -> None:
    # Opening balance one tick under the cap → any projection exceeds.
    ledger = BudgetLedger(opening_balance_usd=29.9999, hard_cap_usd=30.0, max_output_tokens=512)
    with pytest.raises(BudgetExceeded):
        ledger.guard("gpt-5.4", prompt_tokens=10)


def test_budget_guard_passes_within_cap_and_records() -> None:
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=512)
    proj = ledger.guard("gpt-5.4", prompt_tokens=1000)
    assert proj > 0.0
    ledger.record("gpt-5.4", prompt_tokens=1000, completion_tokens=100)
    assert ledger.spent > 10.7479
    assert ledger.remaining < 30.0 - 10.7479 + 1e-9


def test_answer_with_budget_does_not_call_when_guard_trips() -> None:
    ledger = BudgetLedger(opening_balance_usd=29.9999, hard_cap_usd=30.0, max_output_tokens=512)
    spy = _SpyAnswerer()
    with pytest.raises(BudgetExceeded):
        answer_with_budget(spy, reader="gpt-5.4", question="q", context=["ctx"], ledger=ledger)
    assert spy.calls == 0  # halted BEFORE the call


def test_unpinned_reader_fails_closed_in_guard() -> None:
    ledger = BudgetLedger()
    with pytest.raises(UnpinnedPricing):
        ledger.guard("mystery-reader", prompt_tokens=10)


# --------------------------------------------------------------------------- #
# (b2) the distiller is a CHEAP model, NEVER the priced reader (the flagged seam).
# --------------------------------------------------------------------------- #
def test_default_distiller_is_cheap_not_the_priced_reader() -> None:
    # --reader gpt-5.4 with the DEFAULT distiller ⇒ a cheap model, never the priced
    # strong reader (the distiller must not be gpt-5.4; design §4).
    model = resolve_distiller_model(None, STRONG_READER_DEFAULT)
    assert model == CHEAP_READER_DEFAULT
    assert model != STRONG_READER_DEFAULT
    assert model != "gpt-5.4"
    # The cheap distiller is still PINNED so the ledger cap stays enforceable
    # (price_for fail-closed; the cap on the distiller must remain projectable).
    pin, pout = price_for(model)  # does not raise UnpinnedPricing
    assert pin > 0.0 and pout > 0.0


def test_distiller_cannot_be_the_priced_reader() -> None:
    # Explicitly wiring the distiller to the priced strong reader is fail-closed.
    with pytest.raises(SystemExit):
        resolve_distiller_model("gpt-5.4", "gpt-5.4")
    with pytest.raises(SystemExit):
        resolve_distiller_model(STRONG_READER_DEFAULT, STRONG_READER_DEFAULT)


# --------------------------------------------------------------------------- #
# (c) distiller blindness: the distiller sees neither query nor answers.
# --------------------------------------------------------------------------- #
def test_distiller_is_structurally_query_and_answer_blind() -> None:
    # Signatures cannot accept a query or answers (structural blindness).
    assert set(inspect.signature(BlindDistiller.distill).parameters) == {"self", "body"}
    assert "query" not in inspect.signature(distill_corpus).parameters
    assert "answers" not in inspect.signature(distill_corpus).parameters


def test_distiller_prompt_contains_only_the_body() -> None:
    client = _RecClient()
    distiller = BlindDistiller(client)
    out = distiller.distill("The capital of France is Paris.")
    assert out == "ONE LINE SUMMARY"
    prompt = client.seen[0]
    assert "The capital of France is Paris." in prompt
    # The query + answers that exist elsewhere in the eval are NEVER in the prompt.
    assert "SECRET_QUERY" not in prompt
    assert "SECRET_ANSWER" not in prompt


def test_distill_corpus_caches_per_doc_and_is_blind(tmp_path: Path) -> None:
    client = _RecClient()
    distiller = BlindDistiller(client)
    documents = {"d1": "body one about cats", "d2": "body two about dogs"}
    cache = distill_corpus(documents, distiller, cache_path=tmp_path / "distill.json")
    assert set(cache) == {"d1", "d2"}
    assert cache["d1"]["distilled"] == "ONE LINE SUMMARY"
    assert cache["d1"]["model"] == "fake-distiller-v1"
    assert "hash" in cache["d1"]
    # Only bodies were ever sent; no query/answers leaked.
    joined = "\n".join(client.seen)
    assert "body one about cats" in joined and "body two about dogs" in joined
    assert "SECRET_QUERY" not in joined and "SECRET_ANSWER" not in joined
    # Re-run reuses the cache (no extra distill calls).
    n_before = len(client.seen)
    distill_corpus(documents, distiller, cache_path=tmp_path / "distill.json")
    assert len(client.seen) == n_before


def test_distill_corpus_is_budget_capped(tmp_path: Path) -> None:
    client = _RecClient()
    distiller = BlindDistiller(client)
    # A ledger with essentially no room → the pre-call guard halts the distiller.
    ledger = BudgetLedger(opening_balance_usd=29.9999, hard_cap_usd=30.0, max_output_tokens=512)
    with pytest.raises(BudgetExceeded):
        distill_corpus(
            {"d1": "x" * 4000}, distiller,
            cache_path=tmp_path / "d.json", ledger=ledger, priced_model="gpt-5.4",
        )
    assert client.seen == []  # never called


# --------------------------------------------------------------------------- #
# (d) oracle context + oracle_fit_complete.
# --------------------------------------------------------------------------- #
def test_oracle_context_builds_gold_docs_and_fit_flag() -> None:
    docs = {"g1": "AAAA", "g2": "BBBB", "x": "ZZZZ"}
    ctx, complete = oracle_context(["g1", "g2"], docs, budget=100)
    assert ctx == ["AAAA", "BBBB"] and complete is True


def test_oracle_fit_incomplete_under_truncation() -> None:
    docs = {"g1": "AAAA", "g2": "BBBB"}
    ctx, complete = oracle_context(["g1", "g2"], docs, budget=6)
    assert complete is False  # 8 chars of gold > 6-char budget → truncated
    assert ctx[0] == "AAAA"


def test_oracle_incomplete_on_missing_gold_doc() -> None:
    docs = {"g1": "AAAA"}
    ctx, complete = oracle_context(["g1", "missing"], docs, budget=100)
    assert complete is False  # a required gold doc absent from the corpus


def test_oracle_empty_gold_is_incomplete() -> None:
    assert oracle_context([], {"g1": "AAAA"}) == ([], False)


# --------------------------------------------------------------------------- #
# (e) checkpoint HARD-STOP when per-question records absent.
# --------------------------------------------------------------------------- #
def test_load_d0b_cells_hard_stops_without_records(tmp_path: Path) -> None:
    # An aggregate-only D0b artifact (like 0.8.3-d0b-parity-n606.json) has NO records.
    aggregate = tmp_path / "agg.json"
    aggregate.write_text(json.dumps({"accuracy_deltas": {}, "n_questions": 606}), encoding="utf-8")
    with pytest.raises(CheckpointMissingRecords):
        load_d0b_cells(aggregate)
    empty = tmp_path / "empty.json"
    empty.write_text(json.dumps({"records": []}), encoding="utf-8")
    with pytest.raises(CheckpointMissingRecords):
        load_d0b_cells(empty)


def test_load_d0b_cells_returns_per_question_cells(tmp_path: Path) -> None:
    ckpt = tmp_path / "ckpt.json"
    ckpt.write_text(
        json.dumps({
            "records": [
                {"qid": "a", "acc": {"fathomdb": 1.0, "mem0_oss": 0.0},
                 "answers": {"fathomdb": "x", "mem0_oss": None}},
                {"qid": "b", "acc": {"fathomdb": 0.0, "mem0_oss": 1.0}, "answers": {}},
            ]
        }),
        encoding="utf-8",
    )
    cells = load_d0b_cells(ckpt)
    assert cells[("a", "fathomdb")]["acc"] == 1.0
    assert cells[("a", "mem0_oss")]["acc"] == 0.0
    assert cells[("b", "mem0_oss")]["acc"] == 1.0


# --------------------------------------------------------------------------- #
# Component deltas reference the oracle-fit subset only.
# --------------------------------------------------------------------------- #
def test_component_deltas_exclude_unfit_oracle_questions() -> None:
    records = [
        {"reporting_class": "factoid", "oracle_fit_complete": True,
         "acc": {"oracle_raw": 1.0, "fathomdb": 0.0}},
        {"reporting_class": "factoid", "oracle_fit_complete": False,  # excluded
         "acc": {"oracle_raw": 1.0, "fathomdb": 0.0}},
    ]
    d = component_paired_deltas(records, component="RETRIEVAL", cls="factoid")
    assert d == [1.0]


# --------------------------------------------------------------------------- #
# End-to-end runner with fakes (machinery green; doubles the cheap-validate path).
# --------------------------------------------------------------------------- #
def _gold(qid: str, cls: str, gold_ids: tuple[str, ...], answer: str) -> GoldQuery:
    return GoldQuery(query_id=qid, question=f"q-{qid}", reporting_class=cls,
                     answers=(answer,), gold_doc_ids=gold_ids)


def test_run_gap_decomposition_end_to_end_with_fakes(tmp_path: Path) -> None:
    classes = ("factoid", "knowledge_update", "multi_session", "temporal")
    queries: list[GoldQuery] = []
    documents: dict[str, str] = {}
    d0b_cells: dict[tuple[str, str], dict[str, Any]] = {}
    distill_cache: dict[str, dict[str, Any]] = {}
    for cls in classes:
        for j in range(6):
            qid = f"{cls}-{j}"
            gid = f"doc-{qid}"
            answer = f"answer-{qid}"
            documents[gid] = f"context body containing {answer} and more text"
            distill_cache[gid] = {"distilled": f"summary mentioning {answer}", "prompt": "p",
                                  "model": "fake", "hash": "h"}
            queries.append(_gold(qid, cls, (gid,), answer))
            # Reused D0b cells: fathomdb weak, mem0 strong (drives a positive Resid).
            d0b_cells[(qid, "fathomdb")] = {"acc": 0.0, "answer": None}
            d0b_cells[(qid, "mem0_oss")] = {"acc": 1.0, "answer": "x"}

    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
    art = run_gap_decomposition(
        queries=queries, documents=documents, d0b_cells=d0b_cells,
        distill_cache=distill_cache, answerer=_EchoAnswerer(), ledger=ledger,
        reader="gpt-5.4", output=tmp_path / "out.json", fathomdb_adapter=None,
        budget=32000, n_boot=200, classes=classes,
    )
    assert art["schema"] == "0.8.3-gap-decomposition-v1"
    assert set(art["verdicts"]) == set(classes) | {"pooled"}
    # Echo answerer over the raw oracle context (contains the answer) → oracle_raw acc 1.0.
    table = art["component_deltas"]
    assert table["factoid"]["RETRIEVAL"]["n"] == 6
    # The ledger booked spend for the new-arm reader calls and stayed within cap.
    assert art["ledger"]["spent_usd"] >= 10.7479
    assert art["ledger"]["spent_usd"] <= 30.0
    # Retention diagnostic present for every ACTIVE new arm. With no fathomdb adapter
    # the fathomdb_distilled arm is cleanly absent (fix-4 [P2]).
    assert set(art["answer_retention"]) == {"oracle_raw", "oracle_distilled"}
    assert art["answer_retention"]["oracle_raw"]["retention"] == 1.0


def test_per_component_table_and_decide_all_classes_shape() -> None:
    records = []
    for cls in ("factoid", "knowledge_update", "multi_session", "temporal"):
        for j in range(8):
            records.append({
                "reporting_class": cls, "has_answers": True, "oracle_fit_complete": True,
                "acc": {"oracle_raw": 1.0, "fathomdb": 0.0, "oracle_distilled": 1.0, "mem0_oss": 1.0},
            })
    table = per_component_table(records, n_boot=200)
    verdicts = decide_all_classes(table, records)
    assert "pooled" in verdicts
    assert all(v["verdict"] in {
        "RETRIEVAL_DOMINANT", "DISTILLED_FORM_DOMINANT", "MEM0_RESIDUAL_DOMINANT", "INCONCLUSIVE"
    } for v in verdicts.values())


def test_answer_retention_separates_lossy_distill() -> None:
    records = [
        {"has_answers": True, "context_has_gold": {"oracle_raw": True, "oracle_distilled": False}},
        {"has_answers": True, "context_has_gold": {"oracle_raw": True, "oracle_distilled": True}},
    ]
    raw = answer_retention(records, arm="oracle_raw")
    dist = answer_retention(records, arm="oracle_distilled")
    assert raw["retention"] == 1.0
    assert dist["retention"] == 0.5  # a lossy-distill signal, reported separately


# --------------------------------------------------------------------------- #
# fix-2 [P2] — `--mode cheap` defaults to the CHEAP reader (codex §9 P2).
# --------------------------------------------------------------------------- #
def test_cheap_mode_defaults_to_cheap_reader() -> None:
    from eval.gap_decomposition_run import resolve_reader

    # cheap-mode with no explicit --reader must NOT select the priced gpt-5.4.
    assert resolve_reader("cheap", None) == CHEAP_READER_DEFAULT
    assert resolve_reader("cheap", None) != STRONG_READER_DEFAULT
    assert resolve_reader("cheap", None) != "gpt-5.4"
    # full-mode keeps the strong reader; an explicit --reader always wins.
    assert resolve_reader("full", None) == STRONG_READER_DEFAULT
    assert resolve_reader("cheap", "gpt-5.4") == "gpt-5.4"
    assert resolve_reader("full", CHEAP_READER_DEFAULT) == CHEAP_READER_DEFAULT


# --------------------------------------------------------------------------- #
# fix-2 [P1]#2 — the distiller bypasses the QA answer template (codex §9 P1#2).
# A real cheap model behind the QA template returns abstentions ("I don't know"),
# corrupting BOTH distilled arms. The distiller must send the RAW distill prompt.
# --------------------------------------------------------------------------- #
class _FakeCtx:
    def __enter__(self) -> "_FakeCtx":
        return self

    def __exit__(self, *_: Any) -> bool:
        return False

    def read(self) -> bytes:
        return json.dumps(
            {
                "choices": [{"message": {"content": "a one line summary of the doc"}}],
                "usage": {"prompt_tokens": 7, "completion_tokens": 3},
            }
        ).encode("utf-8")


class _CapturingOpen:
    """Captures the outgoing urllib request (exercises the payload-build path)."""

    def __init__(self) -> None:
        self.reqs: list[Any] = []

    def __call__(self, req: Any) -> _FakeCtx:
        self.reqs.append(req)
        return _FakeCtx()


def test_distiller_uses_raw_completion_not_qa_template() -> None:
    from eval.gap_decomposition_run import RawCompletionDistillerClient
    from eval.m1_baseline_run import CostTrackingAnswerer

    ans = CostTrackingAnswerer("gemini-flash-lite", timeout_s=5.0)
    ans.base_url = "http://airlock.test"
    ans.api_key = "k"
    cap = _CapturingOpen()
    ans._open = cap  # type: ignore[method-assign]  # inject the POST seam

    client = RawCompletionDistillerClient(ans)
    distiller = BlindDistiller(client)
    out = distiller.distill("The capital of France is Paris.")

    assert out  # a one-line summary came back, not an abstention
    assert len(cap.reqs) == 1
    payload = json.loads(cap.reqs[0].data.decode("utf-8"))
    content = payload["messages"][0]["content"]
    # The RAW distill prompt is the user message (body + the distill instruction).
    assert "The capital of France is Paris." in content
    assert "One-line summary" in content
    # The QA answer template / empty-context abstention instruction must NOT leak in.
    assert "I don't know" not in content
    assert "precise question-answering assistant" not in content
    assert "ONLY the provided context" not in content


# --------------------------------------------------------------------------- #
# fix-2 [P1]#1 — the $ ledger spend is persisted + restored across a resume
# (budget integrity: the $30 cap is per-EXPERIMENT, not per-PROCESS; codex §9 P1#1).
# --------------------------------------------------------------------------- #
def _decomp_fixtures(
    qids: tuple[str, ...],
) -> tuple[list[GoldQuery], dict[str, str], dict[str, dict[str, Any]], dict[tuple[str, str], dict[str, Any]]]:
    queries: list[GoldQuery] = []
    documents: dict[str, str] = {}
    distill_cache: dict[str, dict[str, Any]] = {}
    d0b_cells: dict[tuple[str, str], dict[str, Any]] = {}
    for qid in qids:
        gid = f"doc-{qid}"
        answer = f"answer-{qid}"
        documents[gid] = f"context body containing {answer} and more text"
        distill_cache[gid] = {"distilled": f"summary mentioning {answer}", "prompt": "p",
                              "model": "fake", "hash": "h"}
        queries.append(_gold(qid, "factoid", (gid,), answer))
        d0b_cells[(qid, "fathomdb")] = {"acc": 0.0, "answer": None}
        d0b_cells[(qid, "mem0_oss")] = {"acc": 1.0, "answer": "x"}
    return queries, documents, distill_cache, d0b_cells


def test_checkpoint_persists_ledger_spend(tmp_path: Path) -> None:
    queries, documents, distill_cache, d0b_cells = _decomp_fixtures(tuple(f"factoid-{j}" for j in range(4)))
    out = tmp_path / "out.json"
    ckpt = out.with_suffix(".checkpoint.json")
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
    art = run_gap_decomposition(
        queries=queries, documents=documents, d0b_cells=d0b_cells,
        distill_cache=distill_cache, answerer=_EchoAnswerer(), ledger=ledger,
        reader="gpt-5.4", output=out, fathomdb_adapter=None,
        budget=32000, n_boot=50, classes=("factoid",), checkpoint_path=ckpt, checkpoint_every=1,
    )
    assert art["ledger"]["spent_usd"] > 10.7479  # reader calls booked spend
    # The cumulative spend is persisted in the checkpoint (atomic with the records).
    persisted = json.loads(ckpt.read_text(encoding="utf-8"))
    assert "ledger_spent_usd" in persisted
    assert persisted["ledger_spent_usd"] == pytest.approx(art["ledger"]["spent_usd"])


def test_resume_restores_ledger_spend_and_trips_cap(tmp_path: Path) -> None:
    queries, documents, distill_cache, d0b_cells = _decomp_fixtures(("factoid-0", "factoid-1"))
    out = tmp_path / "out.json"
    ckpt = out.with_suffix(".checkpoint.json")
    # A prior checkpoint that already spent up to one tick under the $30 cap, with
    # factoid-0's new-arm answers already paid for (so it resumes from the rmap).
    ckpt.write_text(
        json.dumps({
            "records": [
                {"qid": "factoid-0", "reporting_class": "factoid",
                 "answers": {a: "prior" for a in NEW_ARMS}},
            ],
            "ledger_spent_usd": 29.999,
            "mode": "run", "reader": "gpt-5.4",
        }),
        encoding="utf-8",
    )
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=512)
    art = run_gap_decomposition(
        queries=queries, documents=documents, d0b_cells=d0b_cells,
        distill_cache=distill_cache, answerer=_EchoAnswerer(), ledger=ledger,
        reader="gpt-5.4", output=out, fathomdb_adapter=None,
        budget=32000, n_boot=50, classes=("factoid",), checkpoint_path=ckpt, checkpoint_every=1,
    )
    # Restoring the prior $29.999 spend (NOT just the $10.7479 opening) means the
    # NEW factoid-1 reader call trips the cap and the run aborts BEFORE the call.
    assert art["ledger"]["spent_usd"] == pytest.approx(29.999, abs=1e-3)
    assert art["aborted_for_cap"] is True


def test_budget_ledger_restore_spent_sets_running_total() -> None:
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0)
    assert ledger.spent == pytest.approx(10.7479)
    ledger.restore_spent(25.5)
    assert ledger.spent == pytest.approx(25.5)
    assert ledger.remaining == pytest.approx(4.5)


# --------------------------------------------------------------------------- #
# fix-3 [P2] — the strong/priced model is rejected as the distiller in ALL modes
# (codex §9 P2). The old guard only fired when distiller==reader AND reader was
# strong, so `--mode cheap --distiller gpt-5.4` (reader resolved cheap) slipped a
# priced distiller through. The distiller must NEVER be STRONG_READER_DEFAULT.
# --------------------------------------------------------------------------- #
def test_strong_distiller_rejected_in_all_modes() -> None:
    # The flagged hole: reader resolved to a CHEAP model, but the distiller is the
    # priced strong reader — must STILL fail closed.
    with pytest.raises(SystemExit):
        resolve_distiller_model("gpt-5.4", "gemini-flash-lite")
    with pytest.raises(SystemExit):
        resolve_distiller_model(STRONG_READER_DEFAULT, CHEAP_READER_DEFAULT)
    # Independent of the reader: even with no reader context the strong model is
    # rejected as the distiller.
    with pytest.raises(SystemExit):
        resolve_distiller_model(STRONG_READER_DEFAULT, STRONG_READER_DEFAULT)
    # A cheap distiller is always fine (with a cheap or any reader).
    assert resolve_distiller_model("gemini-flash-lite", "gemini-flash-lite") == "gemini-flash-lite"
    assert resolve_distiller_model("gemini-flash-lite", STRONG_READER_DEFAULT) == "gemini-flash-lite"


# --------------------------------------------------------------------------- #
# fix-3 [P1] — an incomplete / cap-aborted run is NON-CITABLE: it must NOT publish
# a DOMINANT verdict over the completed prefix (codex §9 P1). A low-variance prefix
# could otherwise emit a powered DOMINANT verdict for an INCOMPLETE experiment.
# --------------------------------------------------------------------------- #
class _CountingEchoAnswerer(BaseAnswerer):
    """Echoes context[0] (so the raw-oracle arm scores 1.0 and the prefix WOULD be
    RETRIEVAL_DOMINANT) AND books a large fixed prompt-token cost per call, so the
    ledger predictably trips the $30 cap part-way through the query set."""

    model_id = "counting-echo-v1"

    def __init__(self, tokens_per_call: int) -> None:
        self.prompt_tokens = 0
        self.completion_tokens = 0
        self._tpc = tokens_per_call

    @property
    def available(self) -> bool:
        return True

    def answer(self, question: str, context: list[str]) -> Optional[str]:
        self.prompt_tokens += self._tpc
        return context[0] if context else None


def test_capped_run_is_non_citable_never_dominant(tmp_path: Path) -> None:
    # 10 answerable factoid queries; the completed prefix (constant RETRIEVAL=1.0
    # deltas, zero variance) WOULD score RETRIEVAL_DOMINANT if it were published.
    queries, documents, distill_cache, d0b_cells = _decomp_fixtures(tuple(f"factoid-{j}" for j in range(10)))
    out = tmp_path / "out.json"
    ckpt = out.with_suffix(".checkpoint.json")
    # ~$1.25 booked per reader call (1M prompt tokens @ gpt-5.4 in-price) with a
    # ~$19.25 budget head-room ⇒ the cap trips after ~5 full queries (mid-way).
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
    art = run_gap_decomposition(
        queries=queries, documents=documents, d0b_cells=d0b_cells,
        distill_cache=distill_cache, answerer=_CountingEchoAnswerer(1_000_000), ledger=ledger,
        reader="gpt-5.4", output=out, fathomdb_adapter=None,
        budget=32000, n_boot=50, classes=("factoid",), checkpoint_path=ckpt, checkpoint_every=1,
    )
    # The cap tripped mid-way → the run is non-citable / invalid.
    assert art["aborted_for_cap"] is True
    assert art["citable"] is False
    assert art["run_valid"] is False
    assert art["verdict"] == "ABORTED_INCOMPLETE"
    assert art["answer_completeness"] < 1.0
    # The completed prefix held >= 2 RETRIEVAL deltas (it WOULD have been DOMINANT)
    # but NO DOMINANT verdict may be published for any class or pooled.
    assert art["component_deltas"]["pooled"]["RETRIEVAL"]["n"] >= 2
    for cls, dec in art["verdicts"].items():
        assert "DOMINANT" not in dec["verdict"], cls
        assert dec["verdict"] == "ABORTED_INCOMPLETE"


def test_incomplete_run_below_floor_is_non_citable(tmp_path: Path) -> None:
    # An unavailable reader produces no new-arm answers → answer-completeness below
    # the floor → non-citable, even though the budget cap never tripped.
    queries, documents, distill_cache, d0b_cells = _decomp_fixtures(tuple(f"factoid-{j}" for j in range(4)))

    class _Unavailable(BaseAnswerer):
        model_id = "unavailable-v1"

        @property
        def available(self) -> bool:
            return False

        def answer(self, question: str, context: list[str]) -> Optional[str]:
            return None

    out = tmp_path / "out.json"
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
    art = run_gap_decomposition(
        queries=queries, documents=documents, d0b_cells=d0b_cells,
        distill_cache=distill_cache, answerer=_Unavailable(), ledger=ledger,
        reader="gpt-5.4", output=out, fathomdb_adapter=None,
        budget=32000, n_boot=50, classes=("factoid",),
    )
    assert art["aborted_for_cap"] is False
    assert art["citable"] is False
    assert art["run_valid"] is False
    assert art["verdict"] == "ABORTED_INCOMPLETE"
    for dec in art["verdicts"].values():
        assert "DOMINANT" not in dec["verdict"]


def test_complete_run_is_citable_and_may_publish_verdict(tmp_path: Path) -> None:
    # The complete fakes path stays citable (run_valid True) and the pooled verdict
    # is the real frozen decision (regression guard: the non-citable gate must NOT
    # fire on a fully-processed run).
    queries, documents, distill_cache, d0b_cells = _decomp_fixtures(tuple(f"factoid-{j}" for j in range(6)))
    out = tmp_path / "out.json"
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
    art = run_gap_decomposition(
        queries=queries, documents=documents, d0b_cells=d0b_cells,
        distill_cache=distill_cache, answerer=_EchoAnswerer(), ledger=ledger,
        reader="gpt-5.4", output=out, fathomdb_adapter=None,
        budget=32000, n_boot=50, classes=("factoid",),
    )
    assert art["aborted_for_cap"] is False
    assert art["citable"] is True
    assert art["run_valid"] is True
    assert art["answer_completeness"] == 1.0
    assert art["verdict"] == art["verdicts"]["pooled"]["verdict"]
    assert art["verdict"] != "ABORTED_INCOMPLETE"


# --------------------------------------------------------------------------- #
# fix-4 [P1]#1 — the priced DISTILLER's spend is persisted alongside the distill
# cache and restored on resume, so cached-and-skipped docs still count against the
# $30 cap (codex §9 P1#1). Without this a partial distill cache + crash lets the
# next run skip the cached docs WITHOUT restoring the dollars already paid → the
# guard would authorise > the per-experiment cap.
# --------------------------------------------------------------------------- #
def test_distill_corpus_resume_restores_prior_spend_and_trips_cap(tmp_path: Path) -> None:
    from eval.gap_decomposition_run import distill_spent_sidecar

    client = _RecClient()
    distiller = BlindDistiller(client)
    cache_path = tmp_path / "distill.json"
    # A partial cache: d1 was already distilled + PAID for in a prior process.
    cache_path.write_text(
        json.dumps({"d1": {"distilled": "s", "prompt": "p", "model": "fake-distiller-v1", "hash": "h"}}),
        encoding="utf-8",
    )
    # The distiller's cumulative spend persisted alongside the cache (near the cap).
    distill_spent_sidecar(cache_path).write_text(
        json.dumps({"ledger_spent_usd": 29.999}), encoding="utf-8"
    )
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=512)
    # Resuming with a FRESH ledger: d1 is cached+skipped, but its $29.999 prior spend
    # is RESTORED, so distilling the NEW d2 trips the cap BEFORE the call. Without the
    # fix the fresh $10.7479 ledger would wrongly authorise > the $30 cap.
    with pytest.raises(BudgetExceeded):
        distill_corpus(
            {"d1": "x" * 100, "d2": "x" * 4000}, distiller,
            cache_path=cache_path, ledger=ledger, priced_model="gemini-flash-lite",
        )
    assert client.seen == []  # d1 cached; d2 halted before the call → no distill calls
    assert ledger.spent == pytest.approx(29.999, abs=1e-3)


def test_distill_corpus_persists_and_restores_cumulative_spend(tmp_path: Path) -> None:
    from eval.gap_decomposition_run import distill_spent_sidecar

    client = _RecClient()
    distiller = BlindDistiller(client)
    cache_path = tmp_path / "distill.json"
    led1 = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
    distill_corpus({"d1": "x" * 4000}, distiller, cache_path=cache_path,
                   ledger=led1, priced_model="gemini-flash-lite")
    spent1 = led1.spent
    assert spent1 > 10.7479
    # The cumulative spend is persisted alongside the cache (sidecar).
    sidecar = distill_spent_sidecar(cache_path)
    assert sidecar.exists()
    assert json.loads(sidecar.read_text(encoding="utf-8"))["ledger_spent_usd"] == pytest.approx(spent1)
    # A resume with a FRESH ledger restores the prior spend, then pays only for d2,
    # so the cumulative total carries the already-paid d1 (cap is per-EXPERIMENT).
    led2 = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
    distill_corpus({"d1": "x" * 4000, "d2": "y" * 4000}, distiller, cache_path=cache_path,
                   ledger=led2, priced_model="gemini-flash-lite")
    assert led2.spent > spent1  # restored prior d1 spend + the new d2 spend


# --------------------------------------------------------------------------- #
# fix-4 [P1]#2 — a NON-budget answerer failure (retry-exhausted 429/5xx, or a
# non-retryable HTTP error) leaves that arm's cell ABSENT, checkpoints, and the run
# CONTINUES (failure ≠ abstention; codex §9 P1#2). It must NOT crash the run, and the
# completeness gate then marks the artifact non-citable. A BudgetExceeded still
# cleanly cap-aborts (regression guard).
# --------------------------------------------------------------------------- #
class _DistilledFailAnswerer(BaseAnswerer):
    """Raises a NON-budget error on the distilled-summary contexts (models a
    retry-exhausted 5xx) but answers the raw-oracle context normally."""

    model_id = "distill-fail-v1"

    @property
    def available(self) -> bool:
        return True

    def answer(self, question: str, context: list[str]) -> Optional[str]:
        if any("summary" in c for c in context):
            raise RuntimeError("airlock 503 — retries exhausted (non-budget)")
        return context[0] if context else None


def test_answerer_failure_is_missing_cell_not_crash(tmp_path: Path) -> None:
    queries, documents, distill_cache, d0b_cells = _decomp_fixtures(tuple(f"factoid-{j}" for j in range(4)))
    out = tmp_path / "out.json"
    ckpt = out.with_suffix(".checkpoint.json")
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
    # No crash: a non-budget answerer error leaves that arm's cell ABSENT, checkpoints,
    # and the run continues to completion (mirror d0b's failure≠abstention contract).
    art = run_gap_decomposition(
        queries=queries, documents=documents, d0b_cells=d0b_cells,
        distill_cache=distill_cache, answerer=_DistilledFailAnswerer(), ledger=ledger,
        reader="gpt-5.4", output=out, fathomdb_adapter=None,
        budget=32000, n_boot=50, classes=("factoid",), checkpoint_path=ckpt, checkpoint_every=1,
    )
    assert art["aborted_for_cap"] is False  # a per-cell failure, NOT a cap abort
    assert art["citable"] is False          # missing cells → non-citable
    assert art["run_valid"] is False
    assert art["answer_completeness"] < 1.0
    assert art["verdict"] == "ABORTED_INCOMPLETE"
    # The failed arm's cell is ABSENT (not fabricated); the raw arm succeeded.
    persisted = json.loads(ckpt.read_text(encoding="utf-8"))
    answerable = [r for r in persisted["records"] if r["has_answers"]]
    assert answerable and all("oracle_raw" in r["answers"] for r in answerable)
    assert all("oracle_distilled" not in r["answers"] for r in answerable)


# --------------------------------------------------------------------------- #
# fix-4 [P2] — fathomdb_distilled is cleanly ABSENT without an adapter (no paid
# no-op, no crash, not counted in completeness); WITH an adapter the cross-check
# delta acc_fathomdb_distilled − acc_fathomdb + per-arm accuracy are exported
# (codex §9 P2).
# --------------------------------------------------------------------------- #
def test_no_adapter_skips_fathomdb_distilled_cleanly(tmp_path: Path) -> None:
    queries, documents, distill_cache, d0b_cells = _decomp_fixtures(tuple(f"factoid-{j}" for j in range(4)))
    out = tmp_path / "out.json"
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
    spy = _SpyAnswerer()
    art = run_gap_decomposition(
        queries=queries, documents=documents, d0b_cells=d0b_cells,
        distill_cache=distill_cache, answerer=spy, ledger=ledger,
        reader="gpt-5.4", output=out, fathomdb_adapter=None,
        budget=32000, n_boot=50, classes=("factoid",),
    )
    # fathomdb_distilled is simply ABSENT (no adapter): not a paid no-op, not counted.
    assert set(art["answer_retention"]) == {"oracle_raw", "oracle_distilled"}
    assert art["active_new_arms"] == ["oracle_raw", "oracle_distilled"]
    assert spy.calls == 4 * 2  # 2 active arms × 4 questions — NOT a 3rd empty no-op call
    # The run stays citable on the two real arms (fathomdb_distilled not counted).
    assert art["citable"] is True
    assert art["answer_completeness"] == 1.0
    assert "fathomdb_distilled_crosscheck" not in art


def test_with_adapter_exports_fathomdb_distilled_crosscheck(tmp_path: Path) -> None:
    queries, documents, distill_cache, d0b_cells = _decomp_fixtures(tuple(f"factoid-{j}" for j in range(6)))
    # An adapter that retrieves each query's own gold doc (so the distilled fdb body
    # carries the answer) → a measurable acc_fathomdb_distilled − acc_fathomdb delta.
    hits_by_q = {q.question: [Hit(doc_id=q.gold_doc_ids[0], body=documents[q.gold_doc_ids[0]], score=1.0)]
                 for q in queries}
    adapter = _FakeAdapter(hits_by_q)
    out = tmp_path / "out.json"
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
    art = run_gap_decomposition(
        queries=queries, documents=documents, d0b_cells=d0b_cells,
        distill_cache=distill_cache, answerer=_EchoAnswerer(), ledger=ledger,
        reader="gpt-5.4", output=out, fathomdb_adapter=adapter,
        budget=32000, n_boot=50, classes=("factoid",),
    )
    assert art["active_new_arms"] == ["oracle_raw", "oracle_distilled", "fathomdb_distilled"]
    assert "fathomdb_distilled" in art["answer_retention"]
    cc = art["fathomdb_distilled_crosscheck"]
    assert "delta_per_class" in cc and "pooled" in cc["delta_per_class"]
    assert set(cc["per_arm_accuracy"]) == {"fathomdb", "fathomdb_distilled"}
    # fathomdb cells (acc 0.0 from d0b) vs fathomdb_distilled (echo over a distilled
    # body mentioning the answer → 1.0) → a positive cross-check delta.
    assert cc["per_arm_accuracy"]["fathomdb"] == pytest.approx(0.0)
    assert cc["per_arm_accuracy"]["fathomdb_distilled"] == pytest.approx(1.0)
    assert cc["delta_per_class"]["pooled"]["point"] == pytest.approx(1.0)
