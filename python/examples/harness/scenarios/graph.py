from __future__ import annotations

from fathomdb import EdgeInsert, EdgeRetire, NodeInsert, NodeRetire, TraverseDirection, WriteRequest, new_row_id

from ..engine_factory import open_engine
from ..models import (
    GRAPH_EDGE_ID,
    GRAPH_RETIRE_SOURCE,
    GRAPH_SOURCE,
    GRAPH_TASK_A_ID,
    GRAPH_TASK_B_ID,
    HarnessContext,
    RETIRE_CLEAN_CHILD_ID,
    RETIRE_CLEAN_EDGE_ID,
    RETIRE_CLEAN_PARENT_ID,
    RETIRE_CLEAN_SOURCE,
    RETIRE_DANGLING_CHILD_ID,
    RETIRE_DANGLING_EDGE_ID,
    RETIRE_DANGLING_PARENT_ID,
    RETIRE_DANGLING_SOURCE,
    ScenarioResult,
)
from ..verify import assert_no_nodes, assert_semantics_clean


def graph_edge_traversal(context: HarnessContext) -> ScenarioResult:
    context.engine.write(
        WriteRequest(
            label="graph-edge-traversal",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=GRAPH_TASK_A_ID,
                    kind="Task",
                    properties={"title": "Prepare board packet"},
                    source_ref=GRAPH_SOURCE,
                    upsert=True,
                ),
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=GRAPH_TASK_B_ID,
                    kind="Task",
                    properties={"title": "Review budget appendix"},
                    source_ref=GRAPH_SOURCE,
                    upsert=True,
                ),
            ],
            edges=[
                EdgeInsert(
                    row_id=new_row_id(),
                    logical_id=GRAPH_EDGE_ID,
                    source_logical_id=GRAPH_TASK_A_ID,
                    target_logical_id=GRAPH_TASK_B_ID,
                    kind="DEPENDS_ON",
                    properties={},
                    source_ref=GRAPH_SOURCE,
                    upsert=True,
                )
            ],
        )
    )

    rows = (
        context.engine.nodes("Task")
        .filter_logical_id_eq(GRAPH_TASK_A_ID)
        .traverse(direction=TraverseDirection.OUT, label="DEPENDS_ON", max_depth=1)
        .limit(5)
        .execute()
    )
    returned_ids = {node.logical_id for node in rows.nodes}
    assert GRAPH_TASK_B_ID in returned_ids, f"expected traversal to include {GRAPH_TASK_B_ID}, got {returned_ids}"
    return ScenarioResult(name="graph_edge_traversal")


def edge_retire(context: HarnessContext) -> ScenarioResult:
    context.engine.write(
        WriteRequest(
            label="edge-retire",
            edge_retires=[EdgeRetire(logical_id=GRAPH_EDGE_ID, source_ref=GRAPH_RETIRE_SOURCE)],
        )
    )

    rows = (
        context.engine.nodes("Task")
        .filter_logical_id_eq(GRAPH_TASK_A_ID)
        .traverse(direction=TraverseDirection.OUT, label="DEPENDS_ON", max_depth=1)
        .limit(5)
        .execute()
    )
    returned_ids = {node.logical_id for node in rows.nodes}
    assert GRAPH_TASK_B_ID not in returned_ids, (
        f"retired edge should remove {GRAPH_TASK_B_ID} from traversal, got {returned_ids}"
    )
    assert_semantics_clean(context.engine.admin.check_semantics())
    return ScenarioResult(name="edge_retire")


def node_retire_clean(context: HarnessContext) -> ScenarioResult:
    engine = open_engine(
        context.sibling_db("node-retire-clean"),
        mode=context.mode,
        vector_dimension=context.vector_dimension,
    )

    engine.write(
        WriteRequest(
            label="node-retire-clean-seed",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=RETIRE_CLEAN_PARENT_ID,
                    kind="Task",
                    properties={"title": "Clean parent"},
                    source_ref=RETIRE_CLEAN_SOURCE,
                    upsert=True,
                ),
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=RETIRE_CLEAN_CHILD_ID,
                    kind="Task",
                    properties={"title": "Clean child"},
                    source_ref=RETIRE_CLEAN_SOURCE,
                    upsert=True,
                ),
            ],
            edges=[
                EdgeInsert(
                    row_id=new_row_id(),
                    logical_id=RETIRE_CLEAN_EDGE_ID,
                    source_logical_id=RETIRE_CLEAN_PARENT_ID,
                    target_logical_id=RETIRE_CLEAN_CHILD_ID,
                    kind="DEPENDS_ON",
                    properties={},
                    source_ref=RETIRE_CLEAN_SOURCE,
                    upsert=True,
                )
            ],
        )
    )
    engine.write(
        WriteRequest(
            label="node-retire-clean-retire",
            node_retires=[NodeRetire(logical_id=RETIRE_CLEAN_CHILD_ID, source_ref=RETIRE_CLEAN_SOURCE)],
            edge_retires=[EdgeRetire(logical_id=RETIRE_CLEAN_EDGE_ID, source_ref=RETIRE_CLEAN_SOURCE)],
        )
    )

    direct_rows = engine.nodes("Task").filter_logical_id_eq(RETIRE_CLEAN_CHILD_ID).execute()
    assert_no_nodes(direct_rows)
    traversal_rows = (
        engine.nodes("Task")
        .filter_logical_id_eq(RETIRE_CLEAN_PARENT_ID)
        .traverse(direction=TraverseDirection.OUT, label="DEPENDS_ON", max_depth=1)
        .limit(5)
        .execute()
    )
    traversal_ids = {node.logical_id for node in traversal_rows.nodes}
    assert RETIRE_CLEAN_CHILD_ID not in traversal_ids, (
        f"retired node should not remain traversable, got {traversal_ids}"
    )
    report = engine.admin.check_semantics()
    assert report.dangling_edges == 0, f"dangling_edges={report.dangling_edges}"
    return ScenarioResult(
        name="node_retire_clean",
        details={"orphaned_supersession_chains": report.orphaned_supersession_chains},
    )


def node_retire_dangling(context: HarnessContext) -> ScenarioResult:
    engine = open_engine(
        context.sibling_db("node-retire-dangling"),
        mode=context.mode,
        vector_dimension=context.vector_dimension,
    )

    engine.write(
        WriteRequest(
            label="node-retire-dangling-seed",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=RETIRE_DANGLING_PARENT_ID,
                    kind="Task",
                    properties={"title": "Dangling parent"},
                    source_ref=RETIRE_DANGLING_SOURCE,
                    upsert=True,
                ),
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=RETIRE_DANGLING_CHILD_ID,
                    kind="Task",
                    properties={"title": "Dangling child"},
                    source_ref=RETIRE_DANGLING_SOURCE,
                    upsert=True,
                ),
            ],
            edges=[
                EdgeInsert(
                    row_id=new_row_id(),
                    logical_id=RETIRE_DANGLING_EDGE_ID,
                    source_logical_id=RETIRE_DANGLING_PARENT_ID,
                    target_logical_id=RETIRE_DANGLING_CHILD_ID,
                    kind="DEPENDS_ON",
                    properties={},
                    source_ref=RETIRE_DANGLING_SOURCE,
                    upsert=True,
                )
            ],
        )
    )
    engine.write(
        WriteRequest(
            label="node-retire-dangling-retire-node",
            node_retires=[
                NodeRetire(logical_id=RETIRE_DANGLING_CHILD_ID, source_ref=RETIRE_DANGLING_SOURCE)
            ],
        )
    )

    direct_rows = engine.nodes("Task").filter_logical_id_eq(RETIRE_DANGLING_CHILD_ID).execute()
    assert_no_nodes(direct_rows)
    traversal_rows = (
        engine.nodes("Task")
        .filter_logical_id_eq(RETIRE_DANGLING_PARENT_ID)
        .traverse(direction=TraverseDirection.OUT, label="DEPENDS_ON", max_depth=1)
        .limit(5)
        .execute()
    )
    traversal_ids = {node.logical_id for node in traversal_rows.nodes}
    assert RETIRE_DANGLING_CHILD_ID not in traversal_ids, (
        f"retired dangling child should not remain traversable, got {traversal_ids}"
    )

    report = engine.admin.check_semantics()
    assert report.dangling_edges >= 1, f"expected dangling edge, got {report.dangling_edges}"
    return ScenarioResult(
        name="node_retire_dangling",
        details={"dangling_edges_detected": report.dangling_edges},
    )
