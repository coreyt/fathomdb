"""Slice 30 Option 2 — ELPS live harness + memory-class gold + graph-arm R2 run.

TDD: RED commit first (all tests import the not-yet-created modules and assert the
not-yet-implemented behaviours). GREEN commit lands after implementation.

Tests:
  T1  test_elps_harness_hello_ready_protocol      — subprocess handshake
  T2  test_elps_harness_extract_stub_response     — extract round-trip in STUB_MODE
  T3  test_build_fathomdb_elps_path_uses_ingest_with_extractor  — _build_fathomdb ELPS path
  T4  test_extra_gold_queries_appear_in_r2_results — extra-gold merge + temporal class present
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------

_SRC_PYTHON = Path(__file__).resolve().parents[1]  # src/python
_HARNESS = _SRC_PYTHON / "eval" / "elps_live_harness.py"


# ---------------------------------------------------------------------------
# T1 — handshake
# ---------------------------------------------------------------------------


def test_elps_harness_hello_ready_protocol() -> None:
    """Spawn the harness in STUB_MODE; send hello; expect ready."""
    proc = subprocess.Popen(
        [sys.executable, str(_HARNESS)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env={**os.environ, "ELPS_STUB_MODE": "1"},
        text=True,
    )
    try:
        proc.stdin.write(  # type: ignore[union-attr]
            json.dumps({"protocol": "fathomdb.extract.v1", "type": "hello"}) + "\n"
        )
        proc.stdin.flush()  # type: ignore[union-attr]
        line = proc.stdout.readline()  # type: ignore[union-attr]
    finally:
        proc.stdin.close()  # type: ignore[union-attr]
        proc.wait(timeout=10)

    resp = json.loads(line)
    assert resp["type"] == "ready", f"expected 'ready', got: {resp}"
    assert resp["protocol"] == "fathomdb.extract.v1"
    assert resp["schema_version"] == 1
    assert "provider" in resp
    assert "max_docs_per_request" in resp


# ---------------------------------------------------------------------------
# T2 — extract stub response
# ---------------------------------------------------------------------------


def test_elps_harness_extract_stub_response() -> None:
    """After hello/ready, send an extract request; verify result structure."""
    proc = subprocess.Popen(
        [sys.executable, str(_HARNESS)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env={**os.environ, "ELPS_STUB_MODE": "1"},
        text=True,
    )
    try:
        # handshake
        proc.stdin.write(  # type: ignore[union-attr]
            json.dumps({"protocol": "fathomdb.extract.v1", "type": "hello"}) + "\n"
        )
        proc.stdin.flush()  # type: ignore[union-attr]
        ready_line = proc.stdout.readline()  # type: ignore[union-attr]
        ready = json.loads(ready_line)
        assert ready["type"] == "ready"

        # extract
        request_id = "test-req-001"
        extract_msg = {
            "protocol": "fathomdb.extract.v1",
            "type": "extract",
            "request_id": request_id,
            "documents": [
                {
                    "source_doc_id": "doc-001",
                    "kind": "doc",
                    "created_at": "2025-01-01T00:00:00Z",
                    "body": "Alice Smith joined Acme Corp in March 2023.",
                }
            ],
        }
        proc.stdin.write(json.dumps(extract_msg) + "\n")  # type: ignore[union-attr]
        proc.stdin.flush()  # type: ignore[union-attr]
        result_line = proc.stdout.readline()  # type: ignore[union-attr]
    finally:
        proc.stdin.close()  # type: ignore[union-attr]
        proc.wait(timeout=10)

    result = json.loads(result_line)
    assert result["type"] == "result", f"expected 'result', got: {result}"
    assert result["request_id"] == request_id
    assert isinstance(result["entities"], list), "entities must be a list"
    assert isinstance(result["edges"], list), "edges must be a list"
    # Verify edge structure
    for edge in result["edges"]:
        assert "from_entity" in edge
        assert "to_entity" in edge
        assert "relation" in edge
        assert "t_valid" in edge
        assert "confidence" in edge


# ---------------------------------------------------------------------------
# T3 — _build_fathomdb ELPS path
# ---------------------------------------------------------------------------


def test_build_fathomdb_elps_path_uses_ingest_with_extractor(tmp_path: Path) -> None:
    """_build_fathomdb with elps_harness_cmd calls ingest_with_extractor.

    Requires ELPS_STUB_MODE=1 (set in subprocess env below), not in this process.
    Uses a small doc set (3 docs) so it's fast.
    """
    from eval.r2_parity_eval import _build_fathomdb

    docs = {
        "doc-a": "Alice Smith joined Acme Corp in March 2023.",
        "doc-b": "Acme Corp was founded in 2010 by Bob Jones.",
        "doc-c": "The annual revenue of Acme Corp reached $1M in 2022.",
    }
    db = tmp_path / "test_elps.sqlite"

    # Set ELPS_STUB_MODE in the environment so the harness subprocess sees it.
    old_env = os.environ.get("ELPS_STUB_MODE")
    os.environ["ELPS_STUB_MODE"] = "1"
    try:
        adapter, blocker = _build_fathomdb(
            docs,
            db,
            elps_harness_cmd=[sys.executable, str(_HARNESS)],
            elps_limit=3,
            use_graph_arm=True,
        )
    finally:
        if old_env is None:
            os.environ.pop("ELPS_STUB_MODE", None)
        else:
            os.environ["ELPS_STUB_MODE"] = old_env

    assert blocker is None, f"unexpected blocker: {blocker}"
    assert adapter is not None, "adapter should not be None on ELPS ingest success"
    assert adapter._use_graph_arm is True, "_use_graph_arm should be True"
    assert db.exists(), "DB file should have been created"

    # Verify retrieval doesn't crash
    hits = adapter.retrieve("Acme Corp revenue", k=5)
    assert isinstance(hits, list)


# ---------------------------------------------------------------------------
# T4 — extra gold queries appear in R2 results
# ---------------------------------------------------------------------------


def _write_gold(path: Path, queries: list[dict], corpus_hash: str = "fe973fcd49fb_stub") -> Path:
    path.write_text(
        json.dumps({"corpus_hash": corpus_hash, "qrels_version": "stub-v1", "queries": queries}),
        encoding="utf-8",
    )
    return path


def test_extra_gold_queries_appear_in_r2_results(tmp_path: Path) -> None:
    """Merge main + extra gold; verify temporal class appears in results."""
    from eval.r2_parity_eval import (
        Hit,
        NullAnswerer,
        R2Harness,
        StubAdapter,
        _parse_gold,
    )

    # Main gold: factoid class only
    main_gold = _write_gold(
        tmp_path / "main.gold.json",
        queries=[
            {
                "query_id": "q-1",
                "query": "What is the capital of France?",
                "query_class": "factoid",
                "answers": ["Paris"],
                "required_evidence": [{"doc_id": "doc-fr"}],
            }
        ],
    )

    # Extra gold: temporal class
    extra_raw = json.loads(
        _write_gold(
            tmp_path / "extra.gold.json",
            queries=[
                {
                    "query_id": "mc-001",
                    "query": "When did Alice join Acme?",
                    "query_class": "temporal",
                    "answers": ["March 2023"],
                    "required_evidence": [{"doc_id": "doc-a"}],
                }
            ],
        ).read_text(encoding="utf-8")
    )

    # Build harness with merged queries
    harness = R2Harness(gold_path=main_gold, answerer=NullAnswerer())
    extra_queries = _parse_gold(extra_raw)
    harness.queries = list(harness.queries) + extra_queries  # type: ignore[assignment]

    hits_by_query = {
        "What is the capital of France?": [Hit(doc_id="doc-fr", body="Paris.", score=1.0)],
        "When did Alice join Acme?": [Hit(doc_id="doc-a", body="March 2023.", score=1.0)],
    }
    systems = {
        "fathomdb": StubAdapter(name="fathomdb", hits_by_query=hits_by_query),
        "naive_rag": StubAdapter(name="naive_rag", hits_by_query=hits_by_query),
    }

    result = harness.run(systems, k=5)

    assert "temporal" in result["n_queries_per_class"], "temporal class must appear in n_queries"
    assert result["n_queries_per_class"]["temporal"] >= 1, "temporal class must have queries"

    fdb_temporal = result["r2_results"]["fathomdb"].get("temporal")
    assert fdb_temporal is not None, "fathomdb temporal result must exist"
    assert fdb_temporal["recall_at_k"] is not None, "temporal recall_at_k must not be null"
    assert isinstance(fdb_temporal["recall_at_k"], float), "temporal recall must be a float"
