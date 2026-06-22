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
    # Retention diagnostic present for every new arm.
    assert set(art["answer_retention"]) == {"oracle_raw", "oracle_distilled", "fathomdb_distilled"}
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
