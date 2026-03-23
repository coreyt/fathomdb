mod admin;
mod coordinator;
mod projection;
mod runtime;
mod sqlite;
mod writer;

pub use admin::{
    AdminHandle, AdminService, IntegrityReport, SafeExportManifest, SemanticReport, TraceReport,
};
pub use coordinator::{ExecutionCoordinator, NodeRow, QueryRows};
pub use projection::{ProjectionRepairReport, ProjectionService, ProjectionTarget};
pub use runtime::EngineRuntime;
pub use sqlite::{SharedSqlitePolicy, shared_sqlite_policy};
pub use writer::{
    ActionInsert, ChunkInsert, EdgeInsert, NodeInsert, OptionalProjectionTask, RunInsert,
    StepInsert, WriteReceipt, WriteRequest, WriterActor,
};

use thiserror::Error;

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
    #[error("invalid write request: {0}")]
    InvalidWrite(String),
    #[error("bridge error: {0}")]
    Bridge(String),
    #[error("capability missing: {0}")]
    CapabilityMissing(String),
}
