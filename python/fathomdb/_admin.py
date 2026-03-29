from __future__ import annotations

import json
import os

from ._feedback import run_with_feedback
from ._fathomdb import EngineCore
from ._types import (
    FeedbackConfig,
    IntegrityReport,
    LogicalPurgeReport,
    LogicalRestoreReport,
    OperationalCollectionRecord,
    OperationalCompactionReport,
    OperationalPurgeReport,
    OperationalRegisterRequest,
    OperationalRepairReport,
    OperationalTraceReport,
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

    def restore_logical_id(
        self,
        logical_id: str,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> LogicalRestoreReport:
        return LogicalRestoreReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.restore_logical_id",
                    metadata={"logical_id": logical_id},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.restore_logical_id(logical_id),
                )
            )
        )

    def purge_logical_id(
        self,
        logical_id: str,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> LogicalPurgeReport:
        return LogicalPurgeReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.purge_logical_id",
                    metadata={"logical_id": logical_id},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.purge_logical_id(logical_id),
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

    def register_operational_collection(
        self,
        request: OperationalRegisterRequest,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalCollectionRecord:
        return OperationalCollectionRecord.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.register_operational_collection",
                    metadata={"name": request.name, "kind": request.kind.value},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.register_operational_collection(
                        json.dumps(request.to_wire())
                    ),
                )
            )
        )

    def describe_operational_collection(
        self,
        name: str,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalCollectionRecord | None:
        payload = json.loads(
            run_with_feedback(
                surface="python",
                operation_kind="admin.describe_operational_collection",
                metadata={"name": name},
                progress_callback=progress_callback,
                feedback_config=feedback_config,
                operation=lambda: self._core.describe_operational_collection(name),
            )
        )
        if payload is None:
            return None
        return OperationalCollectionRecord.from_wire(payload)

    def trace_operational_collection(
        self,
        collection_name: str,
        record_key: str | None = None,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalTraceReport:
        metadata = {"collection_name": collection_name}
        if record_key is not None:
            metadata["record_key"] = record_key
        return OperationalTraceReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.trace_operational_collection",
                    metadata=metadata,
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.trace_operational_collection(
                        collection_name, record_key
                    ),
                )
            )
        )

    def rebuild_operational_current(
        self,
        collection_name: str | None = None,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalRepairReport:
        metadata = None if collection_name is None else {"collection_name": collection_name}
        return OperationalRepairReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.rebuild_operational_current",
                    metadata=metadata,
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.rebuild_operational_current(collection_name),
                )
            )
        )

    def disable_operational_collection(
        self,
        name: str,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalCollectionRecord:
        return OperationalCollectionRecord.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.disable_operational_collection",
                    metadata={"name": name},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.disable_operational_collection(name),
                )
            )
        )

    def compact_operational_collection(
        self,
        name: str,
        *,
        dry_run: bool,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalCompactionReport:
        return OperationalCompactionReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.compact_operational_collection",
                    metadata={"name": name, "dry_run": str(dry_run).lower()},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.compact_operational_collection(name, dry_run),
                )
            )
        )

    def purge_operational_collection(
        self,
        name: str,
        *,
        before_timestamp: int,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalPurgeReport:
        return OperationalPurgeReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.purge_operational_collection",
                    metadata={"name": name, "before_timestamp": str(before_timestamp)},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.purge_operational_collection(
                        name, before_timestamp
                    ),
                )
            )
        )
