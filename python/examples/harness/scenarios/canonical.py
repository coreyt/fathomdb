"""Scenarios for canonical node/chunk/FTS operations and upsert supersession."""

from __future__ import annotations

from fathomdb import ChunkInsert, ChunkPolicy, NodeInsert, WriteRequest, new_row_id

from ..models import (
    CANONICAL_MEETING_CHUNK_ID,
    CANONICAL_MEETING_ID,
    CANONICAL_MEETING_SOURCE,
    HarnessContext,
    ScenarioResult,
    UPSERT_SOURCE_V1,
    UPSERT_SOURCE_V2,
    UPSERT_TASK_ID,
)
from ..verify import assert_integrity_clean, assert_semantics_clean, assert_single_node


def canonical_node_chunk_fts(context: HarnessContext) -> ScenarioResult:
    """Validate node insert, chunk attach, and full-text search round-trip."""
    context.engine.write(
        WriteRequest(
            label="canonical-node-chunk-fts",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=CANONICAL_MEETING_ID,
                    kind="Meeting",
                    properties={
                        "title": "Q1 budget review",
                        "status": "active",
                        "marker": "quarterlybudgetneedle",
                    },
                    source_ref=CANONICAL_MEETING_SOURCE,
                    upsert=True,
                    chunk_policy=ChunkPolicy.REPLACE,
                )
            ],
            chunks=[
                ChunkInsert(
                    id=CANONICAL_MEETING_CHUNK_ID,
                    node_logical_id=CANONICAL_MEETING_ID,
                    text_content="quarterlybudgetneedle and action items for finance",
                )
            ],
        )
    )

    direct_rows = (
        context.engine.nodes("Meeting").filter_logical_id_eq(CANONICAL_MEETING_ID).limit(1).execute()
    )
    assert_single_node(direct_rows, CANONICAL_MEETING_ID)
    assert direct_rows.nodes[0].properties["title"] == "Q1 budget review"

    text_rows = (
        context.engine.nodes("Meeting")
        .text_search("quarterlybudgetneedle", limit=5)
        .limit(5)
        .execute()
    )
    assert_single_node(text_rows, CANONICAL_MEETING_ID)

    assert_integrity_clean(context.engine.admin.check_integrity())
    return ScenarioResult(name="canonical_node_chunk_fts")


def node_upsert_supersession(context: HarnessContext) -> ScenarioResult:
    """Validate that upserting a node supersedes the prior version cleanly."""
    context.engine.write(
        WriteRequest(
            label="upsert-v1",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=UPSERT_TASK_ID,
                    kind="Task",
                    properties={"title": "Draft follow-up", "revision": "v1"},
                    source_ref=UPSERT_SOURCE_V1,
                    upsert=False,
                )
            ],
        )
    )
    context.engine.write(
        WriteRequest(
            label="upsert-v2",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=UPSERT_TASK_ID,
                    kind="Task",
                    properties={"title": "Send follow-up email", "revision": "v2"},
                    source_ref=UPSERT_SOURCE_V2,
                    upsert=True,
                )
            ],
        )
    )

    active_rows = context.engine.nodes("Task").filter_logical_id_eq(UPSERT_TASK_ID).limit(1).execute()
    assert_single_node(active_rows, UPSERT_TASK_ID)
    assert active_rows.nodes[0].properties["revision"] == "v2"

    from_v1 = context.engine.admin.trace_source(UPSERT_SOURCE_V1)
    assert from_v1.node_rows == 1
    assert from_v1.node_logical_ids == [UPSERT_TASK_ID]

    from_v2 = context.engine.admin.trace_source(UPSERT_SOURCE_V2)
    assert from_v2.node_rows == 1
    assert from_v2.node_logical_ids == [UPSERT_TASK_ID]

    assert_semantics_clean(context.engine.admin.check_semantics())
    return ScenarioResult(name="node_upsert_supersession")
