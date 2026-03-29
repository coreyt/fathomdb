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
    assert report.operational_missing_collections == 0
    assert report.operational_missing_last_mutations == 0


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
    assert trace.operational_mutation_rows == 0
    assert trace.node_logical_ids == ["meeting:trace-test"]
    assert trace.operational_mutation_ids == []

    excised = db.admin.excise_source("action:trace-test")
    assert excised.source_ref == "action:trace-test"
    assert excised.node_rows == 1
    assert excised.operational_mutation_rows == 0

    rows = db.nodes("Meeting").text_search("traceable", limit=5).execute()
    assert rows.nodes == []


def test_invalid_json_path_raises_compile_error(tmp_path: Path) -> None:
    from fathomdb import CompileError, Engine

    db = Engine.open(tmp_path / "agent.db")

    with pytest.raises(CompileError):
        db.nodes("Meeting").filter_json_text_eq("status", "active").compile()


def test_optional_projection_task_preserves_existing_raw_json_string_payload() -> None:
    from fathomdb import OptionalProjectionTask, ProjectionTarget

    task = OptionalProjectionTask(
        target=ProjectionTarget.FTS,
        payload='{"reason":"phase2"}',
    )

    assert task.to_wire() == {
        "target": "fts",
        "payload": '{"reason":"phase2"}',
    }


def test_write_request_builder_optional_backfill_preserves_existing_raw_json_string_payload() -> None:
    from fathomdb import ProjectionTarget, WriteRequestBuilder

    builder = WriteRequestBuilder("backfill-json")
    builder.add_optional_backfill(
        ProjectionTarget.FTS,
        '{"reason":"phase2"}',
    )

    request = builder.build()

    assert request.to_wire()["optional_backfills"] == [
        {
            "target": "fts",
            "payload": '{"reason":"phase2"}',
        }
    ]


def test_vector_query_degrades_when_vector_table_absent(tmp_path: Path) -> None:
    from fathomdb import Engine

    db = Engine.open(tmp_path / "agent.db")
    rows = db.nodes("Meeting").vector_search("budget", limit=3).execute()
    assert rows.was_degraded is True


def test_public_python_admin_client_exposes_operational_collection_lifecycle(tmp_path: Path) -> None:
    from fathomdb import Engine, OperationalCollectionKind, OperationalRegisterRequest

    db = Engine.open(tmp_path / "agent.db")

    latest_state = db.admin.register_operational_collection(
        OperationalRegisterRequest(
            name="connector_health",
            kind=OperationalCollectionKind.LATEST_STATE,
            schema_json="{}",
            retention_json="{}",
            validation_json="",
            format_version=1,
        )
    )
    assert latest_state.name == "connector_health"
    assert latest_state.kind is OperationalCollectionKind.LATEST_STATE
    assert latest_state.disabled_at is None
    assert latest_state.validation_json == ""

    registered = db.admin.register_operational_collection(
        OperationalRegisterRequest(
            name="audit_log",
            kind=OperationalCollectionKind.APPEND_ONLY_LOG,
            schema_json="{}",
            retention_json='{"mode":"keep_last","max_rows":2}',
            validation_json="",
            format_version=1,
        )
    )
    assert registered.name == "audit_log"
    assert registered.kind is OperationalCollectionKind.APPEND_ONLY_LOG
    assert registered.disabled_at is None
    assert registered.validation_json == ""

    described = db.admin.describe_operational_collection("audit_log")
    assert described is not None
    assert described.name == "audit_log"

    traced = db.admin.trace_operational_collection("audit_log")
    assert traced.collection_name == "audit_log"
    assert traced.mutation_count == 0
    assert traced.current_count == 0

    rebuilt = db.admin.rebuild_operational_current("connector_health")
    assert rebuilt.collections_rebuilt == 1
    assert rebuilt.current_rows_rebuilt == 0

    compacted = db.admin.compact_operational_collection("audit_log", dry_run=True)
    assert compacted.collection_name == "audit_log"
    assert compacted.dry_run is True

    purged = db.admin.purge_operational_collection("audit_log", before_timestamp=250)
    assert purged.collection_name == "audit_log"
    assert purged.before_timestamp == 250

    disabled = db.admin.disable_operational_collection("connector_health")
    assert disabled.name == "connector_health"
    assert disabled.disabled_at is not None


def test_public_python_admin_client_reads_operational_rows_by_declared_fields(tmp_path: Path) -> None:
    from fathomdb import (
        Engine,
        OperationalAppend,
        OperationalCollectionKind,
        OperationalFilterClause,
        OperationalFilterValue,
        OperationalReadRequest,
        OperationalRegisterRequest,
        WriteRequest,
    )

    db = Engine.open(tmp_path / "agent.db")

    record = db.admin.register_operational_collection(
        OperationalRegisterRequest(
            name="audit_log",
            kind=OperationalCollectionKind.APPEND_ONLY_LOG,
            schema_json="{}",
            retention_json='{"mode":"keep_all"}',
            filter_fields_json='[{"name":"actor","type":"string","modes":["exact","prefix"]},{"name":"ts","type":"timestamp","modes":["range"]}]',
            validation_json="",
            format_version=1,
        )
    )
    assert record.filter_fields_json.startswith("[")

    db.write(
        WriteRequest(
            label="audit-log",
            operational_writes=[
                OperationalAppend(
                    collection="audit_log",
                    record_key="evt-1",
                    payload_json={"actor": "alice", "ts": 100},
                    source_ref="source:1",
                ),
                OperationalAppend(
                    collection="audit_log",
                    record_key="evt-2",
                    payload_json={"actor": "alice-admin", "ts": 200},
                    source_ref="source:2",
                ),
            ],
        )
    )

    report = db.admin.read_operational_collection(
        OperationalReadRequest(
            collection_name="audit_log",
            filters=[
                OperationalFilterClause.prefix("actor", "alice"),
                OperationalFilterClause.range("ts", lower=150, upper=250),
            ],
            limit=10,
        )
    )
    assert report.collection_name == "audit_log"
    assert report.row_count == 1
    assert report.was_limited is False
    assert [row.record_key for row in report.rows] == ["evt-2"]

    exact = db.admin.read_operational_collection(
        OperationalReadRequest(
            collection_name="audit_log",
            filters=[
                OperationalFilterClause.exact(
                    "actor", OperationalFilterValue.string("alice")
                )
            ],
            limit=10,
        )
    )
    assert exact.row_count == 1
    assert exact.rows[0].record_key == "evt-1"


def test_public_python_admin_client_can_update_operational_filter_contract(tmp_path: Path) -> None:
    from fathomdb import Engine, OperationalCollectionKind, OperationalRegisterRequest

    db = Engine.open(tmp_path / "agent.db")

    db.admin.register_operational_collection(
        OperationalRegisterRequest(
            name="audit_log",
            kind=OperationalCollectionKind.APPEND_ONLY_LOG,
            schema_json="{}",
            retention_json='{"mode":"keep_all"}',
            filter_fields_json="[]",
            validation_json="",
            format_version=1,
        )
    )

    updated = db.admin.update_operational_collection_filters(
        "audit_log",
        '[{"name":"actor","type":"string","modes":["exact"]}]',
    )

    assert updated.filter_fields_json.startswith("[")
    assert '"actor"' in updated.filter_fields_json


def test_public_python_admin_client_updates_and_validates_operational_validation_contract(
    tmp_path: Path,
) -> None:
    from fathomdb import (
        Engine,
        OperationalAppend,
        OperationalCollectionKind,
        OperationalRegisterRequest,
        WriteRequest,
    )

    db = Engine.open(tmp_path / "agent.db")

    db.admin.register_operational_collection(
        OperationalRegisterRequest(
            name="audit_log",
            kind=OperationalCollectionKind.APPEND_ONLY_LOG,
            schema_json="{}",
            retention_json='{"mode":"keep_all"}',
            validation_json="",
            format_version=1,
        )
    )

    validation_json = (
        '{"format_version":1,"mode":"disabled","additional_properties":false,'
        '"fields":[{"name":"status","type":"string","required":true,'
        '"enum":["ok","failed"]}]}'
    )
    updated = db.admin.update_operational_collection_validation(
        "audit_log", validation_json
    )
    assert updated.validation_json == validation_json

    db.write(
        WriteRequest(
            label="history-validation",
            operational_writes=[
                OperationalAppend(
                    collection="audit_log",
                    record_key="evt-1",
                    payload_json={"status": "ok"},
                    source_ref="source:1",
                ),
                OperationalAppend(
                    collection="audit_log",
                    record_key="evt-2",
                    payload_json={"status": "bogus"},
                    source_ref="source:2",
                ),
            ],
        )
    )

    report = db.admin.validate_operational_collection_history("audit_log")
    assert report.collection_name == "audit_log"
    assert report.checked_rows == 2
    assert report.invalid_row_count == 1
    assert report.issues[0].record_key == "evt-2"


def test_report_only_operational_validation_emits_write_warning(tmp_path: Path) -> None:
    from fathomdb import (
        Engine,
        OperationalCollectionKind,
        OperationalPut,
        OperationalRegisterRequest,
        WriteRequest,
    )

    db = Engine.open(tmp_path / "agent.db")
    db.admin.register_operational_collection(
        OperationalRegisterRequest(
            name="connector_health",
            kind=OperationalCollectionKind.LATEST_STATE,
            schema_json="{}",
            retention_json="{}",
            validation_json='{"format_version":1,"mode":"report_only","additional_properties":false,"fields":[{"name":"status","type":"string","required":true,"enum":["ok","failed"]}]}',
            format_version=1,
        )
    )

    receipt = db.write(
        WriteRequest(
            label="report-only",
            operational_writes=[
                OperationalPut(
                    collection="connector_health",
                    record_key="gmail",
                    payload_json={"status": "bogus"},
                    source_ref="source:1",
                )
            ],
        )
    )

    assert receipt.provenance_warnings == []
    assert len(receipt.warnings) == 1
    assert "connector_health" in receipt.warnings[0]


def test_public_python_admin_client_manages_secondary_indexes_and_retention(tmp_path: Path) -> None:
    from fathomdb import (
        Engine,
        OperationalAppend,
        OperationalCollectionKind,
        OperationalRegisterRequest,
        WriteRequest,
    )

    db = Engine.open(tmp_path / "agent.db")

    record = db.admin.register_operational_collection(
        OperationalRegisterRequest(
            name="audit_log",
            kind=OperationalCollectionKind.APPEND_ONLY_LOG,
            schema_json="{}",
            retention_json='{"mode":"keep_last","max_rows":2}',
            filter_fields_json='[{"name":"actor","type":"string","modes":["exact","prefix"]},{"name":"ts","type":"timestamp","modes":["range"]}]',
            validation_json="",
            secondary_indexes_json="[]",
            format_version=1,
        )
    )
    assert record.secondary_indexes_json == "[]"

    db.write(
        WriteRequest(
            label="secondary-index-seed",
            operational_writes=[
                OperationalAppend(
                    collection="audit_log",
                    record_key="evt-1",
                    payload_json={"actor": "alice", "ts": 100},
                    source_ref="source:1",
                ),
                OperationalAppend(
                    collection="audit_log",
                    record_key="evt-2",
                    payload_json={"actor": "alice-admin", "ts": 200},
                    source_ref="source:2",
                ),
                OperationalAppend(
                    collection="audit_log",
                    record_key="evt-3",
                    payload_json={"actor": "bob", "ts": 300},
                    source_ref="source:3",
                ),
            ],
        )
    )

    updated = db.admin.update_operational_collection_secondary_indexes(
        "audit_log",
        '[{"name":"actor_ts","kind":"append_only_field_time","field":"actor","value_type":"string","time_field":"ts"}]',
    )
    assert '"actor_ts"' in updated.secondary_indexes_json

    rebuild = db.admin.rebuild_operational_secondary_indexes("audit_log")
    assert rebuild.collection_name == "audit_log"
    assert rebuild.mutation_entries_rebuilt == 3

    plan = db.admin.plan_operational_retention(1_000, max_collections=10)
    assert plan.collections_examined >= 1
    audit_item = next(item for item in plan.items if item.collection_name == "audit_log")
    assert audit_item.action_kind.value == "keep_last"
    assert audit_item.candidate_deletions == 1

    dry_run = db.admin.run_operational_retention(1_000, max_collections=10, dry_run=True)
    audit_run = next(item for item in dry_run.items if item.collection_name == "audit_log")
    assert audit_run.deleted_mutations == 1


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
    assert semantics.missing_operational_current_rows == 0
    assert semantics.stale_operational_current_rows == 0
    assert semantics.disabled_collection_mutations == 0


def test_grouped_query_returns_root_plus_named_expansion_slots(tmp_path: Path) -> None:
    from fathomdb import ChunkInsert, ChunkPolicy, EdgeInsert, Engine, NodeInsert, TraverseDirection, WriteRequest, new_row_id

    db = Engine.open(tmp_path / "agent.db")

    db.write(
        WriteRequest(
            label="grouped-query",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="meeting-1",
                    kind="Meeting",
                    properties={"title": "Budget review", "priority": 9, "updated_at": 1711843200},
                    source_ref="source:meeting-1",
                    upsert=False,
                    chunk_policy=ChunkPolicy.REPLACE,
                ),
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="task-1",
                    kind="Task",
                    properties={"title": "Draft memo"},
                    source_ref="source:task-1",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="decision-1",
                    kind="Decision",
                    properties={"title": "Approve budget"},
                    source_ref="source:decision-1",
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
                    source_ref="source:edge-1",
                    upsert=False,
                ),
                EdgeInsert(
                    row_id=new_row_id(),
                    logical_id="edge-2",
                    source_logical_id="meeting-1",
                    target_logical_id="decision-1",
                    kind="HAS_DECISION",
                    properties={},
                    source_ref="source:edge-2",
                    upsert=False,
                ),
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

    grouped = (
        db.nodes("Meeting")
        .text_search("budget", limit=5)
        .expand(slot="tasks", direction=TraverseDirection.OUT, label="HAS_TASK", max_depth=1)
        .expand(slot="decisions", direction=TraverseDirection.OUT, label="HAS_DECISION", max_depth=1)
        .execute_grouped()
    )

    assert grouped.was_degraded is False
    assert len(grouped.roots) == 1
    assert grouped.roots[0].logical_id == "meeting-1"
    assert [slot.slot for slot in grouped.expansions] == ["tasks", "decisions"]
    assert grouped.expansions[0].roots[0].root_logical_id == "meeting-1"
    assert grouped.expansions[0].roots[0].nodes[0].logical_id == "task-1"
    assert grouped.expansions[1].roots[0].nodes[0].logical_id == "decision-1"


def test_grouped_query_supports_numeric_and_timestamp_filters(tmp_path: Path) -> None:
    from fathomdb import ChunkInsert, ChunkPolicy, EdgeInsert, Engine, NodeInsert, TraverseDirection, WriteRequest, new_row_id

    db = Engine.open(tmp_path / "agent.db")

    db.write(
        WriteRequest(
            label="grouped-filters",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="meeting-1",
                    kind="Meeting",
                    properties={"title": "Budget review", "priority": 9, "updated_at": 1711843200},
                    source_ref="source:meeting-1",
                    upsert=False,
                    chunk_policy=ChunkPolicy.REPLACE,
                ),
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="meeting-2",
                    kind="Meeting",
                    properties={"title": "Backlog grooming", "priority": 2, "updated_at": 1700000000},
                    source_ref="source:meeting-2",
                    upsert=False,
                    chunk_policy=ChunkPolicy.REPLACE,
                ),
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="task-1",
                    kind="Task",
                    properties={"title": "Draft memo"},
                    source_ref="source:task-1",
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
                    source_ref="source:edge-1",
                    upsert=False,
                )
            ],
            chunks=[
                ChunkInsert(
                    id="chunk-meeting-1",
                    node_logical_id="meeting-1",
                    text_content="budget review agenda",
                ),
                ChunkInsert(
                    id="chunk-meeting-2",
                    node_logical_id="meeting-2",
                    text_content="backlog grooming notes",
                ),
            ],
        )
    )

    grouped = (
        db.nodes("Meeting")
        .filter_json_integer_gte("$.priority", 5)
        .filter_json_timestamp_gte("$.updated_at", 1710000000)
        .expand(slot="tasks", direction=TraverseDirection.OUT, label="HAS_TASK", max_depth=1)
        .execute_grouped()
    )

    assert len(grouped.roots) == 1
    assert grouped.roots[0].logical_id == "meeting-1"
    assert grouped.expansions[0].roots[0].nodes[0].logical_id == "task-1"


def test_write_request_builder_builds_full_bundle_without_manual_cross_reference_threading() -> None:
    from fathomdb import ChunkPolicy, ProjectionTarget, WriteRequestBuilder

    builder = WriteRequestBuilder("memex-bundle")
    meeting = builder.add_node(
        row_id="row-meeting",
        logical_id="meeting-1",
        kind="Meeting",
        properties={"title": "Budget review"},
        source_ref="source:meeting",
        upsert=True,
        chunk_policy=ChunkPolicy.REPLACE,
    )
    task = builder.add_node(
        row_id="row-task",
        logical_id="task-1",
        kind="Task",
        properties={"title": "Draft memo"},
        source_ref="source:task",
        upsert=True,
        chunk_policy=ChunkPolicy.PRESERVE,
    )
    builder.add_edge(
        row_id="row-edge",
        logical_id="edge-1",
        source=meeting,
        target=task,
        kind="HAS_TASK",
        properties={},
        source_ref="source:edge",
        upsert=True,
    )
    chunk = builder.add_chunk(
        id="chunk-1",
        node=meeting,
        text_content="budget discussion",
    )
    run = builder.add_run(
        id="run-1",
        kind="session",
        status="completed",
        properties={},
        source_ref="source:run",
    )
    step = builder.add_step(
        id="step-1",
        run=run,
        kind="llm",
        status="completed",
        properties={},
        source_ref="source:step",
    )
    builder.add_action(
        id="action-1",
        step=step,
        kind="emit",
        status="completed",
        properties={},
        source_ref="source:action",
    )
    builder.add_vec_insert(chunk=chunk, embedding=[0.1, 0.2, 0.3, 0.4])
    builder.add_optional_backfill(ProjectionTarget.FTS, {"reason": "phase2"})
    builder.add_operational_put(
        collection="connector_health",
        record_key="gmail",
        payload_json={"status": "ok"},
        source_ref="source:ops",
    )

    request = builder.build()

    assert request.label == "memex-bundle"
    assert len(request.nodes) == 2
    assert len(request.edges) == 1
    assert len(request.chunks) == 1
    assert len(request.runs) == 1
    assert len(request.steps) == 1
    assert len(request.actions) == 1
    assert len(request.vec_inserts) == 1
    assert len(request.optional_backfills) == 1
    assert len(request.operational_writes) == 1
    assert request.edges[0].source_logical_id == meeting.logical_id
    assert request.edges[0].target_logical_id == task.logical_id
    assert request.chunks[0].node_logical_id == meeting.logical_id
    assert request.steps[0].run_id == run.id
    assert request.actions[0].step_id == step.id
    assert request.vec_inserts[0].chunk_id == chunk.id
    assert request.operational_writes[0].collection == "connector_health"
    assert request.operational_writes[0].record_key == "gmail"
    assert request.operational_writes[0].payload_json == {"status": "ok"}


def test_write_request_builder_rejects_foreign_handles_before_submit() -> None:
    from fathomdb import BuilderValidationError, ChunkPolicy, WriteRequestBuilder

    first = WriteRequestBuilder("first")
    foreign = first.add_node(
        row_id="row-a",
        logical_id="node-a",
        kind="Document",
        properties={},
        source_ref="source:a",
        chunk_policy=ChunkPolicy.PRESERVE,
    )

    second = WriteRequestBuilder("second")
    second.add_chunk(id="chunk-b", node=foreign, text_content="foreign")

    with pytest.raises(BuilderValidationError):
        second.build()


def test_python_write_request_exposes_operational_writes_round_trip(tmp_path: Path) -> None:
    from fathomdb import OperationalPut, WriteRequest

    request = WriteRequest(
        label="operational-only",
        operational_writes=[
            OperationalPut(
                collection="connector_health",
                record_key="gmail",
                payload_json={"status": "ok"},
                source_ref="source:ops",
            )
        ],
    )

    wire = request.to_wire()

    assert wire["label"] == "operational-only"
    assert wire["operational_writes"] == [
        {
            "type": "put",
            "collection": "connector_health",
            "record_key": "gmail",
            "payload_json": '{"status": "ok"}',
            "source_ref": "source:ops",
        }
    ]
