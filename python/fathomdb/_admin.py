from __future__ import annotations

import json
import os

from ._fathomdb import EngineCore
from ._types import (
    IntegrityReport,
    ProjectionRepairReport,
    ProjectionTarget,
    SafeExportManifest,
    SemanticReport,
    TraceReport,
)


class AdminClient:
    def __init__(self, core: EngineCore) -> None:
        self._core = core

    def check_integrity(self) -> IntegrityReport:
        return IntegrityReport.from_wire(json.loads(self._core.check_integrity()))

    def check_semantics(self) -> SemanticReport:
        return SemanticReport.from_wire(json.loads(self._core.check_semantics()))

    def rebuild(self, target: ProjectionTarget | str = ProjectionTarget.ALL) -> ProjectionRepairReport:
        value = target.value if isinstance(target, ProjectionTarget) else target
        return ProjectionRepairReport.from_wire(
            json.loads(self._core.rebuild_projections(value))
        )

    def rebuild_missing(self) -> ProjectionRepairReport:
        return ProjectionRepairReport.from_wire(
            json.loads(self._core.rebuild_missing_projections())
        )

    def trace_source(self, source_ref: str) -> TraceReport:
        return TraceReport.from_wire(json.loads(self._core.trace_source(source_ref)))

    def excise_source(self, source_ref: str) -> TraceReport:
        return TraceReport.from_wire(json.loads(self._core.excise_source(source_ref)))

    def safe_export(
        self,
        destination_path: str | os.PathLike[str],
        *,
        force_checkpoint: bool = True,
    ) -> SafeExportManifest:
        return SafeExportManifest.from_wire(
            json.loads(
                self._core.safe_export(os.fspath(destination_path), force_checkpoint)
            )
        )
