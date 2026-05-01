from __future__ import annotations

from pathlib import Path


def test_public_python_touch_last_accessed_surfaces_metadata_on_flat_and_grouped_reads(
    tmp_path: Path,
) -> None:
    from fathomdb import (
        ChunkInsert,
        ChunkPolicy,
        EdgeInsert,
        Engine,
        LastAccessTouchRequest,
        NodeInsert,
        TraverseDirection,
        WriteRequest,
        new_row_id,
    )

    db = Engine.open(tmp_path / "agent.db")

    db.write(
        WriteRequest(
            label="seed-touch",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="meeting-1",
                    kind="Meeting",
                    properties={"title": "Budget review"},
                    source_ref="source:meeting",
                    upsert=False,
                    chunk_policy=ChunkPolicy.REPLACE,
                ),
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="task-1",
                    kind="Task",
                    properties={"title": "Draft memo"},
                    source_ref="source:task",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
            ],
            edges=[
                EdgeInsert(
                    row_id=new_row_id(),
                    logical_id="edge-1",
                    source_logical_id="meeting-1",
                    target_logical_id="task-1",
                    kind="HAS_TASK",
                    properties={},
                    source_ref="source:edge",
                    upsert=False,
                )
            ],
            chunks=[
                ChunkInsert(
                    id="chunk-meeting-1",
                    node_logical_id="meeting-1",
                    text_content="budget review agenda",
                )
            ],
        )
    )

    report = db.touch_last_accessed(
        LastAccessTouchRequest(
            logical_ids=["meeting-1", "task-1", "task-1"],
            touched_at=1711843200,
        )
    )
    assert report.touched_logical_ids == 2
    assert report.touched_at == 1711843200

    flat = db.nodes("Meeting").filter_logical_id_eq("meeting-1").execute()
    assert flat.nodes[0].last_accessed_at == 1711843200

    grouped = (
        db.nodes("Meeting")
        .filter_logical_id_eq("meeting-1")
        .expand(slot="tasks", direction=TraverseDirection.OUT, label="HAS_TASK", max_depth=1)
        .execute_grouped()
    )
    assert grouped.roots[0].last_accessed_at == 1711843200
    assert grouped.expansions[0].roots[0].nodes[0].last_accessed_at == 1711843200
