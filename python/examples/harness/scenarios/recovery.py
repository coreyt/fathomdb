"""Scenarios for provenance tracing, excision, safe export, projection rebuild, and vector profile restore."""

from __future__ import annotations

from fathomdb import ChunkInsert, ChunkPolicy, NodeInsert, ProjectionRepairReport, ProjectionTarget, WriteRequest, new_row_id

from ..engine_factory import open_engine
from ..models import (
    EXPORT_CHUNK_ID,
    EXPORT_DOCUMENT_ID,
    EXPORT_SOURCE,
    HarnessContext,
    ScenarioResult,
    TRACE_CHUNK_ID,
    TRACE_MEETING_ID,
    TRACE_SOURCE,
)
from ..verify import assert_integrity_clean, assert_no_nodes, assert_semantics_clean, assert_single_node, assert_trace


def trace_and_excise(context: HarnessContext) -> ScenarioResult:
    """Validate source tracing and excision removes all associated data."""
    engine = open_engine(
        context.sibling_db("trace-and-excise"),
        mode=context.mode,
        vector_dimension=context.vector_dimension,
    )

    engine.write(
        WriteRequest(
            label="trace-and-excise",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=TRACE_MEETING_ID,
                    kind="Meeting",
                    properties={"title": "Trace and excise", "marker": "traceableneedle"},
                    source_ref=TRACE_SOURCE,
                    upsert=True,
                    chunk_policy=ChunkPolicy.REPLACE,
                )
            ],
            chunks=[
                ChunkInsert(
                    id=TRACE_CHUNK_ID,
                    node_logical_id=TRACE_MEETING_ID,
                    text_content="traceableneedle appears only in this excision scenario",
                )
            ],
        )
    )

    before_trace = engine.admin.trace_source(TRACE_SOURCE)
    assert_trace(before_trace, node_rows=1, node_logical_ids=[TRACE_MEETING_ID])

    before_rows = engine.nodes("Meeting").text_search("traceableneedle", limit=5).execute()
    assert before_rows.was_degraded is False
    assert len(before_rows.hits) == 1
    assert before_rows.hits[0].node.logical_id == TRACE_MEETING_ID

    after_excise = engine.admin.excise_source(TRACE_SOURCE)
    assert_trace(after_excise, node_rows=1, node_logical_ids=[TRACE_MEETING_ID])

    direct_rows = engine.nodes("Meeting").filter_logical_id_eq(TRACE_MEETING_ID).execute()
    assert_no_nodes(direct_rows)

    after_rows = engine.nodes("Meeting").text_search("traceableneedle", limit=5).execute()
    assert after_rows.was_degraded is False
    assert after_rows.hits == ()

    assert_integrity_clean(engine.admin.check_integrity())
    report = engine.admin.check_semantics()
    assert report.stale_fts_rows == 0, f"stale_fts_rows={report.stale_fts_rows}"
    assert report.fts_rows_for_superseded_nodes == 0, (
        f"fts_rows_for_superseded_nodes={report.fts_rows_for_superseded_nodes}"
    )
    assert report.dangling_edges == 0, f"dangling_edges={report.dangling_edges}"
    return ScenarioResult(
        name="trace_and_excise",
        details={"orphaned_supersession_chains": report.orphaned_supersession_chains},
    )


def safe_export(context: HarnessContext) -> ScenarioResult:
    """Validate safe export produces a manifest with a checksum and page count."""
    context.engine.write(
        WriteRequest(
            label="safe-export-seed",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=EXPORT_DOCUMENT_ID,
                    kind="Document",
                    properties={"title": "Export verification"},
                    source_ref=EXPORT_SOURCE,
                    upsert=True,
                    chunk_policy=ChunkPolicy.REPLACE,
                )
            ],
            chunks=[
                ChunkInsert(
                    id=EXPORT_CHUNK_ID,
                    node_logical_id=EXPORT_DOCUMENT_ID,
                    text_content="export verification payload",
                )
            ],
        )
    )

    export_path = context.sibling_db("safe-export-copy")
    manifest = context.engine.admin.safe_export(export_path, force_checkpoint=False)
    assert manifest.sha256, "safe_export must return a sha256"
    assert manifest.page_count > 0, f"page_count={manifest.page_count}"
    assert manifest.schema_version > 0, f"schema_version={manifest.schema_version}"
    assert manifest.protocol_version == 1, f"protocol_version={manifest.protocol_version}"
    return ScenarioResult(name="safe_export", details={"export_path": str(export_path)})


def projection_rebuild(context: HarnessContext) -> ScenarioResult:
    """Validate rebuild and rebuild-missing repair FTS and VEC projections."""
    missing = context.engine.admin.rebuild_missing()
    assert missing.targets == [ProjectionTarget.FTS], f"unexpected targets={missing.targets}"

    fts = context.engine.admin.rebuild(target=ProjectionTarget.FTS)
    assert fts.targets == [ProjectionTarget.FTS], f"unexpected targets={fts.targets}"

    # rebuild(ALL) touches the vec_nodes_active virtual table, which only
    # exists when the engine was opened with a vector_dimension. Baseline
    # mode has no such profile, so only vector mode exercises the full path.
    if context.mode == "vector":
        all_targets = context.engine.admin.rebuild(target=ProjectionTarget.ALL)
        assert all_targets.targets == [ProjectionTarget.FTS, ProjectionTarget.VEC], (
            f"unexpected targets={all_targets.targets}"
        )

    assert_integrity_clean(context.engine.admin.check_integrity())
    assert_semantics_clean(context.engine.admin.check_semantics())
    return ScenarioResult(name="projection_rebuild")


def restore_vector_profiles(context: HarnessContext) -> ScenarioResult:
    """Validate restore_vector_profiles returns a well-formed repair report."""
    report = context.engine.admin.restore_vector_profiles()
    assert isinstance(report, ProjectionRepairReport), (
        f"expected ProjectionRepairReport, got {type(report).__name__}"
    )
    assert report.rebuilt_rows >= 0

    assert_integrity_clean(context.engine.admin.check_integrity())
    return ScenarioResult(name="restore_vector_profiles")
