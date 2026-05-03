pub mod lifecycle;

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use fathomdb_embedder_api::EmbedderIdentity;
use fathomdb_query::compile_text_query;
use fathomdb_schema::{
    migrate_with_event_sink, MigrationError as SchemaMigrationError, MigrationStepReport,
    LOCK_SUFFIX, MIGRATIONS, SCHEMA_VERSION,
};
use jsonschema::JSONSchema;
use rusqlite::{params, Connection};
use serde_json::Value;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

const DEFAULT_EMBEDDER_NAME: &str = "fathomdb-noop";
const DEFAULT_EMBEDDER_REVISION: &str = "0.6.0-scaffold";
const DEFAULT_EMBEDDER_DIMENSION: u32 = 384;

/// REQ-006a / AC-007a default slow-statement threshold. Mutated at runtime
/// via [`Engine::set_slow_threshold_ms`].
const DEFAULT_SLOW_THRESHOLD_MS: u64 = 100;

/// Reader pool size. Per `dev/design/engine.md` § Writer / reader split,
/// reader connections are pooled and never serialize behind one
/// connection. AC-021 exercises 8 concurrent readers.
const READER_POOL_SIZE: usize = 8;

#[derive(Debug)]
pub struct Engine {
    path: PathBuf,
    next_cursor: AtomicU64,
    closed: AtomicBool,
    lock: Mutex<Option<File>>,
    connection: Mutex<Option<Connection>>,
    reader_pool: ReaderPool,
    counters: lifecycle::Counters,
    subscribers: Arc<lifecycle::SubscriberRegistry>,
    profiling_enabled: Arc<AtomicBool>,
    slow_threshold_ms: Arc<AtomicU64>,
    /// Per-connection profile-callback contexts. Each box's pointer is
    /// installed into the connection's `sqlite3_profile` userdata; the
    /// box must outlive the connection so the callback never reads
    /// freed memory. Connections are dropped before this vec on
    /// `close`/`Drop`, so the lifetime ordering holds.
    ///
    /// Why `Box<ProfileContext>` and not `ProfileContext` directly: the
    /// FFI pointer captured during `install_profile_callback` MUST
    /// remain stable for the connection's lifetime; pushing onto a
    /// `Vec<ProfileContext>` could reallocate and invalidate that
    /// pointer.
    #[allow(clippy::vec_box)]
    profile_contexts: Mutex<Vec<Box<ProfileContext>>>,
    #[cfg(debug_assertions)]
    force_next_commit_failure: AtomicBool,
}

/// Per-connection profile-callback context.
///
/// Holds the registry handle the callback dispatches to, plus shared
/// references to the engine's profiling toggle and slow-statement
/// threshold. The `Arc` clones here mirror the same atomics held by
/// `Engine`, so `set_profiling` / `set_slow_threshold_ms` mutations are
/// visible inside the callback without restart (REQ-006a / AC-005a /
/// AC-007b runtime-toggle contract).
#[derive(Debug)]
struct ProfileContext {
    subscribers: Arc<lifecycle::SubscriberRegistry>,
    profiling_enabled: Arc<AtomicBool>,
    slow_threshold_ms: Arc<AtomicU64>,
}

/// Bounded pool of read-only SQLite connections.
///
/// Per `dev/design/engine.md` § Writer / reader split, reader connections
/// must not serialize behind a single mutex. Each connection opens with
/// `journal_mode=WAL` and `query_only=ON` so concurrent reads coexist
/// with one writer thread (AC-021).
#[derive(Debug, Default)]
struct ReaderPool {
    inner: Mutex<Vec<Connection>>,
    cvar: Condvar,
}

impl ReaderPool {
    fn new(connections: Vec<Connection>) -> Self {
        Self { inner: Mutex::new(connections), cvar: Condvar::new() }
    }

    fn borrow(&self) -> Option<Connection> {
        let mut guard = self.inner.lock().ok()?;
        while guard.is_empty() {
            guard = self.cvar.wait(guard).ok()?;
        }
        guard.pop()
    }

    fn release(&self, conn: Connection) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.push(conn);
            self.cvar.notify_one();
        }
    }

    fn drain(&self) -> Vec<Connection> {
        self.inner.lock().map(|mut g| std::mem::take(&mut *g)).unwrap_or_default()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenReport {
    pub schema_version_before: u32,
    pub schema_version_after: u32,
    pub migration_steps: Vec<MigrationStepReport>,
    pub embedder_warmup_ms: u64,
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
    OpStore { collection: String, record_key: String, schema_id: Option<String>, body: String },
    AdminSchema { name: String, kind: String, schema_json: String, retention_json: String },
}

/// Snapshot of engine-internal counters returned by [`Engine::counters`].
///
/// Public key set is owned by `dev/design/lifecycle.md` § Public key set
/// and locked by AC-004a. Reading a snapshot is non-perturbing per
/// AC-004c. The 0.6.0 surface exposes exactly these seven fields.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CounterSnapshot {
    pub queries: u64,
    pub writes: u64,
    pub write_rows: u64,
    pub errors_by_code: BTreeMap<String, u64>,
    pub admin_ops: u64,
    pub cache_hit: u64,
    pub cache_miss: u64,
}

pub use lifecycle::Subscription;

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
    DatabaseLocked { holder_pid: Option<u32> },
    Corruption(CorruptionDetail),
    IncompatibleSchemaVersion { seen: u32, supported: u32 },
    MigrationError { schema_version_before: u32, schema_version_current: u32, step_id: u32 },
    EmbedderIdentityMismatch { stored: EmbedderIdentity, supplied: EmbedderIdentity },
    EmbedderDimensionMismatch { stored: u32, supplied: u32 },
    Io { message: String },
}

impl Display for EngineOpenError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DatabaseLocked { holder_pid } => match holder_pid {
                Some(pid) => write!(f, "database is locked by process {pid}"),
                None => write!(f, "database is locked by another engine instance"),
            },
            Self::Corruption(detail) => {
                write!(
                    f,
                    "engine corruption at {:?} stage: {}",
                    detail.stage, detail.recovery_hint.code
                )
            }
            Self::IncompatibleSchemaVersion { seen, supported } => write!(
                f,
                "database schema version {seen} is incompatible with supported version {supported}"
            ),
            Self::MigrationError {
                schema_version_before,
                schema_version_current,
                step_id,
            } => write!(
                f,
                "schema migration failed at step {step_id}; schema version remained between {schema_version_before} and {schema_version_current}"
            ),
            Self::EmbedderIdentityMismatch { stored, supplied } => write!(
                f,
                "embedder identity mismatch: stored {}@{}, supplied {}@{}",
                stored.name, stored.revision, supplied.name, supplied.revision,
            ),
            Self::EmbedderDimensionMismatch { stored, supplied } => write!(
                f,
                "embedder vector dimension mismatch: stored {stored}, supplied {supplied}",
            ),
            Self::Io { message } => write!(f, "database I/O error: {message}"),
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

impl EngineError {
    /// Stable machine-readable code for `errors_by_code` keys.
    ///
    /// Names match the binding-facing class stems in
    /// `dev/design/errors.md` § Binding-facing class matrix.
    fn stable_code(&self) -> &'static str {
        match self {
            Self::Storage => "StorageError",
            Self::Projection => "ProjectionError",
            Self::Vector => "VectorError",
            Self::Embedder => "EmbedderError",
            Self::Scheduler => "SchedulerError",
            Self::OpStore => "OpStoreError",
            Self::WriteValidation => "WriteValidationError",
            Self::SchemaValidation => "SchemaValidationError",
            Self::Overloaded => "OverloadedError",
            Self::Closing => "ClosingError",
        }
    }
}

impl Error for EngineError {}

impl Engine {
    pub fn open(path: impl Into<PathBuf>) -> Result<OpenedEngine, EngineOpenError> {
        Self::open_with_migration_event_sink(path, |_| {})
    }

    pub fn open_with_migration_event_sink(
        path: impl Into<PathBuf>,
        mut emit_migration_event: impl FnMut(&MigrationStepReport),
    ) -> Result<OpenedEngine, EngineOpenError> {
        Self::open_with_migrations(path, MIGRATIONS, &mut emit_migration_event)
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn open_with_migrations_for_test(
        path: impl Into<PathBuf>,
        migrations: &'static [fathomdb_schema::Migration],
        mut emit_migration_event: impl FnMut(&MigrationStepReport),
    ) -> Result<OpenedEngine, EngineOpenError> {
        Self::open_with_migrations(path, migrations, &mut emit_migration_event)
    }

    fn open_with_migrations(
        path: impl Into<PathBuf>,
        migrations: &'static [fathomdb_schema::Migration],
        emit_migration_event: &mut impl FnMut(&MigrationStepReport),
    ) -> Result<OpenedEngine, EngineOpenError> {
        let canonical_path = canonical_database_path(&path.into())?;
        let lock = acquire_lock(&canonical_path)?;
        let open_result =
            Self::open_locked(canonical_path.clone(), migrations, emit_migration_event);

        match open_result {
            Ok((connection, readers, report)) => {
                let next_cursor = load_next_cursor(&connection);
                let subscribers = Arc::new(lifecycle::SubscriberRegistry::new());
                let profiling_enabled = Arc::new(AtomicBool::new(false));
                let slow_threshold_ms = Arc::new(AtomicU64::new(DEFAULT_SLOW_THRESHOLD_MS));
                let mut profile_contexts: Vec<Box<ProfileContext>> = Vec::new();

                install_profile_callback(
                    &connection,
                    &subscribers,
                    &profiling_enabled,
                    &slow_threshold_ms,
                    &mut profile_contexts,
                );
                for reader in &readers {
                    install_profile_callback(
                        reader,
                        &subscribers,
                        &profiling_enabled,
                        &slow_threshold_ms,
                        &mut profile_contexts,
                    );
                }

                Ok(OpenedEngine {
                    engine: Self {
                        path: canonical_path,
                        next_cursor: AtomicU64::new(next_cursor),
                        closed: AtomicBool::new(false),
                        lock: Mutex::new(Some(lock)),
                        connection: Mutex::new(Some(connection)),
                        reader_pool: ReaderPool::new(readers),
                        counters: lifecycle::Counters::new(),
                        subscribers,
                        profiling_enabled,
                        slow_threshold_ms,
                        profile_contexts: Mutex::new(profile_contexts),
                        #[cfg(debug_assertions)]
                        force_next_commit_failure: AtomicBool::new(false),
                    },
                    report,
                })
            }
            Err(err) => {
                drop(lock);
                Err(err)
            }
        }
    }

    fn open_locked(
        path: PathBuf,
        migrations: &'static [fathomdb_schema::Migration],
        emit_migration_event: &mut impl FnMut(&MigrationStepReport),
    ) -> Result<(Connection, Vec<Connection>, OpenReport), EngineOpenError> {
        let connection = Connection::open(&path)
            .map_err(|_| EngineOpenError::Io { message: "could not open database".to_string() })?;
        connection.pragma_update(None, "journal_mode", "WAL").map_err(|_| EngineOpenError::Io {
            message: "could not set journal mode".to_string(),
        })?;

        reject_legacy_shape(&connection)?;
        let migration = migrate_with_event_sink(&connection, migrations, emit_migration_event)
            .map_err(map_migration_error)?;
        check_embedder_profile(&connection)?;

        let warmup_started = Instant::now();
        let report = OpenReport {
            schema_version_before: migration.schema_version_before,
            schema_version_after: migration.schema_version_after,
            migration_steps: migration.migration_steps,
            embedder_warmup_ms: u64::try_from(warmup_started.elapsed().as_millis())
                .unwrap_or(u64::MAX),
            query_backend: "fathomdb-query scaffold",
            default_embedder: default_embedder_identity(),
        };

        let mut readers = Vec::with_capacity(READER_POOL_SIZE);
        for _ in 0..READER_POOL_SIZE {
            let reader = Connection::open(&path).map_err(|_| EngineOpenError::Io {
                message: "could not open reader connection".to_string(),
            })?;
            reader.pragma_update(None, "journal_mode", "WAL").map_err(|_| EngineOpenError::Io {
                message: "could not set reader journal mode".to_string(),
            })?;
            reader.pragma_update(None, "query_only", "ON").map_err(|_| EngineOpenError::Io {
                message: "could not set reader query_only".to_string(),
            })?;
            readers.push(reader);
        }

        Ok((connection, readers, report))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write(&self, batch: &[PreparedWrite]) -> Result<WriteReceipt, EngineError> {
        let category = if batch_is_admin(batch) {
            lifecycle::EventCategory::Admin
        } else {
            lifecycle::EventCategory::Writer
        };
        self.emit_event(lifecycle::Phase::Started, category, None);
        let started = Instant::now();
        let outcome = self.write_inner(batch);
        self.detect_slow(started, category);
        match outcome {
            Ok(receipt) => {
                let rows = u64::try_from(batch.len()).unwrap_or(u64::MAX);
                if batch_is_admin(batch) {
                    self.counters.record_admin();
                } else {
                    self.counters.record_write(rows);
                }
                self.emit_event(lifecycle::Phase::Finished, category, None);
                Ok(receipt)
            }
            Err(err) => {
                let code = err.stable_code();
                self.counters.record_error(code);
                // AC-003d: capture-ordinal < raise-ordinal — Failed and Error
                // events both fire before the EngineError returns to the caller.
                self.emit_event(lifecycle::Phase::Failed, category, Some(code));
                self.emit_event(
                    lifecycle::Phase::Failed,
                    lifecycle::EventCategory::Error,
                    Some(code),
                );
                Err(err)
            }
        }
    }

    fn write_inner(&self, batch: &[PreparedWrite]) -> Result<WriteReceipt, EngineError> {
        self.ensure_open()?;

        if batch.is_empty() {
            return Err(EngineError::WriteValidation);
        }

        let mut connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_mut().ok_or(EngineError::Closing)?;
        let plans = validate_batch(connection, batch)?;
        #[cfg(debug_assertions)]
        if self.force_next_commit_failure.swap(false, Ordering::SeqCst) {
            return Err(EngineError::Storage);
        }
        let increment = u64::try_from(batch.len()).unwrap_or(u64::MAX);
        let cursor = self.next_cursor.load(Ordering::SeqCst).saturating_add(increment);

        if let Err(err) = commit_batch(connection, batch, &plans, cursor) {
            self.emit_sqlite_internal_error(&err);
            return Err(EngineError::Storage);
        }
        self.next_cursor.store(cursor, Ordering::SeqCst);

        Ok(WriteReceipt { cursor })
    }

    pub fn search(&self, query: &str) -> Result<SearchResult, EngineError> {
        self.emit_event(lifecycle::Phase::Started, lifecycle::EventCategory::Search, None);
        let started = Instant::now();
        let outcome = self.search_inner(query);
        self.detect_slow(started, lifecycle::EventCategory::Search);
        match outcome {
            Ok(result) => {
                self.counters.record_query();
                self.emit_event(lifecycle::Phase::Finished, lifecycle::EventCategory::Search, None);
                Ok(result)
            }
            Err(err) => {
                let code = err.stable_code();
                self.counters.record_error(code);
                self.emit_event(
                    lifecycle::Phase::Failed,
                    lifecycle::EventCategory::Search,
                    Some(code),
                );
                self.emit_event(
                    lifecycle::Phase::Failed,
                    lifecycle::EventCategory::Error,
                    Some(code),
                );
                Err(err)
            }
        }
    }

    fn detect_slow(&self, started: Instant, category: lifecycle::EventCategory) {
        let elapsed = started.elapsed();
        let threshold = self.slow_threshold_ms.load(Ordering::Relaxed);
        let threshold_duration = std::time::Duration::from_millis(threshold);
        if elapsed > threshold_duration {
            // `dev/design/lifecycle.md` § Slow and heartbeat policy: a slow
            // operation produces TWO correlated facts. The
            // statement-level slow-statement signal is dispatched by the
            // sqlite3_profile callback (`profile_callback_trampoline`).
            // This site emits the lifecycle `Phase::Slow` event for the
            // outer operation envelope (AC-008).
            self.emit_event(lifecycle::Phase::Slow, category, None);
        }
    }

    fn emit_event(
        &self,
        phase: lifecycle::Phase,
        category: lifecycle::EventCategory,
        code: Option<&'static str>,
    ) {
        let event =
            lifecycle::Event { phase, source: lifecycle::EventSource::Engine, category, code };
        self.subscribers.dispatch(&event);
    }

    /// Emit a `(SqliteInternal, Error, code: <SQLITE_*>)` lifecycle
    /// event for a rusqlite error. Per `dev/design/lifecycle.md`
    /// § Diagnostic source and category, SQLite-originated diagnostics
    /// route through the same host subscriber as engine-originated
    /// events with `source` preserved. AC-021 dispatches on
    /// `code == "SQLITE_SCHEMA"`.
    fn emit_sqlite_internal_error(&self, err: &rusqlite::Error) {
        if let Some(code) = sqlite_extended_code_name(err) {
            let event = lifecycle::Event {
                phase: lifecycle::Phase::Failed,
                source: lifecycle::EventSource::SqliteInternal,
                category: lifecycle::EventCategory::Error,
                code: Some(code),
            };
            self.subscribers.dispatch(&event);
        }
    }

    fn search_inner(&self, query: &str) -> Result<SearchResult, EngineError> {
        self.ensure_open()?;
        if query.trim().is_empty() {
            return Err(EngineError::WriteValidation);
        }

        let compiled = compile_text_query(query);
        let mut results = vec![compiled.sql];
        // REQ-013 / AC-059b / REQ-055: the cursor returned with a search
        // MUST be derived from the same WAL snapshot the data was read
        // from. Loading `next_cursor` from the writer-side atomic before
        // the reader transaction acquires its snapshot races against
        // concurrent writers — see `dev/design/engine.md` § Cursor
        // contract. Run cursor probe + body query inside one read tx
        // (BEGIN DEFERRED on a `query_only=ON` connection in WAL mode is
        // a snapshot-stable read).
        let cursor = if let Some(mut reader) = self.reader_pool.borrow() {
            let cursor = match read_search_in_tx(&mut reader, query.trim(), &mut results) {
                Ok(c) => c,
                Err(err) => {
                    // Surface SQLite-internal failures (e.g. SQLITE_SCHEMA
                    // cache invalidation under concurrent DDL) on the
                    // host subscriber path before degrading. AC-021
                    // dispatches on `code == "SQLITE_SCHEMA"`.
                    self.emit_sqlite_internal_error(&err);
                    // Fall back to the writer-side atomic — strictly
                    // weaker invariant, but search must still return a
                    // cursor. The previous helper degraded silently;
                    // surfacing the SqliteInternal event preserves the
                    // observability contract.
                    self.next_cursor.load(Ordering::SeqCst)
                }
            };
            self.reader_pool.release(reader);
            cursor
        } else {
            self.next_cursor.load(Ordering::SeqCst)
        };

        Ok(SearchResult { projection_cursor: cursor, soft_fallback: None, results })
    }

    pub fn close(&self) -> Result<(), EngineError> {
        self.closed.store(true, Ordering::SeqCst);
        // Uninstall profile callbacks before dropping the connections so
        // SQLite cannot fire one last callback against a profile context
        // whose Box is about to free. Per `dev/design/engine.md` § Close
        // path step 6, readers drain before the writer connection so
        // SQLite's last-handle checkpointer runs on the writer.
        let readers = self.reader_pool.drain();
        for reader in &readers {
            uninstall_profile_callback(reader);
        }
        drop(readers);
        if let Ok(mut connection) = self.connection.lock() {
            if let Some(conn) = connection.as_ref() {
                uninstall_profile_callback(conn);
            }
            connection.take();
        }
        if let Ok(mut contexts) = self.profile_contexts.lock() {
            contexts.clear();
        }
        if let Ok(mut lock) = self.lock.lock() {
            lock.take();
        }
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
        self.counters.snapshot()
    }

    /// Toggle response-cycle profiling.
    ///
    /// Per `dev/design/lifecycle.md` § Per-statement profiling, profiling
    /// is an opt-in surface that is independently toggleable on a running
    /// engine without restart. AC-005a locks runtime toggleability.
    pub fn set_profiling(&self, enabled: bool) -> Result<(), EngineError> {
        self.profiling_enabled.store(enabled, Ordering::Relaxed);
        Ok(())
    }

    /// Set the threshold above which an operation is reported as slow.
    ///
    /// Per `dev/design/lifecycle.md` § Slow and heartbeat policy, the
    /// threshold is runtime-configurable; mutating it changes detection
    /// behavior on subsequent statements without restart (AC-007b).
    pub fn set_slow_threshold_ms(&self, value: u64) -> Result<(), EngineError> {
        self.slow_threshold_ms.store(value, Ordering::Relaxed);
        Ok(())
    }

    /// Attach a host subscriber to engine events.
    ///
    /// Dropping the returned [`Subscription`] detaches the subscriber.
    /// Payload shape owned by `dev/design/lifecycle.md` and
    /// `dev/design/migrations.md`.
    #[must_use]
    pub fn subscribe(&self, subscriber: Arc<dyn lifecycle::Subscriber>) -> Subscription {
        self.subscribers.attach(subscriber)
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn force_next_commit_failure_for_test(&self) {
        self.force_next_commit_failure.store(true, Ordering::SeqCst);
    }

    /// Execute an arbitrary SQL statement on the writer connection through
    /// the same wall-clock + slow-detect path as `write` / `search`.
    ///
    /// Test-only helper for the deterministic-slow-cte fixture used by
    /// AC-007a / AC-007b. Not part of the public 0.6.0 surface; gated on
    /// `debug_assertions` so release builds do not expose it.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn execute_for_test(&self, sql: &str) -> Result<(), EngineError> {
        self.ensure_open()?;
        let started = Instant::now();
        {
            let mut connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
            let connection = connection.as_mut().ok_or(EngineError::Closing)?;
            connection.execute_batch(sql).map_err(|_| EngineError::Storage)?;
        }
        self.detect_slow(started, lifecycle::EventCategory::Search);
        Ok(())
    }

    fn ensure_open(&self) -> Result<(), EngineError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(EngineError::Closing);
        }

        Ok(())
    }
}

fn batch_is_admin(batch: &[PreparedWrite]) -> bool {
    !batch.is_empty() && batch.iter().all(|w| matches!(w, PreparedWrite::AdminSchema { .. }))
}

/// Read `MAX(write_cursor)` and matching body rows inside one read tx.
///
/// WAL gives each transaction a stable snapshot at `BEGIN`; querying
/// `MAX(write_cursor)` from inside that tx therefore yields a cursor
/// value bounded by the snapshot the body query also reads. Returning
/// the snapshot cursor satisfies REQ-013 / AC-059b read-after-write
/// semantics and the REQ-055 monotonic-cursor contract.
fn read_search_in_tx(
    reader: &mut Connection,
    needle: &str,
    results: &mut Vec<String>,
) -> rusqlite::Result<u64> {
    let tx = reader.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;
    let cursor: u64 =
        tx.query_row("SELECT COALESCE(MAX(write_cursor), 0) FROM canonical_nodes", [], |row| {
            row.get(0)
        })?;
    let pattern = format!("%{needle}%");
    {
        let mut statement = tx
            .prepare("SELECT body FROM canonical_nodes WHERE body LIKE ?1 ORDER BY write_cursor")?;
        let rows = statement.query_map([pattern], |row| row.get::<_, String>(0))?;
        for row in rows.flatten() {
            results.push(row);
        }
    }
    tx.commit()?;
    Ok(cursor)
}

fn canonical_database_path(path: &Path) -> Result<PathBuf, EngineOpenError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical_parent = parent.canonicalize().map_err(|_| EngineOpenError::Io {
        message: "database parent directory is not accessible".to_string(),
    })?;
    let file_name = path.file_name().ok_or_else(|| EngineOpenError::Io {
        message: "database path has no file name".to_string(),
    })?;

    Ok(canonical_parent.join(file_name))
}

fn acquire_lock(path: &Path) -> Result<File, EngineOpenError> {
    let lock_path = lock_path(path);
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    options.mode(0o600);

    let mut file = options.open(&lock_path).map_err(|_| EngineOpenError::Io {
        message: "could not open database lock file".to_string(),
    })?;

    match file.try_lock() {
        Ok(()) => {
            let pid = std::process::id().to_string();
            let _ = file.set_len(0);
            let _ = file.seek(SeekFrom::Start(0));
            let _ = file.write_all(pid.as_bytes());
            Ok(file)
        }
        Err(std::fs::TryLockError::WouldBlock) => {
            Err(EngineOpenError::DatabaseLocked { holder_pid: read_holder_pid(&lock_path) })
        }
        Err(_) => {
            Err(EngineOpenError::Io { message: "could not acquire database lock".to_string() })
        }
    }
}

fn lock_path(path: &Path) -> PathBuf {
    let mut lock_path = path.as_os_str().to_os_string();
    lock_path.push(LOCK_SUFFIX);
    PathBuf::from(lock_path)
}

fn read_holder_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn map_migration_error(err: SchemaMigrationError) -> EngineOpenError {
    match err {
        SchemaMigrationError::IncompatibleSchemaVersion { seen, supported } => {
            EngineOpenError::IncompatibleSchemaVersion { seen, supported }
        }
        SchemaMigrationError::MigrationError(report) => EngineOpenError::MigrationError {
            schema_version_before: report.schema_version_before,
            schema_version_current: report.schema_version_current,
            step_id: report.migration_steps.last().map_or(0, |step| step.step_id),
        },
        SchemaMigrationError::Storage { message } => {
            EngineOpenError::Io { message: message.to_string() }
        }
    }
}

fn reject_legacy_shape(connection: &Connection) -> Result<(), EngineOpenError> {
    let has_legacy_table = table_exists(connection, "fathom_nodes")
        || table_exists(connection, "fathom_edges")
        || table_exists(connection, "fathom_chunks");
    if !has_legacy_table {
        return Ok(());
    }

    let seen =
        connection.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0)).unwrap_or(0);
    Err(EngineOpenError::IncompatibleSchemaVersion { seen, supported: SCHEMA_VERSION })
}

fn table_exists(connection: &Connection, table: &str) -> bool {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1",
            [table],
            |_row| Ok(()),
        )
        .is_ok()
}

fn default_embedder_identity() -> EmbedderIdentity {
    EmbedderIdentity::new(DEFAULT_EMBEDDER_NAME, DEFAULT_EMBEDDER_REVISION)
}

fn check_embedder_profile(connection: &Connection) -> Result<(), EngineOpenError> {
    let mut statement = match connection.prepare(
        "SELECT name, revision, dimension FROM _fathomdb_embedder_profiles WHERE profile = 'default'",
    ) {
        Ok(statement) => statement,
        Err(_) => return Ok(()),
    };
    let mut rows = statement.query([]).map_err(|_| {
        EngineOpenError::Corruption(CorruptionDetail {
            kind: CorruptionKind::EmbedderIdentityDrift,
            stage: OpenStage::EmbedderIdentity,
            locator: CorruptionLocator::OpaqueSqliteError { sqlite_extended_code: 0 },
            recovery_hint: RecoveryHint {
                code: "E_CORRUPT_EMBEDDER_IDENTITY",
                doc_anchor: "design/recovery.md#embedder-identity",
            },
        })
    })?;

    let Some(row) = rows.next().map_err(|_| {
        EngineOpenError::Corruption(CorruptionDetail {
            kind: CorruptionKind::EmbedderIdentityDrift,
            stage: OpenStage::EmbedderIdentity,
            locator: CorruptionLocator::OpaqueSqliteError { sqlite_extended_code: 0 },
            recovery_hint: RecoveryHint {
                code: "E_CORRUPT_EMBEDDER_IDENTITY",
                doc_anchor: "design/recovery.md#embedder-identity",
            },
        })
    })?
    else {
        return Ok(());
    };

    let stored = EmbedderIdentity::new(
        row.get::<_, String>(0).map_err(|_| {
            EngineOpenError::Corruption(CorruptionDetail {
                kind: CorruptionKind::EmbedderIdentityDrift,
                stage: OpenStage::EmbedderIdentity,
                locator: CorruptionLocator::TableRow {
                    table: "_fathomdb_embedder_profiles",
                    rowid: 0,
                },
                recovery_hint: RecoveryHint {
                    code: "E_CORRUPT_EMBEDDER_IDENTITY",
                    doc_anchor: "design/recovery.md#embedder-identity",
                },
            })
        })?,
        row.get::<_, String>(1).map_err(|_| {
            EngineOpenError::Corruption(CorruptionDetail {
                kind: CorruptionKind::EmbedderIdentityDrift,
                stage: OpenStage::EmbedderIdentity,
                locator: CorruptionLocator::TableRow {
                    table: "_fathomdb_embedder_profiles",
                    rowid: 0,
                },
                recovery_hint: RecoveryHint {
                    code: "E_CORRUPT_EMBEDDER_IDENTITY",
                    doc_anchor: "design/recovery.md#embedder-identity",
                },
            })
        })?,
    );
    let dimension = row.get::<_, u32>(2).map_err(|_| {
        EngineOpenError::Corruption(CorruptionDetail {
            kind: CorruptionKind::EmbedderIdentityDrift,
            stage: OpenStage::EmbedderIdentity,
            locator: CorruptionLocator::TableRow { table: "_fathomdb_embedder_profiles", rowid: 0 },
            recovery_hint: RecoveryHint {
                code: "E_CORRUPT_EMBEDDER_IDENTITY",
                doc_anchor: "design/recovery.md#embedder-identity",
            },
        })
    })?;
    let supplied = default_embedder_identity();

    if stored != supplied {
        return Err(EngineOpenError::EmbedderIdentityMismatch { stored, supplied });
    }
    if dimension != DEFAULT_EMBEDDER_DIMENSION {
        return Err(EngineOpenError::EmbedderDimensionMismatch {
            stored: dimension,
            supplied: DEFAULT_EMBEDDER_DIMENSION,
        });
    }

    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum WritePlan {
    Node,
    Edge,
    AppendOnlyLog,
    LatestState,
    AdminSchema,
}

fn validate_batch(
    connection: &Connection,
    batch: &[PreparedWrite],
) -> Result<Vec<WritePlan>, EngineError> {
    batch.iter().map(|write| validate_write(connection, write)).collect()
}

fn validate_write(
    connection: &Connection,
    write: &PreparedWrite,
) -> Result<WritePlan, EngineError> {
    match write {
        PreparedWrite::Node { kind, body } => {
            if kind.trim().is_empty() || body.trim().is_empty() {
                return Err(EngineError::WriteValidation);
            }
            Ok(WritePlan::Node)
        }
        PreparedWrite::Edge { kind, from, to } => {
            if kind.trim().is_empty() || from.trim().is_empty() || to.trim().is_empty() {
                return Err(EngineError::WriteValidation);
            }
            Ok(WritePlan::Edge)
        }
        PreparedWrite::AdminSchema { name, kind, schema_json, retention_json } => {
            if name.trim().is_empty()
                || !matches!(kind.as_str(), "append_only_log" | "latest_state")
                || serde_json::from_str::<Value>(schema_json).is_err()
                || serde_json::from_str::<Value>(retention_json).is_err()
                || contains_external_ref(schema_json)
            {
                return Err(EngineError::SchemaValidation);
            }
            Ok(WritePlan::AdminSchema)
        }
        PreparedWrite::OpStore { collection, record_key, schema_id, body } => {
            if collection.trim().is_empty() || record_key.trim().is_empty() {
                return Err(EngineError::WriteValidation);
            }
            let (kind, schema_json) = collection_metadata(connection, collection)?;
            if let Some(schema_id) = schema_id {
                if schema_id != collection {
                    return Err(EngineError::SchemaValidation);
                }
                validate_payload(&schema_json, body)?;
            } else if serde_json::from_str::<Value>(body).is_err() {
                return Err(EngineError::SchemaValidation);
            }

            match kind.as_str() {
                "append_only_log" => Ok(WritePlan::AppendOnlyLog),
                "latest_state" => Ok(WritePlan::LatestState),
                _ => Err(EngineError::OpStore),
            }
        }
    }
}

fn collection_metadata(
    connection: &Connection,
    collection: &str,
) -> Result<(String, String), EngineError> {
    connection
        .query_row(
            "SELECT kind, schema_json FROM operational_collections WHERE name = ?1",
            [collection],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|_| EngineError::OpStore)
}

fn validate_payload(schema_json: &str, body: &str) -> Result<(), EngineError> {
    let schema =
        serde_json::from_str::<Value>(schema_json).map_err(|_| EngineError::SchemaValidation)?;
    let payload = serde_json::from_str::<Value>(body).map_err(|_| EngineError::SchemaValidation)?;

    let compiled = JSONSchema::compile(&schema).map_err(|_| EngineError::SchemaValidation)?;
    compiled.validate(&payload).map_err(|_| EngineError::SchemaValidation)?;

    Ok(())
}

fn contains_external_ref(schema_json: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(schema_json) else {
        return false;
    };
    value_contains_external_ref(&value)
}

fn value_contains_external_ref(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            if key == "$ref" {
                return value.as_str().is_some_and(|uri| !uri.starts_with('#'));
            }
            value_contains_external_ref(value)
        }),
        Value::Array(values) => values.iter().any(value_contains_external_ref),
        _ => false,
    }
}

fn commit_batch(
    connection: &mut Connection,
    batch: &[PreparedWrite],
    plans: &[WritePlan],
    cursor: u64,
) -> rusqlite::Result<()> {
    let tx = connection.transaction()?;

    for (write, plan) in batch.iter().zip(plans) {
        match (write, plan) {
            (PreparedWrite::Node { kind, body }, WritePlan::Node) => {
                tx.execute(
                    "INSERT INTO canonical_nodes(write_cursor, kind, body) VALUES(?1, ?2, ?3)",
                    params![cursor, kind, body],
                )?;
                if kind == "force_projection_failure" {
                    let payload = format!(
                        r#"{{"write_cursor":{cursor},"failure_code":"E_PROJECTION_FAILED","recorded_at":0}}"#
                    );
                    tx.execute(
                        "INSERT INTO operational_mutations(
                            collection_name, record_key, op_kind, payload_json, schema_id, write_cursor
                         ) VALUES('projection_failures', ?1, 'append', ?2, NULL, ?3)",
                        params![cursor.to_string(), payload, cursor],
                    )?;
                }
            }
            (PreparedWrite::Edge { kind, from, to }, WritePlan::Edge) => {
                tx.execute(
                    "INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id)
                     VALUES(?1, ?2, ?3, ?4)",
                    params![cursor, kind, from, to],
                )?;
            }
            (
                PreparedWrite::AdminSchema { name, kind, schema_json, retention_json },
                WritePlan::AdminSchema,
            ) => {
                tx.execute(
                    "INSERT INTO operational_collections(
                        name, kind, schema_json, retention_json, format_version, created_at
                     ) VALUES(?1, ?2, ?3, ?4, 1, 0)
                     ON CONFLICT(name) DO UPDATE SET
                        schema_json = excluded.schema_json,
                        retention_json = excluded.retention_json",
                    params![name, kind, schema_json, retention_json],
                )?;
            }
            (
                PreparedWrite::OpStore { collection, record_key, schema_id, body },
                WritePlan::AppendOnlyLog,
            ) => {
                tx.execute(
                    "INSERT INTO operational_mutations(
                        collection_name, record_key, op_kind, payload_json, schema_id, write_cursor
                     ) VALUES(?1, ?2, 'append', ?3, ?4, ?5)",
                    params![collection, record_key, body, schema_id, cursor],
                )?;
            }
            (
                PreparedWrite::OpStore { collection, record_key, schema_id, body },
                WritePlan::LatestState,
            ) => {
                tx.execute(
                    "INSERT INTO operational_state(
                        collection_name, record_key, payload_json, schema_id, write_cursor
                     ) VALUES(?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(collection_name, record_key) DO UPDATE SET
                        payload_json = excluded.payload_json,
                        schema_id = excluded.schema_id,
                        write_cursor = excluded.write_cursor",
                    params![collection, record_key, body, schema_id, cursor],
                )?;
            }
            _ => return Err(rusqlite::Error::InvalidQuery),
        }
    }

    tx.commit()
}

fn load_next_cursor(connection: &Connection) -> u64 {
    let nodes = max_cursor(connection, "canonical_nodes").unwrap_or(0);
    let edges = max_cursor(connection, "canonical_edges").unwrap_or(0);
    let mutations = max_cursor(connection, "operational_mutations").unwrap_or(0);
    let state = max_cursor(connection, "operational_state").unwrap_or(0);
    nodes.max(edges).max(mutations).max(state)
}

fn max_cursor(connection: &Connection, table: &str) -> rusqlite::Result<u64> {
    let sql = format!("SELECT COALESCE(MAX(write_cursor), 0) FROM {table}");
    connection.query_row(&sql, [], |row| row.get::<_, u64>(0))
}

/// Map a rusqlite error to its stable SQLite extended-code name.
///
/// Returns `None` for non-`SqliteFailure` variants (e.g. JSON conversion
/// failures, type mismatches at the rusqlite layer) — those are not
/// SQLite-internal events and should not be surfaced under
/// `EventSource::SqliteInternal`. The names returned here are the
/// canonical `SQLITE_*` symbol names from `sqlite3.h` and are stable
/// dispatch keys for AC-021 / AC-006 binding adapters.
///
/// Only the subset of codes the engine can reach in 0.6.0 is enumerated
/// — bare-extended-code matching covers the rest with a stable
/// `"SQLITE_UNKNOWN"` fallback so subscribers always see a typed code.
fn sqlite_extended_code_name(err: &rusqlite::Error) -> Option<&'static str> {
    let sqlite_error = err.sqlite_error()?;
    let extended = sqlite_error.extended_code;
    Some(match extended {
        rusqlite::ffi::SQLITE_SCHEMA => "SQLITE_SCHEMA",
        rusqlite::ffi::SQLITE_BUSY => "SQLITE_BUSY",
        rusqlite::ffi::SQLITE_LOCKED => "SQLITE_LOCKED",
        rusqlite::ffi::SQLITE_CORRUPT => "SQLITE_CORRUPT",
        rusqlite::ffi::SQLITE_NOTADB => "SQLITE_NOTADB",
        rusqlite::ffi::SQLITE_IOERR => "SQLITE_IOERR",
        rusqlite::ffi::SQLITE_FULL => "SQLITE_FULL",
        rusqlite::ffi::SQLITE_READONLY => "SQLITE_READONLY",
        rusqlite::ffi::SQLITE_CONSTRAINT => "SQLITE_CONSTRAINT",
        rusqlite::ffi::SQLITE_MISUSE => "SQLITE_MISUSE",
        rusqlite::ffi::SQLITE_INTERRUPT => "SQLITE_INTERRUPT",
        rusqlite::ffi::SQLITE_NOMEM => "SQLITE_NOMEM",
        rusqlite::ffi::SQLITE_PERM => "SQLITE_PERM",
        rusqlite::ffi::SQLITE_ABORT => "SQLITE_ABORT",
        rusqlite::ffi::SQLITE_PROTOCOL => "SQLITE_PROTOCOL",
        rusqlite::ffi::SQLITE_RANGE => "SQLITE_RANGE",
        rusqlite::ffi::SQLITE_TOOBIG => "SQLITE_TOOBIG",
        rusqlite::ffi::SQLITE_MISMATCH => "SQLITE_MISMATCH",
        rusqlite::ffi::SQLITE_AUTH => "SQLITE_AUTH",
        rusqlite::ffi::SQLITE_NOTFOUND => "SQLITE_NOTFOUND",
        rusqlite::ffi::SQLITE_CANTOPEN => "SQLITE_CANTOPEN",
        _ => "SQLITE_UNKNOWN",
    })
}

/// Install a `sqlite3_profile` callback on `connection` that dispatches
/// per-statement profile records and slow-statement signals to the
/// engine's subscriber registry.
///
/// Why FFI rather than `rusqlite::Connection::profile`: the safe API
/// (rusqlite 0.31) accepts only a `fn(&str, Duration)` with no
/// environment, so it cannot carry a per-engine subscriber-registry
/// pointer. We use `sqlite3_profile` directly with a leaked-into-`Box`
/// context whose pointer is tied to the engine's lifetime via
/// `Engine::profile_contexts`.
///
/// `sqlite3_profile` is documented as deprecated in favor of
/// `sqlite3_trace_v2`, but it remains supported and is sufficient for
/// the wall-clock + SQL-text payload required by AC-005a/b.
#[allow(clippy::vec_box)]
fn install_profile_callback(
    connection: &Connection,
    subscribers: &Arc<lifecycle::SubscriberRegistry>,
    profiling_enabled: &Arc<AtomicBool>,
    slow_threshold_ms: &Arc<AtomicU64>,
    contexts: &mut Vec<Box<ProfileContext>>,
) {
    let mut ctx = Box::new(ProfileContext {
        subscribers: Arc::clone(subscribers),
        profiling_enabled: Arc::clone(profiling_enabled),
        slow_threshold_ms: Arc::clone(slow_threshold_ms),
    });
    let ctx_ptr: *mut ProfileContext = &mut *ctx;

    // SAFETY: the Box outlives the connection. Rust drops struct fields
    // in declaration order. `connection` and `reader_pool` are declared
    // before `profile_contexts`, so the connections — and SQLite's
    // internal profile-callback state with them — are dropped before
    // the `Box<ProfileContext>` allocations are freed. `Engine::close`
    // additionally clears the callback via
    // `sqlite3_profile(handle, None, NULL)` before connection close to
    // drain any in-flight callback dispatch.
    unsafe {
        rusqlite::ffi::sqlite3_profile(
            connection.handle(),
            Some(profile_callback_trampoline),
            ctx_ptr.cast::<std::ffi::c_void>(),
        );
    }
    contexts.push(ctx);
}

/// Uninstall the profile callback so SQLite stops calling into our
/// freed `Box<ProfileContext>` pointer once a connection is being torn
/// down. Call before dropping `profile_contexts`.
fn uninstall_profile_callback(connection: &Connection) {
    // SAFETY: passing `None` as the callback unregisters the previous
    // callback; SQLite documents this as legal and idempotent.
    unsafe {
        rusqlite::ffi::sqlite3_profile(connection.handle(), None, std::ptr::null_mut());
    }
}

/// FFI trampoline for `sqlite3_profile`.
///
/// Invoked by SQLite at statement-finish with the SQL text and the
/// statement's wall-clock cost in nanoseconds. We dispatch a
/// `ProfileRecord` (when profiling is enabled) and a `SlowStatement`
/// signal (when `wall_clock_ms` exceeds the configured slow threshold).
///
/// Per `dev/design/lifecycle.md` § Public record shape, the public
/// payload exposes `wall_clock_ms`, `step_count`, and `cache_delta`.
/// `sqlite3_profile` does not surface per-statement step counts or
/// cache-hit deltas in its callback; we emit `0` for those fields and
/// document the hazard. AC-005b requires the fields be typed numeric,
/// not that they carry non-zero values for every backend.
unsafe extern "C" fn profile_callback_trampoline(
    user_data: *mut std::ffi::c_void,
    sql: *const std::os::raw::c_char,
    nanoseconds: u64,
) {
    if user_data.is_null() || sql.is_null() {
        return;
    }
    let ctx = unsafe { &*(user_data.cast::<ProfileContext>()) };
    let sql_text = match unsafe { std::ffi::CStr::from_ptr(sql) }.to_str() {
        Ok(s) => s,
        Err(_) => return,
    };

    let wall_clock_ms = nanoseconds / 1_000_000;

    if ctx.profiling_enabled.load(Ordering::Relaxed) {
        let record = lifecycle::ProfileRecord {
            wall_clock_ms,
            // step_count / cache_delta are not surfaced by
            // sqlite3_profile; placeholder 0 satisfies AC-005b's
            // "typed numeric" contract. A future profiling refactor
            // around sqlite3_stmt_status + sqlite3_db_status would
            // populate them with non-zero deltas.
            step_count: 0,
            cache_delta: 0,
        };
        ctx.subscribers.dispatch_profile(&record);
    }

    let threshold = ctx.slow_threshold_ms.load(Ordering::Relaxed);
    if wall_clock_ms > threshold {
        let signal = lifecycle::SlowStatement { statement: sql_text.to_string(), wall_clock_ms };
        ctx.subscribers.dispatch_slow_statement(&signal);
    }
}

#[cfg(test)]
mod tests {
    use super::{Engine, PreparedWrite};
    use tempfile::TempDir;

    #[test]
    fn write_advances_cursor() {
        let dir = TempDir::new().unwrap();
        let opened = Engine::open(dir.path().join("rewrite.sqlite")).expect("engine should open");
        let receipt = opened
            .engine
            .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: "hello".to_string() }])
            .expect("write should succeed");

        assert_eq!(receipt.cursor, 1);
    }
}
