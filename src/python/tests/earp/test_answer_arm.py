"""S9 answer-arm tests — written RED, before `eval.earp.answer_arm` exists.

Gate ordering (declared → opted-in → budget → cheap-validate) with unreached-
stage instrumentation, the resolver couplings, `answer_accuracy` eligibility
both directions, and end-to-end stub runs covering all three dispositions
(skip / blocked / complete-with-fake-cost) plus the self-feeding ledger.

NO test here makes (or may make) a network call: every test runs under BOTH an
instrumented stub that records (or forbids) invocation AND a monkeypatched
`urllib.request.urlopen` that raises (AC-2's defense in depth). `R2_RUN=1` is
NEVER set.
"""

from __future__ import annotations

import hashlib
import json
import urllib.request
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

import pytest

from eval.earp.answer_arm import (
    FDB_EARP_PRICED_ENV,
    AnswerTask,
    R2IdenticalAnswerer,
    StubAnswerAdapter,
    run_answer_arm,
    run_answer_campaign,
)
from eval.earp.config import CONSUMER_REGISTRY, emits, resolve_config
from eval.earp.knobs import CATALOG_BY_NAME
from eval.earp.pricing import D3_AUTHORIZED_USD, LEDGER_ROOT_ENV
from eval.earp.schema import PER_QUERY_SCHEMA_PATH, RESULT_SCHEMA_PATH
from eval.earp.schema.models import (
    Blocker,
    BlockerCode,
    KnobClass,
    RunVerdict,
    WitnessSource,
)
from eval.earp.schema.validate import validate

try:
    import fathomdb._fathomdb  # noqa: F401

    _HAS_BINDING = True
except ImportError:
    _HAS_BINDING = False

requires_binding = pytest.mark.skipif(not _HAS_BINDING, reason="native binding not built")

SHA = "a" * 64
TS = datetime(2026, 8, 7, 12, 0, tzinfo=timezone.utc)

_R2_ENV = ("R2_RUN", "R2_ANSWERER_BASE_URL", "R2_ANSWERER_API_KEY", "R2_ANSWERER_MODEL")


@pytest.fixture(autouse=True)
def _sealed_environment(monkeypatch: pytest.MonkeyPatch) -> None:
    """Every test starts opted OUT with no credentials — and any network
    attempt is an assertion failure, not a hang (AC-2, finding 10)."""
    for name in (FDB_EARP_PRICED_ENV, LEDGER_ROOT_ENV, *_R2_ENV):
        monkeypatch.delenv(name, raising=False)

    def _refuse(*args: Any, **kwargs: Any) -> Any:
        raise AssertionError("a test attempted a network call via urllib.request.urlopen")

    monkeypatch.setattr(urllib.request, "urlopen", _refuse)


def _codes(result: Any) -> set[BlockerCode]:
    return {b.code for b in result.blockers}


def _messages(result: Any) -> str:
    return "\n".join(b.message for b in result.blockers)


# --- resolver couplings ---------------------------------------------------------


def _doc(**over: Any) -> dict[str, Any]:
    doc: dict[str, Any] = {
        "schema_version": "earp.v1",
        "campaign": "characterization",
        "corpus": {"snapshot": "tests/corpus/snapshot.json", "data_root": "data/corpus-data"},
        "gold": {
            "path": "d/all.gold.json",
            "sha256": SHA,
            "corpus_hash": SHA,
            "qrels_version": "ir-c-reused-v2",
        },
        "scenario": {
            "engine": {"use_default_embedder": False},
            "query": {"call": "Engine.search_text_only"},
            "answer_arm": {
                "kind": "r2_identical_answerer",
                "answerer_model": "gemini-2.5-flash-lite",
                "max_queries": 4,
            },
        },
        "metrics": {"evidence_recall_k": [5, 10]},
        "budget": {"estimated_usd": 0.5},
    }
    for key, value in over.items():
        if value is None:
            doc.pop(key, None)
        else:
            doc[key] = value
    return doc


def test_an_answer_arm_config_resolves_and_carries_the_arm() -> None:
    result = resolve_config(_doc())
    assert result.blockers == ()
    assert result.scenario is not None
    arm = result.scenario.answer_arm
    assert arm is not None
    assert arm.kind == "r2_identical_answerer"
    assert arm.max_queries == 4
    assert arm.answerer_model == "gemini-2.5-flash-lite"


def test_max_queries_is_required() -> None:
    """Gate 1 has no meaning without the declared call bound."""
    doc = _doc()
    del doc["scenario"]["answer_arm"]["max_queries"]
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_MISSING_KEY in _codes(result)
    assert any("max_queries" in str(b.detail.get("path")) for b in result.blockers)


def test_an_answer_arm_without_budget_is_a_collected_config_error() -> None:
    result = resolve_config(_doc(budget=None))
    assert BlockerCode.CONFIG_MISSING_KEY in _codes(result)
    assert any(b.detail.get("path") == "budget" for b in result.blockers)


def test_an_unknown_arm_kind_is_refused() -> None:
    doc = _doc()
    doc["scenario"]["answer_arm"]["kind"] = "mem0_oss"
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)


def test_answer_arm_paths_are_registered_to_s9() -> None:
    for path in (
        "scenario.answer_arm",
        "scenario.answer_arm.kind",
        "scenario.answer_arm.answerer_model",
        "scenario.answer_arm.max_queries",
    ):
        assert CONSUMER_REGISTRY[path].slice_id == "S9", path


def test_answer_accuracy_decision_rule_without_the_arm_stays_refused() -> None:
    doc = _doc()
    del doc["scenario"]["answer_arm"]
    doc["decision_rule"] = {"metric": "answer_accuracy", "direction": "greater", "threshold": 0.5}
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_INVALID_VALUE in _codes(result)
    assert any(b.detail.get("path") == "decision_rule.metric" for b in result.blockers)


def test_answer_accuracy_decision_rule_with_the_arm_and_explicit_model_resolves() -> None:
    doc = _doc()
    doc["decision_rule"] = {"metric": "answer_accuracy", "direction": "greater", "threshold": 0.5}
    result = resolve_config(doc)
    assert result.blockers == ()
    assert result.scenario is not None
    assert result.scenario.decision_rule is not None
    assert result.scenario.decision_rule.metric == "answer_accuracy"


def test_a_claim_on_answer_accuracy_requires_the_model_in_the_config() -> None:
    """Config identity: an env-defaulted model would let two different-model
    runs share a config_sha256 and poison S8 pairing/replay."""
    doc = _doc()
    del doc["scenario"]["answer_arm"]["answerer_model"]
    doc["decision_rule"] = {"metric": "answer_accuracy", "direction": "greater", "threshold": 0.5}
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_MISSING_KEY in _codes(result)
    assert any(
        b.detail.get("path") == "scenario.answer_arm.answerer_model" for b in result.blockers
    )


def test_the_env_default_model_is_legal_for_claim_free_runs() -> None:
    doc = _doc()
    del doc["scenario"]["answer_arm"]["answerer_model"]
    result = resolve_config(doc)
    assert result.blockers == ()
    assert result.scenario is not None
    assert result.scenario.answer_arm is not None
    assert result.scenario.answer_arm.answerer_model is None


def test_emits_answer_accuracy_keys_on_the_arm_not_on_negatives() -> None:
    """The k_free coupling to has_negatives belongs to abstention_rate ALONE."""
    ladder = (5, 10)
    assert emits("answer_accuracy", evidence_recall_k=ladder, has_negatives=False,
                 has_answer_arm=True) is True
    assert emits("answer_accuracy", evidence_recall_k=ladder, has_negatives=True,
                 has_answer_arm=False) is False
    assert emits("answer_accuracy@10", evidence_recall_k=ladder, has_negatives=True,
                 has_answer_arm=True) is False
    assert emits("abstention_rate", evidence_recall_k=ladder, has_negatives=False,
                 has_answer_arm=True) is False
    assert emits("abstention_rate", evidence_recall_k=ladder, has_negatives=True,
                 has_answer_arm=False) is True


def test_an_answer_arm_inside_an_arms_campaign_is_a_typed_refusal() -> None:
    """Owned scope line: S9's money gate is single-scenario; multi-arm priced
    execution (2x the spend per run) stays an HITL scope decision, refused
    typed rather than silently ignored."""
    doc = {
        "schema_version": "earp.v1",
        "campaign": "comparison",
        "corpus": {"snapshot": "s.json", "data_root": "d"},
        "gold": {"path": "g.json", "sha256": SHA, "corpus_hash": SHA,
                 "qrels_version": "ir-c-reused-v2"},
        "metrics": {"evidence_recall_k": [10]},
        "budget": {"estimated_usd": 0.5},
        "arms": [
            {
                "name": "control",
                "scenario": {
                    "query": {"call": "Engine.search_text_only"},
                    "answer_arm": {"kind": "r2_identical_answerer", "max_queries": 4},
                },
            },
            {"name": "treatment", "scenario": {"query": {"call": "Engine.search_text_only",
                                                         "limit": 20}}},
        ],
        "comparison": {
            "changed_knobs": ["scenario.query.limit"],
            "metric": "strict_evidence_recall@10",
            "ci_method": "paired_bootstrap",
            "seed": 42,
            "resamples": 100,
            "min_n": 1,
        },
    }
    result = resolve_config(doc)
    assert result.blockers != ()
    assert any(
        b.detail.get("path") == "arms[0].scenario.answer_arm" for b in result.blockers
    )


def test_an_answer_arm_on_a_diagnostic_is_refused() -> None:
    doc = {
        "schema_version": "earp.v1",
        "campaign": "diagnostic",
        "scenario": {
            "fixture": "tests/earp/fixtures/f.jsonl",
            "query": {"call": "Engine.search_text_only", "text": "x"},
            "answer_arm": {"kind": "r2_identical_answerer", "max_queries": 1},
        },
        "budget": {"estimated_usd": 0.1},
    }
    result = resolve_config(doc)
    assert BlockerCode.CONFIG_UNUSED_KEY in _codes(result)


# --- schema shapes ----------------------------------------------------------------

_RESULT_SCHEMA = json.loads(RESULT_SCHEMA_PATH.read_text(encoding="utf-8"))
_PER_QUERY_SCHEMA = json.loads(PER_QUERY_SCHEMA_PATH.read_text(encoding="utf-8"))


def test_witness_source_gains_answer_arm() -> None:
    assert WitnessSource.ANSWER_ARM.value == "answer_arm"
    enum = _RESULT_SCHEMA["$defs"]["witness"]["properties"]["source"]["enum"]
    assert "answer_arm" in enum


def test_per_query_schema_admits_answer_rows_with_null_k() -> None:
    row = {
        "schema_version": "earp.per-query.v1",
        "query_id": "q1",
        "query_class": "exact_fact",
        "k": None,
        "outcome": "scored",
        "strict": None,
        "graded": None,
        "required_n": None,
        "required_hits": None,
        "answer_outcome": "correct",
        "answer_text_sha": hashlib.sha256(b"x").hexdigest(),
        "answer_reason": None,
    }
    assert validate(row, _PER_QUERY_SCHEMA) == []
    # Retrieval rows keep their integer-K contract.
    row_int = dict(row)
    row_int.update({"k": 10, "strict": 1.0, "graded": 1.0, "required_n": 1, "required_hits": 1})
    assert validate(row_int, _PER_QUERY_SCHEMA) == []
    # The vocabulary is closed.
    bad = dict(row)
    bad["answer_outcome"] = "sort_of"
    assert validate(bad, _PER_QUERY_SCHEMA) != []


# --- gate ordering (pure; instrumented stub proves unreached stages) -----------------


def _arm(max_queries: int = 8, model: str | None = "stub-deterministic-v1") -> Any:
    from eval.earp.config import ResolvedAnswerArm  # noqa: PLC0415

    return ResolvedAnswerArm(
        kind="r2_identical_answerer", max_queries=max_queries, answerer_model=model
    )


def _tasks(n: int = 3) -> list[AnswerTask]:
    return [
        AnswerTask(
            query_id=f"q{i}",
            query_class="exact_fact",
            question=f"question {i}",
            ground_truth=("alpha",),
            context=(f"alpha body {i}",),
        )
        for i in range(n)
    ]


def _seed_index(root: Path, cost_usd: float) -> None:
    root.mkdir(parents=True, exist_ok=True)
    (root / "index.jsonl").write_text(
        json.dumps({"run_id": "seed", "cost_usd": cost_usd}) + "\n", encoding="utf-8"
    )


def _opt_in(monkeypatch: pytest.MonkeyPatch, root: Path) -> None:
    monkeypatch.setenv(FDB_EARP_PRICED_ENV, "1")
    monkeypatch.setenv(LEDGER_ROOT_ENV, str(root))


def test_an_unopted_run_skips_visibly_and_reaches_nothing(tmp_path: Path) -> None:
    stub = StubAnswerAdapter(forbid=frozenset({"estimate", "cheap_validate", "run"}))
    outcome = run_answer_arm(
        arm=_arm(), budget_estimated_usd=0.5, tasks=_tasks(),
        experiments_root=tmp_path, adapter=stub,
    )
    assert outcome.skipped is True
    assert outcome.blockers == ()
    assert outcome.actual_usd is None
    assert stub.invocations == []
    skip = next(w for w in outcome.witnesses if w.name == "answer_arm_skipped")
    assert skip.source is WitnessSource.ANSWER_ARM
    assert FDB_EARP_PRICED_ENV in str(skip.value)


def test_opted_in_without_credentials_skips_naming_the_adapter_gate(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv(FDB_EARP_PRICED_ENV, "1")
    stub = StubAnswerAdapter(available_override=False,
                             forbid=frozenset({"estimate", "cheap_validate", "run"}))
    outcome = run_answer_arm(
        arm=_arm(), budget_estimated_usd=0.5, tasks=_tasks(),
        experiments_root=tmp_path, adapter=stub,
    )
    assert outcome.skipped is True
    assert stub.invocations == []
    skip = next(w for w in outcome.witnesses if w.name == "answer_arm_skipped")
    assert "R2_RUN" in str(skip.value)


def test_a_missing_ledger_root_refuses_before_any_estimate(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv(FDB_EARP_PRICED_ENV, "1")  # opted in, but no ledger root
    stub = StubAnswerAdapter(forbid=frozenset({"estimate", "cheap_validate", "run"}))
    outcome = run_answer_arm(
        arm=_arm(), budget_estimated_usd=0.5, tasks=_tasks(),
        experiments_root=tmp_path, adapter=stub,
    )
    assert outcome.skipped is False
    assert stub.invocations == []
    assert [b.stage for b in outcome.blockers] == ["priced.ledger"]


def test_a_mismatched_ledger_root_is_refused(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv(FDB_EARP_PRICED_ENV, "1")
    monkeypatch.setenv(LEDGER_ROOT_ENV, str(tmp_path / "authoritative"))
    stub = StubAnswerAdapter(forbid=frozenset({"estimate", "cheap_validate", "run"}))
    outcome = run_answer_arm(
        arm=_arm(), budget_estimated_usd=0.5, tasks=_tasks(),
        experiments_root=tmp_path / "elsewhere", adapter=stub,
    )
    assert [b.stage for b in outcome.blockers] == ["priced.ledger"]
    assert stub.invocations == []


def test_an_over_budget_preflight_blocks_and_cheap_validate_is_unreached(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    _opt_in(monkeypatch, tmp_path)
    _seed_index(tmp_path, 4.8)
    stub = StubAnswerAdapter(cost_per_call_usd=0.01,
                             forbid=frozenset({"cheap_validate", "run"}))
    outcome = run_answer_arm(
        arm=_arm(), budget_estimated_usd=0.5, tasks=_tasks(),
        experiments_root=tmp_path, adapter=stub,
    )
    assert stub.invocations == ["estimate"]
    blocker = outcome.blockers[0]
    assert blocker.code is BlockerCode.BUDGET_EXCEEDED
    assert blocker.stage == "priced.preflight"
    assert blocker.detail["cumulative_spent_usd"] == pytest.approx(4.8)
    assert blocker.detail["ledger_path"] == str(tmp_path / "index.jsonl")
    assert outcome.cumulative_spent_usd == pytest.approx(4.8)


def test_an_under_declared_estimate_is_refused_before_any_priced_call(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    _opt_in(monkeypatch, tmp_path)
    stub = StubAnswerAdapter(estimate_override=1.0, forbid=frozenset({"cheap_validate", "run"}))
    outcome = run_answer_arm(
        arm=_arm(), budget_estimated_usd=0.5, tasks=_tasks(),
        experiments_root=tmp_path, adapter=stub,
    )
    blocker = outcome.blockers[0]
    assert blocker.code is BlockerCode.CONFIG_INVALID_VALUE
    assert blocker.stage == "priced.preflight"
    assert stub.invocations == ["estimate"]


def test_a_failing_cheap_validation_blocks_before_the_first_priced_call(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    _opt_in(monkeypatch, tmp_path)
    refusal = Blocker(
        code=BlockerCode.CONFIG_INVALID_VALUE,
        message="injected: the $0 validation pass failed",
        stage="priced.preflight",
    )
    stub = StubAnswerAdapter(cost_per_call_usd=0.01, cheap_validate_blocker=refusal,
                             forbid=frozenset({"run"}))
    outcome = run_answer_arm(
        arm=_arm(), budget_estimated_usd=0.5, tasks=_tasks(),
        experiments_root=tmp_path, adapter=stub,
    )
    assert stub.invocations == ["estimate", "cheap_validate"]
    assert [b.stage for b in outcome.blockers] == ["priced.preflight"]


def test_a_fully_gated_stub_run_records_calls_cost_and_witness_order(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    _opt_in(monkeypatch, tmp_path)
    stub = StubAnswerAdapter(cost_per_call_usd=0.01)
    outcome = run_answer_arm(
        arm=_arm(), budget_estimated_usd=0.5, tasks=_tasks(3),
        experiments_root=tmp_path, adapter=stub,
    )
    assert outcome.skipped is False
    assert outcome.blockers == ()
    assert stub.invocations == ["estimate", "cheap_validate", "run"]
    assert len(outcome.calls) == 3
    assert outcome.actual_usd == pytest.approx(0.03)
    assert outcome.accuracy == pytest.approx(1.0)  # the stub echoes the gold-bearing body
    names = [w.name for w in outcome.witnesses]
    # Cheap-validation is witnessed in the SAME run, BEFORE the priced outcome.
    assert names.index("answer_arm_cheap_validate") < names.index("answer_arm_outcome")
    assert "answer_arm_ledger_preflight" in names
    rows = outcome.rows
    assert len(rows) == 3
    for row in rows:
        assert row["k"] is None
        assert row["outcome"] == "scored"
        assert row["answer_outcome"] == "correct"
        assert validate(row, _PER_QUERY_SCHEMA) == []


def test_max_queries_bounds_the_priced_call_count(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    _opt_in(monkeypatch, tmp_path)
    stub = StubAnswerAdapter(cost_per_call_usd=0.01)
    outcome = run_answer_arm(
        arm=_arm(max_queries=2), budget_estimated_usd=0.5, tasks=_tasks(5),
        experiments_root=tmp_path, adapter=stub,
    )
    assert len(outcome.calls) == 2
    assert outcome.actual_usd == pytest.approx(0.02)


def test_the_call_guard_halts_mid_run_with_partials_recorded(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Actual metered spend can overrun the declared per-call worst case; the
    guard halts against REMAINING authorization and the partials are kept."""
    _opt_in(monkeypatch, tmp_path)
    _seed_index(tmp_path, 4.9)  # remaining authorization: 0.10
    stub = StubAnswerAdapter(cost_per_call_usd=0.03, estimate_override=0.09)
    outcome = run_answer_arm(
        arm=_arm(), budget_estimated_usd=0.1, tasks=_tasks(5),
        experiments_root=tmp_path, adapter=stub,
    )
    assert len(outcome.calls) == 3  # halted before the 4th (0.09 + 0.03 > 0.10)
    assert outcome.actual_usd == pytest.approx(0.09)
    halt = outcome.blockers[0]
    assert halt.code is BlockerCode.BUDGET_EXCEEDED
    assert halt.stage == "priced.call_guard"
    assert len(outcome.rows) == 3


# --- the R2 adapter (never the network) ----------------------------------------------


def test_r2_availability_is_the_wrapped_protocol_property() -> None:
    assert R2IdenticalAnswerer().available is False  # no R2_RUN, no base URL, no model


def test_r2_estimate_fails_closed_on_an_unpinned_model() -> None:
    result = R2IdenticalAnswerer(answerer_model="totally-unpinned").estimate(3)
    assert isinstance(result, Blocker)


def test_r2_estimate_prices_a_pinned_model() -> None:
    estimate = R2IdenticalAnswerer(answerer_model="gemini-2.5-flash-lite").estimate(3)
    assert isinstance(estimate, float)
    assert estimate > 0


def test_r2_marks_env_resolution_of_the_model(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("R2_ANSWERER_MODEL", "gemini-2.5-flash-lite")
    env_resolved = R2IdenticalAnswerer()
    assert env_resolved.model_source == "env-resolved"
    assert env_resolved.model_id == "gemini-2.5-flash-lite"
    explicit = R2IdenticalAnswerer(answerer_model="gemini-2.5-flash-lite")
    assert explicit.model_source == "config"


# --- deferred adapters are catalog-refused -------------------------------------------


@pytest.mark.parametrize("name", ["priced_arm.mem0", "priced_arm.extractor", "priced_arm.gpu"])
def test_deferred_priced_arms_are_catalog_refused_with_reasons(name: str) -> None:
    entry = CATALOG_BY_NAME[name]
    assert entry.classification is KnobClass.UNSUPPORTED
    assert entry.call_path is None
    assert "HITL" in entry.reason


# --- end to end (stub-priced; requires the binding for real ingest) ------------------

DOCS = [
    {"doc_id": "d1", "body": "the deal sheet is missing for March", "source_type": "email"},
    {"doc_id": "d2", "body": "parking arrangements for the annual meeting", "source_type": "note"},
    {"doc_id": "d3", "body": "quarterly revenue rose after the deal closed",
     "source_type": "article"},
]

Q1, Q2, Q3, QN = "deal sheet", "parking annual meeting", "quarterly revenue", "zzznothingzzz"

HITS = {Q1: ["d1"], Q2: ["d2"], Q3: ["d3"]}


def _bed(tmp_path: Path) -> dict[str, Any]:
    raw = tmp_path / "raw"
    raw.mkdir(exist_ok=True)
    shard = raw / "synthetic_notes.jsonl"
    shard.write_text("".join(json.dumps(d) + "\n" for d in DOCS), encoding="utf-8")
    digest = hashlib.sha256(shard.read_bytes()).hexdigest()
    snapshot = tmp_path / "snapshot.json"
    snapshot.write_text(
        json.dumps(
            {
                "corpus_hash": "c" * 64,
                "total_docs": len(DOCS),
                "per_source_sha256": [
                    {"source": "synthetic_notes", "sha256": digest, "doc_count": len(DOCS)}
                ],
            }
        ),
        encoding="utf-8",
    )
    gold = tmp_path / "gold.json"
    queries = [
        {
            "query_id": "q1",
            "query": Q1,
            "query_class": "exact_fact",
            "required_evidence": [
                {"evidence_id": "q1#e0", "doc_id": "d1", "necessity": "required"}
            ],
            "expected_top_k_doc_ids": ["d1"],
            "answers": ["deal sheet is missing"],
        },
        {
            "query_id": "q2",
            "query": Q2,
            "query_class": "action",
            "required_evidence": [
                {"evidence_id": "q2#e0", "doc_id": "d2", "necessity": "required"}
            ],
            "expected_top_k_doc_ids": ["d2"],
            "answers": ["parking arrangements"],
        },
        {
            "query_id": "q3",
            "query": Q3,
            "query_class": "exact_fact",
            "required_evidence": [
                {"evidence_id": "q3#e0", "doc_id": "d3", "necessity": "required"}
            ],
            "expected_top_k_doc_ids": ["d3"],
            "answers": ["revenue rose"],
        },
        {
            "query_id": "qn",
            "query": QN,
            "query_class": "negative",
            "required_evidence": [],
            "expected_top_k_doc_ids": [],
        },
    ]
    gold.write_text(
        json.dumps(
            {"corpus_hash": "c" * 64, "qrels_version": "ir-c-reused-v2", "queries": queries}
        ),
        encoding="utf-8",
    )
    return {
        "snapshot": snapshot,
        "gold": gold,
        "gold_sha256": hashlib.sha256(gold.read_bytes()).hexdigest(),
    }


def _priced_doc(tmp_path: Path, bed: dict[str, Any], **over: Any) -> dict[str, Any]:
    return _doc(
        corpus={"snapshot": str(bed["snapshot"]), "data_root": str(tmp_path)},
        gold={
            "path": str(bed["gold"]),
            "sha256": bed["gold_sha256"],
            "corpus_hash": "c" * 64,
            "qrels_version": "ir-c-reused-v2",
        },
        **over,
    )


class _Id:
    def __init__(self, value: str) -> None:
        self.space = "logical"
        self.value = value


class _Hit:
    def __init__(self, value: str) -> None:
        self.id = _Id(value)


class _SearchResult:
    def __init__(self, ids: list[str]) -> None:
        self.results = [_Hit(i) for i in ids]


def _override(query_text: str) -> _SearchResult:
    return _SearchResult(HITS.get(query_text, []))


def _sidecar(run_dir: Path) -> dict[str, Any]:
    return json.loads((run_dir / "earp.result.v1.json").read_text(encoding="utf-8"))


def _per_query(run_dir: Path) -> list[dict[str, Any]]:
    text = (run_dir / "earp.per-query.v1.jsonl").read_text(encoding="utf-8")
    return [json.loads(line) for line in text.strip().splitlines()]


def _index_rows(experiments_root: Path) -> list[dict[str, Any]]:
    text = (experiments_root / "index.jsonl").read_text(encoding="utf-8")
    return [json.loads(line) for line in text.strip().splitlines()]


@requires_binding
def test_the_default_run_completes_with_a_visible_skip_never_a_zero(tmp_path: Path) -> None:
    """AC-2: opted out -> the arm is SKIPPED (witness naming the missing gate),
    the run is COMPLETE, answer_accuracy is not_applicable — and the stub's
    forbid set plus the urlopen trap prove no call of any kind was attempted."""
    bed = _bed(tmp_path)
    result = run_answer_campaign(
        config_doc=_priced_doc(tmp_path, bed),
        experiments_root=tmp_path / "experiments",
        experiment="earp-answer",
        ts=TS,
        adapter=StubAnswerAdapter(forbid=frozenset({"estimate", "cheap_validate", "run"})),
        retrieve_override=_override,
    )
    assert result.verdict is RunVerdict.COMPLETE
    assert result.skipped is True
    assert result.run_dir is not None
    sidecar = _sidecar(result.run_dir)
    witnesses = {w["name"]: w for w in sidecar["witnesses"]}
    skip = witnesses["answer_arm_skipped"]
    assert skip["source"] == "answer_arm"
    assert FDB_EARP_PRICED_ENV in json.dumps(skip["value"])
    accuracy = sidecar["metrics"]["document_metrics"]["answer_accuracy"]
    assert accuracy["status"] == "not_applicable"
    assert accuracy["value"] is None
    cost = sidecar["cost"]
    assert cost["authorized_usd"] == D3_AUTHORIZED_USD
    assert cost["estimated_usd"] == 0.5
    assert cost["actual_usd"] is None
    assert not any(row.get("answer_outcome") for row in _per_query(result.run_dir))
    assert _index_rows(tmp_path / "experiments")[-1]["cost_usd"] == 0.0


@requires_binding
def test_a_blocked_preflight_is_durably_indexed_with_the_arithmetic(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    bed = _bed(tmp_path)
    experiments_root = tmp_path / "experiments"
    _opt_in(monkeypatch, experiments_root)
    _seed_index(experiments_root, 4.9)
    result = run_answer_campaign(
        config_doc=_priced_doc(tmp_path, bed),
        experiments_root=experiments_root,
        experiment="earp-answer",
        ts=TS,
        adapter=StubAnswerAdapter(cost_per_call_usd=0.01, forbid=frozenset({"run"})),
        retrieve_override=_override,
    )
    assert result.verdict is RunVerdict.BLOCKED
    assert result.run_dir is not None
    sidecar = _sidecar(result.run_dir)
    assert sidecar["verdict"] == "blocked"
    blocker = next(b for b in sidecar["blockers"] if b["code"] == "budget_exceeded")
    assert blocker["detail"]["projected_usd"] == pytest.approx(5.4)
    assert blocker["detail"]["ledger_path"] == str(experiments_root / "index.jsonl")
    rows = _index_rows(experiments_root)
    assert rows[-1]["verdict"] == "blocked"
    assert rows[-1]["cost_usd"] == 0.0


@requires_binding
def test_a_complete_stub_run_lands_cost_in_all_three_places(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    bed = _bed(tmp_path)
    experiments_root = tmp_path / "experiments"
    _opt_in(monkeypatch, experiments_root)
    result = run_answer_campaign(
        config_doc=_priced_doc(tmp_path, bed),
        experiments_root=experiments_root,
        experiment="earp-answer",
        ts=TS,
        adapter=StubAnswerAdapter(cost_per_call_usd=0.01),
        retrieve_override=_override,
    )
    assert result.verdict is RunVerdict.COMPLETE
    assert result.skipped is False
    assert result.accuracy == pytest.approx(1.0)
    assert result.run_dir is not None

    sidecar = _sidecar(result.run_dir)
    cost = sidecar["cost"]
    assert cost["actual_usd"] == pytest.approx(0.04)  # 4 queries x $0.01 fake
    assert cost["cumulative_spent_usd"] == 0.0
    assert cost["authorized_usd"] == D3_AUTHORIZED_USD
    accuracy = sidecar["metrics"]["document_metrics"]["answer_accuracy"]
    assert accuracy == {"status": "emitted", "value": 1.0}

    record = json.loads((result.run_dir / "record.json").read_text(encoding="utf-8"))
    assert record["cost_usd"] == pytest.approx(0.04)
    assert _index_rows(experiments_root)[-1]["cost_usd"] == pytest.approx(0.04)

    answer_rows = [row for row in _per_query(result.run_dir) if "answer_outcome" in row]
    assert len(answer_rows) == 4
    by_id = {row["query_id"]: row for row in answer_rows}
    assert by_id["q1"]["answer_outcome"] == "correct"
    assert by_id["q1"]["k"] is None
    assert by_id["q1"]["answer_text_sha"]
    assert by_id["qn"]["answer_outcome"] == "correct_abstention"
    assert by_id["qn"]["answer_text_sha"] is None


@requires_binding
def test_the_second_runs_preflight_sees_the_first_runs_actual(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """D-3 is cumulative because the ledger self-feeds: run 2's preflight reads
    run 1's ACTUAL from the same authoritative index."""
    bed = _bed(tmp_path)
    experiments_root = tmp_path / "experiments"
    _opt_in(monkeypatch, experiments_root)
    first = run_answer_campaign(
        config_doc=_priced_doc(tmp_path, bed),
        experiments_root=experiments_root,
        experiment="earp-answer",
        ts=TS,
        adapter=StubAnswerAdapter(cost_per_call_usd=0.01),
        retrieve_override=_override,
    )
    assert first.verdict is RunVerdict.COMPLETE
    second = run_answer_campaign(
        config_doc=_priced_doc(tmp_path, bed),
        experiments_root=experiments_root,
        experiment="earp-answer",
        ts=TS + timedelta(minutes=1),
        adapter=StubAnswerAdapter(cost_per_call_usd=0.01),
        retrieve_override=_override,
    )
    assert second.verdict is RunVerdict.COMPLETE
    assert second.run_dir is not None
    sidecar = _sidecar(second.run_dir)
    assert sidecar["cost"]["cumulative_spent_usd"] == pytest.approx(0.04)
    preflight = next(
        w for w in sidecar["witnesses"] if w["name"] == "answer_arm_ledger_preflight"
    )
    assert preflight["value"]["cumulative_spent_usd"] == pytest.approx(0.04)


@requires_binding
def test_max_queries_bounds_the_campaign_call_count(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    bed = _bed(tmp_path)
    experiments_root = tmp_path / "experiments"
    _opt_in(monkeypatch, experiments_root)
    doc = _priced_doc(tmp_path, bed)
    doc["scenario"]["answer_arm"]["max_queries"] = 2
    result = run_answer_campaign(
        config_doc=doc,
        experiments_root=experiments_root,
        experiment="earp-answer",
        ts=TS,
        adapter=StubAnswerAdapter(cost_per_call_usd=0.01),
        retrieve_override=_override,
    )
    assert result.verdict is RunVerdict.COMPLETE
    assert result.run_dir is not None
    answer_rows = [row for row in _per_query(result.run_dir) if "answer_outcome" in row]
    assert len(answer_rows) == 2
    assert _sidecar(result.run_dir)["cost"]["actual_usd"] == pytest.approx(0.02)


# --- the CLI ---------------------------------------------------------------------


def test_cli_validate_prints_estimate_cumulative_and_projection(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    from eval.earp.cli import main  # noqa: PLC0415

    ledger_root = tmp_path / "experiments"
    _seed_index(ledger_root, 1.25)
    monkeypatch.setenv(LEDGER_ROOT_ENV, str(ledger_root))
    path = tmp_path / "c.json"
    path.write_text(json.dumps(_doc()), encoding="utf-8")
    assert main(["validate", str(path)]) == 0
    out = capsys.readouterr().out
    for token in ("$0.50", "$1.25", "$1.75", "$5.00"):
        assert token in out
