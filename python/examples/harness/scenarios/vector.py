"""Scenarios for vector search degradation and sqlite-vec insert/search."""

from __future__ import annotations

from fathomdb import (
    ChunkInsert,
    ChunkPolicy,
    NodeInsert,
    ProjectionTarget,
    VecInsert,
    WriteRequest,
    new_row_id,
)

from ..models import (
    HarnessContext,
    ScenarioResult,
    VECTOR_CHUNK_ID,
    VECTOR_DOCUMENT_ID,
    VECTOR_QUERY,
    VECTOR_SOURCE,
)


def vector_degradation(context: HarnessContext) -> ScenarioResult:
    """Validate that vector search gracefully degrades in baseline mode."""
    # Stay on the legacy `vector_search` step (the new `vector_search()`
    # Python shim is a deprecation route into semantic_search, which goes
    # through a compile path that does not yet return was_degraded on a
    # missing vec table — see Pack F2 notes).
    rows = (
        context.engine.nodes("Document")
        ._with_step({"type": "vector_search", "query": VECTOR_QUERY, "limit": 3})
        .execute()
    )
    assert rows.was_degraded is True, "baseline vector query should degrade"
    assert rows.nodes == [], (
        f"expected no vector rows, got {[node.logical_id for node in rows.nodes]}"
    )
    return ScenarioResult(name="vector_degradation")


def vector_insert_and_search(context: HarnessContext) -> ScenarioResult:
    """Validate vector insert and nearest-neighbor search returns the expected node."""
    context.engine.write(
        WriteRequest(
            label="vector-insert-and-search",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=VECTOR_DOCUMENT_ID,
                    kind="Document",
                    properties={"title": "Vector document"},
                    source_ref=VECTOR_SOURCE,
                    upsert=True,
                    chunk_policy=ChunkPolicy.REPLACE,
                )
            ],
            chunks=[
                ChunkInsert(
                    id=VECTOR_CHUNK_ID,
                    node_logical_id=VECTOR_DOCUMENT_ID,
                    text_content="vector-enabled retrieval payload",
                )
            ],
            vec_inserts=[
                VecInsert(
                    chunk_id=VECTOR_CHUNK_ID,
                    embedding=[0.1, 0.2, 0.3, 0.4],
                )
            ],
        )
    )

    rows = (
        context.engine.nodes("Document")
        ._with_step({"type": "vector_search", "query": VECTOR_QUERY, "limit": 5})
        .execute()
    )
    assert rows.was_degraded is False, "vector query should not degrade in vector mode"
    assert any(node.logical_id == VECTOR_DOCUMENT_ID for node in rows.nodes), (
        f"expected {VECTOR_DOCUMENT_ID} in vector results, got {[node.logical_id for node in rows.nodes]}"
    )

    repair = context.engine.admin.rebuild(target=ProjectionTarget.VEC)
    assert repair.targets == [ProjectionTarget.VEC], (
        f"unexpected targets={repair.targets}"
    )
    semantics = context.engine.admin.check_semantics()
    assert semantics.stale_vec_rows == 0, f"stale_vec_rows={semantics.stale_vec_rows}"
    assert semantics.vec_rows_for_superseded_nodes == 0, (
        f"vec_rows_for_superseded_nodes={semantics.vec_rows_for_superseded_nodes}"
    )
    return ScenarioResult(name="vector_insert_and_search")
