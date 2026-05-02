use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use fathomdb_embedder_api::EmbedderIdentity;
use fathomdb_query::compile_text_query;
use fathomdb_schema::SCHEMA_VERSION;

#[derive(Debug)]
pub struct Engine {
    path: PathBuf,
    next_cursor: AtomicU64,
    closed: AtomicBool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenReport {
    pub schema_version: u32,
    pub query_backend: &'static str,
    pub default_embedder: EmbedderIdentity,
}

#[derive(Debug)]
pub struct OpenedEngine {
    pub engine: Engine,
    pub report: OpenReport,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteReceipt {
    pub cursor: u64,
}

/// Soft-fallback signal carried on hybrid `search` results.
///
/// Per `dev/design/retrieval.md` § Soft-fallback signal, this record is
/// present only when one non-essential branch could not contribute. Total
/// request failure is not expressed via this carrier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SoftFallback {
    pub branch: SoftFallbackBranch,
}

/// Which retrieval branch could not contribute to a hybrid search.
///
/// `Vector` means the vector branch could not contribute; `Text` means the
/// text branch could not contribute. Owned by `dev/design/retrieval.md`;
/// the 0.6.0 enum is exactly these two members.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoftFallbackBranch {
    Vector,
    Text,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    pub projection_cursor: u64,
    pub soft_fallback: Option<SoftFallback>,
    pub results: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreparedWrite {
    Node { kind: String, body: String },
    Edge { kind: String, from: String, to: String },
    OpStore { schema_id: String, body: String },
    AdminSchema { name: String, body: String },
}

/// Snapshot of engine-internal counters returned by [`Engine::counters`].
///
/// Field set is owned by `dev/design/lifecycle.md`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CounterSnapshot {}

/// Handle returned by [`Engine::subscribe`].
///
/// Subscriber payload semantics are owned by `dev/design/lifecycle.md` and
/// `dev/design/migrations.md`. Dropping the handle detaches the subscriber.
#[derive(Debug, Default)]
pub struct Subscription {}

/// Stable corruption-on-open detail carried by
/// [`EngineOpenError::Corruption`].
///
/// Layout owned by `dev/design/errors.md` § Corruption detail owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorruptionDetail {
    pub kind: CorruptionKind,
    pub stage: OpenStage,
    pub locator: CorruptionLocator,
    pub recovery_hint: RecoveryHint,
}

/// Open-path corruption category.
///
/// 0.6.0 emits exactly the four members below; per
/// `dev/design/errors.md` § Engine.open corruption table, doctor-only
/// finding codes are not represented here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorruptionKind {
    WalReplayFailure,
    HeaderMalformed,
    SchemaInconsistent,
    EmbedderIdentityDrift,
}

/// `Engine.open` stage at which corruption was detected.
///
/// Per ADR-0.6.0-corruption-open-behavior, `LockAcquisition` is intentionally
/// not a member here; lock contention is surfaced via
/// [`EngineOpenError::DatabaseLocked`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenStage {
    WalReplay,
    HeaderProbe,
    SchemaProbe,
    EmbedderIdentity,
}

/// Locator pointing at the corrupted region of the database file.
///
/// Variant set owned by `dev/design/errors.md` § CorruptionLocator
/// ownership. `OpaqueSqliteError` is the required fallback when SQLite
/// surfaces corruption without a usable structured locator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CorruptionLocator {
    FileOffset { offset: u64 },
    PageId { page: u32 },
    TableRow { table: &'static str, rowid: i64 },
    Vec0ShadowRow { partition: &'static str, rowid: i64 },
    MigrationStep { from: u32, to: u32 },
    OpaqueSqliteError { sqlite_extended_code: i32 },
}

/// Recovery dispatch surface attached to a corruption detail.
///
/// `code` is the stable dispatch key used by bindings and doctor output;
/// `doc_anchor` points at the documentation section that explains the
/// remediation path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryHint {
    pub code: &'static str,
    pub doc_anchor: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineOpenError {
    DatabaseLocked,
    Corruption(CorruptionDetail),
    IncompatibleSchemaVersion,
    MigrationError,
    EmbedderIdentityMismatch { stored: EmbedderIdentity, supplied: EmbedderIdentity },
    EmbedderDimensionMismatch { stored: u32, supplied: u32 },
}

impl Display for EngineOpenError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DatabaseLocked => write!(f, "database is locked by another engine instance"),
            Self::Corruption(detail) => {
                write!(
                    f,
                    "engine corruption at {:?} stage: {}",
                    detail.stage, detail.recovery_hint.code
                )
            }
            Self::IncompatibleSchemaVersion => write!(f, "database schema version is incompatible"),
            Self::MigrationError => write!(f, "schema migration failed"),
            Self::EmbedderIdentityMismatch { stored, supplied } => write!(
                f,
                "embedder identity mismatch: stored {}@{}, supplied {}@{}",
                stored.name, stored.revision, supplied.name, supplied.revision,
            ),
            Self::EmbedderDimensionMismatch { stored, supplied } => write!(
                f,
                "embedder vector dimension mismatch: stored {stored}, supplied {supplied}",
            ),
        }
    }
}

impl Error for EngineOpenError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineError {
    Storage,
    Projection,
    Vector,
    Embedder,
    Scheduler,
    OpStore,
    WriteValidation,
    SchemaValidation,
    Overloaded,
    Closing,
}

impl Display for EngineError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage => write!(f, "storage error"),
            Self::Projection => write!(f, "projection error"),
            Self::Vector => write!(f, "vector error"),
            Self::Embedder => write!(f, "embedder error"),
            Self::Scheduler => write!(f, "scheduler error"),
            Self::OpStore => write!(f, "op-store error"),
            Self::WriteValidation => write!(f, "write validation error"),
            Self::SchemaValidation => write!(f, "schema validation error"),
            Self::Overloaded => write!(f, "engine overloaded"),
            Self::Closing => write!(f, "engine is closing"),
        }
    }
}

impl Error for EngineError {}

impl Engine {
    pub fn open(path: impl Into<PathBuf>) -> Result<OpenedEngine, EngineOpenError> {
        let path = path.into();

        let report = OpenReport {
            schema_version: SCHEMA_VERSION,
            query_backend: "fathomdb-query scaffold",
            default_embedder: EmbedderIdentity::new("fathomdb-noop", "0.6.0-scaffold"),
        };

        Ok(OpenedEngine {
            engine: Self { path, next_cursor: AtomicU64::new(0), closed: AtomicBool::new(false) },
            report,
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write(&self, batch: &[PreparedWrite]) -> Result<WriteReceipt, EngineError> {
        self.ensure_open()?;

        let increment = u64::try_from(batch.len()).unwrap_or(u64::MAX).max(1);
        let previous = self.next_cursor.fetch_add(increment, Ordering::SeqCst);

        Ok(WriteReceipt { cursor: previous.saturating_add(increment) })
    }

    pub fn search(&self, query: &str) -> Result<SearchResult, EngineError> {
        self.ensure_open()?;
        if query.trim().is_empty() {
            return Err(EngineError::WriteValidation);
        }

        let compiled = compile_text_query(query);
        let cursor = self.next_cursor.load(Ordering::SeqCst);

        Ok(SearchResult {
            projection_cursor: cursor,
            soft_fallback: None,
            results: vec![compiled.sql],
        })
    }

    pub fn close(&self) -> Result<(), EngineError> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Block until in-flight writes drain or `timeout_ms` elapses.
    ///
    /// Surface owned by `dev/interfaces/rust.md` § Engine-attached
    /// instrumentation; semantics are owned by `dev/design/lifecycle.md`.
    pub fn drain(&self, _timeout_ms: u64) -> Result<(), EngineError> {
        Ok(())
    }

    /// Snapshot of engine-internal counters.
    ///
    /// Field set owned by `dev/design/lifecycle.md`.
    #[must_use]
    pub fn counters(&self) -> CounterSnapshot {
        CounterSnapshot::default()
    }

    /// Toggle response-cycle profiling.
    pub fn set_profiling(&self, _enabled: bool) -> Result<(), EngineError> {
        Ok(())
    }

    /// Set the threshold above which an operation is reported as slow.
    pub fn set_slow_threshold_ms(&self, _value: u64) -> Result<(), EngineError> {
        Ok(())
    }

    /// Attach a host subscriber to engine events.
    ///
    /// Payload shape owned by `dev/design/lifecycle.md` and
    /// `dev/design/migrations.md`.
    #[must_use]
    pub fn subscribe(&self) -> Subscription {
        Subscription::default()
    }

    fn ensure_open(&self) -> Result<(), EngineError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(EngineError::Closing);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Engine, PreparedWrite};

    #[test]
    fn write_advances_cursor() {
        let opened = Engine::open("rewrite.sqlite").expect("scaffold engine should open");
        let receipt = opened
            .engine
            .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: "hello".to_string() }])
            .expect("write should succeed");

        assert_eq!(receipt.cursor, 1);
    }
}
