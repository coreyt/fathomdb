from __future__ import annotations

import json
import os

from ._feedback import run_with_feedback
from ._fathomdb import EngineCore
from ._types import (
    FeedbackConfig,
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

    def check_integrity(self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None) -> IntegrityReport:
        return IntegrityReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.check_integrity",
                    metadata=None,
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=self._core.check_integrity,
                )
            )
        )

    def check_semantics(self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None) -> SemanticReport:
        return SemanticReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.check_semantics",
                    metadata=None,
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=self._core.check_semantics,
                )
            )
        )

    def rebuild(
        self,
        target: ProjectionTarget | str = ProjectionTarget.ALL,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> ProjectionRepairReport:
        value = target.value if isinstance(target, ProjectionTarget) else target
        return ProjectionRepairReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.rebuild_projections",
                    metadata={"target": value},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.rebuild_projections(value),
                )
            )
        )

    def rebuild_missing(self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None) -> ProjectionRepairReport:
        return ProjectionRepairReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.rebuild_missing_projections",
                    metadata=None,
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=self._core.rebuild_missing_projections,
                )
            )
        )

    def trace_source(
        self,
        source_ref: str,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> TraceReport:
        return TraceReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.trace_source",
                    metadata={"source_ref": source_ref},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.trace_source(source_ref),
                )
            )
        )

    def excise_source(
        self,
        source_ref: str,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> TraceReport:
        return TraceReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.excise_source",
                    metadata={"source_ref": source_ref},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.excise_source(source_ref),
                )
            )
        )

    def safe_export(
        self,
        destination_path: str | os.PathLike[str],
        *,
        force_checkpoint: bool = True,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> SafeExportManifest:
        return SafeExportManifest.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.safe_export",
                    metadata={"destination_path": os.fspath(destination_path)},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.safe_export(
                        os.fspath(destination_path), force_checkpoint
                    ),
                )
            )
        )
