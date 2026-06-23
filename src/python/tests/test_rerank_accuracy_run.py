"""0.8.3 Slice-20 CE-rerank ACCURACY arm runner — RED→GREEN (design §2/§5).

Backend-light: fakes only (no DB, no LLM, no fathomdb extension build). Covers the
TDD DoD:
* (b) the arm RERANKS the fused pool THEN answers — a fixture where the reranked
  top-K differs from the raw top-K ⇒ a DIFFERENT context string reaches the answerer
  (the answerer sees the reranked bodies, not the raw order).
* (c) the reused fathomdb/mem0/oracle cells are read from the checkpoint, NOT
  recomputed (no answerer call for them).
* (d) the paired bootstrap CI + MDE are deterministic (fixed seed) and reuse the
  reviewed class_delta machinery.
* the resilient run loop: checkpoint persists the ledger spend; a resume restores it
  and trips the $-cap; an incomplete/capped run is non-citable (no PASS/GO).
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Optional

import pytest

from eval.ce_rerank_probe import FathomDBRerankAdapter
from eval.gap_decomposition_run import BudgetLedger, CheckpointMissingRecords
from eval.r2_parity_eval import BaseAnswerer, GoldQuery, Hit
from eval.rerank_accuracy_run import (
    BASELINE_ARM,
    RERANK_ARM,
    accuracy_margin_summary,
    load_reused_cells,
    per_arm_accuracy,
    run_rerank_accuracy,
)


# --------------------------------------------------------------------------- #
# Fakes.
# --------------------------------------------------------------------------- #
class _SpyAnswerer(BaseAnswerer):
    """Records every context it is asked to answer; echoes context[0]."""

    model_id = "spy-answerer-v1"

    def __init__(self) -> None:
        self.contexts: list[list[str]] = []
        self.prompt_tokens = 0
        self.completion_tokens = 0

    @property
    def available(self) -> bool:
        return True

    def answer(self, question: str, context: list[str]) -> Optional[str]:
        self.contexts.append(list(context))
        return context[0] if context else None


class _FakeBase:
    """A base FathomDB adapter returning a fixed pool (with bodies) per question."""

    def __init__(self, pool: list[tuple[str, str, float]]) -> None:
        self._pool = pool  # [(doc_id, body, score)] in the BASE (pre-rerank) order

    def retrieve(self, question: str, k: int) -> list[Hit]:
        return [Hit(doc_id=d, body=b, score=s) for d, b, s in self._pool[:k]]


def _reverse_rerank(query: str, passages: list[dict], depth: int) -> list[dict]:
    """Deterministic FAKE reranker: reverse the pool order (a known reorder)."""
    rev = list(reversed(passages))
    n = len(rev)
    return [{"id": p["id"], "score": float(n - i)} for i, p in enumerate(rev)]


def _gold(qid: str, cls: str, answer: str) -> GoldQuery:
    return GoldQuery(
        query_id=qid, question=f"q-{qid}", reporting_class=cls,
        answers=(answer,), gold_doc_ids=(f"doc-{qid}",),
    )


# --------------------------------------------------------------------------- #
# (b) the arm reranks then answers — reranked top-K ≠ raw top-K reaches answerer.
# --------------------------------------------------------------------------- #
def test_answerer_sees_reranked_topk_not_raw_order(tmp_path: Path) -> None:
    # A 12-doc fused pool. The reverse reranker flips the order, so the reranked
    # top-K (bodies 11..2) is a DIFFERENT context string than the raw top-K (0..9).
    pool = [(f"d{i}", f"body {i}", 1.0 / (i + 1)) for i in range(12)]
    base = _FakeBase(pool)
    reranked = FathomDBRerankAdapter(
        base=base, rerank_fn=_reverse_rerank, pool_n=12, rerank_depth=12
    )
    spy = _SpyAnswerer()
    queries = [_gold("factoid-0", "factoid", "body 11")]
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)

    run_rerank_accuracy(
        queries=queries, reused_cells={}, reranked_adapter=reranked,
        answerer=spy, ledger=ledger, reader="gpt-5.4", output=tmp_path / "out.json",
        n_boot=50, classes=("factoid",), checkpoint_every=1,
    )

    assert len(spy.contexts) == 1
    seen = spy.contexts[0]
    raw_topk = [f"body {i}" for i in range(10)]            # base order, top-10
    reranked_topk = [f"body {i}" for i in range(11, 1, -1)]  # reversed, top-10
    assert seen == reranked_topk, "the answerer must see the RERANKED bodies"
    assert seen != raw_topk, "the answerer must NOT see the raw fused order"


# --------------------------------------------------------------------------- #
# (c) reused fathomdb/mem0/oracle cells are read from the checkpoint, not recomputed.
# --------------------------------------------------------------------------- #
def test_reused_cells_not_recomputed(tmp_path: Path) -> None:
    pool = [(f"d{i}", f"body {i}", 1.0 / (i + 1)) for i in range(6)]
    reranked = FathomDBRerankAdapter(
        base=_FakeBase(pool), rerank_fn=_reverse_rerank, pool_n=6, rerank_depth=6
    )
    spy = _SpyAnswerer()
    queries = [_gold(f"factoid-{j}", "factoid", "body 5") for j in range(3)]
    # The already-paid cells (fathomdb/mem0/oracle) come from the checkpoint map.
    reused_cells: dict[tuple[str, str], dict[str, Any]] = {}
    for q in queries:
        reused_cells[(q.query_id, "fathomdb")] = {"acc": 0.0, "answer": None}
        reused_cells[(q.query_id, "mem0_oss")] = {"acc": 1.0, "answer": "x"}
        reused_cells[(q.query_id, "oracle_raw")] = {"acc": 1.0, "answer": "y"}
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)

    out = tmp_path / "out.json"
    art = run_rerank_accuracy(
        queries=queries, reused_cells=reused_cells, reranked_adapter=reranked,
        answerer=spy, ledger=ledger, reader="gpt-5.4", output=out,
        n_boot=50, classes=("factoid",), checkpoint_every=1,
    )
    # The answerer ran EXACTLY once per question — ONLY the reranked arm. The reused
    # fathomdb/mem0/oracle cells were never re-answered.
    assert len(spy.contexts) == 3
    # Those reused acc values are present in the per-arm accuracy block (from cells).
    paa = art["accuracy_margin"]["per_arm_accuracy"]
    assert paa["fathomdb"] == pytest.approx(0.0)
    assert paa["mem0_oss"] == pytest.approx(1.0)
    assert paa["oracle_raw"] == pytest.approx(1.0)
    # The persisted checkpoint records carry only the reranked arm under `answers`.
    persisted = json.loads(out.with_suffix(".checkpoint.json").read_text(encoding="utf-8"))
    for r in persisted["records"]:
        assert set(r["answers"]) == {RERANK_ARM}
        assert set(r["acc"]) == {RERANK_ARM, "fathomdb", "mem0_oss", "oracle_raw"}


# --------------------------------------------------------------------------- #
# (d) paired bootstrap CI + MDE deterministic; margin point correct.
# --------------------------------------------------------------------------- #
def _records_for_margin() -> list[dict[str, Any]]:
    """4 factoid records: reranked beats fathomdb on 2, ties on 2 ⇒ margin point 0.5."""
    return [
        {"reporting_class": "factoid", "has_answers": True,
         "acc": {RERANK_ARM: 1.0, BASELINE_ARM: 0.0, "mem0_oss": 1.0, "oracle_raw": 1.0}},
        {"reporting_class": "factoid", "has_answers": True,
         "acc": {RERANK_ARM: 1.0, BASELINE_ARM: 0.0, "mem0_oss": 1.0, "oracle_raw": 1.0}},
        {"reporting_class": "factoid", "has_answers": True,
         "acc": {RERANK_ARM: 1.0, BASELINE_ARM: 1.0, "mem0_oss": 1.0, "oracle_raw": 1.0}},
        {"reporting_class": "factoid", "has_answers": True,
         "acc": {RERANK_ARM: 1.0, BASELINE_ARM: 1.0, "mem0_oss": 1.0, "oracle_raw": 1.0}},
    ]


def test_accuracy_margin_summary_deterministic_and_correct() -> None:
    records = _records_for_margin()
    s1 = accuracy_margin_summary(records, classes=("factoid",), n_boot=500, seed=0)
    s2 = accuracy_margin_summary(records, classes=("factoid",), n_boot=500, seed=0)
    assert s1 == s2, "same seed ⇒ identical bootstrap CI + MDE (deterministic)"
    fac = s1["per_class"]["factoid"]
    assert fac["margin"]["point"] == 0.5  # mean(1-0,1-0,1-1,1-1)
    assert fac["margin"]["n"] == 4
    assert fac["margin"]["ci_lo"] is not None and fac["margin"]["mde"] is not None
    assert fac["lever_realized"] in {"PASS", "FAIL", "INCONCLUSIVE"}
    pooled = s1["pooled"]
    assert pooled["margin"]["point"] == 0.5
    assert pooled["margin"]["n"] == 4


def test_per_arm_accuracy_excludes_missing_cells() -> None:
    records = [
        {"reporting_class": "factoid", "acc": {RERANK_ARM: 1.0}},
        {"reporting_class": "factoid", "acc": {}},  # reranked cell ABSENT — excluded
    ]
    assert per_arm_accuracy(records, arm=RERANK_ARM) == pytest.approx(1.0)
    assert per_arm_accuracy(records, arm="mem0_oss") is None


# --------------------------------------------------------------------------- #
# load_reused_cells: two checkpoints + HARD-STOP on an aggregate-only artifact.
# --------------------------------------------------------------------------- #
def test_load_reused_cells_merges_two_checkpoints(tmp_path: Path) -> None:
    d0b = tmp_path / "d0b.checkpoint.json"
    d0b.write_text(json.dumps({"records": [
        {"qid": "a", "acc": {"fathomdb": 0.0, "mem0_oss": 1.0}, "answers": {}},
    ]}), encoding="utf-8")
    gap = tmp_path / "gap.checkpoint.json"
    gap.write_text(json.dumps({"records": [
        {"qid": "a", "acc": {"oracle_raw": 1.0}, "answers": {"oracle_raw": "x"}},
    ]}), encoding="utf-8")
    cells = load_reused_cells(d0b, gap)
    assert cells[("a", "fathomdb")]["acc"] == 0.0
    assert cells[("a", "mem0_oss")]["acc"] == 1.0
    assert cells[("a", "oracle_raw")]["acc"] == 1.0


def test_load_reused_cells_hard_stops_on_aggregate(tmp_path: Path) -> None:
    agg = tmp_path / "agg.json"
    agg.write_text(json.dumps({"accuracy_deltas": {}, "n_questions": 606}), encoding="utf-8")
    with pytest.raises(CheckpointMissingRecords):
        load_reused_cells(agg)


def test_load_reused_cells_d0b_only_is_fine(tmp_path: Path) -> None:
    d0b = tmp_path / "d0b.checkpoint.json"
    d0b.write_text(json.dumps({"records": [
        {"qid": "a", "acc": {"fathomdb": 0.0, "mem0_oss": 1.0}, "answers": {}},
    ]}), encoding="utf-8")
    cells = load_reused_cells(d0b)  # no gap checkpoint → no oracle_raw cells
    assert ("a", "fathomdb") in cells
    assert ("a", "oracle_raw") not in cells


# --------------------------------------------------------------------------- #
# Resilience: checkpoint persists ledger spend; resume restores it + trips the cap.
# --------------------------------------------------------------------------- #
def _run_fixtures(qids: tuple[str, ...]) -> tuple[list[GoldQuery], FathomDBRerankAdapter, dict]:
    pool = [(f"d{i}", f"body {i}", 1.0 / (i + 1)) for i in range(4)]
    reranked = FathomDBRerankAdapter(
        base=_FakeBase(pool), rerank_fn=_reverse_rerank, pool_n=4, rerank_depth=4
    )
    queries = [_gold(qid, "factoid", "body 3") for qid in qids]
    reused: dict[tuple[str, str], dict[str, Any]] = {}
    for q in queries:
        reused[(q.query_id, "fathomdb")] = {"acc": 0.0, "answer": None}
        reused[(q.query_id, "mem0_oss")] = {"acc": 1.0, "answer": "x"}
    return queries, reranked, reused


def test_checkpoint_persists_ledger_spend(tmp_path: Path) -> None:
    queries, reranked, reused = _run_fixtures(tuple(f"factoid-{j}" for j in range(4)))
    out = tmp_path / "out.json"
    ckpt = out.with_suffix(".checkpoint.json")
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
    art = run_rerank_accuracy(
        queries=queries, reused_cells=reused, reranked_adapter=reranked,
        answerer=_SpyAnswerer(), ledger=ledger, reader="gpt-5.4", output=out,
        n_boot=50, classes=("factoid",), checkpoint_path=ckpt, checkpoint_every=1,
    )
    assert art["ledger"]["spent_usd"] > 10.7479  # reader calls booked spend
    persisted = json.loads(ckpt.read_text(encoding="utf-8"))
    assert persisted["ledger_spent_usd"] == pytest.approx(art["ledger"]["spent_usd"])


def test_resume_restores_ledger_spend_and_trips_cap(tmp_path: Path) -> None:
    queries, reranked, reused = _run_fixtures(("factoid-0", "factoid-1"))
    out = tmp_path / "out.json"
    ckpt = out.with_suffix(".checkpoint.json")
    # A prior checkpoint that already spent up to one tick under the $30 cap, with
    # factoid-0's reranked answer already paid for (resumes from the rmap).
    ckpt.write_text(json.dumps({
        "records": [{"qid": "factoid-0", "reporting_class": "factoid",
                     "answers": {RERANK_ARM: "prior"}}],
        "ledger_spent_usd": 29.999, "mode": "run", "reader": "gpt-5.4",
    }), encoding="utf-8")
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=512)
    art = run_rerank_accuracy(
        queries=queries, reused_cells=reused, reranked_adapter=reranked,
        answerer=_SpyAnswerer(), ledger=ledger, reader="gpt-5.4", output=out,
        n_boot=50, classes=("factoid",), checkpoint_path=ckpt, checkpoint_every=1,
    )
    # Restoring the prior $29.999 (NOT just the $10.7479 opening) means the NEW
    # factoid-1 reader call trips the cap and the run aborts BEFORE the call.
    assert art["ledger"]["spent_usd"] == pytest.approx(29.999, abs=1e-3)
    assert art["aborted_for_cap"] is True
    assert art["citable"] is False
    assert art["verdict"] == "ABORTED_INCOMPLETE"
    assert art["go"] is False


def test_resume_refuses_config_mismatch_and_missing_config(tmp_path: Path) -> None:
    """codex §9 P1: a config-aware resume must refuse a checkpoint written under a
    different retrieval_config — INCLUDING a pre-config-stamping checkpoint (None) —
    so old engine-blend answers cannot splice into a reblend run + publish an invalid
    paired margin. A matching config resumes normally."""
    queries, reranked, reused = _run_fixtures(("factoid-0", "factoid-1"))
    out = tmp_path / "out.json"
    ckpt = out.with_suffix(".checkpoint.json")
    cfg = {"arm_kind": "reblend", "alpha": 1.0, "pool_n": 10, "k": 10, "context_char_budget": 32000}

    def _run(checkpoint_payload: dict[str, Any], retrieval_config: dict[str, Any]) -> Any:
        ckpt.write_text(json.dumps(checkpoint_payload), encoding="utf-8")
        ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
        return run_rerank_accuracy(
            queries=queries, reused_cells=reused, reranked_adapter=reranked,
            answerer=_SpyAnswerer(), ledger=ledger, reader="gpt-5.4", output=out,
            n_boot=50, classes=("factoid",), checkpoint_path=ckpt, checkpoint_every=1,
            retrieval_config=retrieval_config,
        )

    base_records = [{"qid": "factoid-0", "reporting_class": "factoid",
                     "answers": {RERANK_ARM: "prior"}}]
    # (1) pre-config-stamping checkpoint (no retrieval_config) + a non-empty run cfg ⇒ refuse.
    with pytest.raises(ValueError, match="refusing to resume across configs"):
        _run({"records": base_records, "mode": "run", "reader": "gpt-5.4"}, cfg)
    # (2) a DIFFERENT stamped config ⇒ refuse.
    with pytest.raises(ValueError, match="refusing to resume across configs"):
        _run({"records": base_records, "retrieval_config": {**cfg, "alpha": 0.3}}, cfg)
    # (3) the MATCHING config resumes (no raise) — factoid-0 reused from the checkpoint.
    art = _run({"records": base_records, "retrieval_config": cfg}, cfg)
    assert art["verdict"] in {"PASS", "FAIL", "INCONCLUSIVE", "ABORTED_INCOMPLETE"}


def test_capped_run_is_non_citable_never_pass(tmp_path: Path) -> None:
    queries, reranked, _reused = _run_fixtures(tuple(f"factoid-{j}" for j in range(10)))
    # A reused-cell map that would make the COMPLETED prefix a clean PASS (reranked
    # 1.0 vs fathomdb 0.0) — but the cap trips mid-way so it must NOT publish PASS/GO.
    reused: dict[tuple[str, str], dict[str, Any]] = {}
    for q in queries:
        reused[(q.query_id, "fathomdb")] = {"acc": 0.0, "answer": None}
        reused[(q.query_id, "mem0_oss")] = {"acc": 1.0, "answer": "x"}

    class _CountingSpy(_SpyAnswerer):
        def answer(self, question: str, context: list[str]) -> Optional[str]:
            self.prompt_tokens += 5_000_000  # ~$6.25/call @ gpt-5.4 in-price
            return super().answer(question, context)

    out = tmp_path / "out.json"
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
    art = run_rerank_accuracy(
        queries=queries, reused_cells=reused, reranked_adapter=reranked,
        answerer=_CountingSpy(), ledger=ledger, reader="gpt-5.4", output=out,
        n_boot=50, classes=("factoid",), checkpoint_every=1,
    )
    assert art["aborted_for_cap"] is True
    assert art["citable"] is False
    assert art["run_valid"] is False
    assert art["verdict"] == "ABORTED_INCOMPLETE"
    assert art["go"] is False
    assert art["answer_completeness"] < 1.0


def test_non_citable_suppresses_nested_pooled_and_per_class_decision(tmp_path: Path) -> None:
    # codex §9 [P1]: an INCOMPLETE run whose answered prefix computes a powered nested
    # pooled PASS + go=True must NOT leave that PASS/go in accuracy_margin (pooled or
    # any per_class) — a downstream reader could publish it despite citable=False.
    qids = tuple(f"factoid-{j}" for j in range(8))
    queries, reranked, _r = _run_fixtures(qids)  # gold "body 3" → reranked acc 1.0
    reused: dict[tuple[str, str], dict[str, Any]] = {}
    for q in queries:
        reused[(q.query_id, "fathomdb")] = {"acc": 0.0, "answer": None}
        reused[(q.query_id, "mem0_oss")] = {"acc": 1.0, "answer": "x"}

    class _FailLastAnswerer(_SpyAnswerer):
        def answer(self, question: str, context: list[str]) -> Optional[str]:
            if question == f"q-{qids[-1]}":
                raise RuntimeError("retry-exhausted 5xx — cell ABSENT (failure != abstention)")
            return super().answer(question, context)

    out = tmp_path / "out.json"
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
    art = run_rerank_accuracy(
        queries=queries, reused_cells=reused, reranked_adapter=reranked,
        answerer=_FailLastAnswerer(), ledger=ledger, reader="gpt-5.4", output=out,
        n_boot=50, classes=("factoid",), checkpoint_every=1,
    )

    # The answered prefix (7/8) WOULD compute a powered nested pooled PASS + go=True.
    assert art["answer_completeness"] < 1.0
    assert art["citable"] is False
    # Top-level (already enforced on HEAD).
    assert art["verdict"] == "ABORTED_INCOMPLETE"
    assert art["go"] is False
    # NESTED must be suppressed too — pooled + every per_class entry. The raw stats
    # survive for forensics; only the DECISION fields are neutralized.
    pooled = art["accuracy_margin"]["pooled"]
    assert pooled["lever_realized"] == "ABORTED_INCOMPLETE"
    assert pooled["go"] is False
    assert pooled["reason"] == art["non_citable_reason"]
    assert pooled["margin"]["point"] is not None  # raw stats intact for forensics
    for cls, block in art["accuracy_margin"]["per_class"].items():
        assert block["lever_realized"] == "ABORTED_INCOMPLETE", cls
        assert block["go"] is False, cls
        assert block["reason"] == art["non_citable_reason"], cls


def test_citable_run_keeps_real_nested_pass_and_go(tmp_path: Path) -> None:
    # No-regression: a fully-complete citable run must surface the REAL nested PASS/GO
    # unchanged (no suppression, no injected reason).
    queries, reranked, _r = _run_fixtures(tuple(f"factoid-{j}" for j in range(6)))
    reused: dict[tuple[str, str], dict[str, Any]] = {}
    for q in queries:
        reused[(q.query_id, "fathomdb")] = {"acc": 0.0, "answer": None}
        reused[(q.query_id, "mem0_oss")] = {"acc": 1.0, "answer": "x"}
        reused[(q.query_id, "oracle_raw")] = {"acc": 1.0, "answer": "y"}
    out = tmp_path / "out.json"
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
    art = run_rerank_accuracy(
        queries=queries, reused_cells=reused, reranked_adapter=reranked,
        answerer=_SpyAnswerer(), ledger=ledger, reader="gpt-5.4", output=out,
        n_boot=200, classes=("factoid",),
    )
    assert art["citable"] is True
    pooled = art["accuracy_margin"]["pooled"]
    assert pooled["lever_realized"] == "PASS"
    assert pooled["go"] is True
    assert "reason" not in pooled  # citable runs are untouched
    assert art["verdict"] == "PASS" and art["go"] is True


def test_complete_run_is_citable_and_may_pass(tmp_path: Path) -> None:
    queries, reranked, _r = _run_fixtures(tuple(f"factoid-{j}" for j in range(6)))
    # reranked hits gold (body 3 surfaces via the reverse rerank) → reranked acc 1.0;
    # fathomdb reused acc 0.0 → a clean positive margin.
    reused: dict[tuple[str, str], dict[str, Any]] = {}
    for q in queries:
        reused[(q.query_id, "fathomdb")] = {"acc": 0.0, "answer": None}
        reused[(q.query_id, "mem0_oss")] = {"acc": 1.0, "answer": "x"}
        reused[(q.query_id, "oracle_raw")] = {"acc": 1.0, "answer": "y"}
    out = tmp_path / "out.json"
    ledger = BudgetLedger(opening_balance_usd=10.7479, hard_cap_usd=30.0, max_output_tokens=64)
    art = run_rerank_accuracy(
        queries=queries, reused_cells=reused, reranked_adapter=reranked,
        answerer=_SpyAnswerer(), ledger=ledger, reader="gpt-5.4", output=out,
        n_boot=200, classes=("factoid",),
    )
    assert art["aborted_for_cap"] is False
    assert art["citable"] is True
    assert art["run_valid"] is True
    assert art["answer_completeness"] == 1.0
    assert art["verdict"] == art["accuracy_margin"]["pooled"]["lever_realized"]
    assert art["verdict"] != "ABORTED_INCOMPLETE"
    # The arm pins its frozen parameters in the output (run used the default POOL_N).
    assert art["pool_n"] == 50 and art["rerank_depth"] == 50 and art["k"] == 10
    assert art["ce_model_repo"] == "cross-encoder/ms-marco-TinyBERT-L2-v2"
    assert art["rerank_arm"] == RERANK_ARM and art["baseline_arm"] == BASELINE_ARM
