from __future__ import annotations

import json
from dataclasses import dataclass, field, fields
from enum import Enum
from typing import Any


class ProvenanceMode(str, Enum):
    """Provenance enforcement level for write operations."""

    WARN = "warn"
    REQUIRE = "require"


class ChunkPolicy(str, Enum):
    """Policy for handling existing chunks when upserting a node."""

    PRESERVE = "preserve"
    REPLACE = "replace"


class ProjectionTarget(str, Enum):
    """Which projection indexes to target in rebuild operations."""

    FTS = "fts"
    VEC = "vec"
    ALL = "all"


class OperationalCollectionKind(str, Enum):
    """Storage model for an operational collection."""

    APPEND_ONLY_LOG = "append_only_log"
    LATEST_STATE = "latest_state"


class OperationalFilterMode(str, Enum):
    """Match mode for an operational collection filter clause."""

    EXACT = "exact"
    PREFIX = "prefix"
    RANGE = "range"


class OperationalFilterFieldType(str, Enum):
    """Data type of an operational collection filter field."""

    STRING = "string"
    INTEGER = "integer"
    TIMESTAMP = "timestamp"


class TraverseDirection(str, Enum):
    """Direction of edge traversal in a graph query."""

    IN = "in"
    OUT = "out"


class DrivingTable(str, Enum):
    """Primary table used to drive query execution."""

    NODES = "nodes"
    FTS_NODES = "fts_nodes"
    VEC_NODES = "vec_nodes"


class TelemetryLevel(str, Enum):
    """Resource telemetry collection level.

    Levels are additive — each level includes everything from below it.
    """

    COUNTERS = "counters"
    """Always-on cumulative counters (queries, writes, errors, cache stats)."""

    STATEMENTS = "statements"
    """Per-statement profiling (wall-clock time, VM steps, cache deltas)."""

    PROFILING = "profiling"
    """Deep profiling (scan status, process CPU/memory/IO snapshots)."""


class ResponseCyclePhase(str, Enum):
    """Phase within a feedback response cycle."""

    STARTED = "started"
    SLOW = "slow"
    HEARTBEAT = "heartbeat"
    FINISHED = "finished"
    FAILED = "failed"


@dataclass(frozen=True)
class TelemetrySnapshot:
    """Point-in-time snapshot of engine telemetry counters.

    All counters are cumulative since engine open.  SQLite cache counters
    are aggregated across all reader pool connections.

    Attributes
    ----------
    queries_total : int
        Total read operations executed.
    writes_total : int
        Total write operations committed.
    write_rows_total : int
        Total rows written (nodes + edges + chunks).
    errors_total : int
        Total operation errors.
    admin_ops_total : int
        Total admin operations (integrity checks, exports, rebuilds, etc.).
    cache_hits : int
        SQLite page cache hits (summed across reader pool).
    cache_misses : int
        SQLite page cache misses.
    cache_writes : int
        Pages written to cache.
    cache_spills : int
        Cache pages spilled to disk.
    """

    queries_total: int = 0
    writes_total: int = 0
    write_rows_total: int = 0
    errors_total: int = 0
    admin_ops_total: int = 0
    cache_hits: int = 0
    cache_misses: int = 0
    cache_writes: int = 0
    cache_spills: int = 0

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "TelemetrySnapshot":
        """Create from the dict returned by the native ``telemetry_snapshot()``."""
        return _from_wire_dataclass(cls, payload)


@dataclass(frozen=True)
class FeedbackConfig:
    """Timing thresholds for progress feedback during long operations."""

    slow_threshold_ms: int = 500
    heartbeat_interval_ms: int = 2000


@dataclass(frozen=True)
class ResponseCycleEvent:
    """A single feedback event emitted during an engine operation."""

    operation_id: str
    operation_kind: str
    surface: str
    phase: ResponseCyclePhase
    elapsed_ms: int
    slow_threshold_ms: int
    metadata: dict[str, str]
    error_code: str | None = None
    error_message: str | None = None


@dataclass(frozen=True)
class RawJson:
    """Pre-serialized JSON string that bypasses automatic encoding."""

    text: str


def _encode_json(value: Any) -> str:
    if isinstance(value, RawJson):
        return value.text
    return json.dumps(value)


def _encode_compat_json_payload(value: Any) -> str:
    if isinstance(value, RawJson):
        return value.text
    if isinstance(value, str):
        return value
    return json.dumps(value)


def _decode_json(value: str) -> Any:
    return json.loads(value)


def _from_wire_dataclass(cls, payload: dict[str, Any]):
    allowed = {item.name for item in fields(cls)}
    unknown = set(payload.keys()) - allowed
    if unknown:
        import logging

        logging.getLogger("fathomdb").debug(
            "_from_wire_dataclass: ignoring unknown fields %s for %s",
            unknown,
            cls.__name__,
        )
    filtered = {key: value for key, value in payload.items() if key in allowed}
    return cls(**filtered)


def _enum_value(value: Enum | str | None) -> str | None:
    if value is None:
        return None
    if isinstance(value, Enum):
        return value.value
    return value


@dataclass(frozen=True)
class BindValue:
    """A typed bind parameter in a compiled query."""

    kind: str
    value: Any

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "BindValue":
        return cls(kind=payload["type"], value=payload["value"])


@dataclass(frozen=True)
class ExecutionHints:
    """Engine-provided limits applied during query execution."""

    recursion_limit: int
    hard_limit: int

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "ExecutionHints":
        return cls(
            recursion_limit=payload["recursion_limit"],
            hard_limit=payload["hard_limit"],
        )


@dataclass(frozen=True)
class CompiledQuery:
    """A query compiled to SQL with bind parameters, ready for execution."""

    sql: str
    binds: list[BindValue]
    shape_hash: int
    driving_table: DrivingTable
    hints: ExecutionHints

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "CompiledQuery":
        return cls(
            sql=payload["sql"],
            binds=[BindValue.from_wire(item) for item in payload["binds"]],
            shape_hash=payload["shape_hash"],
            driving_table=DrivingTable(payload["driving_table"]),
            hints=ExecutionHints.from_wire(payload["hints"]),
        )


@dataclass(frozen=True)
class ExpansionSlot:
    """Definition of a named expansion traversal within a grouped query."""

    slot: str
    direction: TraverseDirection
    label: str
    max_depth: int

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "ExpansionSlot":
        return cls(
            slot=payload["slot"],
            direction=TraverseDirection(payload["direction"]),
            label=payload["label"],
            max_depth=payload["max_depth"],
        )


@dataclass(frozen=True)
class CompiledGroupedQuery:
    """A grouped query compiled to SQL with expansion slot definitions."""

    root: CompiledQuery
    expansions: list[ExpansionSlot]
    shape_hash: int
    hints: ExecutionHints

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "CompiledGroupedQuery":
        return cls(
            root=CompiledQuery.from_wire(payload["root"]),
            expansions=[ExpansionSlot.from_wire(item) for item in payload["expansions"]],
            shape_hash=payload["shape_hash"],
            hints=ExecutionHints.from_wire(payload["hints"]),
        )


@dataclass(frozen=True)
class QueryPlan:
    """Execution plan metadata for a query, without running it."""

    sql: str
    bind_count: int
    driving_table: DrivingTable
    shape_hash: int
    cache_hit: bool

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "QueryPlan":
        return cls(
            sql=payload["sql"],
            bind_count=payload["bind_count"],
            driving_table=DrivingTable(payload["driving_table"]),
            shape_hash=payload["shape_hash"],
            cache_hit=payload["cache_hit"],
        )


@dataclass(frozen=True)
class NodeRow:
    """A node returned from a query result set."""

    row_id: str
    logical_id: str
    kind: str
    properties: Any
    content_ref: str | None = None
    last_accessed_at: int | None = None

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "NodeRow":
        return cls(
            row_id=payload["row_id"],
            logical_id=payload["logical_id"],
            kind=payload["kind"],
            properties=_decode_json(payload["properties"]),
            content_ref=payload.get("content_ref"),
            last_accessed_at=payload.get("last_accessed_at"),
        )


@dataclass(frozen=True)
class RunRow:
    """A run returned from a query result set."""

    id: str
    kind: str
    status: str
    properties: Any

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "RunRow":
        return cls(
            id=payload["id"],
            kind=payload["kind"],
            status=payload["status"],
            properties=_decode_json(payload["properties"]),
        )


@dataclass(frozen=True)
class StepRow:
    """A step returned from a query result set."""

    id: str
    run_id: str
    kind: str
    status: str
    properties: Any

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "StepRow":
        return cls(
            id=payload["id"],
            run_id=payload["run_id"],
            kind=payload["kind"],
            status=payload["status"],
            properties=_decode_json(payload["properties"]),
        )


@dataclass(frozen=True)
class ActionRow:
    """An action returned from a query result set."""

    id: str
    step_id: str
    kind: str
    status: str
    properties: Any

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "ActionRow":
        return cls(
            id=payload["id"],
            step_id=payload["step_id"],
            kind=payload["kind"],
            status=payload["status"],
            properties=_decode_json(payload["properties"]),
        )


@dataclass(frozen=True)
class QueryRows:
    """Result set from a flat (non-grouped) query execution."""

    nodes: list[NodeRow]
    runs: list[RunRow]
    steps: list[StepRow]
    actions: list[ActionRow]
    was_degraded: bool

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "QueryRows":
        return cls(
            nodes=[NodeRow.from_wire(item) for item in payload["nodes"]],
            runs=[RunRow.from_wire(item) for item in payload["runs"]],
            steps=[StepRow.from_wire(item) for item in payload["steps"]],
            actions=[ActionRow.from_wire(item) for item in payload["actions"]],
            was_degraded=payload["was_degraded"],
        )


@dataclass(frozen=True)
class ExpansionRootRows:
    """Expanded nodes reached from a single root in a grouped query."""

    root_logical_id: str
    nodes: list[NodeRow]

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "ExpansionRootRows":
        return cls(
            root_logical_id=payload["root_logical_id"],
            nodes=[NodeRow.from_wire(item) for item in payload["nodes"]],
        )


@dataclass(frozen=True)
class ExpansionSlotRows:
    """All expansion results for a named slot across all root nodes."""

    slot: str
    roots: list[ExpansionRootRows]

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "ExpansionSlotRows":
        return cls(
            slot=payload["slot"],
            roots=[ExpansionRootRows.from_wire(item) for item in payload["roots"]],
        )


@dataclass(frozen=True)
class GroupedQueryRows:
    """Result set from a grouped query execution with expansions."""

    roots: list[NodeRow]
    expansions: list[ExpansionSlotRows]
    was_degraded: bool

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "GroupedQueryRows":
        return cls(
            roots=[NodeRow.from_wire(item) for item in payload["roots"]],
            expansions=[ExpansionSlotRows.from_wire(item) for item in payload["expansions"]],
            was_degraded=payload["was_degraded"],
        )


@dataclass(frozen=True)
class IntegrityReport:
    """Result of a database physical and logical integrity check."""

    physical_ok: bool = False
    foreign_keys_ok: bool = False
    missing_fts_rows: int = 0
    missing_property_fts_rows: int = 0
    duplicate_active_logical_ids: int = 0
    operational_missing_collections: int = 0
    operational_missing_last_mutations: int = 0
    warnings: list[str] = field(default_factory=list)

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "IntegrityReport":
        return _from_wire_dataclass(cls, payload)


@dataclass(frozen=True)
class SemanticReport:
    """Result of a semantic consistency check across graph entities."""

    orphaned_chunks: int = 0
    null_source_ref_nodes: int = 0
    broken_step_fk: int = 0
    broken_action_fk: int = 0
    stale_fts_rows: int = 0
    fts_rows_for_superseded_nodes: int = 0
    stale_property_fts_rows: int = 0
    orphaned_property_fts_rows: int = 0
    mismatched_kind_property_fts_rows: int = 0
    duplicate_property_fts_rows: int = 0
    drifted_property_fts_rows: int = 0
    dangling_edges: int = 0
    orphaned_supersession_chains: int = 0
    stale_vec_rows: int = 0
    vec_rows_for_superseded_nodes: int = 0
    missing_operational_current_rows: int = 0
    stale_operational_current_rows: int = 0
    disabled_collection_mutations: int = 0
    orphaned_last_access_metadata_rows: int = 0
    warnings: list[str] = field(default_factory=list)

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "SemanticReport":
        return _from_wire_dataclass(cls, payload)


@dataclass(frozen=True)
class TraceReport:
    """Summary of all entities associated with a source reference."""

    source_ref: str = ""
    node_rows: int = 0
    edge_rows: int = 0
    action_rows: int = 0
    operational_mutation_rows: int = 0
    node_logical_ids: list[str] = field(default_factory=list)
    action_ids: list[str] = field(default_factory=list)
    operational_mutation_ids: list[str] = field(default_factory=list)

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "TraceReport":
        return _from_wire_dataclass(cls, payload)


@dataclass(frozen=True)
class SkippedEdge:
    """An edge that was skipped during restore because an endpoint is missing."""

    edge_logical_id: str = ""
    missing_endpoint: str = ""


@dataclass(frozen=True)
class LogicalRestoreReport:
    """Result of restoring a retired node by logical ID."""

    logical_id: str = ""
    was_noop: bool = False
    restored_node_rows: int = 0
    restored_edge_rows: int = 0
    restored_chunk_rows: int = 0
    restored_fts_rows: int = 0
    restored_property_fts_rows: int = 0
    restored_vec_rows: int = 0
    skipped_edges: list[SkippedEdge] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "LogicalRestoreReport":
        raw_skipped = payload.get("skipped_edges", [])
        skipped = [SkippedEdge(**item) for item in raw_skipped]
        filtered = {
            key: value
            for key, value in payload.items()
            if key in {f.name for f in fields(cls)} and key != "skipped_edges"
        }
        return cls(skipped_edges=skipped, **filtered)


@dataclass(frozen=True)
class LogicalPurgeReport:
    """Result of permanently purging all rows for a logical ID."""

    logical_id: str = ""
    was_noop: bool = False
    deleted_node_rows: int = 0
    deleted_edge_rows: int = 0
    deleted_chunk_rows: int = 0
    deleted_fts_rows: int = 0
    deleted_vec_rows: int = 0
    notes: list[str] = field(default_factory=list)

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "LogicalPurgeReport":
        return _from_wire_dataclass(cls, payload)


@dataclass(frozen=True)
class ProjectionRepairReport:
    """Result of rebuilding projection indexes."""

    targets: list[ProjectionTarget]
    rebuilt_rows: int
    notes: list[str]

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "ProjectionRepairReport":
        return cls(
            targets=[ProjectionTarget(item) for item in payload["targets"]],
            rebuilt_rows=payload["rebuilt_rows"],
            notes=payload["notes"],
        )


@dataclass(frozen=True)
class SafeExportManifest:
    """Manifest describing a safely exported database snapshot."""

    exported_at: int
    sha256: str
    schema_version: int
    protocol_version: int
    page_count: int

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "SafeExportManifest":
        return cls(**payload)


@dataclass(slots=True)
class FtsPropertySchemaRecord:
    """A registered FTS property projection schema for a node kind."""

    kind: str
    property_paths: list[str]
    separator: str
    format_version: int

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "FtsPropertySchemaRecord":
        return cls(
            kind=payload["kind"],
            property_paths=payload.get("property_paths", []),
            separator=payload.get("separator", " "),
            format_version=payload.get("format_version", 1),
        )


@dataclass(frozen=True)
class OperationalCollectionRecord:
    """Metadata record describing a registered operational collection."""

    name: str
    kind: OperationalCollectionKind
    schema_json: str
    retention_json: str
    validation_json: str
    secondary_indexes_json: str
    format_version: int
    created_at: int
    filter_fields_json: str = "[]"
    disabled_at: int | None = None

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "OperationalCollectionRecord":
        return cls(
            name=payload["name"],
            kind=OperationalCollectionKind(payload["kind"]),
            schema_json=payload["schema_json"],
            retention_json=payload["retention_json"],
            validation_json=payload.get("validation_json", ""),
            secondary_indexes_json=payload.get("secondary_indexes_json", "[]"),
            filter_fields_json=payload.get("filter_fields_json", "[]"),
            format_version=payload["format_version"],
            created_at=payload["created_at"],
            disabled_at=payload.get("disabled_at"),
        )


@dataclass(slots=True)
class OperationalRegisterRequest:
    """Request payload for registering a new operational collection."""

    name: str
    kind: OperationalCollectionKind
    schema_json: str
    retention_json: str
    format_version: int
    filter_fields_json: str = "[]"
    validation_json: str = ""
    secondary_indexes_json: str = "[]"

    def to_wire(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "kind": _enum_value(self.kind),
            "schema_json": self.schema_json,
            "retention_json": self.retention_json,
            "filter_fields_json": self.filter_fields_json,
            "validation_json": self.validation_json,
            "secondary_indexes_json": self.secondary_indexes_json,
            "format_version": self.format_version,
        }


@dataclass(frozen=True)
class OperationalFilterValue:
    """A typed value used in an operational collection filter clause."""

    value: str | int

    @classmethod
    def string(cls, value: str) -> "OperationalFilterValue":
        return cls(value=value)

    @classmethod
    def integer(cls, value: int) -> "OperationalFilterValue":
        return cls(value=value)

    def to_wire(self) -> str | int:
        return self.value


@dataclass(frozen=True)
class OperationalFilterClause:
    """A filter clause for querying an operational collection."""

    mode: OperationalFilterMode
    field: str
    value: str | OperationalFilterValue | None = None
    lower: int | None = None
    upper: int | None = None

    @classmethod
    def exact(cls, field: str, value: OperationalFilterValue) -> "OperationalFilterClause":
        return cls(mode=OperationalFilterMode.EXACT, field=field, value=value)

    @classmethod
    def prefix(cls, field: str, value: str) -> "OperationalFilterClause":
        return cls(mode=OperationalFilterMode.PREFIX, field=field, value=value)

    @classmethod
    def range(
        cls, field: str, *, lower: int | None = None, upper: int | None = None
    ) -> "OperationalFilterClause":
        return cls(
            mode=OperationalFilterMode.RANGE,
            field=field,
            lower=lower,
            upper=upper,
        )

    def to_wire(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "mode": _enum_value(self.mode),
            "field": self.field,
        }
        if self.mode is OperationalFilterMode.EXACT:
            assert self.value is not None
            payload["value"] = (
                self.value.to_wire()
                if isinstance(self.value, OperationalFilterValue)
                else self.value
            )
        elif self.mode is OperationalFilterMode.PREFIX:
            payload["value"] = self.value
        else:
            payload["lower"] = self.lower
            payload["upper"] = self.upper
        return payload


@dataclass(frozen=True)
class OperationalMutationRow:
    """A single mutation row from an operational collection."""

    id: str
    collection_name: str
    record_key: str
    op_kind: str
    payload_json: Any
    source_ref: str | None
    created_at: int

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "OperationalMutationRow":
        return cls(
            id=payload["id"],
            collection_name=payload["collection_name"],
            record_key=payload["record_key"],
            op_kind=payload["op_kind"],
            payload_json=_decode_json(payload["payload_json"]),
            source_ref=payload.get("source_ref"),
            created_at=payload["created_at"],
        )


@dataclass(frozen=True)
class OperationalCurrentRow:
    """The current materialized state of a record in an operational collection."""

    collection_name: str
    record_key: str
    payload_json: Any
    updated_at: int
    last_mutation_id: str

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "OperationalCurrentRow":
        return cls(
            collection_name=payload["collection_name"],
            record_key=payload["record_key"],
            payload_json=_decode_json(payload["payload_json"]),
            updated_at=payload["updated_at"],
            last_mutation_id=payload["last_mutation_id"],
        )


@dataclass(frozen=True)
class OperationalTraceReport:
    """Trace of mutations and current state for an operational collection."""

    collection_name: str
    record_key: str | None
    mutation_count: int
    current_count: int
    mutations: list[OperationalMutationRow]
    current_rows: list[OperationalCurrentRow]

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "OperationalTraceReport":
        return cls(
            collection_name=payload["collection_name"],
            record_key=payload.get("record_key"),
            mutation_count=payload["mutation_count"],
            current_count=payload["current_count"],
            mutations=[OperationalMutationRow.from_wire(item) for item in payload["mutations"]],
            current_rows=[OperationalCurrentRow.from_wire(item) for item in payload["current_rows"]],
        )


@dataclass(slots=True)
class OperationalReadRequest:
    """Request payload for reading filtered rows from an operational collection."""

    collection_name: str
    filters: list[OperationalFilterClause]
    limit: int | None = None

    def to_wire(self) -> dict[str, Any]:
        return {
            "collection_name": self.collection_name,
            "filters": [item.to_wire() for item in self.filters],
            "limit": self.limit,
        }


@dataclass(frozen=True)
class OperationalReadReport:
    """Result of reading filtered rows from an operational collection."""

    collection_name: str
    row_count: int
    applied_limit: int
    was_limited: bool
    rows: list[OperationalMutationRow]

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "OperationalReadReport":
        return cls(
            collection_name=payload["collection_name"],
            row_count=payload["row_count"],
            applied_limit=payload["applied_limit"],
            was_limited=payload["was_limited"],
            rows=[OperationalMutationRow.from_wire(item) for item in payload["rows"]],
        )


@dataclass(frozen=True)
class OperationalRepairReport:
    """Result of rebuilding current-state views for operational collections."""

    collections_rebuilt: int
    current_rows_rebuilt: int

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "OperationalRepairReport":
        return cls(**payload)


@dataclass(frozen=True)
class OperationalHistoryValidationIssue:
    """A single validation issue found in an operational collection's history."""

    mutation_id: str
    record_key: str
    op_kind: str
    message: str

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "OperationalHistoryValidationIssue":
        return cls(**payload)


@dataclass(frozen=True)
class OperationalHistoryValidationReport:
    """Result of validating the mutation history of an operational collection."""

    collection_name: str
    checked_rows: int
    invalid_row_count: int
    issues: list[OperationalHistoryValidationIssue]

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "OperationalHistoryValidationReport":
        return cls(
            collection_name=payload["collection_name"],
            checked_rows=payload["checked_rows"],
            invalid_row_count=payload["invalid_row_count"],
            issues=[
                OperationalHistoryValidationIssue.from_wire(item)
                for item in payload["issues"]
            ],
        )


@dataclass(frozen=True)
class OperationalCompactionReport:
    """Result of compacting an operational collection."""

    collection_name: str
    deleted_mutations: int
    dry_run: bool
    before_timestamp: int | None

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "OperationalCompactionReport":
        return cls(**payload)


@dataclass(frozen=True)
class OperationalPurgeReport:
    """Result of purging old mutations from an operational collection."""

    collection_name: str
    deleted_mutations: int
    before_timestamp: int

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "OperationalPurgeReport":
        return cls(**payload)


@dataclass(slots=True)
class OptionalProjectionTask:
    """A deferred projection backfill task included in a write request."""

    target: ProjectionTarget
    payload: Any

    def to_wire(self) -> dict[str, Any]:
        return {
            "target": _enum_value(self.target),
            "payload": _encode_compat_json_payload(self.payload),
        }


@dataclass(slots=True)
class NodeInsert:
    """Wire representation of a node to be inserted or upserted."""

    row_id: str
    logical_id: str
    kind: str
    properties: Any
    source_ref: str | None = None
    upsert: bool = False
    chunk_policy: ChunkPolicy = ChunkPolicy.PRESERVE
    content_ref: str | None = None

    def to_wire(self) -> dict[str, Any]:
        return {
            "row_id": self.row_id,
            "logical_id": self.logical_id,
            "kind": self.kind,
            "properties": _encode_json(self.properties),
            "source_ref": self.source_ref,
            "upsert": self.upsert,
            "chunk_policy": _enum_value(self.chunk_policy),
            "content_ref": self.content_ref,
        }


@dataclass(slots=True)
class EdgeInsert:
    """Wire representation of an edge to be inserted or upserted."""

    row_id: str
    logical_id: str
    source_logical_id: str
    target_logical_id: str
    kind: str
    properties: Any
    source_ref: str | None = None
    upsert: bool = False

    def to_wire(self) -> dict[str, Any]:
        return {
            "row_id": self.row_id,
            "logical_id": self.logical_id,
            "source_logical_id": self.source_logical_id,
            "target_logical_id": self.target_logical_id,
            "kind": self.kind,
            "properties": _encode_json(self.properties),
            "source_ref": self.source_ref,
            "upsert": self.upsert,
        }


@dataclass(slots=True)
class NodeRetire:
    """Wire representation of a node retirement (soft-delete)."""

    logical_id: str
    source_ref: str | None = None

    def to_wire(self) -> dict[str, Any]:
        return {"logical_id": self.logical_id, "source_ref": self.source_ref}


@dataclass(slots=True)
class EdgeRetire:
    """Wire representation of an edge retirement (soft-delete)."""

    logical_id: str
    source_ref: str | None = None

    def to_wire(self) -> dict[str, Any]:
        return {"logical_id": self.logical_id, "source_ref": self.source_ref}


@dataclass(slots=True)
class ChunkInsert:
    """Wire representation of a text chunk to be inserted."""

    id: str
    node_logical_id: str
    text_content: str
    byte_start: int | None = None
    byte_end: int | None = None
    content_hash: str | None = None

    def to_wire(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "node_logical_id": self.node_logical_id,
            "text_content": self.text_content,
            "byte_start": self.byte_start,
            "byte_end": self.byte_end,
            "content_hash": self.content_hash,
        }


@dataclass(slots=True)
class VecInsert:
    """Wire representation of a vector embedding to be inserted."""

    chunk_id: str
    embedding: list[float]

    def to_wire(self) -> dict[str, Any]:
        return {"chunk_id": self.chunk_id, "embedding": self.embedding}


@dataclass(slots=True)
class OperationalAppend:
    """An append mutation for an operational collection."""

    collection: str
    record_key: str
    payload_json: Any
    source_ref: str | None = None

    def to_wire(self) -> dict[str, Any]:
        return {
            "type": "append",
            "collection": self.collection,
            "record_key": self.record_key,
            "payload_json": _encode_json(self.payload_json),
            "source_ref": self.source_ref,
        }


@dataclass(slots=True)
class OperationalPut:
    """A put (upsert) mutation for an operational collection."""

    collection: str
    record_key: str
    payload_json: Any
    source_ref: str | None = None

    def to_wire(self) -> dict[str, Any]:
        return {
            "type": "put",
            "collection": self.collection,
            "record_key": self.record_key,
            "payload_json": _encode_json(self.payload_json),
            "source_ref": self.source_ref,
        }


@dataclass(slots=True)
class OperationalDelete:
    """A delete mutation for an operational collection."""

    collection: str
    record_key: str
    source_ref: str | None = None

    def to_wire(self) -> dict[str, Any]:
        return {
            "type": "delete",
            "collection": self.collection,
            "record_key": self.record_key,
            "source_ref": self.source_ref,
        }


@dataclass(slots=True)
class RunInsert:
    """Wire representation of a run to be inserted or upserted."""

    id: str
    kind: str
    status: str
    properties: Any
    source_ref: str | None = None
    upsert: bool = False
    supersedes_id: str | None = None

    def to_wire(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "kind": self.kind,
            "status": self.status,
            "properties": _encode_json(self.properties),
            "source_ref": self.source_ref,
            "upsert": self.upsert,
            "supersedes_id": self.supersedes_id,
        }


@dataclass(slots=True)
class StepInsert:
    """Wire representation of a step to be inserted or upserted."""

    id: str
    run_id: str
    kind: str
    status: str
    properties: Any
    source_ref: str | None = None
    upsert: bool = False
    supersedes_id: str | None = None

    def to_wire(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "run_id": self.run_id,
            "kind": self.kind,
            "status": self.status,
            "properties": _encode_json(self.properties),
            "source_ref": self.source_ref,
            "upsert": self.upsert,
            "supersedes_id": self.supersedes_id,
        }


@dataclass(slots=True)
class ActionInsert:
    """Wire representation of an action to be inserted or upserted."""

    id: str
    step_id: str
    kind: str
    status: str
    properties: Any
    source_ref: str | None = None
    upsert: bool = False
    supersedes_id: str | None = None

    def to_wire(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "step_id": self.step_id,
            "kind": self.kind,
            "status": self.status,
            "properties": _encode_json(self.properties),
            "source_ref": self.source_ref,
            "upsert": self.upsert,
            "supersedes_id": self.supersedes_id,
        }


@dataclass(slots=True)
class WriteRequest:
    """A batch of mutations (nodes, edges, chunks, etc.) to submit atomically."""

    label: str
    nodes: list[NodeInsert] = field(default_factory=list)
    node_retires: list[NodeRetire] = field(default_factory=list)
    edges: list[EdgeInsert] = field(default_factory=list)
    edge_retires: list[EdgeRetire] = field(default_factory=list)
    chunks: list[ChunkInsert] = field(default_factory=list)
    runs: list[RunInsert] = field(default_factory=list)
    steps: list[StepInsert] = field(default_factory=list)
    actions: list[ActionInsert] = field(default_factory=list)
    optional_backfills: list[OptionalProjectionTask] = field(default_factory=list)
    vec_inserts: list[VecInsert] = field(default_factory=list)
    operational_writes: list[OperationalAppend | OperationalPut | OperationalDelete] = field(
        default_factory=list
    )

    def to_wire(self) -> dict[str, Any]:
        return {
            "label": self.label,
            "nodes": [item.to_wire() for item in self.nodes],
            "node_retires": [item.to_wire() for item in self.node_retires],
            "edges": [item.to_wire() for item in self.edges],
            "edge_retires": [item.to_wire() for item in self.edge_retires],
            "chunks": [item.to_wire() for item in self.chunks],
            "runs": [item.to_wire() for item in self.runs],
            "steps": [item.to_wire() for item in self.steps],
            "actions": [item.to_wire() for item in self.actions],
            "optional_backfills": [item.to_wire() for item in self.optional_backfills],
            "vec_inserts": [item.to_wire() for item in self.vec_inserts],
            "operational_writes": [item.to_wire() for item in self.operational_writes],
        }


@dataclass(frozen=True)
class WriteReceipt:
    """Confirmation returned after a successful write submission."""

    label: str
    optional_backfill_count: int
    warnings: list[str] = field(default_factory=list)
    provenance_warnings: list[str] = field(default_factory=list)

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "WriteReceipt":
        return _from_wire_dataclass(cls, payload)


@dataclass(frozen=True)
class OperationalSecondaryIndexRebuildReport:
    """Result of rebuilding secondary indexes for an operational collection."""

    collection_name: str
    mutation_entries_rebuilt: int
    current_entries_rebuilt: int

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "OperationalSecondaryIndexRebuildReport":
        return cls(**payload)


class OperationalRetentionActionKind(str, Enum):
    """Kind of retention action applied to an operational collection."""

    NOOP = "noop"
    PURGE_BEFORE_SECONDS = "purge_before_seconds"
    KEEP_LAST = "keep_last"


@dataclass(frozen=True)
class OperationalRetentionPlanItem:
    """Planned retention action for a single operational collection."""

    collection_name: str
    action_kind: OperationalRetentionActionKind
    candidate_deletions: int
    before_timestamp: int | None = None
    max_rows: int | None = None
    last_run_at: int | None = None

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "OperationalRetentionPlanItem":
        return cls(
            collection_name=payload["collection_name"],
            action_kind=OperationalRetentionActionKind(payload["action_kind"]),
            candidate_deletions=payload["candidate_deletions"],
            before_timestamp=payload.get("before_timestamp"),
            max_rows=payload.get("max_rows"),
            last_run_at=payload.get("last_run_at"),
        )


@dataclass(frozen=True)
class OperationalRetentionPlanReport:
    """Result of planning retention across operational collections."""

    planned_at: int
    collections_examined: int
    items: list[OperationalRetentionPlanItem]

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "OperationalRetentionPlanReport":
        return cls(
            planned_at=payload["planned_at"],
            collections_examined=payload["collections_examined"],
            items=[OperationalRetentionPlanItem.from_wire(item) for item in payload["items"]],
        )


@dataclass(frozen=True)
class OperationalRetentionRunItem:
    """Result of executing retention for a single operational collection."""

    collection_name: str
    action_kind: OperationalRetentionActionKind
    deleted_mutations: int
    before_timestamp: int | None = None
    max_rows: int | None = None
    rows_remaining: int = 0

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "OperationalRetentionRunItem":
        return cls(
            collection_name=payload["collection_name"],
            action_kind=OperationalRetentionActionKind(payload["action_kind"]),
            deleted_mutations=payload["deleted_mutations"],
            before_timestamp=payload.get("before_timestamp"),
            max_rows=payload.get("max_rows"),
            rows_remaining=payload["rows_remaining"],
        )


@dataclass(frozen=True)
class OperationalRetentionRunReport:
    """Result of executing retention across operational collections."""

    executed_at: int
    collections_examined: int
    collections_acted_on: int
    dry_run: bool
    items: list[OperationalRetentionRunItem]

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "OperationalRetentionRunReport":
        return cls(
            executed_at=payload["executed_at"],
            collections_examined=payload["collections_examined"],
            collections_acted_on=payload["collections_acted_on"],
            dry_run=payload["dry_run"],
            items=[OperationalRetentionRunItem.from_wire(item) for item in payload["items"]],
        )


@dataclass(frozen=True)
class ProvenancePurgeReport:
    """Result of purging old provenance events."""

    events_deleted: int
    events_preserved: int
    oldest_remaining: int | None = None

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "ProvenancePurgeReport":
        return cls(
            events_deleted=payload["events_deleted"],
            events_preserved=payload["events_preserved"],
            oldest_remaining=payload.get("oldest_remaining"),
        )


@dataclass(slots=True)
class LastAccessTouchRequest:
    """Request to update last-accessed timestamps for a set of nodes."""

    logical_ids: list[str]
    touched_at: int
    source_ref: str | None = None

    def to_wire(self) -> dict[str, Any]:
        return {
            "logical_ids": list(self.logical_ids),
            "touched_at": self.touched_at,
            "source_ref": self.source_ref,
        }


@dataclass(frozen=True)
class LastAccessTouchReport:
    """Result of updating last-accessed timestamps."""

    touched_logical_ids: int
    touched_at: int

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "LastAccessTouchReport":
        return cls(**payload)
