use std::path::{Path, PathBuf};

mod feedback;
#[cfg(feature = "python")]
mod python;
#[cfg(feature = "python")]
mod python_types;
mod write_request_builder;

pub use fathomdb_engine::{
    ActionInsert, ActionRow, AdminHandle, ChunkInsert, ChunkPolicy, EdgeInsert, EdgeRetire,
    EngineError, EngineRuntime, ExecutionCoordinator, ExpansionRootRows, ExpansionSlotRows,
    GroupedQueryRows, LastAccessTouchReport, LastAccessTouchRequest, LogicalPurgeReport,
    LogicalRestoreReport, NodeInsert, NodeRetire, NodeRow, OperationalCollectionKind,
    OperationalCollectionRecord, OperationalCompactionReport, OperationalCurrentRow,
    OperationalFilterClause, OperationalFilterField, OperationalFilterFieldType,
    OperationalFilterMode, OperationalFilterValue, OperationalHistoryValidationIssue,
    OperationalHistoryValidationReport, OperationalMutationRow, OperationalPurgeReport,
    OperationalReadReport, OperationalReadRequest, OperationalRegisterRequest,
    OperationalRepairReport, OperationalRetentionActionKind, OperationalRetentionPlanItem,
    OperationalRetentionPlanReport, OperationalRetentionRunItem, OperationalRetentionRunReport,
    OperationalSecondaryIndexDefinition, OperationalSecondaryIndexField,
    OperationalSecondaryIndexRebuildReport, OperationalSecondaryIndexValueType,
    OperationalTraceReport, OperationalValidationContract, OperationalValidationField,
    OperationalValidationFieldType, OperationalValidationMode, OperationalWrite,
    OptionalProjectionTask, ProjectionRepairReport, ProjectionTarget, ProvenanceEvent,
    ProvenanceMode, ProvenancePurgeOptions, ProvenancePurgeReport, QueryPlan, QueryRows, RunInsert,
    RunRow, SafeExportManifest, SafeExportOptions, SkippedEdge, StepInsert, StepRow, VecInsert,
    WriteReceipt, WriteRequest, WriterActor, new_id, new_row_id,
};
pub use fathomdb_engine::{SqliteCacheStatus, TelemetryLevel, TelemetrySnapshot};
pub use fathomdb_query::{
    BindValue, ComparisonOp, CompileError, CompiledGroupedQuery, CompiledQuery, DrivingTable,
    ExecutionHints, ExpansionSlot, Predicate, Query, QueryAst, QueryBuilder, QueryStep,
    ScalarValue, ShapeHash, TraverseDirection, compile_grouped_query, compile_query,
};
pub use fathomdb_schema::{BootstrapReport, Migration, SchemaManager, SchemaVersion};
pub use feedback::{FeedbackConfig, OperationObserver, ResponseCycleEvent, ResponseCyclePhase};
pub use write_request_builder::{
    ActionHandle, ChunkHandle, ChunkRef, EdgeHandle, EdgeRef, NodeHandle, NodeRef, RunHandle,
    RunRef, StepHandle, StepRef, WriteRequestBuilder,
};

use std::collections::BTreeMap;

use feedback::{OperationContext, run_with_feedback};

/// Configuration for opening an [`Engine`] instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineOptions {
    /// Filesystem path to the `SQLite` database file.
    pub database_path: PathBuf,
    /// Controls enforcement of `source_ref` provenance on writes.
    pub provenance_mode: ProvenanceMode,
    /// When `Some(dim)`, the engine opens a vector-capable connection and
    /// bootstraps a `vec_nodes_active` vector table with the given dimension.
    /// Requires the `sqlite-vec` crate feature; ignored if the feature is absent.
    pub vector_dimension: Option<usize>,
    /// Number of read-only `SQLite` connections in the reader pool.
    /// Defaults to 4 when `None`.
    pub read_pool_size: Option<usize>,
    /// Controls how much telemetry the engine collects.
    /// Defaults to [`TelemetryLevel::Counters`] (always-on cumulative counters).
    pub telemetry_level: TelemetryLevel,
}

impl EngineOptions {
    /// Create default engine options pointing at the given database path.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            database_path: path.as_ref().to_path_buf(),
            provenance_mode: ProvenanceMode::Warn,
            vector_dimension: None,
            read_pool_size: None,
            telemetry_level: TelemetryLevel::Counters,
        }
    }
}

/// Top-level handle to a fathomdb graph database.
///
/// An [`Engine`] owns the underlying `SQLite` connections, writer thread, and
/// read pool. Create one via [`Engine::open`] or [`Engine::open_with_feedback`].
#[derive(Debug)]
pub struct Engine {
    runtime: EngineRuntime,
}

#[allow(clippy::missing_errors_doc)]
impl Engine {
    /// Open a fathomdb engine with the given options.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the database cannot be opened or the schema
    /// bootstrap fails.
    pub fn open(options: EngineOptions) -> Result<Self, EngineError> {
        Ok(Self {
            runtime: EngineRuntime::open(
                options.database_path,
                options.provenance_mode,
                options.vector_dimension,
                options.read_pool_size.unwrap_or(4),
                options.telemetry_level,
            )?,
        })
    }

    /// Open a fathomdb engine, emitting feedback events to the observer.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the database cannot be opened or the schema
    /// bootstrap fails.
    pub fn open_with_feedback(
        options: EngineOptions,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<Self, EngineError> {
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "database_path".to_owned(),
            options.database_path.display().to_string(),
        );
        run_with_feedback(
            OperationContext {
                surface: "rust",
                operation_kind: "engine.open",
            },
            metadata,
            Some(observer),
            config,
            engine_error_code,
            || Self::open(options),
        )
    }

    /// Start building a node query for the given kind.
    pub fn query(&self, kind: impl Into<String>) -> QueryBuilder {
        QueryBuilder::nodes(kind)
    }

    /// Returns a handle to the administrative service.
    pub fn admin(&self) -> &AdminHandle {
        self.runtime.admin()
    }

    /// Returns a handle to the single-threaded writer actor.
    pub fn writer(&self) -> &WriterActor {
        self.runtime.writer()
    }

    /// Returns the read-side execution coordinator.
    pub fn coordinator(&self) -> &ExecutionCoordinator {
        self.runtime.coordinator()
    }

    /// Read all telemetry counters and aggregated SQLite cache statistics.
    #[must_use]
    pub fn telemetry_snapshot(&self) -> TelemetrySnapshot {
        self.runtime.telemetry_snapshot()
    }

    /// Update `last_accessed_at` timestamps for a batch of logical IDs.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the writer rejects the request or the
    /// underlying `SQLite` transaction fails.
    pub fn touch_last_accessed(
        &self,
        request: LastAccessTouchRequest,
    ) -> Result<LastAccessTouchReport, EngineError> {
        self.writer().touch_last_accessed(request)
    }

    /// Register a new operational collection.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the collection cannot be created.
    pub fn register_operational_collection(
        &self,
        request: &OperationalRegisterRequest,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        self.admin()
            .service()
            .register_operational_collection(request)
    }

    /// Look up metadata for an operational collection by name.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn describe_operational_collection(
        &self,
        name: &str,
    ) -> Result<Option<OperationalCollectionRecord>, EngineError> {
        self.admin().service().describe_operational_collection(name)
    }

    /// Replace the filter field definitions for an operational collection.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the collection does not exist or the JSON is invalid.
    pub fn update_operational_collection_filters(
        &self,
        name: &str,
        filter_fields_json: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        self.admin()
            .service()
            .update_operational_collection_filters(name, filter_fields_json)
    }

    /// Replace the validation contract for an operational collection.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the collection does not exist or the JSON is invalid.
    pub fn update_operational_collection_validation(
        &self,
        name: &str,
        validation_json: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        self.admin()
            .service()
            .update_operational_collection_validation(name, validation_json)
    }

    /// Replace the secondary index definitions for an operational collection.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the collection does not exist or the JSON is invalid.
    pub fn update_operational_collection_secondary_indexes(
        &self,
        name: &str,
        secondary_indexes_json: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        self.admin()
            .service()
            .update_operational_collection_secondary_indexes(name, secondary_indexes_json)
    }

    /// Return the mutation history for an operational collection, optionally filtered to a single record key.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn trace_operational_collection(
        &self,
        collection_name: &str,
        record_key: Option<&str>,
    ) -> Result<OperationalTraceReport, EngineError> {
        self.admin()
            .service()
            .trace_operational_collection(collection_name, record_key)
    }

    /// Read current-state rows from an operational collection.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn read_operational_collection(
        &self,
        request: &OperationalReadRequest,
    ) -> Result<OperationalReadReport, EngineError> {
        self.admin().service().read_operational_collection(request)
    }

    /// Rebuild the `operational_current` materialized view, optionally scoped to one collection.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn rebuild_operational_current(
        &self,
        collection_name: Option<&str>,
    ) -> Result<OperationalRepairReport, EngineError> {
        self.admin()
            .service()
            .rebuild_operational_current(collection_name)
    }

    /// Validate the mutation history of an operational collection against its contract.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn validate_operational_collection_history(
        &self,
        collection_name: &str,
    ) -> Result<OperationalHistoryValidationReport, EngineError> {
        self.admin()
            .service()
            .validate_operational_collection_history(collection_name)
    }

    /// Drop and recreate secondary index entries for an operational collection.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn rebuild_operational_secondary_indexes(
        &self,
        collection_name: &str,
    ) -> Result<OperationalSecondaryIndexRebuildReport, EngineError> {
        self.admin()
            .service()
            .rebuild_operational_secondary_indexes(collection_name)
    }

    /// Compute a retention plan for operational collections without applying it.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn plan_operational_retention(
        &self,
        now_timestamp: i64,
        collection_names: Option<&[String]>,
        max_collections: Option<usize>,
    ) -> Result<OperationalRetentionPlanReport, EngineError> {
        self.admin().service().plan_operational_retention(
            now_timestamp,
            collection_names,
            max_collections,
        )
    }

    /// Execute the retention plan for operational collections, deleting expired mutations.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn run_operational_retention(
        &self,
        now_timestamp: i64,
        collection_names: Option<&[String]>,
        max_collections: Option<usize>,
        dry_run: bool,
    ) -> Result<OperationalRetentionRunReport, EngineError> {
        self.admin().service().run_operational_retention(
            now_timestamp,
            collection_names,
            max_collections,
            dry_run,
        )
    }

    /// Mark an operational collection as disabled, preventing future mutations.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the collection does not exist or cannot be updated.
    pub fn disable_operational_collection(
        &self,
        name: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        self.admin().service().disable_operational_collection(name)
    }

    /// Compact an operational collection by merging superseded mutation rows.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the collection does not exist or on database failure.
    pub fn compact_operational_collection(
        &self,
        name: &str,
        dry_run: bool,
    ) -> Result<OperationalCompactionReport, EngineError> {
        self.admin()
            .service()
            .compact_operational_collection(name, dry_run)
    }

    /// Permanently delete mutations older than the given timestamp from an operational collection.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the collection does not exist or on database failure.
    pub fn purge_operational_collection(
        &self,
        name: &str,
        before_timestamp: i64,
    ) -> Result<OperationalPurgeReport, EngineError> {
        self.admin()
            .service()
            .purge_operational_collection(name, before_timestamp)
    }

    /// Restore a previously retired node and its associated edges by logical ID.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn restore_logical_id(
        &self,
        logical_id: &str,
    ) -> Result<LogicalRestoreReport, EngineError> {
        self.runtime.telemetry().increment_admin_ops();
        self.admin().service().restore_logical_id(logical_id)
    }

    /// Permanently delete all rows associated with a logical ID (nodes, edges, chunks, FTS, vec).
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn purge_logical_id(&self, logical_id: &str) -> Result<LogicalPurgeReport, EngineError> {
        self.runtime.telemetry().increment_admin_ops();
        self.admin().service().purge_logical_id(logical_id)
    }

    /// Delete provenance events older than the given timestamp.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn purge_provenance_events(
        &self,
        before_timestamp: i64,
        options: &ProvenancePurgeOptions,
    ) -> Result<ProvenancePurgeReport, EngineError> {
        self.runtime.telemetry().increment_admin_ops();
        self.admin()
            .service()
            .purge_provenance_events(before_timestamp, options)
    }

    /// Return the execution plan for a compiled query, with feedback.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn explain_compiled_query_with_feedback(
        &self,
        compiled: &CompiledQuery,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<QueryPlan, EngineError> {
        let mut metadata = BTreeMap::new();
        metadata.insert("shape_hash".to_owned(), compiled.shape_hash.0.to_string());
        run_with_feedback(
            OperationContext {
                surface: "rust",
                operation_kind: "query.explain",
            },
            metadata,
            Some(observer),
            config,
            engine_error_code,
            || Ok(self.coordinator().explain_compiled_read(compiled)),
        )
    }

    /// Execute a compiled query and return matching rows, with feedback.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn execute_compiled_query_with_feedback(
        &self,
        compiled: &CompiledQuery,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<QueryRows, EngineError> {
        let mut metadata = BTreeMap::new();
        metadata.insert("shape_hash".to_owned(), compiled.shape_hash.0.to_string());
        run_with_feedback(
            OperationContext {
                surface: "rust",
                operation_kind: "query.execute",
            },
            metadata,
            Some(observer),
            config,
            engine_error_code,
            || self.coordinator().execute_compiled_read(compiled),
        )
    }

    /// Execute a compiled grouped query and return root rows plus expansion slots, with feedback.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn execute_compiled_grouped_query_with_feedback(
        &self,
        compiled: &CompiledGroupedQuery,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<GroupedQueryRows, EngineError> {
        let mut metadata = BTreeMap::new();
        metadata.insert("shape_hash".to_owned(), compiled.shape_hash.0.to_string());
        run_with_feedback(
            OperationContext {
                surface: "rust",
                operation_kind: "query.execute_grouped",
            },
            metadata,
            Some(observer),
            config,
            engine_error_code,
            || self.coordinator().execute_compiled_grouped_read(compiled),
        )
    }

    /// Submit a write request to the writer actor, with feedback.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the write is invalid or the transaction fails.
    pub fn submit_write_with_feedback(
        &self,
        request: WriteRequest,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<WriteReceipt, EngineError> {
        let mut metadata = BTreeMap::new();
        metadata.insert("label".to_owned(), request.label.clone());
        run_with_feedback(
            OperationContext {
                surface: "rust",
                operation_kind: "write.submit",
            },
            metadata,
            Some(observer),
            config,
            engine_error_code,
            || self.writer().submit(request),
        )
    }

    /// Run `SQLite` integrity and structural consistency checks, with feedback.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn check_integrity_with_feedback(
        &self,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<fathomdb_engine::IntegrityReport, EngineError> {
        self.runtime.telemetry().increment_admin_ops();
        run_with_feedback(
            OperationContext {
                surface: "rust",
                operation_kind: "admin.check_integrity",
            },
            BTreeMap::new(),
            Some(observer),
            config,
            engine_error_code,
            || self.admin().service().check_integrity(),
        )
    }

    /// Run semantic consistency checks (orphaned chunks, dangling edges, etc.), with feedback.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn check_semantics_with_feedback(
        &self,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<fathomdb_engine::SemanticReport, EngineError> {
        self.runtime.telemetry().increment_admin_ops();
        run_with_feedback(
            OperationContext {
                surface: "rust",
                operation_kind: "admin.check_semantics",
            },
            BTreeMap::new(),
            Some(observer),
            config,
            engine_error_code,
            || self.admin().service().check_semantics(),
        )
    }

    /// Rebuild projection tables (FTS, vec) for a given target, with feedback.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn rebuild_projections_with_feedback(
        &self,
        target: ProjectionTarget,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<ProjectionRepairReport, EngineError> {
        self.runtime.telemetry().increment_admin_ops();
        let mut metadata = BTreeMap::new();
        metadata.insert("target".to_owned(), format!("{target:?}").to_lowercase());
        run_with_feedback(
            OperationContext {
                surface: "rust",
                operation_kind: "admin.rebuild_projections",
            },
            metadata,
            Some(observer),
            config,
            engine_error_code,
            || self.admin().service().rebuild_projections(target),
        )
    }

    /// Rebuild only missing projection rows (FTS, vec), with feedback.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn rebuild_missing_projections_with_feedback(
        &self,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<ProjectionRepairReport, EngineError> {
        self.runtime.telemetry().increment_admin_ops();
        run_with_feedback(
            OperationContext {
                surface: "rust",
                operation_kind: "admin.rebuild_missing_projections",
            },
            BTreeMap::new(),
            Some(observer),
            config,
            engine_error_code,
            || self.admin().service().rebuild_missing_projections(),
        )
    }

    /// List all rows associated with a `source_ref`, with feedback.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn trace_source_with_feedback(
        &self,
        source_ref: &str,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<fathomdb_engine::TraceReport, EngineError> {
        self.runtime.telemetry().increment_admin_ops();
        let mut metadata = BTreeMap::new();
        metadata.insert("source_ref".to_owned(), source_ref.to_owned());
        run_with_feedback(
            OperationContext {
                surface: "rust",
                operation_kind: "admin.trace_source",
            },
            metadata,
            Some(observer),
            config,
            engine_error_code,
            || self.admin().service().trace_source(source_ref),
        )
    }

    /// Delete all rows associated with a `source_ref`, with feedback.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn excise_source_with_feedback(
        &self,
        source_ref: &str,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<fathomdb_engine::TraceReport, EngineError> {
        self.runtime.telemetry().increment_admin_ops();
        let mut metadata = BTreeMap::new();
        metadata.insert("source_ref".to_owned(), source_ref.to_owned());
        run_with_feedback(
            OperationContext {
                surface: "rust",
                operation_kind: "admin.excise_source",
            },
            metadata,
            Some(observer),
            config,
            engine_error_code,
            || self.admin().service().excise_source(source_ref),
        )
    }

    /// Export the database to a new file at `destination_path`, with feedback.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the checkpoint or file copy fails.
    pub fn safe_export_with_feedback(
        &self,
        destination_path: &str,
        options: SafeExportOptions,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<SafeExportManifest, EngineError> {
        self.runtime.telemetry().increment_admin_ops();
        let mut metadata = BTreeMap::new();
        metadata.insert("destination_path".to_owned(), destination_path.to_owned());
        run_with_feedback(
            OperationContext {
                surface: "rust",
                operation_kind: "admin.safe_export",
            },
            metadata,
            Some(observer),
            config,
            engine_error_code,
            || {
                self.admin()
                    .service()
                    .safe_export(destination_path, options)
            },
        )
    }
}

/// # Errors
/// Returns the underlying compilation error if query compilation fails.
pub fn compile_query_with_feedback(
    ast: &QueryAst,
    observer: &dyn OperationObserver,
    config: FeedbackConfig,
) -> Result<CompiledQuery, fathomdb_query::CompileError> {
    let mut metadata = BTreeMap::new();
    metadata.insert("root_kind".to_owned(), ast.root_kind.clone());
    run_with_feedback(
        OperationContext {
            surface: "rust",
            operation_kind: "query.compile",
        },
        metadata,
        Some(observer),
        config,
        |_| Some("compile_error".to_owned()),
        || compile_query(ast),
    )
}

#[allow(clippy::unnecessary_wraps)]
fn engine_error_code(error: &EngineError) -> Option<String> {
    let code = match error {
        EngineError::Sqlite(_) => "sqlite_error",
        EngineError::Schema(_) => "schema_error",
        EngineError::Io(_) => "io_error",
        EngineError::WriterRejected(_) => "writer_rejected",
        EngineError::InvalidWrite(_) => "invalid_write",
        EngineError::Bridge(_) => "bridge_error",
        EngineError::CapabilityMissing(_) => "capability_missing",
        EngineError::DatabaseLocked(_) => "database_locked",
    };
    Some(code.to_owned())
}

/// A lightweight session borrowing an [`Engine`] reference.
///
/// Sessions do not own any state beyond the engine reference and are
/// intended for scoped, short-lived interaction patterns.
#[derive(Debug)]
pub struct Session<'a> {
    engine: &'a Engine,
}

impl<'a> Session<'a> {
    /// Create a new session bound to the given engine.
    pub fn new(engine: &'a Engine) -> Self {
        Self { engine }
    }

    /// Start building a node query for the given kind.
    pub fn query(&self, kind: impl Into<String>) -> QueryBuilder {
        self.engine.query(kind)
    }
}
