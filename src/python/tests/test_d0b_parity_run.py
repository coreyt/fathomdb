"""Slice 10 (0.8.3 / D0b) — per-class parity runner: deltas+CIs, resilient resume,
decide_083 wiring, completeness guard. Backend-free: fake adapters + fake answerer
(no DB, no LLM, no ``mem0``, no ``fathomdb`` extension build).

Proves the resilience preconditions BEFORE any spend ([[priced-runs-need-resilience
-before-spend]]): an atomic checkpoint per ≤10 cells, an auto-resume that reuses prior
cells on a kill/restart (re-calling only ABSENT cells), failure ≠ abstention, and a
completeness validity guard that rejects an incomplete matrix.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Optional

import pytest

from eval.d0b_parity_run import (
    ALL_ARMS,
    COMPARATOR_ARMS,
    answer_completeness,
    class_delta,
    external_per_class_for_decide,
    paired_metric_deltas,
    resume_map,
    run_d0b,
)
from eval.decision_rule_083 import MEMORY_CLASSES
from eval.r2_parity_eval import BaseAnswerer, GoldQuery, Hit


# --------------------------------------------------------------------------- #
# Fakes — no backend.
# --------------------------------------------------------------------------- #
class _FakeAdapter:
    """Returns a fixed hit list per question; deterministic."""

    def __init__(self, name: str, hits_by_q: dict[str, list[Hit]]) -> None:
        self.name = name
        self._h = hits_by_q

    def retrieve(self, question: str, k: int) -> list[Hit]:
        return list(self._h.get(question, []))[:k]


class _FakeAnswerer(BaseAnswerer):
    """Deterministic answerer: returns a per-question scripted answer keyed by the
    FIRST context body (so different arms' contexts yield different answers)."""

    model_id = "fake-answerer-v1"

    def __init__(self, script: dict[str, Optional[str]]) -> None:
        self.script = script
        self.n_calls = 0

    @property
    def available(self) -> bool:
        return True

    def answer(self, question: str, context: list[str]) -> Optional[str]:
        self.n_calls += 1
        key = context[0] if context else ""
        return self.script.get(key)


class _FlakyAnswerer(_FakeAnswerer):
    """Raises on a designated set of (call-content) keys to simulate retry-exhausted
    failures (failure ≠ abstention)."""

    def __init__(self, script: dict[str, Optional[str]], fail_keys: set[str]) -> None:
        super().__init__(script)
        self.fail_keys = set(fail_keys)

    def answer(self, question: str, context: list[str]) -> Optional[str]:
        key = context[0] if context else ""
        if key in self.fail_keys:
            raise RuntimeError("simulated retry-exhausted 429")
        return super().answer(question, context)


def _gold(qid: str, cls: str, gold_ids: tuple[str, ...], answers: tuple[str, ...]) -> GoldQuery:
    return GoldQuery(query_id=qid, question=f"q-{qid}", reporting_class=cls, answers=answers, gold_doc_ids=gold_ids)


# --------------------------------------------------------------------------- #
# Pure stats helpers.
# --------------------------------------------------------------------------- #
def test_paired_metric_deltas_excludes_missing_and_off_class() -> None:
    recs = [
        {"reporting_class": "factoid", "recall": {"fathomdb": 1.0, "mem0_oss": 0.0}},
        {"reporting_class": "factoid", "recall": {"fathomdb": 1.0}},  # missing comparator -> excluded
        {"reporting_class": "temporal", "recall": {"fathomdb": 0.0, "mem0_oss": 1.0}},  # off-class
    ]
    d = paired_metric_deltas(recs, metric="recall", treatment="fathomdb", comparator="mem0_oss", cls="factoid")
    assert d == [1.0]


def test_class_delta_point_ci_mde_and_degenerate_sizes() -> None:
    empty = class_delta([])
    assert empty == {"point": None, "ci_lo": None, "ci_hi": None, "mde": None, "n": 0}
    one = class_delta([0.5])
    assert one["n"] == 1 and one["point"] == 0.5 and one["mde"] is None
    many = class_delta([1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0], n_boot=500, seed=0)
    assert many["n"] == 8
    assert many["ci_lo"] <= many["point"] <= many["ci_hi"]
    assert many["mde"] is not None and many["mde"] > 0.0


def test_class_delta_is_deterministic_given_seed() -> None:
    s = [1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0]
    assert class_delta(s, n_boot=400, seed=0) == class_delta(s, n_boot=400, seed=0)


def test_external_per_class_for_decide_raises_on_unusable_class() -> None:
    table = {"mem0_oss": {c: class_delta([0.1] * 6, n_boot=200) for c in MEMORY_CLASSES}}
    ext = external_per_class_for_decide(table, comparator="mem0_oss")
    assert set(ext) == set(MEMORY_CLASSES)
    # drop one class to n=0 -> must raise (no fabricated verdict)
    table["mem0_oss"]["temporal"] = class_delta([])
    with pytest.raises(ValueError):
        external_per_class_for_decide(table, comparator="mem0_oss")


# --------------------------------------------------------------------------- #
# Completeness validity guard.
# --------------------------------------------------------------------------- #
def test_completeness_guard_rejects_incomplete_matrix() -> None:
    recs = [{"has_answers": True} for _ in range(10)]
    arms = ["fathomdb", "mem0_oss", "naive_rag"]  # expected = 30 cells
    ok = answer_completeness(recs, arms=arms, n_errors=0)
    assert ok["run_valid"] is True and ok["completeness"] == 1.0
    bad = answer_completeness(recs, arms=arms, n_errors=3)  # 27/30 = 0.9 < 0.97
    assert bad["run_valid"] is False and bad["completeness"] == 0.9


# --------------------------------------------------------------------------- #
# resume_map membership semantics (key-present None reused; absent re-called).
# --------------------------------------------------------------------------- #
def test_resume_map_membership_semantics() -> None:
    prior = {"records": [{"qid": "a", "answers": {"fathomdb": "x", "mem0_oss": None}}]}
    m = resume_map(prior)
    assert ("a", "fathomdb") in m and m[("a", "fathomdb")] == "x"
    assert ("a", "mem0_oss") in m and m[("a", "mem0_oss")] is None  # abstention reused
    assert ("a", "naive_rag") not in m  # absent -> re-called


# --------------------------------------------------------------------------- #
# End-to-end runner: deltas + CI + decide_083 emitted across all arms.
# --------------------------------------------------------------------------- #
def _build_full_fixture() -> tuple[list[GoldQuery], dict[str, _FakeAdapter], _FakeAnswerer]:
    queries: list[GoldQuery] = []
    hits: dict[str, dict[str, list[Hit]]] = {a: {} for a in ALL_ARMS}
    script: dict[str, Optional[str]] = {}
    for cls in MEMORY_CLASSES:
        for j in range(4):
            qid = f"{cls}-{j}"
            gid = f"g-{qid}"
            q = _gold(qid, cls, (gid,), (f"ans-{qid}",))
            queries.append(q)
            # fathomdb retrieves gold (recall 1) + a body that yields the right answer
            hits["fathomdb"][q.question] = [Hit(doc_id=gid, body=f"body-fdb-{qid}", score=1.0)]
            script[f"body-fdb-{qid}"] = f"ans-{qid}"  # correct
            # mem0 retrieves a wrong doc (recall 0) + a body that yields a wrong answer
            hits["mem0_oss"][q.question] = [Hit(doc_id=f"wrong-{qid}", body=f"body-mem0-{qid}", score=0.5)]
            script[f"body-mem0-{qid}"] = "nope"
            # graphiti retrieves gold (recall 1), correct answer
            hits["graphiti_zep"][q.question] = [Hit(doc_id=gid, body=f"body-gz-{qid}", score=0.9)]
            script[f"body-gz-{qid}"] = f"ans-{qid}"
            # naive retrieves gold half the time
            nid = gid if j % 2 == 0 else f"wrong-{qid}"
            hits["naive_rag"][q.question] = [Hit(doc_id=nid, body=f"body-nr-{qid}", score=0.4)]
            script[f"body-nr-{qid}"] = f"ans-{qid}" if j % 2 == 0 else "nope"
    adapters = {a: _FakeAdapter(a, hits[a]) for a in ALL_ARMS}
    return queries, adapters, _FakeAnswerer(script)


def test_run_d0b_emits_per_class_deltas_cis_and_decide(tmp_path: Path) -> None:
    queries, adapters, answerer = _build_full_fixture()
    out = tmp_path / "d0b.json"
    art = run_d0b(
        mode="full", reader="fake", output=out, queries=queries,
        adapters=adapters, answerer=answerer, n_boot=300, checkpoint_every=10,
    )
    assert art["arms_run"] == list(ALL_ARMS)
    # recall + accuracy delta tables for every comparator x every class
    for table_key in ("recall_deltas", "accuracy_deltas"):
        tbl = art[table_key]
        for comp in COMPARATOR_ARMS:
            for cls in MEMORY_CLASSES:
                cd = tbl[comp][cls]
                assert cd["n"] == 4
                assert cd["point"] is not None and cd["ci_lo"] is not None and cd["ci_hi"] is not None
    # fathomdb beats mem0 on accuracy (1.0 - 0.0 = 1.0) for every class
    for cls in MEMORY_CLASSES:
        assert art["accuracy_deltas"]["mem0_oss"][cls]["point"] == 1.0
    # decide_083 ran and emitted a mechanical verdict + per-class facts
    dec = art["decide_083"]
    assert dec is not None and dec["verdict"] in ("REACHED", "NOT_REACHED")
    assert set(dec["per_class"]) == set(MEMORY_CLASSES)
    assert art["run_valid"] is True


# --------------------------------------------------------------------------- #
# RESILIENCE: kill + restart reuses prior cells, re-calls only absent ones.
# --------------------------------------------------------------------------- #
def test_resume_reuses_prior_cells_and_recalls_only_absent(tmp_path: Path) -> None:
    queries, adapters, _ = _build_full_fixture()
    out = tmp_path / "d0b.json"
    ckpt = out.with_suffix(".checkpoint.json")

    # Pass 1: a flaky answerer that FAILS every naive_rag cell -> those cells are
    # MISSING (absent), counted as errors, never persisted as a scored abstention.
    script = _build_full_fixture()[2].script
    fail_keys = {b for b in script if b.startswith("body-nr-")}
    flaky = _FlakyAnswerer(script, fail_keys=fail_keys)
    run_d0b(mode="full", reader="fake", output=out, queries=queries,
            adapters=adapters, answerer=flaky, n_boot=200, checkpoint_every=2)
    persisted = json.loads(ckpt.read_text())
    rmap = resume_map(persisted)
    # naive_rag cells absent (failed); fathomdb/mem0/graphiti present.
    assert not any(arm == "naive_rag" for (_q, arm) in rmap)
    assert any(arm == "fathomdb" for (_q, arm) in rmap)
    n_reused_before = len(rmap)

    # Pass 2 (restart): a healthy answerer with a call counter. Auto-resume must
    # reuse every persisted cell and re-call ONLY the absent naive_rag cells.
    healthy = _FakeAnswerer(script)
    art2 = run_d0b(mode="full", reader="fake", output=out, queries=queries,
                   adapters=adapters, answerer=healthy, n_boot=200, checkpoint_every=2)
    # exactly the 16 naive_rag cells were (re)called this pass — zero re-spend on reused.
    assert healthy.n_calls == 16
    assert n_reused_before == 48  # 16 q x 3 healthy arms
    # the run is now complete + valid; naive_rag accuracy delta now populated.
    assert art2["run_valid"] is True
    assert art2["accuracy_deltas"]["naive_rag"]["factoid"]["n"] == 4


def test_failure_is_not_a_scored_abstention(tmp_path: Path) -> None:
    queries, adapters, _ = _build_full_fixture()
    out = tmp_path / "d0b.json"
    script = _build_full_fixture()[2].script
    fail_keys = {b for b in script if b.startswith("body-mem0-")}
    flaky = _FlakyAnswerer(script, fail_keys=fail_keys)
    art = run_d0b(mode="full", reader="fake", output=out, queries=queries,
                  adapters=adapters, answerer=flaky, n_boot=200, checkpoint_every=5)
    # mem0 answerer cells all failed -> MISSING, so the accuracy delta has n=0
    # (NOT scored as 16 zeros), and completeness flags the run invalid.
    assert art["accuracy_deltas"]["mem0_oss"]["factoid"]["n"] == 0
    assert art["answer_completeness"]["run_valid"] is False
    assert art["run_valid"] is False
    # decide_083 cannot run on an unusable mem0 class -> recorded error, not a crash.
    assert art["decide_083"] is None and art["decide_083_error"]


def test_atomic_checkpoint_written_every_n(tmp_path: Path) -> None:
    queries, adapters, answerer = _build_full_fixture()
    out = tmp_path / "d0b.json"
    ckpt = out.with_suffix(".checkpoint.json")
    run_d0b(mode="full", reader="fake", output=out, queries=queries,
            adapters=adapters, answerer=answerer, n_boot=100, checkpoint_every=4)
    assert ckpt.exists()
    data = json.loads(ckpt.read_text())
    assert len(data["records"]) == 16  # final flush persists all
