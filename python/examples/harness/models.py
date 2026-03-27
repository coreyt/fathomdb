from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable

from fathomdb import Engine, FeedbackConfig, ProvenanceMode, ResponseCycleEvent


@dataclass(frozen=True)
class ScenarioResult:
    name: str
    details: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class HarnessContext:
    engine: Any
    db_path: Path
    mode: str
    vector_dimension: int
    progress_callback: Callable[[ResponseCycleEvent], None] | None = None
    feedback_config: FeedbackConfig | None = None

    def sibling_db(self, suffix: str) -> Path:
        extension = self.db_path.suffix or ".db"
        return self.db_path.with_name(f"{self.db_path.stem}-{suffix}{extension}")

    def open_engine(
        self,
        database_path: str | Path,
        *,
        provenance_mode: ProvenanceMode | str = ProvenanceMode.WARN,
        vector_dimension: int | None = None,
    ) -> Any:
        from .engine_factory import open_engine

        target_mode = "vector" if vector_dimension is not None else "baseline"
        resolved_vector_dimension = (
            self.vector_dimension if vector_dimension is None else vector_dimension
        )
        return open_engine(
            database_path,
            mode=target_mode,
            vector_dimension=resolved_vector_dimension,
            provenance_mode=provenance_mode,
            progress_callback=self.progress_callback,
            feedback_config=self.feedback_config,
        )


CANONICAL_MEETING_ID = "meeting:q1-budget"
CANONICAL_MEETING_CHUNK_ID = "chunk:meeting:q1-budget:0"
CANONICAL_MEETING_SOURCE = "source:meeting-import"

UPSERT_TASK_ID = "task:follow-up"
UPSERT_SOURCE_V1 = "source:task-follow-up:v1"
UPSERT_SOURCE_V2 = "source:task-follow-up:v2"

GRAPH_TASK_A_ID = "task:graph-a"
GRAPH_TASK_B_ID = "task:graph-b"
GRAPH_EDGE_ID = "edge:graph-a:depends-on:graph-b"
GRAPH_SOURCE = "source:graph-edge"
GRAPH_RETIRE_SOURCE = "source:graph-edge-retire"

RUNTIME_RUN_ID = "run:planner-001"
RUNTIME_STEP_ID = "step:planner-001:1"
RUNTIME_ACTION_ID = "action:planner-001:tool-1"
RUNTIME_ANCHOR_NODE_ID = "document:planner-001-output"
RUNTIME_SOURCE = "source:planner-action-1"

RETIRE_CLEAN_PARENT_ID = "task:retire-clean-parent"
RETIRE_CLEAN_CHILD_ID = "task:retire-clean-child"
RETIRE_CLEAN_EDGE_ID = "edge:retire-clean-parent:depends-on:retire-clean-child"
RETIRE_CLEAN_SOURCE = "source:retire-clean"

RETIRE_DANGLING_PARENT_ID = "task:retire-dangling-parent"
RETIRE_DANGLING_CHILD_ID = "task:retire-dangling-child"
RETIRE_DANGLING_EDGE_ID = "edge:retire-dangling-parent:depends-on:retire-dangling-child"
RETIRE_DANGLING_SOURCE = "source:retire-dangling"

TRACE_MEETING_ID = "meeting:trace-excise"
TRACE_CHUNK_ID = "chunk:meeting:trace-excise:0"
TRACE_SOURCE = "source:trace-excise"

EXPORT_DOCUMENT_ID = "document:export-check"
EXPORT_CHUNK_ID = "chunk:document:export-check:0"
EXPORT_SOURCE = "source:export-check"

VECTOR_DOCUMENT_ID = "document:vector-search"
VECTOR_CHUNK_ID = "chunk:document:vector-search:0"
VECTOR_SOURCE = "source:vector-search"
VECTOR_QUERY = "[0.1, 0.2, 0.3, 0.4]"
