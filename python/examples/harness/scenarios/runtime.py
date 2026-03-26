from __future__ import annotations

from fathomdb import (
    Engine,
    InvalidWriteError,
    NodeInsert,
    ProvenanceMode,
    RunInsert,
    StepInsert,
    ActionInsert,
    WriteRequest,
    new_row_id,
)

from ..models import (
    HarnessContext,
    RUNTIME_ACTION_ID,
    RUNTIME_ANCHOR_NODE_ID,
    RUNTIME_RUN_ID,
    RUNTIME_SOURCE,
    RUNTIME_STEP_ID,
    ScenarioResult,
)
from ..verify import assert_semantics_clean, assert_single_node, assert_trace


def runtime_tables(context: HarnessContext) -> ScenarioResult:
    context.engine.write(
        WriteRequest(
            label="runtime-tables",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id=RUNTIME_ANCHOR_NODE_ID,
                    kind="Document",
                    properties={"title": "Planner output", "status": "ready"},
                    source_ref=RUNTIME_SOURCE,
                    upsert=True,
                )
            ],
            runs=[
                RunInsert(
                    id=RUNTIME_RUN_ID,
                    kind="PlannerRun",
                    status="succeeded",
                    properties={"agent": "planner", "revision": 1},
                    source_ref=RUNTIME_SOURCE,
                    upsert=False,
                )
            ],
            steps=[
                StepInsert(
                    id=RUNTIME_STEP_ID,
                    run_id=RUNTIME_RUN_ID,
                    kind="ToolStep",
                    status="succeeded",
                    properties={"name": "fetch-calendar"},
                    source_ref=RUNTIME_SOURCE,
                    upsert=False,
                )
            ],
            actions=[
                ActionInsert(
                    id=RUNTIME_ACTION_ID,
                    step_id=RUNTIME_STEP_ID,
                    kind="ToolCall",
                    status="succeeded",
                    properties={"tool": "calendar.lookup"},
                    source_ref=RUNTIME_SOURCE,
                    upsert=False,
                )
            ],
        )
    )

    trace = context.engine.admin.trace_source(RUNTIME_SOURCE)
    assert_trace(
        trace,
        node_rows=1,
        edge_rows=0,
        action_rows=1,
        node_logical_ids=[RUNTIME_ANCHOR_NODE_ID],
        action_ids=[RUNTIME_ACTION_ID],
    )

    node_rows = context.engine.nodes("Document").filter_logical_id_eq(RUNTIME_ANCHOR_NODE_ID).execute()
    assert_single_node(node_rows, RUNTIME_ANCHOR_NODE_ID)

    report = context.engine.admin.check_semantics()
    assert report.broken_step_fk == 0, f"broken_step_fk={report.broken_step_fk}"
    assert report.broken_action_fk == 0, f"broken_action_fk={report.broken_action_fk}"
    assert_semantics_clean(report)
    return ScenarioResult(name="runtime_tables")


def provenance_warn_require(context: HarnessContext) -> ScenarioResult:
    warn_db = Engine.open(context.sibling_db("provenance-warn"), provenance_mode=ProvenanceMode.WARN)
    warn_receipt = warn_db.write(
        WriteRequest(
            label="provenance-warn",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="document:provenance-warn",
                    kind="Document",
                    properties={"title": "Missing provenance should warn"},
                    source_ref=None,
                    upsert=True,
                )
            ],
        )
    )
    assert warn_receipt.provenance_warnings, "warn mode should emit provenance warnings"

    require_db = Engine.open(
        context.sibling_db("provenance-require"),
        provenance_mode=ProvenanceMode.REQUIRE,
    )
    try:
        require_db.write(
            WriteRequest(
                label="provenance-require",
                nodes=[
                    NodeInsert(
                        row_id=new_row_id(),
                        logical_id="document:provenance-require",
                        kind="Document",
                        properties={"title": "Missing provenance should fail"},
                        source_ref=None,
                        upsert=True,
                    )
                ],
            )
        )
    except InvalidWriteError:
        return ScenarioResult(
            name="provenance_warn_require",
            details={"warn_warnings": len(warn_receipt.provenance_warnings)},
        )
    raise AssertionError("require mode should reject writes without source_ref")
