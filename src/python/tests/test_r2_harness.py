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
    Mem0OSSAdapter,
    PerClassScorer,
    R2Harness,
    RecordingAnswerer,
    StubAdapter,
    StubAnswerer,
    _make_doc_id_of,
    session_id_of,
)

_REPO_ROOT = Path(__file__).resolve().parents[3]
_D0A_MANIFEST = _REPO_ROOT / "dev/plans/runs/0.8.3-d0a-corpus-manifest.json"
_D0A_GOLD = _REPO_ROOT / "dev/plans/runs/0.8.3-d0a-memory-gold.json"


class _StubHit:
    """Minimal SearchHit stand-in for doc_id_of resolution tests."""

    def __init__(self, id: int, source_id=None) -> None:
        self.id = id
        self.source_id = source_id
        self.body = ""
        self.score = 0.0


def test_chunked_session_source_resolves_to_bare_session_id() -> None:
    """G0 Phase-2 §E — a graph-arm hit's chunked ``source_id`` (``sess#c2``)
    resolves to the bare gold session id, and ``doc_id_of`` prefers ``source_id``
    over the cursor map; a two-arm hit (source_id=None) falls back to the map."""
    assert session_id_of("sess_42#c0") == "sess_42"
    assert session_id_of("sess_42#c17") == "sess_42"
    assert session_id_of("sess_42") == "sess_42"  # idempotent / no suffix

    cursor_to_doc = {7: "cursor_session"}
    doc_id_of = _make_doc_id_of(cursor_to_doc)

    # Graph-arm hit: carries a chunked source_id → canonicalized, map ignored.
    assert doc_id_of(_StubHit(id=7, source_id="sess_42#c2")) == "sess_42"
    # Two-arm hit: no source_id → cursor map.
    assert doc_id_of(_StubHit(id=7, source_id=None)) == "cursor_session"
    # Unknown cursor, no source_id → stringified id fallback.
    assert doc_id_of(_StubHit(id=99, source_id=None)) == "99"

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


# ===========================================================================
# Slice 5 (D0a) — power-sized gold re-pin + answerer seam + Mem0-OSS de-risk
# ===========================================================================

from eval.decision_rule_083 import MEMORY_CLASSES  # noqa: E402 — Slice-5 contract


# ---------------------------------------------------------------------------
# S5-RED-A — corpus-validity guard (the Slice-25 N=0 regression this slice fixes)
# ---------------------------------------------------------------------------


def _good_manifest(n_min: int = 150) -> dict:
    return {
        "corpus_hash": "lmeoracle_deadbeef",
        "n_min": n_min,
        "per_class_gold_counts": {c: n_min for c in MEMORY_CLASSES},
    }


def test_corpus_validity_catches_n0_memory_class() -> None:
    """The guard that was MISSING in Slice 25: a class at N=0 must be flagged.
    Demonstrates the catch (the whole reason this slice exists)."""
    from eval.corpus_validity import validate_repin

    m = _good_manifest()
    m["per_class_gold_counts"]["knowledge_update"] = 0  # the N=0 regression
    problems = validate_repin(m, [])
    assert problems, "validate_repin failed to catch an N=0 memory class"
    assert any("knowledge_update" in p for p in problems)
    assert any("0" in p or "N=0" in p.lower() or "empty" in p.lower() for p in problems)


def test_corpus_validity_catches_under_n_min_class() -> None:
    """A class present but below n_min must be flagged (not silently accepted)."""
    from eval.corpus_validity import validate_repin

    m = _good_manifest(n_min=150)
    m["per_class_gold_counts"]["temporal"] = 149  # one short of n_min
    problems = validate_repin(m, [])
    assert any("temporal" in p for p in problems), problems


def test_corpus_validity_catches_missing_memory_class() -> None:
    """A frozen MEMORY_CLASS entirely absent from the manifest is a failure."""
    from eval.corpus_validity import validate_repin

    m = _good_manifest()
    del m["per_class_gold_counts"]["multi_session"]
    problems = validate_repin(m, [])
    assert any("multi_session" in p for p in problems), problems


def test_corpus_validity_passes_a_well_formed_manifest() -> None:
    """Non-vacuous: a fully-populated, at-or-above-n_min manifest is clean."""
    from eval.corpus_validity import validate_repin

    assert validate_repin(_good_manifest(), []) == []


def test_pinned_d0a_manifest_is_valid() -> None:
    """The REAL pinned re-pin manifest: every MEMORY_CLASS ≥ n_min, no N=0."""
    from eval.corpus_validity import validate_repin

    manifest = json.loads(_D0A_MANIFEST.read_text(encoding="utf-8"))
    gold = json.loads(_D0A_GOLD.read_text(encoding="utf-8"))
    queries = gold.get("queries", [])
    assert validate_repin(manifest, queries) == [], "pinned d0a manifest is invalid"

    cc = manifest["per_class_gold_counts"]
    n_min = manifest["n_min"]
    for c in MEMORY_CLASSES:
        assert cc.get(c, 0) >= n_min, f"class {c} under n_min: {cc.get(c)} < {n_min}"
        assert cc[c] > 0, f"class {c} is N=0"


# ---------------------------------------------------------------------------
# S5-RED-B — identical-answerer seam runs end-to-end over the re-pinned gold
# ---------------------------------------------------------------------------


def test_answerer_seam_scores_over_repinned_gold() -> None:
    """The seam: load the re-pinned LME gold via the non-COR-2 loader, run the
    identical (stub) answerer over a few queries, get a scored result. No
    R2_RUN, no network, no live LLM — the wiring smoke."""
    from eval.r2_parity_eval import load_repin_gold

    corpus_hash, queries = load_repin_gold(_D0A_GOLD)
    assert corpus_hash, "re-pin gold carries a corpus_hash"
    assert queries, "re-pin gold carries queries"

    harness = R2Harness.from_repin_gold(_D0A_GOLD, StubAnswerer())
    assert harness.answerer.available  # stub answerer is always available

    # A stub adapter that returns the gold doc for each query → recall is scorable.
    hits_by_q = {
        q.question: [Hit(doc_id=d, body=f"body for {d}", score=1.0) for d in q.gold_doc_ids]
        for q in harness.queries
        if q.gold_doc_ids
    }
    systems = {"fathomdb": StubAdapter(name="fathomdb", hits_by_query=hits_by_q)}
    out = harness.run(systems, k=10, limit=5)

    assert out["answerer_available"] is True
    assert out["answerer_model"] == StubAnswerer.model_id
    # the memory classes the resolution scores must be representable in the output
    assert set(out["n_queries_per_class"]) >= set(MEMORY_CLASSES)
    # at least one query was actually scored end-to-end
    assert sum(out["n_queries_per_class"].values()) > 0


# ---------------------------------------------------------------------------
# S5-RED-C — Mem0-OSS adapter conformance (fake in-memory backend; no mem0ai)
# ---------------------------------------------------------------------------


class _FakeMem0Backend:
    """In-memory stand-in for ``mem0.Memory`` exposing only what the adapter
    calls (``add`` / ``search``) — so the conformance test needs no live mem0ai."""

    def __init__(self) -> None:
        self._mems: list[dict] = []

    def add(self, body, *, user_id, metadata=None):  # noqa: ANN001
        self._mems.append({"memory": body, "metadata": metadata or {}, "id": f"m{len(self._mems)}"})

    def search(self, *, query, user_id, limit):  # noqa: ANN001
        # naive substring relevance → deterministic ordering
        scored = []
        for i, m in enumerate(self._mems):
            score = 1.0 if any(t in m["memory"].lower() for t in query.lower().split()) else 0.0
            scored.append((score, -i, m))
        scored.sort(reverse=True)
        return {"results": [
            {"id": m["id"], "memory": m["memory"], "metadata": m["metadata"], "score": s}
            for s, _, m in scored[:limit]
        ]}


def test_mem0_adapter_returns_topk_hits_under_shared_contract() -> None:
    adapter = Mem0OSSAdapter(memory=_FakeMem0Backend())
    assert adapter.available is True
    adapter.ingest({"doc-1": "Paris is the capital of France.",
                    "doc-2": "Berlin is the capital of Germany.",
                    "doc-3": "Rome is the capital of Italy."})

    hits = adapter.retrieve("What is the capital of France?", k=2)
    assert isinstance(hits, list)
    assert len(hits) <= 2
    assert all(isinstance(h, Hit) for h in hits)
    assert hits[0].doc_id == "doc-1"  # metadata doc_id round-trips
    assert "Paris" in hits[0].body


def test_mem0_adapter_unavailable_without_backend() -> None:
    """The §9 null-vs-zero distinction: no backend ⇒ not available (clean blocker),
    never a silent empty result."""
    adapter = Mem0OSSAdapter(memory=None)
    assert adapter.available is False
    with pytest.raises(RuntimeError):
        adapter.retrieve("anything", k=5)


def test_local_mem0_config_is_footprint_safe() -> None:
    """The de-risk output: the pinned local backend config is LOCAL (no cloud)."""
    from eval.mem0_local import build_local_mem0_config

    cfg = build_local_mem0_config(api_key="sk-test")
    # LLM points at the local airlock OpenAI-compatible base URL, not the cloud.
    assert "localhost" in cfg["llm"]["config"]["openai_base_url"]
    assert cfg["llm"]["config"]["model"].startswith("qwen")
    # embedder + vector store are local providers (no Mem0 cloud, ADR §3.6).
    assert cfg["embedder"]["provider"] == "huggingface"
    assert cfg["vector_store"]["provider"] in {"chroma", "faiss"}
