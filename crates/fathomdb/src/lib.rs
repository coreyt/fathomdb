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
    ProvenanceMode, ProvenancePurgeOptions, ProvenancePurgeReport, QueryPlan, QueryRows,
    RunInsert, RunRow, SafeExportManifest, SafeExportOptions, SkippedEdge, StepInsert, StepRow,
    VecInsert, WriteReceipt, WriteRequest, WriterActor, new_id, new_row_id,
};
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineOptions {
    pub database_path: PathBuf,
    pub provenance_mode: ProvenanceMode,
    /// When `Some(dim)`, the engine opens a vector-capable connection and
    /// bootstraps a `vec_nodes_active` vector table with the given dimension.
    /// Requires the `sqlite-vec` crate feature; ignored if the feature is absent.
    pub vector_dimension: Option<usize>,
    /// Number of read-only `SQLite` connections in the reader pool.
    /// Defaults to 4 when `None`.
    pub read_pool_size: Option<usize>,
}

impl EngineOptions {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            database_path: path.as_ref().to_path_buf(),
            provenance_mode: ProvenanceMode::Warn,
            vector_dimension: None,
            read_pool_size: None,
        }
    }
}

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
            )?,
        })
    }

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

    pub fn query(&self, kind: impl Into<String>) -> QueryBuilder {
        QueryBuilder::nodes(kind)
    }

    pub fn admin(&self) -> &AdminHandle {
        self.runtime.admin()
    }

    pub fn writer(&self) -> &WriterActor {
        self.runtime.writer()
    }

    pub fn coordinator(&self) -> &ExecutionCoordinator {
        self.runtime.coordinator()
    }

    pub fn touch_last_accessed(
        &self,
        request: LastAccessTouchRequest,
    ) -> Result<LastAccessTouchReport, EngineError> {
        self.writer().touch_last_accessed(request)
    }

    pub fn register_operational_collection(
        &self,
        request: &OperationalRegisterRequest,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        self.admin()
            .service()
            .register_operational_collection(request)
    }

    pub fn describe_operational_collection(
        &self,
        name: &str,
    ) -> Result<Option<OperationalCollectionRecord>, EngineError> {
        self.admin().service().describe_operational_collection(name)
    }

    pub fn update_operational_collection_filters(
        &self,
        name: &str,
        filter_fields_json: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        self.admin()
            .service()
            .update_operational_collection_filters(name, filter_fields_json)
    }

    pub fn update_operational_collection_validation(
        &self,
        name: &str,
        validation_json: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        self.admin()
            .service()
            .update_operational_collection_validation(name, validation_json)
    }

    pub fn update_operational_collection_secondary_indexes(
        &self,
        name: &str,
        secondary_indexes_json: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        self.admin()
            .service()
            .update_operational_collection_secondary_indexes(name, secondary_indexes_json)
    }

    pub fn trace_operational_collection(
        &self,
        collection_name: &str,
        record_key: Option<&str>,
    ) -> Result<OperationalTraceReport, EngineError> {
        self.admin()
            .service()
            .trace_operational_collection(collection_name, record_key)
    }

    pub fn read_operational_collection(
        &self,
        request: &OperationalReadRequest,
    ) -> Result<OperationalReadReport, EngineError> {
        self.admin().service().read_operational_collection(request)
    }

    pub fn rebuild_operational_current(
        &self,
        collection_name: Option<&str>,
    ) -> Result<OperationalRepairReport, EngineError> {
        self.admin()
            .service()
            .rebuild_operational_current(collection_name)
    }

    pub fn validate_operational_collection_history(
        &self,
        collection_name: &str,
    ) -> Result<OperationalHistoryValidationReport, EngineError> {
        self.admin()
            .service()
            .validate_operational_collection_history(collection_name)
    }

    pub fn rebuild_operational_secondary_indexes(
        &self,
        collection_name: &str,
    ) -> Result<OperationalSecondaryIndexRebuildReport, EngineError> {
        self.admin()
            .service()
            .rebuild_operational_secondary_indexes(collection_name)
    }

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

    pub fn disable_operational_collection(
        &self,
        name: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        self.admin().service().disable_operational_collection(name)
    }

    pub fn compact_operational_collection(
        &self,
        name: &str,
        dry_run: bool,
    ) -> Result<OperationalCompactionReport, EngineError> {
        self.admin()
            .service()
            .compact_operational_collection(name, dry_run)
    }

    pub fn purge_operational_collection(
        &self,
        name: &str,
        before_timestamp: i64,
    ) -> Result<OperationalPurgeReport, EngineError> {
        self.admin()
            .service()
            .purge_operational_collection(name, before_timestamp)
    }

    pub fn restore_logical_id(
        &self,
        logical_id: &str,
    ) -> Result<LogicalRestoreReport, EngineError> {
        self.admin().service().restore_logical_id(logical_id)
    }

    pub fn purge_logical_id(&self, logical_id: &str) -> Result<LogicalPurgeReport, EngineError> {
        self.admin().service().purge_logical_id(logical_id)
    }

    pub fn purge_provenance_events(
        &self,
        before_timestamp: i64,
        options: &ProvenancePurgeOptions,
    ) -> Result<ProvenancePurgeReport, EngineError> {
        self.admin()
            .service()
            .purge_provenance_events(before_timestamp, options)
    }

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

    pub fn check_integrity_with_feedback(
        &self,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<fathomdb_engine::IntegrityReport, EngineError> {
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

    pub fn check_semantics_with_feedback(
        &self,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<fathomdb_engine::SemanticReport, EngineError> {
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

    pub fn rebuild_projections_with_feedback(
        &self,
        target: ProjectionTarget,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<ProjectionRepairReport, EngineError> {
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

    pub fn rebuild_missing_projections_with_feedback(
        &self,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<ProjectionRepairReport, EngineError> {
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

    pub fn trace_source_with_feedback(
        &self,
        source_ref: &str,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<fathomdb_engine::TraceReport, EngineError> {
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

    pub fn excise_source_with_feedback(
        &self,
        source_ref: &str,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<fathomdb_engine::TraceReport, EngineError> {
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

    pub fn safe_export_with_feedback(
        &self,
        destination_path: &str,
        options: SafeExportOptions,
        observer: &dyn OperationObserver,
        config: FeedbackConfig,
    ) -> Result<SafeExportManifest, EngineError> {
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
    };
    Some(code.to_owned())
}

#[derive(Debug)]
pub struct Session<'a> {
    engine: &'a Engine,
}

impl<'a> Session<'a> {
    pub fn new(engine: &'a Engine) -> Self {
        Self { engine }
    }

    pub fn query(&self, kind: impl Into<String>) -> QueryBuilder {
        self.engine.query(kind)
    }
}
