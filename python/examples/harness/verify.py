from __future__ import annotations

from fathomdb import IntegrityReport, QueryRows, SemanticReport, TraceReport


def assert_single_node(rows: QueryRows, logical_id: str) -> None:
    assert rows.was_degraded is False, "query unexpectedly degraded"
    assert len(rows.nodes) == 1, f"expected exactly one node, got {len(rows.nodes)}"
    assert rows.nodes[0].logical_id == logical_id, (
        f"expected logical_id={logical_id}, got {rows.nodes[0].logical_id!r}"
    )


def assert_no_nodes(rows: QueryRows) -> None:
    assert rows.was_degraded is False, "query unexpectedly degraded"
    assert rows.nodes == [], f"expected no nodes, got {[node.logical_id for node in rows.nodes]}"


def assert_integrity_clean(report: IntegrityReport) -> None:
    assert report.physical_ok is True, f"physical integrity failed: {report.warnings}"
    assert report.foreign_keys_ok is True, f"foreign keys failed: {report.warnings}"
    assert report.missing_fts_rows == 0, f"missing_fts_rows={report.missing_fts_rows}"
    assert report.duplicate_active_logical_ids == 0, (
        f"duplicate_active_logical_ids={report.duplicate_active_logical_ids}"
    )


def assert_semantics_clean(report: SemanticReport) -> None:
    assert report.orphaned_chunks == 0, f"orphaned_chunks={report.orphaned_chunks}"
    assert report.null_source_ref_nodes == 0, (
        f"null_source_ref_nodes={report.null_source_ref_nodes}"
    )
    assert report.broken_step_fk == 0, f"broken_step_fk={report.broken_step_fk}"
    assert report.broken_action_fk == 0, f"broken_action_fk={report.broken_action_fk}"
    assert report.stale_fts_rows == 0, f"stale_fts_rows={report.stale_fts_rows}"
    assert report.fts_rows_for_superseded_nodes == 0, (
        f"fts_rows_for_superseded_nodes={report.fts_rows_for_superseded_nodes}"
    )
    assert report.dangling_edges == 0, f"dangling_edges={report.dangling_edges}"
    assert report.orphaned_supersession_chains == 0, (
        f"orphaned_supersession_chains={report.orphaned_supersession_chains}"
    )
    assert report.stale_vec_rows == 0, f"stale_vec_rows={report.stale_vec_rows}"
    assert report.vec_rows_for_superseded_nodes == 0, (
        f"vec_rows_for_superseded_nodes={report.vec_rows_for_superseded_nodes}"
    )


def assert_trace(
    report: TraceReport,
    *,
    node_rows: int | None = None,
    edge_rows: int | None = None,
    action_rows: int | None = None,
    node_logical_ids: list[str] | None = None,
    action_ids: list[str] | None = None,
) -> None:
    if node_rows is not None:
        assert report.node_rows == node_rows, (
            f"expected node_rows={node_rows}, got {report.node_rows}"
        )
    if edge_rows is not None:
        assert report.edge_rows == edge_rows, (
            f"expected edge_rows={edge_rows}, got {report.edge_rows}"
        )
    if action_rows is not None:
        assert report.action_rows == action_rows, (
            f"expected action_rows={action_rows}, got {report.action_rows}"
        )
    if node_logical_ids is not None:
        assert report.node_logical_ids == node_logical_ids, (
            f"expected node_logical_ids={node_logical_ids}, got {report.node_logical_ids}"
        )
    if action_ids is not None:
        assert report.action_ids == action_ids, (
            f"expected action_ids={action_ids}, got {report.action_ids}"
        )
