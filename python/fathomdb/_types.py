from __future__ import annotations

import json
from dataclasses import dataclass, field
from enum import Enum
from typing import Any


class ProvenanceMode(str, Enum):
    WARN = "warn"
    REQUIRE = "require"


class ChunkPolicy(str, Enum):
    PRESERVE = "preserve"
    REPLACE = "replace"


class ProjectionTarget(str, Enum):
    FTS = "fts"
    VEC = "vec"
    ALL = "all"


class TraverseDirection(str, Enum):
    IN = "in"
    OUT = "out"


class DrivingTable(str, Enum):
    NODES = "nodes"
    FTS_NODES = "fts_nodes"
    VEC_NODES = "vec_nodes"


class ResponseCyclePhase(str, Enum):
    STARTED = "started"
    SLOW = "slow"
    HEARTBEAT = "heartbeat"
    FINISHED = "finished"
    FAILED = "failed"


@dataclass(frozen=True)
class FeedbackConfig:
    slow_threshold_ms: int = 500
    heartbeat_interval_ms: int = 2000


@dataclass(frozen=True)
class ResponseCycleEvent:
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
    text: str


def _encode_json(value: Any) -> str:
    if isinstance(value, RawJson):
        return value.text
    return json.dumps(value)


def _decode_json(value: str) -> Any:
    return json.loads(value)


def _enum_value(value: Enum | str | None) -> str | None:
    if value is None:
        return None
    if isinstance(value, Enum):
        return value.value
    return value


@dataclass(frozen=True)
class BindValue:
    kind: str
    value: Any

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "BindValue":
        return cls(kind=payload["type"], value=payload["value"])


@dataclass(frozen=True)
class ExecutionHints:
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
class QueryPlan:
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
    row_id: str
    logical_id: str
    kind: str
    properties: Any

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "NodeRow":
        return cls(
            row_id=payload["row_id"],
            logical_id=payload["logical_id"],
            kind=payload["kind"],
            properties=_decode_json(payload["properties"]),
        )


@dataclass(frozen=True)
class RunRow:
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
class IntegrityReport:
    physical_ok: bool
    foreign_keys_ok: bool
    missing_fts_rows: int
    duplicate_active_logical_ids: int
    warnings: list[str]

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "IntegrityReport":
        return cls(**payload)


@dataclass(frozen=True)
class SemanticReport:
    orphaned_chunks: int
    null_source_ref_nodes: int
    broken_step_fk: int
    broken_action_fk: int
    stale_fts_rows: int
    fts_rows_for_superseded_nodes: int
    dangling_edges: int
    orphaned_supersession_chains: int
    stale_vec_rows: int
    vec_rows_for_superseded_nodes: int
    warnings: list[str]

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "SemanticReport":
        return cls(**payload)


@dataclass(frozen=True)
class TraceReport:
    source_ref: str
    node_rows: int
    edge_rows: int
    action_rows: int
    node_logical_ids: list[str]
    action_ids: list[str]

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "TraceReport":
        return cls(**payload)


@dataclass(frozen=True)
class ProjectionRepairReport:
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
    exported_at: int
    sha256: str
    schema_version: int
    protocol_version: int
    page_count: int

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "SafeExportManifest":
        return cls(**payload)


@dataclass(slots=True)
class OptionalProjectionTask:
    target: ProjectionTarget
    payload: str

    def to_wire(self) -> dict[str, Any]:
        return {"target": _enum_value(self.target), "payload": self.payload}


@dataclass(slots=True)
class NodeInsert:
    row_id: str
    logical_id: str
    kind: str
    properties: Any
    source_ref: str | None = None
    upsert: bool = False
    chunk_policy: ChunkPolicy = ChunkPolicy.PRESERVE

    def to_wire(self) -> dict[str, Any]:
        return {
            "row_id": self.row_id,
            "logical_id": self.logical_id,
            "kind": self.kind,
            "properties": _encode_json(self.properties),
            "source_ref": self.source_ref,
            "upsert": self.upsert,
            "chunk_policy": _enum_value(self.chunk_policy),
        }


@dataclass(slots=True)
class EdgeInsert:
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
    logical_id: str
    source_ref: str | None = None

    def to_wire(self) -> dict[str, Any]:
        return {"logical_id": self.logical_id, "source_ref": self.source_ref}


@dataclass(slots=True)
class EdgeRetire:
    logical_id: str
    source_ref: str | None = None

    def to_wire(self) -> dict[str, Any]:
        return {"logical_id": self.logical_id, "source_ref": self.source_ref}


@dataclass(slots=True)
class ChunkInsert:
    id: str
    node_logical_id: str
    text_content: str
    byte_start: int | None = None
    byte_end: int | None = None

    def to_wire(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "node_logical_id": self.node_logical_id,
            "text_content": self.text_content,
            "byte_start": self.byte_start,
            "byte_end": self.byte_end,
        }


@dataclass(slots=True)
class VecInsert:
    chunk_id: str
    embedding: list[float]

    def to_wire(self) -> dict[str, Any]:
        return {"chunk_id": self.chunk_id, "embedding": self.embedding}


@dataclass(slots=True)
class RunInsert:
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
        }


@dataclass(frozen=True)
class WriteReceipt:
    label: str
    optional_backfill_count: int
    provenance_warnings: list[str]

    @classmethod
    def from_wire(cls, payload: dict[str, Any]) -> "WriteReceipt":
        return cls(**payload)
