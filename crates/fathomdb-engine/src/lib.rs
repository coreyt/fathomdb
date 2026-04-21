#[macro_use]
mod trace_support;

mod admin;
mod coordinator;
mod database_lock;
mod embedder;
mod ids;
mod operational;
mod projection;
pub mod rebuild_actor;
mod runtime;
mod sqlite;
mod telemetry;
mod writer;

pub use admin::{
    AdminHandle, AdminService, FtsProfile, FtsPropertyPathMode, FtsPropertyPathSpec,
    FtsPropertySchemaRecord, IntegrityReport, LogicalPurgeReport, LogicalRestoreReport,
    ProjectionImpact, ProvenancePurgeOptions, ProvenancePurgeReport, SafeExportManifest,
    SafeExportOptions, SemanticReport, SkippedEdge, TOKENIZER_PRESETS, TraceReport, VecProfile,
    VectorRegenerationConfig, VectorRegenerationReport, load_vector_regeneration_config,
};
pub use coordinator::{
    ActionRow, EdgeRow, ExecutionCoordinator, ExpansionRootRows, ExpansionSlotRows,
    GroupedQueryRows, NodeRow, ProvenanceEvent, QueryPlan, QueryRows, RunRow, StepRow,
};
#[cfg(feature = "default-embedder")]
pub use embedder::BuiltinBgeSmallEmbedder;
pub use embedder::{EmbedderError, QueryEmbedder, QueryEmbedderIdentity};
pub use ids::{new_id, new_row_id};
pub use operational::{
    OperationalCollectionKind, OperationalCollectionRecord, OperationalCompactionReport,
    OperationalCurrentRow, OperationalFilterClause, OperationalFilterField,
    OperationalFilterFieldType, OperationalFilterMode, OperationalFilterValue,
    OperationalHistoryValidationIssue, OperationalHistoryValidationReport, OperationalMutationRow,
    OperationalPurgeReport, OperationalReadReport, OperationalReadRequest,
    OperationalRegisterRequest, OperationalRepairReport, OperationalRetentionActionKind,
    OperationalRetentionPlanItem, OperationalRetentionPlanReport, OperationalRetentionRunItem,
    OperationalRetentionRunReport, OperationalSecondaryIndexDefinition,
    OperationalSecondaryIndexField, OperationalSecondaryIndexRebuildReport,
    OperationalSecondaryIndexValueType, OperationalTraceReport, OperationalValidationContract,
    OperationalValidationField, OperationalValidationFieldType, OperationalValidationMode,
};
pub use projection::{ProjectionRepairReport, ProjectionService, ProjectionTarget};
pub use rebuild_actor::{RebuildMode, RebuildProgress, RebuildStateRow};
pub use runtime::EngineRuntime;
pub use sqlite::{SharedSqlitePolicy, shared_sqlite_policy};
pub use telemetry::{
    SqliteCacheStatus, TelemetryCounters, TelemetryLevel, TelemetrySnapshot, read_db_cache_status,
};
pub use writer::{
    ActionInsert, ChunkInsert, ChunkPolicy, EdgeInsert, EdgeRetire, LastAccessTouchReport,
    LastAccessTouchRequest, NodeInsert, NodeRetire, OperationalWrite, OptionalProjectionTask,
    ProvenanceMode, RunInsert, StepInsert, VecInsert, WriteReceipt, WriteRequest, WriterActor,
};

use thiserror::Error;

/// Top-level error type for all engine operations.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("schema error: {0}")]
    Schema(#[from] fathomdb_schema::SchemaError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("writer actor rejected request: {0}")]
    WriterRejected(String),
    #[error("write timed out (may still commit): {0}")]
    WriterTimedOut(String),
    #[error("invalid write request: {0}")]
    InvalidWrite(String),
    #[error("bridge error: {0}")]
    Bridge(String),
    #[error("capability missing: {0}")]
    CapabilityMissing(String),
    #[error("database locked: {0}")]
    DatabaseLocked(String),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error(
        "embedder not configured: call Engine::open with a non-None EmbedderChoice to regenerate vector embeddings"
    )]
    EmbedderNotConfigured,
}
