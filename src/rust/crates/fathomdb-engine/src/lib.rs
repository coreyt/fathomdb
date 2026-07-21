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

/// 0.8.18 Slice 5 (#5 vector-equivalence probe) — the committed 45-probe fixture
/// (byte-identical to `fathomdb-embedder/tests/fixtures/candle_onnx_equivalence_probes.txt`;
/// a drift-guard test pins the two copies equal). One probe per non-empty line;
/// lines whose first non-whitespace char is `#` are comments.
const VECTOR_EQUIVALENCE_PROBE_FIXTURE: &str = include_str!("vector_equivalence_probes.txt");

/// 0.8.18 Slice 5 (#5 vector-equivalence probe) — the FROZEN D4 tolerance floor,
/// **P2 component**: the un-centered Phase-2 L2 epsilon. `‖reembed − reference‖₂`
/// (un-centered, `vec_distance_l2` semantics) strictly greater than this ⇒
/// divergence ⇒ dense refused. Named constant so the final ε (HITL look at
/// landing) is trivially tunable. The **P1 component** (Phase-1 mean-centered
/// `embedding_bin` sign-flip count) has an *exact-zero* floor: ANY single flip on
/// the 45 probes ⇒ divergence (see [`VECTOR_EQUIVALENCE_P1_FLIP_FLOOR`]).
const VECTOR_EQUIVALENCE_L2_EPSILON: f32 = 1e-5;

/// 0.8.18 Slice 5 — the FROZEN D4 tolerance floor, **P1 component**: the maximum
/// tolerated Phase-1 mean-centered `embedding_bin` sign-flip count across all 45
/// probes. `0` = exact: any single flip ⇒ divergence ⇒ dense refused.
const VECTOR_EQUIVALENCE_P1_FLIP_FLOOR: u64 = 0;
/// Default drain budget for `rebuild_projections` / `rebuild_vec0`. The
/// rebuild path freezes the scheduler before truncating shadow rows, so
/// the only outstanding work is whatever workers were mid-flight when
/// the call landed; 30 s is generous for normal job sizes and bounded
/// for tests.
#[cfg(feature = "operator")]
const REBUILD_DRAIN_TIMEOUT_MS: u64 = 30_000;
/// OPP-12 Phase-1 (0.8.19 Slice 10) — drain budget the `transition`/`purge`
/// lifecycle verbs use to settle in-flight projection work before mutating.
/// Same 30 s budget as `REBUILD_DRAIN_TIMEOUT_MS`, but not `operator`-gated
/// (the lifecycle verbs are always-on governed surface).
const LIFECYCLE_DRAIN_TIMEOUT_MS: u64 = 30_000;
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
/// 0.8.20 Slice 5b (R-20-E5) — how many times an erasure verb re-tries
/// `PRAGMA wal_checkpoint(TRUNCATE)` before refusing with
/// [`EngineError::ErasureIncomplete`]. Deliberately small: a concurrent reader
/// pinning a WAL snapshot can hold it for an unbounded time, and an erasure verb
/// must fail loudly rather than block a caller indefinitely.
const ERASURE_WAL_TRUNCATE_ATTEMPTS: u32 = 5;
/// 0.8.20 Slice 5b (R-20-E5) — pause between WAL-truncation attempts
/// (~100 ms total budget across [`ERASURE_WAL_TRUNCATE_ATTEMPTS`]).
const ERASURE_WAL_TRUNCATE_BACKOFF_MS: u64 = 25;
/// 0.8.20 Slice 5b (R-20-E6) — the sentinel that replaces an erased
/// `result_stable_ids` element in the telemetry sink. Positional alignment with
/// the parallel `result_ids` array is preserved, so a redacted sink stays
/// parseable by the gold pipeline.
const REDACTED_STABLE_ID: &str = "[erased]";
/// 0.8.20 Slice 5b (design `0.8.20-slice0-erasure-design.md` §2 defect D-A,
/// HITL-ruled 2026-07-19: *"there must be an auditable record of deletion
/// event."*) — op-store collections holding ERASURE-AUDIT records.
///
/// These rows are **exempt from [`enforce_provenance_retention`]**. Before this
/// slice they were swept like any other op-store row: cap-first, oldest-`id`
/// first, with no collection filter — and because the audit row is written
/// *before* the workload that follows it, it was among the FIRST evicted. The
/// proof of erasure was therefore destructible, and shared a retention pool with
/// the very payloads it must prove erased. Accountability (demonstrating *that*
/// an erasure occurred) is a distinct obligation from erasure itself, and a
/// retention sweep must not silently discharge it.
///
/// **Guarantee:** a row in one of these collections is never removed by the
/// retention sweep, and (0.8.20 Slice 5 fix-3) never by
/// [`Engine::excise_collection_record`] either — see
/// [`is_erasure_bookkeeping_collection`].
const ERASURE_AUDIT_COLLECTIONS: &[&str] = &["excise_source_audit", "excise_record_audit"];
/// 0.8.20 Slice 5 fix-1 (codex §9 P2) — the `operational_mutations` collection
/// holding the DURABLE record of a telemetry redaction that is owed but not yet
/// performed. See [`Engine::discharge_pending_redactions`].
///
/// Like the audit collections it is exempt from the retention sweep: an
/// outstanding erasure obligation must not be discharged by cap pressure.
const ERASURE_PENDING_REDACTION_COLLECTION: &str = "erasure_pending_redaction";
/// 0.8.20 Slice 5 fix-3 (codex §9 round-3 P1) — true for the op-store
/// collections that hold the engine's ERASURE BOOKKEEPING: the durable
/// pending-redaction queue and the erasure-audit trail.
///
/// These are engine-owned invariants that happen to be *stored* as op-store
/// records. They are not caller data, and the generic record-erasure verb
/// [`Engine::excise_collection_record`] must refuse to target them:
///
/// * **The pending queue** ([`ERASURE_PENDING_REDACTION_COLLECTION`]) records a
///   telemetry redaction the engine still OWES. Deleting the entry makes
///   [`Engine::complete_erasure_at_rest`] see no outstanding work, so the next
///   erasure verb reports SUCCESS while the erased `l:`/`h:` ids are still in
///   the telemetry sink. That is the exact R-20-E5 violation — *an erasure verb
///   must never report success on an incomplete erasure* — that the queue was
///   introduced to close, and the verb re-opened it through an
///   operator-reachable path (`--excise-collection erasure_pending_redaction
///   --excise-record-key <verb>`).
/// * **The audit trail** ([`ERASURE_AUDIT_COLLECTIONS`]) is protected by the
///   HITL ruling of 2026-07-19: *"there must be an auditable record of deletion
///   event."* Deleting audit rows one-by-one defeats that ruled-on guarantee as
///   surely as a retention sweep would. Accountability is a distinct obligation
///   from erasure, and no verb may silently discharge it.
///
/// Neither carries erasable payload, so refusing them costs a caller nothing: a
/// pending-queue row holds only stable ids the engine is about to remove from
/// the sink (and deletes itself on discharge), and an audit row holds a
/// `source_id` bound by the non-PII rule or a SHA-256 record digest.
///
/// The refusal is TYPED ([`EngineError::InvalidArgument`]), never a silent
/// no-op — an operator who aimed at the wrong collection must be told. The shape
/// mirrors the slice's existing precedent: [`Engine::erase_source`] refuses the
/// reserved `_`-prefixed provenance namespace while [`Engine::excise_source`]
/// stays permissive.
///
/// Gated on `feature = "operator"` to match its only call site,
/// [`Engine::excise_collection_record`], which is itself operator-only: without
/// the matching `cfg` a non-`operator` build emits a `dead_code` warning for a
/// helper that has nothing to guard, because the verb it guards does not exist
/// in that build.
#[cfg(feature = "operator")]
fn is_erasure_bookkeeping_collection(collection: &str) -> bool {
    collection == ERASURE_PENDING_REDACTION_COLLECTION
        || ERASURE_AUDIT_COLLECTIONS.contains(&collection)
}
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
    /// 0.8.8 Slice 15 (OPP-9) — opt-in telemetry sink. `None` (default) = OFF.
    /// Local JSONL append; no network/egress. The OFF path never takes this lock —
    /// it is gated by `telemetry_enabled` (below).
    telemetry: Mutex<Option<TelemetrySink>>,
    /// 0.8.8 Slice 15 — fast OFF-path guard. `false` (default) → search does ZERO
    /// telemetry work: a single `Relaxed` atomic load, NO mutex acquisition (the
    /// §B.1 footprint / zero-cost gate, codex §9 P2). Set `true` by
    /// `enable_telemetry` after the sink is installed; the `telemetry` mutex is only
    /// ever taken when this flag is set.
    telemetry_enabled: AtomicBool,
    /// 0.8.18 Slice 5 (#5 vector-equivalence probe, R-VEQ-4/6) — degraded-open
    /// latch, re-derived at every open by the #5 self-check. `true` ⇒ every
    /// vector-dependent arm refuses at the `search_inner_with_stats` choke point
    /// with `EngineError::VectorEquivalenceMismatch`. Read lock-free on the query
    /// hot path (a single `Relaxed`/`Acquire` load); the text-only/FTS-only path
    /// never reads it.
    dense_disabled: AtomicBool,
    /// R-VEQ-6 — the human-readable reason attached to the query-time refusal (and
    /// surfaced on `OpenReport.dense_disabled_reason`). Set once at open; read only
    /// when `dense_disabled` is `true`.
    dense_disabled_reason: Mutex<Option<String>>,
    /// R-VEQ-6 — telemetry counter: number of query-time vector-dependent-arm
    /// refusals raised because the engine opened in the `dense_disabled` state.
    /// Observable pre/post-query via `vector_equivalence_refusal_count`.
    vector_equivalence_refusals: AtomicU64,
    #[cfg(debug_assertions)]
    force_next_commit_failure: AtomicBool,
}

/// 0.8.8 Slice 15 (OPP-9) — opt-in telemetry capture state (per `enable_telemetry`).
/// Records query→result→feedback events to a local JSONL sink. Ids are
/// `SearchHit.id` — the interim identity carrier per
/// `ADR-0.8.0-canonical-identity-substrate` (write_cursor today; swaps to
/// `logical_id` at the G0 keystone with no carrier reshape), consistent with
/// `PerHitExplain.id`. Query text and `source_id` are NEVER captured (privacy, ADR
/// §C). `query_id = "q{nonce}-{seq}"` is fully deterministic; `ts_monotonic_ms` is
/// monotonic since enable (NOT wall-clock).
struct TelemetrySink {
    path: PathBuf,
    base: Instant,
    nonce: u64,
    seq: u64,
    last_query_id: Option<String>,
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
    /// 0.8.16 Slice 5 / F9 — dedicated importance/confidence reweight flag,
    /// **off by default** (mirrors `recency_reweight_enabled`; NOT `fusion_mode`).
    /// When set, fused hits are multiplicatively reweighted by node `importance`
    /// (`canonical_nodes.importance`) and edge `confidence`
    /// (`canonical_edges.confidence`) AFTER bit-KNN + RRF fusion — `NULL ⇒ neutral
    /// (1.0)`. Flipped by `set_importance_reweight_enabled_for_test`; no production
    /// toggle yet (F9 ships OFF-by-default as a MECHANISM, no eval-quality claim).
    importance_reweight_enabled: AtomicBool,
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
        /// F9 (0.8.16 Slice 5) — whether the dedicated importance/confidence
        /// reweight is enabled for this request (read from
        /// `importance_reweight_enabled`, off by default).
        importance_enabled: bool,
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
        /// 0.8.5 (EXP-0) — CE-blend weight (clamped to `[0,1]` in `ce_rerank`).
        /// `0.3` is the byte-identical default; `1.0` is the measured-parity config.
        alpha: f64,
        /// 0.8.5 (EXP-0) — reranked-pool size (clamped to the hit count). The
        /// binding resolves `pool_n.unwrap_or(rerank_depth)` before dispatch.
        pool_n: usize,
        /// 0.8.8 EXP-OBS (Slice 5) — when `true`, capture per-arm ranks + the
        /// fused/CE score breakdown + query trace into a `SearchResult`
        /// `Explanation` sidecar. `false` (the default for `search`/`search_filtered`/
        /// `search_reranked`) does ZERO extra work and returns `explanation = None`
        /// (R-OBS-2 zero-cost; byte-identical `results`).
        explain: bool,
        /// 0.8.20 Slice 15b fix-2 (R-20-NV / R-20-RV) — the VALIDITY view the
        /// node-hydration SELECTs filter by. `ReadView::default()` reproduces
        /// the pre-fix predicate on any corpus that never authored a window
        /// (step 22 back-filled NULL/NULL with no DEFAULT, and `validity_sql`
        /// treats NULL as unbounded ⇒ the conjunct is a provable no-op there).
        /// The existence axis is refused upstream, never carried here.
        view: ReadView,
        respond: SyncSender<ReaderResponse>,
    },
    /// Slice 30 (G2) — active-only point lookup by `logical_id`. Returns one
    /// slot per requested id, in request order, `None` where no active row
    /// carries that id. Its own typed `respond` channel keeps the `Search`
    /// `ReaderResponse` byte-identical (no Search regression).
    GetById {
        logical_ids: Vec<String>,
        /// R-20-RV — the read view this lookup runs under. `ReadView::default()`
        /// is the strict (pre-slice) view.
        view: ReadView,
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
        /// R-20-RV — the read view this listing runs under.
        view: ReadView,
        respond: SyncSender<rusqlite::Result<Vec<NodeRecord>>>,
    },
    /// Slice 20 (G5) — bounded BFS from a single root node over
    /// `canonical_edges`. Returns the set of reachable nodes (excluding the
    /// root) within `depth` hops, limited to the hard cap 50.
    GraphNeighbors {
        root_logical_id: String,
        depth: u32,
        direction: TraversalDirection,
        /// R-20-RV — the read view applied at EVERY node position of the BFS
        /// CTE (anchor, recursive join, final projection), for every direction.
        view: ReadView,
        respond: SyncSender<rusqlite::Result<Vec<NodeRecord>>>,
    },
    /// 0.8.20 Slice 10b (R-20-NV) — nodes that crossed a validity boundary in
    /// `(since, view-instant]`.
    CrossedBoundarySince {
        since: i64,
        view: ReadView,
        respond: SyncSender<rusqlite::Result<Vec<BoundaryCrossing>>>,
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
    /// OPP-12 Phase-1 (0.8.19 Slice 10, design §3 gap-4) — debug-only request
    /// that asks a worker to read its own connection's `PRAGMA secure_delete`
    /// and return it (`0`/`1`). Used solely by the gap-4 test that asserts the
    /// standing secure_delete flag is ON at EVERY open, not just the writer.
    #[cfg(debug_assertions)]
    SecureDeleteStatus {
        respond: SyncSender<i64>,
    },
}

// G0 Phase-2: the Search response carries a 4th element — the graph-arm frontier
// meter (`GraphFrontierStats`). It rides the internal channel but is dropped before
// `SearchResult` is built (kept OFF the governed surface); the
// `_graph_frontier_stats_for_test` seam captures it. Default (all-zero) on non-graph paths.
// 0.8.8 EXP-OBS (Slice 5): the Search response carries a 5th element — the opt-in
// retrieval `Explanation` (`None` on every default `explain=false` path; `Some`
// only on the `search_explained` path). Like the `GraphFrontierStats` 4th element
// it rides the internal channel as a side-channel; unlike it, the explanation IS
// surfaced (onto `SearchResult.explanation`) when requested.
type ReaderResponse = rusqlite::Result<(
    u64,
    Option<SoftFallback>,
    Vec<SearchHit>,
    GraphFrontierStats,
    Option<Explanation>,
)>;

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

    /// OPP-12 Phase-1 (0.8.19 Slice 10, design §3 gap-4) — broadcast a
    /// `SecureDeleteStatus` request to every worker and collect each worker's
    /// `PRAGMA secure_delete` value. Same broadcast pattern as G.1's
    /// `lookaside_used_per_worker`. Proves the standing secure_delete flag is ON
    /// on the reader-pool connections, not just the writer.
    #[cfg(debug_assertions)]
    fn secure_delete_per_worker(&self) -> Vec<i64> {
        let mut results = Vec::with_capacity(self.senders.len());
        for sender in &self.senders {
            let (tx, rx) = mpsc::sync_channel::<i64>(1);
            if sender.send(ReaderRequest::SecureDeleteStatus { respond: tx }).is_ok() {
                results.push(rx.recv().unwrap_or(-1));
            } else {
                results.push(-1);
            }
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
                importance_enabled,
                vector_stage_only,
                raw_query,
                rerank_depth,
                use_graph_arm,
                alpha,
                pool_n,
                explain,
                view,
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
                    importance_enabled,
                    vector_stage_only,
                    &raw_query,
                    rerank_depth,
                    use_graph_arm,
                    alpha,
                    pool_n,
                    explain,
                    view,
                );
                // Receiver may have been dropped if the caller went
                // away; nothing to do in that case.
                let _ = respond.send(result);
            }
            ReaderRequest::GetById { logical_ids, view, respond } => {
                let result = read_get_by_id_in_tx(&mut connection, &logical_ids, &view);
                let _ = respond.send(result);
            }
            ReaderRequest::ReadCollection { collection, after_id, limit, respond } => {
                let result = read_collection_in_tx(&mut connection, &collection, after_id, limit);
                let _ = respond.send(result);
            }
            ReaderRequest::ReadList { kind, predicates, limit, view, respond } => {
                let result = read_list_in_tx(&mut connection, &kind, &predicates, limit, &view);
                let _ = respond.send(result);
            }
            ReaderRequest::GraphNeighbors { root_logical_id, depth, direction, view, respond } => {
                let result = graph_neighbors_in_tx(
                    &mut connection,
                    &root_logical_id,
                    depth,
                    direction,
                    &view,
                );
                let _ = respond.send(result);
            }
            ReaderRequest::CrossedBoundarySince { since, view, respond } => {
                let result = crossed_boundary_since_in_tx(&mut connection, since, &view);
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
            #[cfg(debug_assertions)]
            ReaderRequest::SecureDeleteStatus { respond } => {
                let value: i64 =
                    connection.query_row("PRAGMA secure_delete", [], |r| r.get(0)).unwrap_or(-1);
                let _ = respond.send(value);
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
            importance_reweight_enabled: AtomicBool::new(false),
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
    /// 0.8.18 Slice 5 (#5 vector-equivalence probe, R-VEQ-6) — degraded-open
    /// observability. `true` iff the open-time #5 self-check re-embedded the 45
    /// committed probes and found a divergence beyond the frozen D4 floor (a
    /// Phase-1 mean-centered `embedding_bin` sign flip OR a Phase-2 un-centered
    /// L2 over `VECTOR_EQUIVALENCE_L2_EPSILON`). When `true`, `Engine::open`
    /// SUCCEEDED but every vector-dependent arm refuses at query time with
    /// `EngineError::VectorEquivalenceMismatch`; the text-only/FTS-only path stays
    /// serviceable. The state is RE-DERIVED at every open (the probe re-runs), so
    /// a reopen with a still-divergent backend stays degraded (never silently
    /// re-enables dense) and a reopen with a matching backend clears it.
    pub dense_disabled: bool,
    /// R-VEQ-6 — human-readable reason for `dense_disabled` (which representation
    /// tripped: P1 flip count or P2 L2). `None` when `dense_disabled == false`.
    pub dense_disabled_reason: Option<String>,
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
/// `source_id` (G0 Phase-2 / BLOCK-2; generalised by TC-31 in 0.8.20 Slice 10a)
/// carries the source-document provenance of a hit — the identifier
/// [`Engine::erase_source`] consumes. It is populated on **every** hit path:
/// - **Node hits** (text/BM25F, vector, and the pre-step-12 legacy text
///   fallback) carry the **node's own** `canonical_nodes.source_id`.
/// - **Edge hits** (edge-FTS from `search_index_edges`, and edge-fact hits
///   hydrated by the vector arm) carry the **edge's own**
///   `canonical_edges.source_id`.
/// - **GraphArm** hits carry the **traversed edge's** `source_id` (the session
///   the fact-edge was extracted from) — unchanged by TC-31 — enabling
///   `doc_id_of` to resolve a graph-reached entity back to a gold session id.
///
/// Before TC-31 only the GraphArm branch populated this, which left
/// `erase_source` shipping with its argument unreachable from a text or vector
/// hit (0.8.19 also stopped surfacing `write_cursor` to the SDKs, removing the
/// only fallback route). It stays `Option<String>`: a row written before 0.8.20,
/// or a GOVERNED row deliberately spared by the step-21 backfill under the TC-11
/// pin, legitimately carries NULL at rest and must read back as `None` rather
/// than a fabricated value.
///
/// The field never participates in ranking, so result order and scores are
/// unaffected.
///
/// C-2 (0.8.19 / OPP-12 record-lifecycle Phase-1, TC-8) — the **id-space** of a
/// [`SearchHit::id`]. A closed, typed enum (NOT a magic-prefixed string) — the
/// C-2 binding ratified in the OPP-12 protocol:
/// - [`Logical`](IdSpaceKind::Logical) — `"l:"`, a governed/canonical node keyed
///   by its `logical_id` (the only lifecycle-addressable space).
/// - [`Content`](IdSpaceKind::Content) — `"h:"`, a doc-seeded/anonymous node
///   keyed by a content hash of its body (the dominant corpus hit class).
/// - [`Passage`](IdSpaceKind::Passage) — `"p:"`, a synthetic `rerank_passages`
///   hit keyed by the caller-supplied passage ordinal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IdSpaceKind {
    /// `"l:"` — governed/canonical node (its `logical_id`).
    Logical,
    /// `"h:"` — doc-seeded/anonymous node (content hash of the body).
    Content,
    /// `"p:"` — synthetic rerank passage (caller-supplied ordinal).
    Passage,
}

impl IdSpaceKind {
    /// The two-char id-space prefix (`"l:"` / `"h:"` / `"p:"`) used in the
    /// prefixed string form. Byte-identical to the pre-swap `derive_stable_id`
    /// tags so real-gold keying stays a no-op.
    #[must_use]
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Logical => "l:",
            Self::Content => "h:",
            Self::Passage => "p:",
        }
    }

    /// The lowercase discriminant (`"logical"` / `"content"` / `"passage"`)
    /// surfaced through the SDK bindings as the `IdSpace.space` field (mirrors
    /// how `SoftFallbackBranch` is surfaced as a `branch` string).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Logical => "logical",
            Self::Content => "content",
            Self::Passage => "passage",
        }
    }
}

/// C-2 (0.8.19 / OPP-12 Phase-1, TC-8) — the typed, non-null, id-space-**total**
/// carrier for [`SearchHit::id`]. Subsumes the interim `write_cursor` id AND the
/// additive Cause-A `stable_id` field of prior releases: the `value` is the BARE
/// id (prefix stripped), and [`to_prefixed`](IdSpace::to_prefixed) reproduces the
/// pre-swap `stable_id` string byte-for-byte (`l:`/`h:` unchanged) so
/// cross-session real-gold keying continues on `id` as a true no-op.
///
/// Lifecycle-addressability is a type check consumed downstream by the
/// `transition`/`purge` verbs: only [`Logical`](IdSpaceKind::Logical) is
/// lifecycle-addressable; `Content`/`Passage` are total-but-not-addressable.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IdSpace {
    /// The typed id-space (`Logical`/`Content`/`Passage`).
    pub space: IdSpaceKind,
    /// The bare id value (id-space prefix stripped).
    pub value: String,
}

impl IdSpace {
    /// A `Logical` (`"l:"`) id carrying `value` (a `logical_id`).
    pub fn logical(value: impl Into<String>) -> Self {
        Self { space: IdSpaceKind::Logical, value: value.into() }
    }

    /// A `Content` (`"h:"`) id carrying `value` (a content hash).
    pub fn content(value: impl Into<String>) -> Self {
        Self { space: IdSpaceKind::Content, value: value.into() }
    }

    /// A `Passage` (`"p:"`) id carrying `value` (a caller-supplied ordinal).
    pub fn passage(value: impl Into<String>) -> Self {
        Self { space: IdSpaceKind::Passage, value: value.into() }
    }

    /// The prefixed string form (`{prefix}{value}`) — byte-identical to the
    /// pre-swap `derive_stable_id` output for `l:`/`h:`.
    #[must_use]
    pub fn to_prefixed(&self) -> String {
        format!("{}{}", self.space.prefix(), self.value)
    }

    /// Parse the prefixed string form back into a typed `IdSpace`. Round-trip
    /// stable: `IdSpace::parse(&x.to_prefixed()) == Some(x)`. Only the FIRST
    /// two-char id-space prefix is stripped, so a value that itself contains
    /// `":"` round-trips unchanged. Returns `None` for an untagged string.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        if let Some(v) = s.strip_prefix("l:") {
            Some(Self::logical(v))
        } else if let Some(v) = s.strip_prefix("h:") {
            Some(Self::content(v))
        } else {
            s.strip_prefix("p:").map(Self::passage)
        }
    }
}

impl std::fmt::Display for IdSpace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.space.prefix(), self.value)
    }
}

/// Derives `Clone, Debug, PartialEq` but **not `Eq`** — `score: f64` forbids
/// total equality.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    /// C-2 (0.8.19 / TC-8) — the typed, non-null, id-space-total hit id
    /// ([`IdSpace`]). Was the interim `write_cursor: u64` in prior releases; now
    /// carries the cross-session-stable key: `value` is the BARE (prefix-stripped)
    /// id, and [`to_prefixed`](IdSpace::to_prefixed) (== `{prefix}{value}`)
    /// reproduces the pre-swap `stable_id` byte-for-byte (the real-gold-keying
    /// no-op). Governed hits are `l:`, doc-seeded hits `h:`, synthetic
    /// passages `p:`. This is the caller-facing identity; the positional
    /// `write_cursor` below is engine-internal book-keeping.
    pub id: IdSpace,
    /// Engine-internal positional cursor (the value `id` carried before the C-2
    /// swap). Reassigned on every re-projection/re-ingest — NOT cross-session
    /// stable, NOT the caller-facing id. Retained because the engine still needs
    /// a positional cursor for its own book-keeping (vector rowid mapping, the
    /// `state='active'` filter lookups, RRF recency/importance reweight keys,
    /// telemetry `result_ids` keying, `search_expand` re-resolution). The SDK
    /// bindings do NOT surface it.
    pub write_cursor: u64,
    pub kind: String,
    pub body: String,
    pub score: f64,
    pub branch: SoftFallbackBranch,
    pub source_id: Option<String>,
    /// 0.8.5 (EXP-0) — per-candidate cross-encoder score `ce_norm =
    /// sigmoid(ce_logit) ∈ [0,1]`. `Some` ONLY for hits inside the reranked pool
    /// (the top `pool_n` when the CE model is loaded); `None` for the unreranked
    /// remainder, the `rerank_depth == 0` identity path, an empty list, and the
    /// no-CE-model soft-fallback. Additive + nullable: it never participates in
    /// ranking, so default-path ordering/scores stay byte-stable.
    pub ce_score: Option<f64>,
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

/// 0.8.20 Slice 10b (R-20-RV / R-20-NV) — the **read view**: the single knob
/// that decides which `canonical_nodes` rows a read verb may see.
///
/// Every field is a *relaxation*: `ReadView::default()` is the STRICT view and
/// compiles to exactly the predicates the five read verbs carried before this
/// slice (`superseded_at IS NULL AND state = 'active'`), so the default read
/// path is behaviourally unchanged. Flags compose INDEPENDENTLY — each one
/// drops exactly one conjunct and no other.
///
/// The view is applied UNIFORMLY by [`Engine::read_get`],
/// [`Engine::read_get_many`], [`Engine::read_list`],
/// [`Engine::read_list_filter`] and [`Engine::graph_neighbors`] — and, inside
/// `graph_neighbors`, at EVERY position of EVERY direction's recursive CTE
/// (anchor, recursive join, final projection), so a relaxation cannot silently
/// apply on one traversal position and not another.
///
/// # World-time only
///
/// `valid_as_of` selects along the **world-time** (validity) axis only.
/// Transaction-time / `history_as_of` is explicitly OUT OF SCOPE — this type
/// deliberately has no way to ask "what did the database believe at time T".
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReadView {
    /// Relax `superseded_at IS NULL` — include superseded (historical) versions
    /// of a row, not just the current one. `false` (default) keeps the shipped
    /// current-version-only behaviour.
    ///
    /// On the point-lookup verbs ([`Engine::read_get`] /
    /// [`Engine::read_get_many`]) a `logical_id` can now match several rows;
    /// the slot resolves DETERMINISTICALLY to the highest `write_cursor` (the
    /// most recent version). Use [`Engine::read_list`] to enumerate history.
    pub include_superseded: bool,

    /// Relax `state = 'active'` — include rows in a non-`active` lifecycle
    /// state (`pending` / `deleted` / `purged`). `false` (default) keeps the
    /// shipped active-only behaviour.
    pub include_inactive: bool,

    /// Relax the validity-window predicate ENTIRELY — return rows whatever
    /// their `[valid_from, valid_until)` window, ignoring `valid_as_of`.
    /// `false` (default) filters to rows valid at the selected instant.
    ///
    /// Note this is a NO-OP on any row with an unbounded (NULL/NULL) window,
    /// which is every row that predates schema step 22.
    pub include_out_of_window: bool,

    /// The instant (INTEGER epoch SECONDS, UTC) at which validity is evaluated.
    /// `None` (default) resolves to *now* at query time.
    ///
    /// This is the **`:now` seam**: whichever way it resolves, the instant is
    /// compiled as a BOUND PARAMETER, never a `datetime('now')` SQL literal —
    /// which is what makes node validity deterministically testable without
    /// clock games. (The shipped EDGE temporal filter still inlines
    /// `datetime('now')`; that path is untouched by this slice.)
    pub valid_as_of: Option<i64>,
}

impl ReadView {
    /// The instant to bind for the validity predicate, or `None` when the view
    /// relaxes validity entirely (in which case no `:now` parameter is emitted
    /// and none must be bound).
    fn now_param(&self) -> Option<i64> {
        if self.include_out_of_window {
            return None;
        }
        Some(self.valid_as_of.unwrap_or_else(current_epoch_seconds))
    }

    /// The existence conjunct for node-table `alias`. Each flag drops exactly
    /// one conjunct; the strict view reproduces the pre-slice predicate pair
    /// verbatim. Always begins with ` AND ` (or is empty), so every call site
    /// must already have a preceding `WHERE` predicate.
    fn existence_sql(&self, alias: &str) -> String {
        let mut sql = String::new();
        if !self.include_superseded {
            sql.push_str(&format!(" AND {alias}.superseded_at IS NULL"));
        }
        if !self.include_inactive {
            sql.push_str(&format!(" AND {alias}.state = 'active'"));
        }
        sql
    }

    /// The validity conjunct for node-table `alias`, bound to positional
    /// parameter `?{now_idx}`. Empty when validity is relaxed.
    ///
    /// Encodes the HALF-OPEN window `[valid_from, valid_until)` with NULL
    /// meaning unbounded on that side — so a NULL/NULL row is valid at every
    /// instant and this conjunct never changes its visibility.
    fn validity_sql(&self, alias: &str, now_idx: usize) -> String {
        if self.include_out_of_window {
            return String::new();
        }
        format!(
            " AND ({alias}.valid_from IS NULL OR {alias}.valid_from <= ?{now_idx}) \
             AND ({alias}.valid_until IS NULL OR {alias}.valid_until > ?{now_idx})"
        )
    }

    /// The full node predicate (existence + validity) for `alias`. This is the
    /// ONE function every read site calls, so no site can drift from another.
    fn node_sql(&self, alias: &str, now_idx: usize) -> String {
        format!("{}{}", self.existence_sql(alias), self.validity_sql(alias, now_idx))
    }

    /// 0.8.20 Slice 15b fix-3 (F2) — resolve this view's validity instant ONCE
    /// and hand back a [`FrozenView`] that carries the resolved value.
    ///
    /// This is the ONLY constructor of a `FrozenView`, and therefore the only
    /// point on the search path where the wall clock is read.
    fn freeze(self) -> FrozenView {
        // TC-33: resolve the instant ONCE, unconditionally, and derive both
        // axes from it. `valid_as_of.unwrap_or_else(current_epoch_seconds)` is
        // exactly what `now_param()` computes, so the clock is read the same
        // number of times as before on every path that reads it at all.
        let resolved = self.valid_as_of.unwrap_or_else(current_epoch_seconds);
        let now = if self.include_out_of_window { None } else { Some(resolved) };
        FrozenView { view: self, now, edge_now: resolved }
    }

    /// 0.8.20 Slice 15b fix-2 — the `search` path honours the VALIDITY axis of a
    /// `ReadView` and refuses the EXISTENCE axis. See [`Engine::search_view`] for
    /// why refusing beats silently ignoring.
    fn reject_existence_relaxation_on_search(&self) -> Result<(), EngineError> {
        let relaxed = match (self.include_superseded, self.include_inactive) {
            (true, true) => "include_superseded + include_inactive",
            (true, false) => "include_superseded",
            (false, true) => "include_inactive",
            (false, false) => return Ok(()),
        };
        Err(EngineError::InvalidArgument {
            msg: format!(
                "ReadView.{relaxed} is not supported on the search path; search hydrates from \
                 projection indexes that are not version-complete, so only the validity axis \
                 (valid_as_of / include_out_of_window) is honoured. Use read_list for history."
            ),
        })
    }
}

/// 0.8.20 Slice 10b (R-20-NV) — one node that crossed a validity boundary
/// inside the interrogated interval, as reported by
/// [`Engine::crossed_boundary_since`].
///
/// A node can cross BOTH boundaries in the same interval (a window that opened
/// and closed inside it), so the two fields are independent `Option`s rather
/// than one enum.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundaryCrossing {
    /// The node that crossed.
    pub node: NodeRecord,
    /// `Some(valid_from)` when the node BECAME VALID inside the interval.
    pub became_valid_at: Option<i64>,
    /// `Some(valid_until)` when the node BECAME INVALID inside the interval.
    pub became_invalid_at: Option<i64>,
}

/// 0.8.20 Slice 15b fix-3 (F2) — a [`ReadView`] whose validity instant has
/// ALREADY been resolved, produced only by [`ReadView::freeze`].
///
/// R-20-NV requires `:now` to bind ONCE PER QUERY — not per row, and not per
/// ARM. The multi-arm search path made that easy to violate: each arm held a
/// `ReadView` and could call `now_param()`, which for the default view
/// (`valid_as_of == None`) reads the wall clock. Two arms, two instants, and a
/// query straddling a validity boundary gets nondeterministic membership.
///
/// The fix is TYPE-LEVEL rather than a comment asking future arms to behave:
/// the instant is resolved once at the top of `read_search_in_tx` and every arm
/// receives a `FrozenView`, which stores the resolved value in `now` and has NO
/// path back to the clock. An arm cannot re-resolve the instant because it
/// never holds anything that could — the failure mode is unreachable, not
/// merely discouraged.
#[derive(Clone, Copy, Debug)]
struct FrozenView {
    /// The underlying view — consulted for SQL SHAPE only (which conjuncts to
    /// emit), never to re-resolve the instant.
    view: ReadView,
    /// The instant resolved at freeze time. `None` ⇔ the view relaxes validity
    /// entirely, in which case no conjunct is emitted and nothing is bound.
    now: Option<i64>,
    /// TC-33 — the instant EDGE validity is evaluated at. Always present.
    ///
    /// The EXISTENCE-relaxation flag `include_out_of_window` belongs to the NODE
    /// validity axis and does NOT relax edge recency: an edge invalidated in the
    /// past stays excluded regardless. So this is the resolved instant even when
    /// `now` is `None`, and it is resolved from the SAME clock read.
    edge_now: i64,
}

impl FrozenView {
    /// The instant to bind, resolved at freeze time. Unlike
    /// [`ReadView::now_param`] this is a stored value: calling it a second time
    /// cannot yield a different answer, and it never touches the clock.
    fn now_param(&self) -> Option<i64> {
        self.now
    }

    /// TC-33 — the instant to bind for the EDGE-validity conjunct
    /// ([`edge_validity_sql`]). Frozen, like [`FrozenView::now_param`].
    ///
    /// Honouring `valid_as_of` here is what finally UNIFIES the node and edge
    /// temporal axes: step 22 recorded "the shipped EDGE path still inlines
    /// `datetime('now')`" as the reason they could not be unified. For the
    /// DEFAULT view (`valid_as_of == None`) this is the wall clock, i.e. exactly
    /// the pre-TC-33 behaviour.
    fn edge_now(&self) -> i64 {
        self.edge_now
    }

    /// The validity conjunct — delegated to the one generator every read site
    /// shares, so the search arms cannot drift from the five read verbs.
    fn validity_sql(&self, alias: &str, now_idx: usize) -> String {
        self.view.validity_sql(alias, now_idx)
    }
}

/// 0.8.20 Slice 15b fix-3 (F2) — how many times [`current_epoch_seconds`] has
/// been called in this process. Test-only observation; see
/// [`clock_reads_for_test`].
static CLOCK_READS: AtomicU64 = AtomicU64::new(0);

/// Test seam — the process-wide count of wall-clock reads on the validity path.
/// Kept OFF the governed surface (`#[doc(hidden)]`, `_for_test`), mirroring the
/// sanctioned `set_vector_stage_only_for_test` / `vector_phase1_sql_for_test`
/// pattern; it is never re-exported from the `fathomdb` facade.
///
/// The counter is PROCESS-WIDE, so a test asserting on a delta must hold a
/// lock that excludes every other clock-reading test in its binary (test
/// binaries are separate processes, so only intra-binary contention matters).
/// `slice15b_search_validity_recall.rs` does this with a file-local mutex.
#[doc(hidden)]
#[must_use]
pub fn clock_reads_for_test() -> u64 {
    CLOCK_READS.load(Ordering::Relaxed)
}

/// Wall-clock now as INTEGER epoch SECONDS (UTC), saturating at 0 before the
/// Unix epoch. The single place the node-validity path reads the clock — and it
/// is read in RUST, then BOUND, never inlined into SQL as `datetime('now')`.
fn current_epoch_seconds() -> i64 {
    // 0.8.20 Slice 15b fix-3 (F2) — meter every wall-clock read on the validity
    // path. R-20-NV requires `:now` to bind ONCE PER QUERY (not per row, not per
    // ARM): if two arms of one query each resolve *now*, a query that straddles
    // a validity boundary can have its arms disagree about which side they are
    // on. That is invisible to a result-shape assertion and unreachable by a
    // deterministic test — you cannot assert on a race. Counting the reads makes
    // the property testable WITHOUT racing the clock, and keeps failing for any
    // arm added later that re-reads it. `Relaxed` is sufficient: the counter is
    // an observation, never a synchronization point.
    CLOCK_READS.fetch_add(1, Ordering::Relaxed);
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// TC-33 — the edge-validity conjunct, bound to positional parameter
/// `?{now_idx}`. THE one generator for "is this edge valid at `:now`", so no
/// read site can drift from another (the same discipline
/// [`ReadView::validity_sql`] applies to node validity).
///
/// An edge is valid at `t` iff it has no invalid-time, or its invalid-time is
/// strictly in the future. `t_invalid` is INTEGER epoch seconds since step 23,
/// so this is a direct integer comparison — no `datetime()` conversion per row.
///
/// **`:now` is a BOUND PARAMETER, never `datetime('now')`.** Before TC-33 every
/// edge read site inlined `datetime('now')`, which made the predicate
/// non-deterministic, untestable, and re-evaluated per row; step 22's comment
/// flagged that as the reason node and edge validity could not be unified. They
/// are unified now.
///
/// Always begins with ` AND `, so every call site must already have a preceding
/// `WHERE` predicate.
fn edge_validity_sql(alias: &str, now_idx: usize) -> String {
    format!(" AND ({alias}.t_invalid IS NULL OR {alias}.t_invalid > ?{now_idx})")
}

/// TC-33 — parse one ISO-8601 timestamp to INTEGER epoch seconds using SQLite's
/// own date parser, via a BOUND parameter.
///
/// Returns `None` when SQLite cannot resolve the value — `strftime` yields SQL
/// NULL for junk (`'not a date'`, `''`, `'2020-13-45T99:99:99Z'`, a bare epoch
/// string, whitespace-padded input, non-ASCII digits) and a digit string for
/// anything it understands.
///
/// **Why SQLite and not a date crate:** there is no `chrono`/`time` dependency
/// anywhere in the workspace, and HITL directed this spelling rather than adding
/// one. The value is BOUND, never interpolated.
///
/// **This does not violate the inline-clock rule.** That rule forbids
/// `datetime('now')` / `strftime('%s','now')` — an inline CLOCK. Parsing a bound
/// user value is deterministic and reads no clock. The current instant still
/// comes from the bound `:now` seam ([`current_epoch_seconds`]).
///
/// The `CAST` matters: `strftime('%s', ...)` returns TEXT (a digit string), not
/// an integer, and the column is `typeof(...) = 'integer'`-checked.
fn iso8601_to_epoch_seconds(connection: &Connection, raw: &str) -> Option<i64> {
    connection
        .query_row("SELECT CAST(strftime('%s', ?1) AS INTEGER)", params![raw], |r| {
            r.get::<_, Option<i64>>(0)
        })
        .ok()
        .flatten()
}

/// TC-33 — render INTEGER epoch seconds back to an ISO-8601 UTC string for the
/// BYO-LLM wire. The exact inverse of [`iso8601_to_epoch_seconds`].
///
/// Storage and the governed SDK are epoch seconds, but the harness protocols
/// (`fathomdb.extract.v1` and the consolidation harness) carry ISO-8601 — LLMs
/// reason about dates as text, and pushing epoch integers onto them would make
/// the wire hostile to the very providers it exists to serve. So the boundary
/// converts in BOTH directions and the representation split stays a boundary
/// concern rather than leaking into the protocol.
fn epoch_seconds_to_iso8601(connection: &Connection, epoch: i64) -> Option<String> {
    connection
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ', ?1, 'unixepoch')", params![epoch], |r| {
            r.get::<_, Option<String>>(0)
        })
        .ok()
        .flatten()
}

/// TC-33 fix-1 — the inclusive epoch-seconds range SQLite's
/// `strftime(..., 'unixepoch')` can render back to ISO-8601. SQLite's date
/// functions cover years 0000..=9999 ONLY, so:
/// - `MIN` = `0000-01-01T00:00:00Z`
/// - `MAX` = `9999-12-31T23:59:59Z`
///
/// These bounds are used only to make the write-boundary rejection message
/// name the limit. The rejection PREDICATE itself is renderability under
/// [`epoch_seconds_to_iso8601`] (i.e. SQLite's own `strftime`), so the guard
/// rejects EXACTLY the values that would later render to a silent `null` on the
/// consolidation wire — the numbers below cannot drift out of agreement with
/// the check they annotate.
const MIN_RENDERABLE_EPOCH: i64 = -62_167_219_200; // 0000-01-01T00:00:00Z
const MAX_RENDERABLE_EPOCH: i64 = 253_402_300_799; // 9999-12-31T23:59:59Z

/// TC-33 fix-1 — reject an edge epoch that SQLite cannot render back to
/// ISO-8601, at the governed write boundary, so it is UNSTORABLE.
///
/// # Why this is the primary layer
///
/// Storage and `PreparedWrite::Edge` carry INTEGER epoch seconds and accept an
/// arbitrary `i64`. The consolidation path renders each candidate's
/// `t_valid`/`t_invalid` to ISO-8601 for the LLM via `strftime(..., 'unixepoch')`,
/// which only spans years 0000..=9999. An epoch outside that range renders to
/// NULL, and the render site would then send a silent `null` for a timestamp
/// that is actually stored NON-NULL — the OUTBOUND twin of the fail-open TC-33
/// removes. A `null` `t_invalid` reads as "still valid", and the consolidation
/// reference stub echoes a winner's `t_valid` straight back as the verdict's
/// `t_invalid`, so the `null` round-trips through the inbound normaliser as
/// "still valid": an invalidated edge silently resurrected.
///
/// Inbound ISO normalisation can never MINT such an epoch (a 4-digit-year ISO
/// string maxes at 9999), so the governed integer surface is the only ingress —
/// which is exactly where this guard sits. Mirrors the inbound
/// [`normalize_extractor_timestamp`] hard-reject and the Node branch's
/// `valid_from >= valid_until` refusal: a typed [`EngineError::InvalidArgument`]
/// naming the offending value and the bound, never a silent coercion.
fn reject_unrenderable_edge_epoch(
    connection: &Connection,
    field: &str,
    value: Option<i64>,
) -> Result<(), EngineError> {
    if let Some(ts) = value {
        if epoch_seconds_to_iso8601(connection, ts).is_none() {
            return Err(EngineError::InvalidArgument {
                msg: format!(
                    "edge field `{field}` = {ts} is outside the epoch-seconds range SQLite can \
                     render to ISO-8601 ([{MIN_RENDERABLE_EPOCH}, {MAX_RENDERABLE_EPOCH}], i.e. \
                     years 0000..=9999). REJECTED rather than stored: such an epoch renders to a \
                     silent NULL on the consolidation wire, and a NULL `t_invalid` reads as \
                     \"still valid\" — resurrecting an invalidated edge."
                ),
            });
        }
    }
    Ok(())
}

/// The JSON type name of `value`, for diagnosing a mistyped extractor field.
fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// TC-33 — normalise one timestamp arriving on the **BYO-LLM extractor
/// boundary** (`fathomdb.extract.v1`) into the INTEGER epoch seconds the storage
/// and governed-SDK layers use. **HARD-REJECTS** anything it cannot normalise.
///
/// This is the layering boundary HITL ratified on 2026-07-21:
/// - the **extractor wire format stays ISO-8601 strings** — LLMs emit text, and
///   this function is the one place that changes;
/// - **storage and the governed SDK surface are INTEGER epoch seconds.**
///
/// # Why rejection, not coercion — fail-open is the defect
///
/// A NULL `t_invalid` means **"still valid"**. So any path that turns an
/// unparseable timestamp into NULL silently **resurrects an invalidated edge**.
/// Two distinct fail-opens are closed here:
///
/// 1. **Malformed strings.** Previously NOTHING parsed or validated these; junk
///    went verbatim into the INSERT. Under the old TEXT column it then failed
///    CLOSED by accident (`datetime('junk')` → NULL ⇒ the read disjunct is
///    falsy ⇒ the row vanished). Under INTEGER that polarity would INVERT.
/// 2. **Non-string JSON — a fail-open that PREDATES TC-33.** The old site read
///    `edge.get("t_invalid").and_then(|v| v.as_str())`, and `as_str()` returns
///    `None` for a JSON number/bool/object. So `"t_invalid": 1710000000` — a
///    plausible mistake, and exactly the epoch form storage now uses — had its
///    invalidation SILENTLY DISCARDED and the edge stored as "still valid".
///
/// `None`/JSON `null`/absent is the ONLY sanctioned way to say "unknown"; it
/// maps to `Ok(None)` and keeps the NULL-means-still-valid semantic.
///
/// Follows the typed-`InvalidArgument`-carrying-the-offending-value pattern of
/// the `validate_write` `Node` branch's `valid_from >= valid_until` check.
fn normalize_extractor_timestamp(
    connection: &Connection,
    field: &str,
    raw: Option<&Value>,
) -> Result<Option<i64>, EngineError> {
    match raw {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => match iso8601_to_epoch_seconds(connection, text) {
            Some(epoch) => Ok(Some(epoch)),
            None => Err(EngineError::InvalidArgument {
                msg: format!(
                    "extractor edge field `{field}` must be an ISO-8601 timestamp SQLite can \
                     parse; got {text:?}, which `strftime('%s', ?)` resolves to NULL. REJECTED \
                     rather than stored as NULL: a NULL `t_invalid` reads as \"still valid\" and \
                     would silently resurrect an invalidated edge. Use JSON null for \"unknown\"."
                ),
            }),
        },
        Some(other) => Err(EngineError::InvalidArgument {
            msg: format!(
                "extractor edge field `{field}` must be an ISO-8601 string or JSON null; got a \
                 JSON {kind} ({other}). The `fathomdb.extract.v1` wire format carries ISO-8601 at \
                 this boundary — INTEGER epoch seconds are the STORAGE representation, not the \
                 wire one. REJECTED rather than coerced to NULL, which reads as \"still valid\".",
                kind = json_type_name(other)
            ),
        }),
    }
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
// 0.8.8 EXP-OBS (field-set ratification): non_exhaustive so future additive fields
// (e.g. the deferred QueryTrace.timings_ms, Q3) are non-breaking. All construction
// is in-crate (engine + tests); external crates read fields only.
#[non_exhaustive]
pub struct SearchResult {
    pub projection_cursor: u64,
    pub soft_fallback: Option<SoftFallback>,
    pub results: Vec<SearchHit>,
    /// 0.8.8 EXP-OBS (Slice 5) — opt-in retrieval explanation **sidecar**.
    /// `Some` ONLY on the `search_explained` path; `None` for every default
    /// (`explain=false`) search, so `results` + `projection_cursor` stay
    /// byte-identical to the pre-0.8.8 shape (R-OBS-2 zero-cost contract,
    /// HITL-ratified sidecar carrier — see
    /// `dev/design/0.8.8-explain-and-telemetry-adr.md` §A.2). Field-set is
    /// PROPOSED/ratification-pending; additive inside `Explanation` so later
    /// amendments do not reshape `SearchResult`/`SearchHit`.
    pub explanation: Option<Explanation>,
}

/// 0.8.8 EXP-OBS (Slice 5) — the opt-in retrieval explanation payload returned
/// behind `search_explained` (the `explain=true` surface). Built from the
/// engine's OWN fusion/rerank machinery (`fuse_three_arms` per-arm ranks,
/// `ce_rerank` blend components) — no parallel machinery (R-OBS-3). Carries a
/// query-level [`QueryTrace`] plus a per-hit breakdown parallel to (and in the
/// same order as) `SearchResult.results`.
///
/// Derives `Clone, Debug, PartialEq` but **not `Eq`** — scores are `f64`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive] // 0.8.8 field-set ratification — additive-safe sidecar
pub struct Explanation {
    pub trace: QueryTrace,
    pub per_hit: Vec<PerHitExplain>,
}

/// 0.8.8 EXP-OBS (Slice 5) — query-level retrieval trace. Reuses the existing
/// `search_reranked` knobs + the active embedder identity; timings are coarse
/// per-stage wall-clock (monotonic) captured only on the explain path.
#[derive(Clone, Debug, PartialEq)]
// 0.8.8 field-set ratification — HARD: leaf absorbs the deferred `timings_ms` (Q3)
// and any future trace field without a contract break.
#[non_exhaustive]
pub struct QueryTrace {
    /// Query LENGTH only (chars) — never the query text (privacy; ADR §C).
    pub query_chars: u32,
    /// Final result limit (`SEARCH_RERANK_LIMIT`-derived `final_limit`).
    pub k: u32,
    pub rerank_depth: u32,
    pub pool_n: u32,
    pub alpha: f64,
    pub use_graph_arm: bool,
    /// Recency reweight (the dedicated G12 flag) was applied.
    pub recency: bool,
    /// Active embedder identity `name@revision` (+ dim), or empty when none.
    pub embedder_id: String,
    /// The CE cross-encoder actually reranked the pool (model loaded + depth>0).
    pub ce_active: bool,
    /// Per-arm input hit counts (pre-fusion).
    pub vector_hits: u32,
    pub text_hits: u32,
    pub graph_hits: u32,
}

/// 0.8.8 EXP-OBS (Slice 5) — per-hit provenance + score breakdown. One entry per
/// returned `SearchHit`, same order. `*_rank` is the 0-based rank the hit's body
/// held in that arm's pre-fusion list (`None` = absent from that arm).
///
/// Derives `Clone, Debug, PartialEq` but **not `Eq`** — scores are `f64`.
#[derive(Clone, Debug, PartialEq)]
// 0.8.8 field-set ratification — HARD: leaf absorbs future arms / score components.
#[non_exhaustive]
pub struct PerHitExplain {
    /// The hit's engine-internal positional `write_cursor` (the pre-C-2
    /// `SearchHit.id`). Post-0.8.19 the caller-facing `SearchHit.id` is a typed
    /// [`IdSpace`]; this field keeps carrying the positional cursor so the explain
    /// sidecar cross-references the telemetry `result_ids` space. Correlate a
    /// `PerHitExplain` to its `SearchHit` by position (both lists are 1:1, same
    /// order).
    pub id: u64,
    /// Winning arm after RRF dedup (vector-first), == `SearchHit.branch`.
    pub arm: SoftFallbackBranch,
    pub vector_rank: Option<u32>,
    pub text_rank: Option<u32>,
    pub graph_rank: Option<u32>,
    /// Raw RRF fused score AFTER recency reweight, BEFORE CE blend (the value
    /// `ce_rerank` normalizes). Faithful to the engine computation — downstream
    /// may normalize. (ADR §A.4 Q1: raw exposed; normalization deferred.)
    pub fused_score: f64,
    /// In-pool cross-encoder score `sigmoid(ce_logit) ∈ [0,1]`, == the returned
    /// `SearchHit.ce_score`; `None` outside the reranked pool / no-CE path.
    pub ce_score: Option<f64>,
    /// Final blended score, == the returned `SearchHit.score`.
    pub blended: f64,
    /// 0.8.16 Slice 5 / F9 — the node `importance` scalar applied to this hit's
    /// fused contribution when the importance reweight is ON, else the raw stored
    /// value. `None` = never assigned (graceful-absent, ranks NEUTRAL). Additive
    /// (`#[non_exhaustive]` leaf absorbs the new score component).
    pub importance: Option<f64>,
    /// 0.8.16 Slice 5 / F9 — the edge `confidence` scalar applied to this hit's
    /// graph-arm contribution when the importance reweight is ON, else the raw
    /// stored value. `None` for node hits / edges without a confidence
    /// (graceful-absent, ranks NEUTRAL).
    pub confidence: Option<f64>,
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
    &["$.status", "$.priority", "$.tags", "$.kind", "$.created_at", "$.action_kind"];

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

// ===== 0.8.11 Slice 40 (#17) — unified filter grammar (G4 + G10) =========

/// 0.8.11 Slice 40 (#17) — a single closed `FilterTerm` of the **unified**
/// filter grammar (ADR-0.8.11-filter-grammar-unification, Option A; closes
/// reserved-gap 37). Exactly **five** variants: the four G10 shorthand metadata
/// fields (`SourceType`/`Kind`/`CreatedAfter`/`Status`) plus the general G4
/// json-path [`Predicate`] (`Json`). The shorthand fields are dedicated typed
/// variants — NOT `Json(Predicate)` over `$.source_type` etc. — precisely so the
/// vec0 search backend can lower them to the *indexed* pre-KNN metadata columns
/// while typed-rejecting an arbitrary `Json` term (D3: no demotion to post-KNN
/// `json_extract`).
///
/// The grammar stays **closed** (inherits ADR-0.8.0 D-F1/D-F2/D-F4/D-F5): no
/// DSL, no caller SQL, no `JsonPathFused*`, no `*_unchecked`, no OR/nesting
/// (implicit AND only); `Json` terms are built ONLY via the validated
/// [`Predicate::json_path_eq`] / [`Predicate::json_path_compare`] constructors
/// (path allowlist enforced at construction). The shipped `ScalarValue` /
/// `ComparisonOp` / `Predicate` vocabulary is reused verbatim — no new grammar.
#[derive(Clone, Debug, PartialEq)]
pub enum FilterTerm {
    /// vec0 partition-key metadata column `source_type` (pre-KNN). On
    /// `read.list` it **constant-folds** against `resolve_source_type(kind)`
    /// (the column does not exist in `canonical_nodes`).
    SourceType(String),
    /// `kind` — the vec0 metadata column (pre-KNN). On `read.list` it
    /// constant-folds against the partition `kind` argument (D1 impl decision:
    /// constant-fold, the simpler total option vs a redundant column clause).
    Kind(String),
    /// `created_at >= bound` (unix seconds). vec0 metadata column (pre-KNN);
    /// lowers to `json_extract(body,'$.created_at') >= ?` on `read.list`.
    CreatedAfter(i64),
    /// vec0 metadata column `status` (pre-KNN); lowers to
    /// `json_extract(body,'$.status') = ?` on `read.list`.
    Status(String),
    /// The general G4 json-path predicate (unchanged shipped grammar). Resolves
    /// **only** on the `read.list` (canonical_nodes) backend; **typed-rejected**
    /// on `search_filtered` because it would require a post-KNN `json_extract`
    /// that defeats the indexed pre-KNN filter (D3 no-demotion guarantee).
    Json(Predicate),
}

/// 0.8.11 Slice 40 (#17) — the unified closed `Filter` contract. ONE superset
/// type with implicit-AND [`FilterTerm`]s, dispatched to one of **two** internal
/// compilation backends (Option A — the TYPE unifies, the COMPILATION
/// dispatches): the vec0-metadata indexed pre-KNN `WHERE` for `search_filtered`,
/// and `json_extract` over `canonical_nodes.body` for `read.list`. The shipped
/// `SearchFilter` (G10) and `Predicate` lists (G4) re-express as sugar that
/// lowers into this type (D4); the `filter=None` byte-identical-0.7.2-SQL pin is
/// preserved because the vec0 lowering routes back through the shipped
/// `vector_filter_clause` compilation verbatim.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filter {
    /// AND-combined terms (implicit AND, inherits D-F5). Empty = unfiltered.
    pub terms: Vec<FilterTerm>,
}

impl From<&SearchFilter> for Filter {
    /// D4 sugar lowering — the shipped G10 [`SearchFilter`] re-expressed as the
    /// unified [`Filter`]. Field → term in the **canonical** order
    /// (`source_type`, `kind`, `created_after`, `status`) so the round-trip back
    /// to a `SearchFilter` (and thus the produced vec0 SQL) is byte-identical.
    fn from(sf: &SearchFilter) -> Self {
        let mut terms = Vec::new();
        if let Some(s) = &sf.source_type {
            terms.push(FilterTerm::SourceType(s.clone()));
        }
        if let Some(k) = &sf.kind {
            terms.push(FilterTerm::Kind(k.clone()));
        }
        if let Some(c) = sf.created_after {
            terms.push(FilterTerm::CreatedAfter(c));
        }
        if let Some(s) = &sf.status {
            terms.push(FilterTerm::Status(s.clone()));
        }
        Filter { terms }
    }
}

impl Filter {
    /// Backend dispatch for `search_filtered` (vec0 — indexed pre-KNN). Lowers
    /// the metadata subset `{SourceType, Kind, CreatedAfter, Status}` back into a
    /// [`SearchFilter`] (which the shipped `vector_filter_clause` compiles to the
    /// pre-KNN `WHERE`), and **typed-rejects** a [`FilterTerm::Json`] term with
    /// [`EngineError::InvalidFilter`] — the explicit no-demotion guarantee (D3).
    /// Field-by-variant assignment makes the output canonical-order-independent
    /// of `terms` ordering (hand-built router filters included). A later
    /// duplicate metadata term overwrites the earlier (last-wins).
    pub fn to_search_filter(&self) -> Result<SearchFilter, EngineError> {
        let mut sf = SearchFilter::default();
        for term in &self.terms {
            match term {
                FilterTerm::SourceType(s) => sf.source_type = Some(s.clone()),
                FilterTerm::Kind(k) => sf.kind = Some(k.clone()),
                FilterTerm::CreatedAfter(c) => sf.created_after = Some(*c),
                FilterTerm::Status(s) => sf.status = Some(s.clone()),
                FilterTerm::Json(_) => {
                    return Err(EngineError::InvalidFilter {
                        reason: "arbitrary json-path predicate not supported on search_filtered; \
                                 it would require a post-KNN json_extract that defeats the \
                                 indexed pre-KNN filter (ADR-0.8.11 D3 no-demotion guarantee)"
                            .to_string(),
                    });
                }
            }
        }
        Ok(sf)
    }

    /// Backend dispatch for `read.list` (canonical_nodes — `json_extract`). The
    /// full set resolves here. Returns:
    /// - `Ok(Some(preds))` — the implicit-AND [`Predicate`] list to run; or
    /// - `Ok(None)` — a constant-folded **guaranteed-empty** result (a `Kind` or
    ///   `SourceType` term that cannot match this partition), so the caller
    ///   returns an empty `Vec` without touching SQL; or
    /// - `Err(InvalidFilter)` — a non-allowlisted path (defense-in-depth; the
    ///   shorthand lowerings only ever use allowlisted paths).
    ///
    /// Lowering (D3): `Json(p)` → `p`; `Status(s)` →
    /// `json_path_eq("$.status", Text(s))`; `CreatedAfter(b)` →
    /// `json_path_compare("$.created_at", Gte, Integer(b))`; `Kind(k)` →
    /// constant-fold vs the partition `kind` arg (no-op if equal, empty if not);
    /// `SourceType(s)` → constant-fold vs `resolve_source_type(kind)` (no-op if
    /// equal, empty otherwise — the column does not exist in `body`).
    fn lower_for_read_list(&self, kind: &str) -> Result<Option<Vec<Predicate>>, EngineError> {
        let mut preds = Vec::new();
        for term in &self.terms {
            match term {
                FilterTerm::Json(p) => preds.push(p.clone()),
                FilterTerm::Status(s) => {
                    preds.push(Predicate::json_path_eq("$.status", ScalarValue::Text(s.clone()))?);
                }
                FilterTerm::CreatedAfter(b) => {
                    preds.push(Predicate::json_path_compare(
                        "$.created_at",
                        ComparisonOp::Gte,
                        ScalarValue::Integer(*b),
                    )?);
                }
                FilterTerm::Kind(k) => {
                    // Constant-fold vs the partition argument (D1 impl decision).
                    if k != kind {
                        return Ok(None);
                    }
                }
                FilterTerm::SourceType(s) => {
                    // source_type is NOT a canonical_nodes column; it is a pure
                    // function of `kind`. Constant-fold (D2/D3).
                    match resolve_source_type(kind) {
                        Ok(resolved) if resolved == s.as_str() => {}
                        _ => return Ok(None),
                    }
                }
            }
        }
        Ok(Some(preds))
    }

    /// 0.8.11 Slice 40 — test seam: expose the vec0 backend dispatch so the
    /// unification suite can pin the typed-rejection (RED→GREEN) and that a
    /// metadata-only Filter lowers losslessly. Returns the lowered
    /// [`SearchFilter`] (or `InvalidFilter` for a `Json` term).
    #[doc(hidden)]
    pub fn to_search_filter_for_test(&self) -> Result<SearchFilter, EngineError> {
        self.to_search_filter()
    }

    /// 0.8.11 Slice 40 — test seam: expose the `read.list` backend lowering so
    /// the unification suite can pin total dispatch incl. the `SourceType`/`Kind`
    /// constant-folds. `Ok(None)` == constant-folded-empty.
    #[doc(hidden)]
    pub fn lower_for_read_list_for_test(
        &self,
        kind: &str,
    ) -> Result<Option<Vec<Predicate>>, EngineError> {
        self.lower_for_read_list(kind)
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

/// 0.8.12 Slice 15 (OPP-2, ADR-0.8.12) — one (subject-entity, relation) axis to
/// consolidate via [`Engine::consolidate_with_provider`]. FathomDB assembles the
/// competing fact-edge cluster for this axis DETERMINISTICALLY (CPU-only, no
/// LLM) by querying active `canonical_edges` where `from_id = subject_logical_id`
/// AND `kind = relation`.
#[derive(Clone, Debug)]
pub struct ConsolidateAxis {
    /// Stable `logical_id` of the subject entity (edge `from_id`).
    pub subject_logical_id: String,
    /// The relation/edge `kind` whose competing fact-edges form the cluster.
    pub relation: String,
}

/// 0.8.12 Slice 15 (OPP-2, ADR-0.8.12) — one competing fact-edge in a candidate
/// cluster sent to the consolidation harness. Assembled deterministically from
/// `canonical_edges`; sent to the harness as the request payload; the harness's
/// verdict references edges back by `edge_ref` (the edge's stable `logical_id`).
#[derive(Clone, Debug)]
pub struct ConsolidateCandidateEdge {
    /// The edge's stable `logical_id` — the ref the harness uses in its verdict.
    pub edge_ref: String,
    /// The fact/relationship text (never rewritten by consolidation — §2.1).
    pub body: Option<String>,
    /// Event valid-time as INTEGER epoch seconds (UTC), if known.
    ///
    /// TC-33: epoch seconds, NOT ISO-8601. ISO-8601 lives only on the BYO-LLM
    /// extractor wire; `normalize_extractor_timestamp` is the one boundary.
    pub t_valid: Option<i64>,
    /// Event invalid-time as INTEGER epoch seconds (UTC), if already
    /// invalidated. `None` = still valid.
    pub t_invalid: Option<i64>,
    /// Extraction confidence ∈ [0.0, 1.0], if known.
    pub confidence: Option<f64>,
    /// Provenance: originating document id.
    pub source_doc_id: Option<String>,
    /// Provenance: extractor model id from the original BYO-LLM ingest.
    pub extractor_model_id: Option<String>,
}

/// 0.8.12 Slice 15 (OPP-2, ADR-0.8.12) — receipt returned by
/// [`Engine::consolidate_with_provider`]. Consolidation records supersession /
/// recency METADATA only (§2.1): edge bodies are never rewritten and no row is
/// ever deleted, so these counts describe metadata transitions, not content
/// changes.
#[derive(Clone, Debug, Default)]
pub struct ConsolidateReceipt {
    /// Number of (subject, relation) axes with a non-empty cluster that were
    /// dispatched to the harness.
    pub clusters_processed: u64,
    /// Number of candidate edges presented across all clusters.
    pub edges_examined: u64,
    /// Number of edges the harness ruled `keep` (no metadata change).
    pub edges_kept: u64,
    /// Number of edges the harness ruled `invalidate` (t_invalid set; row + body
    /// preserved).
    pub edges_invalidated: u64,
    /// Number of edges the harness ruled `supersede`/`merge` (marked superseded
    /// via the existing G0 tombstone column; row + body preserved).
    pub edges_superseded: u64,
}

/// 0.8.20 Slice 5c (R-20-E3) — the provenance of a canonical row: which source
/// document it is attributable to, and therefore what `excise_source` must erase
/// when that source is withdrawn.
///
/// **Why a newtype and not `Option<String>`.** Erasure runs through provenance:
/// a row whose `source_id` is NULL is reachable by NO `excise_source` call and
/// is therefore **un-erasable**. Before 0.8.20 the public `PreparedWrite`
/// carried `source_id: Option<String>`, so a caller could express "no
/// provenance" and silently create such a row. A *runtime* rejection would not
/// have closed this: the facade crate re-exports `PreparedWrite` and
/// `Engine::write` is `pub`, so a caller can build the value directly and skip
/// any validation the engine performs. Replacing the field's type is what makes
/// the absence of provenance **inexpressible** rather than merely rejected —
/// the guarantee is enforced by `rustc`, not by a branch. `tests/ui/` in the
/// facade crate holds the compile-fail witness.
///
/// **This is a BREAKING change**, shipped ON by default as part of the 0.8.20
/// coordinated breaking-pair release. There is deliberately no compatibility
/// shim and no deprecation window: a shim would re-open the hole it closes.
///
/// **Reserved namespace.** Ids beginning with `_` belong to the engine and are
/// rejected by [`SourceId::new`]. Two are currently minted internally:
///
/// * [`SourceId::ENGINE_PREFIX`] (`_engine:`) — rows the engine derives for
///   itself (EXP-S coverage/graph substrate rows), which never pass through
///   `PreparedWrite` (design §4 item 6).
/// * [`SourceId::LEGACY_PRE_0_8_20`] (`_legacy:pre-0.8.20`) — stamped by schema
///   migration step 21 onto pre-0.8.20 rows that were stored with NULL
///   provenance, so they become erasable (R-20-E8). **Gated to UNGOVERNED rows
///   only** (`logical_id IS NULL`); a governed row keeps NULL `source_id` and
///   stays `purge`-addressable by its `logical_id` (TC-11 pin).
///
/// **`source_id` must not be PII.** It survives the erasure it authorises: the
/// `excise_source` audit row in `operational_mutations` records it verbatim, and
/// while 0.8.20 makes that audit row durable (design §2 defect D-A) the rule was
/// always that the handle you erase BY must not itself be the thing needing
/// erasure. Use an opaque document id, not an email address.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceId(String);

impl SourceId {
    /// Reserved prefix for engine-derived rows (design §4 item 6).
    pub const ENGINE_PREFIX: &'static str = "_engine:";

    /// Reserved provenance stamped by schema migration step 21 onto pre-0.8.20
    /// UNGOVERNED rows that were stored with NULL provenance (R-20-E8).
    pub const LEGACY_PRE_0_8_20: &'static str = "_legacy:pre-0.8.20";

    /// The single public constructor. Rejects the two ways a caller could
    /// express "effectively no provenance":
    ///
    /// * an empty or whitespace-only id — it names no source, and
    ///   `excise_source` already refuses the empty string, so such a row would
    ///   be un-erasable in practice;
    /// * an id in the engine's reserved `_`-prefixed namespace — a caller who
    ///   could mint `_legacy:pre-0.8.20` could hide rows among the migration's
    ///   back-filled ones, or mint `_engine:` rows that read as engine
    ///   substrate.
    ///
    /// # Errors
    ///
    /// [`EngineError::WriteValidation`] for either rejection above.
    pub fn new(id: impl Into<String>) -> Result<Self, EngineError> {
        let id = id.into();
        if id.trim().is_empty() || id.starts_with('_') {
            return Err(EngineError::WriteValidation);
        }
        Ok(Self(id))
    }

    /// Mint a reserved `_engine:*` provenance for an engine-derived row. Crate
    /// -internal by construction: the reserved namespace is exactly what
    /// [`SourceId::new`] refuses, so a caller cannot reach this spelling.
    pub(crate) fn engine_derived(role: &str) -> Self {
        Self(format!("{}{role}", Self::ENGINE_PREFIX))
    }

    /// The on-disk `source_id` text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume into the owned on-disk text.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for SourceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Display for SourceId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for SourceId {
    type Error = EngineError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for SourceId {
    type Error = EngineError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
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
        /// REQ-026 / AC-028 / AC-042 recovery seam, made **structurally
        /// mandatory** in 0.8.20 (R-20-E3). Was `Option<String>`; a `None`
        /// landed NULL on disk and produced a row no `excise_source` call could
        /// reach. See [`SourceId`] for why the fix is a type change rather than
        /// a validation check.
        source_id: SourceId,
        /// G0 (Slice 15) — stable cross-re-ingestion identity. `Some(id)`
        /// makes this write a transaction-time supersession of the prior
        /// active version of `(logical_id, kind)` (tombstone-then-insert).
        /// `None` is the legacy/own-identity default: a plain insert with a
        /// NULL `logical_id` (NULL-safe — never collides with other NULLs).
        logical_id: Option<String>,
        /// OPP-12 Phase-1 (0.8.19 Slice 5) — the create-time existence state.
        /// `InitialState::Active` (the [`Default`]) is the back-compat default and
        /// lands `state = 'active'` on disk (value-identical to the migration
        /// step-20 column DEFAULT). `InitialState::Pending` creates a quarantined
        /// node excluded from default retrieval. A `deleted`/`purged` node is
        /// UNREPRESENTABLE at create time (the [`InitialState`] type is the typed
        /// rejection) — those states are reachable only via the Slice-10
        /// `transition`/`purge` verbs.
        state: InitialState,
        /// OPP-12 Phase-1 (0.8.19 Slice 5) — advisory cause for the create-time
        /// `state` (e.g. the quarantine cause for a `pending` node), stored
        /// verbatim in `canonical_nodes.reason`. Engine never interprets it. `None`
        /// lands NULL (the back-compat default).
        reason: Option<String>,
        /// 0.8.20 Slice 15b (TC-34) — world-time validity window, INCLUSIVE lower
        /// bound, INTEGER epoch SECONDS UTC. `None` lands NULL = unbounded below.
        ///
        /// Slice 10b added the `valid_from`/`valid_until` columns, the [`ReadView`]
        /// validity predicate and [`Engine::crossed_boundary_since`] but NO writer,
        /// so a window could only be authored with raw SQL. These two fields are
        /// that writer. They are deliberately FIELDS rather than a new verb,
        /// exactly as [`PreparedWrite::Edge`] already carries `t_valid`/`t_invalid`:
        /// the governed command surface is unchanged.
        ///
        /// The pair is validated together — see `valid_until`.
        valid_from: Option<i64>,
        /// 0.8.20 Slice 15b (TC-34) — world-time validity window, EXCLUSIVE upper
        /// bound, INTEGER epoch SECONDS UTC. `None` lands NULL = unbounded above.
        ///
        /// The window is half-open `[valid_from, valid_until)`, matching the read
        /// predicate in `ReadView::validity_sql` exactly. Because it is half-open,
        /// a pair with `valid_from >= valid_until` describes an EMPTY window that no
        /// instant can ever satisfy — so [`Engine::write`] refuses it with
        /// [`EngineError::InvalidArgument`] rather than storing a row that no
        /// default read could ever return. A ONE-SIDED window is never empty and is
        /// never refused, however extreme its single bound.
        valid_until: Option<i64>,
    },
    Edge {
        kind: String,
        from: String,
        to: String,
        /// REQ-026 / AC-028 / AC-042 recovery seam — see Node. Structurally
        /// mandatory since 0.8.20 (R-20-E3).
        source_id: SourceId,
        /// G0 (Slice 15) — see Node. Supersession semantics are identical on
        /// edges (keyed by `(logical_id, kind)`).
        logical_id: Option<String>,
        /// G11 (Slice 15) — the fact/relationship text. When `Some`, triggers
        /// FTS projection into `search_index_edges` and vector projection via
        /// the projection scheduler (kind `"edge_fact"`). Also triggers
        /// invalidate-not-accumulate on `(from_id, to_id, kind)`.
        body: Option<String>,
        /// G11 (Slice 15) — event valid-time. NULL = unknown / still valid.
        ///
        /// **TC-33 (HITL-RATIFIED 2026-07-21): INTEGER epoch seconds (UTC), not
        /// ISO-8601.** This is the GOVERNED SDK WRITE SURFACE, which carries the
        /// same representation as storage. ISO-8601 survives ONLY on the BYO-LLM
        /// extractor wire (`fathomdb.extract.v1`), where
        /// `normalize_extractor_timestamp` converts it with hard rejection.
        t_valid: Option<i64>,
        /// G11 (Slice 15) — event invalid-time. NULL = still valid.
        ///
        /// **TC-33: INTEGER epoch seconds (UTC)** — see `t_valid`. The
        /// NULL-means-still-valid semantic is load-bearing and unchanged, which
        /// is why the schema pins the type with a `typeof` CHECK rather than
        /// `NOT NULL`.
        t_invalid: Option<i64>,
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

/// EXP-S (0.8.14 Slice 5, D1) — structural-role tag for a canonical row.
///
/// A SEPARATE axis from the doc-type `kind` (email/article/paper/meeting/
/// note/todo/doc/edge_fact): `row_kind` describes *what structural role* a row
/// plays in the "one store, many indexes" substrate, not what document type it
/// carries. Stored in `canonical_nodes.row_kind` (schema migration step 16).
///
/// `Leaf` is the default (a normal record; every existing/normal write is a
/// leaf — back-compat preserving). `Coverage` = coverage/summary rows;
/// `Graph` = graph structural rows. Engine-internal in 0.8.14 — there is NO
/// public Py/TS SDK surface for `row_kind` this release (`Leaf` for all normal
/// writes; `Coverage`/`Graph` are set only by internal paths). Cross-binding
/// parity (X1) is a Slice-40 concern.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowKind {
    Leaf,
    Coverage,
    Graph,
}

impl RowKind {
    /// On-disk `canonical_nodes.row_kind` spelling. Must match the migration
    /// step-16 `DEFAULT 'leaf'` and the schema vocabulary (D1).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            RowKind::Leaf => "leaf",
            RowKind::Coverage => "coverage",
            RowKind::Graph => "graph",
        }
    }
}

/// OPP-12 record-lifecycle Phase-1 (0.8.19 Slice 5) — the existence axis.
///
/// One mutually-exclusive typed enum stored as TEXT in the `canonical_nodes.state`
/// column (schema migration step-20). Semantics (design §2 / plan §1):
///   `Pending` = present + versioned but NOT admitted to default retrieval
///               (quarantine / promotion gate);
///   `Active`  = admitted to default retrieval (the shipped-corpus default);
///   `Deleted` = soft-deleted, retained + recoverable, excluded from default
///               reads, stays indexed behind the flag;
///   `Purged`  = terminal, physically erased.
/// `Deleted`/`Purged` are reachable only through the Phase-2/Slice-10
/// `transition`/`purge` verbs — they can NEVER be a create-time state (see
/// [`InitialState`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleState {
    Pending,
    Active,
    Deleted,
    Purged,
}

impl LifecycleState {
    /// On-disk `canonical_nodes.state` spelling. Must match the migration step-20
    /// `DEFAULT 'active'` and the `state = 'active'` default-read exclusion.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            LifecycleState::Pending => "pending",
            LifecycleState::Active => "active",
            LifecycleState::Deleted => "deleted",
            LifecycleState::Purged => "purged",
        }
    }

    /// Parse the on-disk spelling back into the typed enum. `None` for any value
    /// outside the closed vocabulary (a corrupt/foreign `state`).
    #[must_use]
    pub fn from_str_opt(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(LifecycleState::Pending),
            "active" => Some(LifecycleState::Active),
            "deleted" => Some(LifecycleState::Deleted),
            "purged" => Some(LifecycleState::Purged),
            _ => None,
        }
    }

    /// OPP-12 Phase-1 (0.8.19 Slice 10) — the target states legally reachable from
    /// `self` via the `transition` VERB (design §2 legal-transition table). This
    /// is the verb-specific enumeration reported by `IllegalTransitionError.legal`:
    ///   `Pending` → `[Active, Deleted]`   (promote / reject)
    ///   `Active`  → `[Deleted]`           (soft-delete)
    ///   `Deleted` → `[Active]`            (undelete)
    ///   `Purged`  → `[]`                  (terminal; nothing is reachable)
    /// `Purged` is DELIBERATELY excluded even from `Deleted`: reaching `purged` is
    /// the `purge` verb's job (see [`Engine::purge`]), NOT a legal `transition`
    /// target, so reporting it here would mislead a caller into thinking
    /// `transition(deleted → purged)` is legal when it is not. Likewise `Pending`
    /// is create-time-only and is never a `transition` target. Derived directly
    /// from [`is_legal_transition_move`] so this can never drift from the table.
    #[must_use]
    pub fn legal_next_states(self) -> Vec<LifecycleState> {
        [
            LifecycleState::Pending,
            LifecycleState::Active,
            LifecycleState::Deleted,
            LifecycleState::Purged,
        ]
        .into_iter()
        .filter(|&to| is_legal_transition_move(self, to))
        .collect()
    }
}

/// OPP-12 Phase-1 (0.8.19 Slice 10) — whether `(from, to)` is one of the four
/// legal `transition`-verb moves (design §2 table): `pending→active` (promote),
/// `pending→deleted` (reject), `active→deleted` (soft-delete), `deleted→active`
/// (undelete). Every other pair — self-loops, any move to `Purged` (purge-only)
/// or `Pending` (create-only), or from `Purged` — is illegal via `transition`.
#[must_use]
fn is_legal_transition_move(from: LifecycleState, to: LifecycleState) -> bool {
    matches!(
        (from, to),
        (LifecycleState::Pending, LifecycleState::Active)
            | (LifecycleState::Pending, LifecycleState::Deleted)
            | (LifecycleState::Active, LifecycleState::Deleted)
            | (LifecycleState::Deleted, LifecycleState::Active)
    )
}

/// OPP-12 Phase-1 (0.8.19 Slice 5) — the CREATE-TIME subset of [`LifecycleState`].
///
/// A write can only bring a node into existence as `Pending` or `Active` (design
/// §2 / gap-6). You CANNOT create a `Deleted`/`Purged` node — those states are
/// reachable only via the `transition`/`purge` verbs (Slice 10). Making the
/// create-time surface a separate two-variant type is the TYPED rejection: a
/// `deleted`/`purged` create is simply unrepresentable in the Rust API (the SDK
/// bindings map an out-of-subset string to a typed write-validation error).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum InitialState {
    Pending,
    /// The back-compat default: every pre-lifecycle write lands `Active`, matching
    /// the migration step-20 `DEFAULT 'active'`.
    #[default]
    Active,
}

impl InitialState {
    /// On-disk `canonical_nodes.state` spelling for a create-time state.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            InitialState::Pending => "pending",
            InitialState::Active => "active",
        }
    }

    /// The full [`LifecycleState`] this create-time state corresponds to.
    #[must_use]
    pub fn to_lifecycle_state(self) -> LifecycleState {
        match self {
            InitialState::Pending => LifecycleState::Pending,
            InitialState::Active => LifecycleState::Active,
        }
    }

    /// Parse a caller-supplied create-time `state` string into the create-time
    /// subset. `Some(state)` for `"pending"`/`"active"`; `None` for `"deleted"`,
    /// `"purged"`, or any unknown value — the SDK bindings turn `None` into a
    /// typed write-validation rejection (you cannot CREATE a deleted/purged node).
    #[must_use]
    pub fn from_create_str(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(InitialState::Pending),
            "active" => Some(InitialState::Active),
            _ => None,
        }
    }
}

/// F5 (0.8.14 Slice 10) — per-field BM25F weights for the `search_index_v2`
/// multi-column FTS index. One weight per indexed field
/// (`kind`/`body`/`status`), applied as the field's contribution multiplier in
/// the BM25F weighted-term-frequency accumulation.
///
/// The default is uniform (`1.0` each) — the "unweighted" baseline the R-F5-1
/// acceptance test contrasts against. Boosting a field (e.g. `kind`) makes a
/// match in that field outrank a same-strength match in a lower-weighted field.
/// Engine-internal for 0.8.14: there is NO public Py/TS SDK surface for these
/// tunables this release (cross-binding parity is a Slice-40/X1 concern).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bm25fFieldWeights {
    pub kind: f64,
    pub body: f64,
    pub status: f64,
}

impl Default for Bm25fFieldWeights {
    fn default() -> Self {
        Self { kind: 1.0, body: 1.0, status: 1.0 }
    }
}

/// F5 (0.8.14 Slice 10) — the compiled BM25F query plan for the fielded lexical
/// arm (`ADR-0.8.1` §3.2 `BM25fQueryPlan`). Carries the tunable per-field
/// `weights` and the tunable length-normalization `b` (and the term-saturation
/// `k1`).
///
/// NOTE on `b`: SQLite FTS5's built-in `bm25()` auxiliary function pins its
/// internal `k1`/`b` and exposes ONLY per-column weights — it cannot express a
/// tunable `b`. So the score is computed in-engine (a textbook BM25F over the
/// FTS5-recalled candidates) rather than delegated to the built-in `bm25()`:
/// that is what makes `b` (and `k1`) genuinely tunable here, not a dead
/// parameter. The `search_index_v2` FTS5 index is still load-bearing — it does
/// the candidate recall (`MATCH`) that the scorer then ranks.
///
/// Defaults match Robertson/SQLite BM25 (`b = 0.75`, `k1 = 1.2`) with uniform
/// field weights. Engine-internal for 0.8.14 (no SDK surface).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bm25fQueryPlan {
    pub weights: Bm25fFieldWeights,
    pub b: f64,
    pub k1: f64,
}

impl Default for Bm25fQueryPlan {
    fn default() -> Self {
        Self { weights: Bm25fFieldWeights::default(), b: 0.75, k1: 1.2 }
    }
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
    /// 0.8.12 Slice 15 (OPP-2, ADR-0.8.12) — BYO-LLM consolidation provider
    /// error (protocol mismatch, spawn/handshake failure, task not advertised in
    /// `supported_tasks`, or a malformed/out-of-cluster verdict). Rides the SAME
    /// `provider_session` transport as `Extractor`; this is the task-specific leaf.
    Consolidator,
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
    /// 0.8.18 Slice 5 (#5 vector-equivalence probe KEYSTONE) — the open-time
    /// self-check re-embedded the 45 committed probes with the live backend and
    /// found a divergence beyond the frozen D4 floor (a Phase-1 mean-centered
    /// `embedding_bin` sign flip, OR a Phase-2 un-centered L2 distance over
    /// `VECTOR_EQUIVALENCE_L2_EPSILON`). `Engine::open` succeeded into a degraded
    /// state (`dense_disabled = true`); this query-time error is raised at the
    /// single choke point [`Engine::search_inner_with_stats`] BEFORE any embedding
    /// / vector SQL / graph seeding / CE rerank, refusing EVERY vector-dependent
    /// arm (`search`, `search_expand`, explain/rerank, graph-arm). The explicit
    /// text-only/FTS-only path ([`Engine::search_text_only`]) stays serviceable.
    /// Sibling of the open-time `EngineOpenError::EmbedderIdentityMismatch`; per
    /// ADR-0.8.18 codex R2 U1-1 the refusal surfaces as an `EngineError` (queries
    /// never surface `EngineOpenError`). `reason` carries a human-readable summary.
    VectorEquivalenceMismatch {
        reason: String,
    },
    /// OPP-12 Phase-1 (0.8.19 Slice 10) — a lifecycle `transition`/`purge` move
    /// that the engine-enforced legal-transition table (design §2) forbids.
    /// Raised for an illegal `transition` target (`purged`/`pending` are never
    /// `transition` targets; self-loops; a from→to pair not in the table) AND for
    /// a `purge` precondition failure (purge is legal only from `deleted`).
    /// `from_state`/`to_state` use the FULL, parity-safe field names (S7 — `from`
    /// is a Python reserved word); `legal` enumerates the target states reachable
    /// from `from_state` in the full state machine.
    IllegalTransition {
        from_state: LifecycleState,
        to_state: LifecycleState,
        legal: Vec<LifecycleState>,
    },
    /// OPP-12 Phase-1 (0.8.19 Slice 10) — a lifecycle verb (`transition`/`purge`)
    /// was addressed with a non-`Logical` id space (a `Content`/`h:` doc-seeded or
    /// `Passage`/`p:` synthetic id). Only the `Logical` (`l:`) space is
    /// lifecycle-addressable (design §3); this is a typed refusal, never a panic
    /// or a silent no-op. `id_space` carries the offending [`IdSpaceKind`].
    NotLifecycleAddressable {
        id_space: IdSpaceKind,
    },
    /// 0.8.20 Slice 5b (R-20-E5, design `0.8.20-slice0-erasure-design.md` §4
    /// item 4) — an erasure verb (`purge` / `excise_source` /
    /// `excise_collection_record`) deleted its rows but could NOT complete the
    /// erasure **at rest**, so it refuses to report success.
    ///
    /// The motivating case is the write-ahead log. `PRAGMA secure_delete=ON`
    /// zeroes pages freed inside the database file, but the erased content also
    /// sits in the WAL as committed frames from the ORIGINAL insert: an erasure
    /// DELETE appends new frames, it never rewrites old ones. Only a
    /// `wal_checkpoint(TRUNCATE)` removes them, and a concurrent reader pinning a
    /// WAL snapshot makes that checkpoint return `busy`. After a bounded retry
    /// the verb raises THIS error rather than returning `Ok` over erased bytes
    /// that are still `grep`-able on disk.
    ///
    /// **Contract: an erasure verb must never report success on an incomplete
    /// erasure.** The row deletions are committed and durable when this is
    /// raised; what failed is the at-rest scrub. The remedy is to retry the verb
    /// (or `recover --truncate-wal`) once the blocking reader has finished.
    /// `stage` names the uncompleted step (e.g. `"wal_checkpoint"`,
    /// `"telemetry_redaction"`); `detail` is a human-readable summary.
    ErasureIncomplete {
        stage: String,
        detail: String,
    },
    /// 0.8.20 Slice 15d (R-20-PR) — `configure_projections` refused an
    /// incompatible/DESTRUCTIVE change to an existing projection `name` that was
    /// NOT accompanied by an explicit `drop`. Omission from the spec never drops
    /// (C3, `api-surface.md:27`); a role REMOVAL or a tokenizer/embedder change
    /// on a live projection would silently discard an expensive-to-rebuild
    /// resource, so it is refused with the destructive `delta` surfaced. The
    /// caller re-issues with `drop: [name]` to consciously rebuild.
    ProjectionDestructive {
        name: String,
        delta: String,
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
            Self::Consolidator => write!(f, "consolidator error"),
            Self::InvalidFilter { reason } => write!(f, "invalid filter: {reason}"),
            Self::InvalidArgument { msg } => write!(f, "invalid argument: {msg}"),
            Self::VectorEquivalenceMismatch { reason } => {
                write!(f, "vector-equivalence self-check failed; dense retrieval refused: {reason}")
            }
            Self::IllegalTransition { from_state, to_state, legal } => {
                let legal_list = legal.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
                write!(
                    f,
                    "illegal lifecycle transition {} -> {}; legal targets from {}: [{}]",
                    from_state.as_str(),
                    to_state.as_str(),
                    from_state.as_str(),
                    legal_list,
                )
            }
            Self::NotLifecycleAddressable { id_space } => write!(
                f,
                "id space {:?} ({}) is not lifecycle-addressable; only the logical (l:) space is",
                id_space,
                id_space.prefix(),
            ),
            Self::ErasureIncomplete { stage, detail } => write!(
                f,
                "erasure incomplete at stage '{stage}': the rows were deleted but the erasure \
                 could not be completed at rest ({detail})",
            ),
            Self::ProjectionDestructive { name, delta } => write!(
                f,
                "configure_projections refused a destructive change to projection '{name}' \
                 without an explicit drop ({delta}); re-issue with drop: [\"{name}\"] to rebuild",
            ),
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
            Self::Consolidator => "ConsolidatorError",
            Self::InvalidFilter { .. } => "InvalidFilterError",
            Self::InvalidArgument { .. } => "InvalidArgumentError",
            Self::VectorEquivalenceMismatch { .. } => "VectorEquivalenceMismatchError",
            Self::IllegalTransition { .. } => "IllegalTransitionError",
            Self::NotLifecycleAddressable { .. } => "NotLifecycleAddressableError",
            Self::ErasureIncomplete { .. } => "ErasureIncompleteError",
            Self::ProjectionDestructive { .. } => "ProjectionDestructiveError",
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

/// 0.8.20 Slice 15d (R-20-PR, C-1) — one member of a [`ProjectionSpec`]'s role
/// set. **Exactly three members** (HITL-ratified S8, `api-surface.md:87`):
/// `searchable→FTS` and `searchable→vector` are NOT roles — they are tier labels
/// carried by the `fts`/`vector` sub-objects of the spec, so an attribute is
/// `Searchable` once and the sub-objects select FTS-only / vector-only / both.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ProjectionRole {
    /// Projects into the EAV store + its `(attr_name, attr_value)` composite
    /// index — cheap equality/range, built same-transaction.
    Filterable,
    /// The F9 importance/recency signal. **Graceful-absent (Q6a):** declaring
    /// it is legal and never errors, but the engine DEFERS the build until F9
    /// exists and grafts it on the next idempotent `configure_projections`.
    Rankable,
    /// Full-text / dense recall of the meaning text. The `fts`/`vector`
    /// sub-objects select the sub-target.
    Searchable,
}

impl ProjectionRole {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ProjectionRole::Filterable => "filterable",
            ProjectionRole::Rankable => "rankable",
            ProjectionRole::Searchable => "searchable",
        }
    }

    #[must_use]
    pub fn from_str_opt(value: &str) -> Option<Self> {
        match value {
            "filterable" => Some(ProjectionRole::Filterable),
            "rankable" => Some(ProjectionRole::Rankable),
            "searchable" => Some(ProjectionRole::Searchable),
            _ => None,
        }
    }
}

/// 0.8.20 Slice 15d (R-20-PR) — the `searchable→FTS` sub-target selector.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectionFts {
    /// Optional tokenizer override; `None` ⇒ the engine default FTS5 tokenizer
    /// (`body`-FTS's `porter unicode61 remove_diacritics 2`). A custom
    /// per-attr tokenizer is the ≥0.9.x multi-field FTS work — recorded but
    /// not honoured here (graceful-graft later, same as `rankable`).
    pub tokenizer: Option<String>,
}

/// 0.8.20 Slice 15d (R-20-PR) — the `searchable→vector` sub-target selector.
///
/// **Slice 20 (R-20-DR) attaches `dense_readiness` HERE, additively:** this
/// sub-object is STORED by 15d (so the shape exists and a caller can declare a
/// vector projection) but 15d builds NO embedding / readiness machinery. Slice
/// 20 adds a `dense_readiness` field to this struct and the async flip logic;
/// nothing in 15d's persisted shape has to change for that (the registry column
/// `vector_embedder` + `vector_declared` already round-trip the sub-object).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectionVector {
    /// Optional embedder override; `None` ⇒ the engine's shipped default.
    pub embedder: Option<String>,
}

/// 0.8.20 Slice 15d (R-20-PR / C-1) — a single declarative projection
/// declaration. HITL-ratified shape (`api-surface.md:85-89`):
/// `{ name, roles: Set<ProjectionRole>, fts?, vector? }`. `roles` carries SET
/// semantics (dedup + membership; an attribute can be `Filterable` AND
/// `Searchable`) — encoded here as a sorted, de-duplicated `BTreeSet`. Named
/// `roles`, not `kind` (`kind` is the node/edge type discriminator).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionSpec {
    pub name: String,
    pub roles: BTreeSet<ProjectionRole>,
    pub fts: Option<ProjectionFts>,
    pub vector: Option<ProjectionVector>,
}

/// 0.8.20 Slice 15d (R-20-PR) — the diff [`Engine::configure_projections`]
/// applied. Idempotent re-registration yields `unchanged == true` with all
/// vecs empty (the "re-registration is a no-op" acceptance signal). A
/// destructive change without an explicit `drop` is an `Err`, not a delta.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectionDelta {
    /// Attribute names whose same-transaction projections (EAV / property-FTS)
    /// were (re)built by this apply.
    pub built: Vec<String>,
    /// Attribute names dropped (explicit `drop` list) — their EAV + property-FTS
    /// rows and registry row removed.
    pub dropped: Vec<String>,
    /// Attribute names whose declared roles were persisted but NOT built:
    /// `rankable` (F9 not yet live) and the `searchable→vector` sub-target
    /// (Slice 20). These graft on a future idempotent apply. No error.
    pub deferred: Vec<String>,
    /// True iff nothing was built, dropped, or newly deferred — the whole apply
    /// diffed to a no-op.
    pub unchanged: bool,
}

/// 0.8.20 Slice 5b (R-20-E7) — outcome of
/// [`Engine::excise_collection_record`]. `records_excised` counts the erased
/// `operational_mutations` versions (an append-only-log collection keeps every
/// version of a key); `state_rows_excised` counts the erased
/// `operational_state` row (0 or 1).
///
/// `record_digest` is `SHA-256(collection + 0x1F + record_key)` — the audit
/// handle. The raw `record_key` is deliberately NOT carried: it is arbitrary
/// caller-supplied text and may itself be the identifier being erased, so
/// echoing it into a durable audit row would defeat the erasure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExciseRecordReport {
    pub collection: String,
    pub record_digest: String,
    pub records_excised: u64,
    pub state_rows_excised: u64,
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

/// 0.8.20 Slice 5d (R-20-E8) — one `source_id` bucket in an
/// [`OrphanProvenanceReport`]. `source_id` is `None` for the NULL-provenance
/// bucket, which after migration step 21 should contain ONLY governed NODES.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrphanProvenanceSource {
    /// `None` = the NULL-`source_id` bucket.
    pub source_id: Option<String>,
    /// Canonical rows (nodes + edges) carrying this provenance.
    pub rows: u64,
    /// How many of `rows` carry a `logical_id`.
    ///
    /// NOT the same thing as "purge-addressable": only a NODE's `logical_id`
    /// confers purge-addressability. An EDGE's `logical_id` is a supersession
    /// identity and reaches no erasure verb (see
    /// [`Engine::orphan_provenance`]), so governed edges are counted here but
    /// are NOT subtracted from
    /// [`OrphanProvenanceReport::unerasable_rows`].
    pub governed_rows: u64,
    /// True for the engine's reserved `_`-prefixed namespace (`_engine:*`,
    /// `_legacy:pre-0.8.20`). Reserved buckets are reachable only through the
    /// operator seam `excise_source`, never through the governed
    /// [`Engine::erase_source`].
    pub reserved: bool,
}

/// Result of [`Engine::orphan_provenance`] — the per-`source_id` census behind
/// `fathomdb doctor orphan-provenance` (design §4 item 11).
///
/// `unerasable_rows` is the load-bearing field: canonical rows carrying
/// NEITHER a `source_id` NOR a `logical_id`. Such a row is reachable by no
/// erasure verb at all — `purge` keys on `logical_id`, `erase_source` keys on
/// `source_id` — so it can never be deleted on request. Slice 5c made that
/// state unwritable and migration step 21 back-filled the historical cases, so
/// a non-zero count means the invariant has been violated and the verb exits
/// `DOCTOR_FOUND_ISSUES`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrphanProvenanceReport {
    /// Per-`source_id` buckets, ordered by descending `rows` then `source_id`
    /// so the output is deterministic (a diagnostic that reorders between runs
    /// cannot be diffed).
    pub sources: Vec<OrphanProvenanceSource>,
    /// Total canonical rows surveyed.
    pub total_rows: u64,
    /// Rows with NO `source_id` AND NO `logical_id` — un-erasable by any verb.
    pub unerasable_rows: u64,
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

                // 0.8.18 Slice 5 (#5 vector-equivalence probe KEYSTONE) — run the
                // open-time self-check on the FINAL post-recovery connection (the
                // mean is already pinned/recovered inside open_locked, U1-b). First
                // registration persists the 45 UN-centered f32 references; a
                // subsequent open re-embeds + asserts P1 (mean-centered flip count,
                // floor 0) and P2 (un-centered L2 ε). Divergence ⇒ degraded-open
                // (`dense_disabled=true`), surfaced on the OpenReport (R-VEQ-6); the
                // query-time refusal fires later at `search_inner_with_stats`.
                let veq = run_vector_equivalence_probe(
                    &connection,
                    runtime_embedder.as_deref(),
                    &embedder_identity,
                    report.embedder_mean_vec_pinned,
                );
                report.dense_disabled = veq.dense_disabled;
                report.dense_disabled_reason = veq.reason.clone();

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
                        telemetry: Mutex::new(None),
                        telemetry_enabled: AtomicBool::new(false),
                        dense_disabled: AtomicBool::new(veq.dense_disabled),
                        dense_disabled_reason: Mutex::new(veq.reason),
                        vector_equivalence_refusals: AtomicU64::new(0),
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
        // OPP-12 Phase-1 (0.8.19 Slice 10, design §3 gap-4) — standing
        // `secure_delete=ON` on the writer, applied at EVERY open (fresh + migrated).
        // It zeroes every page freed by a future DELETE, so the Slice-10 `purge`
        // hard-erase is complete WITHOUT a per-purge `VACUUM`. It is a connection
        // PRAGMA (not schema DDL), so it belongs here, not in the 19→20 migration.
        // RESIDUAL (documented, not forced): pages freed on a pre-20 DB BEFORE this
        // was enabled are not retroactively scrubbed; there is no migration-time
        // full `VACUUM` (O(db-size)). NOTE: this is a standing pragma set at EVERY
        // connection open (writer here, plus the reader-pool and
        // `open_runtime_connection`), NOT the writer alone — non-writer connections
        // also free pages (projection / vector-rewrite DELETEs), so a writer-only
        // `secure_delete` would leak freed content on disk. See the matching
        // reader/runtime open comment (~lines 3335-3336).
        connection
            .pragma_update(None, "secure_delete", "ON")
            .map_err(|err| map_open_sqlite_error(err, OpenStage::WalReplay))?;
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

        // 0.8.20 Slice 15d (R-20-PR, Q5) — boot re-derive the projection registry
        // (the engine `ProjectionSpec` is a derived cache). For every persisted
        // declaration, clear + backfill its EAV / property-FTS rows from the
        // canonical nodes so a crash window (registry row survives, projection
        // rows partial) self-heals idempotently. A no-op single empty-table read
        // on every DB that has not declared a projection. On the writer
        // connection, single-threaded, before readers spawn — like the tokenizer
        // reproject above.
        rederive_projections_on_boot(&connection).map_err(|_| EngineOpenError::Io {
            message: "could not re-derive projection registry on boot".to_string(),
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
            // 0.8.18 Slice 5 — set by the #5 self-check in `open_with_migrations`
            // (which has the runtime embedder in scope). `open_locked` returns the
            // non-degraded default; the probe runs after this returns.
            dense_disabled: false,
            dense_disabled_reason: None,
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
            // OPP-12 Phase-1 (0.8.19 Slice 10, design §3 gap-4) — `secure_delete=ON`
            // at EVERY connection open, not just the writer. `secure_delete` is a
            // per-connection pager flag, so a reader-pool connection that frees a
            // page (vector-rewrite / projection DELETEs run off non-writer
            // connections) would otherwise leave that freed content on disk,
            // defeating GDPR erasure. Set BEFORE `query_only=ON` so the ordering is
            // unambiguous (the flag is a pager setting, not a DB write).
            reader
                .pragma_update(None, "secure_delete", "ON")
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
        // 0.8.6 Slice 5 (ADR-0.8.6): the spawn + hello/ready handshake +
        // request_id framing + error mapping now live in the reusable
        // `provider_session` transport seam, parameterized by `ProviderTask`.
        // `ingest_with_extractor` is the thin extract caller: it opens a session
        // for `ProviderTask::Extract` then runs the extract-specific payload
        // build + DB writes. The session owns child reaping via Drop.
        let mut session = self.provider_session(ProviderTask::Extract, cmd)?;
        self.run_extract_session(&mut session, documents)
    }

    /// 0.8.6 Slice 5 (ADR-0.8.6) — open a provider session: spawn the caller
    /// subprocess, run the `hello`/`ready` handshake for `task`, and negotiate
    /// `supported_tasks`. The transport (NDJSON over stdio, the detached stdout
    /// drainer, the bounded-recv timeout, the `request_id` framing, and the
    /// catch-all `EngineError::Extractor` mapping) is identical across tasks;
    /// only the protocol string (`fathomdb.<task>.v1`) and the negotiated task
    /// name differ. For `ProviderTask::Extract` the wire is byte-identical to the
    /// pre-0.8.6 `fathomdb.extract.v1` path.
    fn provider_session(
        &self,
        task: ProviderTask,
        cmd: &[&str],
    ) -> Result<ProviderSession, EngineError> {
        let (program, args) = cmd.split_first().ok_or(EngineError::Extractor)?;
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|_| EngineError::Extractor)?;

        let child_stdin = match child.stdin.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(EngineError::Extractor);
            }
        };
        let child_stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(EngineError::Extractor);
            }
        };

        // fix-35 [P1/P2]: drain stdout on a dedicated thread so (a) every read can
        // be bounded with a timeout — a hung harness can no longer block ingest
        // forever — and (b) the child's stdout pipe is drained continuously,
        // preventing a large-request deadlock (parent blocked writing stdin while
        // the child blocks writing a full stdout pipe). The handle is detached:
        // joining could hang if a misbehaving child holds stdout open past its
        // stdin EOF, so the session's `Drop` (child.kill()) is what guarantees
        // thread exit.
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

        let mut session = ProviderSession {
            task,
            child,
            writer: std::io::BufWriter::new(child_stdin),
            line_rx,
            io_timeout,
            model: None,
            max_docs_per_request: 8,
        };
        // On any handshake/negotiation error the session is dropped here, which
        // reaps the child (Drop) — matching the prior outer kill/wait semantics.
        session.handshake()?;
        Ok(session)
    }

    /// 0.8.6 Slice 5 — extract-specific driver over a `ProviderSession`. The
    /// payload build (documents → entities/edges) and DB writes are byte-identical
    /// to the pre-0.8.6 inner loop; only the spawn/handshake/framing moved into
    /// the shared session.
    fn run_extract_session(
        &self,
        session: &mut ProviderSession,
        documents: &[ExtractDocument],
    ) -> Result<IngestWithExtractorReceipt, EngineError> {
        let extractor_model_id = session.model.clone();
        let max_docs = session.max_docs_per_request;

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

            // Send the framed extract request and receive its matching `result`.
            // The session adds protocol/type/request_id and validates the
            // type=="result" + matching request_id envelope (fix-24 [P2]).
            let result = session
                .request(&request_id, vec![("documents".to_string(), Value::Array(docs_json))])?;

            // R-20-E2 (0.8.20 Slice 5c, design §4 item 10) — every row this batch
            // produces takes its provenance from the CALLER's
            // `ExtractDocument.source_doc_id`, NEVER from the model's echo of that
            // field. The echo is attacker-/error-controlled: a harness that omits
            // it used to yield rows with NULL `source_id`, which no
            // `excise_source` call can reach — the model could make a row
            // permanently un-erasable simply by dropping a key.
            //
            // `resolve_provenance` therefore admits the echo only as a SELECTOR
            // among ids the caller already supplied in THIS batch, and never as a
            // value:
            //
            //   * single-document batch — attribution is unambiguous, so the
            //     caller's id is used and the echo is ignored outright;
            //   * multi-document batch — the echo must name one of the batch's
            //     caller-supplied ids (the caller's own copy of the string is
            //     then stored). An absent or unrecognised echo is a protocol
            //     violation and fails the ingest LOUDLY with
            //     `EngineError::Extractor`, because the alternative — guessing an
            //     attribution — would silently mis-file the row under a document
            //     whose erasure would then not remove it.
            let batch_provenance = batch
                .iter()
                .map(|d| SourceId::new(d.source_doc_id.clone()))
                .collect::<Result<Vec<_>, _>>()?;
            let resolve_provenance = |echo: Option<&str>| -> Result<SourceId, EngineError> {
                if let [only] = batch_provenance.as_slice() {
                    return Ok(only.clone());
                }
                let echo = echo.ok_or(EngineError::Extractor)?;
                batch_provenance
                    .iter()
                    .find(|caller_id| caller_id.as_str() == echo)
                    .cloned()
                    .ok_or(EngineError::Extractor)
            };

            // --- map entities → PreparedWrite::Node with stable logical_id ---
            let entities =
                result.get("entities").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let raw_edges =
                result.get("edges").and_then(|v| v.as_array()).cloned().unwrap_or_default();

            // R3 (SCHEMA-GATE-1): collect substituted_t_valid values from
            // temporal_fallback warnings. An edge whose t_valid matches one of
            // these values had its event time defaulted to created_at (not
            // text-grounded) and must be flagged so BFS can exclude it.
            //
            // TC-33: kept as RAW `Value`s here and normalised below, together
            // with the edge side, through the SAME function. See the
            // normalisation block for why that is load-bearing.
            let raw_fallback_dates: Vec<&Value> = result
                .get("warnings")
                .and_then(|v| v.as_array())
                .map(|ws| {
                    ws.iter()
                        .filter(|w| {
                            w.get("kind").and_then(|k| k.as_str()) == Some("temporal_fallback")
                        })
                        .filter_map(|w| w.get("substituted_t_valid"))
                        .collect()
                })
                .unwrap_or_default();

            if !entities.is_empty() {
                let node_batch: Vec<PreparedWrite> = entities
                    .iter()
                    .map(|entity| -> Result<PreparedWrite, EngineError> {
                        let name = entity.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let kind = entity.get("type").and_then(|v| v.as_str()).unwrap_or("entity");
                        // R-20-E2: caller-grounded, echo used only as a selector.
                        let source_doc_id = resolve_provenance(
                            entity.get("source_doc_id").and_then(|v| v.as_str()),
                        )?;
                        // fix-34 [P1]: derive_logical_id now rejects an empty name
                        // or a ':' in kind — inputs that would collide distinct
                        // entities onto one identity and silently drop one.
                        let logical_id = derive_logical_id(kind, name)?;
                        Ok(PreparedWrite::Node {
                            kind: kind.to_string(),
                            body: name.to_string(),
                            source_id: source_doc_id,
                            logical_id: Some(logical_id),
                            state: InitialState::Active,
                            reason: None,
                            valid_from: None,
                            valid_until: None,
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
                    // Internal existence probe: STRICT view — this must see
                    // exactly the rows the pre-slice code saw.
                    .read_get_many(&ids, &ReadView::default())?
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

                // TC-33 — normalise EVERY extractor timestamp here, in ONE pass,
                // under ONE connection lock, BEFORE any edge is built. Both the
                // edge side (`t_valid`/`t_invalid`) and the temporal_fallback
                // warning side (`substituted_t_valid`) go through the SAME
                // function, and any value that cannot be normalised HARD-REJECTS
                // the whole ingest.
                //
                // **Normalising both sides is load-bearing, and nothing would
                // have caught it.** `temporal_fallback` is decided by comparing
                // the edge's t_valid against the warnings' substituted_t_valid.
                // That was a RAW BYTE-FOR-BYTE STRING MATCH with
                // `.unwrap_or(false)` on the miss path, and `substituted_t_valid`
                // is a FREE-FORM JSON key on the ELPS warnings envelope, not a
                // Rust struct field. So normalising only the edge side would
                // leave the set never matching, `.unwrap_or(false)` firing, and
                // EVERY fallback edge silently becoming a TRUSTED edge — with no
                // compile error anywhere. That flag is the only thing excluding
                // untrustworthy-time edges from graph BFS and graph seeding.
                //
                // Normalising both sides also FIXES a pre-existing brittleness:
                // `2025-03-20T09:30:00Z` and `2025-03-20T09:30:00+00:00` are the
                // same instant but MISS each other under a byte comparison. They
                // now compare equal as epochs.
                //
                // A malformed `substituted_t_valid` rejects rather than being
                // skipped: skipping it would leave the edge unflagged, i.e.
                // treated as TRUSTED — the same fail-open in a different place.
                //
                // The lock is taken and released HERE; `self.write(...)` below
                // re-acquires it, so no lock is held across the write.
                // (t_valid, t_invalid) epoch pair per edge, in `raw_edges` order.
                type EdgeTimes = Vec<(Option<i64>, Option<i64>)>;
                let (edge_times, fallback_epochs): (EdgeTimes, std::collections::HashSet<i64>) = {
                    let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
                    let connection = connection.as_ref().ok_or(EngineError::Closing)?;

                    let mut times = Vec::with_capacity(raw_edges.len());
                    for edge in &raw_edges {
                        times.push((
                            normalize_extractor_timestamp(
                                connection,
                                "t_valid",
                                edge.get("t_valid"),
                            )?,
                            normalize_extractor_timestamp(
                                connection,
                                "t_invalid",
                                edge.get("t_invalid"),
                            )?,
                        ));
                    }

                    let mut epochs = std::collections::HashSet::new();
                    for raw in &raw_fallback_dates {
                        if let Some(epoch) = normalize_extractor_timestamp(
                            connection,
                            "substituted_t_valid",
                            Some(raw),
                        )? {
                            epochs.insert(epoch);
                        }
                    }
                    (times, epochs)
                };

                let edge_batch: Vec<PreparedWrite> = raw_edges
                    .iter()
                    .zip(&edge_times)
                    .map(|(edge, &(t_valid, t_invalid))| -> Result<PreparedWrite, EngineError> {
                        let from_entity =
                            edge.get("from_entity").and_then(|v| v.as_str()).unwrap_or("");
                        let to_entity =
                            edge.get("to_entity").and_then(|v| v.as_str()).unwrap_or("");
                        let relation =
                            edge.get("relation").and_then(|v| v.as_str()).unwrap_or("related_to");
                        let body = edge.get("body").and_then(|v| v.as_str()).map(str::to_string);
                        // TC-33: `t_valid`/`t_invalid` were normalised (and any
                        // malformed or non-string value hard-rejected) in the
                        // pass above; they arrive here as epoch seconds.
                        // fix-26 [P2]: validate confidence is in [0.0, 1.0] at the
                        // protocol boundary; reject out-of-range values.
                        let confidence = match edge.get("confidence").and_then(|v| v.as_f64()) {
                            Some(c) if !(0.0..=1.0).contains(&c) => {
                                return Err(EngineError::Extractor);
                            }
                            c => c,
                        };
                        // R-20-E2: caller-grounded, echo used only as a selector.
                        let source_doc_id =
                            resolve_provenance(edge.get("source_doc_id").and_then(|v| v.as_str()))?;

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

                        // TC-33: BOTH sides are now epochs from the SAME
                        // normalisation, so this compares instants rather than
                        // byte strings.
                        let is_temporal_fallback =
                            t_valid.is_some_and(|tv| fallback_epochs.contains(&tv));
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

        // The `ProviderSession` (and its writer/child) is dropped by the caller
        // when `ingest_with_extractor` returns: Drop sends stdin EOF and reaps
        // the child, matching the prior explicit drop(writer)+kill/wait.
        Ok(IngestWithExtractorReceipt { nodes_written, edges_written, docs_processed })
    }

    /// 0.8.12 Slice 15 (OPP-2, ADR-0.8.12) — BYO-LLM CONSOLIDATION / RECENCY.
    ///
    /// The SECOND consumer of the one `provider_session` transport (ADR-0.8.6):
    /// consolidation reuses the exact NDJSON-over-stdio transport, hello/ready
    /// handshake, `supported_tasks` negotiation, `request_id` framing, and
    /// bounded-recv timeout — only the protocol string
    /// (`fathomdb.consolidate.v1`) and the task-specific payload differ. There is
    /// NO second transport and NO second handshake.
    ///
    /// For each `(subject, relation)` axis, FathomDB assembles a candidate
    /// cluster of competing active fact-edges DETERMINISTICALLY (CPU-only, no
    /// LLM), sends it to the caller-supplied harness, and applies the returned
    /// verdicts. **CALLER-SIDE BYO-LLM**: the harness is the caller's subprocess;
    /// the library never embeds or calls an LLM and makes NO network egress.
    ///
    /// **Load-bearing semantic (ADR-0.8.12 §2.1):** consolidation records
    /// supersession / recency METADATA only — `invalidate` sets `t_invalid`,
    /// `supersede`/`merge` marks the row superseded via the existing G0 tombstone
    /// column. Edge BODIES are NEVER rewritten and NO row is ever deleted (the
    /// 0.8.3 lesson: blind content-merge HURT accuracy). The original rows
    /// survive; the engine stays deterministic.
    ///
    /// Returns [`EngineError::Consolidator`] on any transport/handshake/protocol
    /// fault or a malformed / out-of-cluster verdict.
    pub fn consolidate_with_provider(
        &self,
        cmd: &[&str],
        axes: &[ConsolidateAxis],
    ) -> Result<ConsolidateReceipt, EngineError> {
        // Reuse the shared transport verbatim; remap its (Extractor-flavoured)
        // transport error to the task-specific Consolidator leaf.
        let mut session = self
            .provider_session(ProviderTask::Consolidate, cmd)
            .map_err(|_| EngineError::Consolidator)?;
        self.run_consolidate_session(&mut session, axes)
    }

    /// 0.8.12 Slice 15 — consolidate-specific driver over a `ProviderSession`.
    /// Mirrors [`run_extract_session`][Engine::run_extract_session]: assemble the
    /// task payload, run the framed request over the shared session, apply the
    /// task-specific DB effect. The cluster assembly + verdict application are
    /// CPU-only/deterministic.
    fn run_consolidate_session(
        &self,
        session: &mut ProviderSession,
        axes: &[ConsolidateAxis],
    ) -> Result<ConsolidateReceipt, EngineError> {
        let mut receipt = ConsolidateReceipt::default();

        for (i, axis) in axes.iter().enumerate() {
            // 1. Deterministically assemble the candidate cluster (CPU-only, no LLM).
            let cluster = self.assemble_consolidate_cluster(axis)?;
            if cluster.is_empty() {
                continue;
            }
            receipt.clusters_processed = receipt.clusters_processed.saturating_add(1);
            receipt.edges_examined = receipt.edges_examined.saturating_add(cluster.len() as u64);

            // 2. Send the cluster; receive the verdict envelope. The session adds
            //    protocol/type/request_id and validates type=="result" + matching
            //    request_id. Any transport/protocol fault → Consolidator.
            let request_id = format!("req-{i}");
            // TC-33: storage and `ConsolidateCandidateEdge` are INTEGER epoch
            // seconds, but the harness WIRE is ISO-8601 — the same split as the
            // extractor boundary. Render on the way out; the verdict's
            // `t_invalid` is normalised back on the way in. Without this the
            // harness would receive epoch integers and (since the reference stub
            // echoes the winner's `t_valid` straight back as `t_invalid`) its
            // reply would be rejected by our own inbound normaliser.
            let edges_json: Vec<Value> = {
                let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
                let connection = connection.as_ref().ok_or(EngineError::Closing)?;
                // TC-33 fix-1 backstop [DEFENSIVE — unreachable]. A stored
                // `Some(ts)` that fails to render must NOT become a silent
                // `null`: that is exactly the "still valid" resurrection vector.
                // With `reject_unrenderable_edge_epoch` guarding the write
                // boundary, no unrenderable epoch can reach storage — so this is
                // a hard-assert upholding that invariant STRUCTURALLY, not the
                // primary defence. `None` (unknown) still renders to JSON null;
                // only a NON-NULL stored epoch that fails to render is an error.
                let render = |field: &str, value: Option<i64>| -> Result<Value, EngineError> {
                    match value {
                        None => Ok(Value::Null),
                        Some(ts) => match epoch_seconds_to_iso8601(connection, ts) {
                            Some(iso) => Ok(Value::from(iso)),
                            None => Err(EngineError::InvalidArgument {
                                msg: format!(
                                    "INVARIANT VIOLATION (TC-33 fix-1): stored edge `{field}` = \
                                     {ts} is unrenderable to ISO-8601 and would have gone to the \
                                     consolidation wire as a silent null (\"still valid\"). The \
                                     write boundary should have made this unstorable."
                                ),
                            }),
                        },
                    }
                };
                cluster
                    .iter()
                    .map(|e| {
                        Ok::<Value, EngineError>(serde_json::json!({
                            "edge_ref": e.edge_ref,
                            "body": e.body,
                            "t_valid": render("t_valid", e.t_valid)?,
                            "t_invalid": render("t_invalid", e.t_invalid)?,
                            "confidence": e.confidence,
                            "source_doc_id": e.source_doc_id,
                            "extractor_model_id": e.extractor_model_id,
                        }))
                    })
                    .collect::<Result<Vec<Value>, EngineError>>()?
            };
            let cluster_json = serde_json::json!({
                "subject": axis.subject_logical_id,
                "relation": axis.relation,
                "edges": edges_json,
            });
            let result = session
                .request(&request_id, vec![("cluster".to_string(), cluster_json)])
                .map_err(|_| EngineError::Consolidator)?;

            // 3. Apply the verdicts (metadata-only; original rows + bodies survive).
            let verdicts = result
                .get("verdicts")
                .and_then(|v| v.as_array())
                .ok_or(EngineError::Consolidator)?
                .clone();
            self.apply_consolidate_verdicts(&cluster, &verdicts, &mut receipt)?;
        }

        Ok(receipt)
    }

    /// 0.8.12 Slice 15 — assemble the competing fact-edge cluster for one
    /// `(subject, relation)` axis, deterministically, from active `canonical_edges`
    /// (`from_id = subject AND kind = relation AND superseded_at IS NULL`), ordered
    /// by `write_cursor` (stable insertion order). CPU-only; no network, no LLM.
    fn assemble_consolidate_cluster(
        &self,
        axis: &ConsolidateAxis,
    ) -> Result<Vec<ConsolidateCandidateEdge>, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        let mut stmt = connection
            .prepare(
                "SELECT logical_id, body, t_valid, t_invalid, confidence, source_id, \
                        extractor_model_id \
                 FROM canonical_edges \
                 WHERE from_id = ?1 AND kind = ?2 AND superseded_at IS NULL \
                 ORDER BY write_cursor",
            )
            .map_err(|_| EngineError::Storage)?;
        let rows = stmt
            .query_map(params![axis.subject_logical_id, axis.relation], |r| {
                Ok(ConsolidateCandidateEdge {
                    edge_ref: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    body: r.get(1)?,
                    t_valid: r.get(2)?,
                    t_invalid: r.get(3)?,
                    confidence: r.get(4)?,
                    source_doc_id: r.get(5)?,
                    extractor_model_id: r.get(6)?,
                })
            })
            .map_err(|_| EngineError::Storage)?;
        let out: rusqlite::Result<Vec<ConsolidateCandidateEdge>> = rows.collect();
        // Skip any edge with a NULL/empty logical_id (no stable ref to round-trip).
        Ok(out
            .map_err(|_| EngineError::Storage)?
            .into_iter()
            .filter(|e| !e.edge_ref.is_empty())
            .collect())
    }

    /// 0.8.12 Slice 15 — apply the harness verdicts as METADATA-ONLY transitions
    /// (ADR-0.8.12 §2.1). NEVER rewrites a body, NEVER deletes a row. A verdict
    /// referencing an edge not in the presented cluster, or an unknown verdict
    /// kind, is a protocol fault → [`EngineError::Consolidator`].
    fn apply_consolidate_verdicts(
        &self,
        cluster: &[ConsolidateCandidateEdge],
        verdicts: &[Value],
        receipt: &mut ConsolidateReceipt,
    ) -> Result<(), EngineError> {
        let known: std::collections::HashSet<&str> =
            cluster.iter().map(|e| e.edge_ref.as_str()).collect();
        // fix-1 [P2] bijection: the verdict set must cover the presented cluster
        // EXACTLY — every presented edge ruled on, none ruled on twice.
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();

        let mut connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_mut().ok_or(EngineError::Closing)?;
        let tx = connection.transaction().map_err(|_| EngineError::Storage)?;

        for v in verdicts {
            let edge_ref =
                v.get("edge_ref").and_then(|x| x.as_str()).ok_or(EngineError::Consolidator)?;
            // The harness may only rule on edges FathomDB presented in the cluster.
            if !known.contains(edge_ref) {
                return Err(EngineError::Consolidator);
            }
            // fix-1 [P2]: a repeated edge_ref is a protocol fault (not a bijection).
            if !seen.insert(edge_ref) {
                return Err(EngineError::Consolidator);
            }
            let verdict =
                v.get("verdict").and_then(|x| x.as_str()).ok_or(EngineError::Consolidator)?;
            // Look up the active edge's projection cursor BEFORE any UPDATE so a
            // supersede (which clears `superseded_at IS NULL`) can still find it.
            let active_cursor = Self::active_edge_write_cursor(&tx, edge_ref)?;
            match verdict {
                "keep" => {
                    receipt.edges_kept = receipt.edges_kept.saturating_add(1);
                }
                "invalidate" => {
                    // Recency metadata: set t_invalid; the row and its body are
                    // left intact (this is NOT a destructive content rewrite).
                    //
                    // TC-33: the CONSOLIDATION harness is the same class of
                    // BYO-LLM boundary as the extractor, so it carries ISO-8601
                    // on the wire and is normalised here with the SAME hard
                    // rejection. Previously the raw string went straight into the
                    // UPDATE with no validation whatsoever.
                    // fix-3 [P2]: consolidation is a BYO-LLM PROVIDER boundary, so
                    // a malformed / non-string `t_invalid` is a PROVIDER protocol
                    // fault → `Consolidator`, NOT the extractor/user `InvalidArgument`
                    // that `normalize_extractor_timestamp` emits. Remap it to match
                    // the two sibling failure modes on this same value (missing key
                    // and null/unparseable-to-None, both `Consolidator`). Consistent,
                    // not a diagnostic loss: `Consolidator` is a unit variant and the
                    // adjacent `.ok_or(EngineError::Consolidator)` cases already
                    // discard any message.
                    let ts = normalize_extractor_timestamp(
                        &tx,
                        "t_invalid",
                        Some(v.get("t_invalid").ok_or(EngineError::Consolidator)?),
                    )
                    .map_err(|_| EngineError::Consolidator)?
                    .ok_or(EngineError::Consolidator)?;
                    tx.execute(
                        "UPDATE canonical_edges SET t_invalid = ?1 \
                         WHERE logical_id = ?2 AND superseded_at IS NULL",
                        params![ts, edge_ref],
                    )
                    .map_err(|_| EngineError::Storage)?;
                    // fix-1 [P1]: prune the STATIC projection shadow rows so the
                    // consolidated-away edge stops surfacing in FTS/vector — but
                    // ONLY when the edge is ended as of the engine's "now",
                    // mirroring the graph-traversal filter `edge_validity_sql`.
                    // A future-dated t_invalid keeps the edge valid ⇒ keep the
                    // projection. NON-DESTRUCTIVE: the canonical_edges row + body
                    // survive (ADR-0.8.12 §2.1).
                    //
                    // TC-33: this used to be `SELECT datetime(?1) <= datetime('now')`
                    // — an inline clock, AND a misleading error class: junk made
                    // the SELECT yield SQL NULL, so `r.get::<bool>` failed as
                    // `EngineError::Storage`. Both timestamps are integers now, so
                    // the comparison is plain Rust against the bound `:now` seam.
                    if let Some(cursor) = active_cursor {
                        let ended = ts <= current_epoch_seconds();
                        if ended {
                            // fix-2 [P2]: KEEP the projection terminal row. The
                            // canonical_edges row stays NON-superseded (invalidate
                            // is metadata-only), and `database_has_pending_projection_work`
                            // flags any non-superseded edge that has a body but no
                            // terminal as pending; since `next_pending_projection_jobs`
                            // only scans cursors ABOVE the stored projection cursor, an
                            // already-projected invalidated edge would never be requeued
                            // and `drain()`/`wait_for_idle` would hang forever. Dropping
                            // the FTS/vec shadows (below) hides it from active retrieval;
                            // retaining the terminal keeps the scheduler idle.
                            Self::prune_edge_projection_shadows(&tx, cursor, true)?;
                        }
                    }
                    receipt.edges_invalidated = receipt.edges_invalidated.saturating_add(1);
                }
                // `merge` maps cleanly to supersede + metadata (ADR-0.8.12 §3):
                // the loser is marked superseded; the winner ("by"/"into") is the
                // surviving active row. No body is merged.
                "supersede" | "merge" => {
                    // Mark superseded via the existing G0 tombstone column; the row
                    // survives (invalidate-not-delete). Use a fresh monotonic cursor.
                    let cursor = self.next_cursor.fetch_add(1, Ordering::SeqCst).saturating_add(1);
                    tx.execute(
                        "UPDATE canonical_edges SET superseded_at = ?1 \
                         WHERE logical_id = ?2 AND superseded_at IS NULL",
                        params![cursor, edge_ref],
                    )
                    .map_err(|_| EngineError::Storage)?;
                    // fix-1 [P1]: a superseded edge is unconditionally out of the
                    // active set (graph traversal filters `superseded_at IS NULL`),
                    // so prune its FTS/vector projection shadow rows to match.
                    if let Some(active_cursor) = active_cursor {
                        // A superseded row is excluded from the pending-work check
                        // (`superseded_at IS NOT NULL`), so dropping its terminal too
                        // is safe (matches the excise pattern) and cannot phantom-pend.
                        Self::prune_edge_projection_shadows(&tx, active_cursor, false)?;
                    }
                    receipt.edges_superseded = receipt.edges_superseded.saturating_add(1);
                }
                _ => return Err(EngineError::Consolidator),
            }
        }

        // fix-1 [P2]: bijection completeness — every presented cluster edge must
        // have received exactly one verdict.
        if seen.len() != known.len() {
            return Err(EngineError::Consolidator);
        }

        tx.commit().map_err(|_| EngineError::Storage)?;
        Ok(())
    }

    /// fix-1 [P1] — the active (non-superseded) row's projection `write_cursor`
    /// for a fact-edge `logical_id`, or `None` if there is no active row. The
    /// cursor keys the STATIC projection shadow rows (FTS `search_index_edges`,
    /// vec0 `vector_default` by rowid, `_fathomdb_vector_rows`,
    /// `_fathomdb_projection_terminal`).
    fn active_edge_write_cursor(
        tx: &rusqlite::Transaction<'_>,
        edge_ref: &str,
    ) -> Result<Option<i64>, EngineError> {
        tx.query_row(
            "SELECT write_cursor FROM canonical_edges \
             WHERE logical_id = ?1 AND superseded_at IS NULL",
            params![edge_ref],
            |r| r.get::<_, i64>(0),
        )
        .optional()
        .map_err(|_| EngineError::Storage)
    }

    /// fix-1 [P1] — prune the STATIC projection shadow rows for a canonical
    /// row's `write_cursor` so a consolidated-away edge stops surfacing in
    /// FTS/vector retrieval. Mirrors the excision pattern at
    /// `excise_source_inner` (invalidate-not-delete: the canonical row + body
    /// are NEVER touched here).
    ///
    /// `keep_terminal` retains the `_fathomdb_projection_terminal` marker — set it
    /// when the canonical row stays NON-superseded (an `invalidate` verdict), so the
    /// projection scheduler still treats the cursor as done (fix-2 [P2]); clear it
    /// when the row is superseded (excluded from the pending-work scan anyway).
    ///
    /// FIXED (0.8.12 Slice A, R-CON-2 named default-ON blocker; Slice-20 codex
    /// §9 [P2]): a full `rebuild_projections` re-projects every non-superseded
    /// edge with a body from `canonical_edges` — this used to re-materialise an
    /// invalidated edge's FTS/vec shadows even though graph traversal excludes
    /// it via the `t_invalid > now` filter. The FTS rebuild SELECT
    /// (`rebuild_shadow_state`), the vec projection queue
    /// (`next_pending_projection_jobs`), and the pending-work probe
    /// (`database_has_pending_projection_work`) now all carry the same
    /// `edge_validity_sql` filter as graph traversal (TC-33: INTEGER compare
    /// against the bound `:now`, formerly `datetime(t_invalid) > datetime('now')`),
    /// so a rebuild is durable across the recency exclusion.
    fn prune_edge_projection_shadows(
        tx: &rusqlite::Transaction<'_>,
        cursor: i64,
        keep_terminal: bool,
    ) -> Result<(), EngineError> {
        tx.execute("DELETE FROM search_index_edges WHERE write_cursor = ?1", [cursor])
            .map_err(|_| EngineError::Storage)?;
        // vec0 rowid is the canonical row's write_cursor.
        tx.execute("DELETE FROM vector_default WHERE rowid = ?1", [cursor])
            .map_err(|_| EngineError::Storage)?;
        tx.execute("DELETE FROM _fathomdb_vector_rows WHERE write_cursor = ?1", [cursor])
            .map_err(|_| EngineError::Storage)?;
        if !keep_terminal {
            tx.execute(
                "DELETE FROM _fathomdb_projection_terminal WHERE write_cursor = ?1",
                [cursor],
            )
            .map_err(|_| EngineError::Storage)?;
        }
        Ok(())
    }

    pub fn search(&self, query: &str) -> Result<SearchResult, EngineError> {
        self.search_filtered(query, None)
    }

    /// 0.8.20 Slice 15b fix-2 (R-20-NV / R-20-RV) — `search` under an explicit
    /// [`ReadView`], the escape hatch matching the one the five read verbs got in
    /// Slice 10b. `search(query)` is exactly `search_view(query, &ReadView::default())`.
    ///
    /// **Scope: the VALIDITY axis only.** `include_out_of_window` and
    /// `valid_as_of` are honoured; the EXISTENCE flags (`include_superseded`,
    /// `include_inactive`) are **refused** with
    /// [`EngineError::InvalidArgument`] rather than silently ignored. Relaxing
    /// `superseded_at IS NULL` on a retrieval path would resurrect the stale-body
    /// leak the Slice-15 fix-1 review closed, and search hydrates from projection
    /// indexes (`search_index`, `vector_default`) that are not version-complete —
    /// so "include superseded" has no truthful answer here. Refusing says that;
    /// ignoring would be the dead surface this fix exists to remove.
    ///
    /// Governed surface: PROPOSED / NOT SIGNED (0.8.20 Slice 15b fix-2).
    pub fn search_view(&self, query: &str, view: &ReadView) -> Result<SearchResult, EngineError> {
        self.search_reranked_with_explain(query, None, 0, false, 0.3, 0, false, *view)
    }

    /// 0.8.20 Slice 15b fix-2 (R-20-NV / R-20-RV) — the FULL-arity view entry
    /// point: [`search_reranked`][Engine::search_reranked] /
    /// [`search_explained`][Engine::search_explained] under an explicit
    /// [`ReadView`]. This is what the Python and TypeScript `search(..., view=)`
    /// bindings call, so a caller can combine a content filter, the CE knobs and
    /// a validity view in one query — passing `view` must not silently disable
    /// the filter, and passing a filter must not silently disable `view`.
    ///
    /// `search_reranked(q, f, d, g, a, p)` is exactly
    /// `search_reranked_view(q, f, d, g, a, p, false, &ReadView::default())`.
    ///
    /// Validity axis only; existence flags are refused. See
    /// [`search_view`][Engine::search_view].
    ///
    /// Governed surface: PROPOSED / NOT SIGNED (0.8.20 Slice 15b fix-2).
    #[allow(clippy::too_many_arguments)] // mirrors search_explained + the view
    pub fn search_reranked_view(
        &self,
        query: &str,
        filter: Option<SearchFilter>,
        rerank_depth: usize,
        use_graph_arm: bool,
        alpha: f64,
        pool_n: usize,
        explain: bool,
        view: &ReadView,
    ) -> Result<SearchResult, EngineError> {
        self.search_reranked_with_explain(
            query,
            filter,
            rerank_depth,
            use_graph_arm,
            alpha,
            pool_n,
            explain,
            *view,
        )
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
        // 0.8.11 Slice 40 (R-FIL-2): re-express the shipped G10 `SearchFilter`
        // sugar through the unified `Filter` type, then lower back to the vec0
        // backend's `SearchFilter` (D4). The round-trip is lossless +
        // canonical-order-preserving, so the produced phase-1 SQL stays
        // byte-identical to 0.7.2 on the `None`/all-`None` path. `SearchFilter`
        // never carries a `Json` term, so `to_search_filter` never rejects here.
        let lowered = filter.map(|sf| Filter::from(&sf).to_search_filter()).transpose()?;
        // FIX-6: delegate to search_reranked(depth=0, use_graph_arm=false) to eliminate the
        // ~26-line duplicate body that would otherwise drift with search_reranked.
        // 0.8.5: depth=0 is inert, so the α/pool_n defaults (0.3, 0) never reach the blend.
        self.search_reranked(query, lowered, 0, false, 0.3, 0)
    }

    /// 0.8.11 Slice 40 (#17) — unified-`Filter` entry point for the vec0 search
    /// backend. Lowers the metadata subset to the indexed pre-KNN `WHERE` and
    /// **typed-rejects** a [`FilterTerm::Json`] term with
    /// [`EngineError::InvalidFilter`] (D3 no-demotion guarantee). This is the
    /// unified surface the 0.8.15 router `constraints` block reasons over; the
    /// shipped [`Engine::search_filtered`]`(query, Option<SearchFilter>)` stays
    /// as sugar over the same path.
    pub fn search_filter(&self, query: &str, filter: &Filter) -> Result<SearchResult, EngineError> {
        let sf = filter.to_search_filter()?;
        self.search_reranked(query, Some(sf), 0, false, 0.3, 0)
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
        alpha: f64,
        pool_n: usize,
    ) -> Result<SearchResult, EngineError> {
        // explain=false → `SearchResult.explanation == None`, byte-identical results.
        self.search_reranked_with_explain(
            query,
            filter,
            rerank_depth,
            use_graph_arm,
            alpha,
            pool_n,
            false,
            ReadView::default(),
        )
    }

    /// 0.8.8 EXP-OBS (Slice 5) — `search_explained`: the opt-in `explain=true`
    /// surface. Identical retrieval to [`search_reranked`][Engine::search_reranked]
    /// (same fused/CE ranking, same `results`), additionally returning a
    /// [`Explanation`] sidecar on `SearchResult.explanation` with per-hit arm
    /// provenance + score breakdown + a query-level [`QueryTrace`]. The default
    /// `search`/`search_filtered`/`search_reranked` paths are unaffected and stay
    /// byte-identical (R-OBS-2).
    ///
    /// Governed surface: re-exported from `fathomdb` facade.
    pub fn search_explained(
        &self,
        query: &str,
        filter: Option<SearchFilter>,
        rerank_depth: usize,
        use_graph_arm: bool,
        alpha: f64,
        pool_n: usize,
    ) -> Result<SearchResult, EngineError> {
        self.search_reranked_with_explain(
            query,
            filter,
            rerank_depth,
            use_graph_arm,
            alpha,
            pool_n,
            true,
            ReadView::default(),
        )
    }

    /// 0.8.18 Slice 5 (#5 vector-equivalence probe, R-VEQ-4) — the explicit
    /// **text-only / FTS-only** search path. It does NOT embed the query and does
    /// NOT route through the vector-dependent choke point
    /// [`search_inner_with_stats`][Engine::search_inner_with_stats], so it NEVER
    /// raises [`EngineError::VectorEquivalenceMismatch`] and stays serviceable when
    /// the engine opened in the degraded `dense_disabled` state (the D2 "keep FTS
    /// servable" contract; codex R2 U1-2). Results come from the node-body FTS
    /// branch only — no vector recall, no CE rerank, no graph arm. Available
    /// regardless of degraded state; when dense is healthy it is simply a
    /// text-only view of the same corpus.
    ///
    /// Governed surface: re-exported from the `fathomdb` facade + Py/TS bindings.
    pub fn search_text_only(&self, query: &str) -> Result<SearchResult, EngineError> {
        self.search_text_only_view(query, &ReadView::default())
    }

    /// 0.8.20 Slice 15b fix-2 (R-20-NV / R-20-RV) — [`search_text_only`][Engine::search_text_only]
    /// under an explicit [`ReadView`]. Same validity-axis-only scope, and the same
    /// typed refusal of the existence flags, as [`search_view`][Engine::search_view].
    ///
    /// Governed surface: PROPOSED / NOT SIGNED (0.8.20 Slice 15b fix-2).
    pub fn search_text_only_view(
        &self,
        query: &str,
        view: &ReadView,
    ) -> Result<SearchResult, EngineError> {
        self.ensure_open()?;
        view.reject_existence_relaxation_on_search()?;
        if query.trim().is_empty() {
            return Err(EngineError::WriteValidation);
        }
        let compiled = compile_text_query(query);
        let search_limit = self
            .projection_runtime
            .shared
            .search_limit_override
            .load(Ordering::SeqCst)
            .max(SEARCH_RERANK_LIMIT);
        let (response_tx, response_rx) = mpsc::sync_channel::<ReaderResponse>(1);
        // `query_vector = None` ⇒ `read_search_in_tx` skips the vector branch
        // entirely (no embed, no phase-1 bit-KNN, no phase-2 L2) and returns the
        // text/FTS branch — exactly the un-embedded fallback the hybrid path already
        // takes on an embed miss.
        let request = ReaderRequest::Search {
            compiled,
            query_vector: None,
            query_vector_bin: None,
            search_limit,
            filter: None,
            recency_enabled: false,
            importance_enabled: false,
            vector_stage_only: false,
            raw_query: Box::from(query),
            rerank_depth: 0,
            use_graph_arm: false,
            alpha: 0.3,
            pool_n: 0,
            explain: false,
            view: *view,
            respond: response_tx,
        };
        if self.reader_pool.dispatch(request).is_err() {
            return Err(EngineError::Closing);
        }
        let search_result = response_rx.recv().map_err(|_| EngineError::Storage)?;
        let (cursor, soft_fallback, results, _graph_stats, explanation) = match search_result {
            Ok(result) => result,
            Err(err) => {
                self.emit_sqlite_internal_error(&err);
                return Err(EngineError::Storage);
            }
        };
        Ok(SearchResult { projection_cursor: cursor, soft_fallback, results, explanation })
    }

    /// 0.8.18 Slice 5 (R-VEQ-6) — degraded-open observability accessor. `true` iff
    /// the open-time #5 self-check found a vector-equivalence divergence and every
    /// vector-dependent arm is refusing. Mirrors `OpenReport.dense_disabled`; read
    /// lock-free.
    #[must_use]
    pub fn dense_disabled(&self) -> bool {
        self.dense_disabled.load(Ordering::Acquire)
    }

    /// 0.8.18 Slice 5 (R-VEQ-6) — the human-readable reason for the degraded state
    /// (which representation tripped), or `None` when dense is healthy.
    #[must_use]
    pub fn dense_disabled_reason(&self) -> Option<String> {
        self.dense_disabled_reason.lock().ok().and_then(|g| g.clone())
    }

    /// 0.8.18 Slice 5 (R-VEQ-6) — telemetry counter: number of query-time
    /// vector-dependent-arm refusals raised because the engine opened degraded.
    /// Observable pre/post-query.
    #[must_use]
    pub fn vector_equivalence_refusal_count(&self) -> u64 {
        self.vector_equivalence_refusals.load(Ordering::Relaxed)
    }

    /// Shared event-wrapped body for [`search_reranked`][Engine::search_reranked]
    /// (`explain=false`) and [`search_explained`][Engine::search_explained]
    /// (`explain=true`). Keeps the Started/Finished/Failed lifecycle emissions +
    /// slow detection in one place.
    #[allow(clippy::too_many_arguments)] // mirrors search_reranked + the explain flag
    fn search_reranked_with_explain(
        &self,
        query: &str,
        filter: Option<SearchFilter>,
        rerank_depth: usize,
        use_graph_arm: bool,
        alpha: f64,
        pool_n: usize,
        explain: bool,
        view: ReadView,
    ) -> Result<SearchResult, EngineError> {
        // fix-2: refuse an existence-relaxing view BEFORE any work (and before the
        // Started event), so the refusal is a pure argument error rather than a
        // half-emitted query lifecycle.
        view.reject_existence_relaxation_on_search()?;
        self.emit_event(lifecycle::Phase::Started, lifecycle::EventCategory::Search, None);
        let started = Instant::now();
        let outcome = self.search_inner(
            query,
            filter,
            rerank_depth,
            use_graph_arm,
            alpha,
            pool_n,
            explain,
            view,
        );
        self.detect_slow(started, lifecycle::EventCategory::Search);
        match outcome {
            Ok(result) => {
                self.counters.record_query();
                // 0.8.8 Slice 15 (OPP-9) — opt-in telemetry capture. No-op + no
                // allocation when telemetry is OFF (the default).
                self.capture_telemetry(query, &result);
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

    /// 0.8.8 Slice 15 (OPP-9) — enable opt-in telemetry capture to a local JSONL
    /// `sink_path` (append-only). Off by default; once enabled, each `search`
    /// records a query→result event and `record_feedback` appends agent labels.
    /// Local file only — no network/egress. `query_id` + `ts_monotonic_ms` are
    /// reset deterministically on enable. Idempotent re-enable resets the seq.
    pub fn enable_telemetry(&self, sink_path: &str) -> Result<(), EngineError> {
        // Touch the sink (create + validate writable) before arming capture, so a
        // bad path fails loudly here rather than silently dropping events.
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(sink_path)
            .map_err(|_| EngineError::Storage)?;
        let mut guard = self.telemetry.lock().map_err(|_| EngineError::Storage)?;
        *guard = Some(TelemetrySink {
            path: PathBuf::from(sink_path),
            base: Instant::now(),
            nonce: 0,
            seq: 0,
            last_query_id: None,
        });
        // Arm the fast OFF-path guard LAST (after the sink is installed) so a
        // concurrent search either sees telemetry fully off or fully on.
        self.telemetry_enabled.store(true, Ordering::Release);
        Ok(())
    }

    /// 0.8.8 Slice 15 — the most-recent captured `query_id` (for `record_feedback`).
    /// `None` when telemetry is off or no query has been captured yet.
    pub fn last_telemetry_query_id(&self) -> Option<String> {
        self.telemetry.lock().ok()?.as_ref().and_then(|s| s.last_query_id.clone())
    }

    /// 0.8.8 Slice 15 — capture a query→result telemetry event. No-op (no alloc,
    /// no I/O) when telemetry is off (the default). Best-effort: a sink write error
    /// never fails the search. Captures ONLY ids, arms, and the query LENGTH —
    /// never the query text or `source_id` (privacy, ADR §C).
    ///
    /// ID-SPACES (Cause-A, 0.8.11.2 — honest record). `result_ids` is the interim
    /// `SearchHit.id` == `write_cursor`: within-session consistent but NOT
    /// cross-session-stable (reassigned on re-projection/re-ingest). `arm_of` is
    /// keyed by that same `write_cursor`. Cause-A adds a NEW PARALLEL field
    /// `result_stable_ids` carrying the cross-session-stable id
    /// ([`SearchHit::stable_id`], `logical_id` / content-hash) in the SAME order as
    /// `result_ids`; the existing `write_cursor` keys are RETAINED unchanged so
    /// pre-Cause-A gold and sink byte-output stay valid (the F-8a `id_space` flip
    /// is a separate, conscious step — see
    /// `dev/plans/runs/NOTE-0.8.8-to-steward-id-contract.md`).
    fn capture_telemetry(&self, query: &str, result: &SearchResult) {
        // Fast OFF path (codex §9 P2): a single atomic load when telemetry has
        // never been enabled — NO mutex acquisition, NO contention with the search
        // hot path.
        if !self.telemetry_enabled.load(Ordering::Acquire) {
            return;
        }
        let Ok(mut guard) = self.telemetry.lock() else { return };
        let Some(sink) = guard.as_mut() else { return };
        let query_id = format!("q{}-{}", sink.nonce, sink.seq);
        let ts_monotonic_ms = sink.base.elapsed().as_millis() as u64;
        let mut arm_of = serde_json::Map::new();
        for h in &result.results {
            // Keyed on the engine-internal positional cursor (the pre-C-2
            // `SearchHit.id` == `write_cursor`), byte-unchanged so `record_feedback`
            // + the gold pipeline keep keying on the same `result_ids` space.
            arm_of
                .insert(h.write_cursor.to_string(), serde_json::Value::from(branch_str(h.branch)));
        }
        let event = serde_json::json!({
            "type": "event",
            "schema_version": 1,
            "ts_monotonic_ms": ts_monotonic_ms,
            "query_id": query_id,
            "query_chars": query.chars().count() as u64,
            "result_ids": result.results.iter().map(|h| h.write_cursor).collect::<Vec<u64>>(),
            // Cause-A / C-2: parallel cross-session-stable ids, SAME order as
            // result_ids. Post-C-2 the stable id lives on `SearchHit.id` (its
            // prefixed form == the pre-swap `stable_id` value byte-for-byte), so
            // the emitted bytes are unchanged and the `write_cursor` result_ids
            // keys are retained unchanged (pre-Cause-A gold stays valid).
            "result_stable_ids": result
                .results
                .iter()
                .map(|h| h.id.to_prefixed())
                .collect::<Vec<String>>(),
            "arm_of": arm_of,
        });
        let _ = append_jsonl(&sink.path, &event);
        sink.seq += 1;
        sink.last_query_id = Some(query_id);
    }

    /// 0.8.8 Slice 15 — append an agent-supplied relevance-label record for a
    /// previously-captured `query_id`. `label_source` is the only exogenous string
    /// (caller-declared, e.g. `"agent:hermes"`).
    ///
    /// ID-SPACE (Cause-A, 0.8.11.2 — honest record). `relevant_ids` /
    /// `irrelevant_ids` are the interim `SearchHit.id` == `write_cursor` (the same
    /// space as the captured event's `result_ids`), NOT `logical_id`. The
    /// signature is left byte-stable: the gold pipeline maps these `write_cursor`
    /// keys to the cross-session-stable id via the capture event's parallel
    /// `result_ids` ↔ `result_stable_ids` arrays (`eval/gold_capture.py`), so no
    /// new feedback parameter — and no binding-signature churn — is required.
    /// Errors if telemetry is off.
    pub fn record_feedback(
        &self,
        query_id: &str,
        relevant_ids: &[u64],
        irrelevant_ids: &[u64],
        label_source: &str,
    ) -> Result<(), EngineError> {
        let guard = self.telemetry.lock().map_err(|_| EngineError::Storage)?;
        let sink = guard
            .as_ref()
            .ok_or(EngineError::InvalidArgument { msg: "telemetry is not enabled".to_string() })?;
        // codex §9 [P1] (privacy): `query_id` is an exogenous caller string. Only a
        // deterministic id that `capture_telemetry` has ALREADY emitted may be
        // persisted — otherwise a caller could smuggle query text / a `source_id`
        // into the sink under the `query_id` key. Require the canonical
        // `q{nonce}-{seq}` form with `nonce == sink.nonce` AND `seq < sink.seq`
        // (a seq the capture path has issued). Reject (writing nothing) otherwise.
        let is_issued_id = query_id
            .strip_prefix('q')
            .and_then(|rest| rest.split_once('-'))
            .and_then(|(nonce, seq)| Some((nonce.parse::<u64>().ok()?, seq.parse::<u64>().ok()?)))
            .is_some_and(|(nonce, seq)| nonce == sink.nonce && seq < sink.seq);
        if !is_issued_id {
            return Err(EngineError::InvalidArgument { msg: "unknown query_id".to_string() });
        }
        let record = serde_json::json!({
            "type": "feedback",
            "schema_version": 1,
            "query_id": query_id,
            "relevant_ids": relevant_ids,
            "irrelevant_ids": irrelevant_ids,
            "label_source": label_source,
        });
        append_jsonl(&sink.path, &record).map_err(|_| EngineError::Storage)
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
    #[allow(clippy::too_many_arguments)] // mirrors search_reranked + the explain flag
    fn search_inner(
        &self,
        query: &str,
        filter: Option<SearchFilter>,
        rerank_depth: usize,
        use_graph_arm: bool,
        alpha: f64,
        pool_n: usize,
        explain: bool,
        view: ReadView,
    ) -> Result<SearchResult, EngineError> {
        self.search_inner_with_stats(
            query,
            filter,
            rerank_depth,
            use_graph_arm,
            alpha,
            pool_n,
            explain,
            view,
        )
        .map(|(result, _stats)| result)
    }

    /// G0 Phase-2: the search body, additionally returning the graph-arm frontier
    /// meter. Only the `_graph_frontier_stats_for_test` seam consumes the stats;
    /// `search_inner` (and thus `search_reranked` / `search`) drops them.
    #[allow(clippy::too_many_arguments)] // mirrors search_reranked + the explain flag
    fn search_inner_with_stats(
        &self,
        query: &str,
        filter: Option<SearchFilter>,
        rerank_depth: usize,
        use_graph_arm: bool,
        alpha: f64,
        pool_n: usize,
        explain: bool,
        view: ReadView,
    ) -> Result<(SearchResult, GraphFrontierStats), EngineError> {
        self.ensure_open()?;
        // 0.8.18 Slice 5 (#5 vector-equivalence probe, R-VEQ-4) — the SINGLE
        // vector-dependent choke point. If the open-time self-check found a
        // divergence beyond the D4 floor, refuse EVERY vector-dependent arm
        // (search / search_expand / explain-rerank / graph-arm all funnel here)
        // BEFORE any embedding / vector SQL / graph seeding / CE rerank — no
        // silent partial results. The text-only/FTS-only path
        // (`search_text_only`) does NOT route through here, so FTS stays
        // serviceable in degraded mode.
        if self.dense_disabled.load(Ordering::Acquire) {
            self.vector_equivalence_refusals.fetch_add(1, Ordering::Relaxed);
            let reason =
                self.dense_disabled_reason.lock().ok().and_then(|g| g.clone()).unwrap_or_else(
                    || "open-time #5 vector-equivalence self-check failed".to_string(),
                );
            return Err(EngineError::VectorEquivalenceMismatch { reason });
        }
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
        let importance_enabled =
            self.projection_runtime.shared.importance_reweight_enabled.load(Ordering::SeqCst);
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
            importance_enabled,
            vector_stage_only,
            raw_query: Box::from(query), // FIX-4: Box<str> (16B) not String (24B)
            rerank_depth,
            use_graph_arm,
            alpha,
            pool_n,
            explain,
            view,
            respond: response_tx,
        };
        if self.reader_pool.dispatch(request).is_err() {
            return Err(EngineError::Closing);
        }
        let search_result = response_rx.recv().map_err(|_| EngineError::Storage)?;
        let (cursor, soft_fallback, results, graph_stats, explanation) = match search_result {
            Ok(result) => result,
            Err(err) => {
                self.emit_sqlite_internal_error(&err);
                return Err(EngineError::Storage);
            }
        };

        // The worker (`read_search_in_tx`) has no embedder identity; fill the
        // trace's `embedder_id` here, where `self.runtime_embedder_identity` is in
        // scope. Only on the explain path (`explanation` is `Some`).
        let explanation = explanation.map(|mut exp| {
            let id = &self.runtime_embedder_identity;
            exp.trace.embedder_id = format!("{}@{} (dim={})", id.name, id.revision, id.dimension);
            exp
        });

        Ok((
            SearchResult { projection_cursor: cursor, soft_fallback, results, explanation },
            graph_stats,
        ))
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
        self.search_inner_with_stats(query, None, 0, true, 0.3, 0, false, ReadView::default())
            .map(|(_result, stats)| stats)
    }

    /// Slice 30 (G2) — `read.get`: active-only point lookup by `logical_id`.
    /// Delegates to [`Engine::read_get_many`]; returns the single slot. A
    /// missing/superseded id is `None` (a normal absence, not an error). Reads
    /// ride the ReaderWorkerPool DEFERRED-tx path (never the writer lock).
    pub fn read_get(
        &self,
        logical_id: &str,
        view: &ReadView,
    ) -> Result<Option<NodeRecord>, EngineError> {
        let ids = [logical_id.to_string()];
        let rows = self.read_get_many(&ids, view)?;
        Ok(rows.into_iter().next().flatten())
    }

    /// Slice 30 (G2) — `read.get_many`: active-only point lookup over many
    /// `logical_id`s. Returns one slot per requested id in REQUEST ORDER, `None`
    /// where no active row carries that id (partial, never all-or-nothing).
    pub fn read_get_many(
        &self,
        logical_ids: &[String],
        view: &ReadView,
    ) -> Result<Vec<Option<NodeRecord>>, EngineError> {
        self.ensure_open()?;
        if logical_ids.is_empty() {
            return Ok(Vec::new());
        }
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        let request = ReaderRequest::GetById {
            logical_ids: logical_ids.to_vec(),
            view: *view,
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
        view: &ReadView,
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
            view: *view,
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
        // 0.8.5: depth=0 → no rerank, so α/pool_n (0.3, 0) are inert here.
        let search_result =
            self.search_inner(query, filter, 0, false, 0.3, 0, false, ReadView::default())?;
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
        view: &ReadView,
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
            view: *view,
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

    /// 0.8.11 Slice 40 (#17) — unified-`Filter` entry point for the
    /// canonical_nodes `read.list` backend. Accepts the **full** [`FilterTerm`]
    /// set (D3): `Json` runs the shipped allowlisted `json_extract` path;
    /// `Status`/`CreatedAfter` lower to allowlisted json-paths; `Kind`/`SourceType`
    /// **constant-fold** against the partition `kind` (a guaranteed-empty fold
    /// returns an empty `Vec` without touching SQL). Dispatches to the same
    /// [`Engine::read_list`] machinery the shipped `Predicate` surface uses, so
    /// every inherited invariant (`superseded_at IS NULL`, `json_valid(body)`,
    /// the `canonical_nodes(kind)` index, parameterized binds) is preserved.
    pub fn read_list_filter(
        &self,
        kind: &str,
        filter: &Filter,
        limit: usize,
        view: &ReadView,
    ) -> Result<Vec<NodeRecord>, EngineError> {
        self.ensure_open()?;
        match filter.lower_for_read_list(kind)? {
            None => Ok(Vec::new()),
            Some(preds) => self.read_list(kind, &preds, limit, view),
        }
    }

    /// 0.8.20 Slice 10b (R-20-NV) — the **validity-boundary hook**: which nodes
    /// crossed a `[valid_from, valid_until)` boundary in the half-open interval
    /// `(since, as_of]`?
    ///
    /// `since` and the resolved upper bound are INTEGER epoch SECONDS. The upper
    /// bound is the view's own instant (`view.valid_as_of`, defaulting to now),
    /// so one instant governs both the boundary interval and the view — and, as
    /// everywhere else on this path, it is BOUND, never a `datetime('now')`
    /// literal, so the answer is deterministic for a fixed `(since, as_of)`.
    ///
    /// A node appears once, carrying whichever of the two boundaries it crossed;
    /// a window that both opened AND closed inside the interval reports both.
    /// Rows with an unbounded window on a side cannot cross that side, so a
    /// NULL/NULL row (every row predating schema step 22) never appears.
    ///
    /// The view's EXISTENCE flags still apply (so by default only current,
    /// active rows are considered), but its validity predicate does NOT: the
    /// question is about boundary crossings, not about being valid right now.
    ///
    /// When the view relaxes validity entirely (`include_out_of_window`), the
    /// interval is unbounded above.
    ///
    /// This is world-time only. There is deliberately no transaction-time
    /// (`history_as_of`) counterpart.
    pub fn crossed_boundary_since(
        &self,
        since: i64,
        view: &ReadView,
    ) -> Result<Vec<BoundaryCrossing>, EngineError> {
        self.ensure_open()?;
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        let request =
            ReaderRequest::CrossedBoundarySince { since, view: *view, respond: response_tx };
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
            source_id: SourceId::engine_derived("poison-fixture"),
            logical_id: None,
            state: InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
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
                    source_id: SourceId::engine_derived("poison-fixture"),
                    logical_id: None,
                    state: InitialState::Active,
                    reason: None,
                    valid_from: None,
                    valid_until: None,
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

    /// OPP-12 Phase-1 (0.8.19 Slice 10) — read the writer connection's
    /// `PRAGMA secure_delete` (design §3 gap-4). `true` iff the standing
    /// connection-open PRAGMA is in effect, so `purge` freelist erasure is
    /// complete without a per-purge `VACUUM`.
    #[doc(hidden)]
    pub fn secure_delete_enabled_for_test(&self) -> Result<bool, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        let value: i64 = connection
            .query_row("PRAGMA secure_delete", [], |r| r.get(0))
            .map_err(|_| EngineError::Storage)?;
        Ok(value != 0)
    }

    /// OPP-12 Phase-1 (0.8.19 Slice 10, design §3 gap-4) — `true` iff EVERY
    /// reader-pool connection reports `PRAGMA secure_delete = ON`. Broadcasts a
    /// per-worker probe; proves the standing flag is set on the non-writer
    /// connections (which perform projection/vector-rewrite DELETEs), closing
    /// the GDPR-erasure leak codex flagged.
    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn reader_secure_delete_enabled_for_test(&self) -> Result<bool, EngineError> {
        self.ensure_open()?;
        let per_worker = self.reader_pool.secure_delete_per_worker();
        if per_worker.is_empty() {
            return Err(EngineError::Storage);
        }
        Ok(per_worker.iter().all(|&v| v == 1))
    }

    /// OPP-12 Phase-1 (0.8.19 Slice 10, design §3 gap-4) — `true` iff a freshly
    /// opened projection/runtime connection (`open_runtime_connection`) reports
    /// `PRAGMA secure_delete = ON`. The runtime connection performs the
    /// vector-rewrite/projection DELETEs, so its freed pages must be scrubbed too.
    #[doc(hidden)]
    pub fn runtime_secure_delete_enabled_for_test(&self) -> Result<bool, EngineError> {
        self.ensure_open()?;
        let connection = open_runtime_connection(&self.path).map_err(|_| EngineError::Storage)?;
        let value: i64 = connection
            .query_row("PRAGMA secure_delete", [], |r| r.get(0))
            .map_err(|_| EngineError::Storage)?;
        Ok(value != 0)
    }

    /// EXP-S (0.8.14 Slice 5, D1) — write one canonical node row carrying an
    /// explicit structural `row_kind` (leaf/coverage/graph), routing the index
    /// projection through the SAME `row_kind -> index-target` dispatch seam
    /// (`project_canonical_node_row`) as the production `leaf` write path.
    ///
    /// This is the internal-only writer for `coverage`/`graph` rows (there is no
    /// public SDK surface for `row_kind` in 0.8.14). Cursor assignment preserves
    /// the `rowid == write_cursor == cursor` determinism identity. When the row
    /// projects into an async vector index, the worker pool is notified so the
    /// embed is scheduled exactly as for a normal write.
    #[doc(hidden)]
    pub fn write_canonical_row_with_kind_for_test(
        &self,
        kind: &str,
        body: &str,
        row_kind: RowKind,
    ) -> Result<WriteReceipt, EngineError> {
        self.ensure_open()?;
        let mut connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_mut().ok_or(EngineError::Closing)?;

        // R-20-E3 / design §4 item 6 — this writer BYPASSES `PreparedWrite`, so
        // the `SourceId` newtype cannot reach it; before 0.8.20 it inserted a
        // literal NULL `source_id` and produced a row that no `excise_source`
        // call could reach. Engine-derived rows instead take a reserved
        // `_engine:*` provenance, keyed by the structural role that produced
        // them, so they are both erasable and distinguishable from caller data.
        let engine_provenance = SourceId::engine_derived(row_kind.as_str());

        let cursor = self.next_cursor.load(Ordering::SeqCst).saturating_add(1);
        let enqueued = {
            let tx = connection.transaction().map_err(|_| EngineError::Storage)?;
            // 0.8.20 Slice 15b (TC-34) — this writer takes NO validity window, and
            // that is deliberate rather than an oversight. It is a `#[doc(hidden)]`
            // test-only writer for the internal `coverage`/`graph` row kinds, which
            // have no public SDK surface at all (see the doc comment above); the
            // caller-facing authoring path is `PreparedWrite::Node`, handled in
            // `commit_batch`. Omitting the columns binds NULL — the migration
            // step-22 default and the UNBOUNDED reading — so engine-derived rows
            // stay valid at every instant, which is the only correct answer for a
            // structural row that no caller can address a window to.
            tx.execute(
                "INSERT INTO canonical_nodes(write_cursor, kind, body, source_id, logical_id, row_kind)
                 VALUES(?1, ?2, ?3, ?4, NULL, ?5)",
                params![cursor, kind, body, engine_provenance.as_str(), row_kind.as_str()],
            )
            .map_err(|_| EngineError::Storage)?;
            let enqueued = project_canonical_node_row(
                &tx,
                cursor,
                kind,
                body,
                row_kind,
                ProjectionPass::Write,
                // This #[doc(hidden)] writer inserts with the column DEFAULT
                // `state = 'active'` (no state column in its INSERT), so the row
                // is always active and its attributes project.
                true,
            )
            .map_err(|_| EngineError::Storage)?;
            advance_projection_cursor(&tx).map_err(|_| EngineError::Storage)?;
            tx.commit().map_err(|_| EngineError::Storage)?;
            enqueued
        };
        self.next_cursor.store(cursor, Ordering::SeqCst);
        if enqueued {
            self.projection_runtime.notify_new_work();
        }
        Ok(WriteReceipt { cursor, row_cursors: vec![cursor], dangling_edge_endpoints: 0 })
    }

    /// EXP-S (0.8.14 Slice 5, D1) — select the active canonical rows carrying a
    /// given `row_kind`, returning their `write_cursor`s in cursor order. Proves
    /// the engine can query/select rows by the structural `row_kind` axis.
    #[doc(hidden)]
    pub fn canonical_rows_with_row_kind_for_test(
        &self,
        row_kind: RowKind,
    ) -> Result<Vec<u64>, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        let mut stmt = connection
            .prepare(
                "SELECT write_cursor FROM canonical_nodes
                 WHERE row_kind = ?1 AND superseded_at IS NULL
                 ORDER BY write_cursor",
            )
            .map_err(|_| EngineError::Storage)?;
        let cursors = stmt
            .query_map(params![row_kind.as_str()], |row| row.get::<_, u64>(0))
            .map_err(|_| EngineError::Storage)?
            .collect::<rusqlite::Result<Vec<u64>>>()
            .map_err(|_| EngineError::Storage)?;
        Ok(cursors)
    }

    /// F5 (0.8.14 Slice 10) — the fielded BM25F lexical arm over
    /// `search_index_v2`. Recalls candidate rows through the FTS5 index
    /// (`search_index_v2 MATCH`) and scores them with a textbook BM25F using the
    /// plan's tunable per-field `weights` and tunable `b`/`k1`, returning
    /// `(write_cursor, score)` in descending score order (write_cursor asc as the
    /// deterministic tiebreak). Superseded node versions are excluded (join to
    /// `canonical_nodes WHERE superseded_at IS NULL`).
    ///
    /// This is the engine-internal `BM25fQueryPlan` compiler path (`ADR-0.8.1`
    /// §3.2); there is no public Py/TS SDK surface this release. The score is
    /// computed in-engine (not via SQLite's `bm25()`, which cannot express a
    /// tunable `b`); the FTS5 index remains load-bearing for candidate recall.
    #[doc(hidden)]
    pub fn bm25f_search(
        &self,
        query: &str,
        plan: &Bm25fQueryPlan,
    ) -> Result<Vec<(u64, f64)>, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        bm25f_search_inner(connection, query, plan).map_err(|_| EngineError::Storage)
    }

    /// Embed arbitrary text with the engine's configured runtime embedder,
    /// returning the raw (un-centered) vector.
    ///
    /// This is the read-path embed primitive: it mirrors the search
    /// query-embedding path — a single, direct [`Embedder::embed`] call. The
    /// per-`embed()` watchdog/circuit-breaker guards only the bulk
    /// projection/write path (many embeds, fault isolation), not single
    /// read-side embeds, so a direct call is consistent with how a query is
    /// embedded. Callers get vectors under the engine's *pinned* embedder
    /// identity (`fathomdb-bge-small-en-v1.5` by default) rather than a
    /// parallel, possibly-divergent embedder.
    ///
    /// Returns [`EngineError::EmbedderNotConfigured`] if the engine was opened
    /// without an embedder (`use_default_embedder = false`).
    pub fn embed_text(&self, text: &str) -> Result<Vec<f32>, EngineError> {
        self.ensure_open()?;
        let embedder =
            self.runtime_embedder.as_ref().cloned().ok_or(EngineError::EmbedderNotConfigured)?;
        embedder.embed(text).map_err(map_runtime_embedder_error)
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

    /// 0.8.16 Slice 5 / F9 test seam — flip the dedicated importance/confidence
    /// reweight flag (off by default). The reweight runs AFTER bit-KNN + RRF on
    /// the fused hits (multiplicative-on-fused, `NULL ⇒ neutral`); it is never a
    /// vec0 predicate and is NOT `fusion_mode`. Mirrors
    /// `set_recency_reweight_enabled_for_test`.
    #[doc(hidden)]
    pub fn set_importance_reweight_enabled_for_test(&self, enabled: bool) {
        self.projection_runtime.shared.importance_reweight_enabled.store(enabled, Ordering::SeqCst);
    }

    /// 0.8.16 Slice 5 / F9 (R-F9-1) — set the caller-supplied `importance` ranking
    /// scalar on the `canonical_nodes` row identified by `write_cursor` (the
    /// interim id `SearchHit.id` carries). Validates `importance ∈ [0.0, 1.0]`,
    /// mirroring the existing `canonical_edges.confidence` write-path check —
    /// an out-of-range value is a deterministic [`EngineError::WriteValidation`].
    ///
    /// The 3-way sentinel: NOT calling this leaves the column `NULL` (never
    /// assigned = graceful-absent, ranks NEUTRAL); `0.0` is the explicit floor;
    /// `(0.0, 1.0]` is an explicit importance. Importance is a caller-supplied
    /// scalar — the engine does NOT compute graph-centrality importance (ADR §4
    /// non-goal). Engine-internal minimal surface for this keystone; SDK (Py/TS)
    /// exposure is a Slice-40 concern.
    pub fn write_node_importance(
        &self,
        write_cursor: u64,
        importance: f64,
    ) -> Result<(), EngineError> {
        if !importance.is_finite() || !(0.0..=1.0).contains(&importance) {
            return Err(EngineError::WriteValidation);
        }
        self.ensure_open()?;
        let mut connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_mut().ok_or(EngineError::Closing)?;
        connection
            .execute(
                "UPDATE canonical_nodes SET importance = ?1 WHERE write_cursor = ?2",
                params![importance, write_cursor],
            )
            .map_err(|_| EngineError::Storage)?;
        Ok(())
    }

    /// 0.8.16 Slice 5 / F9 (R-F9-1) — read back the `importance` scalar for the
    /// `canonical_nodes` row identified by `write_cursor`. `None` = SQL `NULL` =
    /// never assigned (graceful-absent). The reciprocal read for
    /// [`Engine::write_node_importance`].
    pub fn node_importance(&self, write_cursor: u64) -> Result<Option<f64>, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        connection
            .query_row(
                "SELECT importance FROM canonical_nodes WHERE write_cursor = ?1 LIMIT 1",
                params![write_cursor],
                |r| r.get::<_, Option<f64>>(0),
            )
            .map_err(|_| EngineError::Storage)
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

    /// OPP-12 Phase-1 (0.8.19 Slice 10) — resolve a lifecycle-verb id argument to
    /// the BARE `logical_id` it addresses, enforcing `Logical`(`l:`)-only
    /// addressability (design §3). An untagged string is taken as a bare
    /// `logical_id` (the `l:` form); an explicit `l:`-prefixed string is stripped
    /// to its value; a `Content`(`h:`) or `Passage`(`p:`) id is a typed
    /// [`EngineError::NotLifecycleAddressable`] refusal (never a panic / no-op).
    fn resolve_lifecycle_target(id: &str) -> Result<String, EngineError> {
        match IdSpace::parse(id) {
            Some(parsed) => match parsed.space {
                IdSpaceKind::Logical => Ok(parsed.value),
                other => Err(EngineError::NotLifecycleAddressable { id_space: other }),
            },
            // Untagged — no id-space prefix; treat as a bare logical_id (l: space).
            None => Ok(id.to_string()),
        }
    }

    /// OPP-12 Phase-1 (0.8.19 Slice 10, R-TR-1/2) — move a governed node between
    /// existence states per the engine-enforced legal-transition table (design
    /// §2): promote `pending→active`, reject `pending→deleted`, soft-delete
    /// `active→deleted`, undelete `deleted→active`. `to_state` is a full
    /// [`LifecycleState`], but `Pending` (create-time only) and `Purged`
    /// (`purge`-only) are never legal `transition` targets, nor are self-loops or
    /// any move from a non-existent/`purged` row — each returns a typed
    /// [`EngineError::IllegalTransition`] enumerating the legal targets.
    ///
    /// `reason` semantics (design §3 gap-6): promote/undelete CLEAR `reason` to
    /// `NULL` (the row is admitted; no standing cause); reject/soft-delete SET
    /// `reason` to the supplied value (`NULL` allowed but the delete-family
    /// expects it). `reason` is advisory — the engine never interprets it.
    ///
    /// Keys on the BARE `logical_id` (`l:` space only); a `Content`(`h:`) or
    /// `Passage`(`p:`) id raises [`EngineError::NotLifecycleAddressable`].
    /// The state flip mutates the single active (`superseded_at IS NULL`) row; a
    /// `deleted` row STAYS node-FTS / vector indexed (gap-5) — only the
    /// `state='active'` default filter excludes those shadows, so an undelete
    /// needs no re-projection there.
    ///
    /// 0.8.20 Slice 15d fix-2 [P2] — the row-owned ATTRIBUTE projection
    /// (`canonical_attributes` / `property_search_index`) is the exception: it has
    /// NO read-side lifecycle filter (the property-FTS5 table cannot carry one), so
    /// it is maintained AT REST to track the backfill's set
    /// (projected ⟺ active ∧ non-superseded). Promote/undelete PROJECT the declared
    /// attributes; soft-delete PURGES them; reject is a no-op.
    pub fn transition(
        &self,
        logical_id: &str,
        to_state: LifecycleState,
        reason: Option<String>,
    ) -> Result<(), EngineError> {
        self.ensure_open()?;
        let lid = Self::resolve_lifecycle_target(logical_id)?;

        // Settle in-flight projection work first: the async projection worker
        // commits vector/FTS shadows on its OWN connection via `BEGIN IMMEDIATE`,
        // so a state flip issued while a worker holds the write lock would
        // SQLITE_BUSY. Draining (unfrozen so any unprojected row completes) leaves
        // the worker idle; a bare state flip enqueues no new projection work.
        self.drain(LIFECYCLE_DRAIN_TIMEOUT_MS)?;

        let mut connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_mut().ok_or(EngineError::Closing)?;
        let tx = connection.transaction().map_err(|_| EngineError::Storage)?;

        // The lifecycle state lives on the single active (superseded_at IS NULL)
        // version; a `deleted` row is still that active version, just flagged.
        // fix-2 [P2] — also read its `write_cursor` + `body` so the row-owned
        // attribute projection can be maintained after the state flip.
        let current: Option<(String, i64, String)> = tx
            .query_row(
                "SELECT state, write_cursor, body FROM canonical_nodes \
                 WHERE logical_id = ?1 AND superseded_at IS NULL",
                params![lid],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)),
            )
            .optional()
            .map_err(|_| EngineError::Storage)?;

        // A missing active row is an absent/purged node — the terminal `Purged`
        // state for legality purposes (nothing is a legal target from there).
        let from_state = match &current {
            Some((s, _, _)) => LifecycleState::from_str_opt(s).ok_or(EngineError::Storage)?,
            None => LifecycleState::Purged,
        };

        if !is_legal_transition_move(from_state, to_state) {
            return Err(EngineError::IllegalTransition {
                from_state,
                to_state,
                legal: from_state.legal_next_states(),
            });
        }

        // Admit (promote/undelete) → clear reason; exclude (reject/soft-delete) →
        // set the supplied reason. `to_state` is Active or Deleted here.
        let new_reason: Option<String> = match to_state {
            LifecycleState::Active => None,
            _ => reason,
        };
        tx.execute(
            "UPDATE canonical_nodes SET state = ?1, reason = ?2 \
             WHERE logical_id = ?3 AND superseded_at IS NULL",
            params![to_state.as_str(), new_reason, lid],
        )
        .map_err(|_| EngineError::Storage)?;

        // fix-2 [P2] — maintain the row-owned attribute projection so it keeps
        // tracking the backfill's set (projected ⟺ active ∧ non-superseded). The
        // transitioned row is the single non-superseded version, so the invariant
        // reduces to `projected ⟺ to_state == Active`. We PURGE unconditionally
        // (idempotent — a no-op on the never-projected pending / already-purged
        // deleted arms) then RE-PROJECT when landing `Active`. This covers every
        // legal move: promote (pending→active) projects the withheld attributes;
        // soft-delete (active→deleted) purges; undelete (deleted→active)
        // re-projects; reject (pending→deleted) is a no-op. The property tables
        // (`canonical_attributes` / `property_search_index`) carry NO read-side
        // lifecycle filter — unlike node-FTS / vector shadows, which the canonical
        // read path already excludes when non-active — so they MUST be maintained
        // at rest, the same rationale as fix-1's purge-on-supersede. Node-FTS /
        // vector shadows are deliberately left intact (gap-5: a deleted row STAYS
        // indexed; only the `state='active'` default read filter hides it).
        if let Some((cursor, body)) = current.as_ref().map(|(_, c, b)| (*c, b.as_str())) {
            purge_row_projections_for_cursor_in(
                &tx,
                cursor,
                &[ProjectionClass::Attribute, ProjectionClass::PropertyFts],
            )
            .map_err(|_| EngineError::Storage)?;
            if matches!(to_state, LifecycleState::Active) {
                project_node_attributes(&tx, cursor, body).map_err(|_| EngineError::Storage)?;
            }
        }
        tx.commit().map_err(|_| EngineError::Storage)?;
        self.counters.record_admin();
        Ok(())
    }

    /// 0.8.20 Slice 15d (R-20-PR / C-1) — the projection registry as a
    /// DECLARATIVE, IDEMPOTENT apply. The engine is the SOLE projection authority
    /// (Q3): it diffs the supplied `specs` against the durable registry and
    /// backfills the difference in ONE transaction. Cheap projections
    /// (`filterable`, `searchable→FTS`) are built same-transaction; `rankable`
    /// and the `searchable→vector` sub-target are PERSISTED but deferred (F9 /
    /// Slice 20) — declaring them never errors (graceful-absent, Q6a).
    ///
    /// `drop` is EXPLICIT (C3, `api-surface.md:27`): omission of a live
    /// projection from `specs` does NOT drop it; removal requires naming it in
    /// `drop`. An incompatible/destructive change to a live projection that is
    /// NOT in `drop` is refused with [`EngineError::ProjectionDestructive`], the
    /// destructive delta surfaced — never silent data loss. Re-applying an
    /// unchanged spec diffs to a no-op ([`ProjectionDelta::unchanged`]).
    ///
    /// Pair with [`Engine::read_projections`] to see current state before
    /// applying.
    pub fn configure_projections(
        &self,
        specs: &[ProjectionSpec],
        drop: &[String],
    ) -> Result<ProjectionDelta, EngineError> {
        self.ensure_open()?;
        // Settle in-flight async projection work first (same rationale as
        // `transition`): the async worker commits on its own connection, so a
        // backfill issued while it holds the write lock would SQLITE_BUSY.
        self.drain(LIFECYCLE_DRAIN_TIMEOUT_MS)?;

        let mut connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_mut().ok_or(EngineError::Closing)?;
        let tx = connection.transaction().map_err(|_| EngineError::Storage)?;
        let delta = apply_projection_config(&tx, specs, drop)?;
        tx.commit().map_err(|_| EngineError::Storage)?;
        self.counters.record_admin();
        Ok(delta)
    }

    /// 0.8.20 Slice 15d (R-20-PR) — read the current projection registry (C5
    /// introspection: `read.projections`). Returns every declared
    /// [`ProjectionSpec`] sorted by name, so a caller can inspect current state
    /// (and the destructive delta a change would cause) BEFORE applying. Pure
    /// read; never mutates.
    pub fn read_projections(&self) -> Result<Vec<ProjectionSpec>, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        let registry = load_projection_registry(connection).map_err(|_| EngineError::Storage)?;
        Ok(registry.iter().map(|(name, stored)| stored.to_spec(name)).collect())
    }

    /// OPP-12 Phase-1 (0.8.19 Slice 10, R-PG-1/2) — irreversibly hard-erase a
    /// governed node. A SEPARATE verb from [`Engine::transition`] (NOT on the
    /// `recovery_denylist`). Precondition: DELETED-FIRST — legal only from
    /// `deleted` (else a typed [`EngineError::IllegalTransition`] to `purged`);
    /// IDEMPOTENT — purging an already-absent/already-purged id is a no-op
    /// success. Keys on the bare `logical_id` (`l:` only); `h:`/`p:` →
    /// [`EngineError::NotLifecycleAddressable`].
    ///
    /// In ONE transaction, physically erases every ROW-OWNED target for the node
    /// (design §3 / gap-3): all `canonical_nodes` versions; its `search_index`,
    /// `search_index_edges`, `search_index_v2` FTS rows; its `vector_default`
    /// (vec0) + `_fathomdb_vector_rows` vectors; its `_fathomdb_projection_terminal`
    /// bookkeeping; and — CASCADE-REMOVE, no content-free stubs — every
    /// `canonical_edges` row touching it (`from_id`/`to_id`) plus those edges'
    /// projection shadows. The global/kind-level registries
    /// `_fathomdb_projection_state` and `_fathomdb_vector_kinds` are NOT keyed to
    /// a node id and are DELIBERATELY untouched.
    ///
    /// Erasure completeness relies on the standing `PRAGMA secure_delete=ON`
    /// (design §3 gap-4) which zeroes every freed page — so no per-purge `VACUUM`.
    /// (Freelist content written on a pre-20 DB before `secure_delete` was on is a
    /// documented residual; there is no forced migration-time `VACUUM`.)
    pub fn purge(&self, logical_id: &str) -> Result<(), EngineError> {
        self.ensure_open()?;
        let lid = Self::resolve_lifecycle_target(logical_id)?;

        // Drain in-flight projection work before the erase, exactly as
        // `excise_source` does: SQLite-WAL would otherwise let a worker that
        // already dequeued a job for a purged cursor commit its vec0 /
        // `_fathomdb_vector_rows` INSERT after our DELETE releases the writer
        // lock, leaving residue that defeats the erasure sweep.
        // Settle every pending projection FIRST (unfrozen) so no unprojected row
        // is left behind that a subsequent freeze would wedge `drain` on, and so
        // the async worker is idle. THEN freeze the scanner (no new work is queued
        // while we erase), confirm idle, and erase in one writer transaction.
        // Freezing before the first drain would stall projection of any
        // just-written row → `database_has_pending_projection_work` never clears →
        // `drain` times out into `Scheduler`.
        self.drain(LIFECYCLE_DRAIN_TIMEOUT_MS)?;
        self.projection_runtime.set_frozen(true);
        let outcome = self.drain(LIFECYCLE_DRAIN_TIMEOUT_MS).and_then(|()| self.purge_inner(&lid));
        self.projection_runtime.set_frozen(false);
        // 0.8.20 Slice 5b (R-20-E5/E6) — the rows are gone from the tables; now
        // finish the erasure AT REST (telemetry sink + `-wal` bytes) before
        // reporting success. Runs after the connection guard inside
        // `purge_inner` has been dropped: `complete_erasure_at_rest` re-acquires
        // it for the checkpoint.
        outcome?;
        self.complete_erasure_at_rest("purge")
    }

    /// The erased rows' prefixed stable ids ([`IdSpace::to_prefixed`]) are NOT
    /// returned: they are enqueued for redaction inside this transaction (see
    /// [`enqueue_pending_redaction`]), because a caller-held vector is lost on the
    /// retry path that codex Â§9 P2 found.
    fn purge_inner(&self, lid: &str) -> Result<(), EngineError> {
        let mut connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_mut().ok_or(EngineError::Closing)?;
        let tx = connection.transaction().map_err(|_| EngineError::Storage)?;

        // Precondition on the active row's state. Absent (never-created or
        // already-purged) → idempotent no-op success.
        let current: Option<String> = tx
            .query_row(
                "SELECT state FROM canonical_nodes \
                 WHERE logical_id = ?1 AND superseded_at IS NULL",
                params![lid],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| EngineError::Storage)?;
        let from_state = match current {
            None => {
                // Idempotent: nothing to erase.
                tx.commit().map_err(|_| EngineError::Storage)?;
                return Ok(());
            }
            Some(s) => LifecycleState::from_str_opt(&s).ok_or(EngineError::Storage)?,
        };
        if from_state != LifecycleState::Deleted {
            // Deleted-first precondition. Dropping `tx` rolls back (no-op read).
            return Err(EngineError::IllegalTransition {
                from_state,
                to_state: LifecycleState::Purged,
                legal: from_state.legal_next_states(),
            });
        }

        // Collect every version cursor for the node, plus every cursor of an edge
        // that touches it (either endpoint), across ALL versions — the projection
        // shadow tables are keyed by these per-row `write_cursor`s.
        let node_cursors: Vec<i64> = {
            let mut stmt = tx
                .prepare("SELECT write_cursor FROM canonical_nodes WHERE logical_id = ?1")
                .map_err(|_| EngineError::Storage)?;
            let rows = stmt
                .query_map(params![lid], |row| row.get::<_, i64>(0))
                .map_err(|_| EngineError::Storage)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|_| EngineError::Storage)?
        };
        let edge_cursors: Vec<i64> = {
            let mut stmt = tx
                .prepare(
                    "SELECT write_cursor FROM canonical_edges \
                     WHERE from_id = ?1 OR to_id = ?1",
                )
                .map_err(|_| EngineError::Storage)?;
            let rows = stmt
                .query_map(params![lid], |row| row.get::<_, i64>(0))
                .map_err(|_| EngineError::Storage)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|_| EngineError::Storage)?
        };

        // 0.8.20 Slice 5b (R-20-E6) — the stable ids the telemetry sink may have
        // persisted for these rows, collected BEFORE the DELETEs.
        let erased_stable_ids = collect_erased_stable_ids(
            &tx,
            "SELECT logical_id, body FROM canonical_nodes WHERE logical_id = ?1",
            "SELECT logical_id, body FROM canonical_edges WHERE from_id = ?1 OR to_id = ?1",
            lid,
        )?;

        // Erase the row-owned projection shadows for every collected cursor.
        // 0.8.20 Slice 5a (R-20-E1): registry-driven — the hand-rolled delete
        // list is gone, so a newly registered projection table is erased here
        // without touching this site. vec0 rowid == the canonical row's
        // write_cursor (see `_fathomdb_vector_rows`).
        for cursor in node_cursors.iter().chain(edge_cursors.iter()) {
            erase_row_projections(&tx, *cursor).map_err(|_| EngineError::Storage)?;
        }

        // Erase the canonical rows: all node versions + all touching edges
        // (gap-3 CASCADE-REMOVE — no content-free stubs in Phase-1).
        tx.execute("DELETE FROM canonical_nodes WHERE logical_id = ?1", params![lid])
            .map_err(|_| EngineError::Storage)?;
        tx.execute("DELETE FROM canonical_edges WHERE from_id = ?1 OR to_id = ?1", params![lid])
            .map_err(|_| EngineError::Storage)?;

        // 0.8.20 Slice 5 fix-1 (codex §9 P2) — durably record the redaction this
        // erasure now owes, atomically with the deletes above. Only when a sink
        // is attached: with telemetry never enabled there is no file the ids
        // could have leaked into, so there is nothing to owe.
        let pending_cursor = self.next_cursor.load(Ordering::SeqCst).saturating_add(1);
        let enqueued =
            self.telemetry_enabled.load(Ordering::Acquire) && !erased_stable_ids.is_empty();
        if enqueued {
            enqueue_pending_redaction(&tx, "purge", &erased_stable_ids, pending_cursor)?;
        }

        tx.commit().map_err(|_| EngineError::Storage)?;
        if enqueued {
            self.next_cursor.store(pending_cursor, Ordering::SeqCst);
        }
        self.counters.record_admin();
        Ok(())
    }

    /// 0.8.20 Slice 5d (R-20-E4, design §4 item 9b) — the **governed SDK
    /// erasure verb**. Deletes every canonical row attributable to `source_id`,
    /// plus its row-owned projections, and finishes the erasure at rest.
    ///
    /// This is NOT `operator`-gated: erasing content a consumer wrote is an
    /// application obligation, not a recovery workflow. Before this slice the
    /// only erasure path was [`Engine::excise_source`], which lives behind the
    /// operator feature (i.e. the CLI), so an SDK-only consumer holding a
    /// deletion obligation over ANONYMOUS content — content with no
    /// `logical_id`, therefore not reachable by [`Engine::purge`] — had no way
    /// to discharge it at all. That gap is what R-20-E4 closes.
    ///
    /// **One engine path.** `erase_source` and `excise_source` are the SAME
    /// operation: both delegate to [`Engine::erase_source_shared`]. They are
    /// not competing implementations, and no behaviour is duplicated.
    ///
    /// **Validation differs, deliberately.** `erase_source` admits only ids
    /// [`SourceId::new`] would admit, so a caller cannot aim the governed verb
    /// at the engine's reserved `_`-prefixed namespace (`_engine:*` substrate,
    /// or the `_legacy:pre-0.8.20` cohort migration step 21 back-filled — a
    /// single call against which would erase every pre-0.8.20 anonymous row).
    /// `excise_source` stays permissive precisely BECAUSE it is the recovery
    /// seam: R-20-E8 requires an operator to be able to excise `_legacy:`.
    ///
    /// **Not a recovery verb.** `erase_source` carries no REQ-054
    /// recovery-denylist name (`{recover, restore, repair, fix, rebuild}`); it
    /// is a lifecycle verb alongside `transition`/`purge`. AC-041 is unaffected.
    ///
    /// # Errors
    ///
    /// [`EngineError::WriteValidation`] for an empty, whitespace-only or
    /// reserved `source_id`; [`EngineError::ErasureIncomplete`] if the erasure
    /// could not be completed at rest (see [`Engine::complete_erasure_at_rest`]).
    pub fn erase_source(&self, source_id: &str) -> Result<ExciseReport, EngineError> {
        // Construct-to-validate: reuse the newtype's rule rather than restating
        // it, so the erasure boundary and the write boundary cannot drift.
        let _validated = SourceId::new(source_id)?;
        self.erase_source_shared("erase_source", source_id)
    }

    /// Phase 9 Pack B / AC-028a/b/c source excise — the **operator/recovery**
    /// spelling of [`Engine::erase_source`], sharing one engine path with it.
    ///
    /// Kept `operator`-gated and kept permissive about reserved ids: this is
    /// the seam an operator uses to excise `_legacy:pre-0.8.20` (R-20-E8) or
    /// `_engine:*` substrate, which the governed SDK verb refuses.
    #[cfg(feature = "operator")]
    pub fn excise_source(&self, source_id: &str) -> Result<ExciseReport, EngineError> {
        if source_id.is_empty() {
            self.ensure_open()?;
            return Err(EngineError::WriteValidation);
        }
        self.erase_source_shared("excise_source", source_id)
    }

    /// The single erasure implementation behind [`Engine::erase_source`] and
    /// [`Engine::excise_source`]. `verb` names the caller for the telemetry
    /// redaction record only; the deletion semantics are identical.
    ///
    /// Non-perturbation: rows from other sources (and rows with NULL
    /// `source_id`) are untouched; the projection cursor is NOT reset
    /// and no blanket projection rebuild is issued.
    fn erase_source_shared(
        &self,
        verb: &'static str,
        source_id: &str,
    ) -> Result<ExciseReport, EngineError> {
        self.ensure_open()?;

        // Drain MUST succeed before the excise transaction. SQLite-WAL
        // would otherwise allow a worker that already dequeued a job
        // for an excised cursor to commit its INSERT into vec0 /
        // _fathomdb_vector_rows after our DELETE releases the writer
        // lock, leaving residue and breaking AC-028b. Surface the
        // timeout instead of swallowing it (Pack A pattern).
        //
        // ORDER IS LOAD-BEARING, exactly as in `purge`: settle every pending
        // projection FIRST (UNFROZEN), and only THEN freeze the scanner and
        // confirm idle. Freezing first parks the dispatcher, so a row written
        // moments ago can never be scanned and enqueued — while `drain` ->
        // `wait_for_idle` keeps seeing it via
        // `database_has_pending_projection_work`, which reads the DATABASE and
        // not the queue. The result is that the ordinary sequence "write a
        // vector-indexed row, then erase it" stalls for the whole
        // LIFECYCLE_DRAIN_TIMEOUT_MS and fails with `Scheduler`.
        // (codex §9 [P2]; `erase_source_drains_before_freezing`.)
        self.drain(LIFECYCLE_DRAIN_TIMEOUT_MS)?;
        self.projection_runtime.set_frozen(true);
        let drain_result = self.drain(LIFECYCLE_DRAIN_TIMEOUT_MS);
        let outcome = drain_result.and_then(|()| self.excise_source_inner(verb, source_id));
        self.projection_runtime.set_frozen(false);
        // 0.8.20 Slice 5b (R-20-E5/E6) — finish the erasure AT REST before
        // reporting success: redact the erased stable ids out of the telemetry
        // sink, then truncate the `-wal` so the erased bytes are not still
        // readable on disk. On persistent checkpoint BUSY this returns
        // `ErasureIncomplete` rather than an `ExciseReport`.
        let report = outcome?;
        self.complete_erasure_at_rest(verb)?;
        Ok(report)
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

    /// 0.8.20 Slice 5d (R-20-E8, design §4 item 11) — doctor
    /// `orphan-provenance` seam: a **read-only** per-`source_id` census over
    /// `canonical_nodes` + `canonical_edges`.
    ///
    /// Answers the operator question the erasure work made askable: *"for this
    /// database, is every row actually reachable by some erasure verb?"* A row
    /// is reachable by `erase_source` / `excise_source` via `source_id`, or —
    /// **if it is a NODE** — by `purge` via `logical_id`. A row with neither is
    /// un-erasable, and is counted into
    /// [`OrphanProvenanceReport::unerasable_rows`].
    ///
    /// The node/edge asymmetry is load-bearing and mirrors migration step 21:
    /// an EDGE's `logical_id` is a supersession identity only and confers no
    /// purge-addressability, so a NULL-`source_id` edge is un-erasable however
    /// governed it looks. See the query comment below.
    ///
    /// CLI-only (no SDK parity), matching the `dump-*` diagnostic family.
    ///
    /// Read-only by construction: this method issues SELECTs exclusively and
    /// opens no transaction.
    #[cfg(feature = "operator")]
    pub fn orphan_provenance(&self) -> Result<OrphanProvenanceReport, EngineError> {
        self.ensure_open()?;
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;

        // One UNION ALL over both canonical tables so a source that spans nodes
        // AND edges reports as a single bucket.
        //
        // TWO DIFFERENT SUMS, and the difference is the whole point:
        //
        // * `governed` counts `logical_id` carriers — a reporting figure;
        // * `purge_addressable` counts rows that `purge` can actually reach,
        //   and it is NODE-ONLY (the edge arm contributes a literal 0).
        //
        // This is the same node/edge asymmetry migration step 21 carries, for
        // the same reason, and the two must stay in step: `purge_inner`
        // resolves its target exclusively through `canonical_nodes` (`SELECT
        // state FROM canonical_nodes WHERE logical_id = ?1`) and then erases
        // edges by ENDPOINT (`from_id`/`to_id`). It NEVER resolves an edge by
        // edge `logical_id` — an edge `logical_id` is only a SUPERSESSION
        // identity and confers no purge-addressability whatsoever.
        //
        // Crediting an edge's `logical_id` here made the diagnostic subtract
        // exactly the rows it exists to find: a NULL-`source_id` edge is
        // reachable by no erasure verb at all, yet `orphan-provenance` would
        // exit CLEAN on precisely the legacy/corrupt shape step 21 closes.
        // False assurance from a governance verb is worse than no verb.
        // (codex §9 [P2]; `null_source_governed_edge_counts_as_unerasable`.)
        let mut stmt = connection
            .prepare(
                "SELECT source_id,
                        COUNT(*) AS rows_total,
                        SUM(CASE WHEN logical_id IS NOT NULL THEN 1 ELSE 0 END) AS governed,
                        SUM(purge_addressable) AS purge_addressable
                   FROM (SELECT source_id,
                                logical_id,
                                CASE WHEN logical_id IS NOT NULL THEN 1 ELSE 0 END
                                    AS purge_addressable
                           FROM canonical_nodes
                         UNION ALL
                         SELECT source_id, logical_id, 0 AS purge_addressable
                           FROM canonical_edges)
                  GROUP BY source_id
                  ORDER BY rows_total DESC, source_id",
            )
            .map_err(|_| EngineError::Storage)?;

        let rows = stmt
            .query_map([], |row| {
                let source_id: Option<String> = row.get(0)?;
                let rows: i64 = row.get(1)?;
                let governed: i64 = row.get(2)?;
                let purge_addressable: i64 = row.get(3)?;
                Ok((source_id, rows, governed, purge_addressable))
            })
            .map_err(|_| EngineError::Storage)?;

        let mut sources = Vec::new();
        let mut total_rows: u64 = 0;
        let mut unerasable_rows: u64 = 0;
        for row in rows {
            let (source_id, rows, governed, purge_addressable) =
                row.map_err(|_| EngineError::Storage)?;
            let rows = u64::try_from(rows).unwrap_or(0);
            let governed_rows = u64::try_from(governed).unwrap_or(0);
            let purge_addressable = u64::try_from(purge_addressable).unwrap_or(0);
            total_rows = total_rows.saturating_add(rows);
            if source_id.is_none() {
                // No provenance: only the PURGE-ADDRESSABLE subset (governed
                // NODES) is reachable. The remainder — including every governed
                // EDGE, whose `logical_id` reaches nothing — is reachable by no
                // erasure verb at all.
                unerasable_rows =
                    unerasable_rows.saturating_add(rows - purge_addressable.min(rows));
            }
            let reserved = source_id.as_deref().is_some_and(|s| s.starts_with('_'));
            sources.push(OrphanProvenanceSource { source_id, rows, governed_rows, reserved });
        }

        Ok(OrphanProvenanceReport { sources, total_rows, unerasable_rows })
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
        // The operator verb keeps SQLite's own busy handler: `recover
        // --truncate-wal` is an explicit, foreground operator act, so waiting out
        // a transient reader is the helpful behaviour.
        self.wal_checkpoint_truncate_once(true)
    }

    /// One `PRAGMA wal_checkpoint(TRUNCATE)` on the writer connection.
    ///
    /// NOT operator-gated: the erasure verbs (`purge` is a default-feature verb)
    /// need it too, and a `#[cfg(feature = "operator")]` helper would break the
    /// default build. Acquires the connection mutex, so callers must NOT already
    /// hold it — every erasure verb calls this AFTER its transaction has
    /// committed and the guard has been dropped.
    ///
    /// `honor_busy_timeout = false` suppresses SQLite's busy handler for the
    /// duration of the checkpoint. rusqlite installs a **5 s** default
    /// `busy_timeout`, so a blocked checkpoint sits for 5 s before reporting
    /// `busy` — under the erasure verbs' bounded retry that compounds to a ~25 s
    /// stall on a verb that is supposed to fail fast. The erasure path therefore
    /// takes the immediate `busy` answer and runs its OWN short backoff; the
    /// prior value is restored before returning, on every path.
    fn wal_checkpoint_truncate_once(
        &self,
        honor_busy_timeout: bool,
    ) -> Result<TruncateWalReport, EngineError> {
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;

        let restore_timeout_ms: Option<i64> = if honor_busy_timeout {
            None
        } else {
            let previous: i64 = connection
                .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
                .map_err(|_| EngineError::Storage)?;
            connection.busy_timeout(Duration::ZERO).map_err(|_| EngineError::Storage)?;
            Some(previous)
        };

        let checkpoint: rusqlite::Result<(i64, i64, i64)> =
            connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            });

        if let Some(previous) = restore_timeout_ms {
            let previous = u64::try_from(previous.max(0)).unwrap_or(0);
            connection
                .busy_timeout(Duration::from_millis(previous))
                .map_err(|_| EngineError::Storage)?;
        }

        let (busy, log_frames, checkpointed_frames) =
            checkpoint.map_err(|_| EngineError::Storage)?;
        let status = if busy == 0 { TruncateWalStatus::Done } else { TruncateWalStatus::Busy };
        Ok(TruncateWalReport {
            status,
            busy: busy.max(0) as u32,
            log_frames: log_frames.max(0) as u32,
            checkpointed_frames: checkpointed_frames.max(0) as u32,
        })
    }

    /// 0.8.20 Slice 5b (R-20-E5) — complete an erasure **at rest** after the
    /// erasing transaction has committed. Two obligations, in order:
    ///
    /// 1. **Telemetry redaction** — drop the erased stable ids out of the opt-in
    ///    telemetry sink. Driven from the DURABLE pending queue
    ///    ([`Engine::discharge_pending_redactions`]), NOT from the ids the caller
    ///    happens to be holding, so a retry after a failed redaction still knows
    ///    what it owes.
    /// 2. **WAL truncation** — `wal_checkpoint(TRUNCATE)` with a BOUNDED retry.
    ///    `PRAGMA secure_delete=ON` zeroes pages freed inside the database file,
    ///    but the erased content also lives in the write-ahead log as committed
    ///    frames from the ORIGINAL insert; the erasure DELETE appends new frames
    ///    rather than rewriting old ones, so without a truncating checkpoint the
    ///    erased body stays `grep`-able in `<db>-wal`.
    ///
    /// A concurrent reader pinning a WAL snapshot makes the checkpoint report
    /// `busy`. After [`ERASURE_WAL_TRUNCATE_ATTEMPTS`] tries the verb raises
    /// [`EngineError::ErasureIncomplete`] — **an erasure verb must never report
    /// success on an incomplete erasure.** The retry budget is deliberately small
    /// (~100 ms total): the caller retries the verb, the verb does not block.
    fn complete_erasure_at_rest(&self, verb: &'static str) -> Result<(), EngineError> {
        // The ids are NOT passed in: they were persisted inside the erasing
        // transaction, and this drains that queue. The WAL truncation below then
        // runs AFTER the pending rows have been deleted, so the freed pages
        // holding them (zeroed by `secure_delete=ON`) are checkpointed out too.
        self.discharge_pending_redactions(verb)?;

        let mut last: Option<TruncateWalReport> = None;
        for attempt in 0..ERASURE_WAL_TRUNCATE_ATTEMPTS {
            let report = self.wal_checkpoint_truncate_once(false)?;
            if report.status == TruncateWalStatus::Done {
                return Ok(());
            }
            last = Some(report);
            if attempt + 1 < ERASURE_WAL_TRUNCATE_ATTEMPTS {
                std::thread::sleep(Duration::from_millis(ERASURE_WAL_TRUNCATE_BACKOFF_MS));
            }
        }
        let frames = last.map_or(0, |r| r.log_frames);
        Err(EngineError::ErasureIncomplete {
            stage: "wal_checkpoint".to_string(),
            detail: format!(
                "`{verb}` deleted its rows, but `wal_checkpoint(TRUNCATE)` reported BUSY on all \
                 {ERASURE_WAL_TRUNCATE_ATTEMPTS} attempts ({frames} frames still in the log) — a \
                 concurrent reader is pinning a WAL snapshot, so the erased bytes remain readable \
                 in the `-wal` file. Retry once the reader has finished."
            ),
        })
    }

    /// 0.8.20 Slice 5 fix-1 (codex §9 P2) — perform every telemetry redaction the
    /// engine still OWES, from the durable pending queue.
    ///
    /// **The defect this closes.** Redaction necessarily runs after the erasing
    /// transaction commits (the sink is a file, not a table, so it cannot join
    /// the transaction). When it failed, the verb correctly raised
    /// `ErasureIncomplete { stage: "telemetry_redaction" }` and told the operator
    /// to retry — but the retry recomputed the id set by querying the canonical
    /// tables, whose rows the FIRST call had already deleted. It therefore got an
    /// EMPTY set, hit the empty-id fast path in
    /// [`Engine::redact_telemetry_stable_ids`], and returned success while the
    /// leaked `l:`/`h:` ids were still sitting in the sink. An erasure verb
    /// reporting success on an incomplete erasure is precisely what R-20-E5
    /// forbids, and it is the worst failure mode available to this slice: silent,
    /// and indistinguishable from a real erasure.
    ///
    /// **The mechanism — an intent log.** The ids are captured BEFORE the deletes
    /// (they are derived from `logical_id`/`body`, which the deletes destroy) and
    /// written into [`ERASURE_PENDING_REDACTION_COLLECTION`] INSIDE the same
    /// transaction, so "the rows are gone" and "a redaction is owed for them"
    /// commit atomically. There is no window in which the rows are deleted and
    /// the obligation is unrecorded. A pending row is deleted only once its
    /// redaction has actually been performed, so the obligation survives process
    /// death, and the empty-id fast path is unreachable while one is outstanding:
    /// this drains the QUEUE, never the caller's id vector.
    ///
    /// The queue is drained by EVERY erasure verb, not just a retry of the one
    /// that failed — an outstanding obligation is the engine's, not one call's.
    ///
    /// **Honest refusal.** If a redaction is owed but no telemetry sink is
    /// attached to this `Engine` (only reachable if the process restarted between
    /// the failure and the retry without re-enabling telemetry), the ids really
    /// are still in the sink file and this returns `ErasureIncomplete` rather
    /// than guessing. Re-enable telemetry on the same sink and retry.
    ///
    /// **Exposure tradeoff, stated plainly.** A pending row holds the stable ids
    /// in the database for the window between the delete and the redaction. That
    /// is a strict improvement: those ids are, during exactly that window,
    /// already readable in the telemetry sink — which is the leak being closed —
    /// and the pending row is deleted the moment the sink is clean, on pages
    /// `secure_delete=ON` zeroes and the subsequent `TRUNCATE` checkpoint clears
    /// from the log.
    fn discharge_pending_redactions(&self, verb: &'static str) -> Result<(), EngineError> {
        let pending = self.load_pending_redactions()?;
        if pending.is_empty() {
            return Ok(());
        }

        let mut ids: Vec<String> =
            pending.iter().flat_map(|(_, ids)| ids.iter().cloned()).collect();
        ids.sort_unstable();
        ids.dedup();

        // A queue entry exists ⇒ a sink was attached when the rows were deleted ⇒
        // the ids are in that file. Never clear the queue without redacting.
        if !self.telemetry_enabled.load(Ordering::Acquire) {
            return Err(EngineError::ErasureIncomplete {
                stage: "telemetry_redaction".to_string(),
                detail: format!(
                    "`{verb}` has {} outstanding telemetry redaction(s) covering {} erased \
                     stable id(s), but no telemetry sink is attached to this engine — the ids \
                     cannot be removed from the sink file. Re-enable telemetry on the same sink \
                     path and retry.",
                    pending.len(),
                    ids.len()
                ),
            });
        }

        // On failure the queue rows stay put and the error propagates: the verb
        // does not report success, and the next call retries the same obligation.
        self.redact_telemetry_stable_ids(verb, &ids)?;

        let row_ids: Vec<i64> = pending.iter().map(|(row_id, _)| *row_id).collect();
        self.clear_pending_redactions(&row_ids)
    }

    /// Read the outstanding redaction queue: `(operational_mutations.id, ids)`.
    fn load_pending_redactions(&self) -> Result<Vec<(i64, Vec<String>)>, EngineError> {
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        let mut stmt = connection
            .prepare(
                "SELECT id, payload_json FROM operational_mutations \
                 WHERE collection_name = ?1 ORDER BY id",
            )
            .map_err(|_| EngineError::Storage)?;
        let rows = stmt
            .query_map([ERASURE_PENDING_REDACTION_COLLECTION], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|_| EngineError::Storage)?;
        let mut pending = Vec::new();
        for row in rows {
            let (row_id, payload) = row.map_err(|_| EngineError::Storage)?;
            // A payload we cannot parse is an obligation we cannot discharge;
            // keeping it (empty) is safe — it never unblocks a false success,
            // and `discharge_pending_redactions` still refuses.
            let ids = serde_json::from_str::<serde_json::Value>(&payload)
                .ok()
                .and_then(|v| v.get("erased_stable_ids").cloned())
                .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
                .unwrap_or_default();
            pending.push((row_id, ids));
        }
        Ok(pending)
    }

    /// Retire queue entries whose redaction has been PERFORMED. Committed before
    /// the caller's WAL truncation so the freed pages are checkpointed out.
    fn clear_pending_redactions(&self, row_ids: &[i64]) -> Result<(), EngineError> {
        if row_ids.is_empty() {
            return Ok(());
        }
        let connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_ref().ok_or(EngineError::Closing)?;
        let mut stmt = connection
            .prepare("DELETE FROM operational_mutations WHERE id = ?1")
            .map_err(|_| EngineError::Storage)?;
        for row_id in row_ids {
            stmt.execute([row_id]).map_err(|_| EngineError::Storage)?;
        }
        Ok(())
    }

    /// 0.8.20 Slice 5b (R-20-E6) — SELECTIVE redaction of erased stable ids from
    /// the opt-in telemetry sink.
    ///
    /// `capture_telemetry` persists `result_stable_ids` — `l:`/`h:` prefixed ids
    /// — into a JSONL file that outlives the erased rows, and nothing in the
    /// engine could previously remove them. A retained `l:` id is not inert:
    /// [`derive_logical_id`] is `SHA256(lowercase(kind) + ":" + lowercase(name))`,
    /// and the case-folding of BOTH inputs shrinks the preimage space, so a
    /// surviving id is dictionary-attackable back to the natural key it was
    /// derived from. An `h:` id is a plain `SHA256(body)`, confirmable against a
    /// guessed body.
    ///
    /// **This MUST NOT truncate the sink.** `sink_path` is CALLER-SUPPLIED and
    /// may hold unrelated operator eval history that the erasure obligation never
    /// covered; the v3 truncation approach was rejected as unsafe. Only the
    /// matching `result_stable_ids` ELEMENTS are replaced with
    /// [`REDACTED_STABLE_ID`], preserving record count, record order and
    /// positional alignment with the parallel `result_ids` array. Lines that are
    /// not engine-authored JSON events are copied through verbatim.
    ///
    /// **Crash safety.** The rewrite is write-temp-then-`rename`: a sibling
    /// `.redact.tmp` is written and fsynced, then atomically renamed over the
    /// sink, so a crash leaves either the old file or the new one — never a
    /// half-rewritten sink. The telemetry mutex is held across the whole rewrite,
    /// so no in-process `capture_telemetry` can append into the window; an
    /// out-of-process appender is handled by re-reading and folding in the tail
    /// delta before the rename (bounded retry).
    ///
    /// The privacy contract is unchanged: query TEXT and `source_id` are never
    /// captured (ADR-0.8.8 §C), so there is nothing else in the sink to redact.
    /// The fast-OFF atomic guard is preserved — when telemetry was never enabled
    /// this is a single relaxed-ordering load and no mutex acquisition.
    fn redact_telemetry_stable_ids(
        &self,
        verb: &'static str,
        erased_stable_ids: &[String],
    ) -> Result<(), EngineError> {
        // Fast OFF path — mirrors `capture_telemetry`. No mutex, no I/O.
        if erased_stable_ids.is_empty() || !self.telemetry_enabled.load(Ordering::Acquire) {
            return Ok(());
        }
        let guard = self.telemetry.lock().map_err(|_| EngineError::Storage)?;
        let Some(sink) = guard.as_ref() else { return Ok(()) };
        let erased: std::collections::HashSet<&str> =
            erased_stable_ids.iter().map(String::as_str).collect();

        match redact_jsonl_stable_ids(&sink.path, &erased) {
            Ok(()) => Ok(()),
            // 0.8.20 Slice 5 fix-3 (codex §9 round-3 P2) — `NotFound` is NOT a
            // discharge. It previously returned `Ok(())` ("the sink is gone,
            // nothing to redact"), which cleared the durable pending queue and
            // let the verb report success. That inference does not hold: a path
            // cannot distinguish `rm` from `mv`, and log rotation of a
            // caller-supplied sink is an ordinary operational event that leaves
            // the erased `l:`/`h:` ids fully readable under the rotated name.
            //
            // The burden of proof is on DISCHARGING the obligation, and the
            // engine cannot meet it here: `TelemetrySink` holds a PATH, not an
            // open handle, so there is no `nlink == 0` witness that the inode was
            // actually unlinked — and even that would not cover a copy taken
            // before the deletion. So there is no narrow provable case to carve
            // out, and `NotFound` fails closed.
            //
            // This cannot fire spuriously for a sink that never existed:
            // `enable_telemetry` CREATES the file before arming capture, so for
            // any engine with telemetry enabled the sink demonstrably existed and
            // `NotFound` means it existed and then vanished.
            Err(err) => Err(EngineError::ErasureIncomplete {
                stage: "telemetry_redaction".to_string(),
                detail: if err.kind() == std::io::ErrorKind::NotFound {
                    format!(
                        "`{verb}` deleted its rows, but the telemetry sink {} no longer exists, \
                         so the erased stable ids could not be redacted from it. A missing path \
                         does NOT prove the sink was deleted — if it was rotated or moved aside, \
                         the erased ids are still readable under its new name. The pending \
                         redaction is durable: restore the sink at this path and retry (if the \
                         sink really was destroyed, an empty file at this path discharges the \
                         obligation).",
                        sink.path.display()
                    )
                } else {
                    format!(
                        "`{verb}` deleted its rows, but the erased stable ids could not be \
                         redacted from the telemetry sink {}: {err}",
                        sink.path.display()
                    )
                },
            }),
        }
    }

    /// The erased rows' prefixed stable ids ([`IdSpace::to_prefixed`]) are NOT
    /// returned to the caller for redaction (R-20-E6). They are enqueued INSIDE
    /// this transaction via [`enqueue_pending_redaction`]: a caller-held vector
    /// is lost on the retry path, which is exactly the false-success codex §9 P2
    /// found. Only the report comes back.
    ///
    /// 0.8.20 Slice 5d (R-20-E4): no longer `operator`-gated — it is the shared
    /// body behind BOTH `erase_source` (governed SDK) and `excise_source`
    /// (operator seam). Still private; the gate that matters is on the two
    /// public spellings.
    fn excise_source_inner(
        &self,
        verb: &'static str,
        source_id: &str,
    ) -> Result<ExciseReport, EngineError> {
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

        // 0.8.20 Slice 5b (R-20-E6) — stable ids the telemetry sink may hold for
        // these rows, collected BEFORE the DELETEs.
        let erased_stable_ids = collect_erased_stable_ids(
            &tx,
            "SELECT logical_id, body FROM canonical_nodes WHERE source_id = ?1",
            "SELECT logical_id, body FROM canonical_edges WHERE source_id = ?1",
            source_id,
        )?;

        // 0.8.20 Slice 5a (R-20-E1) — registry-driven erasure. The previous
        // hand-rolled list here OMITTED `search_index_v2`, a CONTENT-STORING
        // FTS5 table (no `content=''`) that keeps the document body verbatim:
        // after `excise_source` the erased body was still on disk, invisible to
        // every functional test because both v2 read paths discard candidates
        // lacking a live `canonical_nodes` row. `erase_row_projections` covers
        // every registered projection, so the omission cannot recur.
        let mut shadow_invalidated: u64 = 0;
        for cursor in node_cursors.iter().chain(edge_cursors.iter()) {
            shadow_invalidated = shadow_invalidated.saturating_add(
                erase_row_projections(&tx, *cursor).map_err(|_| EngineError::Storage)?,
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
        //
        // DURABILITY (0.8.20 Slice 5b, design v5 §2 defect D-A; HITL-ruled
        // 2026-07-19: *"there must be an auditable record of deletion event."*).
        // This row lands in `operational_mutations`, the same table the retention
        // sweep drains — and it is written BEFORE the workload that follows it,
        // so an oldest-`id`-first sweep evicted it FIRST. It is now protected:
        // `excise_source_audit` is in `ERASURE_AUDIT_COLLECTIONS`, which
        // `enforce_provenance_retention` excludes. The proof of erasure is no
        // longer destructible by ordinary retention pressure.
        //
        // NON-PII `source_id` (rationale corrected in this slice). v4 §3.6
        // justified the "`source_id` must not be PII" rule by claiming the audit
        // row retains it *permanently, by design*. That premise was FALSE — the
        // row was sweepable. The rule stands on a different and simpler footing:
        // this row persists the caller's raw `source_id` verbatim, and an
        // `excise_source` that erased the payload while keeping an identifying
        // source label would not be an erasure. The exemption above makes the
        // retention now genuinely indefinite, which makes the rule MORE
        // load-bearing, not less.
        //
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

        // 0.8.20 Slice 5 fix-1 (codex §9 P2) — durably record the redaction this
        // erasure owes, atomically with the deletes. Shares `audit_cursor`: one
        // erasure event, and this row is retired as soon as the sink is clean.
        if self.telemetry_enabled.load(Ordering::Acquire) {
            enqueue_pending_redaction(&tx, verb, &erased_stable_ids, audit_cursor)?;
        }

        tx.commit().map_err(|_| EngineError::Storage)?;
        self.next_cursor.store(audit_cursor, Ordering::SeqCst);
        Ok(ExciseReport {
            source_ref: source_id.to_string(),
            nodes_excised,
            edges_excised,
            projections_invalidated: shadow_invalidated,
        })
    }

    /// 0.8.20 Slice 5b (R-20-E7) — erase ONE op-store record, by collection and
    /// record key, from both op-store shapes: every `operational_mutations`
    /// version of the key (append-only-log collections) and its
    /// `operational_state` row (latest-state collections).
    ///
    /// **Refuses the engine's own erasure bookkeeping** (0.8.20 Slice 5 fix-3,
    /// codex §9 round-3 P1): a `collection` for which
    /// [`is_erasure_bookkeeping_collection`] holds raises
    /// [`EngineError::InvalidArgument`] before anything is deleted. Aimed at the
    /// pending-redaction queue this verb otherwise destroys an outstanding
    /// erasure obligation, after which the next erasure verb reports success with
    /// the erased ids still in the telemetry sink; aimed at the audit trail it
    /// destroys the auditable record of the deletion event.
    ///
    /// Before this slice the op-store had NO record-level delete at all:
    /// [`enforce_provenance_retention`] is a cap sweep, not an erasure verb, so a
    /// caller holding an erasure obligation over an op-store record had no way to
    /// discharge it. Idempotent — erasing an absent key is a zero-count success.
    ///
    /// Like the other erasure verbs this finishes at rest (telemetry is not
    /// involved — op-store record keys never reach the telemetry sink — but the
    /// `-wal` is), so it can return [`EngineError::ErasureIncomplete`].
    ///
    /// AUDIT (D-A). Appends a row to the retention-exempt `excise_record_audit`
    /// collection. Unlike `source_id`, a `record_key` carries NO non-PII rule:
    /// it is arbitrary caller-supplied text and may itself be the identifier
    /// being erased. The audit therefore records a SHA-256 digest of
    /// `collection` + `record_key`, never the key — enough to prove *that* a
    /// specific record was erased to anyone who already knows the key, and
    /// useless to anyone who does not.
    #[cfg(feature = "operator")]
    pub fn excise_collection_record(
        &self,
        collection: &str,
        record_key: &str,
    ) -> Result<ExciseRecordReport, EngineError> {
        self.ensure_open()?;
        if collection.is_empty() || record_key.is_empty() {
            return Err(EngineError::WriteValidation);
        }
        // 0.8.20 Slice 5 fix-3 (codex §9 round-3 P1) — the engine's own erasure
        // bookkeeping is not caller data and is not excisable. See
        // `is_erasure_bookkeeping_collection` for why each member is protected.
        // Checked BEFORE any deletion so the refusal is total, not partial.
        if is_erasure_bookkeeping_collection(collection) {
            return Err(EngineError::InvalidArgument {
                msg: format!(
                    "`{collection}` is engine-internal erasure bookkeeping and cannot be excised \
                     by `excise_collection_record`. The pending-redaction queue records an \
                     erasure the engine still owes (deleting it would let a later verb report \
                     success on an incomplete erasure, R-20-E5), and the erasure-audit \
                     collections are the auditable record of the deletion event. Pending \
                     redactions retire themselves once performed; retry the erasure verb instead."
                ),
            });
        }
        let report = self.excise_collection_record_inner(collection, record_key)?;
        self.complete_erasure_at_rest("excise_collection_record")?;
        Ok(report)
    }

    #[cfg(feature = "operator")]
    fn excise_collection_record_inner(
        &self,
        collection: &str,
        record_key: &str,
    ) -> Result<ExciseRecordReport, EngineError> {
        let mut connection = self.connection.lock().map_err(|_| EngineError::Storage)?;
        let connection = connection.as_mut().ok_or(EngineError::Closing)?;
        let tx = connection.transaction().map_err(|_| EngineError::Storage)?;

        let records_excised = tx
            .execute(
                "DELETE FROM operational_mutations
                 WHERE collection_name = ?1 AND record_key = ?2",
                params![collection, record_key],
            )
            .map_err(|_| EngineError::Storage)? as u64;
        let state_rows_excised = tx
            .execute(
                "DELETE FROM operational_state
                 WHERE collection_name = ?1 AND record_key = ?2",
                params![collection, record_key],
            )
            .map_err(|_| EngineError::Storage)? as u64;

        let record_digest = digest_record_identity(collection, record_key);
        let excised_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let payload = serde_json::json!({
            "collection": collection,
            "record_digest": record_digest,
            "excised_at": excised_at,
            "records_excised": records_excised,
            "state_rows_excised": state_rows_excised,
        })
        .to_string();
        let audit_cursor = self.next_cursor.load(Ordering::SeqCst).saturating_add(1);
        tx.execute(
            "INSERT INTO operational_mutations(
                collection_name, record_key, op_kind, payload_json, schema_id, write_cursor
             ) VALUES('excise_record_audit', ?1, 'append', ?2, NULL, ?3)",
            params![record_digest, payload, audit_cursor],
        )
        .map_err(|_| EngineError::Storage)?;

        tx.commit().map_err(|_| EngineError::Storage)?;
        self.next_cursor.store(audit_cursor, Ordering::SeqCst);
        self.counters.record_admin();
        Ok(ExciseRecordReport {
            collection: collection.to_string(),
            record_digest,
            records_excised,
            state_rows_excised,
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
        // 0.8.20 Slice 5a (R-20-E1) — registry-driven invalidation. A full
        // rebuild truncates EVERY row-owned projection (the previous hand-rolled
        // list omitted `search_index_v2`, so a rebuild neither dropped stale v2
        // rows nor repopulated the table); a vec0-only rebuild truncates the
        // vector + readiness classes exactly as before. Kind-owned watermark
        // state (`_fathomdb_projection_state`) is deliberately NOT truncated —
        // readiness is reset by rewinding the projection cursor below.
        let rows_invalidated = if include_fts {
            truncate_all_row_projections(&tx).map_err(|_| EngineError::Storage)?
        } else {
            truncate_row_projections_in(&tx, &[ProjectionClass::Vector, ProjectionClass::Readiness])
                .map_err(|_| EngineError::Storage)?
        };
        store_projection_cursor(&tx, 0).map_err(|_| EngineError::Storage)?;
        // 0.8.20 Slice 5a (R-20-E1, work item 1) — the replay runs through the
        // SAME two projectors the write path uses, so the rebuilt projections
        // are identical to what a re-write would have produced. `include_fts`
        // selects the pass: a vec0-only rebuild must not write FTS rows.
        let pass = if include_fts { ProjectionPass::Write } else { ProjectionPass::VectorOnly };
        let mut rows_rebuilt: u64 = 0;
        for row in canonical_node_rows(&tx).map_err(|_| EngineError::Storage)? {
            project_canonical_node_row(
                &tx,
                row.cursor,
                &row.kind,
                &row.body,
                row.row_kind,
                pass,
                // fix-2 [P2]: the attribute half of the replay tracks the backfill's
                // active-and-non-superseded row set; FTS / vector shadows still
                // rebuild for every row (read-side lifecycle filter, unchanged).
                row.attr_projected,
            )
            .map_err(|_| EngineError::Storage)?;
            if include_fts {
                rows_rebuilt = rows_rebuilt.saturating_add(1);
            }
        }
        // fix-26 [P2]: rebuild the edge shadows from active canonical_edges
        // (G11 search_index_edges).
        // 0.8.12 Slice A (R-CON-2 named default-ON blocker; Slice-20 codex
        // §9 [P2]): mirror the graph-traversal recency filter
        // (`edge_validity_sql`) here too, so a full rebuild does not re-surface
        // an edge that recency consolidation already invalidated.
        // 0.8.20 Slice 5a: body-less structural edges are now included in the
        // replay. They project no FTS/vector row, but the write path DOES record
        // their readiness terminal — which this rebuild truncated and (before
        // this slice) never restored, stalling `advance_projection_cursor`.
        // TC-33: the filter is generated by `edge_validity_sql` and `:now` is
        // bound (?1) rather than inlined as `datetime('now')`.
        let edge_rows: Vec<(i64, String, Option<String>)> = {
            let edge_sql = format!(
                "SELECT write_cursor, kind, body FROM canonical_edges \
                 WHERE superseded_at IS NULL{}",
                edge_validity_sql("canonical_edges", 1)
            );
            let mut edge_stmt = tx.prepare(&edge_sql).map_err(|_| EngineError::Storage)?;
            let rows = edge_stmt
                .query_map(params![current_epoch_seconds()], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .map_err(|_| EngineError::Storage)?
                .collect::<rusqlite::Result<_>>()
                .map_err(|_| EngineError::Storage)?;
            rows
        };
        for (cursor, kind, body) in edge_rows {
            let has_body = body.is_some();
            project_canonical_edge_row(&tx, cursor as u64, &kind, body.as_deref(), pass)
                .map_err(|_| EngineError::Storage)?;
            if include_fts && has_body {
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

/// 0.8.8 Slice 15 — the lowercase wire string for a retrieval arm (telemetry +
/// the same spelling `SearchHit.branch` crosses every binding).
fn branch_str(branch: SoftFallbackBranch) -> &'static str {
    match branch {
        SoftFallbackBranch::Vector => "vector",
        SoftFallbackBranch::Text => "text",
        SoftFallbackBranch::TextEdge => "text_edge",
        SoftFallbackBranch::GraphArm => "graph_arm",
    }
}

/// 0.8.8 Slice 15 — append one JSON value as a line to the telemetry sink
/// (append-only, local file; no network). Best-effort caller handles the error.
fn append_jsonl(path: &Path, value: &serde_json::Value) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{value}")?;
    Ok(())
}

/// 0.8.20 Slice 5b (R-20-E6) — rewrite a telemetry JSONL sink with every
/// `result_stable_ids` element in `erased` replaced by [`REDACTED_STABLE_ID`].
///
/// **Selective, never truncating.** Every line is carried across: records that
/// reference no erased id are byte-identical, records that do keep their shape
/// and only lose the matching id VALUES, and a line that is not an
/// engine-authored JSON event (an operator note, a hand-appended record) is
/// copied through verbatim. The sink is a caller-supplied path that may hold
/// unrelated eval history; destroying it is not part of any erasure obligation.
///
/// **Crash safety.** Write-temp-then-`rename`: the redacted content goes to a
/// sibling `<sink>.redact.tmp`, is `sync_all`ed, and is then atomically renamed
/// over the sink. A crash at any point leaves either the intact old file or the
/// complete new one — never a half-rewritten sink. `rename` is atomic because
/// the temp file is a sibling (same directory ⇒ same filesystem).
///
/// **Concurrent appends.** The caller holds the telemetry mutex, so no
/// in-process `capture_telemetry` can append into the window. An out-of-process
/// appender is still possible (the sink is just a file), so before renaming we
/// re-check the source length: if it grew, the tail delta is read, redacted and
/// appended, and the check repeats — bounded, so a pathologically hot external
/// writer surfaces as an error rather than an unbounded loop.
fn redact_jsonl_stable_ids(
    path: &Path,
    erased: &std::collections::HashSet<&str>,
) -> std::io::Result<()> {
    /// Bound on the re-check loop for an out-of-process appender.
    const MAX_TAIL_FOLDS: usize = 8;

    let mut source = std::fs::read(path)?;
    let mut redacted = redact_jsonl_bytes(&source, erased);

    for _ in 0..MAX_TAIL_FOLDS {
        let current = std::fs::read(path)?;
        if current.len() == source.len() {
            let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
            tmp_name.push(".redact.tmp");
            let tmp = path.with_file_name(tmp_name);
            {
                let mut file = std::fs::File::create(&tmp)?;
                file.write_all(&redacted)?;
                file.sync_all()?;
            }
            std::fs::rename(&tmp, path)?;
            return Ok(());
        }
        // Someone appended while we were building the replacement: fold the
        // delta in (redacted) rather than dropping it, then re-check.
        if current.len() > source.len() && current.starts_with(&source) {
            redacted.extend_from_slice(&redact_jsonl_bytes(&current[source.len()..], erased));
        } else {
            // The file was rewritten under us, not appended to. Start over.
            redacted = redact_jsonl_bytes(&current, erased);
        }
        source = current;
    }
    Err(std::io::Error::other(format!(
        "telemetry sink {} is being appended to faster than it can be redacted",
        path.display()
    )))
}

/// Line-wise redaction of a JSONL byte buffer. Non-JSON and non-event lines are
/// passed through unchanged, as is a trailing partial line (no terminating
/// newline) — the sink is append-only, so a partial tail is a torn write, not
/// ours to normalize.
fn redact_jsonl_bytes(bytes: &[u8], erased: &std::collections::HashSet<&str>) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut rest = bytes;
    while !rest.is_empty() {
        let (line, tail) = match rest.iter().position(|b| *b == b'\n') {
            Some(idx) => (&rest[..idx], &rest[idx + 1..]),
            // No trailing newline: a torn/partial final line. Pass it through.
            None => {
                out.extend_from_slice(rest);
                break;
            }
        };
        match redact_jsonl_line(line, erased) {
            Some(replacement) => out.extend_from_slice(replacement.as_bytes()),
            None => out.extend_from_slice(line),
        }
        out.push(b'\n');
        rest = tail;
    }
    out
}

/// `Some(replacement)` when the line is an engine-authored telemetry event whose
/// `result_stable_ids` referenced an erased id; `None` to pass it through.
fn redact_jsonl_line(line: &[u8], erased: &std::collections::HashSet<&str>) -> Option<String> {
    let text = std::str::from_utf8(line).ok()?;
    let mut value: serde_json::Value = serde_json::from_str(text).ok()?;
    let ids = value.get_mut("result_stable_ids")?.as_array_mut()?;
    let mut touched = false;
    for id in ids.iter_mut() {
        if id.as_str().is_some_and(|s| erased.contains(s)) {
            *id = serde_json::Value::from(REDACTED_STABLE_ID);
            touched = true;
        }
    }
    touched.then(|| value.to_string())
}

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
    let min_id = hits.iter().map(|h| h.write_cursor).min().unwrap_or(0);
    let max_id = hits.iter().map(|h| h.write_cursor).max().unwrap_or(0);
    if max_id == min_id {
        return hits;
    }
    let span = (max_id - min_id) as f64;
    let mut reweighted: Vec<SearchHit> = hits
        .into_iter()
        .map(|mut h| {
            let norm = (h.write_cursor - min_id) as f64 / span;
            h.score += RECENCY_WEIGHT * norm;
            h
        })
        .collect();
    // Stable sort preserves the fused order on exact ties.
    reweighted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    reweighted
}

/// 0.8.16 Slice 5 / F9 — OFF-by-default importance/confidence reweight, applied
/// to the fused hits AFTER bit-KNN + RRF (mirrors [`apply_recency_reweight`]).
///
/// Multiplicative-on-fused (ADR-0.8.16 §2.2, HITL-SIGNED 2026-07-08): a hit's
/// score is scaled by node `importance` (`importance_by_id`) × edge `confidence`
/// (`confidence_by_id`), each keyed by the hit's interim id (`write_cursor`). A
/// missing key = `NULL` = never assigned = graceful-absent ⇒ neutral (1.0), the
/// OPP-12 Q6a graceful-absent state. Node hits carry `importance`; graph/edge hits
/// carry `confidence`; the two id-spaces never collide (cursors are globally
/// unique), so each hit gets exactly one non-neutral factor.
///
/// **R-F9-4 graceful-neutral identity:** when `enabled` but *no* hit has a
/// non-neutral factor (every importance/confidence absent), the input is returned
/// **unchanged** — byte-identical to the `enabled == false` result (no re-sort),
/// so declaring the mechanism never perturbs an all-absent corpus.
#[must_use]
pub fn apply_importance_reweight(
    hits: Vec<SearchHit>,
    importance_by_id: &HashMap<u64, f64>,
    confidence_by_id: &HashMap<u64, f64>,
    enabled: bool,
) -> Vec<SearchHit> {
    if !enabled {
        return hits;
    }
    // Graceful-neutral fast path (R-F9-4): if nothing is weighted, do not touch
    // order or scores — identical to the reweight-OFF result.
    let any_weighted = hits.iter().any(|h| {
        importance_by_id.contains_key(&h.write_cursor)
            || confidence_by_id.contains_key(&h.write_cursor)
    });
    if !any_weighted {
        return hits;
    }
    let mut reweighted: Vec<SearchHit> = hits
        .into_iter()
        .map(|mut h| {
            let importance = importance_by_id.get(&h.write_cursor).copied().unwrap_or(1.0);
            let confidence = confidence_by_id.get(&h.write_cursor).copied().unwrap_or(1.0);
            h.score *= importance * confidence;
            h
        })
        .collect();
    // Stable sort preserves the fused order on exact ties.
    reweighted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    reweighted
}

/// 0.8.16 Slice 5 / F9 — build the per-hit importance/confidence weight maps for
/// the candidate `hits` from the durable columns (`canonical_nodes.importance`,
/// `canonical_edges.confidence`). Only NON-NULL values are inserted; an absent
/// value stays out of the map (graceful-absent ⇒ neutral in
/// [`apply_importance_reweight`]). Prepared statements are guarded so a pre-step-18
/// / pre-step-14 schema (no column) yields empty maps rather than an error.
fn build_importance_confidence_maps(
    tx: &rusqlite::Connection,
    hits: &[SearchHit],
) -> rusqlite::Result<(HashMap<u64, f64>, HashMap<u64, f64>)> {
    let mut importance_by_id: HashMap<u64, f64> = HashMap::new();
    let mut confidence_by_id: HashMap<u64, f64> = HashMap::new();
    if let Ok(mut stmt) =
        tx.prepare("SELECT importance FROM canonical_nodes WHERE write_cursor = ?1 LIMIT 1")
    {
        for h in hits {
            if let Ok(Some(v)) = stmt.query_row([h.write_cursor], |r| r.get::<_, Option<f64>>(0)) {
                importance_by_id.insert(h.write_cursor, v);
            }
        }
    }
    if let Ok(mut stmt) = tx.prepare(
        "SELECT confidence FROM canonical_edges \
         WHERE write_cursor = ?1 AND superseded_at IS NULL LIMIT 1",
    ) {
        for h in hits {
            if let Ok(Some(v)) = stmt.query_row([h.write_cursor], |r| r.get::<_, Option<f64>>(0)) {
                confidence_by_id.insert(h.write_cursor, v);
            }
        }
    }
    Ok((importance_by_id, confidence_by_id))
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
/// where both CE and RRF scores are normalized to [0,1] over the reranked pool.
///
/// 0.8.5 (EXP-0): `alpha` (clamped to `[0,1]`) and `pool_n` (the reranked-pool
/// size, clamped to `hits.len()`) are caller-supplied. The defaults
/// `alpha = 0.3, pool_n = rerank_depth` reproduce the pre-slice blend exactly.
/// `rerank_depth == 0` remains the identity gate regardless of `pool_n`.
///
/// This is the rerank hook, **not** the dropped `fusion_mode` knob.
#[doc(hidden)]
#[must_use]
pub fn rerank_fused(
    _query: &str,
    hits: Vec<SearchHit>,
    rerank_depth: usize,
    alpha: f64,
    pool_n: usize,
) -> Vec<SearchHit> {
    // Soft-fallback: depth=0 → identity (byte-identical to old stub). NOTE this
    // early gate is independent of `pool_n`: `rerank_depth == 0, pool_n = 10`
    // does NOT rerank (0.8.5 D4).
    if rerank_depth == 0 {
        return hits;
    }

    // Feature-gated CE inference. In the default build (no feature) this block
    // compiles away and `hits` is returned unchanged regardless of `rerank_depth`.
    // FIX-1: pass `&hits` (borrow) so `hits` remains owned for the soft-fallback path.
    #[cfg(feature = "default-reranker")]
    {
        if let Some(reranked) = ce_rerank(_query, &hits, rerank_depth, alpha, pool_n) {
            return reranked;
        }
    }

    // 0.8.5: the bindings/default callers pass `alpha = 0.3, pool_n = rerank_depth`;
    // referenced here so the no-feature build does not warn on unused params.
    #[cfg(not(feature = "default-reranker"))]
    let _ = (alpha, pool_n);

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
/// through [`rerank_fused`], and projects back to `(id, score, ce_score)` in the reranked
/// order.
///
/// Contract (inherited verbatim from `rerank_fused`): `rerank_depth == 0` OR an
/// empty list returns the input order WITH the input scores, byte-identical — no
/// model load, no network. With `--features default-reranker` and
/// `rerank_depth > 0` the CE blends the top-`depth` and may reorder; with the
/// feature off the CE path compiles away and this is always identity.
///
/// 0.8.2 Slice E2 fix-1 [P2]: returns `Err` when any passage carries a non-finite
/// score (NaN / ±inf), mirroring the malformed-passage loud-fail contract.
/// Callers (pyo3 `rerank` binding, tests) must handle `Result`.
/// (`#[must_use]` removed: `Result` is already `#[must_use]`.)
pub fn rerank_passages(
    query: &str,
    passages: Vec<(u64, String, f64)>,
    rerank_depth: usize,
    alpha: f64,
    pool_n: usize,
) -> Result<Vec<(u64, f64, Option<f64>)>, String> {
    // [P2] guard: reject non-finite scores before they reach normalization/sort.
    // A NaN or ±inf score would produce NaN blended scores and an unstable sort
    // order — surface the error early as the typed WriteValidationError at the
    // pyo3 boundary (mirroring the malformed-passage loud-fail contract).
    for (id, _, score) in &passages {
        if !score.is_finite() {
            return Err(format!(
                "rerank: non-finite score for passage id={id}: {score} \
                 (NaN/\u{00b1}inf must not reach the normalization/sort step)"
            ));
        }
    }
    let hits: Vec<SearchHit> = passages
        .into_iter()
        .map(|(id, body, score)| SearchHit {
            // C-2: synthetic passages carry no canonical identity — mint the
            // `Passage` (`p:`) id from the caller-supplied ordinal. The ordinal
            // is ALSO kept as the engine-internal positional cursor so the
            // projection below returns it byte-unchanged.
            id: IdSpace::passage(id.to_string()),
            write_cursor: id,
            kind: "passage".to_string(),
            body,
            score,
            branch: SoftFallbackBranch::Vector,
            source_id: None,
            ce_score: None,
        })
        .collect();
    // 0.8.5 — project `(id, score, ce_score)` so the binding can surface the CE
    // score per candidate; `ce_score` is `None` for the identity / out-of-pool path.
    // The projected id is the caller's ordinal (the engine-internal `write_cursor`).
    Ok(rerank_fused(query, hits, rerank_depth, alpha, pool_n)
        .into_iter()
        .map(|h| (h.write_cursor, h.score, h.ce_score))
        .collect())
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
    _rerank_depth: usize, // 0.8.5: pool sizing moved to `pool_n`; depth gate stays in `rerank_fused`.
    alpha: f64,
    pool_n: usize,
) -> Option<Vec<SearchHit>> {
    // 0.8.5 (D3) — clamp α to [0,1] silently here so EVERY path (engine search,
    // `rerank_passages`, the bindings) is covered by one clamp, matching the
    // existing `pool_n.min(len)` clamp idiom.
    // codex §9 P2-1: `f64::clamp(NaN)` returns NaN (clamp does NOT map NaN into
    // range) — a non-finite α would then make every blended score NaN and destroy
    // the ranking. The high-level SDKs reject non-finite α, but the low-level
    // `rerank()` / direct-Rust callers don't, so fall back to the documented
    // default α=0.3 here for any non-finite input.
    let alpha = if alpha.is_finite() { alpha.clamp(0.0, 1.0) } else { 0.3 };
    // fix-1 [P2]: short-circuit before touching the singleton when there is
    // nothing to rerank — avoids loading/downloading the ~17 MB model for an
    // empty result set and prevents memoizing a transient load failure.
    if hits.is_empty() {
        return Some(vec![]);
    }

    // Try to get the loaded model. Returns None when weights are absent.
    let model = CandleCrossEncoder::try_get_loaded()?;

    // 0.8.5 (D4) — the reranked pool is the top `pool_n` (caller resolves the
    // `unwrap_or(rerank_depth)` default at the binding), clamped to the hit count.
    let n = pool_n.min(hits.len());
    let top = &hits[..n]; // no split_at_mut needed; borrow slices directly
    let rest = &hits[n..];

    // --- RRF min-max normalization over the top-N pool ---
    let rrf_min = top.iter().map(|h| h.score).fold(f64::INFINITY, f64::min);
    let rrf_max = top.iter().map(|h| h.score).fold(f64::NEG_INFINITY, f64::max);
    let rrf_span = rrf_max - rrf_min;

    // Batched CE scoring: ONE forward over the whole top-N pool instead of N
    // per-pair forwards. The ranking math below (RRF min-max norm, sigmoid,
    // ALPHA blend, sort) is byte-unchanged — only the scoring is batched.
    let bodies: Vec<&str> = top.iter().map(|h| h.body.as_str()).collect();
    let raw_logits = model.score_batch(_query, &bodies);

    let mut scored: Vec<(f64, SearchHit)> = top
        .iter()
        .zip(raw_logits)
        .map(|(h, raw_logit)| {
            let rrf_norm = if rrf_span > 0.0 { (h.score - rrf_min) / rrf_span } else { 1.0 };
            // Sigmoid for CE normalization: 1/(1+exp(-x)).
            let ce_norm = 1.0 / (1.0 + (-raw_logit).exp());
            // 0.8.5 — α is the caller-supplied (clamped) blend weight; default 0.3
            // reproduces the pre-slice `const ALPHA = 0.3` blend exactly.
            let blended = alpha * ce_norm + (1.0 - alpha) * rrf_norm;
            // 0.8.5 (D1) — expose the per-candidate CE score on in-pool hits.
            let mut hit = h.clone();
            hit.ce_score = Some(ce_norm);
            (blended, hit)
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

    /// Batched [`score`](Self::score): score every `(query, passage_i)` pair in a
    /// single forward pass. Returns one logit per passage in input order, each
    /// honoring the same neutral-`0.0`-on-error contract as [`score`](Self::score).
    ///
    /// Fallback: if the batched forward errors as a whole (e.g. an OOM or a
    /// tokenize failure on one pair surfaces as a batch `Err`), we DO NOT
    /// neutralize the entire pool — we fall back to per-pair [`score`](Self::score),
    /// so a single bad pair degrades only its own element to a neutral logit while
    /// the rest keep their real scores. Empty input → empty output (no forward).
    fn score_batch(&self, query: &str, passages: &[&str]) -> Vec<f64> {
        match self.inner.score_batch(query, passages) {
            Ok(logits) => logits.into_iter().map(f64::from).collect(),
            Err(_) => passages.iter().map(|p| self.score(query, p)).collect(),
        }
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
    importance_enabled: bool,
    vector_stage_only: bool,
    raw_query: &str,
    rerank_depth: usize,
    use_graph_arm: bool,
    alpha: f64,
    pool_n: usize,
    explain: bool,
    view: ReadView,
) -> ReaderResponse {
    // 0.8.20 Slice 15b fix-2 (R-20-NV) — the `:now` instant is read HERE, in
    // Rust, ONCE per query, and bound positionally into every node-hydration
    // SELECT. Never `datetime('now')` / `strftime('%s','now')`: an inline clock
    // would make the query non-deterministic, untestable, and re-evaluated per
    // row. `None` ⇒ the view relaxes validity ⇒ no conjunct is emitted and
    // nothing is bound (`validity_sql` returns the empty string).
    //
    // fix-3 (F2): FREEZE the view here, at the single point every arm flows
    // through. `freeze()` is the only place on this path that reads the clock;
    // downstream arms hold a `FrozenView` and have no way to resolve a second,
    // different instant. Previously the graph arm re-derived it from the raw
    // `ReadView`, so a boundary-straddling query could have its arms disagree.
    let view = view.freeze();
    let now_param = view.now_param();
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
            // fix-3 (F1, codex §9 [P2]) — OVERFETCH the phase-2 rerank so the
            // validity/existence filter applied at hydration cannot starve the
            // result set.
            //
            // The defect: hydration drops rows that are expired, superseded or
            // inactive, but it ran on candidates ALREADY truncated to
            // `final_limit`. If the nearest `final_limit` neighbours were all
            // out-of-window they consumed every slot and were then dropped, so
            // valid rows just below the cutoff were never considered — a
            // default search silently returned too few hits, or none.
            //
            // Why overfetch rather than filtering in SQL: the natural fix is to
            // join `canonical_nodes` into the candidate query, but (i) phase 1
            // is a `vec0` KNN and ADR-0.8.11 D3 forbids demoting it with
            // non-metadata predicates, and (ii) there is NO index on
            // `canonical_nodes(write_cursor)`, so an `EXISTS` per candidate
            // would be a full scan × the whole pool on EVERY query — a
            // guaranteed cost to fix a degenerate case.
            //
            // Overfetching is free by comparison: phase 2 already computes
            // `vec_distance_l2` for all `TOP_K_BIT_CANDIDATES` in order to sort
            // them, so raising the LIMIT only returns more of a result set that
            // was already materialized. No extra vec0 work, no schema change,
            // no new index, no second query. The hydration loop below then
            // stops at `final_limit` SURVIVING hits, so the common case does
            // exactly as many hydration probes as before.
            //
            // `max` (not a bare constant) because `set_search_limit_for_test`
            // may raise `final_limit` above the pool for the recall harness;
            // this must never request FEWER candidates than the caller wants.
            let candidate_limit = final_limit.max(TOP_K_BIT_CANDIDATES);
            let sql = build_vector_phase1_sql(filter, candidate_limit);
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
        // Cause-A: the two node/edge SELECTs additively fetch `logical_id` so the
        // hit can carry a stable cross-session id (derive_stable_id). Read-only
        // additive column — ordering/scores are untouched.
        // fix-1 (codex §9): co-locate BOTH existence guards. Node supersession
        // is tombstone-then-insert (`commit_batch`) — the prior `canonical_nodes`
        // row is kept (same `write_cursor`, `state = 'active'`, `superseded_at`
        // set) and, unlike the edge path (fix-30), its stale `vector_default` row
        // is NOT pruned, so the phase-1 bit-KNN can still surface the OLD cursor.
        // Without `superseded_at IS NULL` here that superseded version would
        // hydrate and leak stale content through vector search. This matches the
        // edge branch below and every other retrieval site (design §2: enforce
        // the exclusion at EVERY retrieval site). It only drops already-superseded
        // rows → a no-op on the all-active / non-superseded corpus.
        // TC-31 (0.8.20 Slice 10a): both hydration SELECTs additively fetch the
        // canonical row's OWN `source_id` so a vector hit carries the provenance
        // `erase_source` consumes. These statements already read the canonical
        // row by `write_cursor`, so this is one extra COLUMN on an existing
        // lookup — NOT an extra query. (A per-hit `WHERE write_cursor = ?`
        // probe would be a full scan: there is no index on
        // `canonical_nodes(write_cursor)`. This site already pays that cost by
        // construction; TC-31 must not add a second one.) Read-only additive
        // column — row-set, ordering and scores are untouched.
        // fix-2 (codex §9 [P2]): the validity conjunct comes from
        // `ReadView::validity_sql` — the SAME generator the five read verbs use.
        // It is NOT hand-rolled here: Slice 10's whole design is that the
        // predicate exists in exactly ONE place, so no retrieval site can drift
        // from another. `?1` is the candidate rowid, so `:now` binds at `?2`.
        // On a corpus that never authored a window every row is NULL/NULL and
        // the conjunct matches everything ⇒ default behaviour is unchanged.
        let node_validity = view.validity_sql("canonical_nodes", 2);
        let mut node_stmt = tx.prepare(&format!(
            "SELECT kind, body, logical_id, source_id FROM canonical_nodes \
             WHERE write_cursor = ?1 AND superseded_at IS NULL AND state = 'active'\
             {node_validity} LIMIT 1"
        ))?;
        // fix-2 (codex §9 [P2]): an edge body projected into `vector_default`
        // (kind = "edge_fact") is hydrated HERE by write_cursor. Gating on
        // `superseded_at` alone let an EXPIRED edge (`t_invalid <= :now`) surface
        // its body through the VECTOR arm — the same "validity enforced on
        // traversal, not on search" gap Slice 15b closed for nodes, now on the
        // edge-vector read path. Apply the shared `edge_validity_sql` predicate
        // (the ONE generator every edge read site uses) so no arm can drift.
        // `?1` is the rowid, so the edge `:now` binds at `?2`; the instant is the
        // frozen `view.edge_now()` — a bound value, never `datetime('now')`
        // (the :9161 no-inline-clock rule). edge_now is ALWAYS present, so unlike
        // node validity this conjunct is unconditional (an edge invalidated in the
        // past stays excluded even when node existence is relaxed).
        let edge_validity = edge_validity_sql("canonical_edges", 2);
        let mut edge_stmt = tx.prepare(&format!(
            "SELECT body, logical_id, source_id FROM canonical_edges \
             WHERE write_cursor = ?1 AND superseded_at IS NULL AND body IS NOT NULL\
             {edge_validity} LIMIT 1"
        ))?;
        // The bound parameter list for the node lookup: the candidate rowid,
        // plus `:now` when (and only when) the view emitted a validity conjunct.
        // One instant for the whole query — resolved once, above, not per row.
        let node_params = |rowid: i64| -> Vec<rusqlite::types::Value> {
            let mut p = vec![rusqlite::types::Value::Integer(rowid)];
            if let Some(now) = now_param {
                p.push(rusqlite::types::Value::Integer(now));
            }
            p
        };
        for (rowid, score) in rowids {
            // fix-3 (F1): the candidate list is now the OVERFETCHED pool in
            // exact-L2 order, so the caller's cutoff is applied HERE — after
            // the validity/existence filter, not before it. Bounded worst case:
            // at most `TOP_K_BIT_CANDIDATES` hydration probes when nearly every
            // candidate is filtered out; exactly `final_limit` (i.e. unchanged)
            // when nothing is. Ordering is unchanged — the surviving rows are
            // still emitted nearest-first — so on a corpus with no windows this
            // loop yields byte-identical results to the pre-fix code.
            if results.len() >= final_limit {
                break;
            }
            if let Ok((kind, body, logical_id, source_id)) =
                node_stmt.query_row(rusqlite::params_from_iter(node_params(rowid)), |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                })
            {
                let id = derive_stable_id(logical_id.as_deref(), &body);
                results.push(SearchHit {
                    id,
                    write_cursor: rowid as u64,
                    kind,
                    body,
                    score,
                    branch: SoftFallbackBranch::Vector,
                    // TC-31: the NODE's own provenance (a node hit is erased by
                    // the document it was written from).
                    source_id,
                    ce_score: None,
                });
            } else if let Ok((body, logical_id, source_id)) =
                edge_stmt.query_row(rusqlite::params![rowid, view.edge_now()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
            {
                let id = derive_stable_id(logical_id.as_deref(), &body);
                results.push(SearchHit {
                    id,
                    write_cursor: rowid as u64,
                    kind: "edge_fact".to_string(),
                    body,
                    score,
                    branch: SoftFallbackBranch::TextEdge,
                    // TC-31: the EDGE's own provenance — consistent with the
                    // graph arm's existing edge-source semantics.
                    source_id,
                    ce_score: None,
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
        let limit_clause = perf_limit.map(|k| format!(" LIMIT {k}")).unwrap_or_default();
        // Cause-A: PREFER a logical_id-bearing query — LEFT JOIN canonical_nodes so
        // node hits carry the `l:`-tagged stable id. The join is 1:1 on
        // `write_cursor` (search_index holds node bodies only; edge bodies live in
        // search_index_edges), so the row-set and the `bm25(search_index),
        // write_cursor` ordering are byte-unchanged — only `cn.logical_id` is added.
        // FALL BACK to the original (logical_id-free) query on pre-step-12 schemas
        // (v10) whose `canonical_nodes` lacks `logical_id`: those hits key by the
        // `h:` content-hash. This keeps old-schema search byte-identical to the
        // pre-Cause-A behaviour (the prepare of the plain SQL is the exact prior
        // statement). Columns are qualified because both tables expose `write_cursor`.
        // CORRECTNESS (0.8.11.2 pico): `AND cn.superseded_at IS NULL` drops
        // superseded node versions. Node supersession is tombstone-then-insert
        // (`commit_batch`): the prior `canonical_nodes` row is UPDATEd to set
        // `superseded_at` (row kept, same write_cursor) and a NEW `search_index`
        // row is inserted for the new cursor — the OLD `search_index` row is
        // never deleted, so without this filter both versions stay live in FTS
        // and the stale one is returned. The other arms already filter this way
        // (edge branch, graph-arm node seed, point-recall `read_get_by_id`);
        // only this default node-text branch was missing it. The `LEFT JOIN` is
        // KEPT (not switched to inner): an active row joins to its `cn` with
        // `superseded_at = NULL` (kept); a superseded row joins to its tombstoned
        // `cn` with `superseded_at` NOT NULL (dropped); a legacy/orphan
        // `search_index` row with no `cn` gets `superseded_at = NULL` via the
        // LEFT JOIN (KEPT — preserves prior behaviour for ownerless rows).
        // TC-31 (0.8.20 Slice 10a): `cn.source_id` is selected off the SAME
        // already-present 1:1 LEFT JOIN that supplies `cn.logical_id` — one extra
        // column, no extra query, no row-set or ordering change.
        // fix-2 (codex §9 [P2]): the node-body FTS branch takes the SAME
        // validity conjunct, generated by `ReadView::validity_sql` rather than
        // hand-rolled — the predicate lives in exactly one place (Slice 10).
        // `?1` is the MATCH expression, so `:now` binds at `?2`.
        //
        // The generated conjunct is NULL-PERMISSIVE by construction
        // (`valid_from IS NULL OR ...`), which is exactly what this LEFT JOIN
        // needs: an ownerless `search_index` row with no `cn` reads NULL on both
        // columns and is KEPT, preserving the deliberate keep-ownerless
        // behaviour the `superseded_at` / `state` conjuncts above encode with
        // their explicit `OR ... IS NULL`. No extra `OR IS NULL` is needed here,
        // and none may be added: that would be a second, drifting copy of the
        // predicate.
        //
        // NO-REGRESSION: on a corpus that never authored a window every
        // `cn.valid_from` / `cn.valid_until` is NULL (step 22 back-filled NULL
        // with no DEFAULT), so both disjuncts are TRUE for every row and the
        // row-set, the `bm25(search_index), write_cursor` ordering and the
        // scores are all byte-unchanged.
        let text_validity = view.validity_sql("cn", 2);
        let join_sql = format!(
            "SELECT search_index.body, search_index.kind, search_index.write_cursor, \
             bm25(search_index), cn.logical_id, cn.source_id FROM search_index \
             LEFT JOIN canonical_nodes cn ON cn.write_cursor = search_index.write_cursor \
             WHERE search_index MATCH ?1 \
               AND cn.superseded_at IS NULL \
               AND (cn.state = 'active' OR cn.state IS NULL)\
               {text_validity} \
             ORDER BY bm25(search_index), search_index.write_cursor{limit_clause}"
        );
        // `:now` rides at ?2 only when the view emitted a conjunct; the relaxed
        // view produces the byte-identical single-parameter statement.
        let mut text_params: Vec<rusqlite::types::Value> =
            vec![rusqlite::types::Value::Text(compiled.match_expression.clone())];
        if let Some(now) = now_param {
            text_params.push(rusqlite::types::Value::Integer(now));
        }
        if let Ok(mut statement) = tx.prepare(&join_sql) {
            let rows =
                statement.query_map(rusqlite::params_from_iter(text_params.iter()), |row| {
                    let body = row.get::<_, String>(0)?;
                    let logical_id = row.get::<_, Option<String>>(4)?;
                    Ok(SearchHit {
                        id: derive_stable_id(logical_id.as_deref(), &body),
                        body,
                        kind: row.get::<_, String>(1)?,
                        write_cursor: row.get::<_, i64>(2)? as u64,
                        score: row.get::<_, f64>(3)?,
                        branch: SoftFallbackBranch::Text,
                        // TC-31: the NODE's own provenance. NULL for a legacy /
                        // TC-11-spared governed row, and NULL for an ownerless
                        // `search_index` row the LEFT JOIN keeps with no `cn`.
                        source_id: row.get::<_, Option<String>>(5)?,
                        ce_score: None,
                    })
                })?;
            rows.flatten().collect()
        } else {
            // No `superseded_at IS NULL` filter here (and none is possible): this
            // fallback fires only on pre-step-12 schemas whose `canonical_nodes`
            // lacks `logical_id` — and step-12 adds `logical_id` and
            // `superseded_at` in the SAME migration, so this schema has neither
            // column. Supersession (`commit_batch`) is a no-op without
            // `logical_id`, so no superseded node rows can exist on this path.
            //
            // TC-31 (0.8.20 Slice 10a): `source_id` arrived in step 8, `logical_id`
            // in step 12, so a schema that lands HERE (no `logical_id`) may still
            // HAVE `source_id` — steps 8..11. Try a provenance-bearing variant
            // first, adding only `cn.source_id` over the SAME 1:1 LEFT JOIN shape
            // used above (row-set and ordering unchanged; a missing `cn` row keeps
            // NULL as before). Fall back to the historical, byte-identical
            // provenance-free statement on a pre-step-8 schema, where the column
            // genuinely does not exist and `None` is the only truthful answer.
            let source_sql = format!(
                "SELECT search_index.body, search_index.kind, search_index.write_cursor, \
                 bm25(search_index), cn.source_id FROM search_index \
                 LEFT JOIN canonical_nodes cn ON cn.write_cursor = search_index.write_cursor \
                 WHERE search_index MATCH ?1 \
                 ORDER BY bm25(search_index), search_index.write_cursor{limit_clause}"
            );
            if let Ok(mut statement) = tx.prepare(&source_sql) {
                let rows = statement.query_map([compiled.match_expression.as_str()], |row| {
                    let body = row.get::<_, String>(0)?;
                    Ok(SearchHit {
                        // No logical_id column on this schema → content-hash id.
                        id: derive_stable_id(None, &body),
                        body,
                        kind: row.get::<_, String>(1)?,
                        write_cursor: row.get::<_, i64>(2)? as u64,
                        score: row.get::<_, f64>(3)?,
                        branch: SoftFallbackBranch::Text,
                        source_id: row.get::<_, Option<String>>(4)?,
                        ce_score: None,
                    })
                })?;
                rows.flatten().collect()
            } else {
                // Pre-step-8: no `source_id` column anywhere. Byte-identical to
                // the historical statement.
                let plain_sql = format!(
                    "SELECT body, kind, write_cursor, bm25(search_index) FROM search_index \
                     WHERE search_index MATCH ?1 \
                     ORDER BY bm25(search_index), write_cursor{limit_clause}"
                );
                let mut statement = tx.prepare(&plain_sql)?;
                let rows = statement.query_map([compiled.match_expression.as_str()], |row| {
                    let body = row.get::<_, String>(0)?;
                    Ok(SearchHit {
                        // No logical_id column on this schema → content-hash id.
                        id: derive_stable_id(None, &body),
                        body,
                        kind: row.get::<_, String>(1)?,
                        write_cursor: row.get::<_, i64>(2)? as u64,
                        score: row.get::<_, f64>(3)?,
                        branch: SoftFallbackBranch::Text,
                        source_id: None,
                        ce_score: None,
                    })
                })?;
                rows.flatten().collect()
            }
        }
    };
    let mut text_results: Vec<SearchHit> = Vec::with_capacity(text_candidates.len());
    for hit in text_candidates {
        if text_hit_passes_filter(&tx, hit.write_cursor, &hit.kind, filter)? {
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
        // Cause-A: the JOIN to canonical_edges already exists; additively select
        // `ce.logical_id` (edges always carry one) for the stable hit-id. No
        // ordering/row-set change.
        // TC-31 (0.8.20 Slice 10a): `ce.source_id` rides the SAME existing inner
        // JOIN as `ce.logical_id` — one extra column, no extra query, no
        // row-set/ordering change. An edge hit carries the EDGE's own provenance,
        // matching the graph arm's edge-source semantics.
        // fix-2 (codex §9 [P2]): the JOIN already dropped superseded edge rows,
        // but a body-bearing edge written with `t_invalid <= :now` (expired /
        // invalidated) still MATCHed and surfaced its body through ordinary
        // search — edge temporal validity was enforced on the graph-traversal and
        // projection paths but NOT on this FTS read path. Apply the shared
        // `edge_validity_sql` conjunct (the ONE generator every edge read site
        // uses, so no path can drift). `?1` is the MATCH expression, so the edge
        // `:now` binds at `?2`; the instant is the frozen `view.edge_now()` — a
        // bound value, never `datetime('now')` (the :9161 no-inline-clock rule),
        // and always present (edge invalidation is not relaxed by node existence
        // relaxation).
        let edge_validity = edge_validity_sql("ce", 2);
        let edge_sql = format!(
            "SELECT sei.body, sei.kind, sei.write_cursor, bm25(search_index_edges), \
             ce.logical_id, ce.source_id \
             FROM search_index_edges sei \
             JOIN canonical_edges ce ON ce.write_cursor = sei.write_cursor \
             WHERE search_index_edges MATCH ?1 \
               AND ce.superseded_at IS NULL{edge_validity} \
             ORDER BY bm25(search_index_edges), sei.write_cursor"
        );
        // search_index_edges may not exist on very old DBs not yet at step-14;
        // ignore the error gracefully (returns empty slice).
        if let Ok(mut stmt) = tx.prepare(&edge_sql) {
            if let Ok(rows) = stmt.query_map(
                rusqlite::params![compiled.match_expression.as_str(), view.edge_now()],
                |row| {
                    let body = row.get::<_, String>(0)?;
                    let logical_id = row.get::<_, Option<String>>(4)?;
                    Ok(SearchHit {
                        id: derive_stable_id(logical_id.as_deref(), &body),
                        body,
                        kind: row.get::<_, String>(1)?,
                        write_cursor: row.get::<_, i64>(2)? as u64,
                        score: row.get::<_, f64>(3)?,
                        branch: SoftFallbackBranch::TextEdge,
                        // TC-31: the EDGE's own provenance.
                        source_id: row.get::<_, Option<String>>(5)?,
                        ce_score: None,
                    })
                },
            ) {
                rows.flatten().collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    };
    for row in edge_candidates {
        if edge_fts_hit_passes_filter(&tx, row.write_cursor, &row.kind, filter)? {
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

    // 0.8.8 EXP-OBS (Slice 5) — capture per-arm rank maps + counts BEFORE the arms
    // are consumed by fusion. All reads; only when `explain` (else zero work).
    // `body_rank_map` keeps the FIRST occurrence (== the rank `fuse_three_arms`
    // uses, which dedups keeping the first). `*_fused_scores` is captured from the
    // post-recency / pre-CE intermediate so `fused_score` is faithful to what
    // `ce_rerank` normalizes.
    let body_rank_map = |hits: &[SearchHit]| -> HashMap<String, u32> {
        let mut m: HashMap<String, u32> = HashMap::new();
        for (i, h) in hits.iter().enumerate() {
            m.entry(h.body.clone()).or_insert(i as u32);
        }
        m
    };
    let body_score_map = |hits: &[SearchHit]| -> HashMap<String, f64> {
        hits.iter().map(|h| (h.body.clone(), h.score)).collect()
    };

    let (exp_vector_ranks, exp_text_ranks, exp_vector_n, exp_text_n) = if explain {
        (
            Some(body_rank_map(&vector_results)),
            Some(body_rank_map(&text_results)),
            vector_results.len() as u32,
            text_results.len() as u32,
        )
    } else {
        (None, None, 0, 0)
    };
    let mut exp_graph_ranks: Option<HashMap<String, u32>> = None;
    let mut exp_fused_scores: Option<HashMap<String, f64>> = None;
    let mut exp_graph_n: u32 = 0;
    // F9 (0.8.16 Slice 5) — per-hit importance/confidence contribution maps
    // (keyed by hit id == write_cursor), captured for the explain sidecar.
    let mut exp_importance: Option<HashMap<u64, f64>> = None;
    let mut exp_confidence: Option<HashMap<u64, f64>> = None;

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
        let (graph_candidates, stats, graph_edge_confidence) = bfs_graph_arm_candidates(
            reader,
            &two_arm_fused,
            compiled.match_expression.as_str(),
            3,
            50,
            view,
        )?;
        graph_stats = stats;
        if explain {
            exp_graph_ranks = Some(body_rank_map(&graph_candidates));
            exp_graph_n = graph_candidates.len() as u32;
        }
        // Named intermediate (byte-identical to the prior nested call) so explain
        // can read the pre-CE fused scores without perturbing the ranking.
        let fused = apply_recency_reweight(
            fuse_three_arms(two_arm_fused, vec![], graph_candidates),
            recency_enabled,
        );
        // F9 — importance (node) / confidence (edge) reweight, OFF by default.
        // Order: AFTER recency (consistent placement), BEFORE the CE rerank seam.
        let (imp_map, mut conf_map) = if importance_enabled || explain {
            build_importance_confidence_maps(reader, &fused).unwrap_or_default()
        } else {
            (HashMap::new(), HashMap::new())
        };
        // F9 FIX-1: `build_importance_confidence_maps` keys edge confidence on the
        // EDGE `write_cursor`, which never matches a graph-arm NODE hit's cursor —
        // so it alone leaves graph-arm hits with no edge confidence. Merge the
        // BFS-collected per-node traversing-edge confidence (node cursor ⇒ conf).
        // Node/edge cursors are globally unique, so there is never a key collision
        // with the edge-fact confidence above; `or_insert` documents that intent.
        if importance_enabled || explain {
            for (cursor, conf) in &graph_edge_confidence {
                conf_map.entry(*cursor).or_insert(*conf);
            }
        }
        let fused = apply_importance_reweight(fused, &imp_map, &conf_map, importance_enabled);
        if explain {
            exp_importance = Some(imp_map);
            exp_confidence = Some(conf_map);
            exp_fused_scores = Some(body_score_map(&fused));
        }
        rerank_fused(raw_query, fused, rerank_depth, alpha, pool_n)
    } else {
        // G9 + G12: RRF-fuse the two ranked branches (keyed on body, vector-first
        // tiebreak) into the unconditional new ranking, recency-reweight (gated,
        // off by default), then pass through the identity rerank seam. The
        // vector-empty `soft_fallback` signal was computed above, BEFORE this
        // branch-collapse.
        let fused = apply_recency_reweight(fuse_rrf(vector_results, text_results), recency_enabled);
        // F9 — importance (node) / confidence (edge) reweight, OFF by default.
        // Same placement as the graph-arm branch: after recency, before CE rerank.
        let (imp_map, conf_map) = if importance_enabled || explain {
            build_importance_confidence_maps(reader, &fused).unwrap_or_default()
        } else {
            (HashMap::new(), HashMap::new())
        };
        let fused = apply_importance_reweight(fused, &imp_map, &conf_map, importance_enabled);
        if explain {
            exp_importance = Some(imp_map);
            exp_confidence = Some(conf_map);
            exp_fused_scores = Some(body_score_map(&fused));
        }
        rerank_fused(raw_query, fused, rerank_depth, alpha, pool_n)
    };

    // 0.8.8 EXP-OBS — assemble the sidecar `Explanation` from the captured maps +
    // the final `results`. `embedder_id` is left empty here (the worker has no
    // identity) and filled by `search_inner_with_stats`.
    let explanation = if explain {
        let fused_scores = exp_fused_scores.unwrap_or_default();
        let per_hit: Vec<PerHitExplain> = results
            .iter()
            .map(|h| PerHitExplain {
                // `PerHitExplain.id` carries the engine-internal positional
                // `write_cursor` (the pre-C-2 `SearchHit.id`), matching the
                // telemetry `result_ids` / importance-map key space; the typed
                // `SearchHit.id` is the separate caller-facing identity.
                id: h.write_cursor,
                arm: h.branch,
                vector_rank: exp_vector_ranks.as_ref().and_then(|m| m.get(&h.body).copied()),
                text_rank: exp_text_ranks.as_ref().and_then(|m| m.get(&h.body).copied()),
                graph_rank: exp_graph_ranks.as_ref().and_then(|m| m.get(&h.body).copied()),
                fused_score: fused_scores.get(&h.body).copied().unwrap_or(h.score),
                ce_score: h.ce_score,
                blended: h.score,
                importance: exp_importance.as_ref().and_then(|m| m.get(&h.write_cursor).copied()),
                confidence: exp_confidence.as_ref().and_then(|m| m.get(&h.write_cursor).copied()),
            })
            .collect();
        let ce_active = rerank_depth > 0 && per_hit.iter().any(|p| p.ce_score.is_some());
        Some(Explanation {
            trace: QueryTrace {
                query_chars: raw_query.chars().count() as u32,
                k: final_limit as u32,
                rerank_depth: rerank_depth as u32,
                pool_n: pool_n as u32,
                alpha,
                use_graph_arm,
                recency: recency_enabled,
                embedder_id: String::new(),
                ce_active,
                vector_hits: exp_vector_n,
                text_hits: exp_text_n,
                graph_hits: exp_graph_n,
            },
            per_hit,
        })
    } else {
        None
    };

    Ok((cursor, soft_fallback, results, graph_stats, explanation))
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
///
/// **F9 (0.8.16 Slice 5) confidence carry:** the third tuple element maps each
/// emitted graph-arm hit's `write_cursor` (its `SearchHit.id`, a NODE cursor) to
/// the `confidence` of the EDGE traversed to reach that node — the input the F9
/// reweight (`graph_rrf_score(edge) = confidence × 1/(K+bfs_rank)`) consumes.
/// `build_importance_confidence_maps` keys edge confidence on the EDGE
/// `write_cursor`, which never equals a reached node's cursor, so without this
/// carry edge confidence never reaches a graph-arm hit. **Determinism rule (matches
/// the BLOCK-2 provenance carry):** when several edges reach the same node, the
/// FIRST edge to claim the node in the `visited` dedup wins — i.e. the edge that
/// produced the node's winning `bfs_rank` (seeds are considered before Phase-2
/// neighbors; within a phase, `ORDER BY write_cursor` makes the earliest-written
/// edge win). A NULL edge confidence is simply not inserted ⇒ neutral (1.0).
fn bfs_graph_arm_candidates(
    reader: &mut Connection,
    fused_hits: &[SearchHit],
    match_expression: &str,
    max_depth: u32,
    cap: usize,
    view: FrozenView,
) -> rusqlite::Result<(Vec<SearchHit>, GraphFrontierStats, HashMap<u64, f64>)> {
    // fix-2 (codex §9 [P2]): the opt-in graph arm hydrates NODES too, so it takes
    // the same validity conjunct as the vector and FTS branches — otherwise
    // `search_reranked(.., use_graph_arm = true)` would keep the exact leak the
    // other two branches just closed. Same generator, same bound `:now`.
    //
    // fix-3 (F2): the instant arrives ALREADY RESOLVED in the `FrozenView` — it
    // is the identical value the vector and FTS arms bound. This arm cannot
    // re-read the clock: a `FrozenView` carries no route to one.
    let now_param = view.now_param();
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
    // F9 (0.8.16 Slice 5) — per-hit traversing-edge confidence, keyed by the
    // emitted hit's NODE `write_cursor`. First edge to reach a node wins (visited
    // dedup); NULL confidence is never inserted (⇒ neutral in the reweight).
    let mut edge_confidence_by_cursor: HashMap<u64, f64> = HashMap::new();
    // G0 Phase-2 (BLOCK-1) frontier meter — distinct seed candidates considered vs
    // resolved-active; `resolved_seed_rate` flips 0→>0 once entities/edge-facts seed.
    let mut stats = GraphFrontierStats::default();
    {
        // C1 seeding — gather distinct candidate (logical_id, provenance source_id)
        // pairs from the graph's OWN query-matched FTS surfaces (NOT doc-node hits).
        // Order-preserving dedup (first provenance wins) so `seeds_considered` counts
        // each candidate once. `source_id` is the session the seed traces back to: the
        // matched edge's `source_id` (source A) or the entity node's own (source B).
        // F9: each seed carries the confidence of the edge that surfaced it
        // (`None` for entity-FTS seeds, which have no traversing edge).
        let mut candidate_seeds: Vec<(String, Option<String>, Option<f64>)> = Vec::new();
        let mut seen_candidates: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let push_candidate =
            |lid: String,
             source_id: Option<String>,
             confidence: Option<f64>,
             seen: &mut std::collections::HashSet<String>,
             out: &mut Vec<(String, Option<String>, Option<f64>)>| {
                if seen.insert(lid.clone()) {
                    out.push((lid, source_id, confidence));
                }
            };

        // Seed source A — edge-fact endpoints (primary). Both endpoints of each
        // matched, temporally-live, non-fallback edge are candidate seeds, tagged with
        // the edge's `source_id` provenance and (F9) `confidence`. `search_index_edges`
        // may be absent on very old DBs (< step-14) — degrade to no edge seeds rather
        // than error.
        // TC-33: `?1` MATCH, `?2` LIMIT ⇒ the edge `:now` binds at `?3`.
        if let Ok(mut edge_seed_stmt) = tx.prepare(&format!(
            "SELECT ce.from_id, ce.to_id, ce.source_id, ce.confidence \
             FROM search_index_edges sei \
             JOIN canonical_edges ce ON ce.write_cursor = sei.write_cursor \
             WHERE search_index_edges MATCH ?1 \
               AND ce.superseded_at IS NULL{} \
               AND (ce.temporal_fallback IS NULL OR ce.temporal_fallback = 0) \
             ORDER BY bm25(search_index_edges), sei.write_cursor \
             LIMIT ?2",
            edge_validity_sql("ce", 3)
        )) {
            let rows = edge_seed_stmt.query_map(
                rusqlite::params![match_expression, SEED_FTS_N as i64, view.edge_now()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<f64>>(3)?,
                    ))
                },
            )?;
            for quad in rows {
                let (from_id, to_id, source_id, confidence) = quad?;
                push_candidate(
                    from_id,
                    source_id.clone(),
                    confidence,
                    &mut seen_candidates,
                    &mut candidate_seeds,
                );
                push_candidate(
                    to_id,
                    source_id,
                    confidence,
                    &mut seen_candidates,
                    &mut candidate_seeds,
                );
            }
        }

        // Seed source B — entity-node FTS (isolated / strongly-named entities).
        // `logical_id IS NOT NULL` structurally excludes doc nodes (the bug surface).
        // Provenance = the node's own `source_id` (the session it was extracted from).
        {
            // `?1` MATCH, `?2` LIMIT ⇒ `:now` binds at `?3`.
            let seed_validity = view.validity_sql("cn", 3);
            let mut node_seed_stmt = tx.prepare(&format!(
                "SELECT cn.logical_id, cn.source_id \
                 FROM search_index si \
                 JOIN canonical_nodes cn ON cn.write_cursor = si.write_cursor \
                 WHERE search_index MATCH ?1 \
                   AND cn.superseded_at IS NULL \
                   AND cn.state = 'active' \
                   AND cn.logical_id IS NOT NULL\
                   {seed_validity} \
                 ORDER BY bm25(search_index), si.write_cursor \
                 LIMIT ?2"
            ))?;
            let mut seed_params: Vec<rusqlite::types::Value> = vec![
                rusqlite::types::Value::Text(match_expression.to_string()),
                rusqlite::types::Value::Integer(SEED_FTS_N as i64),
            ];
            if let Some(now) = now_param {
                seed_params.push(rusqlite::types::Value::Integer(now));
            }
            let rows = node_seed_stmt
                .query_map(rusqlite::params_from_iter(seed_params.iter()), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })?;
            for pair in rows {
                let (lid, source_id) = pair?;
                // Entity-FTS seed: no traversing edge ⇒ no edge confidence (neutral).
                push_candidate(lid, source_id, None, &mut seen_candidates, &mut candidate_seeds);
            }
        }

        // Resolve + emit: a seed is `resolved` only if an ACTIVE canonical_node carries
        // that logical_id (dangling edge endpoints count considered-not-resolved). A
        // resolved seed is BOTH a BFS root AND emitted as a graph-arm candidate (depth
        // 0, hop_score 1.0) — so an edge-only query match surfaces the connected ENTITY
        // nodes, not just the fact body (codex §9 [P2]). Seeds whose body is already in
        // the two-arm result are skipped; the cap is respected.
        let active_validity = view.validity_sql("canonical_nodes", 2);
        let mut active_stmt = tx.prepare(&format!(
            "SELECT kind, body, write_cursor FROM canonical_nodes \
             WHERE logical_id = ?1 AND superseded_at IS NULL AND state = 'active'\
             {active_validity} LIMIT 1"
        ))?;
        for (lid, source_id, seed_confidence) in candidate_seeds {
            stats.seeds_considered += 1;
            let mut active_params: Vec<rusqlite::types::Value> =
                vec![rusqlite::types::Value::Text(lid.clone())];
            if let Some(now) = now_param {
                active_params.push(rusqlite::types::Value::Integer(now));
            }
            let row: Option<(String, String, i64)> = active_stmt
                .query_row(rusqlite::params_from_iter(active_params.iter()), |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
                })
                .optional()?;
            if let Some((kind, body, write_cursor)) = row {
                stats.seeds_resolved += 1;
                if visited.insert(lid.clone()) {
                    // Cause-A: the seed's `logical_id` is in hand (`lid`) — derive the
                    // stable id before `lid` is moved onto the frontier (zero extra query).
                    let id = derive_stable_id(Some(&lid), &body);
                    frontier.push_back((lid, 0));
                    if !seed_bodies.contains(body.as_str()) && candidates.len() < cap {
                        // depth-0 hop_score = 1.0/(1.0+0) = 1.0; synthesized penalty for
                        // 'unknown' kind (mirrors the Phase-2 neighbor scoring).
                        let score = if kind == "unknown" { SYNTHESIZED_PENALTY } else { 1.0 };
                        // F9: an edge-seeded endpoint carries its seeding edge's
                        // confidence (source A); entity-FTS seeds carry None.
                        if let Some(c) = seed_confidence {
                            edge_confidence_by_cursor.insert(write_cursor as u64, c);
                        }
                        candidates.push(SearchHit {
                            id,
                            write_cursor: write_cursor as u64,
                            kind,
                            body,
                            score,
                            branch: SoftFallbackBranch::GraphArm,
                            source_id,
                            ce_score: None,
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
        // F9: also carry the traversed edge's `confidence` — the reweight input for
        // the reached node (keyed downstream by the node's `write_cursor`). Same
        // determinism as `source_id`: the earliest-written edge wins the `visited`
        // dedup, so the reached node's confidence is the winning-`bfs_rank` edge's.
        // TC-33: `?1` is the anchor logical_id ⇒ the edge `:now` binds at `?2`.
        &format!(
            "SELECT e.from_id, e.to_id, e.source_id, e.confidence \
             FROM canonical_edges e \
             WHERE (e.from_id = ?1 OR e.to_id = ?1) \
               AND e.superseded_at IS NULL{} \
               AND (e.temporal_fallback IS NULL OR e.temporal_fallback = 0) \
             ORDER BY e.write_cursor \
             LIMIT 64",
            edge_validity_sql("e", 2)
        ),
    )?;
    // Fetch write_cursor alongside kind+body so graph-arm hits carry a real id
    // for apply_recency_reweight (id=0 would force min_id=0 and distort span).
    let body_validity = view.validity_sql("canonical_nodes", 2);
    let mut body_stmt = tx.prepare(&format!(
        "SELECT kind, body, write_cursor FROM canonical_nodes \
         WHERE logical_id = ?1 AND superseded_at IS NULL AND state = 'active'\
         {body_validity} \
         LIMIT 1"
    ))?;

    while let Some((lid, depth)) = frontier.pop_front() {
        if candidates.len() >= cap {
            break;
        }
        if depth >= max_depth {
            continue;
        }

        // Fetch temporal-live neighbors via edges, each paired with the
        // traversing edge's `source_id` (BLOCK-2 provenance carry) and (F9)
        // `confidence` (the reweight input for the reached node).
        let neighbors: Vec<(String, Option<String>, Option<f64>)> = {
            let rows = edge_stmt.query_map(params![&lid, view.edge_now()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                ))
            })?;
            rows.flatten()
                .map(|(from_id, to_id, source_id, confidence)| {
                    let neighbor = if from_id == lid { to_id } else { from_id };
                    (neighbor, source_id, confidence)
                })
                .collect()
        };

        for (neighbor, edge_source_id, edge_confidence) in neighbors {
            if visited.contains(&neighbor) {
                continue;
            }
            visited.insert(neighbor.clone());

            // Fetch neighbor body + write_cursor from canonical_nodes.
            let mut body_params: Vec<rusqlite::types::Value> =
                vec![rusqlite::types::Value::Text(neighbor.clone())];
            if let Some(now) = now_param {
                body_params.push(rusqlite::types::Value::Integer(now));
            }
            let row: Option<(String, String, i64)> = body_stmt
                .query_row(rusqlite::params_from_iter(body_params.iter()), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
                })
                .optional()?;

            if let Some((kind, body, write_cursor)) = row {
                // Skip bodies already covered by the two-arm result.
                if !seed_bodies.contains(body.as_str()) {
                    let hop_score = 1.0 / (1.0 + (depth + 1) as f64);
                    let score =
                        if kind == "unknown" { hop_score * SYNTHESIZED_PENALTY } else { hop_score };
                    // Cause-A: the neighbor's `logical_id` is `neighbor` (still in
                    // scope here; only moved onto the frontier below) — derive the
                    // stable id with no extra query.
                    let id = derive_stable_id(Some(&neighbor), &body);
                    // F9: record the traversing edge's confidence for this node
                    // (first edge wins — this is the winning-`bfs_rank` edge).
                    if let Some(c) = edge_confidence {
                        edge_confidence_by_cursor.insert(write_cursor as u64, c);
                    }
                    candidates.push(SearchHit {
                        id,
                        write_cursor: write_cursor as u64,
                        kind,
                        body,
                        score,
                        branch: SoftFallbackBranch::GraphArm,
                        // BLOCK-2: the session this fact-edge was extracted from.
                        source_id: edge_source_id.clone(),
                        ce_score: None,
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
    Ok((candidates, stats, edge_confidence_by_cursor))
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
    view: &ReadView,
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
        // The `?` placeholders above auto-number 1..=unique.len(), so the
        // validity instant takes the next positional slot.
        let now_idx = unique.len() + 1;
        let node_sql = view.node_sql("canonical_nodes", now_idx);
        // R-20-RV: with `include_superseded` a logical_id can match several
        // rows. `ORDER BY write_cursor` + last-write-wins into `found` resolves
        // the slot DETERMINISTICALLY to the most recent version, rather than
        // leaving it at the mercy of scan order.
        let sql = format!(
            "SELECT logical_id, kind, body, write_cursor
             FROM canonical_nodes
             WHERE logical_id IN ({placeholders}){node_sql}
             ORDER BY write_cursor"
        );
        let mut statement = tx.prepare(&sql)?;
        let mut binds: Vec<rusqlite::types::Value> =
            unique.iter().map(|s| rusqlite::types::Value::Text((*s).clone())).collect();
        if let Some(now) = view.now_param() {
            binds.push(rusqlite::types::Value::Integer(now));
        }
        let rows = statement.query_map(rusqlite::params_from_iter(binds.iter()), |row| {
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
    view: &ReadView,
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
    // R-20-RV/R-20-NV: the view's predicates replace the previously hard-coded
    // existence pair. The validity instant takes the positional slot AFTER the
    // predicate binds (?1 = kind, ?2..=?(1+n) = predicate values), so it is
    // `?{predicates.len() + 2}`. Positional `?N` is order-independent in SQLite,
    // so emitting it here — textually before the predicate clauses appended
    // below — is safe and unambiguous.
    let now_idx = predicates.len() + 2;
    let node_sql = view.node_sql("canonical_nodes", now_idx);
    let mut sql = format!(
        "SELECT logical_id, kind, body, write_cursor \
         FROM canonical_nodes \
         WHERE kind = ?1{node_sql} \
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
    let mut params: Vec<rusqlite::types::Value> = Vec::with_capacity(2 + predicates.len());
    params.push(rusqlite::types::Value::Text(kind.to_string()));
    for pred in predicates {
        params.push(pred.bind_value());
    }
    // Lands at index `now_idx` (= predicates.len() + 2), matching `?{now_idx}`
    // emitted by `ReadView::validity_sql`. Omitted entirely when the view
    // relaxes validity, in which case no `?{now_idx}` was emitted either.
    if let Some(now) = view.now_param() {
        params.push(rusqlite::types::Value::Integer(now));
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

/// Build the BFS CTE SQL for the given `direction`, under `view`.
///
/// Parameters (positional):
///   `?1` — root `logical_id`
///   `?2` — max_depth (`u32`, SDK-facing depth ceiling ≤ 3)
///   `?3` — R-20-NV node-validity instant (`:now` seam), emitted at EVERY node
///          position; omitted entirely when the view relaxes validity.
///
/// `LIMIT {GRAPH_NEIGHBORS_HARD_CAP}` appears on both the CTE and the final SELECT.
///
/// # Why one template instead of three
///
/// The three directions previously carried three hand-maintained copies of the
/// CTE, each repeating the node predicate at THREE positions (anchor, recursive
/// join, final projection) — nine hand-written copies in total. R-20-RV requires
/// a relax flag to apply at every one of them, and nine copies is exactly the
/// shape in which "it works on `Outgoing` but silently not on `Both`" hides. The
/// directions are folded into ONE template parameterised by the two things that
/// actually differ (the edge join condition and the traversed-to expression), so
/// `view.node_sql(...)` is written once per position and applying to all three
/// directions is structural rather than a thing to remember.
///
/// **TC-33: the `canonical_edges` temporal filter is now parameterised too.** It
/// was `datetime(e.t_invalid) > datetime('now')` inline, deliberately left alone
/// while edge validity was ISO-8601 TEXT. Edge timestamps are INTEGER epoch
/// seconds now, so the predicate is generated by [`edge_validity_sql`] and binds
/// the frozen instant at `?4` — no inline clock remains in this template.
fn build_bfs_sql(direction: TraversalDirection, view: &ReadView) -> String {
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
    //
    // `?3` is the node-validity instant. Positional (not named), so the repeated
    // occurrences across the three node positions all bind the SAME value once.
    const NOW_IDX: usize = 3;
    // TC-33: `?4` is the EDGE-validity instant, bound separately because the node
    // instant is `Option` (relaxed by `include_out_of_window`) while edge recency
    // is always applied.
    const EDGE_NOW_IDX: usize = 4;

    // The ONLY two things that differ between directions.
    let (edge_join, target_expr) = match direction {
        TraversalDirection::Outgoing => ("e.from_id = t.logical_id", "e.to_id"),
        TraversalDirection::Incoming => ("e.to_id = t.logical_id", "e.from_id"),
        TraversalDirection::Both => (
            "(e.from_id = t.logical_id OR e.to_id = t.logical_id)",
            "CASE WHEN e.from_id = t.logical_id THEN e.to_id ELSE e.from_id END",
        ),
    };

    // Position 1 (anchor), position 2 (recursive join), position 3 (final
    // projection) — the view is applied at all three, for every direction.
    let anchor_node = view.node_sql("n", NOW_IDX);
    let next_node = view.node_sql("next_n", NOW_IDX);
    let projection_node = view.node_sql("n", NOW_IDX);
    let edge_valid = edge_validity_sql("e", EDGE_NOW_IDX);

    format!(
        "WITH RECURSIVE
  traversal(logical_id, depth, visited) AS (
    SELECT n.logical_id, 0, char(30) || n.logical_id || char(30)
    FROM canonical_nodes n
    WHERE n.logical_id = ?1{anchor_node}
    UNION ALL
    SELECT {target_expr}, t.depth + 1, t.visited || {target_expr} || char(30)
    FROM traversal t
    JOIN canonical_edges e ON {edge_join}
    JOIN canonical_nodes next_n ON next_n.logical_id = {target_expr}{next_node}
    WHERE t.depth < ?2
      AND e.superseded_at IS NULL{edge_valid}
      AND instr(t.visited, char(30) || {target_expr} || char(30)) = 0
    LIMIT {cte_cap}
  )
SELECT DISTINCT n.logical_id, n.kind, n.body, n.write_cursor
FROM traversal tr
JOIN canonical_nodes n ON n.logical_id = tr.logical_id
WHERE tr.logical_id != ?1{projection_node}
LIMIT {cap}"
    )
}

/// Build the BFS CTE SQL for `search_expand` — identical to `build_bfs_sql`
/// but the final SELECT uses `GROUP BY` + `MIN(tr.depth)` so that each
/// expanded node carries its actual BFS distance from the root.
///
/// Returns 5 columns: logical_id, kind, body, write_cursor, min_depth.
fn build_bfs_with_depth_sql() -> String {
    let cap = GRAPH_NEIGHBORS_HARD_CAP;
    let cte_cap = cap * cap; // same multigraph-safe headroom as build_bfs_sql
                             // TC-33: `?1` anchor, `?2` depth ⇒ the edge `:now` binds at `?3`. This is a
                             // SECOND, separate BFS template — the edge-validity predicate has to be
                             // re-grounded here too or `search_expand` silently keeps the old semantics.
    let edge_valid = edge_validity_sql("e", 3);
    format!(
        "WITH RECURSIVE
  traversal(logical_id, depth, visited) AS (
    SELECT n.logical_id, 0, char(30) || n.logical_id || char(30)
    FROM canonical_nodes n
    WHERE n.logical_id = ?1 AND n.superseded_at IS NULL AND n.state = 'active'
    UNION ALL
    SELECT
      CASE WHEN e.from_id = t.logical_id THEN e.to_id ELSE e.from_id END,
      t.depth + 1,
      t.visited || CASE WHEN e.from_id = t.logical_id THEN e.to_id ELSE e.from_id END || char(30)
    FROM traversal t
    JOIN canonical_edges e ON (e.from_id = t.logical_id OR e.to_id = t.logical_id)
    JOIN canonical_nodes next_n
      ON next_n.logical_id = CASE WHEN e.from_id = t.logical_id THEN e.to_id ELSE e.from_id END
      AND next_n.superseded_at IS NULL AND next_n.state = 'active'
    WHERE t.depth < ?2
      AND e.superseded_at IS NULL{edge_valid}
      AND instr(t.visited,
            char(30) || CASE WHEN e.from_id = t.logical_id THEN e.to_id ELSE e.from_id END || char(30)) = 0
    LIMIT {cte_cap}
  )
SELECT n.logical_id, n.kind, n.body, n.write_cursor, MIN(tr.depth) AS min_depth
FROM traversal tr
JOIN canonical_nodes n ON n.logical_id = tr.logical_id
WHERE n.superseded_at IS NULL AND n.state = 'active'
  AND tr.logical_id != ?1
GROUP BY n.logical_id
LIMIT {cap}"
    )
}

/// 0.8.20 Slice 10b (R-20-NV) — the validity-boundary hook, on the DEFERRED
/// reader transaction.
///
/// Reports nodes whose `valid_from` and/or `valid_until` falls in the half-open
/// interval `(since, upper]`. Both bounds are BOUND parameters (`?1`, `?2`) —
/// the node-validity path never inlines `datetime('now')`.
///
/// The view's EXISTENCE conjunct applies (default: current + active rows only);
/// its VALIDITY conjunct deliberately does not, because the question is "did
/// this window cross a boundary", not "is this row valid now".
fn crossed_boundary_since_in_tx(
    reader: &mut Connection,
    since: i64,
    view: &ReadView,
) -> rusqlite::Result<Vec<BoundaryCrossing>> {
    // `now_param()` is None exactly when the view relaxes validity, which here
    // means "no upper bound on the interval".
    let upper = view.now_param().unwrap_or(i64::MAX);
    let existence = view.existence_sql("canonical_nodes");
    // `1 = 1` keeps the leading ` AND ` of `existence_sql` well-formed even when
    // every existence flag is relaxed and the conjunct is empty.
    let sql = format!(
        "SELECT logical_id, kind, body, write_cursor, valid_from, valid_until \
         FROM canonical_nodes \
         WHERE 1 = 1{existence} \
           AND logical_id IS NOT NULL \
           AND ( (valid_from IS NOT NULL AND valid_from > ?1 AND valid_from <= ?2) \
              OR (valid_until IS NOT NULL AND valid_until > ?1 AND valid_until <= ?2) ) \
         ORDER BY write_cursor"
    );
    let tx = reader.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;
    let mut statement = tx.prepare(&sql)?;
    let rows = statement.query_map(params![since, upper], |row| {
        let valid_from: Option<i64> = row.get(4)?;
        let valid_until: Option<i64> = row.get(5)?;
        Ok(BoundaryCrossing {
            node: NodeRecord {
                logical_id: row.get(0)?,
                kind: row.get(1)?,
                body: row.get(2)?,
                write_cursor: row.get::<_, i64>(3)? as u64,
            },
            became_valid_at: valid_from.filter(|t| *t > since && *t <= upper),
            became_invalid_at: valid_until.filter(|t| *t > since && *t <= upper),
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Slice 20 (G5) — execute a bounded BFS on the DEFERRED reader transaction.
/// Called inside the reader worker loop.
fn graph_neighbors_in_tx(
    reader: &mut Connection,
    root_logical_id: &str,
    depth: u32,
    direction: TraversalDirection,
    view: &ReadView,
) -> rusqlite::Result<Vec<NodeRecord>> {
    let sql = build_bfs_sql(direction, view);
    let tx = reader.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;
    let depth_i64 = depth as i64;
    let mut statement = tx.prepare(&sql)?;
    // ?1 root, ?2 depth, ?3 = the NODE validity instant, ?4 = the EDGE validity
    // instant (TC-33).
    //
    // ?3 is bound UNCONDITIONALLY even when the view relaxes node validity and
    // `build_bfs_sql` emitted no `?3`: the template still references ?4, so
    // SQLite's parameter count is 4 and the positions must not shift. Binding an
    // index the SQL never reads is harmless; letting ?4's value slide into ?3
    // would silently compare edge times against a placeholder.
    let frozen = (*view).freeze();
    let binds: Vec<rusqlite::types::Value> = vec![
        rusqlite::types::Value::Text(root_logical_id.to_string()),
        rusqlite::types::Value::Integer(depth_i64),
        rusqlite::types::Value::Integer(frozen.now_param().unwrap_or_default()),
        rusqlite::types::Value::Integer(frozen.edge_now()),
    ];
    let rows = statement.query_map(rusqlite::params_from_iter(binds.iter()), |row| {
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
             WHERE write_cursor = ?1 AND superseded_at IS NULL AND state = 'active'
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
                let cursor_i64 = hit.write_cursor as i64;
                let active: Option<i32> =
                    edge_stmt.query_row([cursor_i64], |row| row.get(0)).optional()?;
                if active.is_some() {
                    hit_logical_ids.push(Some(String::new())); // sentinel: keep hit, skip BFS
                } else {
                    hit_logical_ids.push(None); // superseded edge: drop
                }
            } else {
                let cursor_i64 = hit.write_cursor as i64;
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
        // TC-33: `?3` is the edge-validity instant. `search_expand` has no
        // `ReadView` in scope, so it uses the default (strict) semantics —
        // resolved ONCE here, not per root, so every root in one call agrees.
        let edge_now = current_epoch_seconds();
        for root_id in hit_logical_ids.iter().flatten().filter(|s| !s.is_empty()) {
            let neighbor_rows =
                bfs_stmt.query_map(params![root_id, depth_i64, edge_now], |row| {
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
    // The EXPLAIN index-usage gate measures the DEFAULT (strict) read path.
    let view = ReadView::default();
    let bfs_sql = build_bfs_sql(direction, &view);
    let explain_sql = format!("EXPLAIN QUERY PLAN {bfs_sql}");
    let tx = reader.transaction_with_behavior(rusqlite::TransactionBehavior::Deferred)?;
    let depth_i64 = depth as i64;
    let mut statement = tx.prepare(&explain_sql)?;
    // EXPLAIN QUERY PLAN returns rows: (id, parent, notused, detail).
    // We collect the `detail` column (index 3).
    // The strict view emits `?3` (the node-validity instant) at every node
    // position; TC-33 adds `?4`, the edge-validity instant.
    let frozen = view.freeze();
    let now = frozen.now_param().expect("the strict view always binds a validity instant");
    let rows = statement
        .query_map(params![root_logical_id, depth_i64, now, frozen.edge_now()], |row| {
            row.get::<_, String>(3)
        })?;
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
               AND canonical_edges.superseded_at IS NULL{edge_valid}
               AND _fathomdb_projection_terminal.write_cursor IS NULL
         ) ORDER BY write_cursor
         LIMIT {sql_limit}",
        // TC-33: `?1` is the projection cursor ⇒ the edge `:now` binds at `?2`.
        edge_valid = edge_validity_sql("canonical_edges", 2)
    );
    let mut statement = connection.prepare_cached(&sql)?;
    let rows = statement.query_map(params![cursor, current_epoch_seconds()], |row| {
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
    // 0.8.12 Slice A (R-CON-2 named default-ON blocker; Slice-20 codex §9
    // [P2]) — also exclude t_invalid-excluded (recency-consolidated) edges,
    // mirroring `next_pending_projection_jobs`'s edge arm. Required: without
    // this mirror, a rebuild-truncated t_invalid edge that
    // `next_pending_projection_jobs` now correctly skips would never gain a
    // `_fathomdb_projection_terminal` row, so this probe would flag it as
    // phantom-pending forever and `drain()`/`wait_for_idle` would hang.
    connection
        .query_row(
            // TC-33: no other parameter here ⇒ the edge `:now` binds at `?1`.
            &format!(
                "SELECT 1
             FROM canonical_edges ce
             LEFT JOIN _fathomdb_projection_terminal pt
               ON pt.write_cursor = ce.write_cursor
             WHERE ce.body IS NOT NULL
               AND ce.superseded_at IS NULL{}
               AND pt.write_cursor IS NULL
             LIMIT 1",
                edge_validity_sql("ce", 1)
            ),
            params![current_epoch_seconds()],
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
    row_kind: RowKind,
    /// fix-2 [P2] — whether this row is in the attribute projection's row set
    /// (`state = 'active' AND superseded_at IS NULL`, the exact `backfill_attribute`
    /// predicate). A projector-replay rebuild uses this to gate the attribute
    /// projection so it does not re-surface a pending / superseded node's values.
    /// Node-FTS / vector shadows are rebuilt for every row (their stale versions
    /// are excluded by the read-side lifecycle join, unchanged from before).
    attr_projected: bool,
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
        // 0.8.20 Slice 5a (R-20-E1) — registry-driven: re-tokenize EVERY
        // node-FTS projection, not just `search_index`. `search_index_v2` uses
        // the SAME tokenizer (`porter unicode61 remove_diacritics 2`), so it is
        // equally invalidated by a tokenizer-default upgrade; before this slice
        // it was neither cleared nor re-tokenized here. Edge FTS is out of scope
        // for this open-path repair (it postdates the step-11 upgrade and is
        // rebuilt by `rebuild_projections`).
        truncate_row_projections_in(connection, &[ProjectionClass::NodeFts])?;
        for row in &rows {
            project_canonical_node_row(
                connection,
                row.cursor,
                &row.kind,
                &row.body,
                row.row_kind,
                ProjectionPass::FtsOnly,
                // FtsOnly never touches the attribute store (predates step 24), so
                // `node_active` is inert here; forward the row's flag anyway (it is
                // the backfill's active-and-non-superseded predicate) so the field
                // has a reader in every build configuration.
                row.attr_projected,
            )?;
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
    // fix-2 [P2] — also read `state` + `superseded_at` so a replay rebuild can gate
    // the attribute projection to the backfill's row set. `attr_projected` mirrors
    // the exact `backfill_attribute` predicate (`state = 'active' AND
    // superseded_at IS NULL`): a NULL/foreign state is NOT 'active' and so is
    // excluded, identical to the SQL equality.
    let mut statement = connection.prepare(
        "SELECT write_cursor, kind, body, row_kind, state, superseded_at \
         FROM canonical_nodes ORDER BY write_cursor",
    )?;
    let rows = statement.query_map([], |row| {
        let state: Option<String> = row.get::<_, Option<String>>(4)?;
        let superseded_at: Option<i64> = row.get::<_, Option<i64>>(5)?;
        Ok(CanonicalNodeRow {
            cursor: row.get::<_, u64>(0)?,
            kind: row.get::<_, String>(1)?,
            body: row.get::<_, String>(2)?,
            row_kind: row_kind_from_column(&row.get::<_, String>(3)?),
            attr_projected: state.as_deref() == Some("active") && superseded_at.is_none(),
        })
    })?;
    rows.collect()
}

/// 0.8.20 Slice 5a — inverse of [`RowKind::as_str`] for the stored
/// `canonical_nodes.row_kind` column. An unrecognized spelling degrades to
/// `Leaf`, the column DEFAULT and the shape every pre-EXP-S row carries; that
/// keeps a projector replay behavior-identical to the pre-registry rebuild,
/// which ignored `row_kind` entirely.
fn row_kind_from_column(value: &str) -> RowKind {
    match value {
        "coverage" => RowKind::Coverage,
        "graph" => RowKind::Graph,
        _ => RowKind::Leaf,
    }
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
    // OPP-12 Phase-1 (0.8.19 Slice 10, design §3 gap-4) — `secure_delete=ON` at
    // EVERY open. The projection/vector-rewrite runtime connection performs
    // DELETEs (shadow-table rewrites), so its freed pages must be scrubbed too;
    // setting the pragma only on the writer left a GDPR-erasure leak here.
    connection.pragma_update(None, "secure_delete", "ON")?;
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

/// Cap sweep over the op-store mutation log: keeps the newest `cap` SWEEPABLE
/// rows, dropping the oldest by `id`.
///
/// **Pending-redaction exemption (0.8.20 Slice 5 fix-1).**
/// [`ERASURE_PENDING_REDACTION_COLLECTION`] is exempt on the same principle for a
/// stronger reason: that row is not a record of a discharged obligation but an
/// UNDISCHARGED one. Sweeping it would silently drop an erasure the engine still
/// owes, and the next retry would then report success with the leaked stable ids
/// still in the telemetry sink — exactly the R-20-E5 violation this mechanism
/// exists to prevent.
///
/// **Erasure-audit exemption (0.8.20 Slice 5b, design v5 §2 defect D-A;
/// HITL-ruled 2026-07-19: *"there must be an auditable record of deletion
/// event."*).** Rows in [`ERASURE_AUDIT_COLLECTIONS`] are excluded from BOTH the
/// count and the DELETE, and are therefore **never removed by retention
/// pressure**. Previously this swept `operational_mutations` cap-first,
/// oldest-`id`-first, with no collection filter — so the `excise_source_audit`
/// row proving an erasure occurred shared one retention pool with the very
/// payloads it must prove erased, and (being written before whatever workload
/// followed) was among the first evicted. Accountability is a distinct
/// obligation from erasure; a sweep must not silently discharge it.
///
/// Consequence of excluding audit rows from the count: `cap` is a cap on
/// SWEEPABLE rows, not on the physical table size. That is deliberate — the
/// alternative (counting exempt rows toward the cap) would let a growing audit
/// trail evict ordinary provenance ever more aggressively, and in the limit
/// leave nothing sweepable while the sweep churned every write.
fn enforce_provenance_retention(connection: &Connection, cap: u64) -> rusqlite::Result<()> {
    if cap == 0 {
        return Ok(());
    }
    // Static, engine-internal identifiers — no caller input reaches this SQL.
    let exempt = ERASURE_AUDIT_COLLECTIONS
        .iter()
        .copied()
        .chain(std::iter::once(ERASURE_PENDING_REDACTION_COLLECTION))
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let slack = cap.max(20) / 20;
    let upper = cap.saturating_add(slack.max(1));
    let count: u64 = connection.query_row(
        &format!(
            "SELECT COUNT(*) FROM operational_mutations
             WHERE collection_name NOT IN ({exempt})"
        ),
        [],
        |row| row.get(0),
    )?;
    if count <= upper {
        return Ok(());
    }
    let to_delete = count.saturating_sub(cap);
    connection.execute(
        &format!(
            "DELETE FROM operational_mutations
             WHERE id IN (
                 SELECT id FROM operational_mutations
                 WHERE collection_name NOT IN ({exempt})
                 ORDER BY id
                 LIMIT ?1
             )"
        ),
        [to_delete],
    )?;
    Ok(())
}

/// 0.8.20 Slice 5b (R-20-E6) — the prefixed stable ids
/// ([`IdSpace::to_prefixed`]) of the canonical rows an erasure verb is about to
/// delete, so they can be redacted from the telemetry sink.
///
/// Must be called INSIDE the erasing transaction and BEFORE the DELETEs — after
/// them the rows, and with them the `logical_id`/`body` the ids derive from, are
/// gone. Both queries take one bound parameter (`?1`), applied to nodes and
/// edges respectively; `derive_stable_id` reproduces exactly what
/// `capture_telemetry` wrote into `result_stable_ids`.
fn collect_erased_stable_ids(
    tx: &Connection,
    node_sql: &str,
    edge_sql: &str,
    bind: &str,
) -> Result<Vec<String>, EngineError> {
    let mut ids = Vec::new();
    for sql in [node_sql, edge_sql] {
        let mut stmt = tx.prepare(sql).map_err(|_| EngineError::Storage)?;
        let rows = stmt
            .query_map(params![bind], |row| {
                Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?))
            })
            .map_err(|_| EngineError::Storage)?;
        for row in rows {
            let (logical_id, body) = row.map_err(|_| EngineError::Storage)?;
            ids.push(
                derive_stable_id(logical_id.as_deref(), body.as_deref().unwrap_or(""))
                    .to_prefixed(),
            );
        }
    }
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

/// 0.8.20 Slice 5 fix-1 (codex §9 P2) — record, INSIDE the erasing transaction,
/// that a telemetry redaction is owed for `erased_stable_ids`.
///
/// Must be called in the same transaction as the DELETEs. That is the whole
/// point: "the rows are gone" and "a redaction is owed for them" then commit
/// atomically, so no crash or failure can leave the first true and the second
/// unrecorded. [`Engine::discharge_pending_redactions`] drains the queue and
/// deletes the entry only once the sink has actually been rewritten.
///
/// `record_key` is the VERB, never a stable id — the ids live in the payload,
/// which is deleted on discharge.
fn enqueue_pending_redaction(
    tx: &Connection,
    verb: &str,
    erased_stable_ids: &[String],
    write_cursor: u64,
) -> Result<(), EngineError> {
    if erased_stable_ids.is_empty() {
        return Ok(());
    }
    let payload =
        serde_json::json!({ "verb": verb, "erased_stable_ids": erased_stable_ids }).to_string();
    tx.execute(
        "INSERT INTO operational_mutations(
            collection_name, record_key, op_kind, payload_json, schema_id, write_cursor
         ) VALUES(?1, ?2, 'append', ?3, NULL, ?4)",
        params![ERASURE_PENDING_REDACTION_COLLECTION, verb, payload, write_cursor],
    )
    .map_err(|_| EngineError::Storage)?;
    Ok(())
}

/// 0.8.20 Slice 5b (R-20-E7) — the audit handle for an erased op-store record:
/// `SHA-256(collection + 0x1F + record_key)`, lowercase hex.
///
/// A record key is arbitrary caller-supplied text and may itself be the
/// identifier being erased, so a durable audit row must not echo it. `0x1F`
/// (ASCII unit separator) is the delimiter because it cannot appear in a
/// well-formed collection name, keeping the pairing unambiguous.
#[cfg(feature = "operator")]
fn digest_record_identity(collection: &str, record_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(collection.as_bytes());
    hasher.update([0x1f_u8]);
    hasher.update(record_key.as_bytes());
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
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

/// 0.8.18 Slice 5 (#5 vector-equivalence probe) — parse the committed 45-probe
/// fixture into an ordered `Vec<&str>` (one probe per non-empty, non-`#`-comment
/// line). Order is stable so `probe_ordinal` is deterministic across opens.
fn vector_equivalence_probes() -> Vec<&'static str> {
    VECTOR_EQUIVALENCE_PROBE_FIXTURE
        .lines()
        .map(str::trim_end)
        .filter(|line| {
            let t = line.trim_start();
            !t.is_empty() && !t.starts_with('#')
        })
        .collect()
}

/// 0.8.18 Slice 5 — outcome of the open-time #5 self-check.
struct VectorEquivalenceOutcome {
    dense_disabled: bool,
    reason: Option<String>,
}

/// 0.8.18 Slice 5 — embed one probe under panic isolation. The probe runs at
/// open time on the writer connection BEFORE the projection workers spawn, so a
/// caller-supplied embedder that PANICS (or returns an error / a wrong-dimension
/// vector) must never wedge `Engine::open`. A panic/error/shape-mismatch yields
/// `None`; the CALLERS then fail-SAFE (fix-1 DEFECT #1) — a `None` at population
/// or check time means the vector arm cannot be established/verified, so dense is
/// REFUSED (`dense_disabled=true`), never silently served. `Engine::open` still
/// succeeds (no wedge; ADR-0.6.0 Invariant-5 posture, mirrored open-side).
fn probe_embed(embedder: &dyn Embedder, text: &str, dimension: usize) -> Option<Vec<f32>> {
    let embedded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| embedder.embed(text)));
    match embedded {
        Ok(Ok(vector)) if vector.len() == dimension => Some(vector),
        _ => None,
    }
}

/// 0.8.18 Slice 5 (#5 vector-equivalence probe KEYSTONE) — the open-time
/// self-check. Per `dev/design/0.8.18-slice-0-vector-equivalence-publish-design.md`
/// §U1 + `dev/adr/ADR-0.8.18-vector-equivalence-self-check.md`.
///
/// Runs AFTER open-time mean-recovery/requantize + `ensure_vector_partition`
/// (U1-b) so it reads the FINAL live `mean_vec`. Two paths:
///
///  - **First vector-kind registration** (probe table empty): re-embed the 45
///    committed probes with the LIVE embedder and persist their **UN-centered
///    f32 reference vectors** + embedder identity (R-VEQ-1). Store f32 ONLY —
///    the P1 bits are NEVER persisted (U1-d). Returns `dense_disabled=false`.
///  - **Subsequent open** (probe table populated): re-embed the 45 probes and
///    assert BOTH dense-pipeline representations against the stored references:
///    **(P1)** the Phase-1 mean-centered `embedding_bin` sign-flip count via the
///    SAME `vec_quantize_binary(sign(x − mean_vec))` path as
///    `build_vector_phase1_sql` (floor = 0, exact); **(P2)** the un-centered
///    Phase-2 L2 (`vec_distance_l2` semantics) within `VECTOR_EQUIVALENCE_L2_EPSILON`.
///    Divergence beyond EITHER floor ⇒ `dense_disabled=true` (R-VEQ-2/3).
///
/// Mean-centering is gated by `identity_requires_mean_centering(identity)` ∧
/// `mean_pinned`, applied symmetrically to reference + reembed (un-centered
/// fallback otherwise; NoopEmbedder no-op) — R-VEQ-3c.
///
/// Fail-SAFE, never fail-open (0.8.18 Slice 5 fix-1, DEFECT #1): any inability to
/// RUN or VERIFY the probe — a probe embed that panics/errors/returns wrong-dim, a
/// malformed/missing reference row, an unreadable pinned mean, or a
/// `vec_quantize_binary`/L2 SQL failure — yields `dense_disabled=true` with a clear
/// reason (refuse the un-verifiable dense/fused arm; the text-only/FTS path still
/// serves). `Engine::open` still SUCCEEDS (never wedges on a panicking caller
/// embedder). The distinct-identity cross-vendor refusal (`check_embedder_profile`)
/// remains the PRIMARY gate; this probe is ADDITIVE-ONLY (R-VEQ-5), but on the
/// vector arm it fails CLOSED, not open — same-identity backend drift on an
/// un-verifiable arm is exactly what #5 must catch (R-VEQ-4 "loud typed refuse,
/// never silent").
fn run_vector_equivalence_probe(
    connection: &Connection,
    embedder: Option<&dyn Embedder>,
    identity: &EmbedderIdentity,
    mean_pinned: bool,
) -> VectorEquivalenceOutcome {
    let not_disabled = VectorEquivalenceOutcome { dense_disabled: false, reason: None };

    // No live embedder ⇒ no dense arm to guard (EmbedderChoice::None). The probe
    // is inert; dense writes/queries already fail with EmbedderNotConfigured.
    let Some(embedder) = embedder else { return not_disabled };

    // Gate: the probe only engages once the workspace has REGISTERED a vector
    // kind (`_fathomdb_vector_kinds` non-empty). A fresh workspace that has never
    // committed to vector indexing has no dense arm to guard yet, so the probe
    // does ZERO embed work at that open — this keeps `Engine::open` free of the
    // 45-probe re-embed on empty/vector-less workspaces (and inert for the
    // pathological single-session hang/panic embedder tests, which register their
    // kind AFTER open and never reopen).
    //
    // fix-1 DEFECT #4 — the baseline is established at OPEN, at the first open
    // where a vector kind already exists (population path below). This covers BOTH:
    //   (b) the v18→v19 UPGRADE with pre-existing vector kinds: the baseline is
    //       captured here, at the first v19 open, from the identity-matched
    //       embedder (identity is already gated by `check_embedder_profile`, so the
    //       baseline is the same *claimed* embedder; future backend drift is caught);
    //   (a) a vector kind registered POST-OPEN in a prior session: the baseline is
    //       captured at the NEXT open (this gate + population), again identity-gated.
    // It is deliberately NOT captured in the registering session's write path: a
    // write must NEVER block on the embedder (the async-projection invariant —
    // `ac_029_canonical_writes_complete_under_projection_stall` and the PR-9 embed
    // watchdog/thread-leak bounds), and 45 synchronous probe embeds there would
    // violate it and hang/degrade under a stalling embedder. Serving vector queries
    // in the registering session is SAFE regardless: the serving backend IS the
    // backend that built those vectors, so there is nothing to diverge from. The
    // residual — a same-*identity* backend that drifted between the registering
    // session and the next open is not retroactively caught — is IDENTICAL to the
    // accepted upgrade residual (R-VEQ-5 additive-only; U3 same-identity candle
    // CPU↔CUDA = 0/17280). See `dev/design/0.8.18-slice-5-vector-equivalence-probe.md`.
    let vector_kind_registered: bool = connection
        .query_row("SELECT EXISTS(SELECT 1 FROM _fathomdb_vector_kinds)", [], |r| r.get(0))
        .unwrap_or(false);
    if !vector_kind_registered {
        return not_disabled;
    }

    match probe_populate_or_check(connection, embedder, identity, mean_pinned) {
        Ok(()) => not_disabled,
        Err(reason) => VectorEquivalenceOutcome { dense_disabled: true, reason: Some(reason) },
    }
}

/// 0.8.18 Slice 5 — either PERSIST the baseline (probe table empty) or CHECK
/// against it (probe table populated). `Err(reason)` ⇒ refuse the dense arm
/// (`dense_disabled=true`); `Ok(())` ⇒ dense served. Fail-SAFE throughout.
fn probe_populate_or_check(
    connection: &Connection,
    embedder: &dyn Embedder,
    identity: &EmbedderIdentity,
    mean_pinned: bool,
) -> Result<(), String> {
    let probes = vector_equivalence_probes();
    if probes.is_empty() {
        // Fail-SAFE: the compiled-in probe fixture is empty ⇒ nothing to verify
        // the vector arm against. (Defensive; the fixture is drift-guarded
        // non-empty at 45 probes.)
        return Err(
            "vector-equivalence probe fixture is empty; cannot verify the dense arm".to_string()
        );
    }

    let existing: i64 = connection
        .query_row("SELECT COUNT(*) FROM _fathomdb_embed_probe", [], |r| r.get(0))
        .map_err(|e| format!("could not read the probe reference table: {e}; cannot verify"))?;

    if existing == 0 {
        // Populate, then CONFIRM the just-written baseline is complete before
        // enabling dense (fix-2 DEFECT #1 residual): a population that committed
        // a short/garbled set must never leave dense enabled on the same open.
        probe_populate_baseline(connection, embedder, identity, &probes)?;
        probe_check_against_baseline(connection, embedder, identity, mean_pinned, &probes)
    } else {
        probe_check_against_baseline(connection, embedder, identity, mean_pinned, &probes)
    }
}

/// 0.8.18 Slice 5 — FIRST vector-kind registration: persist the 45 UN-centered
/// f32 reference vectors (R-VEQ-1; store f32 ONLY, never the P1 bits — U1-d).
/// Fail-SAFE (fix-1 DEFECT #1): if the embedder cannot produce EVERY reference
/// (panic/error/wrong-dim) no baseline can be established ⇒ `Err` (refuse dense).
/// The inserts run in a single transaction so a partial/mismatched set is NEVER
/// persisted (rolled back on any error).
fn probe_populate_baseline(
    connection: &Connection,
    embedder: &dyn Embedder,
    identity: &EmbedderIdentity,
    probes: &[&str],
) -> Result<(), String> {
    let dimension = identity.dimension as usize;
    // Embed ALL probes first; a single failure aborts population (store nothing).
    let mut rows: Vec<(i64, &str, Vec<f32>)> = Vec::with_capacity(probes.len());
    for (ordinal, probe) in probes.iter().enumerate() {
        match probe_embed(embedder, probe, dimension) {
            Some(vec) => rows.push((ordinal as i64, probe, vec)),
            None => {
                return Err(format!(
                    "embedder failed to produce a reference vector for probe {ordinal}; \
                     cannot establish a vector-equivalence baseline (dense arm refused)"
                ));
            }
        }
    }
    // Atomic insert — a partial reference set is never persisted (rollback on
    // any error, so a later open cleanly retries population).
    let tx = connection
        .unchecked_transaction()
        .map_err(|e| format!("could not open the probe-baseline transaction: {e}"))?;
    for (ordinal, probe, vec) in &rows {
        let blob = encode_vector_blob(vec);
        tx.execute(
            "INSERT OR REPLACE INTO _fathomdb_embed_probe(
                 probe_ordinal, probe_text, reference_vec,
                 embedder_name, embedder_revision, dim
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![ordinal, probe, blob, identity.name, identity.revision, identity.dimension],
        )
        .map_err(|e| format!("could not persist the probe baseline: {e}"))?;
    }
    tx.commit().map_err(|e| format!("could not commit the probe baseline: {e}"))?;
    Ok(())
}

/// 0.8.18 Slice 5 — SUBSEQUENT open: re-embed the 45 probes and assert BOTH
/// dense-pipeline representations against the stored references — **(P1)** the
/// mean-centered `embedding_bin` sign-flip count (floor 0, exact) and **(P2)** the
/// un-centered Phase-2 L2 (within `VECTOR_EQUIVALENCE_L2_EPSILON`). Fail-SAFE
/// (fix-1 DEFECT #1): a probe embed that panics/errors/returns wrong-dim, a
/// malformed/missing reference row, an unreadable pinned mean, or a
/// `vec_quantize_binary`/L2 SQL failure each ⇒ `Err` (cannot verify ⇒ refuse
/// dense), never a silent skip-and-serve.
///
/// fix-2 (DEFECT #1 residual): BEFORE the divergence check, the STORED baseline is
/// validated to be EXACTLY the committed probe set — the expected row count, a
/// contiguous 0-based `probe_ordinal` per committed probe, each `probe_text` equal
/// to the committed fixture text at that ordinal, each `reference_vec` a well-formed
/// `4 * dim` f32 blob, and the stored embedder identity/dim matching the current
/// one. This closes the partial-baseline / external-tamper fail-open (a 44-of-45
/// table, or a re-attributed/mangled row, previously verified only the rows present
/// or re-embedded a tampered `probe_text` against itself). Any mismatch ⇒ `Err`.
fn probe_check_against_baseline(
    connection: &Connection,
    embedder: &dyn Embedder,
    identity: &EmbedderIdentity,
    mean_pinned: bool,
    probes: &[&str],
) -> Result<(), String> {
    let dimension = identity.dimension as usize;

    // Resolve the live mean. Fail-SAFE: if centering is required + pinned but the
    // mean cannot be read, we cannot reproduce `embedding_bin` ⇒ refuse (P1
    // un-verifiable). NoopEmbedder / no-pin ⇒ un-centered on BOTH sides (R-VEQ-3c).
    let mean_vec = if identity_requires_mean_centering(identity) && mean_pinned {
        match read_pinned_mean_vec(connection, identity.dimension) {
            Ok(Some(mean)) => Some(mean),
            Ok(None) => {
                return Err("mean-centering is required and pinned but mean_vec is absent; \
                     cannot verify P1 (dense arm refused)"
                    .to_string());
            }
            Err(_) => {
                return Err(
                    "could not read the pinned mean_vec; cannot verify P1 (dense arm refused)"
                        .to_string(),
                );
            }
        }
    } else {
        None
    };

    let mut stmt = connection
        .prepare(
            "SELECT probe_ordinal, probe_text, reference_vec, embedder_name, embedder_revision, dim \
             FROM _fathomdb_embed_probe ORDER BY probe_ordinal",
        )
        .map_err(|e| format!("could not read the stored probe references: {e}; cannot verify"))?;
    let stored: Vec<(i64, String, Vec<u8>, String, String, i64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
        .map_err(|e| format!("could not read the stored probe references: {e}; cannot verify"))?;

    // fix-2 (DEFECT #1 residual) — COMPLETENESS validation of the STORED baseline.
    // `COUNT(*) > 0` is NOT proof of a complete, trustworthy baseline: a partially
    // populated or externally-tampered probe table (44 of 45 rows, a gap/dupe in the
    // ordinals, a mangled reference blob, a mismatched probe_text, or a foreign
    // embedder identity) is UNVERIFIABLE stored state. The prior code re-embedded
    // the STORED probe_text and compared it to its OWN reference, so a tampered
    // probe_text verified against itself and a short table verified only the rows
    // present — both fail-OPEN. Atomic population stops the ENGINE from writing a
    // partial set; this closes external corruption, a manual edit, and a future
    // migration bug the engine did not author. Any mismatch ⇒ fail CLOSED (dense
    // refused); the text-only/FTS path still serves. The stored baseline must be
    // EXACTLY the committed probe set, in order, under the current identity.
    if stored.len() != probes.len() {
        return Err(format!(
            "the probe reference table has {} rows but the committed fixture defines {}; \
             the stored baseline is incomplete or corrupt — cannot verify the dense arm (refused)",
            stored.len(),
            probes.len()
        ));
    }
    for (idx, (ordinal, probe_text, ref_blob, name, revision, dim)) in stored.iter().enumerate() {
        // Contiguous 0-based ordinals, one per committed probe (no gaps/dupes).
        if *ordinal != idx as i64 {
            return Err(format!(
                "probe reference ordinals are non-contiguous (row {idx} carries ordinal {ordinal}); \
                 the stored baseline is corrupt — cannot verify the dense arm (refused)"
            ));
        }
        // The stored text MUST be the committed fixture text at this ordinal —
        // otherwise a tampered probe_text re-embeds and verifies against ITSELF,
        // masking drift (the exact fail-open this fix closes).
        if probe_text != probes[idx] {
            return Err(format!(
                "probe reference {ordinal} text does not match the committed fixture; \
                 the stored baseline is tampered or corrupt — cannot verify the dense arm (refused)"
            ));
        }
        // Well-formed f32[dim] reference (4*dim little-endian bytes).
        if ref_blob.len() != dimension * 4 {
            return Err(format!(
                "probe reference {ordinal} is malformed (len {} != {}); \
                 cannot verify the dense arm (refused)",
                ref_blob.len(),
                dimension * 4
            ));
        }
        // The stored embedder identity/dim must match the CURRENT expected identity
        // (defence-in-depth beyond `check_embedder_profile`: catches a baseline row
        // re-attributed to a foreign embedder by external edit/migration).
        if *dim != identity.dimension as i64
            || name != &identity.name
            || revision != &identity.revision
        {
            return Err(format!(
                "probe reference {ordinal} was captured under embedder {name}/{revision}/dim={dim} \
                 but the current embedder is {}/{}/dim={}; the stored baseline does not match — \
                 cannot verify the dense arm (refused)",
                identity.name, identity.revision, identity.dimension
            ));
        }
    }

    let mut total_flips: u64 = 0;
    let mut max_l2: f32 = 0.0;
    let mut worst_probe: Option<String> = None;

    for (ordinal, probe_text, ref_blob, _, _, _) in &stored {
        let reference = decode_vector_blob(ref_blob);
        let reembed = probe_embed(embedder, probe_text, dimension).ok_or_else(|| {
            format!(
                "embedder failed/panicked re-embedding probe {ordinal}; \
                 cannot verify the dense arm (refused)"
            )
        })?;

        // (P2) un-centered L2 — `vec_distance_l2(embedding, vec_f32(query))`.
        let l2 = l2_distance(&reembed, &reference);
        if l2 > max_l2 {
            max_l2 = l2;
            worst_probe = Some(probe_text.clone());
        }

        // (P1) mean-centered Phase-1 flip count — same
        // `vec_quantize_binary(sign(x − mean_vec))` path as build_vector_phase1_sql.
        let (ref_c, reembed_c) = match &mean_vec {
            Some(mean) => (subtract_mean(&reference, mean), subtract_mean(&reembed, mean)),
            None => (reference.clone(), reembed.clone()),
        };
        let ref_bits = quantize_binary_via_sql(connection, &ref_c).ok_or_else(|| {
            format!("vec_quantize_binary SQL failed for probe {ordinal}; cannot verify P1")
        })?;
        let reembed_bits = quantize_binary_via_sql(connection, &reembed_c).ok_or_else(|| {
            format!("vec_quantize_binary SQL failed for probe {ordinal}; cannot verify P1")
        })?;
        total_flips = total_flips.saturating_add(hamming_bytes(&ref_bits, &reembed_bits));
    }

    let p1_tripped = total_flips > VECTOR_EQUIVALENCE_P1_FLIP_FLOOR;
    let p2_tripped = max_l2 > VECTOR_EQUIVALENCE_L2_EPSILON;
    if p1_tripped || p2_tripped {
        let probe_hint = worst_probe.as_deref().unwrap_or("<unknown>");
        return Err(format!(
            "P1 mean-centered embedding_bin flips={total_flips} (floor={VECTOR_EQUIVALENCE_P1_FLIP_FLOOR}), \
             P2 max un-centered L2={max_l2:.3e} (epsilon={VECTOR_EQUIVALENCE_L2_EPSILON:.3e}); \
             worst probe {probe_hint:?}"
        ));
    }
    Ok(())
}

/// 0.8.18 Slice 5 — un-centered Euclidean (L2) distance, matching the
/// `vec_distance_l2` semantics used by the Phase-2 rerank.
fn l2_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| (x - y) * (x - y)).sum::<f32>().sqrt()
}

/// 0.8.18 Slice 5 — produce the packed 1-bit `embedding_bin` blob for a (possibly
/// mean-centered) f32 vector via the SAME SQL `vec_quantize_binary` the production
/// Phase-1 path uses, so the probe's bits are byte-equal to the engine's
/// `embedding_bin` production. `None` on any SQL/serialization error.
fn quantize_binary_via_sql(connection: &Connection, vector: &[f32]) -> Option<Vec<u8>> {
    let json = serde_json::to_string(vector).ok()?;
    connection
        .query_row("SELECT vec_quantize_binary(vec_f32(?1))", [json], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .ok()
}

/// 0.8.18 Slice 5 — Hamming distance (differing bit count) between two equal-length
/// packed bit blobs. Unequal lengths ⇒ count every bit of the length delta as
/// differing (a shape divergence is a divergence).
fn hamming_bytes(a: &[u8], b: &[u8]) -> u64 {
    let common = a.len().min(b.len());
    let mut flips: u64 = 0;
    for i in 0..common {
        flips += u64::from((a[i] ^ b[i]).count_ones());
    }
    let extra = a.len().abs_diff(b.len());
    flips + (extra as u64) * 8
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
    // digest 0.11 returns `hybrid_array::Array`, which (unlike the old
    // `GenericArray`) does not implement `LowerHex`. Format the bytes
    // explicitly — byte-identical lowercase, zero-padded hex to the prior
    // `{:x}` rendering, preserving the load-bearing logical-id derivation.
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Cause-A (0.8.11.2) / C-2 (0.8.19, TC-8) — derive the typed **stable hit-id**
/// ([`IdSpace`]) carried on [`SearchHit::id`] for cross-session real-gold keying.
///
/// The stable id is the active canonical node's `logical_id` — the post-G0
/// supersession-stable identity, preserved across re-projection/re-ingest by the
/// tombstone-then-insert contract (whereas the engine-internal `write_cursor` is
/// reassigned on every re-ingest). When `logical_id` is NULL — the doc-seeded
/// node case, the *dominant* corpus hit type today — we fall back to a content
/// hash of the body so doc hits still carry a re-ingest-survivable key.
///
/// The result is a typed [`IdSpace`]; its `to_prefixed()` reproduces the pre-C-2
/// `stable_id` string byte-for-byte so real-gold keying is a no-op:
/// - [`IdSpace::logical`] (`"l:<logical_id>"`) — entities + edges (graph-arm,
///   vector-node, and edge hits when `logical_id` is present);
/// - [`IdSpace::content`] (`"h:<sha256(body)>"`) — doc nodes with NULL
///   `logical_id`, and any branch that cannot cheaply resolve a `logical_id`.
///
/// Behaviour-neutral: the value never participates in ranking/scoring (same
/// additive posture as `source_id` / `ce_score`).
fn derive_stable_id(logical_id: Option<&str>, body: &str) -> IdSpace {
    match logical_id {
        Some(lid) if !lid.is_empty() => IdSpace::logical(lid),
        _ => {
            let mut hasher = Sha256::new();
            hasher.update(body.as_bytes());
            IdSpace::content(
                hasher.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>(),
            )
        }
    }
}

/// fix-34 [P2]: dedup a batch of [`PreparedWrite`]s by `logical_id`, keeping the
/// first occurrence. Shared by the entity and edge arms of the BYO-LLM ingest
/// path so a harness that returns the same node/edge twice in one response does
/// not write a row that immediately supersedes its sibling.
///
/// **TC-32 (0.8.20) — single-provenance entity dedupe is INTENTIONAL and
/// ACCEPTED.** Because dedupe keeps the FIRST occurrence, same-name entities
/// collapse onto one `logical_id` row that carries only the FIRST document's
/// `source_id`; erasing a later document therefore does not remove the shared
/// entity row. The HITL has ruled this acceptable for now and explicitly
/// declined a multi-source-provenance model. Tracked as TC-32 — do not "fix"
/// this by changing dedupe behaviour without a fresh decision.
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

/// 0.8.6 Slice 5 (ADR-0.8.6) — the family of caller-supplied provider tasks that
/// ride the one NDJSON-over-stdio transport. Each task maps to a wire protocol
/// string `fathomdb.<task>.v1` and a task discriminator name. `Extract` shipped
/// in 0.8.6; `Consolidate` (0.8.12 Slice 15, OPP-2) is the SECOND consumer of
/// this one transport — it adds only a variant, a payload, and an `EngineError`
/// leaf, WITHOUT a second handshake or a second transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderTask {
    Extract,
    /// 0.8.12 Slice 15 (OPP-2, ADR-0.8.12) — consolidation / recency provider.
    Consolidate,
}

impl ProviderTask {
    /// The wire task discriminator, e.g. `"extract"`. Used for `supported_tasks`
    /// negotiation and as the request envelope `type`.
    fn name(self) -> &'static str {
        match self {
            ProviderTask::Extract => "extract",
            ProviderTask::Consolidate => "consolidate",
        }
    }

    /// The protocol string FathomDB sends in `hello`/requests and requires in
    /// `ready`. For `Extract` this is the UNCHANGED `fathomdb.extract.v1` —
    /// byte-identical back-compat for existing ELPS harnesses (ADR-0.8.6 §2.1).
    /// For `Consolidate` it is `fathomdb.consolidate.v1` (ADR-0.8.12 §2).
    fn protocol(self) -> &'static str {
        match self {
            ProviderTask::Extract => "fathomdb.extract.v1",
            ProviderTask::Consolidate => "fathomdb.consolidate.v1",
        }
    }
}

/// 0.8.6 Slice 5 (ADR-0.8.6) — an open provider transport session: the spawned
/// caller subprocess, the buffered stdin writer, the detached stdout-drain
/// channel, the bounded-recv timeout, and the negotiated handshake state
/// (`model` provenance + `max_docs_per_request`). One session serves one task
/// family; the `request`/framing is identical across tasks. `Drop` reaps the
/// child (sends stdin EOF via the writer field's own drop, then kill/wait),
/// replacing the prior explicit outer kill/wait.
struct ProviderSession {
    task: ProviderTask,
    child: std::process::Child,
    writer: std::io::BufWriter<std::process::ChildStdin>,
    line_rx: Receiver<std::io::Result<String>>,
    io_timeout: Duration,
    /// `ready.model`, recorded as output-row provenance (`extractor_model_id`).
    model: Option<String>,
    max_docs_per_request: usize,
}

impl Drop for ProviderSession {
    fn drop(&mut self) {
        // The detached stdout-drain thread exits when the child's stdout closes;
        // kill() guarantees that even for a child that ignores stdin EOF. The
        // `writer` field drops after this (declaration order) sending EOF too.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl ProviderSession {
    /// Run the `hello` → `ready` handshake and `supported_tasks` negotiation.
    /// Validates protocol + schema_version (fix-23 [P2]); rejects a zero
    /// `max_docs_per_request` (fix-1 [P2]); and, when the harness advertises
    /// `supported_tasks`, refuses to proceed unless this session's task is in it.
    /// When `supported_tasks` is absent, the harness is assumed to serve the
    /// requested task (back-compat: existing extract-only harnesses unchanged).
    fn handshake(&mut self) -> Result<(), EngineError> {
        let protocol = self.task.protocol();
        let hello = serde_json::json!({
            "protocol": protocol,
            "type": "hello",
            "schema_version": 1,
        });
        let hello_line = serde_json::to_string(&hello).map_err(|_| EngineError::Extractor)?;
        writeln!(self.writer, "{hello_line}").map_err(|_| EngineError::Extractor)?;
        self.writer.flush().map_err(|_| EngineError::Extractor)?;

        let line = recv_extractor_line(&self.line_rx, self.io_timeout)?;
        let ready: Value = serde_json::from_str(line.trim()).map_err(|_| EngineError::Extractor)?;
        // fix-23 [P2]: validate protocol + schema_version in the ready message per ADR.
        if ready.get("type").and_then(|v| v.as_str()) != Some("ready")
            || ready.get("protocol").and_then(|v| v.as_str()) != Some(protocol)
            || ready.get("schema_version").and_then(|v| v.as_u64()) != Some(1)
        {
            return Err(EngineError::Extractor);
        }

        // 0.8.6 Slice 5 (ADR-0.8.6 §2.2): additive, optional `supported_tasks`
        // negotiation. If present, the harness must advertise this session's task
        // or FathomDB refuses to dispatch it. If absent, default to "serves the
        // requested task" so extract-only harnesses keep working unchanged.
        if let Some(supported) = ready.get("supported_tasks").and_then(|v| v.as_array()) {
            let task_name = self.task.name();
            let advertised = supported.iter().any(|t| t.as_str() == Some(task_name));
            if !advertised {
                return Err(EngineError::Extractor);
            }
        }

        self.model = ready.get("model").and_then(|v| v.as_str()).map(|s| s.to_string());
        let max_docs =
            ready.get("max_docs_per_request").and_then(|v| v.as_u64()).unwrap_or(8) as usize;
        // fix-1 [P2]: reject zero max_docs_per_request to prevent chunks(0) panic.
        if max_docs == 0 {
            return Err(EngineError::Extractor);
        }
        self.max_docs_per_request = max_docs;
        Ok(())
    }

    /// Send one framed request for this session's task and receive its matching
    /// response. `payload` carries the task-specific fields; the envelope keys
    /// (`protocol`, `type`, `request_id`) are added here. The response must have
    /// `type == "result"` and a matching `request_id` (fix-24 [P2]); anything
    /// else (error, wrong id, missing type) is a protocol fault. For `Extract`
    /// the serialized request bytes are identical to the pre-0.8.6 path (serde_json
    /// serializes map keys sorted, independent of insertion order).
    fn request(
        &mut self,
        request_id: &str,
        payload: Vec<(String, Value)>,
    ) -> Result<Value, EngineError> {
        let mut req = serde_json::Map::new();
        req.insert("protocol".to_string(), Value::from(self.task.protocol()));
        req.insert("type".to_string(), Value::from(self.task.name()));
        req.insert("request_id".to_string(), Value::from(request_id));
        for (k, v) in payload {
            req.insert(k, v);
        }
        let req_line =
            serde_json::to_string(&Value::Object(req)).map_err(|_| EngineError::Extractor)?;
        writeln!(self.writer, "{req_line}").map_err(|_| EngineError::Extractor)?;
        self.writer.flush().map_err(|_| EngineError::Extractor)?;

        let result_line = recv_extractor_line(&self.line_rx, self.io_timeout)?;
        let result: Value =
            serde_json::from_str(result_line.trim()).map_err(|_| EngineError::Extractor)?;
        let resp_type = result.get("type").and_then(|v| v.as_str());
        let resp_id = result.get("request_id").and_then(|v| v.as_str());
        if resp_type != Some("result") || resp_id != Some(request_id) {
            return Err(EngineError::Extractor);
        }
        Ok(result)
    }
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
        PreparedWrite::Node { kind, body, logical_id, valid_from, valid_until, .. } => {
            if kind.trim().is_empty() || body.trim().is_empty() {
                return Err(EngineError::WriteValidation);
            }
            // 0.8.20 Slice 15b (TC-34) — the validity window is HALF-OPEN
            // `[valid_from, valid_until)`, so a pair with `from >= until` selects
            // no instant at all: the row would be written but no default read
            // could ever return it. Silently accepting that is a trap, so it is a
            // typed refusal carrying the offending bounds. `InvalidArgument` (not
            // the message-less `WriteValidation`) is deliberate — a caller has to
            // be able to tell WHICH pair was rejected, and it already maps to
            // `InvalidArgumentError` in both bindings.
            //
            // Only the PAIR can be empty. A one-sided window is unbounded on the
            // missing side and can never be empty, so it is never refused.
            if let (Some(from), Some(until)) = (valid_from, valid_until) {
                if from >= until {
                    return Err(EngineError::InvalidArgument {
                        msg: format!(
                            "invalid validity window: valid_from ({from}) must be strictly less \
                             than valid_until ({until}); the window is half-open \
                             [valid_from, valid_until), so this pair can never match any instant"
                        ),
                    });
                }
            }
            // R-20-E3: `source_id` needs no emptiness check here — `SourceId`
            // cannot hold an empty or reserved id, so the check has moved from
            // this branch into the type's constructor.
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
        PreparedWrite::Edge { kind, from, to, logical_id, t_valid, t_invalid, .. } => {
            if kind.trim().is_empty() || from.trim().is_empty() || to.trim().is_empty() {
                return Err(EngineError::WriteValidation);
            }
            // Reject char(30) in from/to: these become from_id/to_id in canonical_edges
            // and appear in BFS visited strings — an \x1e there would corrupt the guard.
            if from.contains('\x1e') || to.contains('\x1e') {
                return Err(EngineError::WriteValidation);
            }
            // R-20-E3: see the Node branch — emptiness is a `SourceId` invariant.
            if let Some(logical_id) = logical_id {
                if logical_id.is_empty() || logical_id.contains('\x1e') {
                    return Err(EngineError::WriteValidation);
                }
            }
            // TC-33 fix-1 (codex §9 P2) — an epoch SQLite cannot render to
            // ISO-8601 must be UNSTORABLE. The governed integer surface is the
            // only way to reach one (inbound ISO normalisation maxes at year
            // 9999), so this write boundary is where it is stopped, before it
            // can render to a silent `null` on the consolidation wire and
            // resurrect an invalidated edge. Structural primary layer; the
            // render site keeps a defensive hard-assert as the backstop.
            reject_unrenderable_edge_epoch(connection, "t_valid", *t_valid)?;
            reject_unrenderable_edge_epoch(connection, "t_invalid", *t_invalid)?;
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

/// 0.8.20 Slice 15d fix-1 finding 2 [P2] — the active (non-superseded) NODE
/// cursors for a `logical_id`, collected BEFORE the tombstone-then-insert
/// supersession UPDATE so the caller can purge the about-to-be-superseded row's
/// row-owned attribute projections. Mirrors [`prior_edge_cursors_by_logical_id`].
/// The partial-unique-active index means this is at most one cursor; a `Vec`
/// keeps it robust and symmetric with the edge path.
fn prior_node_cursors_by_logical_id(
    tx: &rusqlite::Transaction<'_>,
    logical_id: &str,
) -> rusqlite::Result<Vec<i64>> {
    let mut s = tx.prepare_cached(
        "SELECT write_cursor FROM canonical_nodes \
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

/// EXP-S (0.8.14 Slice 5, D2) — the set of coexisting indexes a `row_kind`
/// projects into. `fts` = the FTS index (`search_index`), written SYNCHRONOUSLY
/// in the write transaction; `vector` = the vec0 vector index, written
/// ASYNCHRONOUSLY by the projection worker pool (and additionally gated per
/// doc-type `kind` by [`kind_is_vector_indexed`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct IndexTargetSet {
    fts: bool,
    vector: bool,
}

/// 0.8.20 Slice 5a (R-20-E1) — the class a row-owned projection table belongs
/// to, so the four maintenance sites can each truncate exactly the subset they
/// own without re-deriving a hand-rolled table list.
///
/// - `NodeFts` — same-txn lexical projection of a canonical NODE body.
/// - `EdgeFts` — same-txn lexical projection of a canonical EDGE body.
/// - `Vector` — the async vec0 materialization (written by the embed worker,
///   not by the write path — see [`project_canonical_node_row`]).
/// - `Readiness` — the terminal-cursor bookkeeping that lets
///   `advance_projection_cursor` walk past a row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectionClass {
    NodeFts,
    EdgeFts,
    Vector,
    Readiness,
    /// 0.8.20 Slice 15d (R-20-EAV) — the EAV attribute store (`filterable` +
    /// the value-at-rest for `searchable`). Same-transaction, row-owned.
    Attribute,
    /// 0.8.20 Slice 15d (R-20-EAV) — the property-FTS5 shadow of attribute
    /// values (`searchable→FTS`). Same-transaction, row-owned.
    PropertyFts,
}

/// 0.8.20 Slice 5a (R-20-E1) — one ROW-OWNED projection table: a shadow whose
/// rows are 1:1 with a canonical row's `write_cursor` and therefore MUST die
/// with that row.
#[derive(Clone, Copy, Debug)]
struct RowOwnedProjection {
    /// Table name. `'static` and never caller-derived: safe to interpolate.
    table: &'static str,
    /// The column carrying the owning canonical row's `write_cursor`. For the
    /// vec0 table this is `rowid` — vec0 rowid IS the write_cursor (see the
    /// `_fathomdb_vector_rows.write_cursor UNIQUE` identity).
    cursor_column: &'static str,
    class: ProjectionClass,
}

/// 0.8.20 Slice 5a (R-20-E1) — **the** registry of row-owned projections.
///
/// Every table here is 1:1 with a canonical `write_cursor` and is erased by
/// [`erase_row_projections`] whenever that canonical row is erased. Adding a
/// projection table WITHOUT registering it here re-opens the defect this slice
/// closes (`search_index_v2` was written by one site and deleted by one site,
/// out of five that maintain projections — so `excise_source` left the erased
/// body on disk in a content-storing FTS5 table). The `guard_row_owned_registry`
/// unit test introspects `sqlite_master` and fails if a `write_cursor`-keyed
/// table is missing from this list.
///
/// **NOT here, deliberately (design v5 §1.1): `_fathomdb_projection_state`.**
/// That table is KIND-owned — keyed by `kind`, holding a per-kind enqueue
/// watermark. Erasing one row must NOT rewind a whole kind's watermark, so it
/// must never be deleted per-cursor. A rebuild resets it deliberately; erasure
/// leaves it alone.
const ROW_OWNED_PROJECTIONS: &[RowOwnedProjection] = &[
    RowOwnedProjection {
        table: "search_index",
        cursor_column: "write_cursor",
        class: ProjectionClass::NodeFts,
    },
    RowOwnedProjection {
        table: "search_index_v2",
        cursor_column: "write_cursor",
        class: ProjectionClass::NodeFts,
    },
    RowOwnedProjection {
        table: "search_index_edges",
        cursor_column: "write_cursor",
        class: ProjectionClass::EdgeFts,
    },
    RowOwnedProjection {
        table: "vector_default",
        cursor_column: "rowid",
        class: ProjectionClass::Vector,
    },
    RowOwnedProjection {
        table: "_fathomdb_vector_rows",
        cursor_column: "write_cursor",
        class: ProjectionClass::Vector,
    },
    RowOwnedProjection {
        table: "_fathomdb_projection_terminal",
        cursor_column: "write_cursor",
        class: ProjectionClass::Readiness,
    },
    // 0.8.20 Slice 15d (R-20-EAV) — the EAV attribute store and its property-FTS
    // shadow both hold declared attribute VALUES at rest (potential PII), keyed
    // 1:1 with the owning node's write_cursor. They MUST be reachable by
    // `purge`/`excise_source`: registering them here is what makes
    // `erase_row_projections` delete them without a hand-rolled list (an
    // unregistered content-storing table is exactly the `search_index_v2` leak
    // class this registry closes). The `guard_row_owned_registry` unit test
    // FAILS if either is left unregistered.
    RowOwnedProjection {
        table: "canonical_attributes",
        cursor_column: "write_cursor",
        class: ProjectionClass::Attribute,
    },
    RowOwnedProjection {
        table: "property_search_index",
        cursor_column: "write_cursor",
        class: ProjectionClass::PropertyFts,
    },
];

/// 0.8.20 Slice 5a (R-20-E1) — erase EVERY row-owned projection for one
/// canonical `write_cursor`. Returns the number of shadow rows deleted.
///
/// This is the single erasure primitive: `purge_inner` and `excise_source_inner`
/// both call it, so a new projection table becomes erasable by registering it in
/// [`ROW_OWNED_PROJECTIONS`] — not by remembering to patch two hand-rolled
/// delete lists (the omission that left erased bodies in `search_index_v2`).
fn erase_row_projections(tx: &Connection, write_cursor: i64) -> rusqlite::Result<u64> {
    let mut deleted: u64 = 0;
    for projection in ROW_OWNED_PROJECTIONS {
        let sql =
            format!("DELETE FROM {} WHERE {} = ?1", projection.table, projection.cursor_column);
        deleted = deleted.saturating_add(tx.execute(&sql, [write_cursor])? as u64);
    }
    Ok(deleted)
}

/// 0.8.20 Slice 15d fix-1 finding 2 [P2] — purge the row-owned projections in
/// `classes` for ONE canonical `write_cursor`. Same registry-driven mechanism as
/// [`erase_row_projections`] (iterate [`ROW_OWNED_PROJECTIONS`], delete by the
/// declared cursor column) but scoped to a class SUBSET, so the write path can
/// drop a SUPERSEDED node's `Attribute` + `PropertyFts` rows — making the at-rest
/// property projection active-only — WITHOUT touching the `NodeFts`/`Vector`
/// shadows, whose stale rows the node read path already excludes by joining
/// `canonical_nodes WHERE superseded_at IS NULL`. Consistent with the erasure
/// model: an unregistered table is unreachable here, exactly as with erasure.
fn purge_row_projections_for_cursor_in(
    tx: &Connection,
    write_cursor: i64,
    classes: &[ProjectionClass],
) -> rusqlite::Result<u64> {
    let mut deleted: u64 = 0;
    for projection in ROW_OWNED_PROJECTIONS.iter().filter(|p| classes.contains(&p.class)) {
        let sql =
            format!("DELETE FROM {} WHERE {} = ?1", projection.table, projection.cursor_column);
        deleted = deleted.saturating_add(tx.execute(&sql, [write_cursor])? as u64);
    }
    Ok(deleted)
}

/// 0.8.20 Slice 5a (R-20-E1) — truncate the row-owned projections in `classes`.
/// Returns the number of shadow rows deleted.
fn truncate_row_projections_in(
    tx: &Connection,
    classes: &[ProjectionClass],
) -> rusqlite::Result<u64> {
    let mut deleted: u64 = 0;
    for projection in ROW_OWNED_PROJECTIONS.iter().filter(|p| classes.contains(&p.class)) {
        let sql = format!("DELETE FROM {}", projection.table);
        deleted = deleted.saturating_add(tx.execute(&sql, [])? as u64);
    }
    Ok(deleted)
}

/// 0.8.20 Slice 5a (R-20-E1) — truncate EVERY row-owned projection (the full
/// `rebuild_projections` invalidation). Kind-owned watermark state
/// (`_fathomdb_projection_state`) is deliberately untouched; the rebuild resets
/// readiness by rewinding the projection cursor instead.
#[cfg(feature = "operator")]
fn truncate_all_row_projections(tx: &Connection) -> rusqlite::Result<u64> {
    truncate_row_projections_in(
        tx,
        &[
            ProjectionClass::NodeFts,
            ProjectionClass::EdgeFts,
            ProjectionClass::Vector,
            ProjectionClass::Readiness,
            ProjectionClass::Attribute,
            ProjectionClass::PropertyFts,
        ],
    )
}

/// 0.8.20 Slice 5a (R-20-E1) — which half of a projector's work a call site
/// wants. The projectors are TOTAL (they own every row-owned projection for a
/// canonical row); the pass selects the subset a replay site is rebuilding, so
/// no call site re-implements projection SQL inline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectionPass {
    /// The write path: same-txn FTS **and** async vector enqueue / readiness
    /// termination.
    Write,
    /// Lexical replay only (the open-path tokenizer reproject). Readiness and
    /// vector state are already correct and must not be perturbed.
    FtsOnly,
    /// Readiness + async-vector replay only (`rebuild_vec0`, i.e. a rebuild with
    /// `include_fts = false`): the FTS shadows are not being rebuilt in this
    /// pass, so they must not be written.
    ///
    /// Only the `operator` rebuild seam constructs this pass, so the DEFAULT
    /// (recovery-clean) build sees it as unconstructed — same gate rationale as
    /// the operator methods themselves (feature = gate, not delete).
    #[cfg_attr(not(feature = "operator"), allow(dead_code))]
    VectorOnly,
}

impl ProjectionPass {
    fn writes_fts(self) -> bool {
        matches!(self, ProjectionPass::Write | ProjectionPass::FtsOnly)
    }

    fn writes_vector_state(self) -> bool {
        matches!(self, ProjectionPass::Write | ProjectionPass::VectorOnly)
    }

    /// 0.8.20 Slice 15d (R-20-EAV) — whether this pass (re)projects the declared
    /// attribute set into the EAV store + property-FTS. Only the full `Write`
    /// pass does: the `FtsOnly` tokenizer-upgrade reproject predates step 24 (no
    /// registry/attribute tables exist at that migration point, so it must not
    /// touch them), and `VectorOnly` rebuilds only the async vector shadows. The
    /// operator FTS rebuild uses `Write`, so a full `rebuild_projections`
    /// re-derives attributes cleanly after `truncate_all_row_projections` clears
    /// the two attribute classes.
    fn writes_attributes(self) -> bool {
        matches!(self, ProjectionPass::Write)
    }
}

/// 0.8.20 Slice 15d (R-20-PR) — the on-disk registry row for one declared
/// projection, read back from `_fathomdb_projection_registry`.
///
/// **On-disk encoding of the optional sub-objects.** The `fts_tokenizer` column
/// is tri-valued: SQL `NULL` = no `fts` sub-object; empty string `""` = `fts`
/// present with the engine-default tokenizer; a non-empty string = `fts` with a
/// custom tokenizer. This is what lets `searchable→FTS with default tokenizer`
/// be distinguished durably from `searchable` with no FTS sub-target. `vector`
/// mirrors it with an explicit `vector_declared` bit plus a nullable
/// `vector_embedder`.
#[derive(Clone, Debug, Eq, PartialEq)]
struct StoredProjection {
    roles: BTreeSet<ProjectionRole>,
    fts_present: bool,
    /// `Some(custom)` custom tokenizer; `None` = engine default (only
    /// meaningful when `fts_present`).
    fts_tokenizer: Option<String>,
    vector_declared: bool,
    vector_embedder: Option<String>,
}

impl StoredProjection {
    /// True iff the declared roles want the attribute VALUE stored at rest in
    /// the EAV store: `filterable` (the value IS the filter target) or
    /// `searchable` (the value is the retrievable meaning, and Slice 20's vector
    /// embed will read it from here). `rankable`-only wants no value at rest.
    fn wants_eav(&self) -> bool {
        self.roles.contains(&ProjectionRole::Filterable)
            || self.roles.contains(&ProjectionRole::Searchable)
    }

    /// True iff a `searchable→FTS` property-FTS row should be written: the
    /// `searchable` role AND an `fts` sub-object.
    fn wants_property_fts(&self) -> bool {
        self.roles.contains(&ProjectionRole::Searchable) && self.fts_present
    }

    /// The `fts_tokenizer` column value: `None` (SQL NULL) when no `fts`
    /// sub-object, else the custom tokenizer or `""` for engine-default.
    fn fts_column(&self) -> Option<String> {
        if self.fts_present {
            Some(self.fts_tokenizer.clone().unwrap_or_default())
        } else {
            None
        }
    }

    /// Build from the public [`ProjectionSpec`].
    fn from_spec(spec: &ProjectionSpec) -> Self {
        StoredProjection {
            roles: spec.roles.clone(),
            fts_present: spec.fts.is_some(),
            fts_tokenizer: spec
                .fts
                .as_ref()
                .and_then(|f| f.tokenizer.clone())
                .filter(|t| !t.is_empty()),
            vector_declared: spec.vector.is_some(),
            vector_embedder: spec
                .vector
                .as_ref()
                .and_then(|v| v.embedder.clone())
                .filter(|e| !e.is_empty()),
        }
    }

    /// Reconstruct the public [`ProjectionSpec`] for `read_projections`.
    fn to_spec(&self, name: &str) -> ProjectionSpec {
        ProjectionSpec {
            name: name.to_string(),
            roles: self.roles.clone(),
            fts: if self.fts_present {
                Some(ProjectionFts { tokenizer: self.fts_tokenizer.clone() })
            } else {
                None
            },
            vector: if self.vector_declared {
                Some(ProjectionVector { embedder: self.vector_embedder.clone() })
            } else {
                None
            },
        }
    }

    /// The set of ROLE spellings this declaration DEFERS rather than builds:
    /// `rankable` (F9 not live) and, since 15d builds no embedding, the
    /// `searchable→vector` sub-target. Used to populate `ProjectionDelta.deferred`.
    fn has_deferred(&self) -> bool {
        self.roles.contains(&ProjectionRole::Rankable) || self.vector_declared
    }
}

/// 0.8.20 Slice 15d (R-20-PR) — is `name` a well-formed attribute name? Rejects
/// empty, and any name carrying a double-quote or NUL, so the JSON path
/// `$."<name>"` compiled below is always well-formed (a malformed json path
/// would ERROR inside the write transaction, not merely miss). Caller-supplied,
/// so this is validated at `configure_projections` time.
fn is_valid_attribute_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('"') && !name.contains('\0')
}

/// 0.8.20 Slice 15d — the SQLite JSON path that extracts attribute `name` from a
/// node body. `name` is pre-validated by [`is_valid_attribute_name`]; the path
/// is bound as a PARAMETER (never interpolated into SQL), so this is not an
/// injection surface even before that validation.
fn attribute_json_path(name: &str) -> String {
    format!("$.\"{name}\"")
}

/// 0.8.20 Slice 15d (R-20-PR) — load the durable projection registry
/// (`_fathomdb_projection_registry`) into a name→[`StoredProjection`] map. This
/// is the derived-cache source (Q5) that boot re-derive and every
/// `configure_projections` diff read.
fn load_projection_registry(
    conn: &Connection,
) -> rusqlite::Result<BTreeMap<String, StoredProjection>> {
    let mut out = BTreeMap::new();
    // The registry table is created by schema step 24; a DB migrated to a
    // pre-24 head (e.g. a compatibility/partial-migration test open) does not
    // have it. Absent ⇒ no projections declared ⇒ empty registry, not an error.
    // This keeps boot re-derive and the write-path attribute projector safe on
    // every pre-24 schema.
    let table_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_fathomdb_projection_registry'",
            [],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if !table_exists {
        return Ok(out);
    }
    let mut stmt = conn.prepare(
        "SELECT name, roles, fts_tokenizer, vector_embedder, vector_declared
         FROM _fathomdb_projection_registry",
    )?;
    let rows = stmt.query_map([], |row| {
        let name: String = row.get(0)?;
        let roles_json: String = row.get(1)?;
        let fts_tokenizer: Option<String> = row.get(2)?;
        let vector_embedder: Option<String> = row.get(3)?;
        let vector_declared: i64 = row.get(4)?;
        Ok((name, roles_json, fts_tokenizer, vector_embedder, vector_declared))
    })?;
    for row in rows {
        let (name, roles_json, fts_col, vector_embedder, vector_declared) = row?;
        let roles: BTreeSet<ProjectionRole> = parse_roles_json(&roles_json);
        let fts_present = fts_col.is_some();
        let fts_tokenizer = fts_col.filter(|t| !t.is_empty());
        out.insert(
            name,
            StoredProjection {
                roles,
                fts_present,
                fts_tokenizer,
                vector_declared: vector_declared != 0,
                vector_embedder,
            },
        );
    }
    Ok(out)
}

/// Roles are persisted as a compact, sorted, comma-separated list (set
/// semantics; order-independent). Unknown tokens are ignored (forward-compat).
fn parse_roles_json(s: &str) -> BTreeSet<ProjectionRole> {
    s.split(',').filter_map(|t| ProjectionRole::from_str_opt(t.trim())).collect()
}

fn roles_to_storage(roles: &BTreeSet<ProjectionRole>) -> String {
    roles.iter().map(|r| r.as_str()).collect::<Vec<_>>().join(",")
}

/// 0.8.20 Slice 15d (R-20-PR) — write/overwrite one registry row.
fn persist_projection_row(
    tx: &Connection,
    name: &str,
    stored: &StoredProjection,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO _fathomdb_projection_registry
             (name, roles, fts_tokenizer, vector_embedder, vector_declared)
         VALUES(?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(name) DO UPDATE SET
             roles = excluded.roles,
             fts_tokenizer = excluded.fts_tokenizer,
             vector_embedder = excluded.vector_embedder,
             vector_declared = excluded.vector_declared",
        params![
            name,
            roles_to_storage(&stored.roles),
            stored.fts_column(),
            stored.vector_embedder,
            i64::from(stored.vector_declared),
        ],
    )?;
    Ok(())
}

/// 0.8.20 Slice 15d (R-20-PR) — delete one registry row.
fn remove_projection_row(tx: &Connection, name: &str) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM _fathomdb_projection_registry WHERE name = ?1", params![name])?;
    Ok(())
}

/// 0.8.20 Slice 15d (R-20-EAV) — delete every EAV + property-FTS row for one
/// attribute `name` (all owning nodes). The idempotent-rebuild primitive: a
/// changed or dropped projection clears its rows before (re)backfill.
fn clear_attribute_projection(tx: &Connection, name: &str) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM property_search_index WHERE attr_name = ?1", params![name])?;
    tx.execute("DELETE FROM canonical_attributes WHERE attr_name = ?1", params![name])?;
    Ok(())
}

/// 0.8.20 Slice 15d (R-20-EAV) — project ONE attribute value for ONE node row
/// into the EAV store and (if `searchable→FTS`) the property-FTS shadow. Skips a
/// NULL/absent extraction (an absent attribute means no row, so a `filterable`
/// equality simply never matches it — correct). Shared by the write path and
/// the backfill so they cannot drift.
fn project_one_attribute(
    tx: &Connection,
    cursor: i64,
    body: &str,
    name: &str,
    stored: &StoredProjection,
) -> rusqlite::Result<()> {
    if !stored.wants_eav() {
        return Ok(());
    }
    let path = attribute_json_path(name);
    // json_extract over a non-JSON body would error; guard with json_valid so a
    // plain-text body simply yields no attribute rows.
    //
    // fix-1 finding 1 [P2] — project EVERY JSON scalar type, not just strings.
    // The prior form read the extraction as `Option<String>`; for a JSON number
    // or bool, `json_extract` returns an INTEGER/REAL, the `get::<Option<String>>`
    // conversion FAILED, and `.unwrap_or(None)` silently treated the attribute as
    // absent — so a numeric/boolean filterable value never projected. We now
    // render a single canonical TEXT form per JSON type, keyed on `json_type` so
    // the stored value is deterministic and the SAME value flows to BOTH
    // `canonical_attributes` and `property_search_index` (consistency by
    // construction — one `value` binding below):
    //   - string  -> the text verbatim
    //   - integer -> decimal text (CAST AS TEXT); e.g. 3 -> "3"
    //   - real    -> decimal text (CAST AS TEXT); e.g. 3.5 -> "3.5"
    //   - true    -> "true", false -> "false"  (preserve the JSON literal, NOT the
    //                SQLite `1`/`0` that a bare `CAST(json_extract(...) AS TEXT)`
    //                would yield — so a bool filter matches the value the caller
    //                wrote, and "true" never collides with the number 1).
    //   - null / absent path -> SQL NULL -> no row (an absent attribute correctly
    //                never matches a `filterable` equality).
    //   - object / array -> DELIBERATELY SKIPPED (SQL NULL -> no row): a composite
    //                value is not a scalar filter/FTS target in 15d; projecting its
    //                raw JSON text would be a footgun (nested-field filtering is the
    //                >=0.9.x multi-field work). Skipping is deliberate, not an
    //                accidental type-conversion drop — no scalar type is dropped.
    let value: Option<String> = tx
        .query_row(
            "SELECT CASE WHEN json_valid(?1) THEN
                 CASE json_type(?1, ?2)
                     WHEN 'true'   THEN 'true'
                     WHEN 'false'  THEN 'false'
                     WHEN 'null'   THEN NULL
                     WHEN 'object' THEN NULL
                     WHEN 'array'  THEN NULL
                     ELSE CAST(json_extract(?1, ?2) AS TEXT)
                 END
             END",
            params![body, path],
            |row| row.get::<_, Option<String>>(0),
        )
        .unwrap_or(None);
    let Some(value) = value else {
        return Ok(());
    };
    tx.execute(
        "INSERT INTO canonical_attributes(write_cursor, attr_name, attr_value)
         VALUES(?1, ?2, ?3)",
        params![cursor, name, value],
    )?;
    if stored.wants_property_fts() {
        tx.execute(
            "INSERT INTO property_search_index(attr_value, attr_name, write_cursor)
             VALUES(?1, ?2, ?3)",
            params![value, name, cursor],
        )?;
    }
    Ok(())
}

/// 0.8.20 Slice 15d (R-20-EAV) — the write-path attribute projector: for a
/// just-inserted node, project EVERY declared attribute (reading the live
/// registry from `tx`). Same-transaction, so the node is filter/FTS-retrievable
/// on commit. A no-op when the registry is empty (the pre-`configure_projections`
/// default), so it costs one empty-table scan per node and is behaviour-neutral
/// until a projection is declared.
fn project_node_attributes(tx: &Connection, cursor: i64, body: &str) -> rusqlite::Result<()> {
    let registry = load_projection_registry(tx)?;
    for (name, stored) in &registry {
        project_one_attribute(tx, cursor, body, name, stored)?;
    }
    Ok(())
}

/// 0.8.20 Slice 15d (R-20-PR) — backfill ONE attribute across every ACTIVE,
/// non-superseded canonical node. Called by `configure_projections` when a
/// projection is added/changed (after `clear_attribute_projection`), and by boot
/// re-derive. Idempotent when paired with the clear.
fn backfill_attribute(
    tx: &Connection,
    name: &str,
    stored: &StoredProjection,
) -> rusqlite::Result<()> {
    if !stored.wants_eav() {
        return Ok(());
    }
    let rows: Vec<(i64, String)> = {
        let mut stmt = tx.prepare(
            "SELECT write_cursor, body FROM canonical_nodes
             WHERE superseded_at IS NULL AND state = 'active'",
        )?;
        let collected = stmt
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        collected
    };
    for (cursor, body) in rows {
        project_one_attribute(tx, cursor, &body, name, stored)?;
    }
    Ok(())
}

/// 0.8.20 Slice 15d (R-20-PR) — is `desired` an INCOMPATIBLE/DESTRUCTIVE change
/// to a live `existing` projection? A destructive change discards an
/// expensive-to-rebuild resource and so REQUIRES an explicit `drop` (C3): a role
/// REMOVAL, dropping the `fts`/`vector` sub-target, or changing the tokenizer /
/// embedder. Purely ADDITIVE changes (adding a role, adding an `fts`/`vector`
/// sub-object) are non-destructive and applied in place.
fn is_destructive_projection_change(
    existing: &StoredProjection,
    desired: &StoredProjection,
) -> bool {
    if existing.roles.iter().any(|r| !desired.roles.contains(r)) {
        return true;
    }
    if existing.fts_present
        && (!desired.fts_present || existing.fts_tokenizer != desired.fts_tokenizer)
    {
        return true;
    }
    if existing.vector_declared
        && (!desired.vector_declared || existing.vector_embedder != desired.vector_embedder)
    {
        return true;
    }
    false
}

/// Human-readable summary of the destructive delta, surfaced in
/// [`EngineError::ProjectionDestructive`] so the caller sees WHAT it must drop.
fn describe_projection_delta(existing: &StoredProjection, desired: &StoredProjection) -> String {
    let mut parts: Vec<String> = Vec::new();
    for r in &existing.roles {
        if !desired.roles.contains(r) {
            parts.push(format!("role '{}' removed", r.as_str()));
        }
    }
    if existing.fts_present && !desired.fts_present {
        parts.push("fts sub-target removed".to_string());
    } else if existing.fts_present && existing.fts_tokenizer != desired.fts_tokenizer {
        parts.push("fts tokenizer changed".to_string());
    }
    if existing.vector_declared && !desired.vector_declared {
        parts.push("vector sub-target removed".to_string());
    } else if existing.vector_declared && existing.vector_embedder != desired.vector_embedder {
        parts.push("vector embedder changed".to_string());
    }
    if parts.is_empty() {
        "incompatible change".to_string()
    } else {
        parts.join("; ")
    }
}

/// 0.8.20 Slice 15d (R-20-PR) — the declarative, idempotent diff+backfill apply
/// that backs [`Engine::configure_projections`]. Runs inside the caller's write
/// transaction `tx`. Order: apply `drop`s first (so a drop+re-declare in one
/// call rebuilds fresh), then diff each spec. Idempotent re-registration diffs to
/// an empty delta (`unchanged`). A destructive change without an explicit drop is
/// refused with [`EngineError::ProjectionDestructive`].
fn apply_projection_config(
    tx: &Connection,
    specs: &[ProjectionSpec],
    drop: &[String],
) -> Result<ProjectionDelta, EngineError> {
    // Validate up-front so a bad name aborts before any write.
    for spec in specs {
        if !is_valid_attribute_name(&spec.name) {
            return Err(EngineError::InvalidArgument {
                msg: format!("invalid projection attribute name: {:?}", spec.name),
            });
        }
        if spec.roles.is_empty() {
            return Err(EngineError::InvalidArgument {
                msg: format!("projection '{}' declares no roles", spec.name),
            });
        }
    }
    for name in drop {
        if !is_valid_attribute_name(name) {
            return Err(EngineError::InvalidArgument {
                msg: format!("invalid projection drop name: {name:?}"),
            });
        }
    }

    let mut delta = ProjectionDelta::default();

    // (1) Explicit drops. Omission never drops (C3); only this list does.
    let before_drop = load_projection_registry(tx).map_err(|_| EngineError::Storage)?;
    for name in drop {
        if before_drop.contains_key(name) {
            clear_attribute_projection(tx, name).map_err(|_| EngineError::Storage)?;
            remove_projection_row(tx, name).map_err(|_| EngineError::Storage)?;
            delta.dropped.push(name.clone());
        }
        // dropping an absent projection is an idempotent no-op, not an error.
    }

    // (2) Diff each spec against the post-drop registry.
    let current = load_projection_registry(tx).map_err(|_| EngineError::Storage)?;
    for spec in specs {
        let desired = StoredProjection::from_spec(spec);
        match current.get(&spec.name) {
            Some(existing) if existing == &desired => {
                // Idempotent re-registration — no-op (the keystone acceptance).
            }
            Some(existing) => {
                if is_destructive_projection_change(existing, &desired) {
                    return Err(EngineError::ProjectionDestructive {
                        name: spec.name.clone(),
                        delta: describe_projection_delta(existing, &desired),
                    });
                }
                persist_projection_row(tx, &spec.name, &desired)
                    .map_err(|_| EngineError::Storage)?;
                clear_attribute_projection(tx, &spec.name).map_err(|_| EngineError::Storage)?;
                backfill_attribute(tx, &spec.name, &desired).map_err(|_| EngineError::Storage)?;
                if desired.wants_eav() {
                    delta.built.push(spec.name.clone());
                }
                if desired.has_deferred() && !existing.has_deferred() {
                    delta.deferred.push(spec.name.clone());
                }
            }
            None => {
                persist_projection_row(tx, &spec.name, &desired)
                    .map_err(|_| EngineError::Storage)?;
                clear_attribute_projection(tx, &spec.name).map_err(|_| EngineError::Storage)?;
                backfill_attribute(tx, &spec.name, &desired).map_err(|_| EngineError::Storage)?;
                if desired.wants_eav() {
                    delta.built.push(spec.name.clone());
                }
                if desired.has_deferred() {
                    delta.deferred.push(spec.name.clone());
                }
            }
        }
    }

    delta.unchanged =
        delta.built.is_empty() && delta.dropped.is_empty() && delta.deferred.is_empty();
    Ok(delta)
}

/// 0.8.20 Slice 15d (R-20-PR, Q5) — BOOT re-derive: the engine `ProjectionSpec`
/// is a DERIVED cache, re-driven idempotently on boot. For every persisted
/// registry declaration, clear + backfill its EAV / property-FTS rows from the
/// canonical nodes — so a DB whose registry row survives but whose projection
/// rows are missing/partial (a crash window, a restored registry) CONVERGES on
/// the next open. A no-op (single empty-table read) when no projections are
/// declared — which is every pre-`configure_projections` DB. Runs on the writer
/// connection, single-threaded, before readers spawn.
fn rederive_projections_on_boot(conn: &Connection) -> rusqlite::Result<()> {
    let registry = load_projection_registry(conn)?;
    if registry.is_empty() {
        return Ok(());
    }
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        for (name, stored) in &registry {
            clear_attribute_projection(conn, name)?;
            backfill_attribute(conn, name, stored)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => conn.execute_batch("COMMIT"),
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

/// EXP-S (0.8.14 Slice 5) — the `row_kind -> index-target set` dispatch
/// (ADR-0.8.14 §D2), and the OPP-12 forward-compat seam (ADR-0.8.14 §D5(a) /
/// ledger `TC-1`).
///
/// This is deliberately a per-kind LOOKUP rather than branching inlined at each
/// write call-site: it is the single seam a later declarative OPP-12 projection
/// registry (`dev/design/projection-registry-and-async-embed.md`) would wrap to
/// populate `row_kind -> {filterable, searchable->FTS (same-txn), searchable->
/// vector (async)}` without reshaping the substrate. Per D5, EXP-S implements
/// NO OPP-12 surface here (OPP-12 lands >=0.9.x; re-check at its scheduling) —
/// this function only records the index-target intent so the async-vs-sync split
/// (D5(b)) and the per-kind-extensible terminal-cursor readiness (D5(c)) stay
/// wrappable.
///
/// `Leaf` MUST preserve today's behavior exactly: FTS (sync) + vector (async,
/// gated by `kind_is_vector_indexed`).
fn index_targets_for_row_kind(row_kind: RowKind) -> IndexTargetSet {
    match row_kind {
        // Normal record — identical to pre-EXP-S behavior.
        RowKind::Leaf => IndexTargetSet { fts: true, vector: true },
        // Coverage/summary rows — searchable and embeddable.
        RowKind::Coverage => IndexTargetSet { fts: true, vector: true },
        // Graph structural rows — lexically searchable, not embedded.
        RowKind::Graph => IndexTargetSet { fts: true, vector: false },
    }
}

/// EXP-S (0.8.14 Slice 5, D2/D5) — apply the per-`row_kind` index-target
/// dispatch for one just-inserted canonical node row (write_cursor `cursor`).
///
/// Preserves the OPP-12-shaped split (D5(b)): FTS is written in THIS
/// transaction (same-txn `searchable->FTS`); vector work is only *enqueued*
/// here into `_fathomdb_projection_state` and embedded later, asynchronously,
/// by the projection worker pool (`searchable->vector`). When the row projects
/// into no async vector index, its readiness is terminated up-front (D5(c),
/// per-kind-extensible) so `advance_projection_cursor` can walk past it.
///
/// Returns `true` iff async vector work was enqueued (the caller must then
/// `notify_new_work`). For `RowKind::Leaf` this is behavior-identical to the
/// pre-EXP-S inline node path.
fn project_canonical_node_row(
    tx: &Connection,
    cursor: u64,
    kind: &str,
    body: &str,
    row_kind: RowKind,
    pass: ProjectionPass,
    node_active: bool,
) -> rusqlite::Result<bool> {
    let targets = index_targets_for_row_kind(row_kind);
    if targets.fts && pass.writes_fts() {
        tx.execute(
            "INSERT INTO search_index(body, kind, write_cursor) VALUES(?1, ?2, ?3)",
            params![body, kind, cursor],
        )?;
        // F5 (0.8.14 Slice 10) — same coexisting `searchable->FTS` target also
        // populates the multi-column `search_index_v2` (kind/body/status) so a
        // BM25F query can field-weight the lexical arm. Written SYNCHRONOUSLY in
        // THIS transaction, exactly like `search_index` (rowid==write_cursor
        // identity preserved). The `status` field mirrors the migration-17
        // O(N) re-index: `$.status` from a JSON body, guarded by `json_valid` so
        // non-JSON bodies index an empty status. NOTE (codex fix-1 finding 2):
        // this is F5's OWN `$.status`-derived field for the BM25F `status`
        // column — it is NOT (yet) the value the shipped G10 SearchFilter reads.
        // G10 filtering reads the vec0 `status` column, which is still hardwired
        // to the empty-string sentinel; wiring G10 onto this field is out of
        // scope for F5. Determinism (R-SUB-2) is preserved: the derivation is
        // a pure function of `body`, evaluated in-SQL identically on every run.
        tx.execute(
            "INSERT INTO search_index_v2(kind, body, status, write_cursor)
             VALUES(
                 ?1,
                 ?2,
                 CASE WHEN json_valid(?2)
                      THEN COALESCE(json_extract(?2, '$.status'), '')
                      ELSE '' END,
                 ?3
             )",
            params![kind, body, cursor],
        )?;
    }
    // 0.8.20 Slice 15d (R-20-EAV) — same-transaction attribute projection. Only
    // the full `Write` pass re-derives attributes (see `writes_attributes`): the
    // FtsOnly tokenizer reproject predates step 24 and must not touch the
    // registry/attribute tables; VectorOnly rebuilds only vector shadows. A full
    // operator FTS rebuild uses `Write`, so it re-derives attributes after the
    // truncate.
    //
    // fix-2 [P2]: gated on `node_active`. The at-rest attribute projection tracks
    // EXACTLY the backfill's row set — `state = 'active' AND superseded_at IS NULL`
    // (see `backfill_attribute`). Unlike node-FTS / vector shadows (whose stale
    // versions are excluded by the canonical read path's `superseded_at IS NULL`
    // / `state = 'active'` join), the property tables carry NO read-side lifecycle
    // filter (`property_search_index` is an FTS5 table that cannot), so a pending
    // or superseded node's attribute values would otherwise LEAK into a
    // same-session property filter / property-FTS. The write path passes
    // `state == Active`; a projector-replay rebuild passes `active ∧ non-superseded`
    // per row. Lifecycle transitions maintain the store directly (see
    // `Engine::transition`). Passes where `writes_attributes()` is false ignore the
    // flag entirely.
    if pass.writes_attributes() && node_active {
        project_node_attributes(tx, cursor as i64, body)?;
    }
    let enqueue_vector = targets.vector && kind_is_vector_indexed(tx, kind).unwrap_or(false);
    if pass.writes_vector_state() {
        if enqueue_vector {
            tx.execute(
                "INSERT INTO _fathomdb_projection_state(kind, last_enqueued_cursor, updated_at)
                 VALUES(?1, ?2, 0)
                 ON CONFLICT(kind) DO UPDATE SET last_enqueued_cursor = excluded.last_enqueued_cursor",
                params![kind, cursor],
            )?;
        } else {
            // Never-vector-projected rows terminate the cursor up-front so
            // `advance_projection_cursor` can advance the readiness watermark.
            record_projection_terminal(tx, cursor, "up_to_date")?;
        }
    }
    Ok(enqueue_vector)
}

/// 0.8.20 Slice 5a (R-20-E1, work item 1) — the EDGE half of the total
/// projector, extracted verbatim from the inlined `commit_batch` edge arm.
///
/// Before this extraction there was NO edge projector function: `commit_batch`
/// inlined the edge FTS insert + the edge vector enqueue, and
/// `rebuild_shadow_state` re-implemented a SUBSET of it (edge FTS only, and only
/// for body-carrying edges), so a projector-replay rebuild silently dropped the
/// rest — notably the `up_to_date` readiness terminal that the write path
/// records for a body-less structural edge. With both sites now calling this one
/// function, the write path and the rebuild path produce identical edge
/// projections by construction.
///
/// Mirrors [`project_canonical_node_row`]'s split (ADR-0.8.14 §D5(b)): FTS in
/// THIS transaction; vector work only ENQUEUED, embedded later by the worker
/// pool. Edge bodies enqueue under the fixed kind `"edge_fact"` so
/// `resolve_source_type` maps them to `source_type = "edge_fact"` in
/// `vector_default` (partition correctness); that kind is auto-registered in
/// `_fathomdb_vector_kinds` (idempotent).
///
/// Returns `true` iff async vector work was enqueued.
fn project_canonical_edge_row(
    tx: &Connection,
    cursor: u64,
    kind: &str,
    body: Option<&str>,
    pass: ProjectionPass,
) -> rusqlite::Result<bool> {
    // G11 — edge FTS projection into `search_index_edges` (separate table from
    // node-body `search_index` — Option B partition). Body-less structural
    // edges carry no lexical content and project no FTS row.
    if pass.writes_fts() {
        if let Some(edge_body) = body {
            tx.execute(
                "INSERT INTO search_index_edges(body, kind, write_cursor)
                 VALUES(?1, ?2, ?3)",
                params![edge_body, kind, cursor],
            )?;
        }
    }
    let enqueue_vector = body.is_some();
    if pass.writes_vector_state() {
        if enqueue_vector {
            let now_unix =
                SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
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
            // Do NOT call record_projection_terminal — let the scheduler embed
            // the body and mark it terminal after projection.
        } else {
            record_projection_terminal(tx, cursor, "up_to_date")?;
        }
    }
    Ok(enqueue_vector)
}

/// F5 (0.8.14 Slice 10, fix-1) — tokenizer for the in-engine BM25F scorer.
///
/// Tokenizes `text` through the SAME FTS5 tokenizer that `search_index_v2` uses
/// for candidate recall (`porter unicode61 remove_diacritics 2`), so the scorer
/// measures term-frequency, document-frequency, field length, and average field
/// length under the index's own tokenization — porter stemming + unicode61
/// case-fold + diacritic folding. The previous implementation hand-rolled a
/// second lowercase-alnum splitter; a stemmed/diacritic variant recalled by
/// `MATCH` (e.g. query `run` vs indexed `running`, or `cafe` vs `café`) was then
/// scored as if the term were absent, so ranking was wrong for exactly those
/// variants (codex §9 fix-1 finding 1). Reusing FTS5 itself makes scoring
/// tokenization-faithful without re-implementing porter/unicode61 in Rust.
///
/// Mechanism: round-trip `text` through a temp single-column FTS5 table with the
/// identical tokenizer, then read the emitted token instances back via the
/// `fts5vocab(..., 'instance')` companion. The token multiset is returned in
/// index order (duplicates kept) so callers count tf and field length directly.
/// Query terms and every candidate field go through this one path, so all four
/// statistics are consistent with each other and with the FTS5 index the scorer
/// ranks.
fn fts5_tokenize(connection: &Connection, text: &str) -> rusqlite::Result<Vec<String>> {
    connection.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS temp.bm25f_tok
             USING fts5(t, tokenize = 'porter unicode61 remove_diacritics 2');
         CREATE VIRTUAL TABLE IF NOT EXISTS temp.bm25f_tok_vocab
             USING fts5vocab('bm25f_tok', 'instance');
         DELETE FROM temp.bm25f_tok;",
    )?;
    connection.execute("INSERT INTO temp.bm25f_tok(t) VALUES(?1)", params![text])?;
    let mut stmt =
        connection.prepare("SELECT term FROM temp.bm25f_tok_vocab ORDER BY \"offset\"")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    rows.collect()
}

/// F5 (0.8.14 Slice 10) — build the FTS5 `MATCH` expression for candidate
/// recall from the query's tokens: each token is a double-quoted FTS5 string
/// (tokens are FTS5-emitted stems — unicode61 alnum, no embedded quotes),
/// OR-joined.
fn bm25f_match_expression(terms: &[String]) -> String {
    terms.iter().map(|t| format!("\"{t}\"")).collect::<Vec<_>>().join(" OR ")
}

/// F5 (0.8.14 Slice 10) — the BM25F score for one candidate document.
///
/// Standard BM25F: per query term, accumulate a length-normalized,
/// field-weighted pseudo term-frequency across the fields, then apply the BM25
/// saturation once. `norm_f = 1 - b + b*(len_f/avglen_f)` is the per-field
/// length normalization (this is where tunable `b` bites); `weight_f` is the
/// field boost (this is where the R-F5-1 field weighting bites).
fn bm25f_score_doc(
    plan: &Bm25fQueryPlan,
    query_terms: &[String],
    // (weight, doc field length, corpus avg field length, per-term tf in field)
    fields: &[(f64, f64, f64, &HashMap<String, u32>)],
    doc_count: usize,
    df: &HashMap<String, usize>,
) -> f64 {
    let mut score = 0.0_f64;
    for term in query_terms {
        let mut weighted_tf = 0.0_f64;
        for (weight, len_f, avglen_f, tf_map) in fields {
            if *weight == 0.0 || *avglen_f <= 0.0 {
                continue;
            }
            let tf = *tf_map.get(term).unwrap_or(&0) as f64;
            if tf == 0.0 {
                continue;
            }
            let norm = 1.0 - plan.b + plan.b * (len_f / avglen_f);
            if norm <= 0.0 {
                continue;
            }
            weighted_tf += weight * tf / norm;
        }
        if weighted_tf <= 0.0 {
            continue;
        }
        let dfq = *df.get(term).unwrap_or(&0);
        if dfq == 0 {
            continue;
        }
        let n = doc_count as f64;
        let idf = ((n - dfq as f64 + 0.5) / (dfq as f64 + 0.5) + 1.0).ln();
        score += idf * (weighted_tf * (plan.k1 + 1.0)) / (plan.k1 + weighted_tf);
    }
    score
}

/// F5 (0.8.14 Slice 10) — connection-level implementation of the BM25F lexical
/// arm. See [`Engine::bm25f_search`].
fn bm25f_search_inner(
    connection: &Connection,
    query: &str,
    plan: &Bm25fQueryPlan,
) -> rusqlite::Result<Vec<(u64, f64)>> {
    let query_terms: Vec<String> = {
        let mut seen = BTreeSet::new();
        fts5_tokenize(connection, query)?.into_iter().filter(|t| seen.insert(t.clone())).collect()
    };
    if query_terms.is_empty() {
        return Ok(Vec::new());
    }

    // Corpus pass over ACTIVE rows (superseded versions excluded): accumulate
    // N, total field length per field (for avg field length), and per-term
    // document frequency — all under the SAME FTS5 tokenization the index uses.
    let mut doc_count: usize = 0;
    let mut total_len = [0.0_f64; 3]; // kind, body, status
    let mut df: HashMap<String, usize> = HashMap::new();
    {
        let mut stmt = connection.prepare(
            "SELECT v.kind, v.body, v.status
             FROM search_index_v2 v
             JOIN canonical_nodes cn ON cn.write_cursor = v.write_cursor
             WHERE cn.superseded_at IS NULL AND cn.state = 'active'",
        )?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let fields =
                [row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?];
            doc_count += 1;
            let mut present: BTreeSet<String> = BTreeSet::new();
            for (i, field) in fields.iter().enumerate() {
                let toks = fts5_tokenize(connection, field)?;
                total_len[i] += toks.len() as f64;
                for tok in toks {
                    if query_terms.contains(&tok) {
                        present.insert(tok);
                    }
                }
            }
            for term in present {
                *df.entry(term).or_insert(0) += 1;
            }
        }
    }
    if doc_count == 0 {
        return Ok(Vec::new());
    }
    let avglen = [
        total_len[0] / doc_count as f64,
        total_len[1] / doc_count as f64,
        total_len[2] / doc_count as f64,
    ];

    // Active write_cursor set, to filter FTS5 MATCH candidates (search_index_v2
    // retains superseded versions, exactly like search_index).
    let active: BTreeSet<i64> = {
        let mut stmt = connection
            .prepare("SELECT write_cursor FROM canonical_nodes WHERE superseded_at IS NULL AND state = 'active'")?;
        let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
        rows.collect::<rusqlite::Result<BTreeSet<i64>>>()?
    };

    // Candidate recall through the FTS5 index (this is what makes the v2 index
    // load-bearing), then score each candidate with the in-engine BM25F.
    let match_expr = bm25f_match_expression(&query_terms);
    let mut scored: Vec<(u64, f64)> = Vec::new();
    {
        let mut stmt = connection.prepare(
            "SELECT write_cursor, kind, body, status
             FROM search_index_v2
             WHERE search_index_v2 MATCH ?1",
        )?;
        let mut rows = stmt.query([match_expr.as_str()])?;
        while let Some(row) = rows.next()? {
            let wc = row.get::<_, i64>(0)?;
            if !active.contains(&wc) {
                continue;
            }
            let kind = row.get::<_, String>(1)?;
            let body = row.get::<_, String>(2)?;
            let status = row.get::<_, String>(3)?;

            let mut tf_kind: HashMap<String, u32> = HashMap::new();
            let mut len_kind = 0.0_f64;
            for tok in fts5_tokenize(connection, &kind)? {
                len_kind += 1.0;
                *tf_kind.entry(tok).or_insert(0) += 1;
            }
            let mut tf_body: HashMap<String, u32> = HashMap::new();
            let mut len_body = 0.0_f64;
            for tok in fts5_tokenize(connection, &body)? {
                len_body += 1.0;
                *tf_body.entry(tok).or_insert(0) += 1;
            }
            let mut tf_status: HashMap<String, u32> = HashMap::new();
            let mut len_status = 0.0_f64;
            for tok in fts5_tokenize(connection, &status)? {
                len_status += 1.0;
                *tf_status.entry(tok).or_insert(0) += 1;
            }

            let fields = [
                (plan.weights.kind, len_kind, avglen[0], &tf_kind),
                (plan.weights.body, len_body, avglen[1], &tf_body),
                (plan.weights.status, len_status, avglen[2], &tf_status),
            ];
            let score = bm25f_score_doc(plan, &query_terms, &fields, doc_count, &df);
            scored.push((wc as u64, score));
        }
    }

    // Descending score; write_cursor ascending as the deterministic tiebreak.
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0))
    });
    Ok(scored)
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
            (
                PreparedWrite::Node {
                    kind,
                    body,
                    source_id,
                    logical_id,
                    state,
                    reason,
                    valid_from,
                    valid_until,
                },
                WritePlan::Node,
            ) => {
                // G0 — supersession is tombstone-then-insert in this same txn:
                // mark the prior active version superseded BEFORE inserting the
                // new active row, so the partial-unique-active index never sees
                // two active rows for one logical_id. Scoped to logical_id ALONE
                // (Decision 5, HITL-SIGNED 2026-06-05): a kind-change re-ingest of
                // the same logical_id SUPERSEDES, never forks. No-op when logical_id
                // is None (legacy/own-identity insert, behavior-identical to 0.7.x).
                if let Some(logical_id) = logical_id {
                    // fix-1 finding 2 [P2]: collect the prior active cursor(s)
                    // BEFORE tombstoning so we can purge the superseded row's
                    // row-owned attribute projections and keep the at-rest EAV /
                    // property-FTS store ACTIVE-ONLY. Without this, a same-session
                    // property filter / property-FTS saw BOTH the stale and the
                    // current value until a boot re-derive/reconfigure cleared the
                    // table — a stale read that violates the active-only invariant.
                    let prior_g0 = prior_node_cursors_by_logical_id(&tx, logical_id)?;
                    tx.execute(
                        "UPDATE canonical_nodes SET superseded_at = ?1
                         WHERE logical_id = ?2 AND superseded_at IS NULL",
                        params![cursor, logical_id],
                    )?;
                    // Purge only the Attribute + PropertyFts classes: those tables
                    // have NO `superseded_at IS NULL` read-side filter (the FTS5
                    // `property_search_index` cannot carry one), so their stale rows
                    // MUST be deleted at rest. The NodeFts (`search_index` /
                    // `search_index_v2`) + Vector shadows are left intact — the node
                    // read path already excludes their superseded rows via the
                    // `canonical_nodes WHERE superseded_at IS NULL` join, so purging
                    // them here would be a behaviour change outside this fix's scope.
                    for sc in &prior_g0 {
                        purge_row_projections_for_cursor_in(
                            &tx,
                            *sc,
                            &[ProjectionClass::Attribute, ProjectionClass::PropertyFts],
                        )?;
                    }
                }
                // EXP-S (0.8.14 Slice 5, D1) — a `PreparedWrite::Node` is the
                // `leaf` structural row_kind (a normal record). coverage/graph
                // rows are written via internal paths (row_kind is a SEPARATE
                // axis from the doc-type `kind`, and there is no public SDK
                // surface for it this release). Writing `leaf` explicitly is
                // value-identical to the column DEFAULT.
                // OPP-12 Phase-1 (0.8.19 Slice 5) — persist the create-time
                // existence state + advisory reason. `InitialState::Active`
                // (the default) writes `state = 'active'`, value-identical to the
                // migration step-20 column DEFAULT; `Pending` quarantines the node
                // out of default retrieval (the `state = 'active'` read exclusion).
                // 0.8.20 Slice 15b (TC-34) — persist the world-time validity
                // window. A `None` binds SQL NULL, which is what the migration
                // step-22 columns already hold for every pre-existing row and what
                // `ReadView::validity_sql` reads as UNBOUNDED on that side. So a
                // write that omits the window is byte-identical on disk to a
                // pre-slice write, and default-view visibility cannot drift.
                tx.execute(
                    "INSERT INTO canonical_nodes(write_cursor, kind, body, source_id, logical_id, row_kind, state, reason, valid_from, valid_until)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![cursor, kind, body, source_id.as_str(), logical_id, RowKind::Leaf.as_str(), state.as_str(), reason, valid_from, valid_until],
                )?;
                // EXP-S (D2/D5) — per-row_kind index-target dispatch. For `leaf`
                // this is behavior-identical to the pre-EXP-S inline path: FTS
                // (sync, in-tx) + vector (async, gated by kind_is_vector_indexed);
                // else the cursor is terminated up-front.
                // fix-2 [P2]: gate the attribute projection on the create-time
                // state. A fresh insert is always non-superseded, so the backfill
                // predicate (`state = 'active' AND superseded_at IS NULL`) reduces
                // to `state == Active` here. A `Pending` node is quarantined out of
                // the canonical read model — its declared attributes must NOT reach
                // the property store until a `transition(pending → active)` promotes
                // it (which projects them then). Node-FTS / vector shadows are left
                // to their read-side lifecycle filter, exactly as for supersession.
                project_canonical_node_row(
                    &tx,
                    cursor,
                    kind,
                    body,
                    RowKind::Leaf,
                    ProjectionPass::Write,
                    matches!(state, InitialState::Active),
                )?;
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
                        source_id.as_str(),
                        logical_id,
                        body,
                        t_valid,
                        t_invalid,
                        confidence,
                        extractor_model_id,
                        temporal_fallback_i
                    ],
                )?;
                // 0.8.20 Slice 5a (R-20-E1, work item 1) — edge projection is no
                // longer inlined here: the write path and the rebuild replay
                // share ONE projector, so they cannot drift.
                project_canonical_edge_row(
                    &tx,
                    cursor,
                    kind,
                    body.as_deref(),
                    ProjectionPass::Write,
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
    // TC-33: schema step 23 RECREATES `canonical_edges` (no data migration), so
    // the edge rows that used to hold the high-water mark are gone. Without this
    // term the allocator can hand out a cursor a PREVIOUS edge already used —
    // and stale `_fathomdb_projection_terminal` / `_fathomdb_vector_rows` / vec0
    // rows still key on it, so a brand-new row would be treated as
    // already-projected and never get indexed. Step 23 stashes the pre-drop
    // maximum here; folding it in keeps cursors monotonic across the migration.
    let reserved = reserved_write_cursor(connection);
    nodes.max(edges).max(mutations).max(state).max(reserved)
}

/// The write-cursor high-water mark reserved by schema step 23, or 0 when the
/// key is absent (fresh DB, or a DB that never had edges). Never fails the
/// caller: a missing/unparseable value degrades to 0, which is the pre-TC-33
/// behaviour.
fn reserved_write_cursor(connection: &Connection) -> u64 {
    connection
        .query_row(
            "SELECT value FROM _fathomdb_open_state WHERE key = ?1",
            params![fathomdb_schema::RESERVED_WRITE_CURSOR_KEY],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(0)
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
    use super::{
        derive_stable_id, resolve_source_type, Engine, IdSpace, IdSpaceKind, PreparedWrite,
        KIND_TO_SOURCE_TYPE_CASE_SQL, ROW_OWNED_PROJECTIONS,
    };
    use rusqlite::Connection;
    use tempfile::TempDir;

    /// 0.8.20 Slice 5a (R-20-E1, work item 2) — the registry GUARD.
    ///
    /// Introspects `sqlite_master` on a freshly migrated database and asserts
    /// that EVERY `write_cursor`-keyed table is accounted for: either it is a
    /// registered row-owned projection, or it is one of the explicitly named
    /// canonical / operational tables that are sources of truth, not shadows.
    /// A future projection table therefore cannot be added without either
    /// registering it in [`ROW_OWNED_PROJECTIONS`] (making it erasable at every
    /// maintenance site at once) or consciously failing this test.
    ///
    /// **`_fathomdb_projection_state` is allowlisted as KIND-owned** (design v5
    /// §1.1): it is keyed by `kind`, not by `write_cursor`, and holds a per-kind
    /// enqueue watermark. Erasing one row must not rewind a whole kind's
    /// watermark, so it must NEVER be deleted per-cursor. The test asserts both
    /// halves of that claim — that it carries no `write_cursor` column, and that
    /// it is absent from the row-owned registry.
    #[test]
    fn guard_row_owned_registry() {
        /// Canonical + operational tables: `write_cursor`-carrying SOURCES OF
        /// TRUTH, never row-owned projections of another row.
        const NON_PROJECTION_CURSOR_TABLES: &[&str] =
            &["canonical_nodes", "canonical_edges", "operational_mutations", "operational_state"];

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("registry_guard.fathomdb");
        Engine::open(&path).expect("open").engine.close().expect("close");
        let conn = Connection::open(&path).expect("open sqlite");

        let table_names: Vec<String> = conn
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .expect("prepare")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect");
        assert!(table_names.len() > 5, "sqlite_master introspection returned nothing useful");

        let has_write_cursor = |table: &str| -> bool {
            conn.prepare(&format!("PRAGMA table_info({table})"))
                .and_then(|mut stmt| {
                    let names = stmt
                        .query_map([], |row| row.get::<_, String>(1))?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    Ok(names.iter().any(|n| n == "write_cursor"))
                })
                .unwrap_or(false)
        };

        let registered: Vec<&str> = ROW_OWNED_PROJECTIONS.iter().map(|p| p.table).collect();

        // (1) Every write_cursor-keyed table is registered or explicitly excused.
        for table in &table_names {
            if !has_write_cursor(table) {
                continue;
            }
            assert!(
                registered.contains(&table.as_str())
                    || NON_PROJECTION_CURSOR_TABLES.contains(&table.as_str()),
                "table `{table}` is keyed by write_cursor but is neither registered in \
                 ROW_OWNED_PROJECTIONS nor listed as a non-projection source of truth. \
                 If it is a projection, register it — otherwise erasure will leave its \
                 rows on disk (the `search_index_v2` defect)."
            );
        }

        // (2) Every registered projection actually exists and is erasable by its
        //     declared cursor column (vec0's `rowid` included).
        for projection in ROW_OWNED_PROJECTIONS {
            assert!(
                table_names.iter().any(|t| t == projection.table),
                "registered projection `{}` does not exist in the schema",
                projection.table
            );
            conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM {} WHERE {} = 0",
                    projection.table, projection.cursor_column
                ),
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap_or_else(|err| {
                panic!(
                    "registered projection `{}` is not erasable by `{}`: {err}",
                    projection.table, projection.cursor_column
                )
            });
        }

        // (3) `_fathomdb_projection_state` is KIND-owned, not row-owned.
        assert!(
            !has_write_cursor("_fathomdb_projection_state"),
            "_fathomdb_projection_state gained a write_cursor column — re-decide its ownership \
             class before treating it as kind-owned"
        );
        assert!(
            !registered.contains(&"_fathomdb_projection_state"),
            "_fathomdb_projection_state is KIND-owned (per-kind enqueue watermark) and must \
             never be deleted per-cursor: erasing one row would rewind a whole kind's watermark"
        );
    }

    /// 0.8.20 Slice 15d (R-20-EAV) — PROVE THE GUARD BITES. The two net-new
    /// content-storing projection tables (`canonical_attributes`,
    /// `property_search_index`) are `write_cursor`-keyed and hold attribute
    /// values at rest. This test asserts (1) they ARE registered in
    /// `ROW_OWNED_PROJECTIONS` (so `erase_row_projections` reaches them), and (2)
    /// the guard's core predicate — "registered OR a named source of truth" —
    /// FAILS for either table if it is (hypothetically) removed from the
    /// registry. This is what makes forgetting to register a future
    /// content-storing projection a red test, not a silent erasure leak.
    #[test]
    fn slice15d_attribute_projections_registered_and_guard_bites() {
        const NON_PROJECTION_CURSOR_TABLES: &[&str] =
            &["canonical_nodes", "canonical_edges", "operational_mutations", "operational_state"];

        let registered: Vec<&str> = ROW_OWNED_PROJECTIONS.iter().map(|p| p.table).collect();

        // (1) Both new content-storing projections are registered as row-owned.
        for table in ["canonical_attributes", "property_search_index"] {
            assert!(
                registered.contains(&table),
                "{table} holds attribute values at rest and MUST be in ROW_OWNED_PROJECTIONS \
                 so purge/excise_source reach it"
            );
        }

        // (2) The guard predicate BITES: pretend one of them was never
        //     registered — the guard's "registered OR source-of-truth" check must
        //     reject it (the exact assertion `guard_row_owned_registry` runs).
        for hidden in ["canonical_attributes", "property_search_index"] {
            let as_if_unregistered: Vec<&str> =
                registered.iter().copied().filter(|t| *t != hidden).collect();
            let accepted = as_if_unregistered.contains(&hidden)
                || NON_PROJECTION_CURSOR_TABLES.contains(&hidden);
            assert!(
                !accepted,
                "if {hidden} were unregistered the guard would still (incorrectly) accept it — \
                 the guard does not actually bite"
            );
        }
    }

    /// Cause-A (0.8.11.2) / C-2 (0.8.19) — `derive_stable_id` id-space contract:
    /// a present `logical_id` yields a `Logical` (`"l:"`) [`IdSpace`]; a NULL or
    /// empty `logical_id` falls back to a deterministic `Content` (`"h:"`) sha256
    /// content-hash of the body. The typed spaces are prefix-distinguishable and
    /// the value is behaviour-neutral (never used in ranking). Post-C-2 the helper
    /// returns a typed [`IdSpace`] whose `to_prefixed()` reproduces the pre-swap
    /// string byte-for-byte (eu7 no-op basis).
    #[test]
    fn derive_stable_id_id_space_contract() {
        // logical_id present → Logical space, body-independent.
        assert_eq!(derive_stable_id(Some("alice-1"), "any body"), IdSpace::logical("alice-1"));
        assert_eq!(
            derive_stable_id(Some("alice-1"), "a different body"),
            IdSpace::logical("alice-1")
        );
        // Byte-identical prefixed form to the pre-C-2 `stable_id` string.
        assert_eq!(derive_stable_id(Some("alice-1"), "any body").to_prefixed(), "l:alice-1");

        // NULL logical_id → Content space, deterministic on body.
        let h1 = derive_stable_id(None, "stable body text");
        let h2 = derive_stable_id(None, "stable body text");
        assert_eq!(h1, h2, "content-hash is deterministic");
        assert_eq!(h1.space, IdSpaceKind::Content);
        let h1s = h1.to_prefixed();
        assert!(h1s.starts_with("h:"));
        assert_eq!(h1s.len(), 2 + 64, "h: + sha256 hex");
        assert!(h1s["h:".len()..].chars().all(|c| c.is_ascii_hexdigit()));

        // Empty logical_id is treated as absent (falls back to content-hash).
        assert_eq!(derive_stable_id(Some(""), "stable body text"), h1);

        // Distinct bodies → distinct content-hashes (no collision).
        assert_ne!(derive_stable_id(None, "body A"), derive_stable_id(None, "body B"));
    }

    /// C-2 (0.8.19 / TC-8) — [`IdSpace`] parse/format round-trip is stable across
    /// all three spaces, including a value that itself contains `":"`.
    #[test]
    fn id_space_parse_format_round_trip() {
        let cases = [
            IdSpace::logical("alice-1"),
            IdSpace::content("a".repeat(64)),
            IdSpace::passage("7"),
            IdSpace::logical("l:weird:value"), // value contains the delimiter
        ];
        for id in cases {
            assert_eq!(IdSpace::parse(&id.to_prefixed()), Some(id.clone()), "round-trip {id:?}");
        }
        assert_eq!(IdSpace::logical("x").to_prefixed(), "l:x");
        assert_eq!(IdSpace::content("y").to_prefixed(), "h:y");
        assert_eq!(IdSpace::passage("3").to_prefixed(), "p:3");
        assert_eq!(IdSpace::parse("untagged"), None);
    }

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
                source_id: crate::SourceId::new("test:fixture").expect("test source id"),
                logical_id: None,
                state: crate::InitialState::Active,
                reason: None,
                valid_from: None,
                valid_until: None,
            }])
            .expect("write should succeed");

        assert_eq!(receipt.cursor, 1);
    }
}
