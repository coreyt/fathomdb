from __future__ import annotations

from dataclasses import dataclass
from itertools import count
from typing import Any

from ._types import (
    ActionInsert,
    ChunkInsert,
    ChunkPolicy,
    EdgeInsert,
    EdgeRetire,
    NodeInsert,
    NodeRetire,
    OperationalAppend,
    OperationalDelete,
    OperationalPut,
    OptionalProjectionTask,
    ProjectionTarget,
    RunInsert,
    StepInsert,
    VecInsert,
    WriteRequest,
)
from .errors import BuilderValidationError

_builder_ids = count(1)


@dataclass(frozen=True)
class NodeHandle:
    """Opaque reference to a node added via WriteRequestBuilder."""

    row_id: str
    logical_id: str
    _builder_id: int


@dataclass(frozen=True)
class EdgeHandle:
    """Opaque reference to an edge added via WriteRequestBuilder."""

    logical_id: str
    _builder_id: int


@dataclass(frozen=True)
class RunHandle:
    """Opaque reference to a run added via WriteRequestBuilder."""

    id: str
    _builder_id: int


@dataclass(frozen=True)
class StepHandle:
    """Opaque reference to a step added via WriteRequestBuilder."""

    id: str
    _builder_id: int


@dataclass(frozen=True)
class ActionHandle:
    """Opaque reference to an action added via WriteRequestBuilder."""

    id: str
    _builder_id: int


@dataclass(frozen=True)
class ChunkHandle:
    """Opaque reference to a chunk added via WriteRequestBuilder."""

    id: str
    _builder_id: int


@dataclass(slots=True)
class _PendingEdge:
    row_id: str
    logical_id: str
    source: NodeHandle | str
    target: NodeHandle | str
    kind: str
    properties: Any
    source_ref: str | None
    upsert: bool


@dataclass(slots=True)
class _PendingNodeRetire:
    logical_id: NodeHandle | str
    source_ref: str | None


@dataclass(slots=True)
class _PendingEdgeRetire:
    logical_id: EdgeHandle | str
    source_ref: str | None


@dataclass(slots=True)
class _PendingChunk:
    id: str
    node: NodeHandle | str
    text_content: str
    byte_start: int | None
    byte_end: int | None
    content_hash: str | None


@dataclass(slots=True)
class _PendingStep:
    id: str
    run: RunHandle | str
    kind: str
    status: str
    properties: Any
    source_ref: str | None
    upsert: bool
    supersedes_id: str | None


@dataclass(slots=True)
class _PendingAction:
    id: str
    step: StepHandle | str
    kind: str
    status: str
    properties: Any
    source_ref: str | None
    upsert: bool
    supersedes_id: str | None


@dataclass(slots=True)
class _PendingVec:
    chunk: ChunkHandle | str
    embedding: list[float]


class WriteRequestBuilder:
    """Mutable builder that assembles a WriteRequest from individual mutations.

    Handles are returned when adding nodes, edges, runs, steps, actions, and
    chunks so they can be cross-referenced within the same request.  Call
    :meth:`build` to produce the final :class:`WriteRequest`.
    """

    def __init__(self, label: str) -> None:
        self._builder_id = next(_builder_ids)
        self._label = label
        self._nodes: list[NodeInsert] = []
        self._node_retires: list[_PendingNodeRetire] = []
        self._edges: list[_PendingEdge] = []
        self._edge_retires: list[_PendingEdgeRetire] = []
        self._chunks: list[_PendingChunk] = []
        self._runs: list[RunInsert] = []
        self._steps: list[_PendingStep] = []
        self._actions: list[_PendingAction] = []
        self._optional_backfills: list[OptionalProjectionTask] = []
        self._vec_inserts: list[_PendingVec] = []
        self._operational_writes: list[OperationalAppend | OperationalPut | OperationalDelete] = []

    def add_node(
        self,
        *,
        row_id: str,
        logical_id: str,
        kind: str,
        properties: Any,
        source_ref: str | None = None,
        upsert: bool = False,
        chunk_policy: ChunkPolicy = ChunkPolicy.PRESERVE,
        content_ref: str | None = None,
    ) -> NodeHandle:
        """Add a node insert to the write request.

        Args:
            row_id: Unique row identifier for this version of the node.
            logical_id: Stable logical identifier for the node.
            kind: Node kind (type label).
            properties: JSON-serializable properties payload.
            source_ref: Optional provenance source reference.
            upsert: If True, replace an existing node with the same logical ID.
            chunk_policy: How to handle existing chunks on upsert.
            content_ref: Optional URI referencing external content.

        Returns
        -------
            A NodeHandle that can be used to reference this node elsewhere
            in the same builder.
        """
        handle = NodeHandle(row_id=row_id, logical_id=logical_id, _builder_id=self._builder_id)
        self._nodes.append(
            NodeInsert(
                row_id=row_id,
                logical_id=logical_id,
                kind=kind,
                properties=properties,
                source_ref=source_ref,
                upsert=upsert,
                chunk_policy=chunk_policy,
                content_ref=content_ref,
            )
        )
        return handle

    def retire_node(self, *, logical_id: NodeHandle | str, source_ref: str | None = None) -> None:
        """Mark a node as retired (soft-delete) by logical ID or handle."""
        self._node_retires.append(_PendingNodeRetire(logical_id=logical_id, source_ref=source_ref))

    def add_edge(
        self,
        *,
        row_id: str,
        logical_id: str,
        source: NodeHandle | str,
        target: NodeHandle | str,
        kind: str,
        properties: Any,
        source_ref: str | None = None,
        upsert: bool = False,
    ) -> EdgeHandle:
        """Add an edge insert connecting two nodes.

        Args:
            row_id: Unique row identifier for this version of the edge.
            logical_id: Stable logical identifier for the edge.
            source: Source node (handle or logical ID string).
            target: Target node (handle or logical ID string).
            kind: Edge kind (type label).
            properties: JSON-serializable properties payload.
            source_ref: Optional provenance source reference.
            upsert: If True, replace an existing edge with the same logical ID.

        Returns
        -------
            An EdgeHandle that can be used to reference this edge elsewhere
            in the same builder.
        """
        handle = EdgeHandle(logical_id=logical_id, _builder_id=self._builder_id)
        self._edges.append(
            _PendingEdge(
                row_id=row_id,
                logical_id=logical_id,
                source=source,
                target=target,
                kind=kind,
                properties=properties,
                source_ref=source_ref,
                upsert=upsert,
            )
        )
        return handle

    def retire_edge(self, *, logical_id: EdgeHandle | str, source_ref: str | None = None) -> None:
        """Mark an edge as retired (soft-delete) by logical ID or handle."""
        self._edge_retires.append(_PendingEdgeRetire(logical_id=logical_id, source_ref=source_ref))

    def add_chunk(
        self,
        *,
        id: str,
        node: NodeHandle | str,
        text_content: str,
        byte_start: int | None = None,
        byte_end: int | None = None,
        content_hash: str | None = None,
    ) -> ChunkHandle:
        """Add a text chunk associated with a node.

        Args:
            id: Unique chunk identifier.
            node: Owning node (handle or logical ID string).
            text_content: The text content of the chunk.
            byte_start: Optional byte offset where the chunk starts in the source.
            byte_end: Optional byte offset where the chunk ends in the source.
            content_hash: Optional hash of the external content this chunk was derived from.

        Returns
        -------
            A ChunkHandle for referencing this chunk in vector inserts.
        """
        handle = ChunkHandle(id=id, _builder_id=self._builder_id)
        self._chunks.append(
            _PendingChunk(
                id=id,
                node=node,
                text_content=text_content,
                byte_start=byte_start,
                byte_end=byte_end,
                content_hash=content_hash,
            )
        )
        return handle

    def add_run(
        self,
        *,
        id: str,
        kind: str,
        status: str,
        properties: Any,
        source_ref: str | None = None,
        upsert: bool = False,
        supersedes_id: str | None = None,
    ) -> RunHandle:
        """Add a run insert to the write request.

        Returns
        -------
            A RunHandle for referencing this run when adding steps.
        """
        handle = RunHandle(id=id, _builder_id=self._builder_id)
        self._runs.append(
            RunInsert(
                id=id,
                kind=kind,
                status=status,
                properties=properties,
                source_ref=source_ref,
                upsert=upsert,
                supersedes_id=supersedes_id,
            )
        )
        return handle

    def add_step(
        self,
        *,
        id: str,
        run: RunHandle | str,
        kind: str,
        status: str,
        properties: Any,
        source_ref: str | None = None,
        upsert: bool = False,
        supersedes_id: str | None = None,
    ) -> StepHandle:
        """Add a step insert belonging to a run.

        Returns
        -------
            A StepHandle for referencing this step when adding actions.
        """
        handle = StepHandle(id=id, _builder_id=self._builder_id)
        self._steps.append(
            _PendingStep(
                id=id,
                run=run,
                kind=kind,
                status=status,
                properties=properties,
                source_ref=source_ref,
                upsert=upsert,
                supersedes_id=supersedes_id,
            )
        )
        return handle

    def add_action(
        self,
        *,
        id: str,
        step: StepHandle | str,
        kind: str,
        status: str,
        properties: Any,
        source_ref: str | None = None,
        upsert: bool = False,
        supersedes_id: str | None = None,
    ) -> ActionHandle:
        """Add an action insert belonging to a step.

        Returns
        -------
            An ActionHandle for referencing this action.
        """
        handle = ActionHandle(id=id, _builder_id=self._builder_id)
        self._actions.append(
            _PendingAction(
                id=id,
                step=step,
                kind=kind,
                status=status,
                properties=properties,
                source_ref=source_ref,
                upsert=upsert,
                supersedes_id=supersedes_id,
            )
        )
        return handle

    def add_optional_backfill(self, target: ProjectionTarget | str, payload: Any) -> None:
        """Queue an optional projection backfill task (e.g. FTS or vector)."""
        value = target if isinstance(target, ProjectionTarget) else ProjectionTarget(target)
        self._optional_backfills.append(OptionalProjectionTask(target=value, payload=payload))

    def add_vec_insert(self, *, chunk: ChunkHandle | str, embedding: list[float]) -> None:
        """Add a vector embedding associated with a chunk."""
        self._vec_inserts.append(_PendingVec(chunk=chunk, embedding=embedding))

    def add_operational_append(
        self,
        *,
        collection: str,
        record_key: str,
        payload_json: Any,
        source_ref: str | None = None,
    ) -> None:
        """Append a mutation to an operational collection."""
        self._operational_writes.append(
            OperationalAppend(
                collection=collection,
                record_key=record_key,
                payload_json=payload_json,
                source_ref=source_ref,
            )
        )

    def add_operational_put(
        self,
        *,
        collection: str,
        record_key: str,
        payload_json: Any,
        source_ref: str | None = None,
    ) -> None:
        """Put (upsert) a record into an operational collection."""
        self._operational_writes.append(
            OperationalPut(
                collection=collection,
                record_key=record_key,
                payload_json=payload_json,
                source_ref=source_ref,
            )
        )

    def add_operational_delete(
        self,
        *,
        collection: str,
        record_key: str,
        source_ref: str | None = None,
    ) -> None:
        """Delete a record from an operational collection."""
        self._operational_writes.append(
            OperationalDelete(
                collection=collection,
                record_key=record_key,
                source_ref=source_ref,
            )
        )

    def build(self) -> WriteRequest:
        """Resolve all handles and produce a finalized WriteRequest.

        Raises
        ------
            BuilderValidationError: If any handle belongs to a different builder.
        """
        return WriteRequest(
            label=self._label,
            nodes=list(self._nodes),
            node_retires=[
                NodeRetire(
                    logical_id=self._resolve_node_ref(item.logical_id),
                    source_ref=item.source_ref,
                )
                for item in self._node_retires
            ],
            edges=[
                EdgeInsert(
                    row_id=item.row_id,
                    logical_id=item.logical_id,
                    source_logical_id=self._resolve_node_ref(item.source),
                    target_logical_id=self._resolve_node_ref(item.target),
                    kind=item.kind,
                    properties=item.properties,
                    source_ref=item.source_ref,
                    upsert=item.upsert,
                )
                for item in self._edges
            ],
            edge_retires=[
                EdgeRetire(
                    logical_id=self._resolve_edge_ref(item.logical_id),
                    source_ref=item.source_ref,
                )
                for item in self._edge_retires
            ],
            chunks=[
                ChunkInsert(
                    id=item.id,
                    node_logical_id=self._resolve_node_ref(item.node),
                    text_content=item.text_content,
                    byte_start=item.byte_start,
                    byte_end=item.byte_end,
                    content_hash=item.content_hash,
                )
                for item in self._chunks
            ],
            runs=list(self._runs),
            steps=[
                StepInsert(
                    id=item.id,
                    run_id=self._resolve_run_ref(item.run),
                    kind=item.kind,
                    status=item.status,
                    properties=item.properties,
                    source_ref=item.source_ref,
                    upsert=item.upsert,
                    supersedes_id=item.supersedes_id,
                )
                for item in self._steps
            ],
            actions=[
                ActionInsert(
                    id=item.id,
                    step_id=self._resolve_step_ref(item.step),
                    kind=item.kind,
                    status=item.status,
                    properties=item.properties,
                    source_ref=item.source_ref,
                    upsert=item.upsert,
                    supersedes_id=item.supersedes_id,
                )
                for item in self._actions
            ],
            optional_backfills=list(self._optional_backfills),
            vec_inserts=[
                VecInsert(
                    chunk_id=self._resolve_chunk_ref(item.chunk),
                    embedding=item.embedding,
                )
                for item in self._vec_inserts
            ],
            operational_writes=list(self._operational_writes),
        )

    def _resolve_node_ref(self, value: NodeHandle | str) -> str:
        if isinstance(value, NodeHandle):
            if value._builder_id != self._builder_id:
                raise BuilderValidationError(
                    "node handle belongs to a different WriteRequestBuilder"
                )
            return value.logical_id
        return value

    def _resolve_edge_ref(self, value: EdgeHandle | str) -> str:
        if isinstance(value, EdgeHandle):
            if value._builder_id != self._builder_id:
                raise BuilderValidationError(
                    "edge handle belongs to a different WriteRequestBuilder"
                )
            return value.logical_id
        return value

    def _resolve_run_ref(self, value: RunHandle | str) -> str:
        if isinstance(value, RunHandle):
            if value._builder_id != self._builder_id:
                raise BuilderValidationError(
                    "run handle belongs to a different WriteRequestBuilder"
                )
            return value.id
        return value

    def _resolve_step_ref(self, value: StepHandle | str) -> str:
        if isinstance(value, StepHandle):
            if value._builder_id != self._builder_id:
                raise BuilderValidationError(
                    "step handle belongs to a different WriteRequestBuilder"
                )
            return value.id
        return value

    def _resolve_chunk_ref(self, value: ChunkHandle | str) -> str:
        if isinstance(value, ChunkHandle):
            if value._builder_id != self._builder_id:
                raise BuilderValidationError(
                    "chunk handle belongs to a different WriteRequestBuilder"
                )
            return value.id
        return value
