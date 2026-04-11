from __future__ import annotations

import json
import os

from ._feedback import run_with_feedback
from ._fathomdb import EngineCore
from ._types import (
    FeedbackConfig,
    FtsPropertySchemaRecord,
    IntegrityReport,
    LogicalPurgeReport,
    LogicalRestoreReport,
    OperationalCollectionRecord,
    OperationalCompactionReport,
    OperationalHistoryValidationReport,
    OperationalPurgeReport,
    OperationalReadReport,
    OperationalReadRequest,
    OperationalRegisterRequest,
    OperationalRepairReport,
    OperationalRetentionPlanReport,
    OperationalRetentionRunReport,
    OperationalSecondaryIndexRebuildReport,
    OperationalTraceReport,
    ProjectionRepairReport,
    ProjectionTarget,
    ProvenancePurgeReport,
    SafeExportManifest,
    SemanticReport,
    TraceReport,
    VectorGeneratorPolicy,
    VectorRegenerationConfig,
    VectorRegenerationReport,
)


class AdminClient:
    """Administrative operations for a fathomdb database.

    Provides integrity checks, projection rebuilds, source tracing, safe
    exports, and operational collection management.  Accessed via
    :attr:`Engine.admin`.
    """

    def __init__(self, core: EngineCore) -> None:
        self._core = core

    def check_integrity(self, *, progress_callback=None, feedback_config: FeedbackConfig | None = None) -> IntegrityReport:
        """Run physical and logical integrity checks on the database."""
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
        """Run semantic validation (orphan chunks, dangling edges, etc.)."""
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
        """Rebuild projection indexes (FTS, vector, or all).

        Args:
            target: Which projections to rebuild ("fts", "vec", or "all").
            progress_callback: Optional callback invoked with feedback events.
            feedback_config: Timing thresholds for progress feedback.
        """
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
        """Rebuild only missing projection rows without touching existing ones."""
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

    # ── Vector profile management ──────────────────────────────────

    def restore_vector_profiles(
        self,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> ProjectionRepairReport:
        """Restore vector profile metadata from the database schema."""
        return ProjectionRepairReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.restore_vector_profiles",
                    metadata=None,
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=self._core.restore_vector_profiles,
                )
            )
        )

    def regenerate_vector_embeddings(
        self,
        config: VectorRegenerationConfig,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> VectorRegenerationReport:
        """Regenerate vector embeddings using the supplied configuration.

        Args:
            config: Regeneration configuration specifying the profile, model,
                and generator command.
            progress_callback: Optional callback invoked with feedback events.
            feedback_config: Timing thresholds for progress feedback.
        """
        return VectorRegenerationReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.regenerate_vector_embeddings",
                    metadata={"profile": config.profile, "table_name": config.table_name},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.regenerate_vector_embeddings(
                        json.dumps(config.to_wire())
                    ),
                )
            )
        )

    def regenerate_vector_embeddings_with_policy(
        self,
        config: VectorRegenerationConfig,
        policy: VectorGeneratorPolicy,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> VectorRegenerationReport:
        """Regenerate vector embeddings with explicit generator policy.

        Args:
            config: Regeneration configuration specifying the profile, model,
                and generator command.
            policy: Security and resource limits for the generator subprocess.
            progress_callback: Optional callback invoked with feedback events.
            feedback_config: Timing thresholds for progress feedback.
        """
        return VectorRegenerationReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.regenerate_vector_embeddings_with_policy",
                    metadata={"profile": config.profile, "table_name": config.table_name},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.regenerate_vector_embeddings_with_policy(
                        json.dumps(config.to_wire()), json.dumps(policy.to_wire())
                    ),
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
        """Trace all nodes, edges, and actions originating from a source reference."""
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
        """Remove all data originating from a source reference."""
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
        """Restore a previously retired node by its logical ID."""
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
        """Permanently delete all rows associated with a logical ID."""
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
        """Export a consistent snapshot of the database to *destination_path*.

        Args:
            destination_path: Filesystem path for the exported database file.
            force_checkpoint: Whether to force a WAL checkpoint before export.
            progress_callback: Optional callback invoked with feedback events.
            feedback_config: Timing thresholds for progress feedback.
        """
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

    # ── FTS property schema management ───────────────────────────────

    def register_fts_property_schema(
        self,
        kind: str,
        property_paths: list[str],
        separator: str | None = None,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> FtsPropertySchemaRecord:
        """Register (or update) an FTS property projection schema for a node kind.

        After registration, nodes of this kind will have the declared JSON
        property paths extracted, concatenated with the separator, and indexed
        for full-text search. ``text_search(...)`` transparently covers both
        chunk-backed and property-backed results.

        This is an idempotent upsert: calling it again with different paths or
        separator overwrites the previous schema. Registration does **not**
        rewrite existing FTS rows; call ``rebuild("fts")`` to backfill.

        Paths must use simple ``$.``-prefixed dot-notation (e.g. ``$.title``,
        ``$.address.city``). Array indexing, wildcards, recursive descent, and
        duplicate paths are rejected.

        Args:
            kind: Node kind to register (e.g. ``"Goal"``).
            property_paths: Ordered list of JSON paths to extract.
            separator: Concatenation separator (default ``" "``).
        """
        return FtsPropertySchemaRecord.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.register_fts_property_schema",
                    metadata={"kind": kind},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.register_fts_property_schema(
                        kind, json.dumps(property_paths), separator
                    ),
                )
            )
        )

    def describe_fts_property_schema(
        self,
        kind: str,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> FtsPropertySchemaRecord | None:
        """Return the FTS property schema for a node kind, or None if not registered."""
        payload = json.loads(
            run_with_feedback(
                surface="python",
                operation_kind="admin.describe_fts_property_schema",
                metadata={"kind": kind},
                progress_callback=progress_callback,
                feedback_config=feedback_config,
                operation=lambda: self._core.describe_fts_property_schema(kind),
            )
        )
        if payload is None or payload.get("kind") is None:
            return None
        return FtsPropertySchemaRecord.from_wire(payload)

    def list_fts_property_schemas(
        self,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> list[FtsPropertySchemaRecord]:
        """Return all registered FTS property schemas."""
        payload = json.loads(
            run_with_feedback(
                surface="python",
                operation_kind="admin.list_fts_property_schemas",
                metadata={},
                progress_callback=progress_callback,
                feedback_config=feedback_config,
                operation=lambda: self._core.list_fts_property_schemas(),
            )
        )
        return [FtsPropertySchemaRecord.from_wire(item) for item in payload]

    def remove_fts_property_schema(
        self,
        kind: str,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> None:
        """Remove the FTS property schema for a node kind.

        This deletes the schema row but does **not** delete existing derived
        ``fts_node_properties`` rows. An explicit ``rebuild("fts")`` is
        required to clean up stale rows after removal.

        Raises ``EngineError`` if the kind is not registered.
        """
        run_with_feedback(
            surface="python",
            operation_kind="admin.remove_fts_property_schema",
            metadata={"kind": kind},
            progress_callback=progress_callback,
            feedback_config=feedback_config,
            operation=lambda: self._core.remove_fts_property_schema(kind),
        )

    # ── Operational collection lifecycle ──────────────────────────────

    def register_operational_collection(
        self,
        request: OperationalRegisterRequest,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalCollectionRecord:
        """Register a new operational collection with the given schema and retention."""
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
        """Return the record for a named operational collection, or None if not found."""
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

    def update_operational_collection_filters(
        self,
        name: str,
        filter_fields_json: str,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalCollectionRecord:
        """Replace the filter field definitions for an operational collection."""
        return OperationalCollectionRecord.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.update_operational_collection_filters",
                    metadata={"name": name},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.update_operational_collection_filters(
                        name, filter_fields_json
                    ),
                )
            )
        )

    def update_operational_collection_validation(
        self,
        name: str,
        validation_json: str,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalCollectionRecord:
        """Replace the validation rules for an operational collection."""
        return OperationalCollectionRecord.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.update_operational_collection_validation",
                    metadata={"name": name},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.update_operational_collection_validation(
                        name, validation_json
                    ),
                )
            )
        )

    def update_operational_collection_secondary_indexes(
        self,
        name: str,
        secondary_indexes_json: str,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalCollectionRecord:
        """Replace the secondary index definitions for an operational collection."""
        return OperationalCollectionRecord.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.update_operational_collection_secondary_indexes",
                    metadata={"name": name},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.update_operational_collection_secondary_indexes(
                        name, secondary_indexes_json
                    ),
                )
            )
        )

    def trace_operational_collection(
        self,
        collection_name: str,
        record_key: str | None = None,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalTraceReport:
        """Return mutation and current-state rows for an operational collection.

        Args:
            collection_name: Name of the operational collection to trace.
            record_key: Optional key to narrow the trace to a single record.
            progress_callback: Optional callback invoked with feedback events.
            feedback_config: Timing thresholds for progress feedback.
        """
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

    def read_operational_collection(
        self,
        request: OperationalReadRequest,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalReadReport:
        """Read filtered mutation rows from an operational collection."""
        return OperationalReadReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.read_operational_collection",
                    metadata={"collection_name": request.collection_name},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.read_operational_collection(
                        json.dumps(request.to_wire())
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
        """Rebuild the current-state view for one or all operational collections."""
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

    def validate_operational_collection_history(
        self,
        collection_name: str,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalHistoryValidationReport:
        """Validate the mutation history of an operational collection for consistency."""
        return OperationalHistoryValidationReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.validate_operational_collection_history",
                    metadata={"collection_name": collection_name},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.validate_operational_collection_history(
                        collection_name
                    ),
                )
            )
        )

    def rebuild_operational_secondary_indexes(
        self,
        collection_name: str,
        *,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalSecondaryIndexRebuildReport:
        """Rebuild secondary indexes for an operational collection."""
        return OperationalSecondaryIndexRebuildReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.rebuild_operational_secondary_indexes",
                    metadata={"collection_name": collection_name},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.rebuild_operational_secondary_indexes(
                        collection_name
                    ),
                )
            )
        )

    def plan_operational_retention(
        self,
        now_timestamp: int,
        *,
        collection_names: list[str] | None = None,
        max_collections: int | None = None,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalRetentionPlanReport:
        """Preview which mutations would be purged by the retention policy.

        Args:
            now_timestamp: Reference timestamp (epoch seconds) for retention evaluation.
            collection_names: Limit planning to these collections, or None for all.
            max_collections: Cap the number of collections evaluated.
            progress_callback: Optional callback invoked with feedback events.
            feedback_config: Timing thresholds for progress feedback.
        """
        return OperationalRetentionPlanReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.plan_operational_retention",
                    metadata={"now_timestamp": str(now_timestamp)},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.plan_operational_retention(
                        now_timestamp, collection_names, max_collections
                    ),
                )
            )
        )

    def run_operational_retention(
        self,
        now_timestamp: int,
        *,
        collection_names: list[str] | None = None,
        max_collections: int | None = None,
        dry_run: bool = False,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> OperationalRetentionRunReport:
        """Execute the retention policy, deleting expired mutations.

        Args:
            now_timestamp: Reference timestamp (epoch seconds) for retention evaluation.
            collection_names: Limit execution to these collections, or None for all.
            max_collections: Cap the number of collections processed.
            dry_run: If True, report what would be deleted without modifying data.
            progress_callback: Optional callback invoked with feedback events.
            feedback_config: Timing thresholds for progress feedback.
        """
        return OperationalRetentionRunReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.run_operational_retention",
                    metadata={"now_timestamp": str(now_timestamp), "dry_run": str(dry_run).lower()},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.run_operational_retention(
                        now_timestamp, collection_names, max_collections, dry_run
                    ),
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
        """Disable an operational collection, preventing new writes."""
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
        """Compact an operational collection by removing superseded mutations.

        Args:
            name: Name of the operational collection.
            dry_run: If True, report what would be compacted without modifying data.
            progress_callback: Optional callback invoked with feedback events.
            feedback_config: Timing thresholds for progress feedback.
        """
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
        """Delete all mutations older than *before_timestamp* from a collection.

        Args:
            name: Name of the operational collection.
            before_timestamp: Epoch-seconds cutoff; mutations before this are deleted.
            progress_callback: Optional callback invoked with feedback events.
            feedback_config: Timing thresholds for progress feedback.
        """
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

    def purge_provenance_events(
        self,
        before_timestamp: int,
        *,
        dry_run: bool = False,
        preserve_event_types: list[str] | None = None,
        progress_callback=None,
        feedback_config: FeedbackConfig | None = None,
    ) -> ProvenancePurgeReport:
        """Delete provenance events older than *before_timestamp*.

        Args:
            before_timestamp: Epoch-seconds cutoff; events before this are deleted.
            dry_run: If True, report what would be purged without modifying data.
            preserve_event_types: Event types to keep regardless of age.
            progress_callback: Optional callback invoked with feedback events.
            feedback_config: Timing thresholds for progress feedback.
        """
        options = {
            "dry_run": dry_run,
            "preserve_event_types": preserve_event_types or [],
        }
        return ProvenancePurgeReport.from_wire(
            json.loads(
                run_with_feedback(
                    surface="python",
                    operation_kind="admin.purge_provenance_events",
                    metadata={"before_timestamp": str(before_timestamp)},
                    progress_callback=progress_callback,
                    feedback_config=feedback_config,
                    operation=lambda: self._core.purge_provenance_events(
                        before_timestamp, json.dumps(options)
                    ),
                )
            )
        )
