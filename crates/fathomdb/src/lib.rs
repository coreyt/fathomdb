use std::path::{Path, PathBuf};

mod feedback;
#[cfg(feature = "python")]
mod python;
#[cfg(feature = "python")]
mod python_types;

pub use fathomdb_engine::{
    ActionInsert, ActionRow, AdminHandle, ChunkInsert, ChunkPolicy, EdgeInsert, EdgeRetire,
    EngineError, EngineRuntime, ExecutionCoordinator, NodeInsert, NodeRetire, NodeRow,
    OptionalProjectionTask, ProjectionRepairReport, ProjectionTarget, ProvenanceEvent,
    ProvenanceMode, QueryPlan, QueryRows, RunInsert, RunRow, SafeExportManifest, SafeExportOptions,
    StepInsert, StepRow, VecInsert, WriteReceipt, WriteRequest, WriterActor, new_id, new_row_id,
};
pub use fathomdb_query::{
    BindValue, CompiledQuery, DrivingTable, ExecutionHints, Predicate, Query, QueryAst,
    QueryBuilder, QueryStep, ScalarValue, ShapeHash, TraverseDirection, compile_query,
};
pub use fathomdb_schema::{BootstrapReport, Migration, SchemaManager, SchemaVersion};
pub use feedback::{FeedbackConfig, OperationObserver, ResponseCycleEvent, ResponseCyclePhase};

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
}

impl EngineOptions {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            database_path: path.as_ref().to_path_buf(),
            provenance_mode: ProvenanceMode::Warn,
            vector_dimension: None,
        }
    }
}

#[derive(Debug)]
pub struct Engine {
    runtime: EngineRuntime,
}

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
