use std::path::{Path, PathBuf};

pub mod admin_ffi;
mod feedback;
#[cfg(any(feature = "python", feature = "node"))]
pub mod ffi_types;
#[cfg(feature = "node")]
mod node;
#[cfg(feature = "node")]
mod node_types;
#[cfg(feature = "python")]
mod python;
mod search;
pub mod search_ffi;
mod write_request_builder;

#[cfg(feature = "default-embedder")]
pub use fathomdb_engine::BuiltinBgeSmallEmbedder;
pub use fathomdb_engine::{
    ActionInsert, ActionRow, AdminHandle, BatchEmbedder, ChunkInsert, ChunkPolicy,
    EdgeExpansionRootRows, EdgeExpansionSlotRows, EdgeInsert, EdgeRetire, EdgeRow, EmbedderError,
    EngineError, EngineRuntime, ExecutionCoordinator, ExpansionRootRows, ExpansionSlotRows,
    FtsPropertyPathMode, FtsPropertyPathSpec, FtsPropertySchemaRecord, GroupedQueryRows,
    LastAccessTouchReport, LastAccessTouchRequest, LogicalPurgeReport, LogicalRestoreReport,
    NodeInsert, NodeRetire, NodeRow, OperationalCollectionKind, OperationalCollectionRecord,
    OperationalCompactionReport, OperationalCurrentRow, OperationalFilterClause,
    OperationalFilterField, OperationalFilterFieldType, OperationalFilterMode,
    OperationalFilterValue, OperationalHistoryValidationIssue, OperationalHistoryValidationReport,
    OperationalMutationRow, OperationalPurgeReport, OperationalReadReport, OperationalReadRequest,
    OperationalRegisterRequest, OperationalRepairReport, OperationalRetentionActionKind,
    OperationalRetentionPlanItem, OperationalRetentionPlanReport, OperationalRetentionRunItem,
    OperationalRetentionRunReport, OperationalSecondaryIndexDefinition,
    OperationalSecondaryIndexField, OperationalSecondaryIndexRebuildReport,
    OperationalSecondaryIndexValueType, OperationalTraceReport, OperationalValidationContract,
    OperationalValidationField, OperationalValidationFieldType, OperationalValidationMode,
    OperationalWrite, OptionalProjectionTask, ProjectionRepairReport, ProjectionTarget,
    ProvenanceEvent, ProvenanceMode, ProvenancePurgeOptions, ProvenancePurgeReport, QueryEmbedder,
    QueryEmbedderIdentity, QueryPlan, QueryRows, RebuildProgress, RunInsert, RunRow,
    SafeExportManifest, SafeExportOptions, SkippedEdge, StepInsert, StepRow,
    VectorRegenerationConfig, VectorRegenerationReport, VectorSource, WriteReceipt, WriteRequest,
    WriterActor, new_id, new_row_id,
};
// Pack G: `VecInsert` is deprecated (see engine crate). Re-export it under
// an `#[allow(deprecated)]` so the warning surfaces at caller sites rather
// than here.
#[allow(deprecated)]
pub use fathomdb_engine::VecInsert;
pub use fathomdb_engine::{SqliteCacheStatus, TelemetryLevel, TelemetrySnapshot};
#[doc(hidden)]
pub use fathomdb_query::compile_search_plan;
pub use fathomdb_query::{
    BindValue, BuilderValidationError, ComparisonOp, CompileError, CompiledGroupedQuery,
    CompiledQuery, CompiledRawVectorSearch, CompiledRetrievalPlan, CompiledSearch,
    CompiledSearchPlan, CompiledSemanticSearch, CompiledVectorSearch, DrivingTable, ExecutionHints,
    ExpansionSlot, HitAttribution, NodeRowLite, Predicate, Query, QueryAst, QueryBuilder,
    QueryStep, RetrievalModality, ScalarValue, SearchHit, SearchHitSource, SearchMatchMode,
    SearchRows, ShapeHash, TextQuery, TraverseDirection, compile_grouped_query, compile_query,
    compile_retrieval_plan, compile_search, compile_search_plan_from_queries,
    compile_vector_search,
};
pub use fathomdb_schema::{BootstrapReport, Migration, SchemaManager, SchemaVersion};
pub use feedback::{FeedbackConfig, OperationObserver, ResponseCycleEvent, ResponseCyclePhase};
pub use search::{
    FallbackSearchBuilder, NodeQueryBuilder, RawVectorSearchBuilder, SearchBuilder,
    SemanticSearchBuilder, TextSearchBuilder, VectorSearchBuilder,
};
pub use write_request_builder::{
    ActionHandle, ChunkHandle, ChunkRef, EdgeHandle, EdgeRef, NodeHandle, NodeRef, RunHandle,
    RunRef, StepHandle, StepRef, WriteRequestBuilder,
};

use std::collections::BTreeMap;
use std::sync::Arc;

use feedback::{OperationContext, run_with_feedback};

/// Caller-facing selection of the read-time query embedder.
///
/// Phase 12.5a ships this enum with [`Self::None`] (the default, Phase 12
/// v1 dormancy preserved), [`Self::Builtin`] (a stub until Phase 12.5b
/// lands the Candle + bge-small-en-v1.5 default implementation behind the
/// `default-embedder` feature flag), and [`Self::InProcess`] (a
/// caller-supplied in-process embedder, the most flexible shape).
///
/// A subprocess / external-service variant is intentionally deferred: the
/// existing write-time `VectorRegenerationConfig` covers the analogous
/// batch path and nothing in v1.5a requires a query-time subprocess
/// plumbing story.
#[derive(Clone, Debug, Default)]
pub enum EmbedderChoice {
    /// No read-time embedder. `search()`'s vector branch stays dormant.
    /// This is the default and preserves the Phase 12 v1 behaviour for
    /// callers who do not opt in.
    #[default]
    None,
    /// The built-in default embedder (Candle + bge-small-en-v1.5). Phase
    /// 12.5a ships this variant as a stub that resolves to `None` at
    /// runtime; Phase 12.5b will light it up behind the
    /// `default-embedder` feature flag.
    Builtin,
    /// A caller-supplied in-process embedder.
    InProcess(Arc<dyn QueryEmbedder>),
}

impl PartialEq for EmbedderChoice {
    fn eq(&self, other: &Self) -> bool {
        // `Arc<dyn QueryEmbedder>` is not `PartialEq`; we compare by
        // variant identity only. `InProcess` values compare equal iff
        // both sides point at the same allocation — good enough for the
        // Phase 12.5a surface tests and consistent with how typical
        // `Arc<dyn Trait>` configs are compared.
        match (self, other) {
            (Self::None, Self::None) | (Self::Builtin, Self::Builtin) => true,
            (Self::InProcess(a), Self::InProcess(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Eq for EmbedderChoice {}

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
    /// Phase 12.5a: selects the read-time query embedder, if any.
    /// Defaults to [`EmbedderChoice::None`] — the Phase 12 v1 dormancy
    /// invariant on `search()` is preserved unchanged.
    pub embedder: EmbedderChoice,
    /// Test-only: when `true`, [`Engine::submit_write`] synchronously
    /// drains the vector projection work queue after every commit that
    /// enqueued work, using the engine's configured embedder. This
    /// trades the async worker's availability contract for strict
    /// `write → semantic_search` visibility — **production code must not
    /// set this flag.** Intended for integration tests that want a
    /// single-step write-then-assert flow.
    ///
    /// Defaults to `false`.
    pub auto_drain_vector: bool,
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
            embedder: EmbedderChoice::None,
            auto_drain_vector: false,
        }
    }

    /// Builder-style setter for the read-time query embedder.
    #[must_use]
    pub fn with_embedder(mut self, choice: EmbedderChoice) -> Self {
        self.embedder = choice;
        self
    }
}

/// Resolve an [`EmbedderChoice`] into the concrete
/// `Option<Arc<dyn QueryEmbedder>>` that [`EngineRuntime::open`] takes.
///
/// Phase 12.5a keeps this translation deliberately tight: Phase 12.5b
/// will modify only the `Builtin` arm to construct the Candle + bge-
/// small-en-v1.5 default implementation.
fn resolve_embedder_choice(choice: EmbedderChoice) -> Option<Arc<dyn QueryEmbedder>> {
    match choice {
        EmbedderChoice::None => None,
        EmbedderChoice::Builtin => resolve_builtin_embedder(),
        EmbedderChoice::InProcess(arc) => Some(arc),
    }
}

/// Phase 12.5b: when the `default-embedder` feature is enabled, resolve
/// `EmbedderChoice::Builtin` to a concrete Candle + bge-small-en-v1.5
/// embedder (lazy-loaded on first query). When the feature is disabled,
/// log a warning and return `None` so the vector branch stays dormant
/// rather than erroring — matching the Phase 12.5a stub behavior.
#[cfg(feature = "default-embedder")]
#[allow(clippy::unnecessary_wraps)] // the no-feature twin returns None
fn resolve_builtin_embedder() -> Option<Arc<dyn QueryEmbedder>> {
    Some(Arc::new(fathomdb_engine::BuiltinBgeSmallEmbedder::new()) as Arc<dyn QueryEmbedder>)
}

#[cfg(not(feature = "default-embedder"))]
fn resolve_builtin_embedder() -> Option<Arc<dyn QueryEmbedder>> {
    // Built without the `default-embedder` feature. Callers who asked
    // for `EmbedderChoice::Builtin` in this configuration get the same
    // dormant behavior as `EmbedderChoice::None`. We deliberately do
    // NOT panic — degradation is the whole point of the embedder
    // surface.
    None
}

/// Adapter exposing a `&dyn QueryEmbedder` as a [`BatchEmbedder`] so the
/// engine-layer sync-drain path (`Engine::submit_write` when
/// `auto_drain_vector=true`) can reuse the engine's configured read-time
/// embedder without duplicating it. Preserves the "identity belongs to
/// the embedder" invariant.
struct AutoDrainBatchAdapter<'a> {
    inner: &'a dyn QueryEmbedder,
}

impl BatchEmbedder for AutoDrainBatchAdapter<'_> {
    fn batch_embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        let mut out = Vec::with_capacity(texts.len());
        for text in texts {
            out.push(self.inner.embed_query(text)?);
        }
        Ok(out)
    }
    fn identity(&self) -> fathomdb_engine::QueryEmbedderIdentity {
        self.inner.identity()
    }
    fn max_tokens(&self) -> usize {
        self.inner.max_tokens()
    }
}

/// Top-level handle to a fathomdb graph database.
///
/// An [`Engine`] owns the underlying `SQLite` connections, writer thread, and
/// read pool. Create one via [`Engine::open`] or [`Engine::open_with_feedback`].
#[derive(Debug)]
pub struct Engine {
    runtime: EngineRuntime,
    auto_drain_vector: bool,
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
        let auto_drain_vector = options.auto_drain_vector;
        let embedder = resolve_embedder_choice(options.embedder);
        Ok(Self {
            runtime: EngineRuntime::open(
                options.database_path,
                options.provenance_mode,
                options.vector_dimension,
                options.read_pool_size.unwrap_or(4),
                options.telemetry_level,
                embedder,
            )?,
            auto_drain_vector,
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
    ///
    /// Returns a tethered [`NodeQueryBuilder`] that borrows the engine so
    /// that `.execute()` can dispatch directly to the coordinator without
    /// the caller having to compile-and-execute manually.
    pub fn query(&self, kind: impl Into<String>) -> NodeQueryBuilder<'_> {
        NodeQueryBuilder::new(self, kind)
    }

    /// Start a narrow fallback-text-search chain.
    ///
    /// Unlike [`Self::query`] + `.text_search(...)`, this entry point takes
    /// a caller-provided `strict` shape and an optional caller-provided
    /// `relaxed` shape. When `relaxed` is `None`, the helper runs strict
    /// only — useful for "has any node already matched this strict query?"
    /// dedup-on-write patterns. When `relaxed` is `Some`, the relaxed
    /// branch fires only when strict returns fewer than
    /// [`fathomdb_query::FALLBACK_TRIGGER_K`] hits, and merge/dedup follows
    /// the same deterministic rules as the adaptive `text_search()` path.
    ///
    /// The relaxed shape is used verbatim — it is NOT passed through
    /// [`fathomdb_query::derive_relaxed`] — and the 4-alternative
    /// [`fathomdb_query::RELAXED_BRANCH_CAP`] is NOT applied, so
    /// `SearchRows::was_degraded` is always `false` on this path.
    pub fn fallback_search(
        &self,
        strict: impl Into<String>,
        relaxed: Option<impl Into<String>>,
        limit: usize,
    ) -> FallbackSearchBuilder<'_> {
        let relaxed_string: Option<String> = relaxed.map(Into::into);
        FallbackSearchBuilder::new(self, strict, relaxed_string.as_deref(), limit)
    }

    /// Returns a handle to the administrative service.
    pub fn admin(&self) -> &AdminHandle {
        self.runtime.admin()
    }

    /// Regenerate vector embeddings using the embedder configured at
    /// [`Engine::open`] time via [`EmbedderChoice`].
    ///
    /// 0.4.0 architectural invariant: the regen path and the read path
    /// share a single embedder instance, so vector identity on the
    /// resulting profile is stamped directly from
    /// [`QueryEmbedder::identity`] and cannot drift from what
    /// `search()` will use at read time.
    ///
    /// # Errors
    /// Returns [`EngineError::EmbedderNotConfigured`] if
    /// [`Engine::open`] was called with [`EmbedderChoice::None`] (or
    /// with `EmbedderChoice::Builtin` in a build without the
    /// `default-embedder` feature, which resolves to `None`).
    /// Propagates any underlying [`EngineError`] raised by the admin
    /// service.
    pub fn regenerate_vector_embeddings(
        &self,
        config: &VectorRegenerationConfig,
    ) -> Result<VectorRegenerationReport, EngineError> {
        let coordinator = self.runtime.coordinator();
        let embedder = coordinator
            .query_embedder()
            .ok_or(EngineError::EmbedderNotConfigured)?;
        self.runtime
            .admin()
            .service()
            .regenerate_vector_embeddings(embedder.as_ref(), config)
    }

    /// Returns a handle to the single-threaded writer actor.
    pub fn writer(&self) -> &WriterActor {
        self.runtime.writer()
    }

    /// Submit a write request through the writer actor.
    ///
    /// Identical to `engine.writer().submit(request)` when
    /// `EngineOptions::auto_drain_vector` is `false` (the default).
    ///
    /// When `auto_drain_vector` is `true` (test-only), this method
    /// additionally drains the vector projection work queue
    /// synchronously after the commit returns, using the engine's
    /// configured embedder. Embedder-unavailable or drain errors are
    /// swallowed — the write succeeds regardless — so tests that expect
    /// "write then immediately `semantic_search`" work without a separate
    /// drain step.
    ///
    /// # Errors
    /// Returns [`EngineError`] if the writer actor rejects the request
    /// or the underlying transaction fails.
    pub fn submit_write(&self, request: WriteRequest) -> Result<WriteReceipt, EngineError> {
        let receipt = self.writer().submit(request)?;
        if self.auto_drain_vector {
            self.auto_drain_vector_work();
        }
        Ok(receipt)
    }

    /// Best-effort synchronous drain of the vector projection queue
    /// using the engine's configured embedder. Used only by
    /// [`Self::submit_write`] when `auto_drain_vector` is set.
    fn auto_drain_vector_work(&self) {
        let Some(embedder_arc) = self.runtime.coordinator().query_embedder().cloned() else {
            return;
        };
        let adapter = AutoDrainBatchAdapter {
            inner: embedder_arc.as_ref(),
        };
        let _ = self
            .admin()
            .service()
            .drain_vector_projection(&adapter, std::time::Duration::from_secs(30));
    }

    /// Returns the read-side execution coordinator.
    pub fn coordinator(&self) -> &ExecutionCoordinator {
        self.runtime.coordinator()
    }

    /// Read all telemetry counters and aggregated `SQLite` cache statistics.
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

    /// Register an FTS property projection schema for a node kind.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the schema is invalid or the write fails.
    pub fn register_fts_property_schema(
        &self,
        kind: &str,
        property_paths: &[String],
        separator: Option<&str>,
    ) -> Result<FtsPropertySchemaRecord, EngineError> {
        self.admin()
            .service()
            .register_fts_property_schema(kind, property_paths, separator)
    }

    /// Register an FTS property projection schema with per-path modes
    /// (`scalar` or `recursive`) and optional exclude paths. When the
    /// schema introduces a new recursive-mode path this triggers an eager
    /// transactional rebuild of `fts_node_properties` and
    /// `fts_node_property_positions` for the target kind.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if validation or rebuild fails.
    pub fn register_fts_property_schema_with_entries(
        &self,
        kind: &str,
        entries: &[FtsPropertyPathSpec],
        separator: Option<&str>,
        exclude_paths: &[String],
    ) -> Result<FtsPropertySchemaRecord, EngineError> {
        self.admin()
            .service()
            .register_fts_property_schema_with_entries(
                kind,
                entries,
                separator,
                exclude_paths,
                fathomdb_engine::RebuildMode::Eager,
            )
    }

    /// Return the FTS property schema for a single node kind, if registered.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn describe_fts_property_schema(
        &self,
        kind: &str,
    ) -> Result<Option<FtsPropertySchemaRecord>, EngineError> {
        self.admin().service().describe_fts_property_schema(kind)
    }

    /// Return all registered FTS property schemas.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn list_fts_property_schemas(&self) -> Result<Vec<FtsPropertySchemaRecord>, EngineError> {
        self.admin().service().list_fts_property_schemas()
    }

    /// Remove the FTS property schema for a node kind.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the kind is not registered or the delete fails.
    pub fn remove_fts_property_schema(&self, kind: &str) -> Result<(), EngineError> {
        self.admin().service().remove_fts_property_schema(kind)
    }

    /// Register an FTS property schema using the async shadow-build path.
    ///
    /// Returns immediately with the schema record; the rebuild runs in the
    /// background via [`crate::rebuild_actor::RebuildActor`]. Poll
    /// [`Self::get_property_fts_rebuild_progress`] to observe completion.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] if the schema is invalid or the write fails.
    pub fn register_fts_property_schema_async(
        &self,
        kind: &str,
        property_paths: &[String],
        separator: Option<&str>,
    ) -> Result<FtsPropertySchemaRecord, EngineError> {
        let specs: Vec<FtsPropertyPathSpec> = property_paths
            .iter()
            .map(|p| FtsPropertyPathSpec::scalar(p.clone()))
            .collect();
        self.admin()
            .service()
            .register_fts_property_schema_with_entries(
                kind,
                &specs,
                separator,
                &[],
                fathomdb_engine::RebuildMode::Async,
            )
    }

    /// Return the current async rebuild progress for a kind, or `None` if no
    /// rebuild has been registered.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError`] on database failure.
    pub fn get_property_fts_rebuild_progress(
        &self,
        kind: &str,
    ) -> Result<Option<RebuildProgress>, EngineError> {
        self.runtime
            .coordinator()
            .get_property_fts_rebuild_progress(kind)
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
        EngineError::WriterTimedOut(_) => "writer_timed_out",
        EngineError::InvalidWrite(_) => "invalid_write",
        EngineError::Bridge(_) => "bridge_error",
        EngineError::CapabilityMissing(_) => "capability_missing",
        EngineError::DatabaseLocked(_) => "database_locked",
        EngineError::InvalidConfig(_) => "invalid_config",
        EngineError::EmbedderNotConfigured => "embedder_not_configured",
        EngineError::EmbeddingChangeRequiresAck { .. } => "embedding_change_requires_ack",
        EngineError::KindNotVectorIndexed { .. } => "kind_not_vector_indexed",
        EngineError::DimensionMismatch { .. } => "dimension_mismatch",
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
    pub fn query(&self, kind: impl Into<String>) -> NodeQueryBuilder<'_> {
        self.engine.query(kind)
    }
}
