from __future__ import annotations

from pathlib import Path

import pytest


def test_id_helpers_and_open_integrity_report(tmp_path: Path) -> None:
    from fathomdb import Engine, new_id, new_row_id

    db = Engine.open(tmp_path / "agent.db")

    assert len(new_id()) == 26
    assert "-" in new_row_id()

    report = db.admin.check_integrity()
    assert report.physical_ok is True
    assert report.foreign_keys_ok is True
    assert report.missing_fts_rows == 0
    assert report.duplicate_active_logical_ids == 0


def test_write_and_text_query_round_trip(tmp_path: Path) -> None:
    from fathomdb import ChunkInsert, ChunkPolicy, Engine, NodeInsert, WriteRequest, new_row_id

    db = Engine.open(tmp_path / "agent.db")

    receipt = db.write(
        WriteRequest(
            label="meeting-ingest",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="meeting:budget-2026-03-25",
                    kind="Meeting",
                    properties={"title": "Budget review", "status": "active"},
                    source_ref="action:meeting-import",
                    upsert=True,
                    chunk_policy=ChunkPolicy.REPLACE,
                )
            ],
            chunks=[
                ChunkInsert(
                    id="chunk:meeting:budget-2026-03-25:0",
                    node_logical_id="meeting:budget-2026-03-25",
                    text_content="Budget discussion and action items",
                )
            ],
        )
    )

    assert receipt.label == "meeting-ingest"
    assert receipt.optional_backfill_count == 0
    assert receipt.provenance_warnings == []

    rows = (
        db.nodes("Meeting")
        .text_search("budget", limit=5)
        .filter_json_text_eq("$.status", "active")
        .limit(10)
        .execute()
    )

    assert rows.was_degraded is False
    assert len(rows.nodes) == 1
    assert rows.nodes[0].logical_id == "meeting:budget-2026-03-25"
    assert rows.nodes[0].properties["title"] == "Budget review"


def test_trace_and_excise_source(tmp_path: Path) -> None:
    from fathomdb import ChunkInsert, ChunkPolicy, Engine, NodeInsert, WriteRequest, new_row_id

    db = Engine.open(tmp_path / "agent.db")

    db.write(
        WriteRequest(
            label="meeting-ingest",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="meeting:trace-test",
                    kind="Meeting",
                    properties={"title": "Trace me", "status": "active"},
                    source_ref="action:trace-test",
                    upsert=True,
                    chunk_policy=ChunkPolicy.REPLACE,
                )
            ],
            chunks=[
                ChunkInsert(
                    id="chunk:meeting:trace-test:0",
                    node_logical_id="meeting:trace-test",
                    text_content="traceable transcript",
                )
            ],
        )
    )

    trace = db.admin.trace_source("action:trace-test")
    assert trace.source_ref == "action:trace-test"
    assert trace.node_rows == 1
    assert trace.node_logical_ids == ["meeting:trace-test"]

    excised = db.admin.excise_source("action:trace-test")
    assert excised.source_ref == "action:trace-test"
    assert excised.node_rows == 1

    rows = db.nodes("Meeting").text_search("traceable", limit=5).execute()
    assert rows.nodes == []


def test_invalid_json_path_raises_compile_error(tmp_path: Path) -> None:
    from fathomdb import CompileError, Engine

    db = Engine.open(tmp_path / "agent.db")

    with pytest.raises(CompileError):
        db.nodes("Meeting").filter_json_text_eq("status", "active").compile()


def test_vector_query_degrades_when_vector_table_absent(tmp_path: Path) -> None:
    from fathomdb import Engine

    db = Engine.open(tmp_path / "agent.db")
    rows = db.nodes("Meeting").vector_search("budget", limit=3).execute()
    assert rows.was_degraded is True


def test_vector_write_and_search_round_trip(tmp_path: Path) -> None:
    from fathomdb import ChunkInsert, ChunkPolicy, Engine, NodeInsert, ProjectionTarget, VecInsert, WriteRequest, new_row_id

    db = Engine.open(tmp_path / "agent.db", vector_dimension=4)

    receipt = db.write(
        WriteRequest(
            label="vector-ingest",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="document:vector-2026-03-26",
                    kind="Document",
                    properties={"title": "Vector retrieval", "status": "active"},
                    source_ref="action:vector-import",
                    upsert=True,
                    chunk_policy=ChunkPolicy.REPLACE,
                )
            ],
            chunks=[
                ChunkInsert(
                    id="chunk:document:vector-2026-03-26:0",
                    node_logical_id="document:vector-2026-03-26",
                    text_content="Vector retrieval payload",
                )
            ],
            vec_inserts=[
                VecInsert(
                    chunk_id="chunk:document:vector-2026-03-26:0",
                    embedding=[0.1, 0.2, 0.3, 0.4],
                )
            ],
        )
    )

    assert receipt.provenance_warnings == []

    rows = db.nodes("Document").vector_search("[0.1, 0.2, 0.3, 0.4]", limit=5).execute()

    assert rows.was_degraded is False
    assert len(rows.nodes) >= 1
    assert any(node.logical_id == "document:vector-2026-03-26" for node in rows.nodes)

    repair = db.admin.rebuild(target=ProjectionTarget.VEC)
    assert repair.targets == [ProjectionTarget.VEC]

    semantics = db.admin.check_semantics()
    assert semantics.stale_vec_rows == 0
    assert semantics.vec_rows_for_superseded_nodes == 0
