from __future__ import annotations

import json
import os
from pathlib import Path

from ._admin import AdminClient
from ._fathomdb import EngineCore
from ._query import Query
from ._types import ProvenanceMode, WriteReceipt, WriteRequest


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
    ) -> "Engine":
        mode = provenance_mode.value if isinstance(provenance_mode, ProvenanceMode) else provenance_mode
        core = EngineCore.open(os.fspath(Path(database_path)), mode, vector_dimension)
        return cls(core)

    def nodes(self, kind: str) -> Query:
        return Query(self._core, kind)

    def query(self, kind: str) -> Query:
        return self.nodes(kind)

    def write(self, request: WriteRequest) -> WriteReceipt:
        payload = self._core.submit_write(json.dumps(request.to_wire()))
        return WriteReceipt.from_wire(json.loads(payload))

    def submit(self, request: WriteRequest) -> WriteReceipt:
        return self.write(request)
