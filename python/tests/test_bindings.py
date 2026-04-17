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
    from fathomdb import (
        ChunkInsert,
        ChunkPolicy,
        Engine,
        NodeInsert,
        WriteRequest,
        new_row_id,
    )

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
        .execute()
    )

    assert rows.was_degraded is False
    assert len(rows.hits) == 1
    assert rows.hits[0].node.logical_id == "meeting:budget-2026-03-25"
    assert rows.hits[0].node.properties["title"] == "Budget review"


def test_external_content_roundtrip(tmp_path: Path) -> None:
    from fathomdb import (
        ChunkInsert,
        ChunkPolicy,
        Engine,
        NodeInsert,
        WriteRequest,
        new_row_id,
    )

    db = Engine.open(tmp_path / "agent.db")

    db.write(
        WriteRequest(
            label="ext-content-test",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="doc:ext-report",
                    kind="Document",
                    properties={"title": "Annual Report"},
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                    content_ref="s3://docs/annual-report.pdf",
                ),
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="doc:plain-note",
                    kind="Document",
                    properties={"title": "Meeting Notes"},
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                ),
            ],
            chunks=[
                ChunkInsert(
                    id="chunk:ext-report:0",
                    node_logical_id="doc:ext-report",
                    text_content="revenue grew 15 percent",
                    content_hash="sha256:abc123",
                ),
                ChunkInsert(
                    id="chunk:plain-note:0",
                    node_logical_id="doc:plain-note",
                    text_content="discussed project timelines",
                ),
            ],
        )
    )

    # content_ref surfaces on NodeRow
    rows = db.nodes("Document").filter_logical_id_eq("doc:ext-report").execute()
    assert len(rows.nodes) == 1
    assert rows.nodes[0].content_ref == "s3://docs/annual-report.pdf"

    # Nodes without content_ref return None
    rows = db.nodes("Document").filter_logical_id_eq("doc:plain-note").execute()
    assert len(rows.nodes) == 1
    assert rows.nodes[0].content_ref is None

    # filter_content_ref_not_null returns only content nodes
    rows = db.nodes("Document").filter_content_ref_not_null().limit(10).execute()
    assert len(rows.nodes) == 1
    assert rows.nodes[0].logical_id == "doc:ext-report"

    # filter_content_ref_eq matches exact URI
    rows = (
        db.nodes("Document")
        .filter_content_ref_eq("s3://docs/annual-report.pdf")
        .limit(10)
        .execute()
    )
    assert len(rows.nodes) == 1
    assert rows.nodes[0].logical_id == "doc:ext-report"

    db.close()


def test_trace_and_excise_source(tmp_path: Path) -> None:
    from fathomdb import (
        ChunkInsert,
        ChunkPolicy,
        Engine,
        NodeInsert,
        WriteRequest,
        new_row_id,
    )

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
    assert rows.hits == ()


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


def test_write_request_builder_optional_backfill_preserves_existing_raw_json_string_payload() -> (
    None
):
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


def test_public_python_admin_client_exposes_restore_vector_profiles(
    tmp_path: Path,
) -> None:
    from fathomdb import Engine

    db = Engine.open(tmp_path / "agent.db")

    report = db.admin.restore_vector_profiles()
    assert isinstance(report.targets, list)
    assert report.rebuilt_rows == 0
    assert isinstance(report.notes, list)


def test_public_python_admin_client_exposes_vector_regeneration_types() -> None:
    from fathomdb import VectorRegenerationConfig

    config = VectorRegenerationConfig(
        kind="Document",
        profile="default",
        chunking_policy="sentence",
        preprocessing_policy="strip_html",
    )

    wire = config.to_wire()
    assert wire["kind"] == "Document"
    assert wire["profile"] == "default"
    assert wire["chunking_policy"] == "sentence"
    assert wire["preprocessing_policy"] == "strip_html"
    # 0.4.0: the identity fields are gone from the Python wrapper.
    assert "model_identity" not in wire
    assert "model_version" not in wire
    assert "dimension" not in wire
    assert "normalization_policy" not in wire
    assert "generator_command" not in wire
    # 0.5.0: table_name is replaced by kind.
    assert "table_name" not in wire


def test_vector_regeneration_config_rejects_legacy_fields_at_construction() -> None:
    from fathomdb import VectorRegenerationConfig

    import pytest

    with pytest.raises(TypeError):
        VectorRegenerationConfig(  # type: ignore[call-arg]
            kind="Document",
            profile="default",
            table_name="vec_chunks",
            chunking_policy="sentence",
            preprocessing_policy="strip_html",
        )


def test_regenerate_vector_embeddings_errors_when_engine_has_no_embedder(
    tmp_path: Path,
) -> None:
    """``Engine.open`` with no embedder must reject regen with a clear error."""

    import pytest

    from fathomdb import Engine, FathomError, VectorRegenerationConfig

    db = Engine.open(tmp_path / "agent.db")
    config = VectorRegenerationConfig(
        kind="Document",
        profile="default",
        chunking_policy="per_chunk",
        preprocessing_policy="trim",
    )
    with pytest.raises(FathomError, match="embedder not configured"):
        db.admin.regenerate_vector_embeddings(config)


def test_vector_query_degrades_when_vector_table_absent(tmp_path: Path) -> None:
    from fathomdb import Engine

    db = Engine.open(tmp_path / "agent.db")
    rows = db.nodes("Meeting").vector_search("budget", limit=3).execute()
    assert rows.was_degraded is True


def test_public_python_admin_client_exposes_operational_collection_lifecycle(
    tmp_path: Path,
) -> None:
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


def test_public_python_admin_client_exposes_fts_property_schema_lifecycle(
    tmp_path: Path,
) -> None:
    from fathomdb import Engine, FtsPropertySchemaRecord

    db = Engine.open(tmp_path / "agent.db")

    # Register
    record = db.admin.register_fts_property_schema("Goal", ["$.name", "$.description"])
    assert isinstance(record, FtsPropertySchemaRecord)
    assert record.kind == "Goal"
    assert record.property_paths == ("$.name", "$.description")
    assert record.separator == " "
    assert record.format_version == 1
    # Pack P7.7-fix: scalar-only schemas now also expose the per-entry
    # view with every entry marked Scalar, and an empty exclude_paths.
    from fathomdb import FtsPropertyPathMode

    assert len(record.entries) == 2
    assert all(entry.mode == FtsPropertyPathMode.SCALAR for entry in record.entries)
    assert [entry.path for entry in record.entries] == ["$.name", "$.description"]
    assert record.exclude_paths == ()

    # Describe
    described = db.admin.describe_fts_property_schema("Goal")
    assert described is not None
    assert described.kind == "Goal"
    assert described.property_paths == ("$.name", "$.description")
    assert len(described.entries) == 2
    assert all(entry.mode == FtsPropertyPathMode.SCALAR for entry in described.entries)

    # Describe missing
    missing = db.admin.describe_fts_property_schema("NoSuchKind")
    assert missing is None

    # List
    schemas = db.admin.list_fts_property_schemas()
    assert len(schemas) == 1
    assert schemas[0].kind == "Goal"

    # Update (idempotent upsert)
    updated = db.admin.register_fts_property_schema(
        "Goal", ["$.name", "$.notes"], separator="\n"
    )
    assert updated.property_paths == ("$.name", "$.notes")
    assert updated.separator == "\n"

    # Remove
    db.admin.remove_fts_property_schema("Goal")
    assert db.admin.describe_fts_property_schema("Goal") is None

    # Remove non-existent raises
    import pytest

    with pytest.raises(Exception):
        db.admin.remove_fts_property_schema("Goal")


def test_public_python_admin_client_round_trips_recursive_schema_entries(
    tmp_path: Path,
) -> None:
    """Pack P7.7-fix regression: describe/list must round-trip the
    per-entry schema (including recursive mode) for recursive schemas.

    Before the fix, the engine's load path unconditionally tried to
    deserialize the stored JSON as ``Vec<String>``, which silently fails
    for recursive-bearing schemas (stored as object-shaped JSON) and
    returned an empty ``property_paths`` — and no mode information at
    all.
    """
    from fathomdb import (
        Engine,
        FtsPropertyPathMode,
        FtsPropertyPathSpec,
    )

    db = Engine.open(tmp_path / "recursive.db")

    entries = [
        FtsPropertyPathSpec(path="$.title", mode=FtsPropertyPathMode.SCALAR),
        FtsPropertyPathSpec(path="$.payload", mode=FtsPropertyPathMode.RECURSIVE),
    ]
    registered = db.admin.register_fts_property_schema_with_entries(
        "KnowledgeItem",
        entries,
        exclude_paths=["$.payload.secret"],
    )
    assert registered.kind == "KnowledgeItem"
    assert registered.property_paths == ("$.title", "$.payload")
    assert registered.entries == tuple(entries)
    assert registered.exclude_paths == ("$.payload.secret",)

    described = db.admin.describe_fts_property_schema("KnowledgeItem")
    assert described is not None
    assert described.entries == tuple(entries)
    assert described.property_paths == ("$.title", "$.payload")
    assert described.exclude_paths == ("$.payload.secret",)
    assert described.entries[1].mode == FtsPropertyPathMode.RECURSIVE

    listed = db.admin.list_fts_property_schemas()
    assert len(listed) == 1
    assert listed[0].entries == tuple(entries)
    assert listed[0].exclude_paths == ("$.payload.secret",)


def test_public_python_admin_client_reads_operational_rows_by_declared_fields(
    tmp_path: Path,
) -> None:
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


def test_public_python_admin_client_can_update_operational_filter_contract(
    tmp_path: Path,
) -> None:
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


def test_public_python_admin_client_manages_secondary_indexes_and_retention(
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
    audit_item = next(
        item for item in plan.items if item.collection_name == "audit_log"
    )
    assert audit_item.action_kind.value == "keep_last"
    assert audit_item.candidate_deletions == 1

    dry_run = db.admin.run_operational_retention(
        1_000, max_collections=10, dry_run=True
    )
    audit_run = next(
        item for item in dry_run.items if item.collection_name == "audit_log"
    )
    assert audit_run.deleted_mutations == 1


def test_vector_write_and_search_round_trip(tmp_path: Path) -> None:
    from fathomdb import (
        ChunkInsert,
        ChunkPolicy,
        Engine,
        NodeInsert,
        ProjectionTarget,
        VecInsert,
        WriteRequest,
        new_row_id,
    )

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
    from fathomdb import (
        ChunkInsert,
        ChunkPolicy,
        EdgeInsert,
        Engine,
        NodeInsert,
        TraverseDirection,
        WriteRequest,
        new_row_id,
    )

    db = Engine.open(tmp_path / "agent.db")

    db.write(
        WriteRequest(
            label="grouped-query",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="meeting-1",
                    kind="Meeting",
                    properties={
                        "title": "Budget review",
                        "priority": 9,
                        "updated_at": 1711843200,
                    },
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
        .filter_logical_id_eq("meeting-1")
        .expand(
            slot="tasks", direction=TraverseDirection.OUT, label="HAS_TASK", max_depth=1
        )
        .expand(
            slot="decisions",
            direction=TraverseDirection.OUT,
            label="HAS_DECISION",
            max_depth=1,
        )
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
    from fathomdb import (
        ChunkInsert,
        ChunkPolicy,
        EdgeInsert,
        Engine,
        NodeInsert,
        TraverseDirection,
        WriteRequest,
        new_row_id,
    )

    db = Engine.open(tmp_path / "agent.db")

    db.write(
        WriteRequest(
            label="grouped-filters",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="meeting-1",
                    kind="Meeting",
                    properties={
                        "title": "Budget review",
                        "priority": 9,
                        "updated_at": 1711843200,
                    },
                    source_ref="source:meeting-1",
                    upsert=False,
                    chunk_policy=ChunkPolicy.REPLACE,
                ),
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="meeting-2",
                    kind="Meeting",
                    properties={
                        "title": "Backlog grooming",
                        "priority": 2,
                        "updated_at": 1700000000,
                    },
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
        .expand(
            slot="tasks", direction=TraverseDirection.OUT, label="HAS_TASK", max_depth=1
        )
        .execute_grouped()
    )

    assert len(grouped.roots) == 1
    assert grouped.roots[0].logical_id == "meeting-1"
    assert grouped.expansions[0].roots[0].nodes[0].logical_id == "task-1"


def test_write_request_builder_builds_full_bundle_without_manual_cross_reference_threading() -> (
    None
):
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


def test_python_write_request_exposes_operational_writes_round_trip(
    tmp_path: Path,
) -> None:
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


def test_concurrent_reads_from_multiple_threads(tmp_path: Path) -> None:
    """Issue #30: Engine must be usable from HTTP handler threads.

    Seeds data on the main thread, then reads concurrently from spawned
    threads — the exact pattern that panicked with the old `unsendable`
    marker.
    """
    import threading

    from fathomdb import ChunkPolicy, Engine, NodeInsert, WriteRequest, new_row_id

    db = Engine.open(tmp_path / "agent.db")

    db.write(
        WriteRequest(
            label="seed",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="t:1",
                    kind="Test",
                    properties={"value": "hello"},
                    source_ref="test",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                )
            ],
        )
    )

    errors: list[Exception] = []

    def worker() -> None:
        try:
            rows = db.nodes("Test").limit(10).execute()
            assert len(rows.nodes) == 1
        except Exception as exc:
            errors.append(exc)

    threads = [threading.Thread(target=worker) for _ in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    assert errors == [], f"worker threads failed: {errors}"


def test_close_is_idempotent(tmp_path: Path) -> None:
    """Calling close() twice must not raise."""
    from fathomdb import Engine

    db = Engine.open(tmp_path / "agent.db")
    db.close()
    db.close()


def test_operations_after_close_raise(tmp_path: Path) -> None:
    """Any engine operation after close() must raise FathomError."""
    import pytest

    from fathomdb import Engine, FathomError

    db = Engine.open(tmp_path / "agent.db")
    db.close()
    with pytest.raises(FathomError, match="engine is closed"):
        db.nodes("Test").limit(10).execute()


def test_context_manager_closes_on_exit(tmp_path: Path) -> None:
    """The with-block must close the engine on normal exit."""
    import pytest

    from fathomdb import (
        ChunkPolicy,
        Engine,
        FathomError,
        NodeInsert,
        WriteRequest,
        new_row_id,
    )

    with Engine.open(tmp_path / "agent.db") as db:
        db.write(
            WriteRequest(
                label="seed",
                nodes=[
                    NodeInsert(
                        row_id=new_row_id(),
                        logical_id="t:1",
                        kind="Test",
                        properties={"value": "hello"},
                        source_ref="test",
                        upsert=False,
                        chunk_policy=ChunkPolicy.PRESERVE,
                    )
                ],
            )
        )

    with pytest.raises(FathomError, match="engine is closed"):
        db.nodes("Test").limit(10).execute()


def test_context_manager_closes_on_exception(tmp_path: Path) -> None:
    """The with-block must close the engine even when an exception is raised."""
    import pytest

    from fathomdb import Engine, FathomError

    try:
        with Engine.open(tmp_path / "agent.db") as db:
            raise RuntimeError("deliberate test error")
    except RuntimeError:
        pass

    with pytest.raises(FathomError, match="engine is closed"):
        db.nodes("Test").limit(10).execute()


def test_second_open_raises_database_locked(tmp_path: Path) -> None:
    """Opening the same database twice must raise DatabaseLockedError."""
    import os

    import pytest

    from fathomdb import DatabaseLockedError, Engine

    db = Engine.open(tmp_path / "agent.db")
    with pytest.raises(DatabaseLockedError, match="already in use"):
        Engine.open(tmp_path / "agent.db")

    # Verify error includes holding PID (Unix only; Windows file locks
    # prevent reading the PID from the lock file).
    if os.name != "nt":
        try:
            Engine.open(tmp_path / "agent.db")
        except DatabaseLockedError as exc:
            assert str(os.getpid()) in str(exc), f"error must contain pid: {exc}"

    # First engine should still be functional.
    rows = db.nodes("Test").limit(10).execute()
    assert rows.nodes == []
    db.close()


def test_reopen_after_close_succeeds(tmp_path: Path) -> None:
    """After close(), re-opening the same database must succeed."""
    from fathomdb import Engine

    db = Engine.open(tmp_path / "agent.db")
    db.close()

    db2 = Engine.open(tmp_path / "agent.db")
    rows = db2.nodes("Test").limit(10).execute()
    assert rows.nodes == []
    db2.close()


def test_telemetry_snapshot_returns_typed_dataclass(tmp_path: Path) -> None:
    """telemetry_snapshot() must return a TelemetrySnapshot, not a raw dict."""
    from fathomdb import Engine, TelemetrySnapshot

    db = Engine.open(tmp_path / "agent.db")
    snap = db.telemetry_snapshot()

    assert isinstance(snap, TelemetrySnapshot)
    assert snap.queries_total == 0
    assert snap.writes_total == 0
    assert snap.errors_total == 0
    assert snap.admin_ops_total == 0
    # Cache counters are non-negative (bootstrap may cause some activity)
    assert snap.cache_hits >= 0
    assert snap.cache_misses >= 0


def test_telemetry_counters_increment_after_operations(tmp_path: Path) -> None:
    """Counters must reflect actual engine operations."""
    from fathomdb import (
        ChunkInsert,
        ChunkPolicy,
        Engine,
        NodeInsert,
        WriteRequest,
        new_row_id,
    )

    db = Engine.open(tmp_path / "agent.db")

    before = db.telemetry_snapshot()
    assert before.queries_total == 0
    assert before.writes_total == 0

    db.write(
        WriteRequest(
            label="telemetry-test",
            nodes=[
                NodeInsert(
                    row_id=new_row_id(),
                    logical_id="t:1",
                    kind="Test",
                    properties={"v": 1},
                    source_ref="test",
                    upsert=False,
                    chunk_policy=ChunkPolicy.PRESERVE,
                )
            ],
            chunks=[
                ChunkInsert(
                    id="c1",
                    node_logical_id="t:1",
                    text_content="telemetry test content",
                )
            ],
        )
    )

    after_write = db.telemetry_snapshot()
    assert after_write.writes_total >= 1
    assert after_write.write_rows_total >= 2  # 1 node + 1 chunk

    db.nodes("Test").limit(10).execute()
    db.nodes("Test").limit(10).execute()

    after_query = db.telemetry_snapshot()
    assert after_query.queries_total >= 2

    total = after_query.cache_hits + after_query.cache_misses
    assert total > 0, "expected cache activity after queries"


def test_engine_open_accepts_all_telemetry_levels(tmp_path: Path) -> None:
    """Engine.open must accept every TelemetryLevel variant without error.

    This catches signature misalignment between the Python wrapper and
    the native EngineCore — if the compiled .so doesn't accept the
    telemetry_level parameter, this test fails immediately.
    """
    from fathomdb import Engine, TelemetryLevel

    for level in TelemetryLevel:
        db_path = tmp_path / f"agent-{level.value}.db"
        db = Engine.open(db_path, telemetry_level=level)
        snap = db.telemetry_snapshot()
        assert snap.queries_total == 0
        db.close()

    # Also test string values (the other accepted form)
    for level_str in ("counters", "statements", "profiling"):
        db_path = tmp_path / f"agent-str-{level_str}.db"
        db = Engine.open(db_path, telemetry_level=level_str)
        snap = db.telemetry_snapshot()
        assert snap.queries_total == 0
        db.close()


def test_engine_open_rejects_invalid_telemetry_level(tmp_path: Path) -> None:
    """Invalid telemetry_level values must raise ValueError."""
    from fathomdb import Engine

    with pytest.raises(ValueError, match="invalid telemetry_level"):
        Engine.open(tmp_path / "agent.db", telemetry_level="turbo")
