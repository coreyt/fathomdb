pub mod lifecycle;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Once;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use fathomdb_embedder_api::{Embedder, EmbedderError as RuntimeEmbedderError, EmbedderIdentity};
use fathomdb_query::compile_text_query;
use fathomdb_schema::{
    migrate_with_event_sink, MigrationError as SchemaMigrationError, MigrationStepReport,
    LOCK_SUFFIX, MIGRATIONS, SCHEMA_VERSION,
};
use jsonschema::JSONSchema;
use rusqlite::{params, Connection};
use serde_json::Value;
use sqlite_vec::sqlite3_vec_init;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

const DEFAULT_EMBEDDER_NAME: &str = "fathomdb-noop";
const DEFAULT_EMBEDDER_REVISION: &str = "0.6.0-scaffold";
const DEFAULT_EMBEDDER_DIMENSION: u32 = 384;

/// REQ-006a / AC-007a default slow-statement threshold. Mutated at runtime
/// via [`Engine::set_slow_threshold_ms`].
const DEFAULT_SLOW_THRESHOLD_MS: u64 = 100;
const DEFAULT_VECTOR_PROFILE: &str = "default";
const DEFAULT_VECTOR_PARTITION: &str = "vector_default";
const DEFAULT_PROVENANCE_ROW_CAP: u64 = 1_000_000;
const PROJECTION_CURSOR_KEY: &str = "projection_cursor";
const PROJECTION_WORKERS: usize = 2;
const PROJECTION_INFLIGHT_LIMIT: usize = PROJECTION_WORKERS * 4;
const PROJECTION_COMMIT_BATCH: usize = 16;
const DEFAULT_PROJECTION_RETRY_DELAYS_MS: [u64; 3] = [1_000, 4_000, 16_000];

/// Reader pool size. Per `dev/design/engine.md` § Writer / reader split,
/// reader connections are pooled and never serialize behind one
/// connection. AC-021 exercises 8 concurrent readers.
const READER_POOL_SIZE: usize = 8;

/// Per-reader-connection lookaside slot size, in bytes. Pack 6.G G.1.
/// Picked from G.0 telemetry (`allocator_lookaside` 26.67% conc cycles
/// with 3.89× ratio) + the SQLite docs' typical-workload sizing
/// guidance (https://www.sqlite.org/malloc.html §3): 1200-byte slots
/// cover the small allocations from `sqlite3DbMallocRaw`,
/// `sqlite3Fts5ExprNew`, and `vec0Filter_knn` visible at the top of the
/// concurrent profile.
const READER_LOOKASIDE_SLOT_SIZE: std::os::raw::c_int = 1200;

/// Per-reader-connection lookaside slot count. SQLite default is 128;
/// we use 500 to absorb the per-statement allocation footprint of the
/// hybrid search workload across a sticky worker connection without
/// falling back to the glibc malloc-arena mutex.
const READER_LOOKASIDE_SLOT_COUNT: std::os::raw::c_int = 500;

pub struct Engine {
    path: PathBuf,
    next_cursor: AtomicU64,
    closed: AtomicBool,
    lock: Mutex<Option<File>>,
    connection: Mutex<Option<Connection>>,
    reader_pool: ReaderWorkerPool,
    counters: lifecycle::Counters,
    subscribers: Arc<lifecycle::SubscriberRegistry>,
    profiling_enabled: Arc<AtomicBool>,
    slow_threshold_ms: Arc<AtomicU64>,
    runtime_embedder: Option<Arc<dyn Embedder>>,
    runtime_embedder_identity: EmbedderIdentity,
    projection_runtime: ProjectionRuntime,
    provenance_row_cap: AtomicU64,
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
    /// Pack 6.G G.1 — `sqlite3_db_config(LOOKASIDE)` rc per reader
    /// worker, captured at open time before any PRAGMA / prepare ran
    /// on the connection. Read only by the debug-only test accessor
    /// `reader_lookaside_config_rcs_for_test`; held in release builds
    /// too because the field is set unconditionally at open and a cfg
    /// gate would force two open-locked return shapes.
    #[allow(dead_code)]
    reader_lookaside_rcs: Vec<i32>,
    #[cfg(debug_assertions)]
    force_next_commit_failure: AtomicBool,
}

#[derive(Clone, Debug)]
struct ProjectionJob {
    cursor: u64,
    kind: String,
    body: String,
}

#[derive(Debug, Default)]
struct ProjectionRuntimeState {
    active_jobs: usize,
    queued_jobs: usize,
    frozen: bool,
    pending_scan: bool,
    stopping: bool,
    in_flight: BTreeSet<u64>,
}

struct ProjectionRuntimeShared {
    path: PathBuf,
    embedder: Option<Arc<dyn Embedder>>,
    embedder_identity: EmbedderIdentity,
    state: Mutex<ProjectionRuntimeState>,
    state_cvar: Condvar,
    queue: Mutex<VecDeque<ProjectionJob>>,
    queue_cvar: Condvar,
    retry_delays_ms: Mutex<Vec<u64>>,
}

impl std::fmt::Debug for ProjectionRuntimeShared {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectionRuntimeShared")
            .field("path", &self.path)
            .field("embedder_identity", &self.embedder_identity)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct ProjectionRuntime {
    shared: Arc<ProjectionRuntimeShared>,
    dispatcher: Mutex<Option<JoinHandle<()>>>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("path", &self.path)
            .field("closed", &self.closed.load(Ordering::SeqCst))
            .field("runtime_embedder_identity", &self.runtime_embedder_identity)
            .finish_non_exhaustive()
    }
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

/// Thread-affine reader worker pool (Pack 6 F.0).
///
/// Per `dev/design/engine.md` § Writer / reader split, reader connections
/// must not serialize behind a single mutex. Each worker thread owns
/// exactly one read-only `Connection` for its lifetime; `Connection`
/// objects never cross thread boundaries after startup. `Engine::search`
/// dispatches a request via a per-worker bounded channel using a
/// lock-free round-robin counter on the hot path.
struct ReaderWorkerPool {
    senders: Vec<SyncSender<ReaderRequest>>,
    handles: Mutex<Option<Vec<JoinHandle<()>>>>,
    next: AtomicUsize,
    shutdown: AtomicBool,
    live_workers: Arc<AtomicUsize>,
}

impl std::fmt::Debug for ReaderWorkerPool {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReaderWorkerPool")
            .field("worker_count", &self.senders.len())
            .field("live_workers", &self.live_workers.load(Ordering::Relaxed))
            .field("shutdown", &self.shutdown.load(Ordering::Relaxed))
            .finish()
    }
}

/// One request handled by exactly one reader worker. The response is
/// returned through a fresh oneshot channel so requests cannot be
/// routed to or duplicated across workers.
enum ReaderRequest {
    Search {
        compiled: fathomdb_query::CompiledQuery,
        query_vector: Option<String>,
        respond: SyncSender<ReaderResponse>,
    },
    Shutdown,
    /// Pack 6.G G.1 — debug-only request that asks a worker to read its
    /// own connection's `SQLITE_DBSTATUS_LOOKASIDE_USED` and return the
    /// high-water mark (`hiwtr` out-param). Used solely by the integration
    /// test that asserts post-warmup lookaside slots were consumed; not
    /// on any production path.
    #[cfg(debug_assertions)]
    LookasideStatus {
        respond: SyncSender<i32>,
    },
    /// Pack 6.G G.3.5 — debug-only request that asks a worker to read
    /// `SQLITE_DBSTATUS_CACHE_HIT`, `_CACHE_MISS`, and `_CACHE_USED`
    /// off its own connection and return them as `(hit, miss, used_bytes)`.
    /// `snapshot_label` is opaque to the worker; the caller uses it to
    /// distinguish pre/post snapshots in its own bookkeeping.
    #[cfg(debug_assertions)]
    CacheStatus {
        snapshot_label: String,
        respond: SyncSender<(String, i32, i32, i32)>,
    },
}

type ReaderResponse = rusqlite::Result<(u64, Option<SoftFallback>, Vec<String>)>;

/// Pack 6.G G.3.5 — per-worker cache-pressure snapshot. Carried only on
/// the debug-only `CacheStatus` broadcast path and the test accessor;
/// not part of the public 0.6.0 surface.
#[cfg(debug_assertions)]
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct CacheStatusReply {
    pub worker_idx: usize,
    pub snapshot_label: String,
    pub cache_hit: i32,
    pub cache_miss: i32,
    pub cache_used_bytes: i32,
}

/// Per-worker outbound channel capacity. Round-robin dispatch keeps
/// queue depth at ~0 on hot paths; the small slack absorbs jitter
/// without a runtime mutex.
const READER_WORKER_CHANNEL_CAPACITY: usize = 4;

impl ReaderWorkerPool {
    fn new(connections: Vec<Connection>) -> Self {
        let live_workers = Arc::new(AtomicUsize::new(0));
        let mut senders = Vec::with_capacity(connections.len());
        let mut handles = Vec::with_capacity(connections.len());
        for (idx, connection) in connections.into_iter().enumerate() {
            let (tx, rx) = mpsc::sync_channel::<ReaderRequest>(READER_WORKER_CHANNEL_CAPACITY);
            let live = Arc::clone(&live_workers);
            let handle = thread::Builder::new()
                .name(format!("fathomdb-reader-{idx}"))
                .spawn(move || reader_worker_loop(connection, rx, live))
                .expect("spawn reader worker");
            senders.push(tx);
            handles.push(handle);
        }
        Self {
            senders,
            handles: Mutex::new(Some(handles)),
            next: AtomicUsize::new(0),
            shutdown: AtomicBool::new(false),
            live_workers,
        }
    }

    fn worker_count(&self) -> usize {
        self.senders.len()
    }

    fn live_count(&self) -> usize {
        self.live_workers.load(Ordering::SeqCst)
    }

    /// Pack 6.G G.1 — broadcast a `LookasideStatus` request to every
    /// worker (not round-robin) and collect each worker's
    /// `SQLITE_DBSTATUS_LOOKASIDE_USED`. Used only by the debug
    /// integration test for post-warmup lookaside-slot consumption.
    #[cfg(debug_assertions)]
    fn lookaside_used_per_worker(&self) -> Vec<i32> {
        let mut results = Vec::with_capacity(self.senders.len());
        for sender in &self.senders {
            let (tx, rx) = mpsc::sync_channel::<i32>(1);
            if sender.send(ReaderRequest::LookasideStatus { respond: tx }).is_ok() {
                results.push(rx.recv().unwrap_or(-1));
            } else {
                results.push(-1);
            }
        }
        results
    }

    /// Pack 6.G G.3.5 — broadcast a `CacheStatus` request to every
    /// worker and collect each worker's `(cache_hit, cache_miss,
    /// cache_used_bytes)` triple. Same broadcast pattern as G.1's
    /// `lookaside_used_per_worker`. Returns one `CacheStatusReply` per
    /// worker in worker-index order.
    #[cfg(debug_assertions)]
    fn cache_status_per_worker(&self, snapshot_label: &str) -> Vec<CacheStatusReply> {
        let mut results = Vec::with_capacity(self.senders.len());
        for (idx, sender) in self.senders.iter().enumerate() {
            let (tx, rx) = mpsc::sync_channel::<(String, i32, i32, i32)>(1);
            let request = ReaderRequest::CacheStatus {
                snapshot_label: snapshot_label.to_string(),
                respond: tx,
            };
            if sender.send(request).is_ok() {
                if let Ok((label, hit, miss, used)) = rx.recv() {
                    results.push(CacheStatusReply {
                        worker_idx: idx,
                        snapshot_label: label,
                        cache_hit: hit,
                        cache_miss: miss,
                        cache_used_bytes: used,
                    });
                    continue;
                }
            }
            results.push(CacheStatusReply {
                worker_idx: idx,
                snapshot_label: snapshot_label.to_string(),
                cache_hit: -1,
                cache_miss: -1,
                cache_used_bytes: -1,
            });
        }
        results
    }

    /// Hot path. Lock-free dispatch: `AtomicUsize::fetch_add` selects
    /// the worker, then a single `SyncSender::send` enqueues the
    /// request. No global mutex is taken on the request path.
    fn dispatch(&self, request: ReaderRequest) -> Result<(), ReaderRequest> {
        if self.shutdown.load(Ordering::Relaxed) {
            return Err(request);
        }
        let n = self.senders.len();
        if n == 0 {
            return Err(request);
        }
        let idx = self.next.fetch_add(1, Ordering::Relaxed) % n;
        self.senders[idx].send(request).map_err(|err| err.0)
    }

    /// Signal every worker to exit and join its thread. Idempotent —
    /// safe to call from `Engine::close` and again from
    /// `ReaderWorkerPool::Drop`.
    fn shutdown(&self) {
        if self.shutdown.swap(true, Ordering::SeqCst) {
            return;
        }
        for sender in &self.senders {
            let _ = sender.send(ReaderRequest::Shutdown);
        }
        if let Ok(mut slot) = self.handles.lock() {
            if let Some(handles) = slot.take() {
                for handle in handles {
                    let _ = handle.join();
                }
            }
        }
    }
}

impl Drop for ReaderWorkerPool {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn reader_worker_loop(
    mut connection: Connection,
    rx: Receiver<ReaderRequest>,
    live_workers: Arc<AtomicUsize>,
) {
    live_workers.fetch_add(1, Ordering::SeqCst);
    // Drop guard so the live counter decrements even on panic.
    struct LiveGuard(Arc<AtomicUsize>);
    impl Drop for LiveGuard {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    let _guard = LiveGuard(live_workers);

    while let Ok(request) = rx.recv() {
        match request {
            ReaderRequest::Shutdown => break,
            ReaderRequest::Search { compiled, query_vector, respond } => {
                let result = read_search_in_tx(&mut connection, &compiled, query_vector.as_deref());
                // Receiver may have been dropped if the caller went
                // away; nothing to do in that case.
                let _ = respond.send(result);
            }
            #[cfg(debug_assertions)]
            ReaderRequest::LookasideStatus { respond } => {
                let _ = respond.send(read_lookaside_used_hiwtr(&connection));
            }
            #[cfg(debug_assertions)]
            ReaderRequest::CacheStatus { snapshot_label, respond } => {
                let (hit, miss, used) = read_cache_status(&connection);
                let _ = respond.send((snapshot_label, hit, miss, used));
            }
        }
    }

    // Per `dev/design/engine.md` § Close path, uninstall the profile
    // callback before dropping the connection so SQLite cannot fire
    // one last callback against a `ProfileContext` whose Box is about
    // to free.
    uninstall_profile_callback(&connection);
    drop(connection);
}

impl ProjectionRuntime {
    fn new(
        path: PathBuf,
        embedder: Option<Arc<dyn Embedder>>,
        embedder_identity: EmbedderIdentity,
    ) -> Self {
        let shared = Arc::new(ProjectionRuntimeShared {
            path,
            embedder,
            embedder_identity,
            state: Mutex::new(ProjectionRuntimeState::default()),
            state_cvar: Condvar::new(),
            queue: Mutex::new(VecDeque::new()),
            queue_cvar: Condvar::new(),
            retry_delays_ms: Mutex::new(DEFAULT_PROJECTION_RETRY_DELAYS_MS.to_vec()),
        });

        let dispatcher_shared = Arc::clone(&shared);
        let dispatcher = thread::spawn(move || projection_dispatcher_loop(dispatcher_shared));

        let mut workers = Vec::with_capacity(PROJECTION_WORKERS);
        for _ in 0..PROJECTION_WORKERS {
            let worker_shared = Arc::clone(&shared);
            workers.push(thread::spawn(move || projection_worker_loop(worker_shared)));
        }

        Self { shared, dispatcher: Mutex::new(Some(dispatcher)), workers: Mutex::new(workers) }
    }

    fn notify_new_work(&self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.pending_scan = true;
            self.shared.state_cvar.notify_all();
        }
    }

    fn set_frozen(&self, frozen: bool) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.frozen = frozen;
            if !frozen {
                state.pending_scan = true;
            }
            self.shared.state_cvar.notify_all();
        }
    }

    fn wait_for_idle(&self, timeout_ms: u64) -> bool {
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut state = match self.shared.state.lock() {
            Ok(state) => state,
            Err(_) => return false,
        };
        loop {
            if state.active_jobs == 0 && state.queued_jobs == 0 {
                drop(state);
                if !database_has_pending_projection_work(&self.shared.path).unwrap_or(true) {
                    return true;
                }
                state = match self.shared.state.lock() {
                    Ok(state) => state,
                    Err(_) => return false,
                };
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let wait = deadline.saturating_duration_since(now);
            let Ok((next_state, _)) = self.shared.state_cvar.wait_timeout(state, wait) else {
                return false;
            };
            state = next_state;
        }
    }

    fn set_retry_delays_for_test(&self, delays_ms: &[u64]) {
        if let Ok(mut delays) = self.shared.retry_delays_ms.lock() {
            *delays = delays_ms.to_vec();
        }
    }

    fn stop(&self) {
        if let Ok(mut state) = self.shared.state.lock() {
            if state.stopping {
                return;
            }
            state.stopping = true;
            state.pending_scan = false;
            self.shared.state_cvar.notify_all();
        }
        if let Ok(mut queue) = self.shared.queue.lock() {
            queue.clear();
            self.shared.queue_cvar.notify_all();
        }

        if let Ok(mut dispatcher) = self.dispatcher.lock() {
            if let Some(handle) = dispatcher.take() {
                let _ = handle.join();
            }
        }
        if let Ok(mut workers) = self.workers.lock() {
            for handle in workers.drain(..) {
                let _ = handle.join();
            }
        }
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
    EmbedderNotConfigured,
    KindNotVectorIndexed,
    EmbedderDimensionMismatch { expected: u32, actual: u32 },
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
            Self::EmbedderNotConfigured => write!(f, "embedder is not configured"),
            Self::KindNotVectorIndexed => write!(f, "kind is not configured for vector indexing"),
            Self::EmbedderDimensionMismatch { expected, actual } => {
                write!(f, "embedder dimension mismatch: expected {expected}, actual {actual}")
            }
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
            Self::EmbedderNotConfigured => "EmbedderNotConfiguredError",
            Self::KindNotVectorIndexed => "KindNotVectorIndexedError",
            Self::EmbedderDimensionMismatch { .. } => "EmbedderDimensionMismatchError",
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

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl Engine {
    pub fn open(path: impl Into<PathBuf>) -> Result<OpenedEngine, EngineOpenError> {
        Self::open_with_embedder_and_subscriber(
            path,
            default_embedder_identity(),
            None,
            None,
            &mut |_| {},
        )
    }

    pub fn open_with_migration_event_sink(
        path: impl Into<PathBuf>,
        mut emit_migration_event: impl FnMut(&MigrationStepReport),
    ) -> Result<OpenedEngine, EngineOpenError> {
        Self::open_with_embedder_and_subscriber(
            path,
            default_embedder_identity(),
            None,
            None,
            &mut emit_migration_event,
        )
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn open_with_migrations_for_test(
        path: impl Into<PathBuf>,
        migrations: &'static [fathomdb_schema::Migration],
        mut emit_migration_event: impl FnMut(&MigrationStepReport),
    ) -> Result<OpenedEngine, EngineOpenError> {
        Self::open_with_migrations(
            path,
            migrations,
            default_embedder_identity(),
            None,
            &mut emit_migration_event,
            None,
        )
    }

    #[doc(hidden)]
    pub fn open_with_subscriber_for_test(
        path: impl Into<PathBuf>,
        subscriber: Arc<dyn lifecycle::Subscriber>,
    ) -> Result<OpenedEngine, EngineOpenError> {
        Self::open_with_embedder_and_subscriber(
            path,
            default_embedder_identity(),
            None,
            Some(subscriber),
            &mut |_| {},
        )
    }

    #[doc(hidden)]
    pub fn open_without_embedder_for_test(
        path: impl Into<PathBuf>,
    ) -> Result<OpenedEngine, EngineOpenError> {
        Self::open_with_embedder_and_subscriber(
            path,
            default_embedder_identity(),
            None,
            None,
            &mut |_| {},
        )
    }

    #[doc(hidden)]
    pub fn open_with_embedder_for_test(
        path: impl Into<PathBuf>,
        embedder: Arc<dyn Embedder>,
    ) -> Result<OpenedEngine, EngineOpenError> {
        let identity = embedder.identity();
        Self::open_with_embedder_and_subscriber(path, identity, Some(embedder), None, &mut |_| {})
    }

    fn open_with_embedder_and_subscriber(
        path: impl Into<PathBuf>,
        embedder_identity: EmbedderIdentity,
        runtime_embedder: Option<Arc<dyn Embedder>>,
        initial_subscriber: Option<Arc<dyn lifecycle::Subscriber>>,
        emit_migration_event: &mut impl FnMut(&MigrationStepReport),
    ) -> Result<OpenedEngine, EngineOpenError> {
        Self::open_with_migrations(
            path,
            MIGRATIONS,
            embedder_identity,
            runtime_embedder,
            emit_migration_event,
            initial_subscriber,
        )
    }

    fn open_with_migrations(
        path: impl Into<PathBuf>,
        migrations: &'static [fathomdb_schema::Migration],
        embedder_identity: EmbedderIdentity,
        runtime_embedder: Option<Arc<dyn Embedder>>,
        emit_migration_event: &mut impl FnMut(&MigrationStepReport),
        initial_subscriber: Option<Arc<dyn lifecycle::Subscriber>>,
    ) -> Result<OpenedEngine, EngineOpenError> {
        let canonical_path = canonical_database_path(&path.into())?;
        let lock = acquire_lock(&canonical_path)?;
        let open_result = Self::open_locked(
            canonical_path.clone(),
            migrations,
            &embedder_identity,
            emit_migration_event,
        );

        match open_result {
            Ok((connection, readers, report, reader_lookaside_rcs)) => {
                let next_cursor = load_next_cursor(&connection);
                let subscribers = Arc::new(lifecycle::SubscriberRegistry::new());
                let profiling_enabled = Arc::new(AtomicBool::new(false));
                let slow_threshold_ms = Arc::new(AtomicU64::new(DEFAULT_SLOW_THRESHOLD_MS));
                let mut profile_contexts: Vec<Box<ProfileContext>> = Vec::new();
                let projection_runtime = ProjectionRuntime::new(
                    canonical_path.clone(),
                    runtime_embedder.clone(),
                    embedder_identity.clone(),
                );

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

                let opened = OpenedEngine {
                    engine: Self {
                        path: canonical_path.clone(),
                        next_cursor: AtomicU64::new(next_cursor),
                        closed: AtomicBool::new(false),
                        lock: Mutex::new(Some(lock)),
                        connection: Mutex::new(Some(connection)),
                        reader_pool: ReaderWorkerPool::new(readers),
                        counters: lifecycle::Counters::new(),
                        subscribers,
                        profiling_enabled,
                        slow_threshold_ms,
                        runtime_embedder,
                        runtime_embedder_identity: embedder_identity,
                        projection_runtime,
                        provenance_row_cap: AtomicU64::new(DEFAULT_PROVENANCE_ROW_CAP),
                        profile_contexts: Mutex::new(profile_contexts),
                        reader_lookaside_rcs,
                        #[cfg(debug_assertions)]
                        force_next_commit_failure: AtomicBool::new(false),
                    },
                    report,
                };
                if let Some(subscriber) = initial_subscriber {
                    opened.engine.subscribers.attach_persistent(subscriber);
                }
                if database_has_pending_projection_work(&canonical_path).unwrap_or(false) {
                    opened.engine.projection_runtime.notify_new_work();
                }
                Ok(opened)
            }
            Err(err) => {
                if let Some(subscriber) = initial_subscriber {
                    emit_open_error_event(&subscriber, &err);
                }
                drop(lock);
                Err(err)
            }
        }
    }

    fn open_locked(
        path: PathBuf,
        migrations: &'static [fathomdb_schema::Migration],
        embedder_identity: &EmbedderIdentity,
        emit_migration_event: &mut impl FnMut(&MigrationStepReport),
    ) -> Result<(Connection, Vec<Connection>, OpenReport, Vec<i32>), EngineOpenError> {
        register_sqlite_vec_extension();
        let connection = Connection::open(&path)
            .map_err(|err| map_open_sqlite_error(err, OpenStage::HeaderProbe))?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|err| map_open_sqlite_error(err, OpenStage::WalReplay))?;
        probe_open_integrity(&connection)?;

        reject_legacy_shape(&connection)?;
        let migration = migrate_with_event_sink(&connection, migrations, emit_migration_event)
            .map_err(map_migration_error)?;
        check_embedder_profile(&connection, embedder_identity)?;
        ensure_vector_partition(&connection, embedder_identity.dimension).map_err(|_| {
            EngineOpenError::Io { message: "could not initialize vector partition".to_string() }
        })?;

        let warmup_started = Instant::now();
        let report = OpenReport {
            schema_version_before: migration.schema_version_before,
            schema_version_after: migration.schema_version_after,
            migration_steps: migration.migration_steps,
            embedder_warmup_ms: u64::try_from(warmup_started.elapsed().as_millis())
                .unwrap_or(u64::MAX),
            query_backend: "fathomdb-query + sqlite-vec",
            default_embedder: embedder_identity.clone(),
        };

        let mut readers = Vec::with_capacity(READER_POOL_SIZE);
        let mut lookaside_rcs: Vec<i32> = Vec::with_capacity(READER_POOL_SIZE);
        for _ in 0..READER_POOL_SIZE {
            let reader = Connection::open(&path)
                .map_err(|err| map_open_sqlite_error(err, OpenStage::HeaderProbe))?;
            // Pack 6.G G.1: configure per-connection lookaside BEFORE
            // any PRAGMA / prepare runs on this reader. Reordering this
            // after the journal-mode / query_only PRAGMAs would let
            // SQLite silently ignore the lookaside setting.
            let rc: i32 = configure_reader_lookaside(&reader);
            debug_assert_eq!(
                rc,
                rusqlite::ffi::SQLITE_OK,
                "sqlite3_db_config(LOOKASIDE) must return SQLITE_OK on a freshly opened reader",
            );
            lookaside_rcs.push(rc);
            reader
                .pragma_update(None, "journal_mode", "WAL")
                .map_err(|err| map_open_sqlite_error(err, OpenStage::WalReplay))?;
            reader
                .pragma_update(None, "query_only", "ON")
                .map_err(|err| map_open_sqlite_error(err, OpenStage::SchemaProbe))?;
            readers.push(reader);
        }

        Ok((connection, readers, report, lookaside_rcs))
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
        let projection_jobs = collect_projection_jobs(connection, batch)?;
        #[cfg(debug_assertions)]
        if self.force_next_commit_failure.swap(false, Ordering::SeqCst) {
            return Err(EngineError::Storage);
        }
        let increment = u64::try_from(batch.len()).unwrap_or(u64::MAX);
        let cursor = self.next_cursor.load(Ordering::SeqCst).saturating_add(increment);
        let pending_projection = !projection_jobs.is_empty();

        if let Err(err) = commit_batch(
            connection,
            batch,
            &plans,
            cursor,
            pending_projection,
            self.provenance_row_cap.load(Ordering::Relaxed),
        ) {
            self.emit_sqlite_internal_error(&err);
            return Err(EngineError::Storage);
        }
        self.next_cursor.store(cursor, Ordering::SeqCst);
        if pending_projection {
            self.projection_runtime.notify_new_work();
        }

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
        // REQ-013 / AC-059b / REQ-055: the cursor returned with a search
        // MUST be derived from the same WAL snapshot the data was read
        // from. Loading `next_cursor` from the writer-side atomic before
        // the reader transaction acquires its snapshot races against
        // concurrent writers — see `dev/design/engine.md` § Cursor
        // contract. Run cursor probe + body query inside one read tx
        // (BEGIN DEFERRED on a `query_only=ON` connection in WAL mode is
        // a snapshot-stable read).
        let query_vector = self
            .runtime_embedder
            .as_ref()
            .and_then(|embedder| embedder.embed(query).ok())
            .and_then(|vector| serde_json::to_string(&vector).ok());
        let (response_tx, response_rx) = mpsc::sync_channel::<ReaderResponse>(1);
        let request = ReaderRequest::Search { compiled, query_vector, respond: response_tx };
        if self.reader_pool.dispatch(request).is_err() {
            return Err(EngineError::Closing);
        }
        let search_result = response_rx.recv().map_err(|_| EngineError::Storage)?;
        let (cursor, soft_fallback, results) = match search_result {
            Ok(result) => result,
            Err(err) => {
                self.emit_sqlite_internal_error(&err);
                return Err(EngineError::Storage);
            }
        };

        Ok(SearchResult { projection_cursor: cursor, soft_fallback, results })
    }

    pub fn close(&self) -> Result<(), EngineError> {
        self.closed.store(true, Ordering::SeqCst);
        self.projection_runtime.stop();
        // Uninstall profile callbacks before dropping the connections so
        // SQLite cannot fire one last callback against a profile context
        // whose Box is about to free. Per `dev/design/engine.md` § Close
        // path step 6, readers drain before the writer connection so
        // SQLite's last-handle checkpointer runs on the writer. Each
        // reader worker uninstalls its own callback inside
        // `reader_worker_loop` before dropping its connection, then
        // exits — `shutdown` joins those threads here.
        self.reader_pool.shutdown();
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
    pub fn drain(&self, timeout_ms: u64) -> Result<(), EngineError> {
        self.ensure_open()?;
        if self.projection_runtime.wait_for_idle(timeout_ms) {
            Ok(())
        } else {
            Err(EngineError::Scheduler)
        }
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
    pub fn reader_worker_count_for_test(&self) -> usize {
        self.reader_pool.worker_count()
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn live_reader_worker_count_for_test(&self) -> usize {
        self.reader_pool.live_count()
    }

    /// Pack 6.G G.1 — return the `sqlite3_db_config(LOOKASIDE)` rc
    /// captured for each reader worker at open time, in worker index
    /// order. SQLITE_OK (= 0) means the lookaside was configured
    /// before any allocation happened on the connection.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn reader_lookaside_config_rcs_for_test(&self) -> Vec<i32> {
        self.reader_lookaside_rcs.clone()
    }

    /// Pack 6.G G.1 — query each reader worker's
    /// `SQLITE_DBSTATUS_LOOKASIDE_USED` counter. A value > 0 means at
    /// least one allocation was satisfied from the per-connection
    /// lookaside arena (proof the configuration was honored before the
    /// first prepare).
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn reader_lookaside_used_per_worker_for_test(&self) -> Vec<i32> {
        self.reader_pool.lookaside_used_per_worker()
    }

    /// Pack 6.G G.3.5 — broadcast a debug-only `CacheStatus` request to
    /// every reader worker and collect per-worker
    /// `SQLITE_DBSTATUS_CACHE_HIT` / `_CACHE_MISS` / `_CACHE_USED`
    /// values. Counters are monotonic (reset flag = 0); callers compute
    /// pre/post deltas explicitly.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn cache_status_per_worker_for_test(&self, label: &str) -> Vec<CacheStatusReply> {
        self.reader_pool.cache_status_per_worker(label)
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

    #[doc(hidden)]
    pub fn run_one_thread_poison_for_test(&self) -> Result<(), EngineError> {
        self.ensure_open()?;
        let context = lifecycle::StressFailureContext {
            thread_group_id: 1,
            op_kind: "search".to_string(),
            last_error_chain: vec![EngineError::Closing.to_string()],
            projection_state: "UpToDate".to_string(),
        };
        self.subscribers.dispatch_stress_failure(&context);
        Ok(())
    }

    #[doc(hidden)]
    pub fn set_projection_scheduler_frozen_for_test(&self, frozen: bool) {
        self.projection_runtime.set_frozen(frozen);
    }

    #[doc(hidden)]
    pub fn set_projection_retry_delays_for_test(&self, delays_ms: &[u64]) {
        self.projection_runtime.set_retry_delays_for_test(delays_ms);
    }

    #[doc(hidden)]
    pub fn projection_status_for_test(
        &self,
        kind: &str,
    ) -> Result<lifecycle::ProjectionStatus, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        projection_status(connection, kind)
    }

    #[doc(hidden)]
    pub fn has_vector_for_cursor_for_test(&self, cursor: u64) -> Result<bool, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        terminal_state_for_cursor(connection, cursor)
            .map(|state| matches!(state.as_deref(), Some("up_to_date")))
            .map_err(|_| EngineError::Storage)
    }

    #[doc(hidden)]
    pub fn projection_failure_count_for_test(&self, cursor: u64) -> Result<u64, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        connection
            .query_row(
                "SELECT COUNT(*) FROM operational_mutations
                 WHERE collection_name = 'projection_failures'
                   AND record_key = ?1",
                [cursor.to_string()],
                |row| row.get::<_, u64>(0),
            )
            .map_err(|_| EngineError::Storage)
    }

    #[doc(hidden)]
    pub fn set_provenance_row_cap_for_test(&self, cap: Option<u64>) {
        self.provenance_row_cap.store(cap.unwrap_or(0), Ordering::Relaxed);
    }

    #[doc(hidden)]
    pub fn provenance_row_count_for_test(&self) -> Result<u64, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        connection
            .query_row("SELECT COUNT(*) FROM operational_mutations", [], |row| row.get::<_, u64>(0))
            .map_err(|_| EngineError::Storage)
    }

    #[doc(hidden)]
    pub fn oldest_provenance_record_key_for_test(
        &self,
        collection: &str,
    ) -> Result<Option<String>, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        connection
            .query_row(
                "SELECT record_key FROM operational_mutations
                 WHERE collection_name = ?1
                 ORDER BY id
                 LIMIT 1",
                [collection],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|err| match err {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                _ => Err(EngineError::Storage),
            })
    }

    #[doc(hidden)]
    pub fn configure_vector_kind_for_test(&self, kind: &str) -> Result<(), EngineError> {
        self.ensure_open()?;
        let mut connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_mut().ok_or(EngineError::Closing)?;
        connection
            .execute(
                "INSERT OR REPLACE INTO _fathomdb_vector_kinds(kind, profile, created_at)
                 VALUES(?1, ?2, 0)",
                params![kind, DEFAULT_VECTOR_PROFILE],
            )
            .map_err(|_| EngineError::Storage)?;
        Ok(())
    }

    #[doc(hidden)]
    pub fn write_vector_for_test(
        &self,
        kind: &str,
        text: &str,
    ) -> Result<WriteReceipt, EngineError> {
        self.ensure_open()?;
        let embedder =
            self.runtime_embedder.as_ref().cloned().ok_or(EngineError::EmbedderNotConfigured)?;

        let mut connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_mut().ok_or(EngineError::Closing)?;
        if !kind_is_vector_indexed(connection, kind)? {
            return Err(EngineError::KindNotVectorIndexed);
        }

        let expected = default_profile_dimension(connection)?;
        ensure_vector_partition(connection, expected).map_err(|_| EngineError::Storage)?;
        let vector = embedder.embed(text).map_err(map_runtime_embedder_error)?;
        let actual = u32::try_from(vector.len()).unwrap_or(u32::MAX);
        if actual != expected {
            return Err(EngineError::EmbedderDimensionMismatch { expected, actual });
        }

        let cursor = self.next_cursor.load(Ordering::SeqCst).saturating_add(1);
        let blob = encode_vector_blob(&vector);
        let tx = connection.transaction().map_err(|_| EngineError::Storage)?;
        tx.execute(
            "INSERT INTO _fathomdb_vector_rows(rowid, kind, write_cursor) VALUES(?1, ?2, ?3)",
            params![cursor, kind, cursor],
        )
        .map_err(|_| EngineError::Storage)?;
        tx.execute(
            "INSERT INTO vector_default(rowid, embedding) VALUES(?1, ?2)",
            params![cursor, blob],
        )
        .map_err(|_| EngineError::Storage)?;
        tx.commit().map_err(|_| EngineError::Storage)?;
        self.next_cursor.store(cursor, Ordering::SeqCst);
        Ok(WriteReceipt { cursor })
    }

    #[doc(hidden)]
    pub fn vector_row_count_for_test(&self) -> Result<u64, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        connection
            .query_row("SELECT COUNT(*) FROM vector_default", [], |row| row.get::<_, u64>(0))
            .map_err(|_| EngineError::Storage)
    }

    #[doc(hidden)]
    pub fn read_vector_blob_for_test(&self, rowid: i64) -> Result<Vec<u8>, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        connection
            .query_row("SELECT embedding FROM vector_default WHERE rowid = ?1", [rowid], |row| {
                row.get::<_, Vec<u8>>(0)
            })
            .map_err(|_| EngineError::Storage)
    }

    #[doc(hidden)]
    pub fn default_embedder_profile_for_test(&self) -> Result<EmbedderIdentity, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        load_default_profile(connection).map_err(|_| EngineError::Storage)
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

/// Read projection cursor and matching body rows inside one read tx.
fn read_search_in_tx(
    reader: &mut Connection,
    compiled: &fathomdb_query::CompiledQuery,
    query_vector: Option<&str>,
) -> rusqlite::Result<(u64, Option<SoftFallback>, Vec<String>)> {
    let tx = reader.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;
    let cursor = load_projection_cursor(&tx)?;
    let vector_results = if let Some(query_vector) = query_vector {
        let mut rowids = Vec::new();
        {
            let mut statement = tx.prepare(
                "SELECT rowid
                 FROM vector_default
                 WHERE embedding MATCH vec_f32(?1)
                 ORDER BY distance
                 LIMIT 10",
            )?;
            let rows = statement.query_map([query_vector], |row| row.get::<_, i64>(0))?;
            for row in rows.flatten() {
                rowids.push(row);
            }
        }
        let mut results = Vec::new();
        let mut statement =
            tx.prepare("SELECT body FROM canonical_nodes WHERE write_cursor = ?1 LIMIT 1")?;
        for rowid in rowids {
            if let Ok(body) = statement.query_row([rowid], |row| row.get::<_, String>(0)) {
                results.push(body);
            }
        }
        results
    } else {
        Vec::new()
    };
    let vector_rows_visible = !vector_results.is_empty();
    let soft_fallback = if query_vector.is_some() && !vector_rows_visible {
        tx.query_row(
            "SELECT 1
             FROM search_index
             JOIN _fathomdb_vector_kinds ON _fathomdb_vector_kinds.kind = search_index.kind
             LEFT JOIN _fathomdb_projection_terminal
               ON _fathomdb_projection_terminal.write_cursor = search_index.write_cursor
             WHERE search_index MATCH ?1
              AND _fathomdb_projection_terminal.write_cursor IS NULL
             LIMIT 1",
            [compiled.match_expression.as_str()],
            |_row| Ok(SoftFallback { branch: SoftFallbackBranch::Vector }),
        )
        .ok()
    } else {
        None
    };
    let mut seen = BTreeSet::new();
    let mut results = Vec::new();
    for row in vector_results {
        if seen.insert(row.clone()) {
            results.push(row);
        }
    }
    {
        let mut statement = tx.prepare(
            "SELECT body FROM search_index WHERE search_index MATCH ?1 ORDER BY write_cursor",
        )?;
        let rows = statement
            .query_map([compiled.match_expression.as_str()], |row| row.get::<_, String>(0))?;
        for row in rows.flatten() {
            if seen.insert(row.clone()) {
                results.push(row);
            }
        }
    }
    tx.commit()?;
    Ok((cursor, soft_fallback, results))
}

fn projection_dispatcher_loop(shared: Arc<ProjectionRuntimeShared>) {
    let connection = match open_runtime_connection(&shared.path) {
        Ok(connection) => connection,
        Err(_) => return,
    };
    loop {
        let in_flight = {
            let mut state = match shared.state.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            while !state.stopping
                && (!state.pending_scan
                    || state.frozen
                    || state.active_jobs + state.queued_jobs >= PROJECTION_INFLIGHT_LIMIT)
            {
                state = match shared.state_cvar.wait(state) {
                    Ok(state) => state,
                    Err(_) => return,
                };
            }
            if state.stopping {
                return;
            }
            state.pending_scan = false;
            state.in_flight.clone()
        };

        match next_pending_projection_job(&connection, &in_flight) {
            Ok(Some(job)) => {
                if let Ok(mut state) = shared.state.lock() {
                    state.queued_jobs = state.queued_jobs.saturating_add(1);
                    state.in_flight.insert(job.cursor);
                    state.pending_scan = true;
                    shared.state_cvar.notify_all();
                }
                if let Ok(mut queue) = shared.queue.lock() {
                    queue.push_back(job);
                    shared.queue_cvar.notify_one();
                }
            }
            Ok(None) => {}
            Err(_) => {
                if let Ok(mut state) = shared.state.lock() {
                    state.pending_scan = false;
                    shared.state_cvar.notify_all();
                }
            }
        }
    }
}

fn projection_worker_loop(shared: Arc<ProjectionRuntimeShared>) {
    let mut connection = match open_runtime_connection(&shared.path) {
        Ok(connection) => connection,
        Err(_) => return,
    };
    if ensure_vector_partition(&connection, shared.embedder_identity.dimension).is_err() {
        return;
    }
    loop {
        let jobs = {
            let mut queue = match shared.queue.lock() {
                Ok(queue) => queue,
                Err(_) => return,
            };
            loop {
                let stopping = shared.state.lock().map(|state| state.stopping).unwrap_or(true);
                if stopping && queue.is_empty() {
                    return;
                }
                if let Some(job) = queue.pop_front() {
                    let mut jobs = vec![job];
                    while jobs.len() < PROJECTION_COMMIT_BATCH {
                        let Some(job) = queue.pop_front() else {
                            break;
                        };
                        jobs.push(job);
                    }
                    if let Ok(mut state) = shared.state.lock() {
                        state.queued_jobs = state.queued_jobs.saturating_sub(jobs.len());
                        state.active_jobs = state.active_jobs.saturating_add(jobs.len());
                        shared.state_cvar.notify_all();
                    }
                    break jobs;
                }
                queue = match shared.queue_cvar.wait(queue) {
                    Ok(queue) => queue,
                    Err(_) => return,
                };
            }
        };

        run_projection_jobs(&shared, &mut connection, &jobs);

        if let Ok(mut state) = shared.state.lock() {
            state.active_jobs = state.active_jobs.saturating_sub(jobs.len());
            for job in &jobs {
                state.in_flight.remove(&job.cursor);
            }
            if !state.stopping {
                state.pending_scan = true;
            }
            shared.state_cvar.notify_all();
        }
    }
}

enum ProjectionOutcome {
    Success { cursor: u64, kind: String, blob: Vec<u8> },
    Failure { cursor: u64, failure_code: &'static str },
}

fn run_projection_jobs(
    shared: &ProjectionRuntimeShared,
    connection: &mut Connection,
    jobs: &[ProjectionJob],
) {
    let mut outcomes = Vec::with_capacity(jobs.len());
    for job in jobs {
        outcomes.push(run_projection_job(shared, job));
    }
    let _ = commit_projection_outcomes(connection, &outcomes);
}

fn run_projection_job(shared: &ProjectionRuntimeShared, job: &ProjectionJob) -> ProjectionOutcome {
    let delays = shared.retry_delays_ms.lock().map(|delays| delays.clone()).unwrap_or_default();
    let mut last_code = "EmbedderError";
    for (attempt, delay_ms) in std::iter::once(0_u64).chain(delays.iter().copied()).enumerate() {
        if attempt > 0 {
            if shared.state.lock().map(|state| state.stopping).unwrap_or(true) {
                return ProjectionOutcome::Failure { cursor: job.cursor, failure_code: last_code };
            }
            thread::sleep(Duration::from_millis(delay_ms));
        }
        let vector = match shared.embedder.as_ref() {
            Some(embedder) => match embedder.embed(&job.body) {
                Ok(vector) => vector,
                Err(RuntimeEmbedderError::Timeout) => {
                    last_code = "EmbedderError";
                    continue;
                }
                Err(RuntimeEmbedderError::Failed { .. }) => {
                    last_code = "EmbedderError";
                    continue;
                }
            },
            None => {
                last_code = "EmbedderNotConfiguredError";
                continue;
            }
        };

        if u32::try_from(vector.len()).unwrap_or(u32::MAX) != shared.embedder_identity.dimension {
            last_code = "EmbedderDimensionMismatchError";
            continue;
        }

        let blob = encode_vector_blob(&vector);
        return ProjectionOutcome::Success { cursor: job.cursor, kind: job.kind.clone(), blob };
    }

    ProjectionOutcome::Failure { cursor: job.cursor, failure_code: last_code }
}

fn next_pending_projection_job(
    connection: &Connection,
    in_flight: &BTreeSet<u64>,
) -> rusqlite::Result<Option<ProjectionJob>> {
    let cursor = load_projection_cursor(connection)?;
    let mut statement = connection.prepare_cached(
        "SELECT canonical_nodes.write_cursor, canonical_nodes.kind, canonical_nodes.body
         FROM canonical_nodes
         JOIN _fathomdb_vector_kinds ON _fathomdb_vector_kinds.kind = canonical_nodes.kind
         LEFT JOIN _fathomdb_projection_terminal
           ON _fathomdb_projection_terminal.write_cursor = canonical_nodes.write_cursor
         WHERE canonical_nodes.write_cursor > ?1
           AND _fathomdb_projection_terminal.write_cursor IS NULL
         ORDER BY canonical_nodes.write_cursor
         LIMIT 32",
    )?;
    let rows = statement.query_map([cursor], |row| {
        Ok(ProjectionJob { cursor: row.get(0)?, kind: row.get(1)?, body: row.get(2)? })
    })?;
    for row in rows {
        let job = row?;
        if !in_flight.contains(&job.cursor) {
            return Ok(Some(job));
        }
    }
    Ok(None)
}

fn database_has_pending_projection_work(path: &Path) -> rusqlite::Result<bool> {
    let connection = open_runtime_connection(path)?;
    let cursor = load_projection_cursor(&connection)?;
    connection
        .query_row(
            "SELECT 1
             FROM canonical_nodes
             JOIN _fathomdb_vector_kinds ON _fathomdb_vector_kinds.kind = canonical_nodes.kind
             LEFT JOIN _fathomdb_projection_terminal
               ON _fathomdb_projection_terminal.write_cursor = canonical_nodes.write_cursor
             WHERE canonical_nodes.write_cursor > ?1
               AND _fathomdb_projection_terminal.write_cursor IS NULL
             LIMIT 1",
            [cursor],
            |_row| Ok(true),
        )
        .or_else(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => Ok(false),
            _ => Err(err),
        })
}

fn open_runtime_connection(path: &Path) -> rusqlite::Result<Connection> {
    let connection = Connection::open(path)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    Ok(connection)
}

fn load_projection_cursor(connection: &Connection) -> rusqlite::Result<u64> {
    connection
        .query_row(
            "SELECT value FROM _fathomdb_open_state WHERE key = ?1",
            [PROJECTION_CURSOR_KEY],
            |row| row.get::<_, String>(0),
        )
        .map(|value| value.parse::<u64>().unwrap_or(0))
        .or_else(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => Ok(0),
            _ => Err(err),
        })
}

fn store_projection_cursor(connection: &Connection, cursor: u64) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO _fathomdb_open_state(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![PROJECTION_CURSOR_KEY, cursor.to_string()],
    )?;
    Ok(())
}

fn record_projection_terminal(
    connection: &Connection,
    cursor: u64,
    state: &str,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT OR IGNORE INTO _fathomdb_projection_terminal(write_cursor, state) VALUES(?1, ?2)",
        params![cursor, state],
    )?;
    Ok(())
}

fn terminal_state_for_cursor(
    connection: &Connection,
    cursor: u64,
) -> rusqlite::Result<Option<String>> {
    connection
        .query_row(
            "SELECT state FROM _fathomdb_projection_terminal WHERE write_cursor = ?1",
            [cursor],
            |row| row.get::<_, String>(0),
        )
        .map(Some)
        .or_else(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            _ => Err(err),
        })
}

fn advance_projection_cursor(connection: &Connection) -> rusqlite::Result<u64> {
    let mut cursor = load_projection_cursor(connection)?;
    loop {
        let next = cursor.saturating_add(1);
        if terminal_state_for_cursor(connection, next)?.is_some() {
            cursor = next;
        } else {
            break;
        }
    }
    store_projection_cursor(connection, cursor)?;
    Ok(cursor)
}

fn commit_projection_outcomes(
    connection: &mut Connection,
    outcomes: &[ProjectionOutcome],
) -> rusqlite::Result<()> {
    let tx = connection.transaction()?;
    for outcome in outcomes {
        match outcome {
            ProjectionOutcome::Success { cursor, kind, blob } => {
                if terminal_state_for_cursor(&tx, *cursor)?.is_some() {
                    continue;
                }
                tx.execute(
                    "INSERT OR IGNORE INTO _fathomdb_vector_rows(rowid, kind, write_cursor) VALUES(?1, ?2, ?3)",
                    params![cursor, kind, cursor],
                )?;
                tx.execute(
                    "INSERT OR IGNORE INTO vector_default(rowid, embedding) VALUES(?1, ?2)",
                    params![cursor, blob],
                )?;
                record_projection_terminal(&tx, *cursor, "up_to_date")?;
            }
            ProjectionOutcome::Failure { cursor, failure_code } => {
                if terminal_state_for_cursor(&tx, *cursor)?.is_some() {
                    continue;
                }
                let existing: u64 = tx.query_row(
                    "SELECT COUNT(*) FROM operational_mutations
                     WHERE collection_name = 'projection_failures'
                       AND json_extract(payload_json, '$.write_cursor') = ?1",
                    [cursor],
                    |row| row.get(0),
                )?;
                if existing == 0 {
                    let payload = format!(
                        r#"{{"write_cursor":{cursor},"failure_code":"{failure_code}","recorded_at":0}}"#
                    );
                    tx.execute(
                        "INSERT INTO operational_mutations(
                            collection_name, record_key, op_kind, payload_json, schema_id, write_cursor
                         ) VALUES('projection_failures', ?1, 'append', ?2, NULL, ?3)",
                        params![cursor.to_string(), payload, cursor],
                    )?;
                }
                record_projection_terminal(&tx, *cursor, "failed")?;
            }
        }
    }
    advance_projection_cursor(&tx)?;
    tx.commit()
}

fn enforce_provenance_retention(connection: &Connection, cap: u64) -> rusqlite::Result<()> {
    if cap == 0 {
        return Ok(());
    }
    let slack = cap.max(20) / 20;
    let upper = cap.saturating_add(slack.max(1));
    let count: u64 =
        connection.query_row("SELECT COUNT(*) FROM operational_mutations", [], |row| row.get(0))?;
    if count <= upper {
        return Ok(());
    }
    let to_delete = count.saturating_sub(cap);
    connection.execute(
        "DELETE FROM operational_mutations
         WHERE id IN (
             SELECT id FROM operational_mutations
             ORDER BY id
             LIMIT ?1
         )",
        [to_delete],
    )?;
    Ok(())
}

fn projection_status(
    connection: &Connection,
    kind: &str,
) -> Result<lifecycle::ProjectionStatus, EngineError> {
    let latest = connection
        .query_row(
            "SELECT COALESCE(MAX(write_cursor), 0) FROM canonical_nodes WHERE kind = ?1",
            [kind],
            |row| row.get::<_, u64>(0),
        )
        .map_err(|_| EngineError::Storage)?;
    if latest == 0 {
        return Ok(lifecycle::ProjectionStatus::UpToDate);
    }
    let pending: u64 = connection
        .query_row(
            "SELECT COUNT(*)
             FROM canonical_nodes
             LEFT JOIN _fathomdb_projection_terminal
               ON _fathomdb_projection_terminal.write_cursor = canonical_nodes.write_cursor
             WHERE canonical_nodes.kind = ?1
               AND _fathomdb_projection_terminal.write_cursor IS NULL",
            [kind],
            |row| row.get(0),
        )
        .map_err(|_| EngineError::Storage)?;
    if pending > 0 {
        return Ok(lifecycle::ProjectionStatus::Pending);
    }
    match terminal_state_for_cursor(connection, latest).map_err(|_| EngineError::Storage)? {
        Some(state) if state == "failed" => Ok(lifecycle::ProjectionStatus::Failed),
        _ => Ok(lifecycle::ProjectionStatus::UpToDate),
    }
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

fn register_sqlite_vec_extension() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| unsafe {
        let entrypoint: unsafe extern "C" fn(
            *mut rusqlite::ffi::sqlite3,
            *mut *const std::os::raw::c_char,
            *const rusqlite::ffi::sqlite3_api_routines,
        ) -> std::os::raw::c_int = std::mem::transmute(sqlite3_vec_init as *const ());
        rusqlite::ffi::sqlite3_auto_extension(Some(entrypoint));
    });
}

fn probe_open_integrity(connection: &Connection) -> Result<(), EngineOpenError> {
    connection
        .query_row("PRAGMA schema_version", [], |row| row.get::<_, u32>(0))
        .map(|_| ())
        .map_err(|err| map_open_sqlite_error(err, OpenStage::SchemaProbe))
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

fn load_default_profile(connection: &Connection) -> rusqlite::Result<EmbedderIdentity> {
    connection.query_row(
        "SELECT name, revision, dimension FROM _fathomdb_embedder_profiles WHERE profile = ?1",
        [DEFAULT_VECTOR_PROFILE],
        |row| {
            Ok(EmbedderIdentity::new(
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
            ))
        },
    )
}

fn default_profile_dimension(connection: &Connection) -> Result<u32, EngineError> {
    load_default_profile(connection)
        .map(|identity| identity.dimension)
        .map_err(|_| EngineError::Storage)
}

fn kind_is_vector_indexed(connection: &Connection, kind: &str) -> Result<bool, EngineError> {
    connection
        .query_row("SELECT 1 FROM _fathomdb_vector_kinds WHERE kind = ?1", [kind], |_row| Ok(()))
        .map(|_| true)
        .or_else(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => Ok(false),
            _ => Err(EngineError::Storage),
        })
}

fn ensure_vector_partition(connection: &Connection, dimension: u32) -> rusqlite::Result<()> {
    let sql = format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS {DEFAULT_VECTOR_PARTITION} USING vec0(embedding float[{dimension}])"
    );
    connection.execute_batch(&sql)
}

fn encode_vector_blob(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|value| value.to_le_bytes()).collect()
}

fn map_runtime_embedder_error(err: RuntimeEmbedderError) -> EngineError {
    match err {
        RuntimeEmbedderError::Failed { .. } | RuntimeEmbedderError::Timeout => {
            EngineError::Embedder
        }
    }
}

fn default_embedder_identity() -> EmbedderIdentity {
    EmbedderIdentity::new(
        DEFAULT_EMBEDDER_NAME,
        DEFAULT_EMBEDDER_REVISION,
        DEFAULT_EMBEDDER_DIMENSION,
    )
}

fn check_embedder_profile(
    connection: &Connection,
    supplied: &EmbedderIdentity,
) -> Result<(), EngineOpenError> {
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
        connection
            .execute(
                "INSERT INTO _fathomdb_embedder_profiles(profile, name, revision, dimension)
                 VALUES(?1, ?2, ?3, ?4)",
                params![
                    DEFAULT_VECTOR_PROFILE,
                    supplied.name,
                    supplied.revision,
                    supplied.dimension
                ],
            )
            .map_err(|_| EngineOpenError::Io {
                message: "could not persist embedder profile".to_string(),
            })?;
        return Ok(());
    };

    let stored_name = row.get::<_, String>(0).map_err(|_| {
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
    let stored_revision = row.get::<_, String>(1).map_err(|_| {
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

    let stored = EmbedderIdentity::new(stored_name, stored_revision, dimension);

    if stored.name != supplied.name || stored.revision != supplied.revision {
        return Err(EngineOpenError::EmbedderIdentityMismatch {
            stored,
            supplied: supplied.clone(),
        });
    }
    if dimension != supplied.dimension {
        return Err(EngineOpenError::EmbedderDimensionMismatch {
            stored: dimension,
            supplied: supplied.dimension,
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

fn collect_projection_jobs(
    connection: &Connection,
    batch: &[PreparedWrite],
) -> Result<Vec<ProjectionJob>, EngineError> {
    let mut jobs = Vec::new();
    for write in batch {
        if let PreparedWrite::Node { kind, body } = write {
            if kind_is_vector_indexed(connection, kind)? {
                jobs.push(ProjectionJob { cursor: 0, kind: kind.clone(), body: body.clone() });
            }
        }
    }
    Ok(jobs)
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
    pending_projection: bool,
    provenance_row_cap: u64,
) -> rusqlite::Result<()> {
    let tx = connection.transaction()?;

    for (write, plan) in batch.iter().zip(plans) {
        match (write, plan) {
            (PreparedWrite::Node { kind, body }, WritePlan::Node) => {
                tx.execute(
                    "INSERT INTO canonical_nodes(write_cursor, kind, body) VALUES(?1, ?2, ?3)",
                    params![cursor, kind, body],
                )?;
                tx.execute(
                    "INSERT INTO search_index(body, kind, write_cursor) VALUES(?1, ?2, ?3)",
                    params![body, kind, cursor],
                )?;
                if kind_is_vector_indexed(&tx, kind).unwrap_or(false) {
                    tx.execute(
                        "INSERT INTO _fathomdb_projection_state(kind, last_enqueued_cursor, updated_at)
                         VALUES(?1, ?2, 0)
                         ON CONFLICT(kind) DO UPDATE SET last_enqueued_cursor = excluded.last_enqueued_cursor",
                        params![kind, cursor],
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

    if !pending_projection {
        record_projection_terminal(&tx, cursor, "up_to_date")?;
    }
    enforce_provenance_retention(&tx, provenance_row_cap)?;
    advance_projection_cursor(&tx)?;

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
///
/// Diagnostic completeness for unmapped codes: when this helper returns
/// `"SQLITE_UNKNOWN"`, the numeric extended code is not lost — it
/// remains on the underlying `rusqlite::Error::SqliteFailure` carried
/// in the engine error chain that subscribers can inspect via
/// `EngineError`'s `source()`. Expanding the enumerated subset (or
/// surfacing the numeric code as a typed payload field) is a 0.7+
/// improvement.
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

fn sqlite_extended_code_name_from_int(extended: i32) -> &'static str {
    match extended {
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
    }
}

fn map_open_sqlite_error(err: rusqlite::Error, stage: OpenStage) -> EngineOpenError {
    let Some(sqlite_error) = err.sqlite_error() else {
        return EngineOpenError::Io { message: "could not open database".to_string() };
    };
    match sqlite_error.extended_code {
        rusqlite::ffi::SQLITE_CORRUPT | rusqlite::ffi::SQLITE_NOTADB => {
            EngineOpenError::Corruption(CorruptionDetail {
                kind: match stage {
                    OpenStage::WalReplay => CorruptionKind::WalReplayFailure,
                    OpenStage::HeaderProbe => CorruptionKind::HeaderMalformed,
                    OpenStage::SchemaProbe => CorruptionKind::SchemaInconsistent,
                    OpenStage::EmbedderIdentity => CorruptionKind::EmbedderIdentityDrift,
                },
                stage,
                locator: CorruptionLocator::OpaqueSqliteError {
                    sqlite_extended_code: sqlite_error.extended_code,
                },
                recovery_hint: RecoveryHint {
                    code: match stage {
                        OpenStage::WalReplay => "E_CORRUPT_WAL_REPLAY",
                        OpenStage::HeaderProbe => "E_CORRUPT_HEADER",
                        OpenStage::SchemaProbe => "E_CORRUPT_SCHEMA",
                        OpenStage::EmbedderIdentity => "E_CORRUPT_EMBEDDER_IDENTITY",
                    },
                    doc_anchor: match stage {
                        OpenStage::WalReplay => "design/recovery.md#wal-replay",
                        OpenStage::HeaderProbe => "design/recovery.md#header-format",
                        OpenStage::SchemaProbe => "design/recovery.md#schema-inconsistency",
                        OpenStage::EmbedderIdentity => "design/recovery.md#embedder-identity",
                    },
                },
            })
        }
        _ => EngineOpenError::Io { message: "could not open database".to_string() },
    }
}

fn emit_open_error_event(subscriber: &Arc<dyn lifecycle::Subscriber>, err: &EngineOpenError) {
    if let EngineOpenError::Corruption(detail) = err {
        let code = match detail.locator {
            CorruptionLocator::OpaqueSqliteError { sqlite_extended_code } => {
                Some(sqlite_extended_code_name_from_int(sqlite_extended_code))
            }
            _ => None,
        };
        let event = lifecycle::Event {
            phase: lifecycle::Phase::Failed,
            source: lifecycle::EventSource::SqliteInternal,
            category: lifecycle::EventCategory::Corruption,
            code,
        };
        subscriber.on_event(&event);
    }
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
    // before `profile_contexts`. `ReaderWorkerPool::Drop` joins every
    // reader worker, and each worker uninstalls and drops its owned
    // connection inside `reader_worker_loop` before the worker thread
    // returns. Therefore all connections — and SQLite's internal
    // profile-callback state with them — are torn down before the
    // `Box<ProfileContext>` allocations are freed. `Engine::close`
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

/// Pack 6.G G.1 — configure SQLite per-connection lookaside on a reader
/// worker connection. Must be called BEFORE any statement is prepared
/// or any PRAGMA is run on `connection`; per the SQLite docs
/// (https://www.sqlite.org/malloc.html §3) lookaside is silently
/// ignored if reconfigured after the first allocation on the
/// connection. Passing `NULL` for the buffer pointer lets SQLite
/// allocate the lookaside backing memory itself.
///
/// rusqlite 0.31's `set_db_config` only handles the boolean
/// `DbConfig::*` variants; `SQLITE_DBCONFIG_LOOKASIDE` is not surfaced
/// (it is commented out in `rusqlite/src/config.rs`), so we call the
/// raw FFI directly.
///
/// Returns the rc of `sqlite3_db_config` so callers can debug-assert
/// `SQLITE_OK` and surface configuration failure under
/// `debug_assertions` test builds without expanding the public surface.
fn configure_reader_lookaside(connection: &Connection) -> std::os::raw::c_int {
    // SAFETY: `connection.handle()` returns a valid `*mut sqlite3` for
    // the lifetime of `connection`. The variadic
    // `sqlite3_db_config(LOOKASIDE)` call expects three trailing
    // arguments of types `void*`, `int`, `int` — the prototype shape
    // documented in `sqlite3.h`. We pass a null buffer so SQLite owns
    // the lookaside backing allocation, and the slot size / count from
    // the G.1 constants. No allocations happen on the connection
    // before this call (reader open path is `Connection::open` ->
    // `configure_reader_lookaside` -> first PRAGMA).
    unsafe {
        rusqlite::ffi::sqlite3_db_config(
            connection.handle(),
            rusqlite::ffi::SQLITE_DBCONFIG_LOOKASIDE,
            std::ptr::null_mut::<std::ffi::c_void>(),
            READER_LOOKASIDE_SLOT_SIZE,
            READER_LOOKASIDE_SLOT_COUNT,
        )
    }
}

/// Read the high-water-mark for `SQLITE_DBSTATUS_LOOKASIDE_USED` on
/// `connection`. The `current` out-param is the live checked-out slot
/// count and decays as transactions finalize, so it is unreliable as
/// post-warmup evidence. The `hiwtr` out-param latches the largest
/// observed `current` value since the last reset and is the right
/// signal that lookaside was honored at any point on this connection.
/// Reset flag is `0` so reading does not clear the high-water mark.
#[cfg(debug_assertions)]
fn read_lookaside_used_hiwtr(connection: &Connection) -> std::os::raw::c_int {
    let mut current: std::os::raw::c_int = 0;
    let mut hiwtr: std::os::raw::c_int = 0;
    // SAFETY: handle is valid; both out pointers are to local stack
    // ints; reset flag 0 is documented as legal.
    unsafe {
        rusqlite::ffi::sqlite3_db_status(
            connection.handle(),
            rusqlite::ffi::SQLITE_DBSTATUS_LOOKASIDE_USED,
            &mut current,
            &mut hiwtr,
            0,
        );
    }
    hiwtr
}

/// Pack 6.G G.3.5 — read the three page-cache pressure counters on
/// `connection`: `SQLITE_DBSTATUS_CACHE_HIT`, `_CACHE_MISS`, and
/// `_CACHE_USED`. Returns `(hit, miss, used_bytes)`. Hit/miss are
/// monotonic counters (reset flag = 0 here); used_bytes is the live
/// page-cache memory footprint at call time. The caller is expected to
/// take pre/post snapshots and do delta arithmetic explicitly.
#[cfg(debug_assertions)]
fn read_cache_status(
    connection: &Connection,
) -> (std::os::raw::c_int, std::os::raw::c_int, std::os::raw::c_int) {
    let mut hit_current: std::os::raw::c_int = 0;
    let mut hit_hiwtr: std::os::raw::c_int = 0;
    let mut miss_current: std::os::raw::c_int = 0;
    let mut miss_hiwtr: std::os::raw::c_int = 0;
    let mut used_current: std::os::raw::c_int = 0;
    let mut used_hiwtr: std::os::raw::c_int = 0;
    // SAFETY: `connection.handle()` returns a valid `*mut sqlite3` for
    // the lifetime of `connection`. All out-pointers are to local stack
    // ints. Reset flag 0 is documented as legal (no counter is reset).
    unsafe {
        rusqlite::ffi::sqlite3_db_status(
            connection.handle(),
            rusqlite::ffi::SQLITE_DBSTATUS_CACHE_HIT,
            &mut hit_current,
            &mut hit_hiwtr,
            0,
        );
        rusqlite::ffi::sqlite3_db_status(
            connection.handle(),
            rusqlite::ffi::SQLITE_DBSTATUS_CACHE_MISS,
            &mut miss_current,
            &mut miss_hiwtr,
            0,
        );
        rusqlite::ffi::sqlite3_db_status(
            connection.handle(),
            rusqlite::ffi::SQLITE_DBSTATUS_CACHE_USED,
            &mut used_current,
            &mut used_hiwtr,
            0,
        );
    }
    // CACHE_HIT / CACHE_MISS are monotonic counters reported in the
    // `current` out-param; CACHE_USED is the live byte count, also in
    // `current`. The hiwtr values are unused for this telemetry.
    (hit_current, miss_current, used_current)
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
