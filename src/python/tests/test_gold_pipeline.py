"""0.8.8 Slice 20 (OPP-9) real-gold pipeline test.

Two halves:

1. ENGINE-DRIVEN (R-TEL-2): drive the Slice-15 telemetry sink with a real
   engine search + agent feedback, then assert ``build_gold_records`` produces
   exactly one valid :class:`~eval.gold_capture.GoldRecord` in the engine id
   namespace and that the offline frozen-candidate scorer scores it sanely.
   Mirrors ``src/python/tests/test_telemetry_parity.py`` for the engine setup.

2. PURE-UNIT: feed ``build_gold_records`` a hand-written JSONL fixture string
   (no engine) covering malformed lines and the no-feedback (dropped) case.

EVAL-ONLY surface (``eval/`` modules are test-infra, not shipped in the wheel).
"""

from __future__ import annotations

import time
from pathlib import Path

from eval.frozen_candidate_scorer import score_gold, score_gold_record
from eval.gold_capture import (
    ID_SPACE,
    PROVENANCE,
    SCHEMA_VERSION,
    GoldRecord,
    build_gold_records,
)
from fathomdb import Engine, SearchHit

# FTS-only corpus (mirrors test_telemetry_parity.py).
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


def test_gold_pipeline_engine_to_record_and_score(db_path: str, tmp_path: Path) -> None:
    """Engine telemetry → GoldRecord → offline score, end to end."""
    sink = tmp_path / "telemetry.jsonl"
    engine = Engine.open(db_path)
    try:
        _seed(engine)
        # Warm projection BEFORE enabling telemetry so the post-enable search is a
        # single-shot deterministic capture (one event row).
        warm = _search_after_projection(engine, "hybrid")
        assert warm, "projection should be ready before enabling telemetry"

        engine.enable_telemetry(str(sink))

        result = engine.search("hybrid")
        assert result.results, "expected hits to capture"
        qid = engine.last_telemetry_query_id()
        assert qid == "q0-0"

        returned_ids = [sh.id for sh in result.results]
        relevant_id = returned_ids[0]
        # Label the top returned hit relevant; no irrelevant labels.
        engine.record_feedback(qid, [relevant_id], [], "agent:test")
    finally:
        engine.close()

    records = build_gold_records(str(sink), embedder_id="bge-small@test (dim=384)")
    assert len(records) == 1, f"expected exactly one labeled gold record, got {len(records)}"
    rec = records[0]

    # Shape + namespace contract (§B.2 / §3d).
    assert rec.schema_version == SCHEMA_VERSION
    assert rec.query_id == "q0-0"
    assert rec.id_space == ID_SPACE == "engine-logical-id"
    assert rec.provenance == PROVENANCE == "telemetry-capture"
    assert rec.embedder_id == "bge-small@test (dim=384)"
    assert rec.query_chars == len("hybrid")

    # candidate_ids == the search result ids (the frozen returned pool), in order.
    assert rec.candidate_ids == tuple(returned_ids)
    # The fed id is labeled relevant (1).
    assert rec.labels[relevant_id] == 1
    assert all(v in (0, 1) for v in rec.labels.values())

    # Offline frozen-candidate scorer: the relevant id IS in candidates → recall 1.0;
    # every labeled-returned candidate is relevant → precision 1.0.
    metrics = score_gold_record(rec)
    assert metrics["recall"] == 1.0
    assert metrics["precision"] == 1.0
    assert metrics["n_relevant_labels"] == 1
    assert metrics["n_candidates"] == len(returned_ids)

    agg = score_gold([rec])
    assert agg["n_records"] == 1
    assert agg["mean_recall"] == 1.0
    assert agg["mean_precision"] == 1.0


# --------------------------------------------------------------------------- #
# Pure-unit: build_gold_records over a hand-written JSONL fixture (no engine)
# --------------------------------------------------------------------------- #


def test_build_gold_records_malformed_and_no_feedback(tmp_path: Path) -> None:
    """Malformed lines are skipped; events without feedback are dropped."""
    sink = tmp_path / "fixture.jsonl"
    sink.write_text(
        "\n".join(
            [
                # q-labeled: event + feedback (relevant 10, irrelevant 11) → gold.
                '{"type":"event","schema_version":1,"query_id":"q-labeled",'
                '"query_chars":5,"result_ids":[10,11,12],"arm_of":{"10":"vector"}}',
                '{"type":"feedback","schema_version":1,"query_id":"q-labeled",'
                '"relevant_ids":[10],"irrelevant_ids":[11],"label_source":"agent:x"}',
                # q-nofb: event only, no feedback → dropped (no unlabeled gold).
                '{"type":"event","schema_version":1,"query_id":"q-nofb",'
                '"query_chars":3,"result_ids":[20,21],"arm_of":{}}',
                # Malformed: not JSON.
                "this is not json at all",
                # Malformed: valid JSON but not an object.
                "[1, 2, 3]",
                # Blank line.
                "",
                # Feedback with no matching event → skipped.
                '{"type":"feedback","schema_version":1,"query_id":"q-orphan",'
                '"relevant_ids":[99],"irrelevant_ids":[],"label_source":"agent:x"}',
            ]
        ),
        encoding="utf-8",
    )

    records = build_gold_records(str(sink))
    # Only q-labeled survives (q-nofb dropped, q-orphan has no event, malformed skipped).
    assert len(records) == 1
    rec = records[0]
    assert isinstance(rec, GoldRecord)
    assert rec.query_id == "q-labeled"
    assert rec.id_space == "engine-logical-id"
    assert rec.candidate_ids == (10, 11, 12)
    assert rec.labels == {10: 1, 11: 0}
    assert rec.query_chars == 5
    assert rec.embedder_id == ""  # default sentinel

    # Scoring the fixture: relevant id 10 is in candidates → recall 1.0; of the
    # labeled-returned {10:1, 11:0}, one of two is relevant → precision 0.5.
    metrics = score_gold_record(rec)
    assert metrics["recall"] == 1.0
    assert metrics["precision"] == 0.5
    assert metrics["n_labeled_returned"] == 2
    assert metrics["n_relevant_labels"] == 1


def test_build_gold_records_relevant_not_in_candidates() -> None:
    """A relevant label outside the frozen pool drives recall below 1.0."""
    sink_dir = Path(__file__).parent
    # Build via a tmp file to keep the no-engine path identical.
    import tempfile

    with tempfile.NamedTemporaryFile("w", suffix=".jsonl", delete=False, dir=sink_dir) as fh:
        fh.write(
            '{"type":"event","schema_version":1,"query_id":"q1",'
            '"query_chars":4,"result_ids":[1,2],"arm_of":{}}\n'
        )
        fh.write(
            '{"type":"feedback","schema_version":1,"query_id":"q1",'
            '"relevant_ids":[1,3],"irrelevant_ids":[],"label_source":"agent:x"}\n'
        )
        path = fh.name
    try:
        records = build_gold_records(path)
    finally:
        Path(path).unlink()

    assert len(records) == 1
    rec = records[0]
    # Relevant ids {1, 3}; only 1 is in candidates {1, 2} → recall 0.5.
    metrics = score_gold_record(rec)
    assert metrics["recall"] == 0.5
    # Of labeled-returned candidates [1] (2 and 3 are unlabeled/absent), all relevant.
    assert metrics["precision"] == 1.0
    assert metrics["n_relevant_labels"] == 2
