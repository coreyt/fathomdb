pub mod lifecycle;
mod pcache2;

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Once;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use fathomdb_embedder::EmbedderEvent;
// `MeanRecomputeTrigger` is used only by the operator-gated `recompute_mean`.
#[cfg(feature = "operator")]
use fathomdb_embedder::MeanRecomputeTrigger;
use fathomdb_embedder_api::{Embedder, EmbedderError as RuntimeEmbedderError, EmbedderIdentity};
use fathomdb_query::compile_text_query;
use fathomdb_schema::{
    migrate_with_event_sink, MigrationError as SchemaMigrationError, MigrationStepReport,
    LOCK_SUFFIX, MIGRATIONS, SCHEMA_VERSION,
};
// `CANONICAL_TABLES` is used only by the operator-gated `dump_row_counts`.
#[cfg(feature = "operator")]
use fathomdb_schema::CANONICAL_TABLES;
use jsonschema::JSONSchema;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
// `sha2::Digest` + `sha2::Sha256` — used by `safe_export` (operator-gated)
// and unconditionally by `ingest_with_extractor` (G11 logical_id derivation).
#[cfg(feature = "operator")]
use sha2::Digest;
#[cfg(not(feature = "operator"))]
use sha2::Digest as _;
use sha2::Sha256;
use sqlite_vec::sqlite3_vec_init;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

// EU-5b lock-flip: the engine's default embedder identity is now the
// pinned bge-small variant. Pre-existing 0.7.0 workspaces opened with
// `EmbedderChoice::Default` will fail-closed on identity mismatch per
// ADR-0.6.0-vector-identity-embedder-owned; callers can still hold an
// older noop profile by supplying `EmbedderChoice::Caller(NoopEmbedder)`.
const DEFAULT_EMBEDDER_NAME: &str = "fathomdb-bge-small-en-v1.5";
const DEFAULT_EMBEDDER_REVISION: &str = "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a";
const DEFAULT_EMBEDDER_DIMENSION: u32 = 384;

/// Identity name of the bge-small embedder. `OpenReport.embedder_mean_centering_required`
/// is `true` iff the live embedder identity reports this name. NoopEmbedder
/// is `false`. Lifted out as a constant so the EU-5b lock-flip (when the
/// engine's default identity becomes bge-small) is a single-line change.
///
/// TODO(EU-5b): when `DEFAULT_EMBEDDER_NAME` flips to this constant, the
/// Default path will populate `embedder_mean_centering_required = true`
/// without further engine work. Caller-supplied bge-small (rare today)
/// already does the right thing.
const BGE_SMALL_EMBEDDER_NAME: &str = "fathomdb-bge-small-en-v1.5";

/// REQ-006a / AC-007a default slow-statement threshold. Mutated at runtime
/// via [`Engine::set_slow_threshold_ms`].
const DEFAULT_SLOW_THRESHOLD_MS: u64 = 100;
const DEFAULT_VECTOR_PROFILE: &str = "default";
const DEFAULT_VECTOR_PARTITION: &str = "vector_default";
/// Default drain budget for `rebuild_projections` / `rebuild_vec0`. The
/// rebuild path freezes the scheduler before truncating shadow rows, so
/// the only outstanding work is whatever workers were mid-flight when
/// the call landed; 30 s is generous for normal job sizes and bounded
/// for tests.
#[cfg(feature = "operator")]
const REBUILD_DRAIN_TIMEOUT_MS: u64 = 30_000;
/// 0.8.0 Slice 5 (G1) — schema version that introduces the global FTS5
/// tokenizer-default upgrade (`SCHEMA_VERSION` 11, migration step 11). A DB
/// migrated to (or past) this version re-tokenizes `search_index` from
/// canonical source rows on open (the drop+recreate leaves the FTS index
/// empty). Repair is keyed off the completion marker below — NOT off crossing
/// the step boundary — so it is crash-retryable (see
/// `SEARCH_INDEX_TOKENIZER_REPROJECT_MARKER_KEY`).
const SEARCH_INDEX_TOKENIZER_SCHEMA_VERSION: u32 = 11;
/// 0.8.0 Slice 5 (G1) fix-1 — `_fathomdb_open_state` key set, in the SAME
/// transaction as the reproject DELETE+INSERT, once the post-tokenizer-upgrade
/// re-tokenization commits durably. Step 11 commits `user_version = 11` with an
/// EMPTY `search_index` in its own transaction; the reproject runs in a later
/// transaction on open. A crash in that window leaves a durable `user_version =
/// 11` + empty index. Gating repair on a boundary crossing (`before < 11`)
/// would skip it on the next open (it sees `before == 11`), stranding the index
/// empty forever. Gating on this marker's ABSENCE instead makes repair
/// idempotent and crash-retryable: written atomically with the reindex, so a
/// crash before commit leaves no marker and the next open re-runs.
const SEARCH_INDEX_TOKENIZER_REPROJECT_MARKER_KEY: &str =
    "search_index_tokenizer_reproject_complete";
const DEFAULT_PROVENANCE_ROW_CAP: u64 = 1_000_000;
const PROJECTION_CURSOR_KEY: &str = "projection_cursor";
const PROJECTION_WORKERS: usize = 2;
/// PR-9 — ADR-0.6.0-embedder-protocol **Invariant 5** default per-`embed()`
/// watchdog deadline. Every projection-path embed runs under this timeout;
/// a hung embed surfaces `RuntimeEmbedderError::Timeout` (engaging the
/// existing retry/failure path) rather than parking a worker forever. The
/// EU-5f `catch_unwind` only catches *panics*; this catches *hangs*.
const DEFAULT_EMBED_TIMEOUT_MS: u64 = 30_000;
/// PR-9 — embed circuit-breaker threshold: the maximum number of watchdog
/// embed threads allowed alive at once before the breaker latches and
/// projection jobs fail fast (see `embed_circuit_open` / `live_embed_threads`).
/// Healthy serialized operation keeps the live count at 0–1, so reaching this
/// many concurrently-alive embed threads means timed-out embeds are piling up
/// (a hung/wedged embedder); the breaker then caps the abandoned-thread leak
/// at roughly this count.
const DEFAULT_EMBED_CIRCUIT_THRESHOLD: u64 = 8;
const PROJECTION_COMMIT_BATCH: usize = 16;
// Each worker should be able to grab a full commit batch while another
// worker has the same waiting in the queue. Below this, the dispatcher
// throttles below the workers' commit-batch capacity.
const PROJECTION_INFLIGHT_LIMIT: usize = PROJECTION_WORKERS * PROJECTION_COMMIT_BATCH;
// SQL fetch cap inside the dispatcher: enough to fill the in-flight
// budget in a single scan so we don't pay one SQL roundtrip per job.
const PROJECTION_SCAN_FETCH: usize = PROJECTION_INFLIGHT_LIMIT;
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
    /// PR-9 — ADR-0.6.0-embedder-protocol Invariant 5 per-`embed()` watchdog
    /// deadline (ms). Read lock-free on the projection hot path. Default
    /// `DEFAULT_EMBED_TIMEOUT_MS` (30s); the test seam
    /// `set_embed_timeout_ms_for_test` lowers it so the hanging-embedder
    /// test need not wait 30s. A hung embed surfaces
    /// `RuntimeEmbedderError::Timeout`, engaging the existing retry/failure
    /// path in `run_projection_job`.
    embed_timeout_ms: AtomicU64,
    /// PR-9 — engine-side embed serialization guard. The pool runs
    /// `PROJECTION_WORKERS` workers; this guard ensures the shared
    /// `Arc<dyn Embedder>` is invoked by at most one worker at a time.
    ///
    /// Rationale is SAFETY, not throughput. The engine accepts arbitrary
    /// caller-supplied embedders (the pyo3 / napi bridges, per ADR-0.6.0)
    /// whose `embed` is `Sync` only by trait contract; many real impls (a
    /// GIL-bound Python model, a non-reentrant native lib, an internal cache)
    /// are not actually safe under concurrent calls. Serializing engine-side
    /// makes the projection robust to embedders that are not truly
    /// concurrency-safe, without the engine having to trust each impl. The
    /// default `CandleBgeEmbedder` was shown safe under concurrent forwards
    /// in the PR-9 pre-flight, so for it the guard is belt-and-suspenders.
    ///
    /// Throughput is ~neutral: `candle` fans every `BertModel::forward` onto a
    /// single process-wide rayon pool, so two concurrent forwards merely
    /// share that pool (trading per-embed latency, not aggregate work) rather
    /// than getting 2x — serializing avoids some scheduler/cache thrash but is
    /// not a large win. (An earlier "~13x" figure compared a debug-build
    /// unserialized run against a release-build number and was withdrawn; a
    /// PR-9 micro-benchmark put release embeds at ~14 ms short / ~960 ms for a
    /// 512-token doc, watchdog overhead ~0.)
    ///
    /// Commit/IO stays parallel across workers (see `commit_gate`); this guard
    /// wraps only the embed call. It is held by the worker across the watchdog
    /// call and released here, so a timed-out (abandoned) embed frees it and
    /// cannot stall the pool — the guard owns no data, so a panic-resumed
    /// embed that poisons it is recovered via `into_inner`.
    ///
    /// Deliberate trade-off (codex PR-9 CONCERN-1, accepted): on the *timeout*
    /// path the worker drops this guard while the abandoned detached embed
    /// thread is still running lock-free, so serialization is briefly relaxed
    /// until that thread finishes. This is the prescribed choice over holding
    /// the guard inside the embed thread — which would let a genuinely-hung
    /// embed hold it forever and deadlock the whole pool, exactly the wedge
    /// ADR-0.6.0 Invariant 5 and this slice's spec forbid. Timeouts are the
    /// fault path only; the embed circuit breaker (`embed_circuit_open`) caps
    /// how many such abandoned threads can be alive at once. A future slice may
    /// replace this hard serialize with an operator-configurable embed
    /// concurrency limit (ADR-0.6.0 Invariant 4 pool-size override) for I/O-
    /// or GPU-bound embedders; that knob is out of PR-9 scope.
    embed_serialize: Mutex<()>,
    /// PR-9 — embed circuit breaker. `live_embed_threads` counts watchdog embed
    /// threads currently alive (incremented when one is spawned, decremented
    /// when it finishes — see `embed_with_watchdog`). Under healthy serialized
    /// operation this is 0 or 1; it only grows when timed-out embeds are
    /// abandoned and keep running (ADR-0.6.0 Invariant 5 forbids aborting a
    /// running embed). When a new embed would push the live count to
    /// `embed_circuit_threshold`, the breaker latches `embed_circuit_open` and
    /// projection jobs fail fast WITHOUT spawning further embeds — bounding the
    /// abandoned-thread leak to ~threshold REGARDLESS of whether the embedder
    /// hangs on every input or only intermittently (a returning embed
    /// decrements the count rather than resetting a streak, so an
    /// intermittently-hanging embedder still latches as its hung threads pile
    /// up, and a merely-slow-but-returning embedder self-clears and never
    /// false-trips). Latches for the engine session (a reopen resets it); a
    /// half-open/cool-down retry is future work. `threshold == 0` disables it.
    live_embed_threads: Arc<AtomicU64>,
    embed_circuit_open: AtomicBool,
    embed_circuit_threshold: AtomicU64,
    /// EU-5b — streaming mean accumulator for the per-workspace mean
    /// pinning lifecycle (`dev/design/embedder.md` §0.3). `Some(_)` iff
    /// the identity is MC-required AND no mean has been pinned yet on
    /// disk. The accumulator graduates to `None` after the at-pin
    /// commit; subsequent docs feed nothing.
    mean_accumulator: Mutex<Option<MeanAccumulator>>,
    /// EU-5b — `MeanVecPinned` events queued by the projection-commit
    /// transaction for the next test-seam drain. Production callers
    /// consume these via the `OpenReport.embedder_events` channel; the
    /// drain seam is `Engine::drain_mean_centering_events_for_test`.
    pending_events: Mutex<Vec<EmbedderEvent>>,
    /// EU-5f — serializes the body of `commit_projection_outcomes` across
    /// the `PROJECTION_WORKERS` worker connections. Each worker commits on
    /// its own connection; holding this gate for the whole commit makes the
    /// commit transactions totally ordered, which is what makes the at-pin
    /// re-quantize pass provably complete (every row is wholly before or
    /// after the unique pin tx, so none can survive un-centered). Embedding
    /// (`run_projection_job`) runs OUTSIDE the gate and stays parallel.
    commit_gate: Mutex<()>,
    /// 0.7.2 PR-2bc S1 fix-1 — overridable phase-2 rerank `LIMIT` for the
    /// search hot path. Equals `SEARCH_RERANK_LIMIT` (10) in production; a
    /// test seam (`set_search_limit_for_test`) can RAISE it (clamped to >=10,
    /// so it can never shrink below production semantics) so the recall
    /// harness can pull top-(10+slack) and exclude the self-retrieving
    /// query-source doc before truncating to 10. Production reads this atomic
    /// (default 10) — there is NO env var read on the hot path.
    search_limit_override: AtomicUsize,
    /// Slice 10 / G12-recency — dedicated recency-reweight flag, **off by
    /// default** (NOT `fusion_mode`). When set, fused hits are reweighted toward
    /// the more recent `write_cursor` AFTER bit-KNN. Flipped by the
    /// `set_recency_reweight_enabled_for_test` seam; no production toggle yet.
    recency_reweight_enabled: AtomicBool,
    /// GA-2 / Slice-40 (◆ B-1) measurement seam, **off by default**. When set,
    /// `read_search_in_tx` returns the pre-fusion VECTOR-branch ranking
    /// (bit-KNN K=192 + f32 rerank) verbatim — the ANN-quantization fidelity
    /// signal — INSTEAD of the unconditional RRF-fused result. This changes
    /// nothing for any production caller (the flag is never set outside the
    /// `eu7` recall harness via `set_vector_stage_only_for_test`); it does NOT
    /// reintroduce a `fusion_mode` knob (RRF stays unconditional) and does NOT
    /// alter `fuse_rrf` / `rerank_fused` / recency. It only lets the AC-075
    /// recall gate measure ANN+ vector top-10 vs the exact-f32 VECTOR top-10
    /// ground truth in isolation (the quantization-FIDELITY axis the 0.90 floor
    /// is defined to measure), not the hybrid `search()` output.
    vector_stage_only_for_test: AtomicBool,
    /// 0.7.2 PR-2b — debug-only fault injection: when set, `recompute_mean_in_tx`
    /// errors AFTER writing `mean_vec` but BEFORE finishing the re-quantize
    /// pass, so the crash-atomicity test can prove the whole recompute rolls
    /// back (no half-recentered corpus). One-shot (cleared on consume).
    #[cfg(debug_assertions)]
    force_recompute_failure: AtomicBool,
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
        /// Un-centered f32 query vector serialized for `vec_f32`. Phase 2
        /// f32 rerank uses this verbatim.
        query_vector: Option<String>,
        /// EU-5a2 — (possibly centered) f32 query vector for the phase 1
        /// `vec_quantize_binary` sign-quant. Equal to `query_vector` for
        /// non-MC-required identities (the EU-5a2 default).
        query_vector_bin: Option<String>,
        /// 0.7.2 PR-2bc S1 fix-1 — phase-2 rerank `LIMIT`. Read from
        /// `ProjectionRuntimeShared::search_limit_override` (default
        /// `SEARCH_RERANK_LIMIT` = 10, clamped >=10) by `search_inner`
        /// before dispatch, so the worker never reads any env var.
        search_limit: usize,
        /// G10 — optional closed metadata filter (`None` = unfiltered, the
        /// byte-identical-to-0.7.2 path). Applied in the phase-1 candidates
        /// statement (vector branch) and as a Rust post-filter (text branch).
        /// Boxed so the `ReaderRequest::Search` variant stays small (the request
        /// rides a `Result<(), ReaderRequest>` retry channel).
        filter: Option<Box<SearchFilter>>,
        /// G12-recency — whether the dedicated recency reweight is enabled for
        /// this request (read from `recency_reweight_enabled`, off by default).
        recency_enabled: bool,
        /// GA-2 / Slice-40 (◆ B-1) measurement seam — when true the worker
        /// returns the pre-fusion vector-branch ranking instead of the fused
        /// result (read from `vector_stage_only_for_test`, off by default).
        vector_stage_only: bool,
        /// 0.8.1 Slice 10 (R1) — raw query text for the CE reranker. Passed
        /// from `search_inner` to `read_search_in_tx` → `rerank_fused`.
        /// FIX-4: `Box<str>` (16 bytes) instead of `String` (24 bytes) to keep
        /// the Search variant smaller (mirroring the boxed `filter` field).
        raw_query: Box<str>,
        /// 0.8.1 Slice 10 (R1) — per-request rerank depth (snapshot of
        /// `ProjectionRuntimeShared::rerank_depth`). `0` = identity path.
        rerank_depth: usize,
        /// 0.8.1 Slice 30 (R3) — when `true`, run the graph-BFS arm (seeded
        /// from top-10 fused hits, depth ≤ 3, cap 50, temporal filter) and
        /// fuse its candidates into the final ranking via `fuse_three_arms`.
        /// When `false` (the default), the graph arm pool is `vec![]` and
        /// results are byte-identical to the pre-Slice-30 two-arm pipeline.
        use_graph_arm: bool,
        respond: SyncSender<ReaderResponse>,
    },
    /// Slice 30 (G2) — active-only point lookup by `logical_id`. Returns one
    /// slot per requested id, in request order, `None` where no active row
    /// carries that id. Its own typed `respond` channel keeps the `Search`
    /// `ReaderResponse` byte-identical (no Search regression).
    GetById {
        logical_ids: Vec<String>,
        respond: SyncSender<rusqlite::Result<Vec<Option<NodeRecord>>>>,
    },
    /// Slice 30 (G3) — paginated op-store read-back over `operational_mutations`
    /// for a `collection`, `ORDER BY id`, with a MANDATORY (already-clamped)
    /// limit + optional after-id cursor.
    ReadCollection {
        collection: String,
        after_id: Option<i64>,
        limit: usize,
        respond: SyncSender<rusqlite::Result<Vec<OpStoreRow>>>,
    },
    /// Slice 35 (G4) — list active canonical nodes of a `kind`, filtered by
    /// zero or more `Predicate`s (AND-combined), up to `limit` rows.
    /// Path validation already happened at `Predicate` construction time;
    /// the worker only compiles + executes parameterized SQL.
    ReadList {
        kind: String,
        predicates: Vec<Predicate>,
        limit: usize,
        respond: SyncSender<rusqlite::Result<Vec<NodeRecord>>>,
    },
    /// Slice 20 (G5) — bounded BFS from a single root node over
    /// `canonical_edges`. Returns the set of reachable nodes (excluding the
    /// root) within `depth` hops, limited to the hard cap 50.
    GraphNeighbors {
        root_logical_id: String,
        depth: u32,
        direction: TraversalDirection,
        respond: SyncSender<rusqlite::Result<Vec<NodeRecord>>>,
    },
    /// Slice 20 (G6) — compose the previous search result with BFS expansion.
    /// Resolves search hit `write_cursor`s to `logical_id`s, runs G5 traversal
    /// for each root, deduplicates, and returns a `SearchExpandResult`.
    SearchExpand {
        search_hits: Vec<SearchHit>,
        depth: u32,
        respond: SyncSender<rusqlite::Result<SearchExpandResult>>,
    },
    /// Slice 20 test seam — run `EXPLAIN QUERY PLAN` on the BFS CTE SQL for
    /// the given root/depth/direction and return the plan detail lines.
    #[doc(hidden)]
    ExplainGraphNeighbors {
        root_logical_id: String,
        depth: u32,
        direction: TraversalDirection,
        respond: SyncSender<rusqlite::Result<Vec<String>>>,
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

// G0 Phase-2: the Search response carries a 4th element — the graph-arm frontier
// meter (`GraphFrontierStats`). It rides the internal channel but is dropped before
// `SearchResult` is built (kept OFF the governed surface); the
// `_graph_frontier_stats_for_test` seam captures it. Default (all-zero) on non-graph paths.
type ReaderResponse =
    rusqlite::Result<(u64, Option<SoftFallback>, Vec<SearchHit>, GraphFrontierStats)>;

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
    // The `Search` variant contains a SyncSender and boxed fields (filter, raw_query);
    // even after FIX-4 (raw_query: Box<str>), the variant remains large due to the
    // SyncSender channel ownership. The Err return is only ever a no-worker/shutdown
    // signal, never heap-allocated repeatedly, so the allow is justified by the
    // channel ownership model.
    #[allow(clippy::result_large_err)]
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
            ReaderRequest::Search {
                compiled,
                query_vector,
                query_vector_bin,
                search_limit,
                filter,
                recency_enabled,
                vector_stage_only,
                raw_query,
                rerank_depth,
                use_graph_arm,
                respond,
            } => {
                let result = read_search_in_tx(
                    &mut connection,
                    &compiled,
                    query_vector.as_deref(),
                    query_vector_bin.as_deref(),
                    search_limit,
                    filter.as_deref(),
                    recency_enabled,
                    vector_stage_only,
                    &raw_query,
                    rerank_depth,
                    use_graph_arm,
                );
                // Receiver may have been dropped if the caller went
                // away; nothing to do in that case.
                let _ = respond.send(result);
            }
            ReaderRequest::GetById { logical_ids, respond } => {
                let result = read_get_by_id_in_tx(&mut connection, &logical_ids);
                let _ = respond.send(result);
            }
            ReaderRequest::ReadCollection { collection, after_id, limit, respond } => {
                let result = read_collection_in_tx(&mut connection, &collection, after_id, limit);
                let _ = respond.send(result);
            }
            ReaderRequest::ReadList { kind, predicates, limit, respond } => {
                let result = read_list_in_tx(&mut connection, &kind, &predicates, limit);
                let _ = respond.send(result);
            }
            ReaderRequest::GraphNeighbors { root_logical_id, depth, direction, respond } => {
                let result =
                    graph_neighbors_in_tx(&mut connection, &root_logical_id, depth, direction);
                let _ = respond.send(result);
            }
            ReaderRequest::SearchExpand { search_hits, depth, respond } => {
                let result = search_expand_in_tx(&mut connection, &search_hits, depth);
                let _ = respond.send(result);
            }
            ReaderRequest::ExplainGraphNeighbors { root_logical_id, depth, direction, respond } => {
                let result = explain_graph_neighbors_in_tx(
                    &mut connection,
                    &root_logical_id,
                    depth,
                    direction,
                );
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
        mean_already_pinned: bool,
    ) -> Self {
        // EU-5b/EU-5f — only allocate the streaming accumulator when the
        // workspace's identity is MC-required AND no mean has been pinned
        // yet on disk. Allocating it for an already-pinned workspace would
        // let a later 256-doc run RE-pin and overwrite the compute-once
        // mean (violating `dev/design/embedder.md` §0.3). Other identities
        // pay no memory cost (`Option::None`).
        let mc_required = identity_requires_mean_centering(&embedder_identity);
        let mean_accumulator = if mc_required && !mean_already_pinned {
            Some(MeanAccumulator::new(embedder_identity.dimension as usize))
        } else {
            None
        };
        let shared = Arc::new(ProjectionRuntimeShared {
            path,
            embedder,
            embedder_identity,
            state: Mutex::new(ProjectionRuntimeState::default()),
            state_cvar: Condvar::new(),
            queue: Mutex::new(VecDeque::new()),
            queue_cvar: Condvar::new(),
            retry_delays_ms: Mutex::new(DEFAULT_PROJECTION_RETRY_DELAYS_MS.to_vec()),
            embed_timeout_ms: AtomicU64::new(DEFAULT_EMBED_TIMEOUT_MS),
            embed_serialize: Mutex::new(()),
            live_embed_threads: Arc::new(AtomicU64::new(0)),
            embed_circuit_open: AtomicBool::new(false),
            embed_circuit_threshold: AtomicU64::new(DEFAULT_EMBED_CIRCUIT_THRESHOLD),
            mean_accumulator: Mutex::new(mean_accumulator),
            pending_events: Mutex::new(Vec::new()),
            commit_gate: Mutex::new(()),
            search_limit_override: AtomicUsize::new(SEARCH_RERANK_LIMIT),
            recency_reweight_enabled: AtomicBool::new(false),
            vector_stage_only_for_test: AtomicBool::new(false),
            #[cfg(debug_assertions)]
            force_recompute_failure: AtomicBool::new(false),
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

    fn set_embed_timeout_ms_for_test(&self, timeout_ms: u64) {
        self.shared.embed_timeout_ms.store(timeout_ms, Ordering::Relaxed);
    }

    fn set_embed_circuit_threshold_for_test(&self, threshold: u64) {
        self.shared.embed_circuit_threshold.store(threshold, Ordering::Relaxed);
    }

    fn embed_circuit_open_for_test(&self) -> bool {
        self.shared.embed_circuit_open.load(Ordering::Relaxed)
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
    /// Total wall time the loader spent materializing default-embedder
    /// weights — covers HF GETs, sha256 verification, atomic rename,
    /// parent-dir fsync (POSIX), and cache directory writes. This is
    /// the "engine open paid by the embedder" envelope, useful for SLA
    /// budgeting; it is intentionally wider than just the bytes-flowing
    /// time so callers see the full first-use cost.
    ///
    /// `Some(ms)` when network bytes flowed (`bytes_downloaded > 0`);
    /// `None` for caller-supplied embedders (loader bypassed) and on
    /// full cache hits (no bytes flowed). For pure per-file network
    /// analysis, use the `DefaultEmbedderDownload` events on
    /// [`embedder_events`](Self::embedder_events) — each event carries
    /// the file's bytes + sha256 + cache path.
    pub embedder_download_ms: Option<u64>,
    /// Structured loader events (`dev/design/embedder.md` §7). Empty for
    /// caller-supplied embedders; populated from `LoadedWeights.events`
    /// for the Default path.
    pub embedder_events: Vec<EmbedderEvent>,
    /// Static identity capability (`dev/design/embedder.md` §0.6). True
    /// iff the live embedder identity is the bge-small default, which is
    /// the only identity that ships with the EU-5a2 mean-centering apply
    /// paths. `false` for `fathomdb-noop` and for any other
    /// caller-supplied identity. EU-5b's identity flip makes the Default
    /// path return `true` here.
    pub embedder_mean_centering_required: bool,
    /// Dynamic workspace state (`dev/design/embedder.md` §0.6). True iff
    /// `_fathomdb_embedder_profiles.mean_vec IS NOT NULL` for the default
    /// profile. EU-5a2 reads from the schema column added in migration
    /// step 10; the value is dimension-validated (§0.2) at open time
    /// and fails closed via `EmbedderIdentityMismatch` on drift.
    pub embedder_mean_vec_pinned: bool,
}

#[derive(Debug)]
pub struct OpenedEngine {
    pub engine: Engine,
    pub report: OpenReport,
}

/// EU-5b — loader-supplied open-time telemetry threaded into
/// `OpenReport.embedder_download_ms` and `OpenReport.embedder_events`.
#[derive(Clone, Debug)]
struct LoaderInfo {
    download_ms: Option<u64>,
    events: Vec<EmbedderEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteReceipt {
    /// The batch high-water cursor — the `write_cursor` of the last row written
    /// (also the engine's new `next_cursor`). Unchanged from 0.7.x.
    pub cursor: u64,
    /// G0 (Slice 15) — the per-row `write_cursor` of each row in the batch, 1:1
    /// with input order. This is the `write_cursor`-as-row-id identity carrier
    /// (HITL-accepted for 0.8.0; a dedicated `row_id` is deferred). For an
    /// N-row batch this is `[cursor-N+1, …, cursor]`.
    pub row_cursors: Vec<u64>,
    /// G8 (Slice 20 / F10) — count of edge endpoints in this batch that point at
    /// a non-existent **or superseded** canonical node. An endpoint is dangling
    /// when no **active** node (`superseded_at IS NULL`) carries its `logical_id`;
    /// `from_id` and `to_id` are probed independently, so one edge contributes 0,
    /// 1, or 2. This is **informational** (default FLAG-AND-COUNT: the batch
    /// commits regardless) and `0` whenever the batch committed no active edges.
    pub dangling_edge_endpoints: u64,
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

/// Which retrieval branch produced a hit (or could not contribute).
///
/// `Vector` = ANN vector branch (node bodies); `Text` = node-body FTS branch;
/// `TextEdge` = edge-body hit (FTS via `search_index_edges` OR vector-projected
/// edge facts — both produce the same kind="edge_fact" row shape and share the
/// same downstream handling in `search_expand_in_tx`). `Vector`/`Text` also
/// used as soft-fallback signal when the respective branch is empty.
/// `GraphArm` = R3 (Slice 30) BFS-reachable node from the temporal fact-edge
/// graph arm. Owned by `dev/design/retrieval.md`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SoftFallbackBranch {
    Vector,
    Text,
    /// G11 (Slice 15) — edge-body hit from `search_index_edges` FTS or from
    /// `vector_default` edge-fact projection. `kind = "edge_fact"` in both cases.
    TextEdge,
    /// R3 (Slice 30) — BFS-reachable node from the temporal fact-edge graph arm.
    /// Only present when `use_graph_arm = true`. Nodes in the graph arm were NOT
    /// in the initial vector/text fused result (newly-reached nodes only).
    GraphArm,
}

/// A single structured search hit (G1 / AC-057a-clean).
///
/// Both retrieval branches emit this shape. `id` is the canonical row's
/// `write_cursor` — the **interim** identity carrier per
/// `dev/adr/ADR-0.8.0-canonical-identity-substrate.md`; it swaps to
/// `logical_id` at the G0 keystone (Slice 15) with no carrier reshape.
/// `score` is the **G9 RRF-fused** relevance (`Σ 1/(RRF_K + rank)` over the
/// branches that surfaced this body; higher = more relevant), optionally
/// recency-reweighted when the dedicated recency flag is on. Raw `vec_distance_l2`
/// and `bm25()` are fused on **rank**, never compared raw (they are not
/// comparable). `branch` tags which retrieval branch produced the representative
/// hit (vector-first when a body is surfaced by both).
///
/// `source_id` (G0 Phase-2 / BLOCK-2) carries the source-document provenance of
/// a hit. For the GraphArm branch it is the **traversed edge's** `source_id`
/// (the session the fact-edge was extracted from), enabling `doc_id_of` to
/// resolve a graph-reached entity back to a gold session id. For every two-arm
/// (vector/text/edge) hit it is `None` — those resolve via the cursor map. The
/// field is additive and nullable; `use_graph_arm=false` results are byte-stable
/// (every hit `source_id == None`).
///
/// Derives `Clone, Debug, PartialEq` but **not `Eq`** — `score: f64` forbids
/// total equality.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    pub id: u64,
    pub kind: String,
    pub body: String,
    pub score: f64,
    pub branch: SoftFallbackBranch,
    pub source_id: Option<String>,
}

/// G0 Phase-2 (E0a / BLOCK-1) — graph-arm frontier instrumentation. A
/// **side-channel** meter (deliberately NOT a `SearchResult`/`SearchHit` field —
/// byte stability) that proves whether the graph arm seeds a non-empty frontier.
/// Under the current doc-seeded path the frontier is empty (doc nodes carry
/// `logical_id = NULL`), so `seeds_resolved == 0` and `resolved_seed_rate == 0.0`
/// — this meter is the measurement that proves it (and, post-C1, the 0→>0 flip).
///
/// `resolved_seed_rate = seeds_resolved / seeds_considered`, with `0/0 → 0.0`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GraphFrontierStats {
    /// Hits inspected as seed candidates (the `take(SEED_N)` window, skipping TextEdge).
    pub seeds_considered: u32,
    /// Seed candidates that resolved to an active `logical_id` (pushed onto the frontier).
    pub seeds_resolved: u32,
    /// Whether the BFS frontier was non-empty after seeding.
    pub frontier_nonempty: bool,
    /// Number of graph-arm `SearchHit`s emitted (reachable, not already in the two-arm result).
    pub graph_candidates_emitted: u32,
}

impl GraphFrontierStats {
    /// `seeds_resolved / seeds_considered`, defined as `0.0` when nothing was considered.
    pub fn resolved_seed_rate(&self) -> f64 {
        if self.seeds_considered == 0 {
            0.0
        } else {
            f64::from(self.seeds_resolved) / f64::from(self.seeds_considered)
        }
    }
}

/// Slice 30 (G2) — an active canonical node row returned by `read.get` /
/// `read.get_many`.
///
/// `logical_id` is the queried stable identity (echoed). `write_cursor` is the
/// interim id carrier (same column `SearchHit.id` carries). Only ACTIVE rows
/// (`superseded_at IS NULL`) are ever materialised into this shape; a missing or
/// superseded `logical_id` is a normal absence (`None`), never an error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeRecord {
    pub logical_id: String,
    pub kind: String,
    pub body: String,
    pub write_cursor: u64,
}

/// Slice 30 (G3) — one `operational_mutations` row returned by `read.collection`
/// / `read.mutations`. `id` is the autoincrement PK (the after-id cursor key).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpStoreRow {
    pub id: i64,
    pub collection: String,
    pub record_key: String,
    pub op_kind: String,
    pub payload: String,
    pub schema_id: Option<String>,
    pub write_cursor: u64,
}

/// Hybrid `search` result. `results` carries structured [`SearchHit`]s in
/// vector-first, dedup-on-body order. Derives `Clone, Debug, PartialEq` but
/// **not `Eq`** — each hit carries a `score: f64`.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    pub projection_cursor: u64,
    pub soft_fallback: Option<SoftFallback>,
    pub results: Vec<SearchHit>,
}

// ===== G4 filter grammar types (Slice 35) ===============================

/// G4 (Slice 35) — scalar value for [`Predicate`] comparisons.
///
/// Shared vocabulary with G10 — defined once at the `fathomdb-engine` crate
/// root so reserved-gap 37 (full G4↔G10 unification) can import it without a
/// path change. Derives `Clone, Debug, PartialEq` per the ADR contract
/// (D-F1 exhaustiveness: exactly `{Text, Integer, Bool}`).
#[derive(Clone, Debug, PartialEq)]
pub enum ScalarValue {
    Text(String),
    Integer(i64),
    Bool(bool),
}

/// G4 (Slice 35) — comparison operator for [`Predicate::JsonPathCompare`].
///
/// Shared vocabulary (same crate-root export as `ScalarValue`). Closed
/// enum: `{Gt, Gte, Lt, Lte}` per D-F1. Derives `Clone, Debug, PartialEq`.
#[derive(Clone, Debug, PartialEq)]
pub enum ComparisonOp {
    Gt,
    Gte,
    Lt,
    Lte,
}

/// Allowed JSON paths for [`Predicate`] constructors. The SQL compilation in
/// [`Engine::read_list`] uses the **allowlist constant** (a server-side literal),
/// never the caller-supplied string, so only paths in this set reach
/// `json_extract`. Callers receive [`EngineError::InvalidFilter`] for any
/// non-allowlisted path — no passthrough, no panic.
///
/// To extend: add an entry here. No API change is needed; the constructor
/// accepts the new path string once it appears in this array.
const PREDICATE_PATH_ALLOWLIST: &[&str] =
    &["$.status", "$.priority", "$.tags", "$.kind", "$.created_at"];

/// G4 (Slice 35) — closed typed predicate for [`Engine::read_list`] filter.
///
/// Exactly two variants per ADR D-F1 (`{JsonPathEq, JsonPathCompare}`).
/// The fused variants (`JsonPathFused*`) and all `*_unchecked` builders are
/// explicitly EXCLUDED (ADR D-F2). Use the validated constructors
/// [`Predicate::json_path_eq`] / [`Predicate::json_path_compare`]; they
/// enforce the path allowlist at construction time.
///
/// Multiple predicates in [`Engine::read_list`] are combined by implicit AND
/// (D-F5). Compilation target: `json_extract(body, '$.field') <op> ?` with
/// a bound parameter (never interpolated — injection-safe per D-F4).
#[derive(Clone, Debug, PartialEq)]
pub enum Predicate {
    /// `json_extract(body, path) = ?` (equality).
    JsonPathEq { path: String, value: ScalarValue },
    /// `json_extract(body, path) <op> ?` (inequality).
    JsonPathCompare { path: String, op: ComparisonOp, value: ScalarValue },
}

impl Predicate {
    /// Construct a `JsonPathEq` predicate with allowlist validation.
    ///
    /// Returns [`EngineError::InvalidFilter`] if `path` is not in
    /// [`PREDICATE_PATH_ALLOWLIST`]; never panics on bad input.
    pub fn json_path_eq(path: impl Into<String>, value: ScalarValue) -> Result<Self, EngineError> {
        let path = path.into();
        if !PREDICATE_PATH_ALLOWLIST.contains(&path.as_str()) {
            return Err(EngineError::InvalidFilter {
                reason: format!("path '{path}' is not in the predicate path allowlist"),
            });
        }
        Ok(Self::JsonPathEq { path, value })
    }

    /// Construct a `JsonPathCompare` predicate with allowlist validation.
    ///
    /// Returns [`EngineError::InvalidFilter`] if `path` is not in
    /// [`PREDICATE_PATH_ALLOWLIST`]; never panics on bad input.
    pub fn json_path_compare(
        path: impl Into<String>,
        op: ComparisonOp,
        value: ScalarValue,
    ) -> Result<Self, EngineError> {
        let path = path.into();
        if !PREDICATE_PATH_ALLOWLIST.contains(&path.as_str()) {
            return Err(EngineError::InvalidFilter {
                reason: format!("path '{path}' is not in the predicate path allowlist"),
            });
        }
        Ok(Self::JsonPathCompare { path, op, value })
    }

    /// Return the validated path string for use in SQL compilation.
    /// This always returns a path that is in `PREDICATE_PATH_ALLOWLIST`.
    fn path(&self) -> &str {
        match self {
            Self::JsonPathEq { path, .. } => path.as_str(),
            Self::JsonPathCompare { path, .. } => path.as_str(),
        }
    }

    /// Compile this predicate to a SQL WHERE clause fragment.
    /// The path is validated at construction time and is always an allowlist
    /// constant — never the raw caller-supplied string.
    fn to_sql_clause(&self, param_idx: usize) -> String {
        // The path is already validated against the allowlist at construction.
        // We use the allowlist entry (the stored path) directly as a SQL literal.
        // The VALUE is always a bound `?` parameter (injection-safe).
        //
        // Type guards prevent cross-type matches caused by SQLite's json_extract
        // coercing JSON booleans to integer 1/0:
        //   - Bool predicates: AND json_type IN ('true', 'false') — exclude integers
        //   - Integer predicates: AND json_type = 'integer' — exclude booleans
        // Text predicates need no guard: json_extract returns TEXT for strings and
        // the coercion never conflates TEXT with integer/bool.
        let path = self.path();
        match self {
            Self::JsonPathEq { value, .. } => match value {
                ScalarValue::Bool(_) => format!(
                    "json_extract(body, '{path}') = ?{param_idx} \
                     AND json_type(body, '{path}') IN ('true', 'false')"
                ),
                ScalarValue::Integer(_) => format!(
                    "json_extract(body, '{path}') = ?{param_idx} \
                     AND json_type(body, '{path}') = 'integer'"
                ),
                ScalarValue::Text(_) => {
                    format!("json_extract(body, '{path}') = ?{param_idx}")
                }
            },
            Self::JsonPathCompare { op, value, .. } => {
                let op_str = match op {
                    ComparisonOp::Gt => ">",
                    ComparisonOp::Gte => ">=",
                    ComparisonOp::Lt => "<",
                    ComparisonOp::Lte => "<=",
                };
                match value {
                    ScalarValue::Bool(_) => format!(
                        "json_extract(body, '{path}') {op_str} ?{param_idx} \
                         AND json_type(body, '{path}') IN ('true', 'false')"
                    ),
                    ScalarValue::Integer(_) => format!(
                        "json_extract(body, '{path}') {op_str} ?{param_idx} \
                         AND json_type(body, '{path}') = 'integer'"
                    ),
                    ScalarValue::Text(_) => format!(
                        "json_extract(body, '{path}') {op_str} ?{param_idx} \
                         AND json_type(body, '{path}') = 'text'"
                    ),
                }
            }
        }
    }

    /// Bind the value of this predicate as a rusqlite parameter.
    fn bind_value(&self) -> rusqlite::types::Value {
        let value = match self {
            Self::JsonPathEq { value, .. } => value,
            Self::JsonPathCompare { value, .. } => value,
        };
        match value {
            ScalarValue::Text(s) => rusqlite::types::Value::Text(s.clone()),
            ScalarValue::Integer(i) => rusqlite::types::Value::Integer(*i),
            ScalarValue::Bool(b) => rusqlite::types::Value::Integer(i64::from(*b)),
        }
    }
}

// ===== Slice 20 (G5/G6) — graph traversal types =========================

/// Slice 20 (G5) — direction of graph traversal for
/// [`Engine::graph_neighbors`] / [`Engine::search_expand`].
///
/// `Outgoing` follows edges where the root is the `from_id` (source).
/// `Incoming` follows edges where the root is the `to_id` (target).
/// `Both` follows edges in either direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraversalDirection {
    Outgoing,
    Incoming,
    Both,
}

/// Slice 20 (G6) — result of [`Engine::search_expand`]: initial search hits
/// plus nodes reached by bounded BFS expansion that are not already in the
/// search hit set.
#[derive(Clone, Debug)]
pub struct SearchExpandResult {
    /// Original RRF-scored search results (G1+G9 hybrid).
    pub search_hits: Vec<SearchHit>,
    /// Nodes reached by graph traversal but NOT already in `search_hits`.
    /// Each entry is `(node, hop_count)` where `hop_count` is the BFS depth
    /// from the nearest search hit that reached this node.
    pub expanded: Vec<(NodeRecord, u32)>,
    /// Deduplicated union of all logical_ids (search hits first, then expanded).
    pub all_logical_ids: Vec<String>,
}

/// G10 — closed metadata filter for [`Engine::search_filtered`] (Slice 10).
///
/// All fields are optional; a `None` field imposes no constraint, and an
/// all-`None` filter (or `None` filter) is the unfiltered path whose phase-1 SQL
/// is byte-identical to 0.7.2. This is a **closed struct**, not an open filter
/// DSL (ADR-0.8.0-agent-memory-retrieval-and-identity Q1); the filter-grammar /
/// `list` decision stays a later-slice concern.
///
/// `created_after` is a `created_at >= bound` lower bound in unix seconds.
/// `status` is wired through to the vec0 `status` metadata column. vec0 TEXT
/// metadata columns are **NOT NULL-able**, so the "no real population yet" state
/// is an **empty-string sentinel** `''` (a forced deviation from the planned
/// "NULL plumbing"; a real population source is reserved-gap candidate 13). A
/// `status = Some("open")`-style filter therefore prunes every row until that
/// population slice lands.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchFilter {
    pub source_type: Option<String>,
    pub kind: Option<String>,
    pub created_after: Option<i64>,
    pub status: Option<String>,
}

impl SearchFilter {
    /// True when no field constrains the search — equivalent to `None`. Used to
    /// keep the unfiltered code path (and its byte-identical SQL) on the
    /// all-`None` struct.
    fn is_unfiltered(&self) -> bool {
        self.source_type.is_none()
            && self.kind.is_none()
            && self.created_after.is_none()
            && self.status.is_none()
    }
}

/// G11 (Slice 15) — a document sent to a BYO-LLM extraction harness via
/// [`Engine::ingest_with_extractor`].
#[derive(Clone, Debug)]
pub struct ExtractDocument {
    /// Stable opaque identifier for this document. Used as `source_id` on
    /// ingested edges and for provenance tracking.
    pub source_doc_id: String,
    /// Full text body of the document to extract entities and relationships from.
    pub body: String,
}

/// G11 (Slice 15) — receipt returned by [`Engine::ingest_with_extractor`].
#[derive(Clone, Debug, Default)]
pub struct IngestWithExtractorReceipt {
    /// Number of `canonical_nodes` rows written (new entity insertions; skipped
    /// for entities that already have a matching active logical_id).
    pub nodes_written: u64,
    /// Number of `canonical_edges` rows written (new fact-edge insertions;
    /// superseded prior edges are ALSO counted as rows written).
    pub edges_written: u64,
    /// Number of documents processed (including no-facts documents).
    pub docs_processed: u64,
}

/// Batch input shape for [`Engine::write`].
///
/// Marked `#[non_exhaustive]` per ADR-0.6.0-prepared-write-shape; new
/// entity variants land in 0.6.x without a major bump. Adding fields to
/// existing variants remains a binding-coordination change.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum PreparedWrite {
    Node {
        kind: String,
        body: String,
        /// REQ-026 / AC-028 / AC-042 recovery seam. `None` is the
        /// back-compat default and lands as NULL on disk; callers that
        /// participate in `excise_source` / `trace_source_ref` must
        /// supply a stable identifier.
        source_id: Option<String>,
        /// G0 (Slice 15) — stable cross-re-ingestion identity. `Some(id)`
        /// makes this write a transaction-time supersession of the prior
        /// active version of `(logical_id, kind)` (tombstone-then-insert).
        /// `None` is the legacy/own-identity default: a plain insert with a
        /// NULL `logical_id` (NULL-safe — never collides with other NULLs).
        logical_id: Option<String>,
    },
    Edge {
        kind: String,
        from: String,
        to: String,
        /// REQ-026 / AC-028 / AC-042 recovery seam — see Node.
        source_id: Option<String>,
        /// G0 (Slice 15) — see Node. Supersession semantics are identical on
        /// edges (keyed by `(logical_id, kind)`).
        logical_id: Option<String>,
        /// G11 (Slice 15) — the fact/relationship text. When `Some`, triggers
        /// FTS projection into `search_index_edges` and vector projection via
        /// the projection scheduler (kind `"edge_fact"`). Also triggers
        /// invalidate-not-accumulate on `(from_id, to_id, kind)`.
        body: Option<String>,
        /// G11 (Slice 15) — event valid-time (ISO-8601). NULL = unknown / still valid.
        t_valid: Option<String>,
        /// G11 (Slice 15) — event invalid-time (ISO-8601). NULL = still valid.
        t_invalid: Option<String>,
        /// G11 (Slice 15) — extraction confidence ∈ [0.0, 1.0]. NULL for
        /// non-BYO-LLM-ingested edges.
        confidence: Option<f64>,
        /// G11 (Slice 15) — opaque model/provider id from the BYO-LLM harness
        /// `ready.model` field. NULL for non-BYO-LLM edges.
        extractor_model_id: Option<String>,
        /// R3 (Slice 30, SCHEMA-GATE-1, HITL-SIGNED 2026-06-13) — set when the
        /// ELPS extractor defaulted this edge's `t_valid` to `created_at` rather
        /// than deriving it from the document text. Such edges have untrustworthy
        /// event times and are excluded from graph-arm BFS temporal queries.
        /// `None`/`false` = not a fallback; `Some(true)` = fallback.
        temporal_fallback: Option<bool>,
    },
    OpStore {
        collection: String,
        record_key: String,
        schema_id: Option<String>,
        body: String,
    },
    AdminSchema {
        name: String,
        kind: String,
        schema_json: String,
        retention_json: String,
    },
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
    DatabaseLocked {
        holder_pid: Option<u32>,
    },
    Corruption(CorruptionDetail),
    IncompatibleSchemaVersion {
        seen: u32,
        supported: u32,
    },
    MigrationError {
        schema_version_before: u32,
        schema_version_current: u32,
        step_id: u32,
    },
    EmbedderIdentityMismatch {
        stored: EmbedderIdentity,
        supplied: EmbedderIdentity,
    },
    EmbedderDimensionMismatch {
        stored: u32,
        supplied: u32,
    },
    /// Embedder runtime returned a typed error during `Engine::open`.
    Embedder(RuntimeEmbedderError),
    Io {
        message: String,
    },
}

/// Caller-facing selector for the embedder used by an opened engine
/// (`dev/design/embedder.md` §0).
#[derive(Clone)]
pub enum EmbedderChoice {
    /// Use the engine's default embedder. With the `default-embedder`
    /// Cargo feature enabled, this materializes a `CandleBgeEmbedder`
    /// via the EU-3 loader at `Engine::open`; on first use the loader
    /// downloads pinned bge-small-en-v1.5 weights from HuggingFace per
    /// `ADR-0.7.1-default-embedder-weight-fetch`. Without the feature,
    /// this returns `EmbedderError::Failed` directing the caller to
    /// rebuild with `--features default-embedder` or supply
    /// `EmbedderChoice::Caller`.
    Default,
    /// Caller supplies the embedder instance. The supplied embedder's
    /// `identity()` becomes the workspace's default-profile identity.
    Caller(Arc<dyn Embedder>),
    /// No embedder configured. Engine opens; subsequent vector writes
    /// fail with `EngineError::EmbedderNotConfigured`. Useful for
    /// read-only or canonical-only flows.
    None,
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
            Self::Embedder(err) => match err {
                RuntimeEmbedderError::Timeout => write!(f, "embedder timeout during open"),
                RuntimeEmbedderError::Failed { message } => {
                    write!(f, "embedder failure during open: {message}")
                }
            },
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
    EmbedderDimensionMismatch {
        expected: u32,
        actual: u32,
    },
    Scheduler,
    OpStore,
    WriteValidation,
    SchemaValidation,
    Overloaded,
    Closing,
    /// G11 (Slice 15) — BYO-LLM extractor subprocess error (protocol mismatch,
    /// spawn failure, or harness-returned error code).
    Extractor,
    /// G4 (Slice 35) — filter predicate construction error: non-allowlisted
    /// path or invalid filter argument. NOT a panic — returned as a typed error
    /// from [`Predicate::json_path_eq`] / [`Predicate::json_path_compare`].
    InvalidFilter {
        reason: String,
    },
    /// Slice 20 (G5/G6) — an argument is out of the accepted range (e.g.
    /// `depth > 3` for graph traversal). The `msg` field carries a
    /// human-readable explanation; it is intentionally non-exhaustive so the
    /// binding layer can forward it as a `ValueError` / `TypeError`.
    InvalidArgument {
        msg: String,
    },
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
            Self::Extractor => write!(f, "extractor error"),
            Self::InvalidFilter { reason } => write!(f, "invalid filter: {reason}"),
            Self::InvalidArgument { msg } => write!(f, "invalid argument: {msg}"),
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
            Self::Extractor => "ExtractorError",
            Self::InvalidFilter { .. } => "InvalidFilterError",
            Self::InvalidArgument { .. } => "InvalidArgumentError",
        }
    }
}

impl Error for EngineError {}

/// Doctor `check-integrity` invocation flags. `quick` and `round_trip`
/// are accepted in 0.6.0 but treated as default; only `full` activates
/// `PRAGMA integrity_check`. Per `dev/design/recovery.md` § Doctor-only
/// flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CheckIntegrityOpts {
    pub quick: bool,
    pub full: bool,
    pub round_trip: bool,
}

/// One section of an [`IntegrityReport`]. Either every check in the
/// section was clean, or one or more typed [`Finding`]s describe the
/// detected issue. Per AC-043b.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Section {
    Clean,
    Findings(Vec<Finding>),
}

/// Single doctor finding record. Stable report-shape per AC-043c. The
/// `code` and `doc_anchor` strings are stable dispatch keys owned by
/// `dev/design/recovery.md` § Code-to-operator-action cross-reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Finding {
    pub code: &'static str,
    pub stage: &'static str,
    pub locator: CorruptionLocator,
    pub doc_anchor: &'static str,
    pub detail: String,
}

/// Three-section integrity report. AC-043a pins exactly these three
/// keys.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityReport {
    pub physical: Section,
    pub logical: Section,
    pub semantic: Section,
}

/// Result of a successful [`Engine::safe_export`] call. The returned
/// `manifest_sha256` equals the SHA-256 of the export file bytes (per
/// AC-039a) and matches the `sha256` field written into the manifest
/// JSON.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafeExportArtifact {
    pub export_path: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest_sha256: String,
}

/// Phase 9 Pack B trace report (AC-042). One event per canonical row
/// attributable to the requested `source_id`, ordered by `write_cursor`
/// ascending.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceReport {
    pub source_ref: String,
    pub events: Vec<TraceEvent>,
}

/// Single canonical-row tracing record. `table` is one of
/// `"canonical_nodes"` or `"canonical_edges"`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceEvent {
    pub write_cursor: u64,
    pub kind: String,
    pub table: &'static str,
}

/// Which shadow-state surface a [`RebuildReport`] describes.
/// `Projections` covers the full FTS5 + vec0 + projection-terminal
/// rebuild emitted by [`Engine::rebuild_projections`]. `Vec0` covers
/// the vec0-only path emitted by [`Engine::rebuild_vec0`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RebuildKind {
    Projections,
    Vec0,
}

/// Structured result of a rebuild operation. `rows_invalidated` is the
/// total shadow-state rows truncated before re-derivation; `rows_rebuilt`
/// is the count of rows the synchronous rebuild loop re-materialised
/// (asynchronous re-enqueue work performed by the projection scheduler is
/// not counted here). `projection_cursor_after` is the post-rebuild value
/// of the projection cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RebuildReport {
    pub kind: RebuildKind,
    pub rows_invalidated: u64,
    pub rows_rebuilt: u64,
    pub projection_cursor_after: u64,
}

/// Phase 9 Pack B excise report (AC-028a/b/c). Counts are post-excise
/// totals; `projections_invalidated` reports the shadow-row invalidation
/// total (FTS5 + vec0 + projection terminal) for the excised source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExciseReport {
    pub source_ref: String,
    pub nodes_excised: u64,
    pub edges_excised: u64,
    pub projections_invalidated: u64,
}

/// Typed outcome of [`Engine::verify_embedder`]. Mismatches do not raise
/// `EngineError`; the operator workflow needs to see the stored vs.
/// supplied pair to decide on next action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifyEmbedderStatus {
    Match,
    IdentityMismatch,
    DimensionMismatch,
    BothMismatch,
}

/// Result of [`Engine::verify_embedder`]. `stored_identity` is the
/// `name:revision` pair persisted in `_fathomdb_embedder_profiles`;
/// `supplied_identity` echoes the operator's input verbatim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifyEmbedderReport {
    pub stored_identity: String,
    pub stored_dimension: u32,
    pub supplied_identity: String,
    pub supplied_dimension: u32,
    pub status: VerifyEmbedderStatus,
}

/// Single table or index entry emitted by [`Engine::dump_schema`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaObject {
    pub name: String,
    pub sql: String,
}

/// Result of [`Engine::dump_schema`]. `user_version` is the
/// `PRAGMA user_version` sentinel. Canonical tables appear first per
/// [`fathomdb_schema::CANONICAL_TABLES`], then remaining non-`sqlite_*`
/// tables alphabetically. Indexes follow the same alphabetical rule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DumpSchemaReport {
    pub user_version: u32,
    pub tables: Vec<SchemaObject>,
    pub indexes: Vec<SchemaObject>,
}

/// Single canonical-table row count emitted by [`Engine::dump_row_counts`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableRowCount {
    pub name: String,
    pub rows: u64,
}

/// Result of [`Engine::dump_row_counts`]. Canonical tables only;
/// projection / FTS / vec0 shadow tables are excluded. Order matches
/// [`fathomdb_schema::CANONICAL_TABLES`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DumpRowCountsReport {
    pub counts: Vec<TableRowCount>,
}

/// Result of [`Engine::dump_profile`]. Mirrors the open-time embedder
/// posture + the per-kind vector configuration registered in
/// `_fathomdb_vector_kinds`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DumpProfileReport {
    pub embedder_identity: String,
    pub embedder_dimension: u32,
    pub vectorized_kinds: Vec<String>,
}

/// 0.7.2 PR-2b — result of [`Engine::recompute_mean`] (the manual
/// `doctor recompute-mean` path) and of the shared in-transaction
/// recompute core. `drift_cos_before` is the cosine between the freshly
/// derived corpus mean and the previously-pinned mean (1.0 when nothing
/// was pinned yet, i.e. a first pin). `mean_was_pinned` distinguishes a
/// refresh of an existing mean from an initial pin. See
/// `dev/design/embedder.md` §0.3.
#[derive(Clone, Debug, PartialEq)]
pub struct MeanRecomputeReport {
    pub dim: u32,
    pub old_doc_count: u64,
    pub doc_count_requantized: u64,
    pub drift_cos_before: f32,
    pub mean_was_pinned: bool,
    pub elapsed_ms: u64,
}

/// Typed outcome of [`Engine::truncate_wal`]. `Done` matches SQLite's
/// `busy = 0` return from `PRAGMA wal_checkpoint(TRUNCATE)`; any other
/// value surfaces as `Busy`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TruncateWalStatus {
    Done,
    Busy,
}

/// Result of [`Engine::truncate_wal`]. Carries the three counters
/// returned by `PRAGMA wal_checkpoint(TRUNCATE)`: `busy`, `log_frames`,
/// `checkpointed_frames`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TruncateWalReport {
    pub status: TruncateWalStatus,
    pub busy: u32,
    pub log_frames: u32,
    pub checkpointed_frames: u32,
}

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
            None,
            &mut |_| {},
        )
    }

    /// Open an engine with an explicit [`EmbedderChoice`].
    ///
    /// Per `dev/design/embedder.md` §0 + the 0.7.1 EU-5 campaign, this is
    /// the canonical entry point for selecting how the workspace's
    /// default embedder is supplied. See [`EmbedderChoice`] for the
    /// semantics of each variant; in particular `Default` materializes
    /// the pinned BGE embedder via the loader when the `default-embedder`
    /// feature is enabled.
    pub fn open_with_choice(
        path: impl Into<PathBuf>,
        choice: EmbedderChoice,
    ) -> Result<OpenedEngine, EngineOpenError> {
        match choice {
            EmbedderChoice::Default => Self::open_default_embedder(path),
            EmbedderChoice::Caller(embedder) => {
                let identity = embedder.identity();
                Self::open_with_embedder_and_subscriber(
                    path,
                    identity,
                    Some(embedder),
                    None,
                    None,
                    &mut |_| {},
                )
            }
            EmbedderChoice::None => Self::open_with_embedder_and_subscriber(
                path,
                default_embedder_identity(),
                None,
                None,
                None,
                &mut |_| {},
            ),
        }
    }

    /// EU-5b: materialize the engine's pinned default embedder
    /// (`CandleBgeEmbedder` backed by the EU-3 loader) and open the
    /// workspace with it. Without the `default-embedder` feature, fails
    /// with a typed `Embedder` error rather than touching the network.
    #[cfg(feature = "default-embedder")]
    fn open_default_embedder(path: impl Into<PathBuf>) -> Result<OpenedEngine, EngineOpenError> {
        use std::time::Instant as DownloadInstant;
        let download_start = DownloadInstant::now();
        let weights = fathomdb_embedder::loader::load_pinned_default_embedder().map_err(|err| {
            EngineOpenError::Embedder(RuntimeEmbedderError::Failed {
                message: format!("default embedder loader: {err}"),
            })
        })?;
        let events = weights.events.clone();
        let download_ms = if weights.bytes_downloaded > 0 {
            Some(u64::try_from(download_start.elapsed().as_millis()).unwrap_or(u64::MAX))
        } else {
            None
        };
        let embedder =
            fathomdb_embedder::CandleBgeEmbedder::new_from_weights(weights).map_err(|err| {
                EngineOpenError::Embedder(RuntimeEmbedderError::Failed {
                    message: format!("default embedder construct: {err}"),
                })
            })?;
        let embedder: Arc<dyn Embedder> = Arc::new(embedder);
        let identity = embedder.identity();
        let loader_info = LoaderInfo { download_ms, events };
        Self::open_with_embedder_and_subscriber(
            path,
            identity,
            Some(embedder),
            Some(loader_info),
            None,
            &mut |_| {},
        )
    }

    #[cfg(not(feature = "default-embedder"))]
    fn open_default_embedder(_path: impl Into<PathBuf>) -> Result<OpenedEngine, EngineOpenError> {
        Err(EngineOpenError::Embedder(RuntimeEmbedderError::Failed {
            message: "EmbedderChoice::Default requires the `default-embedder` Cargo feature"
                .to_string(),
        }))
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
        Self::open_with_embedder_and_subscriber(
            path,
            identity,
            Some(embedder),
            None,
            None,
            &mut |_| {},
        )
    }

    fn open_with_embedder_and_subscriber(
        path: impl Into<PathBuf>,
        embedder_identity: EmbedderIdentity,
        runtime_embedder: Option<Arc<dyn Embedder>>,
        loader_info: Option<LoaderInfo>,
        initial_subscriber: Option<Arc<dyn lifecycle::Subscriber>>,
        emit_migration_event: &mut impl FnMut(&MigrationStepReport),
    ) -> Result<OpenedEngine, EngineOpenError> {
        Self::open_with_migrations(
            path,
            MIGRATIONS,
            embedder_identity,
            runtime_embedder,
            loader_info,
            emit_migration_event,
            initial_subscriber,
        )
    }

    fn open_with_migrations(
        path: impl Into<PathBuf>,
        migrations: &'static [fathomdb_schema::Migration],
        embedder_identity: EmbedderIdentity,
        runtime_embedder: Option<Arc<dyn Embedder>>,
        loader_info: Option<LoaderInfo>,
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
            Ok((connection, readers, mut report, reader_lookaside_rcs)) => {
                // EU-5b — splice the loader's measurements + structured
                // events into the report. The loader path is the only
                // surface that produces these today; caller-supplied
                // embedders and EmbedderChoice::None leave them as the
                // open_locked defaults (None / empty).
                if let Some(info) = loader_info {
                    if info.download_ms.is_some() {
                        report.embedder_download_ms = info.download_ms;
                    }
                    if !info.events.is_empty() {
                        report.embedder_events = info.events;
                    }
                }
                let next_cursor = load_next_cursor(&connection);
                let subscribers = Arc::new(lifecycle::SubscriberRegistry::new());
                let profiling_enabled = Arc::new(AtomicBool::new(false));
                let slow_threshold_ms = Arc::new(AtomicU64::new(DEFAULT_SLOW_THRESHOLD_MS));
                let mut profile_contexts: Vec<Box<ProfileContext>> = Vec::new();
                let projection_runtime = ProjectionRuntime::new(
                    canonical_path.clone(),
                    runtime_embedder.clone(),
                    embedder_identity.clone(),
                    report.embedder_mean_vec_pinned,
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
        init_perf_experiments_runtime();
        register_sqlite_vec_extension();
        let mut connection = Connection::open(&path)
            .map_err(|err| map_open_sqlite_error(err, OpenStage::HeaderProbe))?;
        // Order pinned by `dev/design/errors.md` § OpenStage matrix: each
        // step routes its own SQLite-level error to a distinct
        // `CorruptionKind` (Header → WalReplay → Schema → EmbedderIdentity).
        // The schema and WAL probes both happen BEFORE `pragma WAL`
        // because that pragma also reads page 1 — letting it run first
        // would reclassify schema-side corruption as a WAL replay
        // failure, breaking the AC-035b stable-code contract.
        probe_database_header(&connection)?;
        probe_open_integrity(&connection)?;
        probe_wal_sidecar(&path)?;
        // 0.7.0 perf-experiments: apply writer-side experiment PRAGMAs
        // (page_size, etc.) BEFORE journal_mode + migrations. page_size
        // is silently ignored once any table exists; this is the only
        // legal window to set it on a fresh DB. Gated on
        // FATHOMDB_PERF_EXPERIMENTS=1; no-op in production.
        apply_perf_experiment_writer_pragmas(&connection);
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|err| map_open_sqlite_error(err, OpenStage::WalReplay))?;

        reject_legacy_shape(&connection)?;
        let migration = migrate_with_event_sink(&connection, migrations, emit_migration_event)
            .map_err(map_migration_error)?;
        // 0.8.0 Slice 5 (G1) — global FTS5 tokenizer-default upgrade. Step 11
        // drops + recreates `search_index` with the new tokenizer, leaving it
        // EMPTY on a migrated DB. The projection scheduler will NOT
        // repopulate it (`database_has_pending_projection_work` keys "pending"
        // off `_fathomdb_projection_terminal`, which the migration does not
        // clear). Re-tokenize from the canonical source rows here, on the
        // writer connection, single-threaded, before readers spawn —
        // projection-only, no source-record migration.
        //
        // Crash-retryable (fix-1): step 11 commits `user_version = 11` with an
        // empty index in its OWN transaction; this reproject commits in a
        // LATER transaction. A crash in that window leaves a durable v11 + empty
        // index, on which a boundary-crossing guard (`before < 11`) is FALSE,
        // skipping repair forever. So gate on the completion marker's ABSENCE
        // (written atomically with the reindex) instead: idempotent, and a
        // crash before the reindex commit simply re-runs on the next open.
        if migration.schema_version_after >= SEARCH_INDEX_TOKENIZER_SCHEMA_VERSION
            && !search_index_tokenizer_reproject_complete(&connection).map_err(|_| {
                EngineOpenError::Io {
                    message: "could not read search_index tokenizer reproject marker".to_string(),
                }
            })?
        {
            reproject_search_index_after_tokenizer_upgrade(&connection).map_err(|_| {
                EngineOpenError::Io {
                    message: "could not re-tokenize search_index after tokenizer upgrade"
                        .to_string(),
                }
            })?;
        }
        let mut embedder_mean_vec_pinned = check_embedder_profile(&connection, embedder_identity)?;
        ensure_vector_partition(&mut connection, embedder_identity.dimension).map_err(|_| {
            EngineOpenError::Io { message: "could not initialize vector partition".to_string() }
        })?;

        // EU-5f — recovery pin (`dev/design/embedder.md` §0.3, Hazard 4). If
        // the identity is MC-required, no mean is pinned, yet the workspace
        // already holds >= MEAN_VEC_PIN_THRESHOLD vector rows (e.g. a crash
        // between the threshold-crossing write and its pin commit), derive
        // the mean from the existing un-centered rows and pin+re-quantize
        // now, single-threaded, before the projection workers spawn. The
        // NULL guard makes this idempotent on subsequent opens.
        if identity_requires_mean_centering(embedder_identity) && !embedder_mean_vec_pinned {
            let row_count: u64 = connection
                .query_row("SELECT COUNT(*) FROM vector_default", [], |row| row.get(0))
                .unwrap_or(0);
            if row_count >= MEAN_VEC_PIN_THRESHOLD {
                recover_mean_vec_pin(&mut connection, embedder_identity).map_err(|_| {
                    EngineOpenError::Io {
                        message: "could not recover mean-centering pin".to_string(),
                    }
                })?;
                embedder_mean_vec_pinned = true;
            }
        }

        let warmup_started = Instant::now();
        // Static identity capability — see `dev/design/embedder.md`
        // §0.6. Today only the bge-small identity reports `true`; the
        // noop scaffolding identity is `false`. EU-5b's identity flip
        // makes the Default path return `true` here automatically.
        let embedder_mean_centering_required = embedder_identity.name == BGE_SMALL_EMBEDDER_NAME;
        // EU-5a2 — populated from `_fathomdb_embedder_profiles.mean_vec`
        // by `check_embedder_profile` above (was hard-coded `false` in
        // EU-5a1). Dimension invariant (§0.2) enforced by that check.
        let report = OpenReport {
            schema_version_before: migration.schema_version_before,
            schema_version_after: migration.schema_version_after,
            migration_steps: migration.migration_steps,
            embedder_warmup_ms: u64::try_from(warmup_started.elapsed().as_millis())
                .unwrap_or(u64::MAX),
            query_backend: "fathomdb-query + sqlite-vec",
            default_embedder: embedder_identity.clone(),
            // TODO(EU-5b): surface `LoadedWeights.download_ms` from the
            // loader once the Default path materializes through it.
            embedder_download_ms: None,
            // TODO(EU-5b): surface `LoadedWeights.events` from the loader.
            embedder_events: Vec::new(),
            embedder_mean_centering_required,
            embedder_mean_vec_pinned,
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
            apply_perf_experiment_reader_pragmas(&reader);
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
        // One cursor per row. `base_cursor` is the last committed cursor;
        // row i in the batch gets cursor `base_cursor + i + 1`, and the
        // batch's final cursor (returned in WriteReceipt and stored as
        // the new `next_cursor`) is `base_cursor + batch.len()`. Sharing
        // one cursor across the batch previously collapsed every vec0
        // INSERT onto the same rowid via `INSERT OR IGNORE` — see
        // `dev/notes/0.7.0-engine-batch-vec0-collapse.md`.
        let base_cursor = self.next_cursor.load(Ordering::SeqCst);
        let increment = u64::try_from(batch.len()).unwrap_or(u64::MAX);
        let last_cursor = base_cursor.saturating_add(increment);
        // G11 (Slice 15) — edge bodies also need projection-runtime notification.
        // `collect_projection_jobs` only tracks Node items (pre-fetched for
        // cursor assignment); edge bodies update `_fathomdb_projection_state` in
        // `commit_batch` but need the scanner to wake up via `notify_new_work`.
        let has_edge_body_work =
            batch.iter().any(|w| matches!(w, PreparedWrite::Edge { body: Some(_), .. }));
        let pending_projection = !projection_jobs.is_empty() || has_edge_body_work;

        let dangling_edge_endpoints = match commit_batch(
            connection,
            batch,
            &plans,
            base_cursor,
            self.provenance_row_cap.load(Ordering::Relaxed),
        ) {
            Ok(count) => count,
            Err(err) => {
                self.emit_sqlite_internal_error(&err);
                return Err(EngineError::Storage);
            }
        };
        self.next_cursor.store(last_cursor, Ordering::SeqCst);
        if pending_projection {
            self.projection_runtime.notify_new_work();
        }

        // G0 — surface the per-row cursors (1:1 with input order). Row i got
        // `base_cursor + i + 1`, matching the allocation in `commit_batch`.
        let row_cursors = (0..batch.len())
            .map(|i| base_cursor.saturating_add((i as u64).saturating_add(1)))
            .collect();
        Ok(WriteReceipt { cursor: last_cursor, row_cursors, dangling_edge_endpoints })
    }

    /// G11 (Slice 15) — BYO-LLM ingest: spawn an external extraction harness
    /// speaking the `fathomdb.extract.v1` NDJSON-over-stdio protocol, send
    /// documents for extraction, and write the resulting entities
    /// (→ `canonical_nodes`) and fact-edges (→ `canonical_edges` with G11
    /// enrichment columns) to the store.
    ///
    /// `cmd` is argv (first element = program, rest = args). Documents are
    /// batched per the harness's `max_docs_per_request`. Entity `logical_id`
    /// is derived as `sha256("<type>:<name>")` (lowercase, hex-encoded) for
    /// stable cross-re-ingestion identity. Edge `logical_id` is derived as
    /// `sha256("<from_lid>:<to_lid>:<relation>")`. Both are consistent with
    /// G0 supersession: re-ingesting the same document yields the same ids,
    /// triggering tombstone-then-insert rather than accumulation.
    ///
    /// Returns [`EngineError::Extractor`] on protocol errors (bad handshake,
    /// subprocess spawn failure, JSON decode error). `no_facts` warnings from
    /// the harness are not errors and do not affect the receipt counts.
    pub fn ingest_with_extractor(
        &self,
        cmd: &[&str],
        documents: &[ExtractDocument],
    ) -> Result<IngestWithExtractorReceipt, EngineError> {
        let (program, args) = cmd.split_first().ok_or(EngineError::Extractor)?;
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|_| EngineError::Extractor)?;

        let child_stdin = child.stdin.take().ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            EngineError::Extractor
        })?;
        let child_stdout = child.stdout.take().ok_or_else(|| {
            let _ = child.kill();
            let _ = child.wait();
            EngineError::Extractor
        })?;

        // Delegate to inner; always reap the child regardless of outcome.
        // drop(writer) inside inner sends EOF → child exits; child.wait() here reaps.
        // On error paths, inner's writer is dropped by Rust's auto-drop → EOF sent;
        // child.kill() is a no-op if already exited.
        let result = self.ingest_with_extractor_inner(child_stdin, child_stdout, documents);
        let _ = child.kill();
        let _ = child.wait();
        result
    }

    fn ingest_with_extractor_inner(
        &self,
        child_stdin: std::process::ChildStdin,
        child_stdout: std::process::ChildStdout,
        documents: &[ExtractDocument],
    ) -> Result<IngestWithExtractorReceipt, EngineError> {
        // --- handshake: hello → ready ---
        let hello = serde_json::json!({
            "protocol": "fathomdb.extract.v1",
            "type": "hello",
            "schema_version": 1,
        });
        let mut writer = std::io::BufWriter::new(child_stdin);

        // fix-35 [P1/P2]: drain stdout on a dedicated thread so (a) every read can
        // be bounded with a timeout — a hung harness can no longer block ingest
        // forever — and (b) the child's stdout pipe is drained continuously,
        // preventing a large-request deadlock (parent blocked writing stdin while
        // the child blocks writing a full stdout pipe). The handle is detached:
        // joining could hang if a misbehaving child holds stdout open past its
        // stdin EOF, so the outer `child.kill()` is what guarantees thread exit.
        let io_timeout = extractor_io_timeout();
        let (line_tx, line_rx) = mpsc::channel::<std::io::Result<String>>();
        thread::spawn(move || {
            let mut reader = BufReader::new(child_stdout);
            loop {
                let mut buf = String::new();
                match reader.read_line(&mut buf) {
                    Ok(0) => break,
                    Ok(_) => {
                        if line_tx.send(Ok(buf)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = line_tx.send(Err(e));
                        break;
                    }
                }
            }
        });

        let hello_line = serde_json::to_string(&hello).map_err(|_| EngineError::Extractor)?;
        writeln!(writer, "{hello_line}").map_err(|_| EngineError::Extractor)?;
        writer.flush().map_err(|_| EngineError::Extractor)?;

        let line = recv_extractor_line(&line_rx, io_timeout)?;
        let ready: Value = serde_json::from_str(line.trim()).map_err(|_| EngineError::Extractor)?;
        // fix-23 [P2]: validate protocol + schema_version in the ready message per ADR.
        if ready.get("type").and_then(|v| v.as_str()) != Some("ready")
            || ready.get("protocol").and_then(|v| v.as_str()) != Some("fathomdb.extract.v1")
            || ready.get("schema_version").and_then(|v| v.as_u64()) != Some(1)
        {
            return Err(EngineError::Extractor);
        }
        let extractor_model_id = ready.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());
        let max_docs =
            ready.get("max_docs_per_request").and_then(|v| v.as_u64()).unwrap_or(8) as usize;
        // fix-1 [P2]: reject zero max_docs_per_request to prevent chunks(0) panic.
        if max_docs == 0 {
            return Err(EngineError::Extractor);
        }

        // --- per-batch extract → write loop ---
        let mut nodes_written: u64 = 0;
        let mut edges_written: u64 = 0;
        let docs_processed = documents.len() as u64;

        for (batch_idx, batch) in documents.chunks(max_docs).enumerate() {
            let request_id = format!("req-{batch_idx}");
            let docs_json: Vec<Value> = batch
                .iter()
                .map(|d| {
                    serde_json::json!({
                        "source_doc_id": d.source_doc_id,
                        "body": d.body,
                    })
                })
                .collect();

            let extract = serde_json::json!({
                "protocol": "fathomdb.extract.v1",
                "type": "extract",
                "request_id": request_id,
                "documents": docs_json,
            });
            let extract_line =
                serde_json::to_string(&extract).map_err(|_| EngineError::Extractor)?;
            writeln!(writer, "{extract_line}").map_err(|_| EngineError::Extractor)?;
            writer.flush().map_err(|_| EngineError::Extractor)?;

            let result_line = recv_extractor_line(&line_rx, io_timeout)?;
            let result: Value =
                serde_json::from_str(result_line.trim()).map_err(|_| EngineError::Extractor)?;

            // fix-24 [P2]: require type=="result" AND matching request_id; any
            // other envelope (error, wrong id, missing type) is a protocol fault.
            let resp_type = result.get("type").and_then(|v| v.as_str());
            let resp_id = result.get("request_id").and_then(|v| v.as_str());
            if resp_type != Some("result") || resp_id != Some(request_id.as_str()) {
                return Err(EngineError::Extractor);
            }

            // --- map entities → PreparedWrite::Node with stable logical_id ---
            let entities =
                result.get("entities").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let raw_edges =
                result.get("edges").and_then(|v| v.as_array()).cloned().unwrap_or_default();

            // R3 (SCHEMA-GATE-1): collect substituted_t_valid values from
            // temporal_fallback warnings. An edge whose t_valid matches one of
            // these values had its event time defaulted to created_at (not
            // text-grounded) and must be flagged so BFS can exclude it.
            let fallback_dates: std::collections::HashSet<String> = result
                .get("warnings")
                .and_then(|v| v.as_array())
                .map(|ws| {
                    ws.iter()
                        .filter(|w| {
                            w.get("kind").and_then(|k| k.as_str()) == Some("temporal_fallback")
                        })
                        .filter_map(|w| {
                            w.get("substituted_t_valid")
                                .and_then(|v| v.as_str())
                                .map(str::to_string)
                        })
                        .collect()
                })
                .unwrap_or_default();

            if !entities.is_empty() {
                let node_batch: Vec<PreparedWrite> = entities
                    .iter()
                    .map(|entity| -> Result<PreparedWrite, EngineError> {
                        let name = entity.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let kind = entity.get("type").and_then(|v| v.as_str()).unwrap_or("entity");
                        let source_doc_id = entity
                            .get("source_doc_id")
                            .and_then(|v| v.as_str())
                            .map(str::to_string);
                        // fix-34 [P1]: derive_logical_id now rejects an empty name
                        // or a ':' in kind — inputs that would collide distinct
                        // entities onto one identity and silently drop one.
                        let logical_id = derive_logical_id(kind, name)?;
                        Ok(PreparedWrite::Node {
                            kind: kind.to_string(),
                            body: name.to_string(),
                            source_id: source_doc_id,
                            logical_id: Some(logical_id),
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                // fix-29/fix-34 [P2]: deduplicate within the batch by logical_id so
                // a harness that returns the same entity twice does not write a row
                // that immediately supersedes its sibling (shared with the edge arm).
                let node_batch = dedup_prepared_by_logical_id(node_batch);

                // fix-23 [P2]: skip entities whose logical_id is already active
                // to avoid needless supersede churn on re-ingest.
                let ids: Vec<String> = node_batch
                    .iter()
                    .filter_map(|w| {
                        if let PreparedWrite::Node { logical_id: Some(id), .. } = w {
                            Some(id.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                let existing: std::collections::HashSet<String> = self
                    .read_get_many(&ids)?
                    .into_iter()
                    .zip(ids)
                    .filter_map(|(opt, id)| opt.map(|_| id))
                    .collect();
                let new_nodes: Vec<PreparedWrite> = node_batch
                    .into_iter()
                    .filter(|w| {
                        if let PreparedWrite::Node { logical_id: Some(id), .. } = w {
                            !existing.contains(id)
                        } else {
                            true
                        }
                    })
                    .collect();
                if !new_nodes.is_empty() {
                    let n = new_nodes.len() as u64;
                    self.write(&new_nodes)?;
                    nodes_written = nodes_written.saturating_add(n);
                }
            }

            // --- map edges → PreparedWrite::Edge with G11 columns ---
            if !raw_edges.is_empty() {
                // fix-33 [P1]: the protocol gives edges NO endpoint types —
                // `from_entity`/`to_entity` reference entities BY NAME (or alias).
                // Build a name+alias → (canonical name, type) index from the same
                // result's `entities[]` so each endpoint's logical_id matches the
                // node's. (Nodes derive id from the entity's real type; defaulting
                // the edge endpoint kind to "entity" orphaned every contract-faithful
                // edge from its nodes and tripped the G8 dangling probe.)
                //
                // Two passes so a canonical NAME always wins over a (different
                // entity's) ALIAS regardless of `entities[]` order: pass 1 inserts
                // all canonical names, pass 2 fills aliases only where no name
                // already claims that key. (Name↔name clashes remain first-wins —
                // contradictory input; no principled resolution exists.)
                let mut entity_index: std::collections::HashMap<String, (String, String)> =
                    std::collections::HashMap::new();
                for entity in &entities {
                    let name = entity.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    if name.is_empty() {
                        continue;
                    }
                    let kind =
                        entity.get("type").and_then(|v| v.as_str()).unwrap_or("entity").to_string();
                    entity_index
                        .entry(name.to_lowercase())
                        .or_insert_with(|| (name.to_string(), kind));
                }
                for entity in &entities {
                    let name = entity.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    if name.is_empty() {
                        continue;
                    }
                    let kind =
                        entity.get("type").and_then(|v| v.as_str()).unwrap_or("entity").to_string();
                    if let Some(aliases) = entity.get("aliases").and_then(|v| v.as_array()) {
                        for alias in aliases.iter().filter_map(|a| a.as_str()) {
                            if !alias.is_empty() {
                                entity_index
                                    .entry(alias.to_lowercase())
                                    .or_insert_with(|| (name.to_string(), kind.clone()));
                            }
                        }
                    }
                }

                let edge_batch: Vec<PreparedWrite> = raw_edges
                    .iter()
                    .map(|edge| -> Result<PreparedWrite, EngineError> {
                        let from_entity =
                            edge.get("from_entity").and_then(|v| v.as_str()).unwrap_or("");
                        let to_entity =
                            edge.get("to_entity").and_then(|v| v.as_str()).unwrap_or("");
                        let relation =
                            edge.get("relation").and_then(|v| v.as_str()).unwrap_or("related_to");
                        let body = edge.get("body").and_then(|v| v.as_str()).map(str::to_string);
                        let t_valid =
                            edge.get("t_valid").and_then(|v| v.as_str()).map(str::to_string);
                        let t_invalid =
                            edge.get("t_invalid").and_then(|v| v.as_str()).map(str::to_string);
                        // fix-26 [P2]: validate confidence is in [0.0, 1.0] at the
                        // protocol boundary; reject out-of-range values.
                        let confidence = match edge.get("confidence").and_then(|v| v.as_f64()) {
                            Some(c) if !(0.0..=1.0).contains(&c) => {
                                return Err(EngineError::Extractor);
                            }
                            c => c,
                        };
                        let source_doc_id =
                            edge.get("source_doc_id").and_then(|v| v.as_str()).map(str::to_string);

                        // fix-33 [P1]: resolve each endpoint via the entities[]
                        // index (by name or alias) → the entity's canonical
                        // (name, type); fall back to kind "entity" only for a truly
                        // unlisted name (synthesized dangling endpoints ARE listed,
                        // so this is the defensive path). derive_logical_id (fix-34)
                        // still rejects an empty name / ':' in kind.
                        let (from_name, from_kind) = entity_index
                            .get(&from_entity.to_lowercase())
                            .cloned()
                            .unwrap_or_else(|| (from_entity.to_string(), "entity".to_string()));
                        let (to_name, to_kind) = entity_index
                            .get(&to_entity.to_lowercase())
                            .cloned()
                            .unwrap_or_else(|| (to_entity.to_string(), "entity".to_string()));
                        let from_lid = derive_logical_id(&from_kind, &from_name)?;
                        let to_lid = derive_logical_id(&to_kind, &to_name)?;
                        let edge_key = format!("{from_lid}:{to_lid}:{relation}");
                        let edge_lid = derive_logical_id("edge", &edge_key)?;

                        let is_temporal_fallback = t_valid
                            .as_deref()
                            .map(|tv| fallback_dates.contains(tv))
                            .unwrap_or(false);
                        Ok(PreparedWrite::Edge {
                            kind: relation.to_string(),
                            from: from_lid,
                            to: to_lid,
                            source_id: source_doc_id,
                            logical_id: Some(edge_lid),
                            body,
                            t_valid,
                            t_invalid,
                            confidence,
                            extractor_model_id: extractor_model_id.clone(),
                            temporal_fallback: if is_temporal_fallback { Some(true) } else { None },
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                // fix-34 [P2]: dedup edges by logical_id, mirroring the node arm
                // (fix-29) — a duplicate edge in one harness response would
                // otherwise write a row that immediately supersedes its sibling.
                let edge_batch = dedup_prepared_by_logical_id(edge_batch);
                let n = edge_batch.len() as u64;
                self.write(&edge_batch)?;
                edges_written = edges_written.saturating_add(n);
            }
        }

        // Drop stdin → signal EOF to subprocess; outer shell waits for it.
        drop(writer);

        Ok(IngestWithExtractorReceipt { nodes_written, edges_written, docs_processed })
    }

    pub fn search(&self, query: &str) -> Result<SearchResult, EngineError> {
        self.search_filtered(query, None)
    }

    /// G10 — hybrid `search` with an optional closed [`SearchFilter`]. `None`
    /// (or an all-`None` filter) is the unfiltered path whose phase-1 SQL is
    /// byte-identical to 0.7.2. The filter prunes the vector branch in the
    /// single phase-1 candidates statement and constrains the text branch by the
    /// same metadata. Ranking is the unconditional G9 RRF fusion.
    pub fn search_filtered(
        &self,
        query: &str,
        filter: Option<SearchFilter>,
    ) -> Result<SearchResult, EngineError> {
        // FIX-6: delegate to search_reranked(depth=0, use_graph_arm=false) to eliminate the
        // ~26-line duplicate body that would otherwise drift with search_reranked.
        self.search_reranked(query, filter, 0, false)
    }

    /// 0.8.1 Slice 10 (R1) / Slice 30 (R3) — `search_reranked`: hybrid search
    /// with optional CE reranking and optional graph-BFS third arm. `rerank_depth
    /// = 0` is the identity (soft-fallback) path, byte-identical to
    /// [`search_filtered`][Engine::search_filtered]. `rerank_depth = N > 0`
    /// applies the cross-encoder over the top-N fused hits (when the
    /// `default-reranker` feature is enabled and the model is loaded); without the
    /// model, the call falls back to the fused order.
    ///
    /// `use_graph_arm = false` (the default) produces byte-identical results to
    /// the pre-Slice-30 two-arm pipeline. `use_graph_arm = true` seeds a BFS over
    /// temporal fact-edges from the top-10 fused hits and fuses the reachable
    /// nodes as a third RRF arm.
    ///
    /// Governed surface: re-exported from `fathomdb` facade.
    pub fn search_reranked(
        &self,
        query: &str,
        filter: Option<SearchFilter>,
        rerank_depth: usize,
        use_graph_arm: bool,
    ) -> Result<SearchResult, EngineError> {
        self.emit_event(lifecycle::Phase::Started, lifecycle::EventCategory::Search, None);
        let started = Instant::now();
        let outcome = self.search_inner(query, filter, rerank_depth, use_graph_arm);
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

    /// Thin wrapper: the production search path that discards the G0 Phase-2
    /// frontier meter (it never reaches `SearchResult` / the governed surface).
    fn search_inner(
        &self,
        query: &str,
        filter: Option<SearchFilter>,
        rerank_depth: usize,
        use_graph_arm: bool,
    ) -> Result<SearchResult, EngineError> {
        self.search_inner_with_stats(query, filter, rerank_depth, use_graph_arm)
            .map(|(result, _stats)| result)
    }

    /// G0 Phase-2: the search body, additionally returning the graph-arm frontier
    /// meter. Only the `_graph_frontier_stats_for_test` seam consumes the stats;
    /// `search_inner` (and thus `search_reranked` / `search`) drops them.
    fn search_inner_with_stats(
        &self,
        query: &str,
        filter: Option<SearchFilter>,
        rerank_depth: usize,
        use_graph_arm: bool,
    ) -> Result<(SearchResult, GraphFrontierStats), EngineError> {
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
        // EU-5a2 mean-centering apply path (query side). `query_vector`
        // is ALWAYS un-centered (used by the f32 vec_distance_l2 rerank
        // in phase 2). `query_vector_bin` is the (possibly centered) f32
        // fed to `vec_quantize_binary` in phase 1. The centering decision
        // mirrors the write path: identity must be MC-required AND a
        // mean_vec must be pinned. NoopEmbedder collapses to
        // `query_vector_bin == query_vector` until EU-5b.
        let raw_query_vector =
            self.runtime_embedder.as_ref().and_then(|embedder| embedder.embed(query).ok());
        let query_vector_bin = match raw_query_vector.as_ref() {
            Some(vector) if identity_requires_mean_centering(&self.runtime_embedder_identity) => {
                let pinned = {
                    let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
                    let connection = connection.as_ref().ok_or(EngineError::Closing)?;
                    read_pinned_mean_vec(connection, self.runtime_embedder_identity.dimension)?
                };
                match pinned {
                    Some(mean) => serde_json::to_string(&subtract_mean(vector, &mean)).ok(),
                    None => serde_json::to_string(vector).ok(),
                }
            }
            Some(vector) => serde_json::to_string(vector).ok(),
            None => None,
        };
        let query_vector = raw_query_vector.and_then(|vector| serde_json::to_string(&vector).ok());
        // 0.7.2 PR-2bc S1 fix-1 — phase-2 rerank LIMIT. Production default is
        // `SEARCH_RERANK_LIMIT` (10); the test seam may RAISE it, clamped to
        // the production floor so a test can never shrink search semantics.
        let search_limit = self
            .projection_runtime
            .shared
            .search_limit_override
            .load(Ordering::SeqCst)
            .max(SEARCH_RERANK_LIMIT);
        let recency_enabled =
            self.projection_runtime.shared.recency_reweight_enabled.load(Ordering::SeqCst);
        let vector_stage_only =
            self.projection_runtime.shared.vector_stage_only_for_test.load(Ordering::SeqCst);
        let (response_tx, response_rx) = mpsc::sync_channel::<ReaderResponse>(1);
        let request = ReaderRequest::Search {
            compiled,
            query_vector,
            query_vector_bin,
            search_limit,
            filter: filter.map(Box::new),
            recency_enabled,
            vector_stage_only,
            raw_query: Box::from(query), // FIX-4: Box<str> (16B) not String (24B)
            rerank_depth,
            use_graph_arm,
            respond: response_tx,
        };
        if self.reader_pool.dispatch(request).is_err() {
            return Err(EngineError::Closing);
        }
        let search_result = response_rx.recv().map_err(|_| EngineError::Storage)?;
        let (cursor, soft_fallback, results, graph_stats) = match search_result {
            Ok(result) => result,
            Err(err) => {
                self.emit_sqlite_internal_error(&err);
                return Err(EngineError::Storage);
            }
        };

        Ok((SearchResult { projection_cursor: cursor, soft_fallback, results }, graph_stats))
    }

    /// G0 Phase-2 (BLOCK-1) test seam — runs the graph-arm retrieval path and
    /// returns the frontier meter (`GraphFrontierStats`) for `query`. Mirrors the
    /// sanctioned `set_vector_stage_only_for_test` / `_configure_vector_kind_for_test`
    /// pattern: kept OFF the governed surface (test/eval-only), so the meter never
    /// appears on `SearchResult`. Used by the recall harness to prove the
    /// doc-seeded frontier is empty (`resolved_seed_rate == 0.0`) and, post-C1, the
    /// 0→>0 flip.
    pub fn _graph_frontier_stats_for_test(
        &self,
        query: &str,
    ) -> Result<GraphFrontierStats, EngineError> {
        self.search_inner_with_stats(query, None, 0, true).map(|(_result, stats)| stats)
    }

    /// Slice 30 (G2) — `read.get`: active-only point lookup by `logical_id`.
    /// Delegates to [`Engine::read_get_many`]; returns the single slot. A
    /// missing/superseded id is `None` (a normal absence, not an error). Reads
    /// ride the ReaderWorkerPool DEFERRED-tx path (never the writer lock).
    pub fn read_get(&self, logical_id: &str) -> Result<Option<NodeRecord>, EngineError> {
        let ids = [logical_id.to_string()];
        let rows = self.read_get_many(&ids)?;
        Ok(rows.into_iter().next().flatten())
    }

    /// Slice 30 (G2) — `read.get_many`: active-only point lookup over many
    /// `logical_id`s. Returns one slot per requested id in REQUEST ORDER, `None`
    /// where no active row carries that id (partial, never all-or-nothing).
    pub fn read_get_many(
        &self,
        logical_ids: &[String],
    ) -> Result<Vec<Option<NodeRecord>>, EngineError> {
        self.ensure_open()?;
        if logical_ids.is_empty() {
            return Ok(Vec::new());
        }
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        let request =
            ReaderRequest::GetById { logical_ids: logical_ids.to_vec(), respond: response_tx };
        if self.reader_pool.dispatch(request).is_err() {
            return Err(EngineError::Closing);
        }
        match response_rx.recv().map_err(|_| EngineError::Storage)? {
            Ok(rows) => Ok(rows),
            Err(err) => {
                self.emit_sqlite_internal_error(&err);
                Err(EngineError::Storage)
            }
        }
    }

    /// Slice 20 (G5) — `read.neighbors`: bounded BFS from `root_logical_id`
    /// over `canonical_edges`. Returns nodes reachable within `depth` hops
    /// (`1..=3`) in the given `direction`, excluding the root itself.
    ///
    /// Hard cap: 50 results (engine-enforced `LIMIT 50`).
    /// Traversal filter: `superseded_at IS NULL AND (t_invalid IS NULL OR t_invalid > now)`.
    ///
    /// Returns `Err(EngineError::InvalidArgument)` for `depth > 3`.
    /// Returns `Ok(vec![])` for an unknown/superseded root.
    /// Reads ride the `ReaderWorkerPool` DEFERRED-tx path.
    pub fn graph_neighbors(
        &self,
        root_logical_id: &str,
        depth: u32,
        direction: TraversalDirection,
    ) -> Result<Vec<NodeRecord>, EngineError> {
        self.ensure_open()?;
        if depth == 0 || depth > 3 {
            return Err(EngineError::InvalidArgument {
                msg: format!("traversal depth {depth} is out of range; must be 1, 2, or 3"),
            });
        }
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        let request = ReaderRequest::GraphNeighbors {
            root_logical_id: root_logical_id.to_string(),
            depth,
            direction,
            respond: response_tx,
        };
        if self.reader_pool.dispatch(request).is_err() {
            return Err(EngineError::Closing);
        }
        match response_rx.recv().map_err(|_| EngineError::Storage)? {
            Ok(nodes) => Ok(nodes),
            Err(err) => {
                self.emit_sqlite_internal_error(&err);
                Err(EngineError::Storage)
            }
        }
    }

    /// Slice 20 (G6) — `search_expand`: hybrid search (`G1+G9`) followed by
    /// bounded BFS expansion (`G5`) of each search hit. Returns the original
    /// search hits (with RRF scores) plus nodes reachable from any hit via
    /// up to `depth` hops that are NOT already in the search hit set.
    ///
    /// Returns `Err(EngineError::InvalidArgument)` for `depth > 3`.
    /// A `depth = 0` call returns search hits with their logical_ids resolved
    /// but no BFS expansion. Reads ride the `ReaderWorkerPool` DEFERRED-tx path.
    ///
    /// **Snapshot note:** the search phase (`search_inner`) and the expansion
    /// phase (`SearchExpand` reader request) run in separate DEFERRED reader
    /// transactions; a write that lands between them is visible to expansion
    /// but not search (or vice-versa). In practice the window is negligible for
    /// single-process embedded use. The expansion phase mitigates drift by
    /// filtering `search_hits` to only include hits whose `write_cursor` is
    /// still active in the expansion snapshot (superseded hits are dropped from
    /// the result rather than surfaced with stale data).
    pub fn search_expand(
        &self,
        query: &str,
        filter: Option<SearchFilter>,
        depth: u32,
    ) -> Result<SearchExpandResult, EngineError> {
        self.ensure_open()?;
        if depth > 3 {
            return Err(EngineError::InvalidArgument {
                msg: format!("traversal depth {depth} exceeds the SDK ceiling of 3"),
            });
        }
        // Step 1: run the hybrid search to get initial hits (no CE reranking in expand).
        let search_result = self.search_inner(query, filter, 0, false)?;
        if search_result.results.is_empty() {
            return Ok(SearchExpandResult {
                search_hits: Vec::new(),
                expanded: Vec::new(),
                all_logical_ids: Vec::new(),
            });
        }
        // Step 2: dispatch to the reader pool to resolve logical_ids and run BFS.
        // depth=0 is forwarded to the reader so it can populate all_logical_ids
        // (the union of search-hit logical_ids), even with no expansion.
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        let request = ReaderRequest::SearchExpand {
            search_hits: search_result.results,
            depth,
            respond: response_tx,
        };
        if self.reader_pool.dispatch(request).is_err() {
            return Err(EngineError::Closing);
        }
        match response_rx.recv().map_err(|_| EngineError::Storage)? {
            Ok(result) => Ok(result),
            Err(err) => {
                self.emit_sqlite_internal_error(&err);
                Err(EngineError::Storage)
            }
        }
    }

    /// Slice 20 test seam — run `EXPLAIN QUERY PLAN` on the BFS CTE SQL and
    /// return the plan detail lines. Used by `explain_plan_uses_indexes`.
    #[doc(hidden)]
    pub fn explain_graph_neighbors_for_test(
        &self,
        root_logical_id: &str,
        depth: u32,
        direction: TraversalDirection,
    ) -> Result<Vec<String>, EngineError> {
        self.ensure_open()?;
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        let request = ReaderRequest::ExplainGraphNeighbors {
            root_logical_id: root_logical_id.to_string(),
            depth,
            direction,
            respond: response_tx,
        };
        if self.reader_pool.dispatch(request).is_err() {
            return Err(EngineError::Closing);
        }
        match response_rx.recv().map_err(|_| EngineError::Storage)? {
            Ok(plan) => Ok(plan),
            Err(err) => {
                self.emit_sqlite_internal_error(&err);
                Err(EngineError::Storage)
            }
        }
    }

    /// Slice 30 (G3) — `read.collection`: paginated op-store read-back over
    /// `operational_mutations` for `collection`, `ORDER BY id`. `limit` is
    /// MANDATORY (clamped to the ~1M cap); `after_id` is the exclusive cursor.
    /// Reads ride the ReaderWorkerPool DEFERRED-tx path.
    pub fn read_collection(
        &self,
        collection: &str,
        after_id: Option<i64>,
        limit: usize,
    ) -> Result<Vec<OpStoreRow>, EngineError> {
        self.read_collection_dispatch(collection, after_id, limit)
    }

    /// Slice 30 (G3) — `read.mutations`: the mutation-log-oriented alias surface
    /// over the SAME op-store read-back as [`Engine::read_collection`].
    pub fn read_mutations(
        &self,
        collection: &str,
        after_id: Option<i64>,
        limit: usize,
    ) -> Result<Vec<OpStoreRow>, EngineError> {
        self.read_collection_dispatch(collection, after_id, limit)
    }

    fn read_collection_dispatch(
        &self,
        collection: &str,
        after_id: Option<i64>,
        limit: usize,
    ) -> Result<Vec<OpStoreRow>, EngineError> {
        self.ensure_open()?;
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        let request = ReaderRequest::ReadCollection {
            collection: collection.to_string(),
            after_id,
            limit,
            respond: response_tx,
        };
        if self.reader_pool.dispatch(request).is_err() {
            return Err(EngineError::Closing);
        }
        match response_rx.recv().map_err(|_| EngineError::Storage)? {
            Ok(rows) => Ok(rows),
            Err(err) => {
                self.emit_sqlite_internal_error(&err);
                Err(EngineError::Storage)
            }
        }
    }

    /// Slice 35 (G4) — `read.list`: list active `canonical_nodes` of a given
    /// `kind`, optionally filtered by a closed [`Predicate`] set, up to `limit`
    /// rows. Returns `Vec<NodeRecord>` (active only; `superseded_at IS NULL`).
    ///
    /// Multiple predicates are combined as AND (D-F5). An empty predicate slice
    /// returns all active nodes of the given kind up to `limit` (unfiltered path).
    /// Compilation target: `json_extract(body, '$.field') <op> ?` with bound
    /// parameters (injection-safe per D-F4). See `dev/adr/ADR-0.8.0-filter-grammar.md`.
    ///
    /// Path validation happens at [`Predicate`] construction time; `read_list`
    /// revalidates as defense-in-depth (enum variants are `pub`, so direct
    /// struct-literal construction could bypass the constructors).
    pub fn read_list(
        &self,
        kind: &str,
        predicates: &[Predicate],
        limit: usize,
    ) -> Result<Vec<NodeRecord>, EngineError> {
        self.ensure_open()?;
        // Defense-in-depth: revalidate paths even if the caller bypassed the
        // validated constructors by constructing enum variants directly.
        for pred in predicates {
            let path = pred.path();
            if !PREDICATE_PATH_ALLOWLIST.contains(&path) {
                return Err(EngineError::InvalidFilter {
                    reason: format!("path '{path}' is not in the predicate path allowlist"),
                });
            }
        }
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        let request = ReaderRequest::ReadList {
            kind: kind.to_string(),
            predicates: predicates.to_vec(),
            limit,
            respond: response_tx,
        };
        if self.reader_pool.dispatch(request).is_err() {
            return Err(EngineError::Closing);
        }
        match response_rx.recv().map_err(|_| EngineError::Storage)? {
            Ok(rows) => Ok(rows),
            Err(err) => {
                self.emit_sqlite_internal_error(&err);
                Err(EngineError::Storage)
            }
        }
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

    /// One-thread-poison robustness fixture (AC-009).
    ///
    /// Spawns four reader threads + one writer thread that all make
    /// forward progress (single canonical write + repeated searches),
    /// plus one designated poison thread that runs an empty-batch write
    /// — a deterministic `EngineError::WriteValidation`. The captured
    /// poison failure is dispatched as a `StressFailureContext` whose
    /// `last_error_chain` is `[EngineError::stable_code(),
    /// engine_error.to_string()]` per the lifecycle § Stress-failure
    /// context payload contract.
    #[doc(hidden)]
    #[cfg(debug_assertions)]
    pub fn run_one_thread_poison_for_test(&self) -> Result<(), EngineError> {
        self.ensure_open()?;

        // Forward-progress writer seeds a row so readers + the poison
        // thread share a non-trivial canonical state.
        self.write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "poison-fixture-seed".to_string(),
            source_id: None,
            logical_id: None,
        }])?;

        let poison_outcome: Mutex<Option<EngineError>> = Mutex::new(None);
        let poison_thread_id: AtomicU64 = AtomicU64::new(0);

        thread::scope(|scope| {
            // N=4 reader threads make forward progress.
            for _ in 0..4 {
                scope.spawn(|| {
                    for _ in 0..4 {
                        let _ = self.search("poison-fixture-seed");
                    }
                });
            }
            // One forward-progress writer thread.
            scope.spawn(|| {
                let _ = self.write(&[PreparedWrite::Node {
                    kind: "doc".to_string(),
                    body: "writer-progress".to_string(),
                    source_id: None,
                    logical_id: None,
                }]);
            });
            // One poison thread — empty batch is a deterministic
            // WriteValidation failure.
            scope.spawn(|| {
                // Use a non-zero, deterministic group id so subscribers
                // see a stable identifier across runs of the fixture.
                poison_thread_id.store(1, Ordering::SeqCst);
                if let Err(err) = self.write(&[]) {
                    *poison_outcome.lock().expect("poison_outcome lock") = Some(err);
                }
            });
        });

        let err = poison_outcome
            .into_inner()
            .expect("poison_outcome lock")
            .expect("poison thread must produce a deterministic error");

        let projection_state = match self.projection_status_for_test("doc") {
            Ok(lifecycle::ProjectionStatus::Pending) => "Pending",
            Ok(lifecycle::ProjectionStatus::Failed) => "Failed",
            Ok(lifecycle::ProjectionStatus::UpToDate) => "UpToDate",
            // Default to UpToDate when projection status is unobservable
            // (e.g. embedder not configured for the seed kind). The
            // value is still one of the documented enum stringifications
            // per AC-010.
            Err(_) => "UpToDate",
        };

        let context = lifecycle::StressFailureContext {
            thread_group_id: poison_thread_id.load(Ordering::SeqCst),
            op_kind: "write".to_string(),
            last_error_chain: vec![err.stable_code().to_string(), err.to_string()],
            projection_state: projection_state.to_string(),
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

    /// PR-9 — lower the ADR-0.6.0 Invariant 5 per-`embed()` watchdog deadline
    /// for tests (production default is `DEFAULT_EMBED_TIMEOUT_MS` = 30s).
    #[doc(hidden)]
    pub fn set_embed_timeout_ms_for_test(&self, timeout_ms: u64) {
        self.projection_runtime.set_embed_timeout_ms_for_test(timeout_ms);
    }

    /// PR-9 — lower the embed circuit-breaker threshold for tests (production
    /// default `DEFAULT_EMBED_CIRCUIT_THRESHOLD`); 0 disables the breaker.
    #[doc(hidden)]
    pub fn set_embed_circuit_threshold_for_test(&self, threshold: u64) {
        self.projection_runtime.set_embed_circuit_threshold_for_test(threshold);
    }

    /// PR-9 — whether the embed circuit breaker has latched open.
    #[doc(hidden)]
    pub fn embed_circuit_open_for_test(&self) -> bool {
        self.projection_runtime.embed_circuit_open_for_test()
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
        // EU-5a2 mean-centering apply path (write side). f32 BLOB stored
        // is ALWAYS un-centered; the sign-quant input is the centered
        // vector iff the identity is MC-required AND a `mean_vec` is
        // pinned. NoopEmbedder identity (the only EU-5a2 live one) is
        // NOT MC-required, so this is a no-op until EU-5b's flip.
        let blob = encode_vector_blob(&vector);
        let bin_blob = if identity_requires_mean_centering(&self.runtime_embedder_identity) {
            match read_pinned_mean_vec(connection, self.runtime_embedder_identity.dimension)? {
                Some(mean) => encode_vector_blob(&subtract_mean(&vector, &mean)),
                None => blob.clone(),
            }
        } else {
            blob.clone()
        };
        let source_type = resolve_source_type(kind)?;
        let now_unix =
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;

        // EU-5b — feed the streaming mean accumulator (if live) and detect
        // a threshold-crossing pin. The mean materialization, pre-pin
        // re-quantize, and `MeanVecPinned` event emission all happen in
        // the SAME SQLite transaction as the row INSERT.
        let pin_event = {
            let runtime = &self.projection_runtime.shared;
            let mut accumulator =
                runtime.mean_accumulator.lock().map_err(|_| EngineError::Storage)?;
            if let Some(acc) = accumulator.as_mut() {
                acc.add(&vector);
                if acc.count() >= MEAN_VEC_PIN_THRESHOLD {
                    let mean = acc.materialize();
                    *accumulator = None;
                    Some(mean)
                } else {
                    None
                }
            } else {
                None
            }
        };

        let tx = connection.transaction().map_err(|_| EngineError::Storage)?;
        tx.execute(
            "INSERT INTO _fathomdb_vector_rows(rowid, kind, write_cursor) VALUES(?1, ?2, ?3)",
            params![cursor, kind, cursor],
        )
        .map_err(|_| EngineError::Storage)?;
        tx.execute(
            // Slice 10 / G10 — `status` ships an empty-string sentinel only:
            // vec0 TEXT metadata columns are NOT NULL-able ("Expected text for
            // TEXT metadata column"), so the "no real population yet" state is
            // `''`, not NULL (deviation from the prompt's "NULL plumbing" wording,
            // forced by vec0; reserved-gap candidate 13 is the real source).
            "INSERT INTO vector_default(
                rowid, embedding, embedding_bin, source_type, kind, created_at, status
             ) VALUES(?1, ?2, vec_quantize_binary(?3), ?4, ?5, ?6, '')",
            params![cursor, blob, bin_blob, source_type, kind, now_unix],
        )
        .map_err(|_| EngineError::Storage)?;

        let mut emitted_event: Option<EmbedderEvent> = None;
        if let Some(mean_vec) = pin_event {
            let mean_bytes = encode_vector_blob(&mean_vec);
            tx.execute(
                "UPDATE _fathomdb_embedder_profiles SET mean_vec = ?1 WHERE profile = 'default'",
                params![mean_bytes],
            )
            .map_err(|_| EngineError::Storage)?;
            // Read all pre-pin (rowid, embedding) and re-quantize within
            // the same tx. The just-inserted row above is also covered.
            let rows: Vec<(i64, Vec<u8>)> = {
                let mut statement = tx
                    .prepare("SELECT rowid, embedding FROM vector_default ORDER BY rowid")
                    .map_err(|_| EngineError::Storage)?;
                let mapped = statement
                    .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)))
                    .map_err(|_| EngineError::Storage)?;
                let mut out = Vec::new();
                for r in mapped {
                    out.push(r.map_err(|_| EngineError::Storage)?);
                }
                out
            };
            let (doc_count, _) = run_pin_and_requantize_pass(&tx, &rows, &mean_vec)?;
            emitted_event = Some(EmbedderEvent::MeanVecPinned {
                dim: u32::try_from(mean_vec.len()).unwrap_or(u32::MAX),
                doc_count,
            });
        }

        tx.commit().map_err(|_| EngineError::Storage)?;

        if let Some(ev) = emitted_event {
            if let Ok(mut events) = self.projection_runtime.shared.pending_events.lock() {
                events.push(ev);
            }
        }

        self.next_cursor.store(cursor, Ordering::SeqCst);
        // G8 — this path (embedder-profile pin) commits no canonical edges, so
        // no endpoint can dangle.
        Ok(WriteReceipt { cursor, row_cursors: vec![cursor], dangling_edge_endpoints: 0 })
    }

    /// EU-5b test seam — drain MeanVecPinned events queued by the
    /// projection-commit pin transaction since the last drain. Production
    /// callers consume these via `OpenReport.embedder_events`; this seam
    /// exists so the EU-5b RED test can observe the live emission.
    #[doc(hidden)]
    pub fn drain_mean_centering_events_for_test(&self) -> Result<Vec<EmbedderEvent>, EngineError> {
        self.ensure_open()?;
        let mut events = self
            .projection_runtime
            .shared
            .pending_events
            .lock()
            .map_err(|_| EngineError::Storage)?;
        let out = std::mem::take(&mut *events);
        Ok(out)
    }

    /// 0.7.2 PR-2b — NON-test observation seam. Drains and returns every
    /// `EmbedderEvent` queued since the last drain (mean pin, manual mean
    /// recompute). Production callers use
    /// this to observe the synchronous recompute work; events are queued
    /// only AFTER the recompute transaction is durable, so a rolled-back
    /// recompute never surfaces. Mirrors the at-open
    /// `OpenReport.embedder_events` channel for the steady-state path.
    pub fn drain_embedder_events(&self) -> Result<Vec<EmbedderEvent>, EngineError> {
        self.ensure_open()?;
        let mut events = self
            .projection_runtime
            .shared
            .pending_events
            .lock()
            .map_err(|_| EngineError::Storage)?;
        Ok(std::mem::take(&mut *events))
    }

    /// 0.7.2 PR-2b — explicit `doctor recompute-mean` path. Re-derives the
    /// pinned corpus mean from the current `vector_default` rows and
    /// re-quantizes every row, SYNCHRONOUSLY in one transaction. ALWAYS
    /// allowed at any corpus size — this is the ONLY mean-refresh path as of
    /// 0.7.2 (the automatic in-ingest drift detector was carved out / deferred
    /// to 0.8.x; see `dev/design/embedder.md` §0.3).
    ///
    /// Serializes against the projection workers via `commit_gate` so the
    /// re-quantize sees a totally-ordered history, exactly like the at-pin
    /// commit. Publishes a `MeanVecRecomputed { trigger: Manual }` event
    /// only after the transaction is durable. No-op-safe on a non-MC
    /// identity (returns `EmbedderNotConfigured` rather than corrupting an
    /// un-centered workspace).
    #[cfg(feature = "operator")]
    pub fn recompute_mean(&self) -> Result<MeanRecomputeReport, EngineError> {
        self.ensure_open()?;
        let identity = self.runtime_embedder_identity.clone();
        if !identity_requires_mean_centering(&identity) {
            return Err(EngineError::EmbedderNotConfigured);
        }
        let report = {
            // Hold the commit gate for the whole recompute so no projection
            // worker commit interleaves with the re-quantize.
            let _gate = self
                .projection_runtime
                .shared
                .commit_gate
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let mut connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
            let connection = connection.as_mut().ok_or(EngineError::Closing)?;
            let tx = connection.transaction().map_err(|_| EngineError::Storage)?;
            #[cfg(debug_assertions)]
            let fail = self
                .projection_runtime
                .shared
                .force_recompute_failure
                .swap(false, Ordering::SeqCst);
            #[cfg(not(debug_assertions))]
            let fail = false;
            let report = recompute_mean_in_tx_inner(&tx, &identity, fail)?;
            tx.commit().map_err(|_| EngineError::Storage)?;
            report
        };
        // Post-durable-commit publish.
        if let Ok(mut events) = self.projection_runtime.shared.pending_events.lock() {
            events.push(EmbedderEvent::MeanVecRecomputed {
                dim: report.dim,
                doc_count: report.doc_count_requantized,
                trigger: MeanRecomputeTrigger::Manual,
            });
        }
        Ok(report)
    }

    /// 0.7.2 PR-2bc S1 fix-1 test seam — RAISE the phase-2 rerank `LIMIT`
    /// above the production `SEARCH_RERANK_LIMIT` (10) so the recall harness
    /// can pull top-(10+slack) and exclude the self-retrieving query-source
    /// doc before truncating to 10. The search path clamps the stored value
    /// to the production floor, so a test can never shrink search fanout
    /// below production semantics. Production reads the same atomic and never
    /// consults any env var.
    #[doc(hidden)]
    pub fn set_search_limit_for_test(&self, limit: usize) {
        self.projection_runtime.shared.search_limit_override.store(limit, Ordering::SeqCst);
    }

    /// Slice 10 / G12-recency test seam — flip the dedicated recency-reweight
    /// flag (off by default). The reweight runs AFTER bit-KNN on the fused hits;
    /// it is never a vec0 predicate and is NOT `fusion_mode`.
    #[doc(hidden)]
    pub fn set_recency_reweight_enabled_for_test(&self, enabled: bool) {
        self.projection_runtime.shared.recency_reweight_enabled.store(enabled, Ordering::SeqCst);
    }

    /// GA-2 / Slice-40 (◆ B-1) measurement seam — make `search()` return the
    /// pre-fusion VECTOR-branch ranking (the ANN+ bit-KNN K=192 + f32 rerank
    /// signal) instead of the unconditional RRF-fused result, so the eu7 recall
    /// gate (AC-075) can measure ANN-quantization FIDELITY — vector top-10 vs
    /// the exact-f32 VECTOR top-10 ground truth — in isolation. Off by default;
    /// never set on any production path. This is NOT a `fusion_mode` knob:
    /// production RRF fusion stays unconditional and `fuse_rrf`/`rerank_fused`/
    /// recency are unchanged. Mirrors `set_recency_reweight_enabled_for_test`
    /// (release-available, since eu7 runs in `--release`).
    #[doc(hidden)]
    pub fn set_vector_stage_only_for_test(&self, enabled: bool) {
        self.projection_runtime.shared.vector_stage_only_for_test.store(enabled, Ordering::SeqCst);
    }

    /// 0.7.2 PR-2b test seam — arm a one-shot fault inside the NEXT
    /// `recompute_mean` so it errors after the `mean_vec` UPDATE but before
    /// the re-quantize completes. Proves the recompute tx rolls back whole.
    #[doc(hidden)]
    #[cfg(debug_assertions)]
    pub fn force_next_recompute_failure_for_test(&self) {
        self.projection_runtime.shared.force_recompute_failure.store(true, Ordering::SeqCst);
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

    /// Doctor read-only integrity report. Three-section output per
    /// AC-043a/b. `opts.full` adds `PRAGMA integrity_check`. `quick` and
    /// `round_trip` are accepted but treated as default for 0.6.0.
    #[cfg(feature = "operator")]
    pub fn check_integrity(
        &self,
        opts: CheckIntegrityOpts,
    ) -> Result<IntegrityReport, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        Ok(IntegrityReport {
            physical: physical_section(connection, opts.full),
            logical: logical_section(connection),
            semantic: semantic_section(connection),
        })
    }

    /// Doctor bit-preserving export. Runs `VACUUM INTO` to produce a
    /// self-contained SQLite file at `out`, computes SHA-256 of the
    /// resulting bytes, and writes a JSON manifest at `manifest`. Per
    /// AC-039a/b.
    #[cfg(feature = "operator")]
    pub fn safe_export(
        &self,
        out: &Path,
        manifest: &Path,
    ) -> Result<SafeExportArtifact, EngineError> {
        self.ensure_open()?;
        {
            let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
            let connection = connection.as_ref().ok_or(EngineError::Closing)?;
            let target = out.to_string_lossy().to_string();
            connection
                .execute("VACUUM INTO ?1", params![target])
                .map_err(|_| EngineError::Storage)?;
        }
        let bytes = std::fs::read(out).map_err(|_| EngineError::Storage)?;
        let digest = sha2::Sha256::digest(&bytes);
        let sha256_hex = hex_encode(digest.as_slice());
        let export_abs = out.canonicalize().unwrap_or_else(|_| out.to_path_buf());
        let manifest_json = serde_json::json!({
            "export_path": export_abs.to_string_lossy(),
            "sha256": sha256_hex,
            "byte_count": bytes.len() as u64,
        });
        let manifest_bytes =
            serde_json::to_vec_pretty(&manifest_json).map_err(|_| EngineError::Storage)?;
        std::fs::write(manifest, &manifest_bytes).map_err(|_| EngineError::Storage)?;
        Ok(SafeExportArtifact {
            export_path: out.to_path_buf(),
            manifest_path: manifest.to_path_buf(),
            manifest_sha256: sha256_hex,
        })
    }

    /// Operator regenerate workflow per `dev/design/projections.md`
    /// § Regenerate workflow. Drains in-flight projection work, then
    /// truncates FTS5 + vec0 shadow rows, resets the projection cursor,
    /// and lets the scheduler re-enqueue every canonical row. Durable
    /// `projection_failures` audit rows are preserved per design. AC-044
    /// + AC-063c.
    #[cfg(feature = "operator")]
    pub fn rebuild_projections(&self) -> Result<RebuildReport, EngineError> {
        self.ensure_open()?;
        self.run_rebuild(true, RebuildKind::Projections)
    }

    /// Vec0-only variant of [`Engine::rebuild_projections`]. Leaves
    /// FTS5 shadow content untouched; per recovery design,
    /// `recover --rebuild-vec0` is the surface for vec0-only repair.
    #[cfg(feature = "operator")]
    pub fn rebuild_vec0(&self) -> Result<RebuildReport, EngineError> {
        self.ensure_open()?;
        self.run_rebuild(false, RebuildKind::Vec0)
    }

    /// Phase 9 Pack B / AC-042 source trace. Returns the canonical-row
    /// id set produced by `source_id`, ordered by `write_cursor`. Empty
    /// string is not a valid `source_id`; rows with NULL `source_id`
    /// are excluded from every result.
    #[cfg(feature = "operator")]
    pub fn trace_source_ref(&self, source_id: &str) -> Result<TraceReport, EngineError> {
        self.ensure_open()?;
        if source_id.is_empty() {
            return Err(EngineError::WriteValidation);
        }
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;

        let mut events: Vec<TraceEvent> = Vec::new();
        let mut nodes = connection
            .prepare(
                "SELECT write_cursor, kind FROM canonical_nodes WHERE source_id = ?1
                 ORDER BY write_cursor",
            )
            .map_err(|_| EngineError::Storage)?;
        let node_rows = nodes
            .query_map([source_id], |row| {
                Ok(TraceEvent {
                    write_cursor: row.get::<_, i64>(0)? as u64,
                    kind: row.get::<_, String>(1)?,
                    table: "canonical_nodes",
                })
            })
            .map_err(|_| EngineError::Storage)?;
        for row in node_rows {
            events.push(row.map_err(|_| EngineError::Storage)?);
        }

        let mut edges = connection
            .prepare(
                "SELECT write_cursor, kind FROM canonical_edges WHERE source_id = ?1
                 ORDER BY write_cursor",
            )
            .map_err(|_| EngineError::Storage)?;
        let edge_rows = edges
            .query_map([source_id], |row| {
                Ok(TraceEvent {
                    write_cursor: row.get::<_, i64>(0)? as u64,
                    kind: row.get::<_, String>(1)?,
                    table: "canonical_edges",
                })
            })
            .map_err(|_| EngineError::Storage)?;
        for row in edge_rows {
            events.push(row.map_err(|_| EngineError::Storage)?);
        }

        events.sort_by_key(|e| e.write_cursor);
        Ok(TraceReport { source_ref: source_id.to_string(), events })
    }

    /// Phase 9 Pack B / AC-028a/b/c source excise. Drains in-flight
    /// projection work, then deletes every canonical row attributable
    /// to `source_id` plus the FTS5 + vec0 shadow rows that referenced
    /// those cursors, and appends an audit row to the
    /// `excise_source_audit` operational collection.
    ///
    /// Non-perturbation: rows from other sources (and rows with NULL
    /// `source_id`) are untouched; the projection cursor is NOT reset
    /// and no blanket projection rebuild is issued.
    #[cfg(feature = "operator")]
    pub fn excise_source(&self, source_id: &str) -> Result<ExciseReport, EngineError> {
        self.ensure_open()?;
        if source_id.is_empty() {
            return Err(EngineError::WriteValidation);
        }

        // Drain MUST succeed before the excise transaction. SQLite-WAL
        // would otherwise allow a worker that already dequeued a job
        // for an excised cursor to commit its INSERT into vec0 /
        // _fathomdb_vector_rows after our DELETE releases the writer
        // lock, leaving residue and breaking AC-028b. Surface the
        // timeout instead of swallowing it (Pack A pattern).
        self.projection_runtime.set_frozen(true);
        let drain_result = self.drain(REBUILD_DRAIN_TIMEOUT_MS);
        let outcome = drain_result.and_then(|()| self.excise_source_inner(source_id));
        self.projection_runtime.set_frozen(false);
        outcome
    }

    /// Doctor `verify-embedder` seam (AC-040a). Compares the
    /// `_fathomdb_embedder_profiles` row to the operator-supplied
    /// `name:revision` identity + dimension; never raises on mismatch.
    #[cfg(feature = "operator")]
    pub fn verify_embedder(
        &self,
        supplied_identity: &str,
        supplied_dimension: u32,
    ) -> Result<VerifyEmbedderReport, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        let stored = load_default_profile(connection).map_err(|_| EngineError::Storage)?;
        let stored_identity = format!("{}:{}", stored.name, stored.revision);
        let identity_match = stored_identity == supplied_identity;
        let dimension_match = stored.dimension == supplied_dimension;
        let status = match (identity_match, dimension_match) {
            (true, true) => VerifyEmbedderStatus::Match,
            (false, true) => VerifyEmbedderStatus::IdentityMismatch,
            (true, false) => VerifyEmbedderStatus::DimensionMismatch,
            (false, false) => VerifyEmbedderStatus::BothMismatch,
        };
        Ok(VerifyEmbedderReport {
            stored_identity,
            stored_dimension: stored.dimension,
            supplied_identity: supplied_identity.to_string(),
            supplied_dimension,
            status,
        })
    }

    /// Doctor `dump-schema` seam (AC-040a). Returns the
    /// `PRAGMA user_version` sentinel plus the table + index inventory
    /// from `sqlite_schema`, excluding `sqlite_*` internal rows.
    /// Canonical tables appear first per [`CANONICAL_TABLES`].
    #[cfg(feature = "operator")]
    pub fn dump_schema(&self) -> Result<DumpSchemaReport, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        let user_version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|_| EngineError::Storage)?;
        let tables = read_schema_objects(connection, "table")?;
        let indexes = read_schema_objects(connection, "index")?;
        Ok(DumpSchemaReport { user_version, tables: order_canonical_first(tables), indexes })
    }

    /// Doctor `dump-row-counts` seam (AC-040a). Emits canonical-table
    /// counts only; projection / FTS / vec0 shadow tables are excluded.
    #[cfg(feature = "operator")]
    pub fn dump_row_counts(&self) -> Result<DumpRowCountsReport, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        let mut counts = Vec::with_capacity(CANONICAL_TABLES.len());
        for name in CANONICAL_TABLES {
            let rows: u64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |row| row.get(0))
                .map_err(|_| EngineError::Storage)?;
            counts.push(TableRowCount { name: (*name).to_string(), rows });
        }
        Ok(DumpRowCountsReport { counts })
    }

    /// Doctor `dump-profile` seam (AC-040a). Returns the stored
    /// embedder identity + dimension plus the registered vectorized
    /// kinds from `_fathomdb_vector_kinds`.
    #[cfg(feature = "operator")]
    pub fn dump_profile(&self) -> Result<DumpProfileReport, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        let stored = load_default_profile(connection).map_err(|_| EngineError::Storage)?;
        let mut stmt = connection
            .prepare("SELECT kind FROM _fathomdb_vector_kinds ORDER BY kind")
            .map_err(|_| EngineError::Storage)?;
        let rows =
            stmt.query_map([], |row| row.get::<_, String>(0)).map_err(|_| EngineError::Storage)?;
        let mut vectorized_kinds = Vec::new();
        for row in rows {
            vectorized_kinds.push(row.map_err(|_| EngineError::Storage)?);
        }
        Ok(DumpProfileReport {
            embedder_identity: format!("{}:{}", stored.name, stored.revision),
            embedder_dimension: stored.dimension,
            vectorized_kinds,
        })
    }

    /// Recover `--truncate-wal` seam. Runs
    /// `PRAGMA wal_checkpoint(TRUNCATE)` and returns the three counters
    /// SQLite reports. `status = Busy` when SQLite signalled a blocked
    /// checkpoint (`busy != 0`); the WAL may still be partially
    /// checkpointed in that case.
    #[cfg(feature = "operator")]
    pub fn truncate_wal(&self) -> Result<TruncateWalReport, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        let (busy, log_frames, checkpointed_frames): (i64, i64, i64) = connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|_| EngineError::Storage)?;
        let status = if busy == 0 { TruncateWalStatus::Done } else { TruncateWalStatus::Busy };
        Ok(TruncateWalReport {
            status,
            busy: busy.max(0) as u32,
            log_frames: log_frames.max(0) as u32,
            checkpointed_frames: checkpointed_frames.max(0) as u32,
        })
    }

    #[cfg(feature = "operator")]
    fn excise_source_inner(&self, source_id: &str) -> Result<ExciseReport, EngineError> {
        let mut connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_mut().ok_or(EngineError::Closing)?;
        let tx = connection.transaction().map_err(|_| EngineError::Storage)?;

        // Collect the cursor sets up-front so we can targeted-delete
        // shadow rows AND emit an accurate audit row in one txn.
        let node_cursors: Vec<i64> = {
            let mut stmt = tx
                .prepare("SELECT write_cursor FROM canonical_nodes WHERE source_id = ?1")
                .map_err(|_| EngineError::Storage)?;
            let rows = stmt
                .query_map([source_id], |row| row.get::<_, i64>(0))
                .map_err(|_| EngineError::Storage)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|_| EngineError::Storage)?
        };
        let edge_cursors: Vec<i64> = {
            let mut stmt = tx
                .prepare("SELECT write_cursor FROM canonical_edges WHERE source_id = ?1")
                .map_err(|_| EngineError::Storage)?;
            let rows = stmt
                .query_map([source_id], |row| row.get::<_, i64>(0))
                .map_err(|_| EngineError::Storage)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|_| EngineError::Storage)?
        };

        let mut shadow_invalidated: u64 = 0;
        for cursor in node_cursors.iter().chain(edge_cursors.iter()) {
            shadow_invalidated = shadow_invalidated.saturating_add(
                tx.execute("DELETE FROM search_index WHERE write_cursor = ?1", [cursor])
                    .map_err(|_| EngineError::Storage)? as u64,
            );
            // fix-26 [P2]: also excise edge FTS rows (G11 search_index_edges).
            shadow_invalidated = shadow_invalidated.saturating_add(
                tx.execute("DELETE FROM search_index_edges WHERE write_cursor = ?1", [cursor])
                    .map_err(|_| EngineError::Storage)? as u64,
            );
            // vec0 rowid is the canonical row's write_cursor (see
            // `_fathomdb_vector_rows.write_cursor UNIQUE`).
            shadow_invalidated = shadow_invalidated.saturating_add(
                tx.execute("DELETE FROM vector_default WHERE rowid = ?1", [cursor])
                    .map_err(|_| EngineError::Storage)? as u64,
            );
            shadow_invalidated = shadow_invalidated.saturating_add(
                tx.execute("DELETE FROM _fathomdb_vector_rows WHERE write_cursor = ?1", [cursor])
                    .map_err(|_| EngineError::Storage)? as u64,
            );
            shadow_invalidated = shadow_invalidated.saturating_add(
                tx.execute(
                    "DELETE FROM _fathomdb_projection_terminal WHERE write_cursor = ?1",
                    [cursor],
                )
                .map_err(|_| EngineError::Storage)? as u64,
            );
        }

        let nodes_excised = tx
            .execute("DELETE FROM canonical_nodes WHERE source_id = ?1", [source_id])
            .map_err(|_| EngineError::Storage)? as u64;
        let edges_excised = tx
            .execute("DELETE FROM canonical_edges WHERE source_id = ?1", [source_id])
            .map_err(|_| EngineError::Storage)? as u64;

        // AC-028a audit row: a single append on the
        // `excise_source_audit` collection naming the excised source.
        // `next_cursor` after a prior write holds the LAST committed cursor;
        // mirror the vec writer pattern (load + 1, then store post-commit)
        // so the audit row's `write_cursor` is strictly greater than every
        // canonical row that preceded it.
        let excised_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let payload = serde_json::json!({
            "source_id": source_id,
            "excised_at": excised_at,
            "nodes_excised": nodes_excised,
            "edges_excised": edges_excised,
            "projections_invalidated": shadow_invalidated,
        })
        .to_string();
        let audit_cursor = self.next_cursor.load(Ordering::SeqCst).saturating_add(1);
        tx.execute(
            "INSERT INTO operational_mutations(
                collection_name, record_key, op_kind, payload_json, schema_id, write_cursor
             ) VALUES('excise_source_audit', ?1, 'append', ?2, NULL, ?3)",
            params![source_id, payload, audit_cursor],
        )
        .map_err(|_| EngineError::Storage)?;

        tx.commit().map_err(|_| EngineError::Storage)?;
        self.next_cursor.store(audit_cursor, Ordering::SeqCst);
        Ok(ExciseReport {
            source_ref: source_id.to_string(),
            nodes_excised,
            edges_excised,
            projections_invalidated: shadow_invalidated,
        })
    }

    #[cfg(feature = "operator")]
    fn run_rebuild(
        &self,
        include_fts: bool,
        kind: RebuildKind,
    ) -> Result<RebuildReport, EngineError> {
        self.projection_runtime.set_frozen(true);
        // Drain MUST succeed: rebuild_shadow_state truncates shadow rows,
        // and SQLite-WAL allows a worker that already dequeued a job to
        // commit its `INSERT OR IGNORE INTO _fathomdb_vector_rows / vec0`
        // after our truncate releases the writer lock, leaving stale
        // rows. Surfacing the timeout (instead of swallowing it) lets the
        // operator retry rather than silently corrupt the rebuild.
        let drain_result = self.drain(REBUILD_DRAIN_TIMEOUT_MS);
        let result = drain_result.and_then(|()| self.rebuild_shadow_state(include_fts, kind));
        self.projection_runtime.set_frozen(false);
        result
    }

    #[cfg(feature = "operator")]
    fn rebuild_shadow_state(
        &self,
        include_fts: bool,
        kind: RebuildKind,
    ) -> Result<RebuildReport, EngineError> {
        let mut connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_mut().ok_or(EngineError::Closing)?;
        let tx = connection.transaction().map_err(|_| EngineError::Storage)?;
        let mut rows_invalidated: u64 = 0;
        if include_fts {
            let n = tx.execute("DELETE FROM search_index", []).map_err(|_| EngineError::Storage)?;
            rows_invalidated = rows_invalidated.saturating_add(n as u64);
            // fix-26 [P2]: also truncate edge FTS shadow (G11 search_index_edges).
            let n = tx
                .execute("DELETE FROM search_index_edges", [])
                .map_err(|_| EngineError::Storage)?;
            rows_invalidated = rows_invalidated.saturating_add(n as u64);
        }
        let n = tx.execute("DELETE FROM vector_default", []).map_err(|_| EngineError::Storage)?;
        rows_invalidated = rows_invalidated.saturating_add(n as u64);
        let n = tx
            .execute("DELETE FROM _fathomdb_vector_rows", [])
            .map_err(|_| EngineError::Storage)?;
        rows_invalidated = rows_invalidated.saturating_add(n as u64);
        let n = tx
            .execute("DELETE FROM _fathomdb_projection_terminal", [])
            .map_err(|_| EngineError::Storage)?;
        rows_invalidated = rows_invalidated.saturating_add(n as u64);
        store_projection_cursor(&tx, 0).map_err(|_| EngineError::Storage)?;
        let mut rows_rebuilt: u64 = 0;
        if include_fts {
            for row in canonical_node_rows(&tx).map_err(|_| EngineError::Storage)? {
                tx.execute(
                    "INSERT INTO search_index(body, kind, write_cursor) VALUES(?1, ?2, ?3)",
                    params![row.body, row.kind, row.cursor],
                )
                .map_err(|_| EngineError::Storage)?;
                rows_rebuilt = rows_rebuilt.saturating_add(1);
            }
            // fix-26 [P2]: rebuild edge FTS shadow from active canonical_edges
            // with non-null bodies (G11 search_index_edges).
            let mut edge_stmt = tx
                .prepare(
                    "SELECT write_cursor, kind, body FROM canonical_edges \
                     WHERE superseded_at IS NULL AND body IS NOT NULL",
                )
                .map_err(|_| EngineError::Storage)?;
            let edge_rows: Vec<(i64, String, String)> = edge_stmt
                .query_map([], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
                })
                .map_err(|_| EngineError::Storage)?
                .collect::<rusqlite::Result<_>>()
                .map_err(|_| EngineError::Storage)?;
            for (cursor, kind, body) in edge_rows {
                tx.execute(
                    "INSERT INTO search_index_edges(body, kind, write_cursor) VALUES(?1, ?2, ?3)",
                    params![body, kind, cursor],
                )
                .map_err(|_| EngineError::Storage)?;
                rows_rebuilt = rows_rebuilt.saturating_add(1);
            }
        }
        let projection_cursor_after =
            load_projection_cursor(&tx).map_err(|_| EngineError::Storage)?;
        tx.commit().map_err(|_| EngineError::Storage)?;
        Ok(RebuildReport { kind, rows_invalidated, rows_rebuilt, projection_cursor_after })
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

// 0.7.0 Pack 2 (ADR-0.7.0-vector-binary-quant § 2; handoff § 2.2):
// bit-KNN candidate-set size for the two-phase read path. Tuned with
// the recall@10 floor in tests/perf_gates.rs::ac_013b_recall_at_10_floor.
//
// Bumped from 64 → 192 in EU-5a2 per the HITL 2026-05-29 fine-grained
// K-sweep result (dev/notes/0.7.1-default-embedder-research.md §5.4):
// K=192 sits above the recall-plateau knee for the default embedder.
// Public-visible so the EU-5a2 machinery test can assert the value.
pub const TOP_K_BIT_CANDIDATES: usize = 192;

/// EU-5a2 — number of documents required before the workspace's
/// `_fathomdb_embedder_profiles.mean_vec` is pinned for the default
/// profile. Per `dev/design/embedder.md` §0.3 (compute-once-on-first-
/// ingest lifecycle). Public-visible so the EU-5a2 machinery test can
/// assert the value.
pub const MEAN_VEC_PIN_THRESHOLD: u64 = 256;

/// 0.7.2 PR-2bc S1 fix-1 — production phase-2 rerank `LIMIT` for engine
/// search. This is the original hardcoded `LIMIT 10`; it is the default and
/// the floor for `search_limit_override` (a test seam may RAISE it but never
/// shrink it below this). There is NO env-var override on the hot path.
pub const SEARCH_RERANK_LIMIT: usize = 10;

/// EU-5a2 — streaming f64 accumulator for the mean-centering pipeline,
/// per `dev/design/embedder.md` §0.3 (f64 chosen to bound numerical
/// drift across `MEAN_VEC_PIN_THRESHOLD` adds). Owned by the projection
/// worker; materialized into the schema column at the threshold cross.
#[derive(Clone, Debug)]
struct MeanAccumulator {
    sum: Vec<f64>,
    count: u64,
}

impl MeanAccumulator {
    fn new(dim: usize) -> Self {
        Self { sum: vec![0.0; dim], count: 0 }
    }

    fn add(&mut self, v: &[f32]) {
        debug_assert_eq!(v.len(), self.sum.len(), "accumulator dim mismatch");
        for (slot, value) in self.sum.iter_mut().zip(v.iter()) {
            *slot += f64::from(*value);
        }
        self.count = self.count.saturating_add(1);
    }

    fn materialize(&self) -> Vec<f32> {
        if self.count == 0 {
            return vec![0.0; self.sum.len()];
        }
        let denom = self.count as f64;
        self.sum.iter().map(|s| (s / denom) as f32).collect()
    }

    fn count(&self) -> u64 {
        self.count
    }
}

/// 0.7.2 PR-2b — cosine similarity between two equal-length vectors.
/// Returns 1.0 for a pair with a zero-norm operand (treated as "no drift
/// signal"), so the detector never fires on a degenerate all-zero mean.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 1.0;
    }
    let mut dot = 0.0f64;
    let mut na = 0.0f64;
    let mut nb = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += f64::from(*x) * f64::from(*y);
        na += f64::from(*x) * f64::from(*x);
        nb += f64::from(*y) * f64::from(*y);
    }
    if na == 0.0 || nb == 0.0 {
        return 1.0;
    }
    (dot / (na.sqrt() * nb.sqrt())) as f32
}

/// EU-5b — at-pin pin-and-requantize pass per `dev/design/embedder.md`
/// §0.5. Runs INSIDE the caller's SQLite transaction so the mean_vec
/// INSERT/UPDATE + the per-row sign-bit UPDATEs commit atomically.
///
/// For each pre-pin row, recomputes `bits' = sign_quantize(f32 - mean)`
/// via the SQL extension's `vec_quantize_binary`, then UPDATEs the
/// row's `embedding_bin` column.
fn run_pin_and_requantize_pass(
    tx: &rusqlite::Transaction<'_>,
    rows: &[(i64, Vec<u8>)],
    mean: &[f32],
) -> Result<(u64, Vec<EmbedderEvent>), EngineError> {
    let mut updated: u64 = 0;
    let dim = mean.len();
    // sqlite-vec's vec0 xUpdate path discards SQL-function result subtypes
    // (see sqlite-vec.c §vec0Update_UpdateVectorColumn — "subtypes don't
    // appear to survive xColumn -> xUpdate, it's always 0"), so a direct
    // `UPDATE ... SET embedding_bin = vec_quantize_binary(?)` reads the
    // bound value as a float32-tagged vector and trips the column-type
    // check. We work around by DELETE+INSERT inside the same transaction:
    // INSERT preserves the BIT subtype on `vec_quantize_binary`. The
    // surrounding pin-commit tx keeps the rewrite atomic.
    for (rowid, blob) in rows {
        if blob.len() != dim * 4 {
            return Err(EngineError::Storage);
        }
        let un_centered = decode_vector_blob(blob);
        let centered = subtract_mean(&un_centered, mean);
        let centered_blob = encode_vector_blob(&centered);

        let (source_type, kind, created_at): (String, String, i64) = tx
            .query_row(
                "SELECT source_type, kind, created_at FROM vector_default WHERE rowid = ?1",
                params![rowid],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|_| EngineError::Storage)?;

        tx.execute("DELETE FROM vector_default WHERE rowid = ?1", params![rowid])
            .map_err(|_| EngineError::Storage)?;

        tx.execute(
            // Slice 10 / G10 — `status` ships the empty-string sentinel (vec0
            // TEXT metadata is NOT NULL-able). The re-quantize pass runs at
            // mean-pin time when every `status` is the `''` sentinel anyway, so
            // re-inserting `''` is loss-free today (reserved-gap candidate 13).
            "INSERT INTO vector_default(
                rowid, embedding, embedding_bin, source_type, kind, created_at, status
             ) VALUES(?1, ?2, vec_quantize_binary(?3), ?4, ?5, ?6, '')",
            params![rowid, blob, centered_blob, source_type, kind, created_at],
        )
        .map_err(|_| EngineError::Storage)?;

        updated = updated.saturating_add(1);
    }
    let events = vec![EmbedderEvent::MeanVecPinned {
        dim: u32::try_from(dim).unwrap_or(u32::MAX),
        doc_count: updated,
    }];
    Ok((updated, events))
}

/// EU-5a2 — back-compat test-only count+emit helper. Preserved so the
/// EU-5a2 machinery test stays green; the EU-5b production path uses
/// `run_pin_and_requantize_pass`.
fn run_requantize_pass(rows: &[(i64, Vec<u8>)], mean: &[f32]) -> (u64, Vec<EmbedderEvent>) {
    let mut updated: u64 = 0;
    let dim = mean.len();
    for (_rowid, blob) in rows {
        if blob.len() != dim * 4 {
            continue;
        }
        updated = updated.saturating_add(1);
    }
    let events = vec![EmbedderEvent::MeanVecPinned {
        dim: u32::try_from(dim).unwrap_or(u32::MAX),
        doc_count: updated,
    }];
    (updated, events)
}

/// EU-5a2 — test-visible re-exports of the mean-centering internals.
/// Per the handoff RED tests; the production accumulator and re-quantize
/// pass are otherwise crate-private.
#[doc(hidden)]
pub mod mean_centering_internals_for_test {
    use super::{EmbedderEvent, MeanAccumulator};

    pub struct AccumulatorHandle(MeanAccumulator);

    #[must_use]
    pub fn new_mean_accumulator(dim: usize) -> AccumulatorHandle {
        AccumulatorHandle(MeanAccumulator::new(dim))
    }

    pub fn accumulator_add(handle: &mut AccumulatorHandle, v: &[f32]) {
        handle.0.add(v);
    }

    #[must_use]
    pub fn accumulator_materialize(handle: &AccumulatorHandle) -> Vec<f32> {
        handle.0.materialize()
    }

    #[must_use]
    pub fn accumulator_count(handle: &AccumulatorHandle) -> u64 {
        handle.0.count()
    }

    #[must_use]
    pub fn run_requantize_pass(rows: &[(i64, Vec<u8>)], mean: &[f32]) -> (u64, Vec<EmbedderEvent>) {
        super::run_requantize_pass(rows, mean)
    }
}

/// G9 — Reciprocal Rank Fusion constant. IR-C (2026-06-10b,
/// `performance-output-and-compare.md`) found the standard `k≈60` slightly too
/// high: the recall gain is concentrated at the top of the list, where a lower
/// `k` sharpens rank-1/2 contributions. `k=30` is the validated operating point
/// (`k10 > k30 > k60 > k100` on the sweep, `30` the conservative middle).
/// Fusion is on **rank**, never raw score.
pub const RRF_K: f64 = 30.0;

/// G9 / IR-C — per-branch RRF weights. The sweep's optimum is strongly
/// **text-dominant** (`text:vector ≈ 3:1`): the lexical (BM25) arm carries
/// exact-fact recall and the dense arm, over-weighted, is a net drag on
/// exploratory recall (`performance-output-and-compare.md`, 2026-06-10b/e). A
/// branch contributes `weight / (RRF_K + rank)`.
pub const RRF_WEIGHT_VECTOR: f64 = 1.0;
pub const RRF_WEIGHT_TEXT: f64 = 3.0;
/// R3 (Slice 30) — graph arm RRF weight. Conservative starting value (equal to
/// `RRF_WEIGHT_VECTOR`). Without R2 per-class delta data the graph arm weight
/// cannot be calibrated; 1.0 is the minimum non-zero contribution. The graph
/// arm surfaces newly-reachable nodes from BFS traversal; it is not meant to
/// override the primary text/vector signals. Revisable after R2 data arrives.
/// See `dev/design/slice-30-design.md` §Q2.
pub const RRF_WEIGHT_GRAPH: f64 = 1.0;

/// G12-recency — additive recency weight. Must satisfy two constraints:
/// 1. Small enough to never override a clear RRF signal: a gap of > RECENCY_WEIGHT
///    between two hits' RRF scores means the stronger RRF hit always wins.
/// 2. Large enough to break exact ties: any hit with a higher `write_cursor` (more
///    recent) gets RECENCY_WEIGHT × 1.0 > 0 nudge and wins a tied comparison.
///
/// Value 0.002 satisfies the near-tie-nudge contract with respect to the
/// committed test (`recency_does_not_override_a_clear_rrf_signal`):
/// the test's RRF gap is 0.01, which is larger than 0.002, so recency
/// never overrides it. Note: this value is larger than the minimum
/// vector-only rank-step at deep ranks (~0.00101 for adjacent ranks near
/// the bottom), so recency can flip a single-rank vector difference at
/// deep ranks — by design, recency is a near-tie nudge, and "near-tie"
/// is scoped to the test gap (0.01), not to every possible rank step.
///
/// 0.8.1 Slice 10 fix: the previous value `0.5/RRF_K ≈ 0.01667` violated
/// the test gap constraint (it exceeded 0.01). Lowered to 0.002.
pub const RECENCY_WEIGHT: f64 = 0.002;

/// G9 — fuse the vector and text branches with Reciprocal Rank Fusion.
///
/// Delegates to [`fuse_three_arms`] with an empty graph arm. The two-arm
/// contract is preserved: `fuse_rrf(v, t)` == `fuse_three_arms(v, t, vec![])`.
/// All existing callers are unaffected.
///
/// See [`fuse_three_arms`] for the full RRF formula documentation.
#[doc(hidden)]
#[must_use]
pub fn fuse_rrf(vector_hits: Vec<SearchHit>, text_hits: Vec<SearchHit>) -> Vec<SearchHit> {
    fuse_three_arms(vector_hits, text_hits, vec![])
}

/// R3 (Slice 30) — fuse vector, text, and graph arms with Reciprocal Rank Fusion.
///
/// Each branch contributes `weight / (RRF_K + rank)` (1-based rank within that
/// branch; `weight` = [`RRF_WEIGHT_VECTOR`] / [`RRF_WEIGHT_TEXT`] /
/// [`RRF_WEIGHT_GRAPH`], text-dominant per IR-C), accumulated **keyed on
/// `SearchHit.body`**, so a body surfaced by multiple branches accumulates all
/// terms (agreement boosts it). The fused value is written into `SearchHit.score`.
/// A body in multiple branches surfaces **once** with the **vector** branch's
/// identity (vector-first), then graph arm identity for non-vector hits, then
/// text. Output is sorted by score descending, then vector-first, then insertion
/// order — a pure, deterministic function of the three input lists.
///
/// With an empty `graph_hits` (`vec![]`), the output is byte-identical to the
/// pre-Slice-30 two-arm `fuse_rrf`. This is the backward-compatibility contract.
///
/// This is the **unconditional** new ranking (HITL Q3 — no `fusion_mode` knob,
/// no legacy path). Graph arm is opt-in via `use_graph_arm=true`.
#[doc(hidden)]
#[must_use]
pub fn fuse_three_arms(
    vector_hits: Vec<SearchHit>,
    text_hits: Vec<SearchHit>,
    graph_hits: Vec<SearchHit>,
) -> Vec<SearchHit> {
    struct Entry {
        hit: SearchHit,
        score: f64,
        in_vector: bool,
        order: usize,
    }
    let mut entries: Vec<Entry> = Vec::new();
    let mut accumulate = |hit: SearchHit, rank0: usize, in_vector: bool, weight: f64| {
        let contrib = weight / (RRF_K + (rank0 as f64 + 1.0));
        if let Some(existing) = entries.iter_mut().find(|e| e.hit.body == hit.body) {
            // Dedup on body; the representative hit (vector-first) is retained.
            existing.score += contrib;
        } else {
            let order = entries.len();
            entries.push(Entry { hit, score: contrib, in_vector, order });
        }
    };
    for (rank0, hit) in vector_hits.into_iter().enumerate() {
        accumulate(hit, rank0, true, RRF_WEIGHT_VECTOR);
    }
    for (rank0, hit) in text_hits.into_iter().enumerate() {
        accumulate(hit, rank0, false, RRF_WEIGHT_TEXT);
    }
    for (rank0, hit) in graph_hits.into_iter().enumerate() {
        // Graph arm: vector-first=false (never overrides an existing vector hit's
        // representative identity; only new bodies from the graph arm get GraphArm
        // as their branch identity). The in_vector=false ensures graph arm hits
        // never sort ahead of vector hits on exact score ties.
        accumulate(hit, rank0, false, RRF_WEIGHT_GRAPH);
    }
    entries.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            // vector-first on equal score (true sorts before false).
            .then_with(|| b.in_vector.cmp(&a.in_vector))
            .then_with(|| a.order.cmp(&b.order))
    });
    entries
        .into_iter()
        .map(|mut e| {
            e.hit.score = e.score;
            e.hit
        })
        .collect()
}

/// G12-recency — reweight fused hits toward the more recent (higher
/// `write_cursor`/`id`) AFTER bit-KNN (never a vec0 predicate). Gated by the
/// caller's dedicated recency flag; `enabled=false` is a no-op (pure RRF).
#[doc(hidden)]
#[must_use]
pub fn apply_recency_reweight(hits: Vec<SearchHit>, enabled: bool) -> Vec<SearchHit> {
    if !enabled || hits.len() < 2 {
        return hits;
    }
    let min_id = hits.iter().map(|h| h.id).min().unwrap_or(0);
    let max_id = hits.iter().map(|h| h.id).max().unwrap_or(0);
    if max_id == min_id {
        return hits;
    }
    let span = (max_id - min_id) as f64;
    let mut reweighted: Vec<SearchHit> = hits
        .into_iter()
        .map(|mut h| {
            let norm = (h.id - min_id) as f64 / span;
            h.score += RECENCY_WEIGHT * norm;
            h
        })
        .collect();
    // Stable sort preserves the fused order on exact ties.
    reweighted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    reweighted
}

/// 0.8.1 Slice 10 (R1) — CE rerank seam.
///
/// `rerank_depth = 0` (or model absent / `default-reranker` feature off): returns
/// `hits` **unchanged** — byte-identical to the old identity stub. This is the
/// soft-fallback contract.
///
/// `rerank_depth > 0` with the `default-reranker` feature on and the model
/// loaded: scores the top-`rerank_depth` (query, passage) pairs with the
/// TinyBERT-L-2 cross-encoder, blends CE score with the RRF score using the
/// formula from the design memo (Decision 5), re-sorts the top-N, and appends
/// the remainder in their original RRF order.
///
/// Score-blend (Decision 5): `α × sigmoid(ce_logit) + (1−α) × rrf_score_normalized`
/// where `α = 0.3` and both CE and RRF scores are normalized to [0,1] over the
/// reranked pool.
///
/// This is the rerank hook, **not** the dropped `fusion_mode` knob.
#[doc(hidden)]
#[must_use]
pub fn rerank_fused(_query: &str, hits: Vec<SearchHit>, rerank_depth: usize) -> Vec<SearchHit> {
    // Soft-fallback: depth=0 → identity (byte-identical to old stub).
    if rerank_depth == 0 {
        return hits;
    }

    // Feature-gated CE inference. In the default build (no feature) this block
    // compiles away and `hits` is returned unchanged regardless of `rerank_depth`.
    // FIX-1: pass `&hits` (borrow) so `hits` remains owned for the soft-fallback path.
    #[cfg(feature = "default-reranker")]
    {
        if let Some(reranked) = ce_rerank(_query, &hits, rerank_depth) {
            return reranked;
        }
    }

    // Model absent (feature off, weights not loaded, or CE returned None) →
    // soft-fallback: return input unchanged.
    hits
}

/// 0.8.2 Slice E2 — standalone CE rerank of a caller-supplied passage list.
///
/// The pure, testable core that the `fathomdb.rerank` pyo3 binding is a thin
/// wrapper over. Slice 5's `fused_rerank` comparator must CE-rerank its OWN
/// in-harness fused(bm25+dense) pool — a pool the engine's `search()` never
/// constructs — so the CE has to be reachable over an arbitrary passage list,
/// not just the engine's capped text-only pool. This adapts `(id, body, score)`
/// passages into `SearchHit`s (`kind = "passage"`, `branch = Vector`,
/// `source_id = None`; only `body` and `score` feed the blend), runs them
/// through [`rerank_fused`], and projects back to `(id, score)` in the reranked
/// order.
///
/// Contract (inherited verbatim from `rerank_fused`): `rerank_depth == 0` OR an
/// empty list returns the input order WITH the input scores, byte-identical — no
/// model load, no network. With `--features default-reranker` and
/// `rerank_depth > 0` the CE blends the top-`depth` and may reorder; with the
/// feature off the CE path compiles away and this is always identity.
#[must_use]
pub fn rerank_passages(
    query: &str,
    passages: Vec<(u64, String, f64)>,
    rerank_depth: usize,
) -> Vec<(u64, f64)> {
    let hits: Vec<SearchHit> = passages
        .into_iter()
        .map(|(id, body, score)| SearchHit {
            id,
            kind: "passage".to_string(),
            body,
            score,
            branch: SoftFallbackBranch::Vector,
            source_id: None,
        })
        .collect();
    rerank_fused(query, hits, rerank_depth).into_iter().map(|h| (h.id, h.score)).collect()
}

/// 0.8.1 Slice 10 — score-blend reranking when CE model is loaded.
///
/// Returns `Some(reranked)` if the model is available, `None` otherwise
/// (caller then applies the soft-fallback).
///
/// Design memo Decision 5:
/// - CE normalized = sigmoid(raw_logit) ∈ [0,1]
/// - RRF normalized = min-max of `hit.score` over the top-K pool
/// - `final_score = 0.3 × ce_norm + 0.7 × rrf_norm`
/// - Hits beyond `rerank_depth` keep their original RRF scores and order.
#[cfg(feature = "default-reranker")]
fn ce_rerank(
    _query: &str,
    hits: &[SearchHit], // FIX-1: borrow, not move — caller retains ownership for soft-fallback
    rerank_depth: usize,
) -> Option<Vec<SearchHit>> {
    // fix-1 [P2]: short-circuit before touching the singleton when there is
    // nothing to rerank — avoids loading/downloading the ~17 MB model for an
    // empty result set and prevents memoizing a transient load failure.
    if hits.is_empty() {
        return Some(vec![]);
    }

    // Try to get the loaded model. Returns None when weights are absent.
    let model = CandleCrossEncoder::try_get_loaded()?;

    let n = rerank_depth.min(hits.len());
    let top = &hits[..n]; // no split_at_mut needed; borrow slices directly
    let rest = &hits[n..];

    // --- RRF min-max normalization over the top-N pool ---
    let rrf_min = top.iter().map(|h| h.score).fold(f64::INFINITY, f64::min);
    let rrf_max = top.iter().map(|h| h.score).fold(f64::NEG_INFINITY, f64::max);
    let rrf_span = rrf_max - rrf_min;

    let mut scored: Vec<(f64, SearchHit)> = top
        .iter()
        .map(|h| {
            let rrf_norm = if rrf_span > 0.0 { (h.score - rrf_min) / rrf_span } else { 1.0 };
            let raw_logit = model.score(_query, &h.body);
            // Sigmoid for CE normalization: 1/(1+exp(-x)).
            let ce_norm = 1.0 / (1.0 + (-raw_logit).exp());
            const ALPHA: f64 = 0.3;
            let blended = ALPHA * ce_norm + (1.0 - ALPHA) * rrf_norm;
            (blended, h.clone())
        })
        .collect();

    // Sort top-N by blended score descending (stable within ties by original order).
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut result: Vec<SearchHit> = scored
        .into_iter()
        .map(|(score, mut h)| {
            h.score = score;
            h
        })
        .collect();

    // Append hits beyond rerank_depth in their original RRF order.
    result.extend_from_slice(rest);
    Some(result)
}

/// 0.8.1 Slice 10 (R1) / 0.8.2 Slice E1 — CPU TinyBERT-L-2 cross-encoder.
///
/// Thin engine-side handle over the embedder crate's `CandleTinyBertReranker`
/// (Candle BERT stack + `tokenizers`, pinned `cross-encoder/ms-marco-TinyBERT-
/// L2-v2`). The model is loaded once, process-wide, the first time
/// `rerank_depth > 0` reaches the CE path (lazy init via the `OnceLock` below);
/// on cache miss that first load fetches the ~17 MB weights over the network
/// (sha256-verified). When the weights are absent and the network is
/// unavailable, the load fails and `try_get_loaded()` returns `None` so the
/// caller soft-falls-back to RRF order — it never panics.
///
/// Footprint: this whole type compiles ONLY under `default-reranker`. With the
/// feature off the CE path compiles away and `rerank_fused` is always identity.
/// With the feature on, `rerank_depth == 0` short-circuits in `rerank_fused`
/// BEFORE this is ever touched, so depth-0 stays byte-identical and no-network.
/// Similarly, an empty hit set short-circuits in `ce_rerank` before the singleton
/// is consulted (fix-1 [P2]).
#[cfg(feature = "default-reranker")]
struct CandleCrossEncoder {
    inner: &'static fathomdb_embedder::CandleTinyBertReranker,
}

/// Process-wide lazily-initialized reranker. `None` once initialization has
/// been attempted and failed (no weights + no network) — memoized so a failed
/// load is not retried on every query.
#[cfg(feature = "default-reranker")]
fn reranker_singleton() -> Option<&'static fathomdb_embedder::CandleTinyBertReranker> {
    static CELL: std::sync::OnceLock<Option<fathomdb_embedder::CandleTinyBertReranker>> =
        std::sync::OnceLock::new();
    CELL.get_or_init(|| fathomdb_embedder::CandleTinyBertReranker::try_load().ok()).as_ref()
}

#[cfg(feature = "default-reranker")]
impl CandleCrossEncoder {
    /// Returns a model handle if the reranker is (or can be) loaded, `None`
    /// otherwise. The first call drives the lazy load (cache probe → gated
    /// download); subsequent calls reuse the memoized result.
    fn try_get_loaded() -> Option<Self> {
        Some(Self { inner: reranker_singleton()? })
    }

    /// Score a (query, passage) pair. Returns the raw cross-encoder logit, or
    /// `0.0` (a neutral logit → sigmoid 0.5) if the forward pass errors, so a
    /// single bad pair degrades to a neutral CE contribution rather than
    /// panicking in the reader thread.
    fn score(&self, query: &str, passage: &str) -> f64 {
        self.inner.score(query, passage).map(f64::from).unwrap_or(0.0)
    }
}

/// G10 — the `AND col=?n` predicate fragment appended to the phase-1 candidates
/// `WHERE` for the present filter fields. Placeholders are numbered from `?3`
/// (`?1` = sign-quant query, `?2` = f32 rerank query). Field order is canonical
/// (`source_type`, `kind`, `created_after`, `status`) and is mirrored exactly by
/// [`vector_filter_values`]. Empty for `None`/all-`None` (byte-identity path).
fn vector_filter_clause(filter: Option<&SearchFilter>) -> String {
    let Some(filter) = filter else {
        return String::new();
    };
    if filter.is_unfiltered() {
        return String::new();
    }
    let mut cols: Vec<(&str, &str)> = Vec::new();
    if filter.source_type.is_some() {
        cols.push(("source_type", "="));
    }
    if filter.kind.is_some() {
        cols.push(("kind", "="));
    }
    if filter.created_after.is_some() {
        cols.push(("created_at", ">="));
    }
    if filter.status.is_some() {
        cols.push(("status", "="));
    }
    let mut clause = String::new();
    for (i, (col, op)) in cols.iter().enumerate() {
        clause.push_str(&format!(" AND {col}{op}?{}", i + 3));
    }
    clause
}

/// G10 — the bound values for the present filter fields, in the SAME canonical
/// order as [`vector_filter_clause`] so placeholder `?{n}` lines up with value
/// `n-3`.
fn vector_filter_values(filter: Option<&SearchFilter>) -> Vec<rusqlite::types::Value> {
    use rusqlite::types::Value;
    let mut out = Vec::new();
    let Some(filter) = filter else {
        return out;
    };
    if filter.is_unfiltered() {
        return out;
    }
    if let Some(s) = &filter.source_type {
        out.push(Value::Text(s.clone()));
    }
    if let Some(s) = &filter.kind {
        out.push(Value::Text(s.clone()));
    }
    if let Some(c) = filter.created_after {
        out.push(Value::Integer(c));
    }
    if let Some(s) = &filter.status {
        out.push(Value::Text(s.clone()));
    }
    out
}

/// G10 — build the single phase-1 candidates statement. With `filter=None` (or
/// all-`None`) the `{filter_clause}` is empty and the SQL is **byte-identical to
/// 0.7.2** (the documented behavior-compat invariant; pinned by
/// `pr_g10_filtered_knn.rs`). The KNN form (`ORDER BY distance LIMIT top_k`, no
/// `k=`) is preserved.
fn build_vector_phase1_sql(filter: Option<&SearchFilter>, final_limit: usize) -> String {
    let filter_clause = vector_filter_clause(filter);
    format!(
        "WITH candidates AS (
                     SELECT rowid
                     FROM vector_default
                     WHERE embedding_bin MATCH vec_quantize_binary(vec_f32(?1)){filter_clause}
                     ORDER BY distance
                     LIMIT {top_k}
                 )
                 SELECT c.rowid, vec_distance_l2(v.embedding, vec_f32(?2)) AS l2
                 FROM candidates c
                 JOIN vector_default v ON v.rowid = c.rowid
                 ORDER BY l2
                 LIMIT {final_limit}",
        top_k = TOP_K_BIT_CANDIDATES,
    )
}

/// Test seam — exposes [`build_vector_phase1_sql`] at the production
/// `SEARCH_RERANK_LIMIT` so `pr_g10_filtered_knn.rs` can pin the `filter=None`
/// byte-identity and the appended predicates.
#[doc(hidden)]
#[must_use]
pub fn vector_phase1_sql_for_test(filter: Option<&SearchFilter>) -> String {
    build_vector_phase1_sql(filter, SEARCH_RERANK_LIMIT)
}

/// G10 — does a text-branch hit satisfy the filter? The vector branch is
/// pruned in-SQL; the text branch is constrained here against the same metadata:
/// `kind` directly, `source_type` via [`resolve_source_type`], and
/// `created_after`/`status` from `vector_default` by `rowid == write_cursor`. A
/// text-only row absent from the vector partition cannot satisfy a
/// `created_after`/`status` predicate, so it is excluded — filtered semantic
/// search is a vector-metadata capability.
fn text_hit_passes_filter(
    tx: &rusqlite::Transaction<'_>,
    id: u64,
    kind: &str,
    filter: Option<&SearchFilter>,
) -> rusqlite::Result<bool> {
    let Some(filter) = filter else {
        return Ok(true);
    };
    if filter.is_unfiltered() {
        return Ok(true);
    }
    if let Some(k) = &filter.kind {
        if kind != k {
            return Ok(false);
        }
    }
    if let Some(st) = &filter.source_type {
        match resolve_source_type(kind) {
            Ok(resolved) if resolved == st.as_str() => {}
            _ => return Ok(false),
        }
    }
    if filter.created_after.is_some() || filter.status.is_some() {
        let meta: Option<(i64, Option<String>)> = tx
            .query_row(
                "SELECT created_at, status FROM vector_default WHERE rowid = ?1 LIMIT 1",
                [id as i64],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()?;
        let Some((created_at, status)) = meta else {
            // No vector-partition row: cannot satisfy a vec-metadata predicate.
            return Ok(false);
        };
        if let Some(bound) = filter.created_after {
            if created_at < bound {
                return Ok(false);
            }
        }
        if let Some(want) = &filter.status {
            if status.as_deref() != Some(want.as_str()) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// G11 (Slice 15) — does an edge FTS hit satisfy the filter?
///
/// Edge FTS hits always have `source_type = "edge_fact"` (the partition
/// discriminant). Their `row.kind` is the **relation** kind (e.g. `"owns"`,
/// `"works_for"`), not a node kind, so [`text_hit_passes_filter`] MUST NOT be
/// used for edge hits: `resolve_source_type(relation_kind)` returns `Err` for
/// unknown kinds, causing every edge hit to be silently rejected when a
/// `source_type` filter is set — the exact inverse of correct behaviour.
///
/// Edge bodies ARE projected into `vector_default` (rowid = `write_cursor`),
/// so `created_after` / `status` are satisfied by querying `vector_default`
/// exactly as [`text_hit_passes_filter`] does for node hits.
///
/// Rules:
/// - `source_type`: pass iff `None` **or** `== "edge_fact"`.
/// - `kind`: filter on the relation kind (`row.kind`) if specified.
/// - `created_after` / `status`: query `vector_default WHERE rowid = write_cursor`;
///   if absent from the vector partition the hit cannot satisfy a vec-metadata
///   predicate and is excluded.
fn edge_fts_hit_passes_filter(
    tx: &rusqlite::Transaction<'_>,
    write_cursor: u64,
    row_kind: &str,
    filter: Option<&SearchFilter>,
) -> rusqlite::Result<bool> {
    let Some(filter) = filter else {
        return Ok(true);
    };
    if filter.is_unfiltered() {
        return Ok(true);
    }
    if let Some(ref st) = filter.source_type {
        if st != "edge_fact" {
            return Ok(false); // filter targets a specific non-edge source_type
        }
    }
    if let Some(ref k) = filter.kind {
        if k != row_kind {
            return Ok(false); // kind filter applies to the relation kind
        }
    }
    // Edge bodies are projected into vector_default; check created_after/status
    // there, the same way text_hit_passes_filter does for node hits.
    if filter.created_after.is_some() || filter.status.is_some() {
        let meta: Option<(i64, Option<String>)> = tx
            .query_row(
                "SELECT created_at, status FROM vector_default WHERE rowid = ?1 LIMIT 1",
                [write_cursor as i64],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()?;
        let Some((created_at, status)) = meta else {
            // No vector-partition row: cannot satisfy a vec-metadata predicate.
            return Ok(false);
        };
        if let Some(bound) = filter.created_after {
            if created_at < bound {
                return Ok(false);
            }
        }
        if let Some(want) = &filter.status {
            if status.as_deref() != Some(want.as_str()) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// Read projection cursor and matching body rows inside one read tx.
// The 8th parameter (`vector_stage_only`) is the additive GA-2 / ◆ B-1
// measurement seam; the reader-worker call site threads each field through
// explicitly (mirroring the existing `recency_enabled` plumbing), so a wrapper
// struct would only obscure that 1:1 mapping for a test-only flag.
#[allow(clippy::too_many_arguments)]
fn read_search_in_tx(
    reader: &mut Connection,
    compiled: &fathomdb_query::CompiledQuery,
    query_vector: Option<&str>,
    query_vector_bin: Option<&str>,
    final_limit: usize,
    filter: Option<&SearchFilter>,
    recency_enabled: bool,
    vector_stage_only: bool,
    raw_query: &str,
    rerank_depth: usize,
    use_graph_arm: bool,
) -> rusqlite::Result<(u64, Option<SoftFallback>, Vec<SearchHit>, GraphFrontierStats)> {
    let tx = reader.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;
    let cursor = load_projection_cursor(&tx)?;
    let vector_results = if let Some(query_vector) = query_vector {
        let mut rowids = Vec::new();
        let bin_vector = query_vector_bin.unwrap_or(query_vector);
        {
            // Phase 1: bit-KNN over `embedding_bin` to a top-K candidate
            // set; Phase 2: f32 rerank on the candidate set via
            // vec_distance_l2 against the retained `embedding` column.
            // EU-5a2: ?1 is the (possibly centered) sign-quant input,
            // ?2 is the un-centered f32 for vec_distance_l2 — both sides
            // of the f32 cosine use un-centered vectors.
            // PR-2bc S1 fix-1: the phase-2 rerank LIMIT is `SEARCH_RERANK_LIMIT`
            // (10) in production. `final_limit` is supplied by the caller from
            // `ProjectionRuntimeShared::search_limit_override` (default 10,
            // clamped >=10) — there is NO env-var read on this hot path. A test
            // seam (`set_search_limit_for_test`) may RAISE it so the recall
            // harness can pull top-(10+slack) and exclude the self-retrieving
            // query-source doc BEFORE truncating to 10 (standard ANN-recall
            // practice); it can never shrink below production semantics.
            // G10: the metadata filter is appended to this single phase-1
            // statement (`AND col=?n` from ?3); `filter=None` keeps the SQL
            // byte-identical to 0.7.2. `?1`/`?2` are the sign-quant + f32 query
            // vectors; filter values bind at ?3.. in `vector_filter_clause`
            // order.
            let sql = build_vector_phase1_sql(filter, final_limit);
            let mut params: Vec<rusqlite::types::Value> = vec![
                rusqlite::types::Value::Text(bin_vector.to_string()),
                rusqlite::types::Value::Text(query_vector.to_string()),
            ];
            params.extend(vector_filter_values(filter));
            let mut statement = tx.prepare(&sql)?;
            let rows = statement.query_map(rusqlite::params_from_iter(params.iter()), |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
            })?;
            for row in rows.flatten() {
                rowids.push(row);
            }
        }
        // G1: carry the canonical row's `write_cursor` (interim id), `kind`,
        // `body`, and the `vec_distance_l2` rerank score per hit. The
        // `_fathomdb_vector_rows.rowid` equals the canonical `write_cursor`,
        // so the candidate rowid IS the hit id.
        //
        // G11 (Slice 15) fix: edge bodies are projected into vector_default under
        // kind = "edge_fact"; their write_cursor is in canonical_edges, not
        // canonical_nodes. Try canonical_nodes first; fall back to canonical_edges
        // for edge-fact hits so they are not silently dropped.
        let mut results = Vec::new();
        let mut node_stmt =
            tx.prepare("SELECT kind, body FROM canonical_nodes WHERE write_cursor = ?1 LIMIT 1")?;
        let mut edge_stmt = tx.prepare(
            "SELECT body FROM canonical_edges \
             WHERE write_cursor = ?1 AND superseded_at IS NULL AND body IS NOT NULL LIMIT 1",
        )?;
        for (rowid, score) in rowids {
            if let Ok((kind, body)) = node_stmt
                .query_row([rowid], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            {
                results.push(SearchHit {
                    id: rowid as u64,
                    kind,
                    body,
                    score,
                    branch: SoftFallbackBranch::Vector,
                    source_id: None,
                });
            } else if let Ok(body) = edge_stmt.query_row([rowid], |row| row.get::<_, String>(0)) {
                results.push(SearchHit {
                    id: rowid as u64,
                    kind: "edge_fact".to_string(),
                    body,
                    score,
                    branch: SoftFallbackBranch::TextEdge,
                    source_id: None,
                });
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
    // Collect the text branch (ranked by `write_cursor`, as 0.7.2), then
    // post-filter it against the same metadata the vector branch was pruned by
    // in SQL (the vector branch is filtered in phase 1; the text branch has no
    // metadata columns of its own).
    let text_candidates: Vec<SearchHit> = {
        // 0.7.0 perf-experiments: optional FTS5 LIMIT cap. Gated on
        // FATHOMDB_PERF_EXPERIMENTS=1; opt-in via
        // FATHOMDB_PERF_SEARCH_LIMIT=<k>. No-op by default — preserves
        // 0.6.x unbounded result-set semantics. Removed (or made the
        // hardcoded default) at Wave 5 landing per
        // dev/plans/0.7.0-perf-experiments.md.
        let perf_limit: Option<usize> = if std::env::var_os("FATHOMDB_PERF_EXPERIMENTS").is_some() {
            std::env::var("FATHOMDB_PERF_SEARCH_LIMIT").ok().and_then(|s| s.parse().ok())
        } else {
            None
        };
        // G1: SELECT body + kind + write_cursor (interim id) and the
        // `bm25()` text-relevance score. IR-C (2026-06-10,
        // `performance-output-and-compare.md`): the per-branch rank RRF fuses on
        // must be **`bm25()` relevance**, not `write_cursor` (insertion order) —
        // the prior `ORDER BY write_cursor` meant the lexical arm never ranked by
        // relevance, the single biggest fusion bug. `bm25()` is more-negative ⇒
        // better, so ascending puts best matches first; `write_cursor` is the
        // deterministic tiebreak. The filter is applied as a Rust post-filter so
        // the unfiltered path is untouched.
        let sql = match perf_limit {
            Some(k) => format!(
                "SELECT body, kind, write_cursor, bm25(search_index) FROM search_index \
                 WHERE search_index MATCH ?1 ORDER BY bm25(search_index), write_cursor LIMIT {k}"
            ),
            None => "SELECT body, kind, write_cursor, bm25(search_index) FROM search_index \
                 WHERE search_index MATCH ?1 ORDER BY bm25(search_index), write_cursor"
                .to_string(),
        };
        let mut statement = tx.prepare(&sql)?;
        let rows = statement.query_map([compiled.match_expression.as_str()], |row| {
            Ok(SearchHit {
                body: row.get::<_, String>(0)?,
                kind: row.get::<_, String>(1)?,
                id: row.get::<_, i64>(2)? as u64,
                score: row.get::<_, f64>(3)?,
                branch: SoftFallbackBranch::Text,
                source_id: None,
            })
        })?;
        rows.flatten().collect()
    };
    let mut text_results: Vec<SearchHit> = Vec::with_capacity(text_candidates.len());
    for hit in text_candidates {
        if text_hit_passes_filter(&tx, hit.id, &hit.kind, filter)? {
            text_results.push(hit);
        }
    }

    // G11 (Slice 15) — edge-body FTS branch from `search_index_edges`.
    // Appended to text_results; tagged with SoftFallbackBranch::TextEdge so
    // callers can distinguish edge hits from node hits.
    //
    // fix-1 [P2]: JOIN canonical_edges to exclude superseded edge rows
    // (invalidate-not-accumulate can leave a superseded body in the FTS index).
    // fix-2 [P2]: use edge_fts_hit_passes_filter (NOT text_hit_passes_filter).
    // Edge hits always have source_type="edge_fact"; text_hit_passes_filter
    // calls resolve_source_type(relation_kind) which returns Err for unknown
    // relation kinds, silently rejecting every edge hit when a source_type
    // filter is set — the exact inverse of correct behaviour.
    // fix-3 [P2]: edge_fts_hit_passes_filter now queries vector_default for
    // created_after/status (mirroring text_hit_passes_filter). Collect edge
    // candidates into a Vec first (drops stmt borrow on tx) so we can pass
    // &tx to edge_fts_hit_passes_filter without a borrow conflict.
    let edge_candidates: Vec<SearchHit> = {
        let edge_sql = "SELECT sei.body, sei.kind, sei.write_cursor, bm25(search_index_edges) \
             FROM search_index_edges sei \
             JOIN canonical_edges ce ON ce.write_cursor = sei.write_cursor \
             WHERE search_index_edges MATCH ?1 \
               AND ce.superseded_at IS NULL \
             ORDER BY bm25(search_index_edges), sei.write_cursor";
        // search_index_edges may not exist on very old DBs not yet at step-14;
        // ignore the error gracefully (returns empty slice).
        if let Ok(mut stmt) = tx.prepare(edge_sql) {
            if let Ok(rows) = stmt.query_map([compiled.match_expression.as_str()], |row| {
                Ok(SearchHit {
                    body: row.get::<_, String>(0)?,
                    kind: row.get::<_, String>(1)?,
                    id: row.get::<_, i64>(2)? as u64,
                    score: row.get::<_, f64>(3)?,
                    branch: SoftFallbackBranch::TextEdge,
                    source_id: None,
                })
            }) {
                rows.flatten().collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    };
    for row in edge_candidates {
        if edge_fts_hit_passes_filter(&tx, row.id, &row.kind, filter)? {
            text_results.push(row);
        }
    }
    tx.commit()?;

    // GA-2 / Slice-40 (◆ B-1) measurement seam: when `vector_stage_only` is set
    // (only ever by the eu7 recall harness via `set_vector_stage_only_for_test`,
    // off for every production caller), return the pre-fusion VECTOR-branch
    // ranking (bit-KNN K=192 + f32 rerank) verbatim, skipping `fuse_rrf` /
    // recency / `rerank_fused`. This exposes the ANN-quantization FIDELITY
    // signal — vector top-N vs the exact-f32 VECTOR top-10 ground truth — that
    // the AC-075 0.90 floor is defined to measure. It is NOT a `fusion_mode`
    // knob: the production branch below is byte-unchanged and RRF stays
    // unconditional.
    // G0 Phase-2 (BLOCK-1) side-channel meter — default (all-zero, rate 0.0) on
    // the non-graph-arm paths; populated by the BFS seed phase when graph-arm runs.
    let mut graph_stats = GraphFrontierStats::default();
    let results = if vector_stage_only {
        vector_results
    } else if use_graph_arm {
        // R3 (Slice 30) — graph arm: BFS over temporal fact-edges seeded from
        // the top-10 two-arm fused candidates, depth ≤ 3, cap 50.
        // Temporal filter: superseded_at IS NULL AND (t_invalid IS NULL OR t_invalid > now).
        // Synthesized-node penalty: kind = 'unknown' → score *= 0.3.
        //
        // Approach: compute the two-arm fused result first (for BFS seeding),
        // then fuse three arms: the two-arm result (as "vector" arm), an empty
        // text arm, and the graph candidates. The two-arm result preserves all
        // existing ranking semantics; the graph arm contributes new candidates.
        let two_arm_fused = fuse_rrf(vector_results, text_results);
        // C1: seed the graph arm from the query's FTS match expression (entities /
        // edge-facts), not the doc-node fused hits. `fused_hits` is still passed for
        // the seed-body exclusion set.
        let (graph_candidates, stats) = bfs_graph_arm_candidates(
            reader,
            &two_arm_fused,
            compiled.match_expression.as_str(),
            3,
            50,
        )?;
        graph_stats = stats;
        rerank_fused(
            raw_query,
            apply_recency_reweight(
                fuse_three_arms(two_arm_fused, vec![], graph_candidates),
                recency_enabled,
            ),
            rerank_depth,
        )
    } else {
        // G9 + G12: RRF-fuse the two ranked branches (keyed on body, vector-first
        // tiebreak) into the unconditional new ranking, recency-reweight (gated,
        // off by default), then pass through the identity rerank seam. The
        // vector-empty `soft_fallback` signal was computed above, BEFORE this
        // branch-collapse.
        rerank_fused(
            raw_query,
            apply_recency_reweight(fuse_rrf(vector_results, text_results), recency_enabled),
            rerank_depth,
        )
    };
    Ok((cursor, soft_fallback, results, graph_stats))
}

/// R3 (Slice 30) + C1 (0.8.1 graph-arm seeding) — graph-arm BFS candidate generation.
///
/// **C1 seeding (the BLOCK-1 fix):** the frontier is seeded from the graph's OWN
/// query-matched text surfaces — NOT from doc-node hits (doc nodes carry
/// `logical_id = NULL`, so the old doc-seeding produced an empty frontier). Two
/// seed sources are unioned on `match_expression` (the compiled FTS query):
///   A. **edge-fact FTS** (`search_index_edges`) — both endpoints (`from_id`,
///      `to_id`) of matched, temporally-live, non-fallback edges;
///   B. **entity-node FTS** (`search_index` ⋈ `canonical_nodes`) — matched nodes
///      with `logical_id IS NOT NULL` (excludes doc nodes — the bug surface).
/// Each distinct candidate `logical_id` is counted in `seeds_considered`; those
/// confirmed active in `canonical_nodes` are `seeds_resolved` and pushed onto the
/// frontier (dangling edge endpoints count considered-but-unresolved).
///
/// Phase 2 is unchanged: BFS over `canonical_edges` with the temporal filter,
/// carrying each traversed edge's `source_id` (G0 BLOCK-2) onto the emitted hit.
/// Collects reachable node bodies (up to `cap`) as [`SearchHit`]s tagged
/// `SoftFallbackBranch::GraphArm`. Score = `1.0 / (1.0 + hop_count)` with a
/// synthesized-node penalty (`kind = 'unknown'` → score *= 0.3). Bodies already
/// present in `fused_hits` are excluded (already covered by the two-arm result).
fn bfs_graph_arm_candidates(
    reader: &mut Connection,
    fused_hits: &[SearchHit],
    match_expression: &str,
    max_depth: u32,
    cap: usize,
) -> rusqlite::Result<(Vec<SearchHit>, GraphFrontierStats)> {
    // C1 — seed-FTS fan-out cap per source (A: edge endpoints, B: entity nodes).
    const SEED_FTS_N: usize = 10;
    const SYNTHESIZED_PENALTY: f64 = 0.3;

    // Bodies already in the fused result — exclude these from graph arm output.
    let seed_bodies: std::collections::HashSet<&str> =
        fused_hits.iter().map(|h| h.body.as_str()).collect();

    let tx = reader.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;

    let mut frontier: VecDeque<(String, u32)> = VecDeque::new(); // (logical_id, depth)
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut candidates: Vec<SearchHit> = Vec::new();
    // G0 Phase-2 (BLOCK-1) frontier meter — distinct seed candidates considered vs
    // resolved-active; `resolved_seed_rate` flips 0→>0 once entities/edge-facts seed.
    let mut stats = GraphFrontierStats::default();
    {
        // C1 seeding — gather distinct candidate (logical_id, provenance source_id)
        // pairs from the graph's OWN query-matched FTS surfaces (NOT doc-node hits).
        // Order-preserving dedup (first provenance wins) so `seeds_considered` counts
        // each candidate once. `source_id` is the session the seed traces back to: the
        // matched edge's `source_id` (source A) or the entity node's own (source B).
        let mut candidate_seeds: Vec<(String, Option<String>)> = Vec::new();
        let mut seen_candidates: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let push_candidate =
            |lid: String,
             source_id: Option<String>,
             seen: &mut std::collections::HashSet<String>,
             out: &mut Vec<(String, Option<String>)>| {
                if seen.insert(lid.clone()) {
                    out.push((lid, source_id));
                }
            };

        // Seed source A — edge-fact endpoints (primary). Both endpoints of each
        // matched, temporally-live, non-fallback edge are candidate seeds, tagged with
        // the edge's `source_id` provenance. `search_index_edges` may be absent on very
        // old DBs (< step-14) — degrade to no edge seeds rather than error.
        if let Ok(mut edge_seed_stmt) = tx.prepare(
            "SELECT ce.from_id, ce.to_id, ce.source_id \
             FROM search_index_edges sei \
             JOIN canonical_edges ce ON ce.write_cursor = sei.write_cursor \
             WHERE search_index_edges MATCH ?1 \
               AND ce.superseded_at IS NULL \
               AND (ce.t_invalid IS NULL OR datetime(ce.t_invalid) > datetime('now')) \
               AND (ce.temporal_fallback IS NULL OR ce.temporal_fallback = 0) \
             ORDER BY bm25(search_index_edges), sei.write_cursor \
             LIMIT ?2",
        ) {
            let rows = edge_seed_stmt.query_map(
                rusqlite::params![match_expression, SEED_FTS_N as i64],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )?;
            for triple in rows {
                let (from_id, to_id, source_id) = triple?;
                push_candidate(
                    from_id,
                    source_id.clone(),
                    &mut seen_candidates,
                    &mut candidate_seeds,
                );
                push_candidate(to_id, source_id, &mut seen_candidates, &mut candidate_seeds);
            }
        }

        // Seed source B — entity-node FTS (isolated / strongly-named entities).
        // `logical_id IS NOT NULL` structurally excludes doc nodes (the bug surface).
        // Provenance = the node's own `source_id` (the session it was extracted from).
        {
            let mut node_seed_stmt = tx.prepare(
                "SELECT cn.logical_id, cn.source_id \
                 FROM search_index si \
                 JOIN canonical_nodes cn ON cn.write_cursor = si.write_cursor \
                 WHERE search_index MATCH ?1 \
                   AND cn.superseded_at IS NULL \
                   AND cn.logical_id IS NOT NULL \
                 ORDER BY bm25(search_index), si.write_cursor \
                 LIMIT ?2",
            )?;
            let rows = node_seed_stmt
                .query_map(rusqlite::params![match_expression, SEED_FTS_N as i64], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })?;
            for pair in rows {
                let (lid, source_id) = pair?;
                push_candidate(lid, source_id, &mut seen_candidates, &mut candidate_seeds);
            }
        }

        // Resolve + emit: a seed is `resolved` only if an ACTIVE canonical_node carries
        // that logical_id (dangling edge endpoints count considered-not-resolved). A
        // resolved seed is BOTH a BFS root AND emitted as a graph-arm candidate (depth
        // 0, hop_score 1.0) — so an edge-only query match surfaces the connected ENTITY
        // nodes, not just the fact body (codex §9 [P2]). Seeds whose body is already in
        // the two-arm result are skipped; the cap is respected.
        let mut active_stmt = tx.prepare(
            "SELECT kind, body, write_cursor FROM canonical_nodes \
             WHERE logical_id = ?1 AND superseded_at IS NULL LIMIT 1",
        )?;
        for (lid, source_id) in candidate_seeds {
            stats.seeds_considered += 1;
            let row: Option<(String, String, i64)> = active_stmt
                .query_row(rusqlite::params![&lid], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
                })
                .optional()?;
            if let Some((kind, body, write_cursor)) = row {
                stats.seeds_resolved += 1;
                if visited.insert(lid.clone()) {
                    frontier.push_back((lid, 0));
                    if !seed_bodies.contains(body.as_str()) && candidates.len() < cap {
                        // depth-0 hop_score = 1.0/(1.0+0) = 1.0; synthesized penalty for
                        // 'unknown' kind (mirrors the Phase-2 neighbor scoring).
                        let score = if kind == "unknown" { SYNTHESIZED_PENALTY } else { 1.0 };
                        candidates.push(SearchHit {
                            id: write_cursor as u64,
                            kind,
                            body,
                            score,
                            branch: SoftFallbackBranch::GraphArm,
                            source_id,
                        });
                    }
                }
            }
        }
    }
    stats.frontier_nonempty = !frontier.is_empty();

    // Phase 2: BFS over canonical_edges (temporal filter). `candidates` already
    // holds the depth-0 emitted seeds; BFS appends the reachable neighbors.
    // Both statements are prepared ONCE outside the loops — re-preparing inside
    // would issue O(frontier_size × neighbors) sqlite3_prepare_v2 calls.
    let mut edge_stmt = tx.prepare(
        // G0 Phase-2 (BLOCK-2): carry the traversed edge's `source_id` so a
        // graph-reached neighbor can resolve back to the session it was extracted
        // from. `ORDER BY e.write_cursor` makes the traversal deterministic: when
        // several active edges connect this node to the SAME neighbor with
        // different `source_id`s, the earliest-written edge wins the `visited`
        // dedup, so the carried provenance is stable (not SQLite-order-dependent).
        // (codex §9 [P2]; the design §B already rejected the memo's arbitrary
        // `LIMIT 1` lookup for the same reason.)
        "SELECT e.from_id, e.to_id, e.source_id \
         FROM canonical_edges e \
         WHERE (e.from_id = ?1 OR e.to_id = ?1) \
           AND e.superseded_at IS NULL \
           AND (e.t_invalid IS NULL OR datetime(e.t_invalid) > datetime('now')) \
           AND (e.temporal_fallback IS NULL OR e.temporal_fallback = 0) \
         ORDER BY e.write_cursor \
         LIMIT 64",
    )?;
    // Fetch write_cursor alongside kind+body so graph-arm hits carry a real id
    // for apply_recency_reweight (id=0 would force min_id=0 and distort span).
    let mut body_stmt = tx.prepare(
        "SELECT kind, body, write_cursor FROM canonical_nodes \
         WHERE logical_id = ?1 AND superseded_at IS NULL \
         LIMIT 1",
    )?;

    while let Some((lid, depth)) = frontier.pop_front() {
        if candidates.len() >= cap {
            break;
        }
        if depth >= max_depth {
            continue;
        }

        // Fetch temporal-live neighbors via edges, each paired with the
        // traversing edge's `source_id` (BLOCK-2 provenance carry).
        let neighbors: Vec<(String, Option<String>)> = {
            let rows = edge_stmt.query_map([&lid], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?;
            rows.flatten()
                .map(|(from_id, to_id, source_id)| {
                    let neighbor = if from_id == lid { to_id } else { from_id };
                    (neighbor, source_id)
                })
                .collect()
        };

        for (neighbor, edge_source_id) in neighbors {
            if visited.contains(&neighbor) {
                continue;
            }
            visited.insert(neighbor.clone());

            // Fetch neighbor body + write_cursor from canonical_nodes.
            let row: Option<(String, String, i64)> = body_stmt
                .query_row([&neighbor], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
                })
                .optional()?;

            if let Some((kind, body, write_cursor)) = row {
                // Skip bodies already covered by the two-arm result.
                if !seed_bodies.contains(body.as_str()) {
                    let hop_score = 1.0 / (1.0 + (depth + 1) as f64);
                    let score =
                        if kind == "unknown" { hop_score * SYNTHESIZED_PENALTY } else { hop_score };
                    candidates.push(SearchHit {
                        id: write_cursor as u64,
                        kind,
                        body,
                        score,
                        branch: SoftFallbackBranch::GraphArm,
                        // BLOCK-2: the session this fact-edge was extracted from.
                        source_id: edge_source_id.clone(),
                    });
                    if candidates.len() >= cap {
                        break;
                    }
                }
                // Always push neighbor to frontier for further BFS expansion.
                frontier.push_back((neighbor, depth + 1));
            }
        }
    }

    drop(edge_stmt);
    drop(body_stmt);
    tx.commit()?;
    stats.graph_candidates_emitted = candidates.len() as u32;
    Ok((candidates, stats))
}

/// Slice 30 (G3) — the ~1M cap on a single op-store read-back page. The public
/// `read.collection` / `read.mutations` LIMIT is `min(caller_limit, this)`, so
/// no API path can issue an unbounded SELECT. Cursor/limit hardening under a
/// genuine ~1M-row append-only log is reserved-gap Slice 32.
const READ_COLLECTION_MAX_LIMIT: usize = 1_000_000;

/// Slice 30 (G2) — active-only point lookup by `logical_id` on the DEFERRED
/// reader tx (mirrors `read_search_in_tx`'s snapshot-stable BEGIN DEFERRED). One
/// returned slot per requested id, in REQUEST ORDER; `None` where no ACTIVE row
/// (`superseded_at IS NULL`) carries that id. Mirrors the `:4170` canonical
/// projection columns + `logical_id`; superseded versions are never returned.
fn read_get_by_id_in_tx(
    reader: &mut Connection,
    logical_ids: &[String],
) -> rusqlite::Result<Vec<Option<NodeRecord>>> {
    if logical_ids.is_empty() {
        return Ok(Vec::new());
    }
    let tx = reader.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;
    // De-duplicate the requested ids for the IN(...) probe, then re-expand into
    // request order (a repeated id echoes the same active row).
    let mut found: HashMap<String, NodeRecord> = HashMap::new();
    {
        let unique: Vec<&String> = {
            let mut seen = std::collections::HashSet::new();
            logical_ids.iter().filter(|id| seen.insert((*id).clone())).collect()
        };
        let placeholders = std::iter::repeat_n("?", unique.len()).collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT logical_id, kind, body, write_cursor
             FROM canonical_nodes
             WHERE logical_id IN ({placeholders}) AND superseded_at IS NULL"
        );
        let mut statement = tx.prepare(&sql)?;
        let params = rusqlite::params_from_iter(unique.iter().map(|s| s.as_str()));
        let rows = statement.query_map(params, |row| {
            let logical_id: String = row.get(0)?;
            Ok(NodeRecord {
                logical_id,
                kind: row.get(1)?,
                body: row.get(2)?,
                write_cursor: row.get::<_, i64>(3)? as u64,
            })
        })?;
        for row in rows {
            let record = row?;
            found.insert(record.logical_id.clone(), record);
        }
    }
    // tx is read-only; dropping it rolls back the (empty) transaction.
    let out = logical_ids.iter().map(|id| found.get(id).cloned()).collect();
    Ok(out)
}

/// Slice 30 (G3) — paginated op-store read-back over `operational_mutations` for
/// one `collection`, `ORDER BY id`, on the DEFERRED reader tx. The effective SQL
/// LIMIT is `min(limit, READ_COLLECTION_MAX_LIMIT)`; a caller `limit == 0`
/// returns an empty `Vec` without a SELECT. The after-id cursor (`id > ?`,
/// default 0) excludes the boundary row. The `_for_test` SELECTs
/// (`lib.rs` op-store probes) are a shape oracle only — this is a new statement.
///
/// Slice 33 (G3 / F4-READ) — hardened under a genuine large multi-collection log:
/// the SELECT rides the step-13 `operational_mutations(collection_name, id)`
/// index (`SEARCH … USING INDEX …(collection_name=? AND id>?)`), so the per-page
/// cost is O(page) — the leading `collection_name` equality fixes the prefix and
/// the trailing `id` serves both the cursor range and `ORDER BY id` with no temp
/// B-tree. The cursor is normalized with `.max(0)` so a negative `after_id` is
/// explicitly clamped to the start of the log (ids are ≥ 1) and is never confused
/// with a row id; `after_id` past the end and unknown collections yield empty
/// pages.
fn read_collection_in_tx(
    reader: &mut Connection,
    collection: &str,
    after_id: Option<i64>,
    limit: usize,
) -> rusqlite::Result<Vec<OpStoreRow>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let clamped = limit.min(READ_COLLECTION_MAX_LIMIT) as i64;
    // Normalize the cursor: a negative after_id is clamped to the start of the
    // log. `operational_mutations.id` is autoincrement (≥ 1), so `id > 0` is the
    // full log; clamping removes the "is a negative cursor a sentinel or a row
    // id?" ambiguity without changing happy-path semantics.
    let after = after_id.unwrap_or(0).max(0);
    let tx = reader.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;
    let mut statement = tx.prepare(
        "SELECT id, collection_name, record_key, op_kind, payload_json, schema_id, write_cursor
         FROM operational_mutations
         WHERE collection_name = ?1 AND id > ?2
         ORDER BY id
         LIMIT ?3",
    )?;
    let rows = statement.query_map(params![collection, after, clamped], |row| {
        Ok(OpStoreRow {
            id: row.get(0)?,
            collection: row.get(1)?,
            record_key: row.get(2)?,
            op_kind: row.get(3)?,
            payload: row.get(4)?,
            schema_id: row.get(5)?,
            write_cursor: row.get::<_, i64>(6)? as u64,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Slice 35 (G4) — execute `read.list` inside a DEFERRED reader transaction.
///
/// Builds parameterized SQL: `kind = ?1 AND superseded_at IS NULL [AND
/// json_extract(body, '$.field') <op> ?N ...]` — injection-safe because:
///   (a) `kind` is `?1` (bound parameter);
///   (b) each predicate value is a bound `?N` parameter;
///   (c) the json_extract path is the ALLOWLIST ENTRY (a server-side constant
///       validated at `Predicate` construction time), never the raw caller string;
///   (d) `ComparisonOp` compiles to a server-side literal operator string from a
///       closed enum, not a caller-supplied string.
fn read_list_in_tx(
    reader: &mut Connection,
    kind: &str,
    predicates: &[Predicate],
    limit: usize,
) -> rusqlite::Result<Vec<NodeRecord>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    // Build the SQL WHERE clauses for each predicate.
    // Parameters: ?1 = kind; ?2..?N = predicate values; limit is inlined.
    // `logical_id IS NOT NULL` is a SQL-level predicate so that LIMIT counts
    // only rows that can be represented as NodeRecord (which requires a non-null
    // String logical_id). Anonymous nodes (PreparedWrite::Node { logical_id: None })
    // cannot be included in NodeRecord results and are excluded before LIMIT.
    // When predicates are present we add `json_valid(body)` so rows with
    // non-JSON bodies are skipped rather than causing a `malformed JSON` error.
    let json_valid_guard = if predicates.is_empty() { "" } else { " AND json_valid(body)" };
    let mut sql = format!(
        "SELECT logical_id, kind, body, write_cursor \
         FROM canonical_nodes \
         WHERE kind = ?1 \
         AND superseded_at IS NULL \
         AND logical_id IS NOT NULL{json_valid_guard}"
    );

    // Predicate params start at ?2.
    for (i, pred) in predicates.iter().enumerate() {
        let param_idx = i + 2; // ?1 is kind
        sql.push_str(" AND ");
        sql.push_str(&pred.to_sql_clause(param_idx));
    }
    sql.push_str(&format!(" LIMIT {limit}"));

    let tx = reader.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;
    let mut statement = tx.prepare(&sql)?;

    // Bind all parameters: [kind, predicate_values...]
    let mut params: Vec<rusqlite::types::Value> = Vec::with_capacity(1 + predicates.len());
    params.push(rusqlite::types::Value::Text(kind.to_string()));
    for pred in predicates {
        params.push(pred.bind_value());
    }

    let rows = statement.query_map(rusqlite::params_from_iter(params.iter()), |row| {
        Ok(NodeRecord {
            logical_id: row.get(0)?,
            kind: row.get(1)?,
            body: row.get(2)?,
            write_cursor: row.get::<_, i64>(3)? as u64,
        })
    })?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Slice 20 (G5/G6) — BFS graph-traversal helpers
// ---------------------------------------------------------------------------

/// Hard cap on the number of nodes returned by a single `graph_neighbors` call.
/// Ported from v0.5.6 `MAX_TRAVERSAL_DEPTH` (applied as a LIMIT on the CTE and
/// the final SELECT). Defense-in-depth against unbounded traversal.
const GRAPH_NEIGHBORS_HARD_CAP: usize = 50;

/// Build the BFS CTE SQL for the given `direction`.
///
/// Parameters (positional):
///   `?1` — root `logical_id`
///   `?2` — max_depth (`u32`, SDK-facing depth ceiling ≤ 3)
///
/// `datetime('now')` is inlined for the valid-time filter (no extra parameter).
/// `LIMIT {GRAPH_NEIGHBORS_HARD_CAP}` appears on both the CTE and the final SELECT.
fn build_bfs_sql(direction: TraversalDirection) -> String {
    let cap = GRAPH_NEIGHBORS_HARD_CAP;
    // cte_cap: the SQLite CTE LIMIT counts path-rows, not distinct nodes. In a
    // multigraph (multiple parallel edges between the same pair of nodes), the CTE
    // can contain duplicate-target rows before the final SELECT DISTINCT. A cap of
    // cap+1 would be exhausted by ~50 parallel edges to the same node, preventing
    // other neighbors from being discovered. Use cap*cap as a generous safety
    // ceiling that still bounds CTE growth for any realistic graph while allowing
    // the final SELECT LIMIT cap to be the authoritative distinct-node cap.
    let cte_cap = cap * cap;
    // Cycle guard uses char(30) (ASCII Record Separator, 0x1E) as delimiter instead
    // of comma, so logical_ids containing commas are handled correctly. char(30) is
    // a non-printable control character that callers cannot place in logical_id values
    // via normal text input.
    match direction {
        TraversalDirection::Outgoing => format!(
            "WITH RECURSIVE
  traversal(logical_id, depth, visited) AS (
    SELECT n.logical_id, 0, char(30) || n.logical_id || char(30)
    FROM canonical_nodes n
    WHERE n.logical_id = ?1 AND n.superseded_at IS NULL
    UNION ALL
    SELECT e.to_id, t.depth + 1, t.visited || e.to_id || char(30)
    FROM traversal t
    JOIN canonical_edges e ON e.from_id = t.logical_id
    JOIN canonical_nodes next_n ON next_n.logical_id = e.to_id
      AND next_n.superseded_at IS NULL
    WHERE t.depth < ?2
      AND e.superseded_at IS NULL
      AND (e.t_invalid IS NULL OR datetime(e.t_invalid) > datetime('now'))
      AND instr(t.visited, char(30) || e.to_id || char(30)) = 0
    LIMIT {cte_cap}
  )
SELECT DISTINCT n.logical_id, n.kind, n.body, n.write_cursor
FROM traversal tr
JOIN canonical_nodes n ON n.logical_id = tr.logical_id
WHERE n.superseded_at IS NULL
  AND tr.logical_id != ?1
LIMIT {cap}"
        ),
        TraversalDirection::Incoming => format!(
            "WITH RECURSIVE
  traversal(logical_id, depth, visited) AS (
    SELECT n.logical_id, 0, char(30) || n.logical_id || char(30)
    FROM canonical_nodes n
    WHERE n.logical_id = ?1 AND n.superseded_at IS NULL
    UNION ALL
    SELECT e.from_id, t.depth + 1, t.visited || e.from_id || char(30)
    FROM traversal t
    JOIN canonical_edges e ON e.to_id = t.logical_id
    JOIN canonical_nodes next_n ON next_n.logical_id = e.from_id
      AND next_n.superseded_at IS NULL
    WHERE t.depth < ?2
      AND e.superseded_at IS NULL
      AND (e.t_invalid IS NULL OR datetime(e.t_invalid) > datetime('now'))
      AND instr(t.visited, char(30) || e.from_id || char(30)) = 0
    LIMIT {cte_cap}
  )
SELECT DISTINCT n.logical_id, n.kind, n.body, n.write_cursor
FROM traversal tr
JOIN canonical_nodes n ON n.logical_id = tr.logical_id
WHERE n.superseded_at IS NULL
  AND tr.logical_id != ?1
LIMIT {cap}"
        ),
        TraversalDirection::Both => format!(
            "WITH RECURSIVE
  traversal(logical_id, depth, visited) AS (
    SELECT n.logical_id, 0, char(30) || n.logical_id || char(30)
    FROM canonical_nodes n
    WHERE n.logical_id = ?1 AND n.superseded_at IS NULL
    UNION ALL
    SELECT
      CASE WHEN e.from_id = t.logical_id THEN e.to_id ELSE e.from_id END,
      t.depth + 1,
      t.visited || CASE WHEN e.from_id = t.logical_id THEN e.to_id ELSE e.from_id END || char(30)
    FROM traversal t
    JOIN canonical_edges e ON (e.from_id = t.logical_id OR e.to_id = t.logical_id)
    JOIN canonical_nodes next_n
      ON next_n.logical_id = CASE WHEN e.from_id = t.logical_id THEN e.to_id ELSE e.from_id END
      AND next_n.superseded_at IS NULL
    WHERE t.depth < ?2
      AND e.superseded_at IS NULL
      AND (e.t_invalid IS NULL OR datetime(e.t_invalid) > datetime('now'))
      AND instr(t.visited,
            char(30) || CASE WHEN e.from_id = t.logical_id THEN e.to_id ELSE e.from_id END || char(30)) = 0
    LIMIT {cte_cap}
  )
SELECT DISTINCT n.logical_id, n.kind, n.body, n.write_cursor
FROM traversal tr
JOIN canonical_nodes n ON n.logical_id = tr.logical_id
WHERE n.superseded_at IS NULL
  AND tr.logical_id != ?1
LIMIT {cap}"
        ),
    }
}

/// Build the BFS CTE SQL for `search_expand` — identical to `build_bfs_sql`
/// but the final SELECT uses `GROUP BY` + `MIN(tr.depth)` so that each
/// expanded node carries its actual BFS distance from the root.
///
/// Returns 5 columns: logical_id, kind, body, write_cursor, min_depth.
fn build_bfs_with_depth_sql() -> String {
    let cap = GRAPH_NEIGHBORS_HARD_CAP;
    let cte_cap = cap * cap; // same multigraph-safe headroom as build_bfs_sql
    format!(
        "WITH RECURSIVE
  traversal(logical_id, depth, visited) AS (
    SELECT n.logical_id, 0, char(30) || n.logical_id || char(30)
    FROM canonical_nodes n
    WHERE n.logical_id = ?1 AND n.superseded_at IS NULL
    UNION ALL
    SELECT
      CASE WHEN e.from_id = t.logical_id THEN e.to_id ELSE e.from_id END,
      t.depth + 1,
      t.visited || CASE WHEN e.from_id = t.logical_id THEN e.to_id ELSE e.from_id END || char(30)
    FROM traversal t
    JOIN canonical_edges e ON (e.from_id = t.logical_id OR e.to_id = t.logical_id)
    JOIN canonical_nodes next_n
      ON next_n.logical_id = CASE WHEN e.from_id = t.logical_id THEN e.to_id ELSE e.from_id END
      AND next_n.superseded_at IS NULL
    WHERE t.depth < ?2
      AND e.superseded_at IS NULL
      AND (e.t_invalid IS NULL OR datetime(e.t_invalid) > datetime('now'))
      AND instr(t.visited,
            char(30) || CASE WHEN e.from_id = t.logical_id THEN e.to_id ELSE e.from_id END || char(30)) = 0
    LIMIT {cte_cap}
  )
SELECT n.logical_id, n.kind, n.body, n.write_cursor, MIN(tr.depth) AS min_depth
FROM traversal tr
JOIN canonical_nodes n ON n.logical_id = tr.logical_id
WHERE n.superseded_at IS NULL
  AND tr.logical_id != ?1
GROUP BY n.logical_id
LIMIT {cap}"
    )
}

/// Slice 20 (G5) — execute a bounded BFS on the DEFERRED reader transaction.
/// Called inside the reader worker loop.
fn graph_neighbors_in_tx(
    reader: &mut Connection,
    root_logical_id: &str,
    depth: u32,
    direction: TraversalDirection,
) -> rusqlite::Result<Vec<NodeRecord>> {
    let sql = build_bfs_sql(direction);
    let tx = reader.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;
    let depth_i64 = depth as i64;
    let mut statement = tx.prepare(&sql)?;
    let rows = statement.query_map(params![root_logical_id, depth_i64], |row| {
        Ok(NodeRecord {
            logical_id: row.get(0)?,
            kind: row.get(1)?,
            body: row.get(2)?,
            write_cursor: row.get::<_, i64>(3)? as u64,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Slice 20 (G6) — resolve search hit `write_cursor`s to `logical_id`s, run
/// BFS for each root, and merge into a [`SearchExpandResult`]. Called inside
/// the reader worker loop on the DEFERRED reader transaction.
fn search_expand_in_tx(
    reader: &mut Connection,
    search_hits: &[SearchHit],
    depth: u32,
) -> rusqlite::Result<SearchExpandResult> {
    let tx = reader.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;

    // Step 1: resolve write_cursor → logical_id for each search hit.
    // Possible outcomes per hit:
    //   - None: no matching write_cursor in canonical_nodes (superseded) → drop.
    //   - Some(""):  row exists but logical_id IS NULL (anonymous node) or TextEdge hit
    //                → keep as valid search result, skip BFS expansion (empty sentinel).
    //   - Some(lid): active named node → keep; use as BFS root.
    let mut hit_logical_ids: Vec<Option<String>> = Vec::with_capacity(search_hits.len());
    {
        let mut node_stmt = tx.prepare(
            "SELECT logical_id FROM canonical_nodes
             WHERE write_cursor = ?1 AND superseded_at IS NULL
             LIMIT 1",
        )?;
        let mut edge_stmt = tx.prepare(
            "SELECT 1 FROM canonical_edges
             WHERE write_cursor = ?1 AND superseded_at IS NULL
             LIMIT 1",
        )?;
        for hit in search_hits {
            if hit.branch == SoftFallbackBranch::TextEdge {
                // Edge-body hit: verify the edge row is still active in THIS snapshot.
                // Stale edge hits (superseded between search and expansion) are dropped.
                let cursor_i64 = hit.id as i64;
                let active: Option<i32> =
                    edge_stmt.query_row([cursor_i64], |row| row.get(0)).optional()?;
                if active.is_some() {
                    hit_logical_ids.push(Some(String::new())); // sentinel: keep hit, skip BFS
                } else {
                    hit_logical_ids.push(None); // superseded edge: drop
                }
            } else {
                let cursor_i64 = hit.id as i64;
                // Returns Option<Option<String>>:
                //   None         → no row → superseded
                //   Some(None)   → row with NULL logical_id → anonymous node
                //   Some(Some(s)) → active named node
                let resolved = node_stmt
                    .query_row([cursor_i64], |row| row.get::<_, Option<String>>(0))
                    .optional()?;
                match resolved {
                    None => hit_logical_ids.push(None), // superseded: drop
                    Some(None) => hit_logical_ids.push(Some(String::new())), // anon: keep, skip BFS
                    Some(Some(lid)) => hit_logical_ids.push(Some(lid)), // named: keep + BFS root
                }
            }
        }
    }

    // Build a set of logical_ids present in the search hits (for deduplication).
    // Empty-string sentinels (TextEdge hits) are excluded — they are not real node ids.
    let hit_id_set: std::collections::HashSet<String> =
        hit_logical_ids.iter().filter_map(|id| id.clone()).filter(|s| !s.is_empty()).collect();

    // Step 2: for each root logical_id, run the BFS and collect expanded nodes.
    // A node already in `hit_id_set` is NOT added to `expanded`.
    // Use the depth-aware variant so each node reports its actual BFS distance.
    let bfs_sql = build_bfs_with_depth_sql();
    let depth_i64 = depth as i64;
    // nearest_hop: for each expanded logical_id track the minimum hop count
    // seen across ALL search-hit roots. A node reachable from multiple roots
    // at different depths must report the shortest distance (nearest root).
    let mut nearest_hop: std::collections::HashMap<String, (NodeRecord, u32)> =
        std::collections::HashMap::new();

    if depth > 0 {
        let mut bfs_stmt = tx.prepare(&bfs_sql)?;
        for root_id in hit_logical_ids.iter().flatten().filter(|s| !s.is_empty()) {
            let neighbor_rows = bfs_stmt.query_map(params![root_id, depth_i64], |row| {
                let node = NodeRecord {
                    logical_id: row.get(0)?,
                    kind: row.get(1)?,
                    body: row.get(2)?,
                    write_cursor: row.get::<_, i64>(3)? as u64,
                };
                let min_depth: i64 = row.get(4)?;
                Ok((node, min_depth as u32))
            })?;
            for row_result in neighbor_rows {
                let (node, hop_count) = row_result?;
                if hit_id_set.contains(&node.logical_id) {
                    // Already a search hit — skip (search score takes priority).
                    continue;
                }
                nearest_hop
                    .entry(node.logical_id.clone())
                    .and_modify(|(_, prev_hop)| {
                        if hop_count < *prev_hop {
                            *prev_hop = hop_count;
                        }
                    })
                    .or_insert((node, hop_count));
            }
        }
    }

    // Materialize expanded in insertion order (deterministic for tests).
    let mut expanded: Vec<(NodeRecord, u32)> = nearest_hop.into_values().collect();
    expanded.sort_by(|(a, _), (b, _)| a.logical_id.cmp(&b.logical_id));

    // Filter search_hits to only include those whose write_cursor resolved to an
    // active logical_id in THIS snapshot. Hits that were superseded between the
    // search phase and the expansion phase (the two-snapshot window) are dropped
    // rather than returned with stale data.
    let resolved_hits: Vec<SearchHit> = search_hits
        .iter()
        .zip(hit_logical_ids.iter())
        .filter_map(|(hit, lid)| lid.as_ref().map(|_| hit.clone()))
        .collect();

    // Build `all_logical_ids` = resolved search-hit logical_ids + expanded node ids.
    // Empty-string sentinels (TextEdge hits) are excluded — they are not real node ids.
    let mut all_logical_ids: Vec<String> =
        hit_logical_ids.into_iter().flatten().filter(|s| !s.is_empty()).collect();
    for (node, _) in &expanded {
        if !all_logical_ids.contains(&node.logical_id) {
            all_logical_ids.push(node.logical_id.clone());
        }
    }

    Ok(SearchExpandResult { search_hits: resolved_hits, expanded, all_logical_ids })
}

/// Slice 20 test seam — run `EXPLAIN QUERY PLAN` on the BFS CTE SQL and return
/// the plan `detail` column (column index 3) for each row. Used by
/// `explain_plan_uses_indexes` to assert index usage.
fn explain_graph_neighbors_in_tx(
    reader: &mut Connection,
    root_logical_id: &str,
    depth: u32,
    direction: TraversalDirection,
) -> rusqlite::Result<Vec<String>> {
    let bfs_sql = build_bfs_sql(direction);
    let explain_sql = format!("EXPLAIN QUERY PLAN {bfs_sql}");
    let tx = reader.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;
    let depth_i64 = depth as i64;
    let mut statement = tx.prepare(&explain_sql)?;
    // EXPLAIN QUERY PLAN returns rows: (id, parent, notused, detail).
    // We collect the `detail` column (index 3).
    let rows =
        statement.query_map(params![root_logical_id, depth_i64], |row| row.get::<_, String>(3))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
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

        // Fetch up to the in-flight budget in one SQL roundtrip and
        // enqueue them as a batch — previously this loop fetched ONE job
        // per cycle, which capped projection throughput at one row per
        // scanner/worker handshake regardless of how much work was queued
        // in canonical_nodes.
        let budget = {
            let state = match shared.state.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            PROJECTION_INFLIGHT_LIMIT.saturating_sub(state.active_jobs + state.queued_jobs)
        };
        let fetch_cap = budget.clamp(1, PROJECTION_SCAN_FETCH);
        match next_pending_projection_jobs(&connection, &in_flight, fetch_cap) {
            Ok(jobs) if !jobs.is_empty() => {
                if let Ok(mut state) = shared.state.lock() {
                    state.queued_jobs = state.queued_jobs.saturating_add(jobs.len());
                    for job in &jobs {
                        state.in_flight.insert(job.cursor);
                    }
                    state.pending_scan = true;
                    shared.state_cvar.notify_all();
                }
                if let Ok(mut queue) = shared.queue.lock() {
                    for job in jobs {
                        queue.push_back(job);
                    }
                    shared.queue_cvar.notify_all();
                }
            }
            Ok(_) => {}
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
    if ensure_vector_partition(&mut connection, shared.embedder_identity.dimension).is_err() {
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

        // EU-5f — isolate worker faults. A panic inside `embed()` (or the
        // commit) must not skip the state cleanup below, or `active_jobs`
        // would stay elevated forever and `wait_for_idle` / `drain` would
        // wedge into `EngineError::Scheduler` (Finding A). Mirrors the
        // reader pool's `LiveGuard` panic-safety. The local commit tx rolls
        // back on unwind, leaving the connection clean for reuse.
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_projection_jobs(&shared, &mut connection, &jobs);
        }))
        .is_err();
        if panicked {
            commit_projection_panic_failures(&shared, &mut connection, &jobs);
        }

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
    /// `blob` is the un-centered f32 BLOB persisted to
    /// `vector_default.embedding`. `bin_blob` is the (possibly centered)
    /// f32 BLOB fed to `vec_quantize_binary` for the sign-bit column.
    /// EU-5a2: `bin_blob == blob` unless the identity is MC-required
    /// AND a mean_vec is pinned.
    Success {
        cursor: u64,
        kind: String,
        blob: Vec<u8>,
        bin_blob: Vec<u8>,
    },
    Failure {
        cursor: u64,
        failure_code: &'static str,
    },
}

fn run_projection_jobs(
    shared: &ProjectionRuntimeShared,
    connection: &mut Connection,
    jobs: &[ProjectionJob],
) {
    let outcomes = embed_projection_batch(shared, jobs);
    let _ = commit_projection_outcomes(connection, &outcomes, shared);
}

/// Embed a whole commit-batch in ONE `embed_batch` call (amortizes per-call
/// overhead; saturates the GPU — minutes -> seconds on a full-corpus embed). The
/// batched path is the fast HAPPY path only; on ANY anomaly — no embedder, breaker
/// open, single job, batch timeout/failure, row-count or per-row dimension mismatch
/// — it falls back to the proven per-job [`run_projection_job`], which carries the
/// full retry + circuit-breaker + failure-isolation semantics. So batching can only
/// make the common case faster, never change correctness. A panic inside the batch
/// embed resume-unwinds exactly like the per-embed watchdog, so the worker's
/// batch-level `catch_unwind` records `ProjectionPanic` as before.
///
/// Batching is **opt-in** via `FATHOMDB_PROJECTION_BATCH=1` (`true`/`on` accepted).
/// It reshapes the PR-9 per-embed watchdog/breaker accounting into per-batch, so the
/// conservative DEFAULT keeps the proven per-job path — leaving every PR-9 safety
/// test (watchdog, serialization, circuit breaker) behaving exactly as before. The
/// eval GPU-embed run sets the env to get the batched-forward speedup (minutes ->
/// seconds), where the per-job fallback below still backs every error case.
fn projection_batch_enabled() -> bool {
    matches!(
        std::env::var("FATHOMDB_PROJECTION_BATCH").ok().as_deref(),
        Some("1") | Some("true") | Some("on")
    )
}

fn embed_projection_batch(
    shared: &ProjectionRuntimeShared,
    jobs: &[ProjectionJob],
) -> Vec<ProjectionOutcome> {
    let per_job = || jobs.iter().map(|job| run_projection_job(shared, job)).collect();

    let Some(embedder) = shared.embedder.as_ref() else {
        return per_job();
    };
    if jobs.len() < 2
        || shared.embed_circuit_open.load(Ordering::Relaxed)
        || !projection_batch_enabled()
    {
        return per_job();
    }

    let bodies: Vec<String> = jobs.iter().map(|job| job.body.clone()).collect();
    let embed_timeout = Duration::from_millis(shared.embed_timeout_ms.load(Ordering::Relaxed));
    // Each row keeps its single-embed budget worst-case (batch <= COMMIT_BATCH=16).
    let batch_timeout = embed_timeout.saturating_mul(jobs.len() as u32);

    let vectors = {
        // PR-9 — serialize the embedder call (ONE batched call at a time) and make
        // the breaker decision with the guard held (race-free vs other workers),
        // mirroring `run_projection_job`. The batch thread counts as one live embed.
        let _embed_permit =
            shared.embed_serialize.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let threshold = shared.embed_circuit_threshold.load(Ordering::Relaxed);
        if shared.embed_circuit_open.load(Ordering::Relaxed)
            || (threshold != 0 && shared.live_embed_threads.load(Ordering::Relaxed) >= threshold)
        {
            shared.embed_circuit_open.store(true, Ordering::Relaxed);
            return per_job();
        }
        match embed_batch_with_watchdog(
            embedder,
            &bodies,
            batch_timeout,
            &shared.live_embed_threads,
        ) {
            Ok(vectors) => vectors,
            // Timeout / failed / disconnected -> the per-job path retries each row
            // and engages the breaker exactly as before.
            Err(_) => return per_job(),
        }
    };

    if vectors.len() != jobs.len() {
        return per_job();
    }
    let mut outcomes = Vec::with_capacity(jobs.len());
    for (job, vector) in jobs.iter().zip(vectors) {
        if u32::try_from(vector.len()).unwrap_or(u32::MAX) != shared.embedder_identity.dimension {
            // A row came back wrong-dim: fall back per-job for the whole batch
            // (rare; keeps the dimension-mismatch failure path identical).
            return per_job();
        }
        // Mirror run_projection_job's post-embed step exactly: persisted f32 BLOB is
        // un-centered; centering for the binary column is finalized in
        // commit_projection_outcomes (so bin_blob == blob here).
        let blob = encode_vector_blob(&vector);
        let bin_blob = blob.clone();
        outcomes.push(ProjectionOutcome::Success {
            cursor: job.cursor,
            kind: job.kind.clone(),
            blob,
            bin_blob,
        });
    }
    outcomes
}

/// EU-5f — record every job in a panicked batch as a terminal projection
/// failure so the scheduler does not re-enqueue and re-panic on the same
/// cursors. Best-effort; runs after the worker caught a panic.
fn commit_projection_panic_failures(
    shared: &ProjectionRuntimeShared,
    connection: &mut Connection,
    jobs: &[ProjectionJob],
) {
    let outcomes: Vec<ProjectionOutcome> = jobs
        .iter()
        .map(|job| ProjectionOutcome::Failure {
            cursor: job.cursor,
            failure_code: "ProjectionPanic",
        })
        .collect();
    let _ = commit_projection_outcomes(connection, &outcomes, shared);
}

/// PR-9 — ADR-0.6.0-embedder-protocol **Invariant 5**: run one `embed()`
/// under a per-call deadline. A hung (non-panicking) embed would otherwise
/// park a projection worker forever — the EU-5f `catch_unwind` only catches
/// *panics*. On timeout we return `RuntimeEmbedderError::Timeout`, which the
/// caller's existing retry/failure path already handles.
///
/// Cancellation follows Invariant 5 exactly: the embed runs on a detached
/// thread that is allowed to *finish + discard* its result — never aborted
/// mid-call (there is no safe thread-cancel API). The caller (the projection
/// worker) holds `embed_serialize` across this call, but DROPS it the moment
/// this returns — including on timeout — so the abandoned detached thread
/// runs lock-free and a hung embed can neither hold the serialization guard
/// forever nor deadlock the pool. (The commit happens later, outside this
/// call, under the separate `commit_gate`.)
///
/// Panic-transparent: if `embed()` panics, the panic payload is captured on
/// the watchdog thread and resumed on the worker thread, so the existing
/// batch-level `catch_unwind` records `ProjectionPanic` exactly as before.
///
/// `live` counts embed threads currently alive: incremented before the spawn
/// and decremented by the thread when it finishes (even if its result was
/// abandoned on timeout). The caller reads it to bound the abandoned-thread
/// leak via the circuit breaker.
fn embed_with_watchdog(
    embedder: &Arc<dyn Embedder>,
    body: &str,
    timeout: Duration,
    live: &Arc<AtomicU64>,
) -> Result<Vec<f32>, RuntimeEmbedderError> {
    let (tx, rx) = mpsc::channel();
    let embedder = Arc::clone(embedder);
    let body = body.to_string();
    // Count this embed thread as live before spawning; the thread decrements
    // when it finishes, whether or not its result is still wanted.
    live.fetch_add(1, Ordering::Relaxed);
    let live_thread = Arc::clone(live);
    thread::spawn(move || {
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| embedder.embed(&body)));
        // The receiver may already be gone (this call timed out): an async
        // channel send never blocks, and a send to a dropped receiver is a
        // no-op error we deliberately ignore — the result is discarded.
        let _ = tx.send(outcome);
        live_thread.fetch_sub(1, Ordering::Relaxed);
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(result)) => result,
        Ok(Err(panic_payload)) => std::panic::resume_unwind(panic_payload),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(RuntimeEmbedderError::Timeout),
        // The watchdog thread dropped its sender without sending — should not
        // happen (panics are captured above), but treat as a failed embed so
        // the retry/failure path engages rather than silently succeeding.
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(RuntimeEmbedderError::Failed {
            message: "embed watchdog thread dropped its result channel".to_string(),
        }),
    }
}

/// Batch sibling of [`embed_with_watchdog`]: run ONE `embed_batch` on a detached,
/// timeout-bounded thread. Same Invariant-5 cancellation contract (the thread is
/// allowed to finish + discard on timeout, never aborted mid-call), same
/// panic-transparency (a panic is resumed on the caller so the worker's batch-level
/// `catch_unwind` records `ProjectionPanic`), same `live` accounting (one batch
/// thread = one live embed, bounding the abandoned-thread leak via the breaker).
fn embed_batch_with_watchdog(
    embedder: &Arc<dyn Embedder>,
    bodies: &[String],
    timeout: Duration,
    live: &Arc<AtomicU64>,
) -> Result<Vec<Vec<f32>>, RuntimeEmbedderError> {
    let (tx, rx) = mpsc::channel();
    let embedder = Arc::clone(embedder);
    let bodies = bodies.to_vec();
    live.fetch_add(1, Ordering::Relaxed);
    let live_thread = Arc::clone(live);
    thread::spawn(move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let refs: Vec<&str> = bodies.iter().map(String::as_str).collect();
            embedder.embed_batch(&refs)
        }));
        let _ = tx.send(outcome);
        live_thread.fetch_sub(1, Ordering::Relaxed);
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(result)) => result,
        Ok(Err(panic_payload)) => std::panic::resume_unwind(panic_payload),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(RuntimeEmbedderError::Timeout),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(RuntimeEmbedderError::Failed {
            message: "embed batch watchdog thread dropped its result channel".to_string(),
        }),
    }
}

fn run_projection_job(shared: &ProjectionRuntimeShared, job: &ProjectionJob) -> ProjectionOutcome {
    // PR-9 — embed circuit breaker (see `embed_circuit_open`). Once abandoned
    // (timed-out) embed threads have piled up to the threshold the embedder is
    // treated as broken; fail subsequent jobs fast WITHOUT attempting an embed,
    // so a wedged embedder cannot keep leaking abandoned watchdog threads. This
    // entry check is the fast path; the latch decision itself is made under the
    // embed guard below (race-free against other workers).
    if shared.embed_circuit_open.load(Ordering::Relaxed) {
        return ProjectionOutcome::Failure { cursor: job.cursor, failure_code: "EmbedderError" };
    }
    let delays = shared.retry_delays_ms.lock().map(|delays| delays.clone()).unwrap_or_default();
    let mut last_code = "EmbedderError";
    for (attempt, delay_ms) in std::iter::once(0_u64).chain(delays.iter().copied()).enumerate() {
        if attempt > 0 {
            if shared.state.lock().map(|state| state.stopping).unwrap_or(true) {
                return ProjectionOutcome::Failure { cursor: job.cursor, failure_code: last_code };
            }
            thread::sleep(Duration::from_millis(delay_ms));
        }
        // PR-9 — re-check the breaker on every attempt, not just at entry:
        // another worker (or an earlier attempt of this job) may have latched
        // it while we were sleeping between retries. Bail before spawning yet
        // another timeout-bound watchdog thread, so the abandoned-thread leak
        // stays bounded even on the multi-retry path.
        if shared.embed_circuit_open.load(Ordering::Relaxed) {
            return ProjectionOutcome::Failure { cursor: job.cursor, failure_code: last_code };
        }
        // PR-9 / ADR-0.6.0 Invariant 5 — every embed runs under the per-call
        // watchdog deadline so a hung embed surfaces Timeout instead of
        // parking this worker forever.
        let embed_timeout = Duration::from_millis(shared.embed_timeout_ms.load(Ordering::Relaxed));
        let vector = match shared.embedder.as_ref() {
            Some(embedder) => {
                // PR-9 — serialize the embed call engine-side (see
                // `embed_serialize`): the shared embedder is invoked one call
                // at a time, for SAFETY with arbitrary caller-supplied
                // embedders (throughput is ~neutral on the candle default).
                // The guard is held across the watchdog call and released
                // here, so commit/IO below stays parallel and a timed-out
                // embed frees it. The guard owns no data; a panic-resumed
                // embed poisons it, so we recover the inner guard rather than
                // wedge the whole pool.
                let _embed_permit =
                    shared.embed_serialize.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                // PR-9 — breaker decision, made WITH the guard held so it is
                // race-free against other workers: if abandoned embed threads
                // from earlier timeouts have piled up to the threshold, latch
                // the breaker and fail fast WITHOUT spawning another one. The
                // live count is checked here (also covers a breaker latched by
                // another worker while we were queued on the lock), bounding
                // the abandoned-thread leak to ~threshold regardless of whether
                // the embedder hangs always or only intermittently.
                let threshold = shared.embed_circuit_threshold.load(Ordering::Relaxed);
                if shared.embed_circuit_open.load(Ordering::Relaxed)
                    || (threshold != 0
                        && shared.live_embed_threads.load(Ordering::Relaxed) >= threshold)
                {
                    shared.embed_circuit_open.store(true, Ordering::Relaxed);
                    return ProjectionOutcome::Failure {
                        cursor: job.cursor,
                        failure_code: last_code,
                    };
                }
                match embed_with_watchdog(
                    embedder,
                    &job.body,
                    embed_timeout,
                    &shared.live_embed_threads,
                ) {
                    Ok(vector) => vector,
                    Err(RuntimeEmbedderError::Timeout) => {
                        // The embed thread is now abandoned (still counted in
                        // live_embed_threads until it returns); the breaker
                        // check above caps how many can accumulate.
                        last_code = "EmbedderError";
                        continue;
                    }
                    Err(RuntimeEmbedderError::Failed { .. }) => {
                        last_code = "EmbedderError";
                        continue;
                    }
                }
            }
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
        // EU-5a2 mean-centering apply path (projection write side). The
        // f32 BLOB persisted is ALWAYS un-centered; `bin_blob` carries
        // the (possibly centered) f32 fed to `vec_quantize_binary`. The
        // centering decision is finalized in `commit_projection_outcomes`
        // where the writer connection is in-hand and the read of
        // `_fathomdb_embedder_profiles.mean_vec` is in the same tx as
        // the INSERT. NoopEmbedder (EU-5a2's only live identity) is not
        // MC-required, so `bin_blob == blob` throughout EU-5a2.
        let bin_blob = blob.clone();
        return ProjectionOutcome::Success {
            cursor: job.cursor,
            kind: job.kind.clone(),
            blob,
            bin_blob,
        };
    }

    ProjectionOutcome::Failure { cursor: job.cursor, failure_code: last_code }
}

fn next_pending_projection_jobs(
    connection: &Connection,
    in_flight: &BTreeSet<u64>,
    max_jobs: usize,
) -> rusqlite::Result<Vec<ProjectionJob>> {
    if max_jobs == 0 {
        return Ok(Vec::new());
    }
    let cursor = load_projection_cursor(connection)?;
    // Over-fetch by `in_flight.len()` so the post-filter still returns
    // up to `max_jobs` after skipping cursors already in-flight.
    let sql_limit = max_jobs.saturating_add(in_flight.len()).min(256);
    // G11 (Slice 15) — UNION extends the projection queue to include edge bodies.
    // Edge bodies use kind `'edge_fact'` so `resolve_source_type` maps them to
    // `source_type = 'edge_fact'` in `vector_default` (partition correctness).
    // The UNION is ordered by write_cursor so projection proceeds in
    // insertion order across nodes and edges.
    let sql = format!(
        "SELECT write_cursor, kind, body FROM (
             SELECT canonical_nodes.write_cursor, canonical_nodes.kind, canonical_nodes.body
             FROM canonical_nodes
             JOIN _fathomdb_vector_kinds
               ON _fathomdb_vector_kinds.kind = canonical_nodes.kind
             LEFT JOIN _fathomdb_projection_terminal
               ON _fathomdb_projection_terminal.write_cursor = canonical_nodes.write_cursor
             WHERE canonical_nodes.write_cursor > ?1
               AND _fathomdb_projection_terminal.write_cursor IS NULL

             UNION ALL

             SELECT canonical_edges.write_cursor, 'edge_fact', canonical_edges.body
             FROM canonical_edges
             JOIN _fathomdb_vector_kinds
               ON _fathomdb_vector_kinds.kind = 'edge_fact'
             LEFT JOIN _fathomdb_projection_terminal
               ON _fathomdb_projection_terminal.write_cursor = canonical_edges.write_cursor
             WHERE canonical_edges.write_cursor > ?1
               AND canonical_edges.body IS NOT NULL
               AND canonical_edges.superseded_at IS NULL
               AND _fathomdb_projection_terminal.write_cursor IS NULL
         ) ORDER BY write_cursor
         LIMIT {sql_limit}"
    );
    let mut statement = connection.prepare_cached(&sql)?;
    let rows = statement.query_map([cursor], |row| {
        Ok(ProjectionJob { cursor: row.get(0)?, kind: row.get(1)?, body: row.get(2)? })
    })?;
    let mut jobs = Vec::with_capacity(max_jobs);
    for row in rows {
        let job = row?;
        if in_flight.contains(&job.cursor) {
            continue;
        }
        jobs.push(job);
        if jobs.len() >= max_jobs {
            break;
        }
    }
    Ok(jobs)
}

fn database_has_pending_projection_work(path: &Path) -> rusqlite::Result<bool> {
    let connection = open_runtime_connection(path)?;
    let cursor = load_projection_cursor(&connection)?;
    // Check canonical_nodes for un-projected work.
    let has_node_work: bool = connection
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
        })?;
    if has_node_work {
        return Ok(true);
    }
    // G11 (Slice 15) fix-1 [P2] — also check canonical_edges for edge bodies
    // that were not projected before the engine closed. Without this check,
    // drain() returns idle while edge vectors remain unembedded on reopen.
    // fix-31 [P2]: exclude superseded edges from the pending check so the
    // scheduler does not pick up stale tombstoned rows as projection work.
    connection
        .query_row(
            "SELECT 1
             FROM canonical_edges ce
             LEFT JOIN _fathomdb_projection_terminal pt
               ON pt.write_cursor = ce.write_cursor
             WHERE ce.body IS NOT NULL
               AND ce.superseded_at IS NULL
               AND pt.write_cursor IS NULL
             LIMIT 1",
            [],
            |_row| Ok(true),
        )
        .or_else(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => Ok(false),
            _ => Err(err),
        })
}

struct CanonicalNodeRow {
    cursor: u64,
    kind: String,
    body: String,
}

/// 0.8.0 Slice 5 (G1) — re-tokenize `search_index` from the canonical source
/// rows after the step-11 tokenizer-default upgrade drops + recreates the FTS5
/// virtual table. Projection-only: it reads `canonical_nodes` (the source of
/// truth, untouched) and rewrites the FTS shadow; it performs **no**
/// source-record migration. Every canonical node already carries an FTS row at
/// write time (the projection-time INSERT is unconditional), so reinserting
/// every node exactly reproduces the prior index content under the new
/// tokenizer. Runs in a single transaction on the writer connection before
/// readers spawn.
///
/// Crash-retryable (fix-1): the reindex and its durable completion marker
/// (`SEARCH_INDEX_TOKENIZER_REPROJECT_MARKER_KEY` in `_fathomdb_open_state`)
/// commit together in ONE `BEGIN IMMEDIATE…COMMIT`. A crash before the commit
/// rolls both back, leaving no marker; the next open re-runs. A crash after
/// the commit finds the marker present and skips. Idempotent.
fn reproject_search_index_after_tokenizer_upgrade(connection: &Connection) -> rusqlite::Result<()> {
    let rows = canonical_node_rows(connection)?;
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        connection.execute("DELETE FROM search_index", [])?;
        {
            let mut statement = connection
                .prepare("INSERT INTO search_index(body, kind, write_cursor) VALUES(?1, ?2, ?3)")?;
            for row in &rows {
                statement.execute(params![row.body, row.kind, row.cursor])?;
            }
        }
        connection.execute(
            "INSERT INTO _fathomdb_open_state(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SEARCH_INDEX_TOKENIZER_REPROJECT_MARKER_KEY, "1"],
        )?;
        Ok(())
    })();
    match result {
        Ok(()) => connection.execute_batch("COMMIT"),
        Err(err) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

/// 0.8.0 Slice 5 (G1) fix-1 — has the post-tokenizer-upgrade re-tokenization
/// committed durably on this DB? Keys off the
/// `SEARCH_INDEX_TOKENIZER_REPROJECT_MARKER_KEY` row written inside the reindex
/// transaction; its absence on a v11 DB means the reindex never committed
/// (fresh-after-step-11 or crash-in-window) and must (re-)run.
///
/// A MISSING `_fathomdb_open_state` table is reported as "complete" (skip the
/// reproject): that table is created by migration step 1, so its absence means
/// the DB never ran our migrations (e.g. a synthetic DB whose `user_version`
/// was stamped to 11 by hand, or a legacy/foreign shape). Such DBs are
/// rejected by the downstream embedder-identity/integrity probes; the reproject
/// must not run — and must not mask those errors — on them. On a genuinely
/// migrated DB the table always exists, so the crash-repair path is unaffected.
fn search_index_tokenizer_reproject_complete(connection: &Connection) -> rusqlite::Result<bool> {
    match connection.query_row(
        "SELECT value FROM _fathomdb_open_state WHERE key = ?1",
        [SEARCH_INDEX_TOKENIZER_REPROJECT_MARKER_KEY],
        |row| row.get::<_, String>(0),
    ) {
        Ok(value) => Ok(value == "1"),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
        Err(rusqlite::Error::SqliteFailure(_, Some(ref message)))
            if message.contains("no such table") =>
        {
            Ok(true)
        }
        Err(err) => Err(err),
    }
}

fn canonical_node_rows(connection: &Connection) -> rusqlite::Result<Vec<CanonicalNodeRow>> {
    let mut statement = connection
        .prepare("SELECT write_cursor, kind, body FROM canonical_nodes ORDER BY write_cursor")?;
    let rows = statement.query_map([], |row| {
        Ok(CanonicalNodeRow {
            cursor: row.get::<_, u64>(0)?,
            kind: row.get::<_, String>(1)?,
            body: row.get::<_, String>(2)?,
        })
    })?;
    rows.collect()
}

#[cfg(feature = "operator")]
fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(hex_nibble(byte >> 4));
        out.push(hex_nibble(byte & 0x0f));
    }
    out
}

#[cfg(feature = "operator")]
fn hex_nibble(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + value - 10) as char,
        _ => unreachable!(),
    }
}

#[cfg(feature = "operator")]
fn physical_section(connection: &Connection, full: bool) -> Section {
    let mut findings = Vec::new();
    if let Err(err) = connection.query_row("PRAGMA page_count", [], |row| row.get::<_, i64>(0)) {
        findings.push(Finding {
            code: "E_CORRUPT_HEADER",
            stage: "PhysicalProbe",
            locator: locator_from_rusqlite_error(&err),
            doc_anchor: "design/recovery.md#header-malformed",
            detail: format!("page_count probe failed: {err}"),
        });
    }
    if full {
        match collect_integrity_check_findings(connection) {
            Ok(rows) => findings.extend(rows),
            Err(err) => findings.push(Finding {
                code: "E_CORRUPT_INTEGRITY_CHECK",
                stage: "IntegrityCheck",
                locator: locator_from_rusqlite_error(&err),
                doc_anchor: "design/recovery.md#integrity-check-full-findings",
                detail: format!("PRAGMA integrity_check failed: {err}"),
            }),
        }
    }
    if findings.is_empty() {
        Section::Clean
    } else {
        Section::Findings(findings)
    }
}

#[cfg(feature = "operator")]
fn logical_section(connection: &Connection) -> Section {
    let mut findings = Vec::new();
    if let Err(err) = connection.query_row("PRAGMA schema_version", [], |row| row.get::<_, i64>(0))
    {
        findings.push(Finding {
            code: "E_CORRUPT_SCHEMA",
            stage: "SchemaProbe",
            locator: locator_from_rusqlite_error(&err),
            doc_anchor: "design/recovery.md#schema-inconsistent",
            detail: format!("schema_version probe failed: {err}"),
        });
    }
    match connection.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0)) {
        Ok(0) => findings.push(Finding {
            code: "E_CORRUPT_SCHEMA",
            stage: "SchemaProbe",
            locator: CorruptionLocator::MigrationStep { from: 0, to: 0 },
            doc_anchor: "design/recovery.md#schema-inconsistent",
            detail: "user_version is zero".to_string(),
        }),
        Ok(_) => {}
        Err(err) => findings.push(Finding {
            code: "E_CORRUPT_SCHEMA",
            stage: "SchemaProbe",
            locator: locator_from_rusqlite_error(&err),
            doc_anchor: "design/recovery.md#schema-inconsistent",
            detail: format!("user_version probe failed: {err}"),
        }),
    }
    if findings.is_empty() {
        Section::Clean
    } else {
        Section::Findings(findings)
    }
}

#[cfg(feature = "operator")]
fn semantic_section(connection: &Connection) -> Section {
    match load_default_profile(connection) {
        Ok(_) => Section::Clean,
        Err(rusqlite::Error::QueryReturnedNoRows) => Section::Findings(vec![Finding {
            code: "E_CORRUPT_EMBEDDER_IDENTITY",
            stage: "EmbedderIdentity",
            locator: CorruptionLocator::OpaqueSqliteError { sqlite_extended_code: 0 },
            doc_anchor: "design/recovery.md#embedder-identity-drift",
            detail: "default embedder profile row is missing".to_string(),
        }]),
        Err(err) => Section::Findings(vec![Finding {
            code: "E_CORRUPT_EMBEDDER_IDENTITY",
            stage: "EmbedderIdentity",
            locator: locator_from_rusqlite_error(&err),
            doc_anchor: "design/recovery.md#embedder-identity-drift",
            detail: format!("default embedder profile probe failed: {err}"),
        }]),
    }
}

#[cfg(feature = "operator")]
fn collect_integrity_check_findings(connection: &Connection) -> rusqlite::Result<Vec<Finding>> {
    let mut statement = connection.prepare("PRAGMA integrity_check")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut findings = Vec::new();
    for row in rows {
        let message = row?;
        if message == "ok" {
            continue;
        }
        findings.push(Finding {
            code: "E_CORRUPT_INTEGRITY_CHECK",
            stage: "IntegrityCheck",
            locator: CorruptionLocator::OpaqueSqliteError {
                sqlite_extended_code: rusqlite::ffi::SQLITE_CORRUPT,
            },
            doc_anchor: "design/recovery.md#integrity-check-full-findings",
            detail: message,
        });
    }
    Ok(findings)
}

#[cfg(feature = "operator")]
fn locator_from_rusqlite_error(err: &rusqlite::Error) -> CorruptionLocator {
    let extended = err.sqlite_error().map(|inner| inner.extended_code).unwrap_or(0);
    CorruptionLocator::OpaqueSqliteError { sqlite_extended_code: extended }
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
    shared: &ProjectionRuntimeShared,
) -> rusqlite::Result<()> {
    let embedder_identity = &shared.embedder_identity;
    let mc = identity_requires_mean_centering(embedder_identity);
    // EU-5f — serialize the whole commit across workers so the at-pin
    // re-quantize sees a totally-ordered history (see `commit_gate`).
    let _gate = shared.commit_gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let tx = connection.transaction()?;
    // EU-5a2/EU-5f — the live pinned mean. Read once at the top; may pin
    // mid-batch (set to `Some` after a threshold-crossing row below).
    let mut current_mean: Option<Vec<f32>> = if mc {
        tx.query_row(
            "SELECT mean_vec FROM _fathomdb_embedder_profiles WHERE profile = 'default'",
            [],
            |row| row.get::<_, Option<Vec<u8>>>(0),
        )
        .ok()
        .flatten()
        .map(|bytes| decode_vector_blob(&bytes))
    } else {
        None
    };
    let mut staged_events: Vec<EmbedderEvent> = Vec::new();
    for outcome in outcomes {
        match outcome {
            ProjectionOutcome::Success { cursor, kind, blob, bin_blob } => {
                if terminal_state_for_cursor(&tx, *cursor)?.is_some() {
                    continue;
                }
                // EU-5f — feed the streaming accumulator and decide the pin
                // atomically under the accumulator lock (add -> count ->
                // take), so exactly one row/worker can cross the threshold.
                // Only while MC-required and not yet pinned.
                let pin_mean: Option<Vec<f32>> = if mc && current_mean.is_none() {
                    let mut acc = shared.mean_accumulator.lock().unwrap_or_else(|p| p.into_inner());
                    match acc.as_mut() {
                        Some(a) => {
                            a.add(&decode_vector_blob(bin_blob));
                            if a.count() >= MEAN_VEC_PIN_THRESHOLD {
                                let mean = a.materialize();
                                *acc = None;
                                Some(mean)
                            } else {
                                None
                            }
                        }
                        None => None,
                    }
                } else {
                    None
                };

                let source_type = resolve_source_type(kind).map_err(|_| {
                    rusqlite::Error::SqliteFailure(
                        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT),
                        Some(format!("unknown kind for source_type mapping: {kind}")),
                    )
                })?;
                let now_unix =
                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
                        as i64;
                tx.execute(
                    "INSERT OR IGNORE INTO _fathomdb_vector_rows(rowid, kind, write_cursor) VALUES(?1, ?2, ?3)",
                    params![cursor, kind, cursor],
                )?;
                // EU-5a2/EU-5f — sign-quant input is the mean-subtracted
                // vector iff a mean is live (`current_mean`); otherwise the
                // un-centered `bin_blob`. A row inserted just before the
                // crossing is centered retroactively by the re-quantize
                // pass below.
                let centered_blob: Vec<u8> = match &current_mean {
                    Some(mean) if mean.len() * 4 == bin_blob.len() => {
                        encode_vector_blob(&subtract_mean(&decode_vector_blob(bin_blob), mean))
                    }
                    _ => bin_blob.clone(),
                };
                tx.execute(
                    // Slice 10 / G10 — `status` ships the empty-string sentinel
                    // (vec0 TEXT metadata is NOT NULL-able); no real population
                    // source yet (reserved-gap candidate 13).
                    "INSERT OR IGNORE INTO vector_default(
                        rowid, embedding, embedding_bin, source_type, kind, created_at, status
                     ) VALUES(?1, ?2, vec_quantize_binary(?3), ?4, ?5, ?6, '')",
                    params![cursor, blob, centered_blob, source_type, kind, now_unix],
                )?;
                record_projection_terminal(&tx, *cursor, "up_to_date")?;

                // EU-5f — this row crossed the threshold: pin the mean and
                // re-quantize every row written so far (incl. earlier rows
                // in this same tx, which are visible to the SELECT) within
                // the same transaction so the pin is atomic.
                if let Some(mean) = pin_mean {
                    tx.execute(
                        "UPDATE _fathomdb_embedder_profiles SET mean_vec = ?1 WHERE profile = 'default'",
                        params![encode_vector_blob(&mean)],
                    )?;
                    let rows: Vec<(i64, Vec<u8>)> = {
                        let mut statement = tx.prepare(
                            "SELECT rowid, embedding FROM vector_default ORDER BY rowid",
                        )?;
                        let mapped = statement.query_map([], |row| {
                            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
                        })?;
                        let mut out = Vec::new();
                        for r in mapped {
                            out.push(r?);
                        }
                        out
                    };
                    let (doc_count, _) =
                        run_pin_and_requantize_pass(&tx, &rows, &mean).map_err(|_| {
                            rusqlite::Error::SqliteFailure(
                                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
                                Some("mean-centering re-quantize pass failed".to_string()),
                            )
                        })?;
                    staged_events.push(EmbedderEvent::MeanVecPinned {
                        dim: u32::try_from(mean.len()).unwrap_or(u32::MAX),
                        doc_count,
                    });
                    current_mean = Some(mean);
                }
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
    // 0.7.2 PR-2bc S2 — the AUTOMATIC in-ingest drift detector (EWMA recent
    // mean + cos-threshold + debounce + 200k cap + `MeanRecomputeDeferred`)
    // was CARVED OUT and DEFERRED to 0.8.x; its recall premise was refuted
    // (the mean is a non-lever) and the benefit is unmeasured. The mean is
    // refreshed only on demand via `Engine::recompute_mean` (the
    // `doctor recompute-mean` verb). See `dev/design/embedder.md` §0.3 and
    // `dev/plans/prompts/0.8.x-auto-mean-drift-DEFERRED.md`. Nothing here
    // mutates `mean_vec` after the initial pin.

    advance_projection_cursor(&tx)?;
    tx.commit()?;
    // EU-5f — publish MeanVecPinned only after the pin tx is durable, so a
    // rolled-back pin never emits a spurious event.
    if !staged_events.is_empty() {
        if let Ok(mut events) = shared.pending_events.lock() {
            events.extend(staged_events);
        }
    }
    Ok(())
}

/// EU-5f — open-time recovery pin (`dev/design/embedder.md` §0.3, Hazard 4).
/// Derives the corpus mean from the existing un-centered `vector_default`
/// rows, pins it, and re-quantizes every row, all in one transaction on the
/// single-threaded open connection (no workers running yet, so no gate is
/// needed). Called only when MC is required, no mean is pinned, and the row
/// count already meets the threshold.
fn recover_mean_vec_pin(
    connection: &mut Connection,
    identity: &EmbedderIdentity,
) -> Result<(), EngineError> {
    let tx = connection.transaction().map_err(|_| EngineError::Storage)?;
    recompute_mean_in_tx(&tx, identity)?;
    tx.commit().map_err(|_| EngineError::Storage)?;
    Ok(())
}

/// 0.7.2 PR-2b — shared mean (re)compute core, run INSIDE the caller's
/// transaction. Derives the FULL-corpus mean from the un-centered
/// `vector_default.embedding` BLOBs, writes `mean_vec`, and re-quantizes
/// EVERY row via the existing [`run_pin_and_requantize_pass`] so no row is
/// left under a stale centering.
///
/// This generalizes the EU-5f open-time recovery pin: it has NO "no mean
/// pinned yet" guard, so it equally serves the FIRST pin (recovery) and a
/// REFRESH of an already-pinned mean (PR-2b drift / `doctor recompute-mean`).
/// The caller owns the transaction boundary, which is what makes a fault
/// between the `mean_vec` UPDATE and re-quantize completion roll back
/// wholesale (`dev/design/embedder.md` §0.5 atomicity). It does NOT publish
/// any event — that is the caller's job, strictly post-durable-commit.
fn recompute_mean_in_tx(
    tx: &rusqlite::Transaction<'_>,
    identity: &EmbedderIdentity,
) -> Result<MeanRecomputeReport, EngineError> {
    recompute_mean_in_tx_inner(tx, identity, false)
}

/// 0.7.2 PR-2b — recompute core with an optional fault-injection point. The
/// `fail_after_mean_update` flag (debug builds only, set via a test seam)
/// errors AFTER the `mean_vec` UPDATE but BEFORE the re-quantize completes,
/// so the caller's tx rolls back the partial recentering.
fn recompute_mean_in_tx_inner(
    tx: &rusqlite::Transaction<'_>,
    identity: &EmbedderIdentity,
    fail_after_mean_update: bool,
) -> Result<MeanRecomputeReport, EngineError> {
    let started = Instant::now();
    let dim = identity.dimension as usize;
    // The previously-pinned mean (if any) is read first so we can report
    // the pre-recompute drift cosine.
    let old_mean = read_pinned_mean_vec(tx, identity.dimension)?;
    let rows: Vec<(i64, Vec<u8>)> = {
        let mut statement = tx
            .prepare("SELECT rowid, embedding FROM vector_default ORDER BY rowid")
            .map_err(|_| EngineError::Storage)?;
        let mapped = statement
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)))
            .map_err(|_| EngineError::Storage)?;
        let mut out = Vec::new();
        for r in mapped {
            out.push(r.map_err(|_| EngineError::Storage)?);
        }
        out
    };
    let mut accumulator = MeanAccumulator::new(dim);
    for (_rowid, blob) in &rows {
        if blob.len() != dim * 4 {
            return Err(EngineError::Storage);
        }
        accumulator.add(&decode_vector_blob(blob));
    }
    let old_doc_count = accumulator.count();
    let mean = accumulator.materialize();
    let drift_cos_before = match &old_mean {
        Some(old) => cosine_similarity(&mean, old),
        None => 1.0,
    };
    tx.execute(
        "UPDATE _fathomdb_embedder_profiles SET mean_vec = ?1 WHERE profile = 'default'",
        params![encode_vector_blob(&mean)],
    )
    .map_err(|_| EngineError::Storage)?;
    if fail_after_mean_update {
        // Injected fault: bail before re-quantizing so the caller's tx
        // rolls back the `mean_vec` UPDATE too (crash-atomicity proof).
        return Err(EngineError::Storage);
    }
    let (doc_count, _) = run_pin_and_requantize_pass(tx, &rows, &mean)?;
    Ok(MeanRecomputeReport {
        dim: u32::try_from(dim).unwrap_or(u32::MAX),
        old_doc_count,
        doc_count_requantized: doc_count,
        drift_cos_before,
        mean_was_pinned: old_mean.is_some(),
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
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

/// 0.7.0 perf-experiments hook: process-start `sqlite3_config` calls.
/// Runs exactly once per process; must precede any `Connection::open`.
/// Gated on `FATHOMDB_PERF_EXPERIMENTS=1`. Each individual config
/// option is opt-in via its own env var so unrelated experiments do
/// not implicitly co-fire.
///
/// Currently supports:
/// - `FATHOMDB_PERF_SQLITE_MEMSTATUS_OFF=1`:
///   `sqlite3_config(SQLITE_CONFIG_MEMSTATUS, 0)` — drops the
///   allocator stats locking surface (whitepaper § 7.4). Composes
///   with other levers; small payoff alone.
///
/// Pattern: shutdown → config → initialize, mirroring B.1 attempt #2
/// (`d448263`, reverted). The captured rc for each config call is
/// logged to stderr so experiments can verify the call took effect.
fn init_perf_experiments_runtime() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if std::env::var_os("FATHOMDB_PERF_EXPERIMENTS").is_none() {
            return;
        }
        let memstatus_off =
            std::env::var_os("FATHOMDB_PERF_SQLITE_MEMSTATUS_OFF").is_some_and(|v| v == "1");
        // FATHOMDB_PERF_SQLITE_PAGECACHE=<page_size_bytes>:<page_count>
        // E.g. "4096:5000" => pre-allocate 4096 B × 5000 pages = 20 MB
        // global page-cache backing. SQLite distributes this across
        // connections; reduces global allocator pressure for page
        // cache fills.
        let pagecache = std::env::var("FATHOMDB_PERF_SQLITE_PAGECACHE").ok();
        // FATHOMDB_PERF_SQLITE_PCACHE2=1 installs the per-instance
        // custom page-cache allocator (pcache2.rs). Targets AC-020
        // residual contention on the default pcache1 mutex.
        let pcache2_on =
            std::env::var_os("FATHOMDB_PERF_SQLITE_PCACHE2").is_some_and(|v| v == "1");
        if !memstatus_off && pagecache.is_none() && !pcache2_on {
            return;
        }
        // SAFETY: sqlite3_shutdown / sqlite3_initialize are documented
        // as safe to call before any other SQLite API; sqlite3_config
        // must be called between shutdown and initialize. We pre-empt
        // rusqlite's lazy first-call sqlite3_initialize via this
        // explicit shutdown-then-config-then-initialize sequence,
        // identical to B.1 attempt #2's plumbing.
        unsafe {
            let rc_shutdown = rusqlite::ffi::sqlite3_shutdown();
            let rc_memstatus = if memstatus_off {
                rusqlite::ffi::sqlite3_config(rusqlite::ffi::SQLITE_CONFIG_MEMSTATUS, 0_i32)
            } else {
                -1
            };
            // SQLITE_CONFIG_PAGECACHE = 7 per sqlite3.h. With buffer=NULL,
            // SQLite allocates the backing memory itself but still
            // partitions it for use as the page-cache pool.
            let rc_pagecache = if let Some(spec) = pagecache.as_ref() {
                let mut parts = spec.split(':');
                let sz = parts.next().and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
                let n = parts.next().and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
                if sz > 0 && n > 0 {
                    rusqlite::ffi::sqlite3_config(
                        7, // SQLITE_CONFIG_PAGECACHE
                        std::ptr::null_mut::<std::ffi::c_void>(),
                        sz,
                        n,
                    )
                } else {
                    eprintln!(
                        "perf-experiment: bad FATHOMDB_PERF_SQLITE_PAGECACHE spec '{spec}' (expect '<bytes>:<count>')"
                    );
                    -1
                }
            } else {
                -1
            };
            let rc_pcache2 = if pcache2_on {
                // SQLITE_CONFIG_PCACHE2 = 18 per sqlite3.h. The methods
                // table must outlive the SQLite engine; we pass a
                // pointer to our static.
                rusqlite::ffi::sqlite3_config(
                    rusqlite::ffi::SQLITE_CONFIG_PCACHE2,
                    &raw const pcache2::PCACHE2_METHODS.0,
                )
            } else {
                -1
            };
            let rc_init = rusqlite::ffi::sqlite3_initialize();
            eprintln!(
                "perf-experiment: runtime-config rcs shutdown={rc_shutdown} \
                 memstatus={rc_memstatus} pagecache={rc_pagecache} pcache2={rc_pcache2} \
                 initialize={rc_init} (0=SQLITE_OK; 21=SQLITE_MISUSE; -1=not configured)"
            );
        }
    });
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
    // `SELECT COUNT(*) FROM sqlite_schema` forces a full traversal of the
    // sqlite_schema b-tree; this surfaces page-1 b-tree corruption that a
    // bare `PRAGMA schema_version` (which only reads the schema cookie
    // out of the file header) would miss.
    connection
        .query_row("SELECT COUNT(*) FROM sqlite_schema", [], |row| row.get::<_, i64>(0))
        .map(|_| ())
        .map_err(|err| map_open_sqlite_error(err, OpenStage::SchemaProbe))
}

fn probe_database_header(connection: &Connection) -> Result<(), EngineOpenError> {
    connection
        .query_row("PRAGMA application_id", [], |row| row.get::<_, i64>(0))
        .map(|_| ())
        .map_err(|err| map_open_sqlite_error(err, OpenStage::HeaderProbe))
}

/// Pre-`pragma WAL` sidecar validation. SQLite silently discards a WAL
/// file whose header magic is wrong or whose advertised page size is
/// outside `[512, SQLITE_MAX_PAGE_SIZE]`, which would cause us to lose
/// committed frames at open time. AC-035a requires that we instead
/// refuse to open with `Corruption(WalReplayFailure)` rather than
/// silently rebuild from a truncated WAL.
fn probe_wal_sidecar(db_path: &Path) -> Result<(), EngineOpenError> {
    let mut wal_path = db_path.as_os_str().to_owned();
    wal_path.push("-wal");
    let wal_path = PathBuf::from(wal_path);
    // Bounded read: the WAL header is fixed-layout in the first 32
    // bytes (magic + format + page-size + checkpoint-seq + salts +
    // checksums); frame data starts at offset 32 and is irrelevant to
    // the magic + page-size pre-check. A `std::fs::read` of the whole
    // sidecar would force an unclean-shutdown open path to allocate
    // and copy the entire WAL into memory before SQLite touches
    // recovery — a real latency + RSS regression on AC-035.
    use std::io::Read;
    let mut file = match std::fs::File::open(&wal_path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Ok(()),
    };
    let mut bytes = [0u8; 32];
    if file.read_exact(&mut bytes).is_err() {
        // A short (< 32-byte) sidecar carries no committed frames;
        // SQLite treats it as empty and re-initializes WAL state.
        return Ok(());
    }
    let magic = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let page_size = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    // WAL_MAGIC mask per SQLite `walIndexRecover`: low bit distinguishes
    // big-endian vs little-endian checksum encoding; the rest of the
    // magic is fixed.
    const WAL_MAGIC_MASK: u32 = 0xFFFF_FFFE;
    const WAL_MAGIC: u32 = 0x377F_0682;
    const SQLITE_MAX_PAGE_SIZE: u32 = 65536;
    let magic_ok = (magic & WAL_MAGIC_MASK) == WAL_MAGIC;
    let page_size_ok =
        page_size.is_power_of_two() && (512..=SQLITE_MAX_PAGE_SIZE).contains(&page_size);
    if magic_ok && page_size_ok {
        return Ok(());
    }
    Err(EngineOpenError::Corruption(CorruptionDetail {
        kind: CorruptionKind::WalReplayFailure,
        stage: OpenStage::WalReplay,
        locator: CorruptionLocator::FileOffset { offset: if !magic_ok { 0 } else { 8 } },
        recovery_hint: RecoveryHint {
            code: "E_CORRUPT_WAL_REPLAY",
            doc_anchor: "design/recovery.md#wal-replay-failures",
        },
    }))
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

#[cfg(feature = "operator")]
fn read_schema_objects(
    connection: &Connection,
    obj_type: &str,
) -> Result<Vec<SchemaObject>, EngineError> {
    let mut stmt = connection
        .prepare(
            "SELECT name, sql FROM sqlite_schema
             WHERE type = ?1 AND name NOT LIKE 'sqlite_%' AND sql IS NOT NULL
             ORDER BY name",
        )
        .map_err(|_| EngineError::Storage)?;
    let rows = stmt
        .query_map([obj_type], |row| {
            Ok(SchemaObject { name: row.get::<_, String>(0)?, sql: row.get::<_, String>(1)? })
        })
        .map_err(|_| EngineError::Storage)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|_| EngineError::Storage)?);
    }
    Ok(out)
}

#[cfg(feature = "operator")]
fn order_canonical_first(mut objects: Vec<SchemaObject>) -> Vec<SchemaObject> {
    let mut canonical: Vec<SchemaObject> = Vec::new();
    for name in CANONICAL_TABLES {
        if let Some(pos) = objects.iter().position(|o| o.name == *name) {
            canonical.push(objects.remove(pos));
        }
    }
    canonical.extend(objects);
    canonical
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

fn ensure_vector_partition(connection: &mut Connection, dimension: u32) -> rusqlite::Result<()> {
    // 0.7.0 Pack 1 schema per dev/design/0.7.0-vector-quant-pack1.md D1/D2:
    // f32 `embedding` + binary-quant sibling `embedding_bin` + `source_type`
    // partition key + `kind` + `created_at`. The vec0 column type is
    // dim-parameterized, so the reshape lives here rather than in the
    // SQL-only migration framework — see fathomdb-schema migration step 9
    // and dev/plans/runs/0.7.0-PVQ-P1-IMPL-output.json for the deviation
    // from the design memo's "Choose (a)" guidance.
    //
    // Three paths:
    //   (1) no vector_default       -> CREATE at new shape.
    //   (2) old single-column shape -> stage + drop + recreate at new shape
    //                                  + repopulate with vec_quantize_binary.
    //   (3) already new shape       -> no-op.
    let existing_sql: Option<String> = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
            [DEFAULT_VECTOR_PARTITION],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    // Slice 10 / G10 — 3-way shape-sentinel (fixes the prior
    // `contains("embedding_bin")` no-op that hid the `status` column from
    // existing Pack-1 DBs):
    //   `status` present       -> Pack-2 (current) shape, no-op.
    //   `embedding_bin` present -> Pack-1 -> stage + recreate + back-fill status.
    //   neither                 -> legacy single-column -> migrate to current.
    match existing_sql {
        None => create_vector_partition(connection, dimension),
        Some(sql) if sql.contains("status") => Ok(()),
        Some(sql) if sql.contains("embedding_bin") => {
            migrate_vector_partition_pack1_to_pack2(connection, dimension)
        }
        Some(_) => migrate_vector_partition_to_pack1(connection, dimension),
    }
}

/// The current (Pack-2) `vector_default` vec0 shape. Slice 10 / G10 adds a plain
/// `status TEXT` metadata column — **not** aux (`+status`): aux columns
/// hard-error under a KNN `WHERE`, and the G10 filter constrains `status` in the
/// phase-1 KNN statement. `status` ships NULL plumbing only (no population source
/// yet).
fn vector_partition_create_sql(dimension: u32, if_not_exists: bool) -> String {
    let guard = if if_not_exists { "IF NOT EXISTS " } else { "" };
    format!(
        "CREATE VIRTUAL TABLE {guard}{DEFAULT_VECTOR_PARTITION} USING vec0(\
            embedding float[{dimension}],\
            embedding_bin bit[{dimension}],\
            source_type TEXT partition key,\
            kind TEXT,\
            created_at INTEGER,\
            status TEXT\
         )"
    )
}

fn create_vector_partition(connection: &Connection, dimension: u32) -> rusqlite::Result<()> {
    connection.execute_batch(&vector_partition_create_sql(dimension, true))
}

/// Slice 10 / G10 — stage + recreate + back-fill upgrade of an existing
/// **Pack-1** `vector_default` (has `embedding_bin`, lacks `status`) to the
/// Pack-2 shape. The existing `embedding_bin` blob is preserved verbatim (it may
/// be mean-centered; re-quantizing from `embedding` would drop the centering),
/// and `status` back-fills NULL. Same transactional discipline as
/// `migrate_vector_partition_to_pack1`: a single `Connection::transaction()`;
/// reader handles are not opened until `ensure_vector_partition` returns, and
/// cross-process access is serialized by the sidecar lock, so readers never see
/// a partial reshape.
fn migrate_vector_partition_pack1_to_pack2(
    connection: &mut Connection,
    dimension: u32,
) -> rusqlite::Result<()> {
    let tx = connection.transaction()?;
    tx.execute_batch(
        "CREATE TABLE _fathomdb_vector_pack2_stage (
             rowid         INTEGER PRIMARY KEY,
             embedding     BLOB NOT NULL,
             embedding_bin BLOB NOT NULL,
             source_type   TEXT,
             kind          TEXT,
             created_at    INTEGER
         );
         INSERT INTO _fathomdb_vector_pack2_stage(
             rowid, embedding, embedding_bin, source_type, kind, created_at
         )
             SELECT rowid, embedding, embedding_bin, source_type, kind, created_at
             FROM vector_default;
         DROP TABLE vector_default;",
    )?;
    tx.execute_batch(&vector_partition_create_sql(dimension, false))?;
    // `vec_bit(...)` re-tags the staged blob with the BIT subtype vec0's bit
    // column requires (a raw blob loses the subtype and fails the type check).
    // This preserves the existing (possibly mean-centered) bits verbatim — no
    // re-quantize, so centering survives the upgrade. `status` back-fills the
    // empty-string sentinel (vec0 TEXT metadata is NOT NULL-able; reserved-gap
    // candidate 13).
    tx.execute_batch(
        "INSERT INTO vector_default(
             rowid, embedding, embedding_bin, source_type, kind, created_at, status
         )
             SELECT rowid, embedding, vec_bit(embedding_bin), source_type, kind, created_at, ''
             FROM _fathomdb_vector_pack2_stage;
         DROP TABLE _fathomdb_vector_pack2_stage;",
    )?;
    tx.commit()
}

/// SQL fragment implementing the D3 `kind -> source_type` map.
/// Used both by the Pack 1 reshape migration and by the drift-detection
/// unit test that pins it to [`resolve_source_type`].
const KIND_TO_SOURCE_TYPE_CASE_SQL: &str = "CASE s.kind
    WHEN 'email'   THEN 'email'
    WHEN 'article' THEN 'article'
    WHEN 'paper'   THEN 'paper'
    WHEN 'meeting' THEN 'meeting'
    WHEN 'note'    THEN 'note'
    WHEN 'todo'    THEN 'todo'
    WHEN 'doc'     THEN 'article'
    ELSE 'article'
END";

/// Pack 1 in-place reshape of `vector_default`. Stages the existing
/// f32 corpus + each row's `kind`, drops the old single-column vec0
/// table, recreates at the runtime `dimension` with the Pack 1
/// columns, then repopulates with SQL-side `vec_quantize_binary` +
/// the D3 `kind -> source_type` mapping. The preflight CHECK on
/// unknown kinds has already run as migration step 9 by the time we
/// get here.
///
/// Atomicity: the DROP+CREATE+repopulate sequence runs inside a
/// rusqlite `Connection::transaction()` (DEFERRED begin per rusqlite
/// `transaction.rs:417`). Cross-process serialization is provided by
/// the engine's sidecar `acquire_lock` at `open_with_migrations`
/// (`lib.rs:1127` area); reader handles are not opened until
/// `ensure_vector_partition` returns (`lib.rs:1241` area), so readers
/// never observe a partial reshape.
fn migrate_vector_partition_to_pack1(
    connection: &mut Connection,
    dimension: u32,
) -> rusqlite::Result<()> {
    let tx = connection.transaction()?;
    tx.execute_batch(
        "CREATE TABLE _fathomdb_vector_migration_v0_7_0 (
             rowid     INTEGER PRIMARY KEY,
             embedding BLOB NOT NULL,
             kind      TEXT NOT NULL
         );
         INSERT INTO _fathomdb_vector_migration_v0_7_0(rowid, embedding, kind)
             SELECT v.rowid, v.embedding, r.kind
             FROM vector_default v
             JOIN _fathomdb_vector_rows r ON r.rowid = v.rowid;
         DROP TABLE vector_default;",
    )?;
    // Slice 10 / G10 — recreate directly at the Pack-2 shape (adds `status`), so
    // a legacy single-column DB lands the current shape in one reshape.
    tx.execute_batch(&vector_partition_create_sql(dimension, false))?;
    // `status` back-fills the empty-string sentinel (vec0 TEXT metadata is NOT
    // NULL-able; reserved-gap candidate 13). Legacy single-column DBs predate
    // mean-centering, so re-quantizing from the un-centered `embedding` is
    // correct here.
    let repopulate_sql = format!(
        "INSERT INTO vector_default(
             rowid, embedding, embedding_bin, source_type, kind, created_at, status
         )
         SELECT
             s.rowid,
             s.embedding,
             vec_quantize_binary(s.embedding),
             {KIND_TO_SOURCE_TYPE_CASE_SQL},
             s.kind,
             strftime('%s', 'now'),
             ''
         FROM _fathomdb_vector_migration_v0_7_0 s;
         DROP TABLE _fathomdb_vector_migration_v0_7_0;"
    );
    tx.execute_batch(&repopulate_sql)?;
    tx.commit()
}

fn encode_vector_blob(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|value| value.to_le_bytes()).collect()
}

fn decode_vector_blob(bytes: &[u8]) -> Vec<f32> {
    debug_assert_eq!(bytes.len() % 4, 0, "f32 BLOB length must be multiple of 4");
    bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

/// EU-5a2 — does the live embedder identity request mean-centering?
/// Identity-name compare per EU-5a1's BGE_SMALL_EMBEDDER_NAME constant
/// (`dev/design/embedder.md` §0.6). NoopEmbedder returns `false`.
fn identity_requires_mean_centering(identity: &EmbedderIdentity) -> bool {
    identity.name == BGE_SMALL_EMBEDDER_NAME
}

/// EU-5a2 — read the pinned mean vector from
/// `_fathomdb_embedder_profiles.mean_vec` for the default profile.
/// Returns `Ok(None)` when the column is NULL or the row is missing;
/// returns `Err(EngineError::Storage)` on dimension drift (the open-time
/// `check_embedder_profile` already fails closed for this, so a runtime
/// drift here would be an internal-inconsistency signal).
fn read_pinned_mean_vec(
    connection: &Connection,
    dimension: u32,
) -> Result<Option<Vec<f32>>, EngineError> {
    let bytes: Option<Vec<u8>> = connection
        .query_row(
            "SELECT mean_vec FROM _fathomdb_embedder_profiles WHERE profile = 'default'",
            [],
            |row| row.get::<_, Option<Vec<u8>>>(0),
        )
        .or_else(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
        .map_err(|_| EngineError::Storage)?;
    let Some(bytes) = bytes else { return Ok(None) };
    let expected_len = (dimension as usize).saturating_mul(4);
    if bytes.len() != expected_len {
        return Err(EngineError::Storage);
    }
    let mut out = Vec::with_capacity(dimension as usize);
    for chunk in bytes.chunks_exact(4) {
        let arr = [chunk[0], chunk[1], chunk[2], chunk[3]];
        out.push(f32::from_le_bytes(arr));
    }
    Ok(Some(out))
}

/// EU-5a2 — pointwise `v - mean`. Length-checked debug-assert; caller
/// guarantees equal length via `read_pinned_mean_vec` + dimension check.
fn subtract_mean(v: &[f32], mean: &[f32]) -> Vec<f32> {
    debug_assert_eq!(v.len(), mean.len(), "subtract_mean dim mismatch");
    v.iter().zip(mean.iter()).map(|(a, b)| *a - *b).collect()
}

/// Maps the writer-facing `kind` value to the locked Pack 1
/// `source_type` partition-key vocabulary. Must stay in lockstep with
/// the CASE WHEN inlined in migration step 9
/// (`fathomdb-schema/src/lib.rs`); the drift-detection unit test in
/// this module's `tests` mod enforces that. Per
/// `dev/design/0.7.0-vector-quant-pack1.md` D3.
fn resolve_source_type(kind: &str) -> Result<&'static str, EngineError> {
    Ok(match kind {
        "email" => "email",
        "article" => "article",
        "paper" => "paper",
        "meeting" => "meeting",
        "note" => "note",
        "todo" => "todo",
        // Synthetic AC-013 test fixture; coerced so the 6-value HITL lock holds.
        "doc" => "article",
        // G11 (Slice 15) — edge-body projection; separate `source_type` partition
        // key distinguishes edge vectors from node vectors in `vector_default`.
        "edge_fact" => "edge_fact",
        _ => return Err(EngineError::Storage),
    })
}

/// G11 (Slice 15) — derive a stable hex-encoded sha256 logical_id from a
/// `(kind, name)` pair. Both inputs are lowercased before hashing so that
/// entity identity is case-insensitive (`"Alice"` == `"alice"`). The
/// canonical form is `sha256("<kind>:<name>")` — identical to the
/// ADR-0.8.1-byo-llm derivation rule.
///
/// fix-34 [P1]: because `:` is the delimiter, a `:` in `kind` would let the
/// split point move and collide two distinct `(kind, name)` pairs onto one
/// identity (e.g. `("a:b","c")` and `("a","b:c")` both hash `"a:b:c"`),
/// silently dropping one entity via batch dedup / G0 supersession. An empty
/// `name` collapses every name-less entity of a kind onto `sha256("<kind>:")`.
/// We reject both at the boundary; this preserves the ADR derivation rule
/// (a colon-free `kind` makes the first `:` an unambiguous delimiter, so a `:`
/// in `name` stays safe — edge keys deliberately rely on that).
fn derive_logical_id(kind: &str, name: &str) -> Result<String, EngineError> {
    if kind.contains(':') || name.is_empty() {
        return Err(EngineError::Extractor);
    }
    let input = format!("{}:{}", kind.to_lowercase(), name.to_lowercase());
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

/// fix-34 [P2]: dedup a batch of [`PreparedWrite`]s by `logical_id`, keeping the
/// first occurrence. Shared by the entity and edge arms of the BYO-LLM ingest
/// path so a harness that returns the same node/edge twice in one response does
/// not write a row that immediately supersedes its sibling.
fn dedup_prepared_by_logical_id(batch: Vec<PreparedWrite>) -> Vec<PreparedWrite> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    batch
        .into_iter()
        .filter(|w| match w {
            PreparedWrite::Node { logical_id: Some(id), .. }
            | PreparedWrite::Edge { logical_id: Some(id), .. } => seen.insert(id.clone()),
            _ => true,
        })
        .collect()
}

/// fix-35 [P2]: BYO-LLM extractor I/O timeout. Defaults to 300s to accommodate
/// slow LLM harnesses; override (in milliseconds) via
/// `FATHOMDB_EXTRACTOR_TIMEOUT_MS` (tests use this to exercise the hung-harness
/// path quickly).
fn extractor_io_timeout() -> Duration {
    std::env::var("FATHOMDB_EXTRACTOR_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or_else(|| Duration::from_secs(300))
}

/// fix-35 [P1/P2]: receive one line from the stdout reader thread, bounded by
/// `timeout`. A timeout, a closed channel (reader thread ended / child EOF), or
/// an underlying io error all map to [`EngineError::Extractor`].
fn recv_extractor_line(
    rx: &Receiver<std::io::Result<String>>,
    timeout: Duration,
) -> Result<String, EngineError> {
    match rx.recv_timeout(timeout) {
        Ok(Ok(line)) => Ok(line),
        _ => Err(EngineError::Extractor),
    }
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
) -> Result<bool, EngineOpenError> {
    // Returns `true` iff `_fathomdb_embedder_profiles.mean_vec IS NOT NULL`
    // for the default profile (and its byte length matches `4 * dimension`
    // per `dev/design/embedder.md` §0.2). EU-5a2: column lands in step 10.
    let mut statement = match connection.prepare(
        "SELECT name, revision, dimension, mean_vec FROM _fathomdb_embedder_profiles WHERE profile = 'default'",
    ) {
        Ok(statement) => statement,
        Err(_) => return Ok(false),
    };
    let mut rows = statement.query([]).map_err(|_| {
        EngineOpenError::Corruption(CorruptionDetail {
            kind: CorruptionKind::EmbedderIdentityDrift,
            stage: OpenStage::EmbedderIdentity,
            locator: CorruptionLocator::OpaqueSqliteError { sqlite_extended_code: 0 },
            recovery_hint: RecoveryHint {
                code: "E_CORRUPT_EMBEDDER_IDENTITY",
                doc_anchor: "design/recovery.md#embedder-identity-drift",
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
                doc_anchor: "design/recovery.md#embedder-identity-drift",
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
        return Ok(false);
    };

    let stored_name = row.get::<_, String>(0).map_err(|_| {
        EngineOpenError::Corruption(CorruptionDetail {
            kind: CorruptionKind::EmbedderIdentityDrift,
            stage: OpenStage::EmbedderIdentity,
            locator: CorruptionLocator::TableRow { table: "_fathomdb_embedder_profiles", rowid: 0 },
            recovery_hint: RecoveryHint {
                code: "E_CORRUPT_EMBEDDER_IDENTITY",
                doc_anchor: "design/recovery.md#embedder-identity-drift",
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
                doc_anchor: "design/recovery.md#embedder-identity-drift",
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
                doc_anchor: "design/recovery.md#embedder-identity-drift",
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

    // EU-5a2 / `dev/design/embedder.md` §0.2 invariant: if `mean_vec` is
    // populated, byte length MUST equal `4 * dimension`. Debug builds
    // assert; release builds fail closed via EmbedderIdentityMismatch
    // (the same fail-closed channel the rest of profile drift takes).
    let mean_vec: Option<Vec<u8>> = row.get::<_, Option<Vec<u8>>>(3).map_err(|_| {
        EngineOpenError::Corruption(CorruptionDetail {
            kind: CorruptionKind::EmbedderIdentityDrift,
            stage: OpenStage::EmbedderIdentity,
            locator: CorruptionLocator::TableRow { table: "_fathomdb_embedder_profiles", rowid: 0 },
            recovery_hint: RecoveryHint {
                code: "E_CORRUPT_EMBEDDER_IDENTITY",
                doc_anchor: "design/recovery.md#embedder-identity-drift",
            },
        })
    })?;
    let pinned = match mean_vec {
        Some(bytes) => {
            let expected_len = (dimension as usize).saturating_mul(4);
            // `dev/design/embedder.md` §0.2 invariant: when populated,
            // `mean_vec` byte length MUST equal `4 * dimension`. Fail
            // closed via the existing identity-drift channel in both
            // debug and release builds — tests deliberately poke
            // malformed values to exercise this branch.
            if bytes.len() != expected_len {
                return Err(EngineOpenError::EmbedderIdentityMismatch {
                    stored,
                    supplied: supplied.clone(),
                });
            }
            true
        }
        None => false,
    };

    Ok(pinned)
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
        if let PreparedWrite::Node { kind, body, .. } = write {
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
        PreparedWrite::Node { kind, body, source_id, logical_id } => {
            if kind.trim().is_empty() || body.trim().is_empty() {
                return Err(EngineError::WriteValidation);
            }
            if let Some(source_id) = source_id {
                if source_id.is_empty() {
                    return Err(EngineError::WriteValidation);
                }
            }
            // G0 — an explicit logical_id must be non-empty (NULL/None is the
            // legacy default; an empty string is never a valid identity).
            // Also reject char(30) = \x1e (ASCII RS), which is the BFS cycle-guard
            // delimiter; allowing it would corrupt the visited-path substring test.
            if let Some(logical_id) = logical_id {
                if logical_id.is_empty() || logical_id.contains('\x1e') {
                    return Err(EngineError::WriteValidation);
                }
            }
            Ok(WritePlan::Node)
        }
        PreparedWrite::Edge { kind, from, to, source_id, logical_id, .. } => {
            if kind.trim().is_empty() || from.trim().is_empty() || to.trim().is_empty() {
                return Err(EngineError::WriteValidation);
            }
            // Reject char(30) in from/to: these become from_id/to_id in canonical_edges
            // and appear in BFS visited strings — an \x1e there would corrupt the guard.
            if from.contains('\x1e') || to.contains('\x1e') {
                return Err(EngineError::WriteValidation);
            }
            if let Some(source_id) = source_id {
                if source_id.is_empty() {
                    return Err(EngineError::WriteValidation);
                }
            }
            if let Some(logical_id) = logical_id {
                if logical_id.is_empty() || logical_id.contains('\x1e') {
                    return Err(EngineError::WriteValidation);
                }
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

// fix-30 [P2]: helpers to collect active edge write_cursors BEFORE a supersession
// UPDATE so the callers can prune stale vector_default rows.
fn prior_edge_cursors_by_logical_id(
    tx: &rusqlite::Transaction<'_>,
    logical_id: &str,
) -> rusqlite::Result<Vec<i64>> {
    let mut s = tx.prepare_cached(
        "SELECT write_cursor FROM canonical_edges \
         WHERE logical_id = ?1 AND superseded_at IS NULL",
    )?;
    let rows = s.query_map(params![logical_id], |r| r.get(0))?;
    rows.collect()
}

fn prior_edge_cursors_by_triple(
    tx: &rusqlite::Transaction<'_>,
    from: &str,
    to: &str,
    kind: &str,
) -> rusqlite::Result<Vec<i64>> {
    let mut s = tx.prepare_cached(
        "SELECT write_cursor FROM canonical_edges \
         WHERE from_id = ?1 AND to_id = ?2 AND kind = ?3 AND superseded_at IS NULL",
    )?;
    let rows = s.query_map(params![from, to, kind], |r| r.get(0))?;
    rows.collect()
}

fn commit_batch(
    connection: &mut Connection,
    batch: &[PreparedWrite],
    plans: &[WritePlan],
    base_cursor: u64,
    provenance_row_cap: u64,
) -> rusqlite::Result<u64> {
    let tx = connection.transaction()?;

    for (i, (write, plan)) in batch.iter().zip(plans).enumerate() {
        // Per-row cursor: row i gets `base_cursor + i + 1`. See the
        // comment in `Engine::write_inner`.
        let cursor = base_cursor.saturating_add((i as u64).saturating_add(1));
        match (write, plan) {
            (PreparedWrite::Node { kind, body, source_id, logical_id }, WritePlan::Node) => {
                // G0 — supersession is tombstone-then-insert in this same txn:
                // mark the prior active version superseded BEFORE inserting the
                // new active row, so the partial-unique-active index never sees
                // two active rows for one logical_id. Scoped to logical_id ALONE
                // (Decision 5, HITL-SIGNED 2026-06-05): a kind-change re-ingest of
                // the same logical_id SUPERSEDES, never forks. No-op when logical_id
                // is None (legacy/own-identity insert, behavior-identical to 0.7.x).
                if let Some(logical_id) = logical_id {
                    tx.execute(
                        "UPDATE canonical_nodes SET superseded_at = ?1
                         WHERE logical_id = ?2 AND superseded_at IS NULL",
                        params![cursor, logical_id],
                    )?;
                }
                tx.execute(
                    "INSERT INTO canonical_nodes(write_cursor, kind, body, source_id, logical_id)
                     VALUES(?1, ?2, ?3, ?4, ?5)",
                    params![cursor, kind, body, source_id, logical_id],
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
                } else {
                    // Non-vector-indexed nodes will never be projected,
                    // so terminate the cursor up-front to let
                    // `advance_projection_cursor` walk past it.
                    record_projection_terminal(&tx, cursor, "up_to_date")?;
                }
            }
            (
                PreparedWrite::Edge {
                    kind,
                    from,
                    to,
                    source_id,
                    logical_id,
                    body,
                    t_valid,
                    t_invalid,
                    confidence,
                    extractor_model_id,
                    temporal_fallback,
                },
                WritePlan::Edge,
            ) => {
                // G0 — identical tombstone-then-insert supersession on edges,
                // keyed by logical_id ALONE (Decision 5, HITL-SIGNED 2026-06-05;
                // edge `kind` is relationship-type, not identity — a kind-change
                // re-ingest of the same edge logical_id SUPERSEDES, never forks).
                // No-op when logical_id is None.
                if let Some(logical_id) = logical_id {
                    // fix-30 [P2]: collect prior active cursors BEFORE tombstoning
                    // so stale vector_default rows can be pruned.
                    let prior_g0 = prior_edge_cursors_by_logical_id(&tx, logical_id)?;
                    tx.execute(
                        "UPDATE canonical_edges SET superseded_at = ?1
                         WHERE logical_id = ?2 AND superseded_at IS NULL",
                        params![cursor, logical_id],
                    )?;
                    for sc in &prior_g0 {
                        tx.execute("DELETE FROM vector_default WHERE rowid = ?1", [sc])?;
                        tx.execute(
                            "DELETE FROM _fathomdb_vector_rows WHERE write_cursor = ?1",
                            [sc],
                        )?;
                        // fix-32 [P2]: record terminal so advance_projection_cursor
                        // can walk past this now-superseded cursor.
                        record_projection_terminal(&tx, *sc as u64, "superseded")?;
                    }
                }
                // G11 — invalidate-not-accumulate: for fact-edges (body IS NOT NULL),
                // tombstone any prior active edge on the same (from_id, to_id, kind)
                // BEFORE inserting the new row. This is DIFFERENT from the G0
                // logical_id tombstone: it is keyed on the triple, not the identity.
                // Regular edges (body=None) skip this path — they retain G0 semantics.
                if body.is_some() {
                    // fix-30 [P2]: collect and prune vector shadow for the superseded edge.
                    let prior_g11 = prior_edge_cursors_by_triple(&tx, from, to, kind)?;
                    tx.execute(
                        "UPDATE canonical_edges SET superseded_at = ?1
                         WHERE from_id = ?2 AND to_id = ?3 AND kind = ?4 AND superseded_at IS NULL",
                        params![cursor, from, to, kind],
                    )?;
                    for sc in &prior_g11 {
                        tx.execute("DELETE FROM vector_default WHERE rowid = ?1", [sc])?;
                        tx.execute(
                            "DELETE FROM _fathomdb_vector_rows WHERE write_cursor = ?1",
                            [sc],
                        )?;
                        // fix-32 [P2]: mark terminal so projection cursor can advance.
                        record_projection_terminal(&tx, *sc as u64, "superseded")?;
                    }
                }
                let temporal_fallback_i: Option<i64> =
                    temporal_fallback.and_then(|f| if f { Some(1) } else { None });
                tx.execute(
                    "INSERT INTO canonical_edges(
                         write_cursor, kind, from_id, to_id, source_id, logical_id,
                         body, t_valid, t_invalid, confidence, extractor_model_id,
                         temporal_fallback
                     ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        cursor,
                        kind,
                        from,
                        to,
                        source_id,
                        logical_id,
                        body,
                        t_valid,
                        t_invalid,
                        confidence,
                        extractor_model_id,
                        temporal_fallback_i
                    ],
                )?;
                // G11 — edge FTS projection into `search_index_edges` (separate
                // table from node-body `search_index` — Option B partition).
                if let Some(edge_body) = body.as_ref() {
                    tx.execute(
                        "INSERT INTO search_index_edges(body, kind, write_cursor)
                         VALUES(?1, ?2, ?3)",
                        params![edge_body, kind, cursor],
                    )?;
                }
                // G11 — edge vector projection: enqueue for projection scheduler
                // under a fixed kind `"edge_fact"` (so resolve_source_type maps it
                // to `source_type = "edge_fact"` in vector_default). Auto-register
                // "edge_fact" in _fathomdb_vector_kinds (idempotent).
                if body.is_some() {
                    let now_unix =
                        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
                            as i64;
                    tx.execute(
                        "INSERT OR IGNORE INTO _fathomdb_vector_kinds(kind, profile, created_at)
                         VALUES('edge_fact', 'default', ?1)",
                        params![now_unix],
                    )?;
                    tx.execute(
                        "INSERT INTO _fathomdb_projection_state(
                             kind, last_enqueued_cursor, updated_at
                         ) VALUES('edge_fact', ?1, 0)
                         ON CONFLICT(kind) DO UPDATE
                             SET last_enqueued_cursor = excluded.last_enqueued_cursor",
                        params![cursor],
                    )?;
                    // Do NOT call record_projection_terminal — let the scheduler
                    // embed the body and mark it terminal after projection.
                } else {
                    record_projection_terminal(&tx, cursor, "up_to_date")?;
                }
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
                record_projection_terminal(&tx, cursor, "up_to_date")?;
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
                record_projection_terminal(&tx, cursor, "up_to_date")?;
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
                record_projection_terminal(&tx, cursor, "up_to_date")?;
            }
            _ => return Err(rusqlite::Error::InvalidQuery),
        }
    }

    // G8 (Slice 20 / F10) — cross-row dangling-edge flag-and-count. This runs
    // AFTER the batch loop (so every same-batch node is already on disk in `tx`
    // and a same-batch later-inserted endpoint is visible) and BEFORE retention /
    // projection-cursor / commit. It is the cross-row reason this lives here and
    // not in single-row pre-insert `validate_write`. Default is FLAG-AND-COUNT:
    // we only COUNT, never roll back (strict-mode rollback is deferred to
    // reserved-gap band 22 — adding a write-options surface is out of scope).
    //
    // Probe is `logical_id`-alone against the step-12 partial index
    // `canonical_nodes_logical_active_idx ON canonical_nodes(logical_id)
    // WHERE superseded_at IS NULL` (its leading column + partial predicate), so
    // it SEARCHes the index with no SCAN (see `tests/pr_g8_dangling_edges.rs`
    // case (f)). There is no node-kind to match: `canonical_edges` stores only
    // the edge's own kind, not the endpoint node's kind.
    let dangling_edge_endpoints = {
        // O(N) pre-pass: record, per `logical_id`, the LAST (highest) index at
        // which an `Edge { logical_id: Some(_), .. }` with that id appears. Keyed
        // by `logical_id` ALONE (Decision 5, HITL-SIGNED 2026-06-05) to match the
        // supersession UPDATE, which keys by logical_id alone: a kind-change
        // re-ingest of the same edge logical_id SUPERSEDES the earlier one.
        // Iterating front-to-back and overwriting means the stored value ends up
        // as the final index for each id. An edge at index `i` with that id is
        // then in-batch-superseded iff `last_index[lid] > i`. This is
        // behavior-identical to the prior per-edge `batch[i+1..]` `.any(..)` scan
        // (which was O(N²) under the single-writer txn) — same skip-set, same count.
        let mut last_index: HashMap<&str, usize> = HashMap::new();
        for (i, write) in batch.iter().enumerate() {
            if let PreparedWrite::Edge { logical_id: Some(lid), .. } = write {
                last_index.insert(lid.as_str(), i);
            }
        }

        let mut probe = tx.prepare(
            "SELECT 1 FROM canonical_nodes WHERE logical_id = ?1 AND superseded_at IS NULL LIMIT 1",
        )?;
        let mut count: u64 = 0;
        for (i, write) in batch.iter().enumerate() {
            if let PreparedWrite::Edge { from, to, logical_id, .. } = write {
                // Honor `edge.superseded_at IS NULL`: an edge inserted in this
                // batch is active unless a LATER same-batch edge with the same
                // `Some(logical_id)` tombstoned it (the loop's supersession
                // UPDATE). Skip such an in-batch-superseded edge. Edges with
                // `logical_id: None` are never superseded-in-batch.
                if let Some(lid) = logical_id {
                    let superseded_in_batch =
                        last_index.get(lid.as_str()).is_some_and(|&last| last > i);
                    if superseded_in_batch {
                        continue;
                    }
                }
                // Probe `from_id` and `to_id` independently (0, 1, or 2 per edge).
                for endpoint in [from, to] {
                    if !probe.exists(params![endpoint])? {
                        count = count.saturating_add(1);
                    }
                }
            }
        }
        count
    };

    enforce_provenance_retention(&tx, provenance_row_cap)?;
    advance_projection_cursor(&tx)?;

    tx.commit()?;
    Ok(dangling_edge_endpoints)
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
                        OpenStage::WalReplay => "design/recovery.md#wal-replay-failures",
                        OpenStage::HeaderProbe => "design/recovery.md#header-malformed",
                        OpenStage::SchemaProbe => "design/recovery.md#schema-inconsistent",
                        OpenStage::EmbedderIdentity => "design/recovery.md#embedder-identity-drift",
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
/// 0.7.0 perf-experiments hook: apply caller-supplied reader PRAGMAs
/// from the `FATHOMDB_PERF_READER_PRAGMAS` env var. Format:
/// comma-separated `name=value` pairs (e.g.
/// `cache_size=-262144,mmap_size=268435456,temp_store=MEMORY`).
///
/// **Gated on `FATHOMDB_PERF_EXPERIMENTS=1`.** No-op if the gate env
/// var is unset, so production paths are never affected. Failures to
/// apply individual PRAGMAs are logged to stderr (via `eprintln!`) but
/// do not error the connection open — experiments are best-effort,
/// not contract.
///
/// Scope: 0.7.0 perf-experiment campaign per
/// `dev/plans/0.7.0-perf-experiments.md`. Once Wave 5 picks the
/// landing combination, the chosen PRAGMAs are hardcoded as the new
/// reader-open default and this hook is removed.
/// 0.7.0 perf-experiments hook: apply writer-side PRAGMAs from
/// `FATHOMDB_PERF_WRITER_PRAGMAS` (same format as reader hook).
/// **Runs BEFORE migrations** so PRAGMAs like `page_size` that must
/// precede any table creation take effect on a fresh DB.
///
/// Gated on `FATHOMDB_PERF_EXPERIMENTS=1`. No-op otherwise.
fn apply_perf_experiment_writer_pragmas(connection: &Connection) {
    if std::env::var_os("FATHOMDB_PERF_EXPERIMENTS").is_none() {
        return;
    }
    let raw = match std::env::var("FATHOMDB_PERF_WRITER_PRAGMAS") {
        Ok(s) if !s.is_empty() => s,
        _ => return,
    };
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (name, value) = match entry.split_once('=') {
            Some((n, v)) => (n.trim(), v.trim()),
            None => {
                eprintln!("perf-experiment: bad writer pragma entry (expect name=value): {entry}");
                continue;
            }
        };
        if name.is_empty() {
            eprintln!("perf-experiment: empty pragma name in writer entry: {entry}");
            continue;
        }
        match connection.pragma_update(None, name, value) {
            Ok(()) => {
                eprintln!(
                    "perf-experiment: applied PRAGMA {name}={value} on writer (pre-migration)"
                );
            }
            Err(err) => {
                eprintln!("perf-experiment: writer PRAGMA {name}={value} failed: {err}");
            }
        }
    }
}

fn apply_perf_experiment_reader_pragmas(connection: &Connection) {
    if std::env::var_os("FATHOMDB_PERF_EXPERIMENTS").is_none() {
        return;
    }
    let raw = match std::env::var("FATHOMDB_PERF_READER_PRAGMAS") {
        Ok(s) if !s.is_empty() => s,
        _ => return,
    };
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (name, value) = match entry.split_once('=') {
            Some((n, v)) => (n.trim(), v.trim()),
            None => {
                eprintln!("perf-experiment: bad pragma entry (expect name=value): {entry}");
                continue;
            }
        };
        if name.is_empty() {
            eprintln!("perf-experiment: empty pragma name in entry: {entry}");
            continue;
        }
        match connection.pragma_update(None, name, value) {
            Ok(()) => {
                eprintln!("perf-experiment: applied PRAGMA {name}={value} on reader");
            }
            Err(err) => {
                eprintln!("perf-experiment: PRAGMA {name}={value} failed: {err}");
            }
        }
    }
}

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
    use super::{resolve_source_type, Engine, PreparedWrite, KIND_TO_SOURCE_TYPE_CASE_SQL};
    use rusqlite::Connection;
    use tempfile::TempDir;

    // Pack 1 drift-detection: the Rust helper used by the two writer
    // sites must agree with the CASE WHEN used by the Pack 1 reshape
    // migration in `migrate_vector_partition_to_pack1`. The CASE SQL
    // is exported as `KIND_TO_SOURCE_TYPE_CASE_SQL`; this test
    // exercises it against an in-memory SQLite (no sqlite-vec extension
    // required — only the CASE) and asserts byte-equal output with the
    // Rust helper for every kind in the locked Pack 1 vocabulary
    // (incl. the synthetic `doc` -> `article` coercion). See
    // `dev/design/0.7.0-vector-quant-pack1.md` D3 / D4.
    #[test]
    fn resolve_source_type_drift_check() {
        let kinds = ["email", "article", "paper", "meeting", "note", "todo", "doc"];

        // 1. Rust helper return values (table is the contract: changes
        //    here must be reflected in the SQL CASE or this test fails).
        let want: &[(&str, &str)] = &[
            ("email", "email"),
            ("article", "article"),
            ("paper", "paper"),
            ("meeting", "meeting"),
            ("note", "note"),
            ("todo", "todo"),
            ("doc", "article"),
        ];
        for (kind, expected) in want {
            let got = resolve_source_type(kind).unwrap_or_else(|_| {
                panic!("resolve_source_type({kind}) returned Err; want Ok({expected})")
            });
            assert_eq!(got, *expected, "Rust helper drift for kind={kind}");
        }
        assert!(
            resolve_source_type("banana").is_err(),
            "unknown kind must surface as writer error"
        );

        // 2. SQL CASE evaluated against the same kinds. Build a
        //    one-row staging row per kind and SELECT through
        //    KIND_TO_SOURCE_TYPE_CASE_SQL; assert each row equals the
        //    Rust helper's output. Drift in either direction fails.
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        conn.execute_batch("CREATE TABLE s(kind TEXT NOT NULL)").expect("create s");
        for kind in &kinds {
            conn.execute("INSERT INTO s(kind) VALUES (?1)", [kind]).expect("insert kind");
        }
        let sql = format!("SELECT s.kind, {KIND_TO_SOURCE_TYPE_CASE_SQL} FROM s");
        let mut stmt = conn.prepare(&sql).expect("prepare CASE");
        let rows: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .expect("query")
            .map(|r| r.expect("row"))
            .collect();
        assert_eq!(rows.len(), kinds.len(), "row count drift");
        for (kind, sql_result) in &rows {
            let rust_result = resolve_source_type(kind).expect("known kind");
            assert_eq!(
                sql_result, rust_result,
                "SQL CASE vs Rust helper drift for kind={kind}: SQL={sql_result}, Rust={rust_result}"
            );
        }
    }

    #[test]
    fn write_advances_cursor() {
        let dir = TempDir::new().unwrap();
        let opened = Engine::open(dir.path().join("rewrite.sqlite")).expect("engine should open");
        let receipt = opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: "hello".to_string(),
                source_id: None,
                logical_id: None,
            }])
            .expect("write should succeed");

        assert_eq!(receipt.cursor, 1);
    }
}
