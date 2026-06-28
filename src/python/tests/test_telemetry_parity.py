"""0.8.8 Slice 15 (OPP-9) telemetry-capture parity (Python SDK).

Mirrors the engine contract in
``src/rust/crates/fathomdb-engine/tests/telemetry_capture.rs``: telemetry is
off by default (no captured ``query_id``; the feedback API errors), an opt-in
local JSONL sink records a query→result event keyed on the stable id with a
deterministic sequential ``query_id`` (``q0-0``, ``q0-1`` …), a correlated
agent-feedback row is appended, and the privacy guarantees hold — the query
TEXT and ``source_id`` are NEVER written to the sink. The TypeScript half is
``src/ts/tests/telemetry-parity.test.ts`` (same contract, same corpus).
"""

from __future__ import annotations

import json
import time
from pathlib import Path

import pytest

from fathomdb import Engine, SearchHit

# FTS-only corpus. Both query words ("hybrid", "retrieval") must NOT be
# substrings of any JSONL key the sink emits (type/query_id/result_ids/arm_of/
# label_source/branch names) so the privacy assertions are meaningful.
_CORPUS = [
    {"kind": "doc", "body": "hybrid retrieval alpha"},
    {"kind": "doc", "body": "hybrid retrieval beta"},
]


def _search_after_projection(engine: Engine, query: str) -> list[SearchHit]:
    """Poll search until async projection has caught up (non-empty hits)."""
    deadline = time.monotonic() + 10.0
    last: list[SearchHit] = []
    while time.monotonic() < deadline:
        result = engine.search(query)
        last = list(result.results)
        if last:
            return last
        time.sleep(0.02)
    return last


def _seed(engine: Engine) -> None:
    for doc in _CORPUS:
        engine.write([doc])
    engine.drain(timeout_s=30)


def test_telemetry_off_by_default(db_path: str) -> None:
    engine = Engine.open(db_path)
    try:
        _seed(engine)
        hits = _search_after_projection(engine, "hybrid")
        assert hits, "expected hits"
        # Off by default: no captured query id...
        assert engine.last_telemetry_query_id() is None
        # ...and the feedback API errors when telemetry is off.
        with pytest.raises(Exception):
            engine.record_feedback("q0-0", [hits[0].id], [], "agent:test")
    finally:
        engine.close()


def test_telemetry_captures_event_and_feedback(db_path: str, tmp_path: Path) -> None:
    sink = tmp_path / "telemetry.jsonl"
    engine = Engine.open(db_path)
    try:
        _seed(engine)
        # Warm projection BEFORE enabling telemetry so each post-enable search is
        # a single-shot deterministic capture (every search after enable records
        # exactly one event — a poll loop would capture extras).
        warm = _search_after_projection(engine, "hybrid")
        assert warm, "projection should be ready before enabling telemetry"

        engine.enable_telemetry(str(sink))

        # First captured query → deterministic id "q0-0".
        r0 = engine.search("hybrid")
        assert r0.results, "expected hits to capture"
        assert engine.last_telemetry_query_id() == "q0-0"
        # Second query → "q0-1" (deterministic sequential id).
        engine.search("retrieval")
        assert engine.last_telemetry_query_id() == "q0-1"

        # Attach agent feedback correlated to the first query.
        engine.record_feedback("q0-0", [r0.results[0].id], [], "agent:test")
    finally:
        engine.close()

    body = sink.read_text(encoding="utf-8")
    lines = body.splitlines()
    # 2 event rows + 1 feedback row.
    assert len(lines) == 3, f"expected 2 events + 1 feedback, got {len(lines)}"

    ev0 = json.loads(lines[0])
    assert ev0["type"] == "event"
    assert ev0["query_id"] == "q0-0"
    assert ev0["schema_version"] == 1
    assert ev0["query_chars"] == len("hybrid")
    assert isinstance(ev0["result_ids"], list) and ev0["result_ids"]
    assert isinstance(ev0["arm_of"], dict)

    fb = json.loads(lines[2])
    assert fb["type"] == "feedback"
    assert fb["query_id"] == "q0-0"
    assert fb["label_source"] == "agent:test"

    # Privacy (ADR §C): the query TEXT never appears in the sink; only ids/length.
    assert "hybrid" not in body, "query text must NOT be captured"
    assert "retrieval" not in body, "query text must NOT be captured"
    # `source_id` is never a key in the sink (leak vector).
    assert "source_id" not in body, "source_id must NOT be captured"
