from __future__ import annotations

import json
import os
from pathlib import Path

from ._admin import AdminClient
from ._feedback import run_with_feedback
from ._fathomdb import EngineCore
from ._query import Query
from ._types import FeedbackConfig, ProvenanceMode, WriteReceipt, WriteRequest


class Engine:
    def __init__(self, core: EngineCore) -> None:
        self._core = core
        self.admin = AdminClient(core)

    @classmethod
    def open(
        cls,
        database_path: str | os.PathLike[str],
        *,
        provenance_mode: ProvenanceMode | str = ProvenanceMode.WARN,
        vector_dimension: int | None = None,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> "Engine":
        mode = provenance_mode.value if isinstance(provenance_mode, ProvenanceMode) else provenance_mode
        path = os.fspath(Path(database_path))
        core = run_with_feedback(
            surface="python",
            operation_kind="engine.open",
            metadata={"database_path": path},
            progress_callback=progress_callback,
            feedback_config=feedback_config,
            operation=lambda: EngineCore.open(path, mode, vector_dimension),
        )
        return cls(core)

    def nodes(self, kind: str) -> Query:
        return Query(self._core, kind)

    def query(self, kind: str) -> Query:
        return self.nodes(kind)

    def write(
        self,
        request: WriteRequest,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> WriteReceipt:
        payload = run_with_feedback(
            surface="python",
            operation_kind="write.submit",
            metadata={"label": request.label},
            progress_callback=progress_callback,
            feedback_config=feedback_config,
            operation=lambda: self._core.submit_write(json.dumps(request.to_wire())),
        )
        return WriteReceipt.from_wire(json.loads(payload))

    def submit(
        self,
        request: WriteRequest,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> WriteReceipt:
        return self.write(
            request,
            progress_callback=progress_callback,
            feedback_config=feedback_config,
        )
