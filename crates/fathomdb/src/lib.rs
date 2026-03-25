use std::path::{Path, PathBuf};

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
