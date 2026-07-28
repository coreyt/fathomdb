//! 0.8.20 Slice 21a-1 (R-20-CR) — **TC-57 characterization harness**.
//!
//! ## What this file is
//!
//! This is the CHARACTERIZATION harness for TC-57 (ledger seq-84): a loop of
//! **governed** writes (`PreparedWrite::Node { logical_id: Some(..) }`) racing
//! the async projection worker intermittently returns `EngineError::Storage`.
//! The companion written characterization is
//! `dev/design/0.8.20-tc57-write-race-characterization.md`.
//!
//! ## Status of the two engine-level arms: LIVE — and RED as of this commit
//!
//! [`tc57_repro_governed_write_loop_races_projection_worker`] and its control
//! [`tc57_control_anonymous_write_loop_does_not_race`] were BOTH `#[ignore]`d in
//! the characterization commit `a569246a` on purpose — the repro arm was expected
//! **RED** at baseline `41a81c17`, and the control was disabled alongside it so
//! the pair was enabled and disabled as one unit (the characterization is a
//! COMPARISON, and half a comparison is worse than none).
//!
//! **Slice 21a-2 removes both `#[ignore]` attributes as its RED→GREEN step. This
//! commit is the RED half**: the attributes are gone, `src/lib.rs` is still
//! byte-identical to baseline, and the repro arm therefore FAILS. The fix lands
//! in the very next commit. Nothing else in this file changes across the pair —
//! the arms assert exactly what they asserted while they were ignored, which is
//! what makes the green a real signal rather than a rewritten goalpost.
//!
//! The three `tc57_mechanism_*` tests are pure-rusqlite pins. They are NOT
//! `#[ignore]`d: they assert what SQLite does, not what the engine does, and
//! they pass at baseline.
//!
//! ## What was MEASURED at baseline `41a81c17`
//!
//! | arm | result |
//! |---|---|
//! | repro (`logical_id: Some`) | **10/10 runs failed**, always at write index 2 |
//! | control (`logical_id: None`) | **0/10 runs failed** |
//!
//! The failure emits the lifecycle code `"SQLITE_BUSY"` — i.e. EXTENDED code 5,
//! **not** 517 — and returns in **0 ms** against rusqlite's default 5 000 ms busy
//! timeout (measured). So the engine's failure is the HELD-WRITE-LOCK promotion
//! case pinned by [`tc57_mechanism_held_write_lock_upgrade_is_unretryable_busy_5`],
//! not the stale-snapshot 517 case pinned by
//! [`tc57_mechanism_stale_snapshot_upgrade_is_busy_snapshot_517`]. Both are the
//! SAME root cause (a DEFERRED transaction that reads before it writes) and
//! neither is retryable by `busy_timeout`; the prior write-ups that named 517
//! alone were describing only one of the two exits. See
//! `dev/design/0.8.20-tc57-write-race-characterization.md`.
//!
//! ## Deliberately NOT fixed here
//!
//! Slice 21a-1 is characterization only (ruling `seq-111`). This file adds
//! tests and nothing else; `src/lib.rs` is byte-identical to baseline.

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::lifecycle::{Event, EventSource, Subscriber};
use fathomdb_engine::{Engine, EngineError, InitialState, PreparedWrite, SourceId};
use fathomdb_schema::SQLITE_SUFFIX;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// A live, in-process, deterministic embedder with a small per-call delay.
///
/// Why a LIVE embedder and not the `default-embedder` cargo feature: the
/// projection worker only performs vector work (and therefore only commits on
/// its own runtime connection) when `shared.embedder.is_some()` — see the
/// `Deferred` early-return in `run_projection_job` (`lib.rs:12834`). Without a
/// live embedder there is no second writer and the race cannot exist.
/// `Engine::open_with_embedder_for_test` (`lib.rs:4340`) supplies one with no
/// feature flag and no network, so this target compiles to REAL tests in the
/// default merge gate rather than to zero tests.
#[derive(Clone, Debug)]
struct DelayEmbedder {
    identity: EmbedderIdentity,
    delay: Duration,
}

impl DelayEmbedder {
    fn new(delay: Duration) -> Self {
        Self { identity: EmbedderIdentity::new("deterministic", "rev-a", 384), delay }
    }
}

impl Embedder for DelayEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        let mut v = vec![0.0_f32; self.identity.dimension as usize];
        v[0] = 1.0;
        Ok(v)
    }
}

/// Captures every `EventSource::SqliteInternal` code the engine emits.
///
/// This is the ONLY route by which a caller can learn anything about the
/// underlying rusqlite error on this path: `write_inner` calls
/// `emit_sqlite_internal_error(&err)` and then returns the UNIT variant
/// `EngineError::Storage` (`lib.rs:4994-4996`), dropping `err`. `EngineError`
/// carries no `source()` payload for `Storage` (`lib.rs:3537`), so the numeric
/// extended code is unrecoverable from the public API.
#[derive(Default)]
struct SqliteCodeSink {
    codes: Mutex<Vec<&'static str>>,
}

impl Subscriber for SqliteCodeSink {
    fn on_event(&self, event: &Event) {
        if event.source == EventSource::SqliteInternal {
            if let Some(code) = event.code {
                self.codes.lock().unwrap().push(code);
            }
        }
    }
}

/// A GOVERNED node write — `logical_id: Some(..)`. This is the arm under test:
/// it takes the `commit_batch` supersession branch (`lib.rs:17791-17820`), whose
/// FIRST statement inside the deferred transaction is a READ
/// (`prior_node_cursors_by_logical_id`, `lib.rs:17799`).
fn governed_node(i: usize) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: format!(r#"{{"summary":"meaning {i}"}}"#),
        source_id: SourceId::new("test:fixture").expect("source id"),
        logical_id: Some(format!("tc57-{i}")),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

/// The CONTROL — an ANONYMOUS node write, `logical_id: None`. Byte-identical to
/// [`governed_node`] in every other field, so the only independent variable is
/// the supersession read.
fn anonymous_node(i: usize) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: format!(r#"{{"summary":"meaning {i}"}}"#),
        source_id: SourceId::new("test:fixture").expect("source id"),
        logical_id: None,
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

/// How many writes one arm issues. Sized from the ledger repro (which failed at
/// write 4 of a 300-write loop): large enough that the SELECT→UPDATE window is
/// offered to the worker hundreds of times, small enough to run in ~2 s.
const WRITES: usize = 300;

/// The outcome of one arm: which write index (if any) failed, and every
/// SQLite-internal code the engine emitted along the way.
struct ArmOutcome {
    first_failure: Option<(usize, EngineError)>,
    sqlite_codes: Vec<&'static str>,
    embed_calls: usize,
    /// Wall time of the FAILING `Engine::write` call. This is load-bearing
    /// evidence, not decoration: a plain `SQLITE_BUSY` reached by exhausting a
    /// busy handler takes the whole busy timeout, whereas `SQLITE_BUSY_SNAPSHOT`
    /// (and a zero busy timeout) return immediately. The duration therefore
    /// discriminates the two even though the engine discards the numeric code.
    failure_wall_ms: u128,
    /// Wall time of the SLOWEST successful write, as the baseline the failing
    /// write's duration is read against.
    slowest_ok_ms: u128,
}

/// Run one arm of the comparison: `WRITES` single-row writes in a tight loop
/// against an engine whose async projection worker is live and committing on its
/// OWN connection.
///
/// `governed = true` uses [`governed_node`], `false` uses [`anonymous_node`].
/// Everything else — embedder, vector-kind enrolment, loop shape, pacing — is
/// identical between the arms.
fn run_arm(governed: bool) -> ArmOutcome {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join(format!("tc57_{}{SQLITE_SUFFIX}", if governed { "g" } else { "a" }));
    let embed_calls = Arc::new(AtomicUsize::new(0));
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(CountingDelay::new(embed_calls.clone())),
    )
    .expect("open");
    let engine = &opened.engine;
    let sink = Arc::new(SqliteCodeSink::default());
    let _subscription = engine.subscribe(sink.clone());
    // Enrol `doc` as a vector kind so every write enqueues an embed job; without
    // this the worker has nothing to commit and never contends.
    engine.configure_vector_kind_for_test("doc").expect("vector kind");

    let mut first_failure = None;
    let mut failure_wall_ms = 0_u128;
    let mut slowest_ok_ms = 0_u128;
    for i in 0..WRITES {
        let batch = if governed { governed_node(i) } else { anonymous_node(i) };
        let started = Instant::now();
        let result = engine.write(&[batch]);
        let elapsed = started.elapsed().as_millis();
        match result {
            Ok(_) => slowest_ok_ms = slowest_ok_ms.max(elapsed),
            Err(err) => {
                first_failure = Some((i, err));
                failure_wall_ms = elapsed;
                break;
            }
        }
        // Yield so the projection worker actually interleaves rather than being
        // starved by a writer that never releases the CPU.
        std::thread::sleep(Duration::from_millis(1));
    }

    let codes = sink.codes.lock().unwrap().clone();
    // Best-effort settle + close; a failed arm may leave work outstanding, and
    // this harness is not asserting on drain.
    let _ = engine.drain(30_000);
    let _ = opened.engine.close();
    ArmOutcome {
        first_failure,
        sqlite_codes: codes,
        embed_calls: embed_calls.load(Ordering::SeqCst),
        failure_wall_ms,
        slowest_ok_ms,
    }
}

/// [`DelayEmbedder`] plus a call counter, so "the worker really ran" is asserted
/// rather than assumed (a vacuous arm in which no embed ever happened would
/// report a clean pass for the wrong reason).
#[derive(Debug)]
struct CountingDelay {
    inner: DelayEmbedder,
    calls: Arc<AtomicUsize>,
}

impl CountingDelay {
    fn new(calls: Arc<AtomicUsize>) -> Self {
        Self { inner: DelayEmbedder::new(Duration::from_millis(1)), calls }
    }
}

impl Embedder for CountingDelay {
    fn identity(&self) -> EmbedderIdentity {
        self.inner.identity()
    }

    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.embed(text)
    }
}

// ---------------------------------------------------------------------------
// Arm 1 — the repro (EXPECTED RED at baseline 41a81c17)
// ---------------------------------------------------------------------------

/// **TC-57 REPRO ARM. Expected RED at baseline `41a81c17`.**
///
/// A loop of GOVERNED writes racing the live projection worker must not fail.
/// At baseline it fails with `EngineError::Storage` (10/10 runs) because the
/// governed branch of `commit_batch` READS before it WRITES inside a rusqlite
/// DEFERRED transaction (`lib.rs:17764` opens `connection.transaction()`,
/// `:17799` reads, `:17800-17804` upgrades), while the projection worker holds
/// the write lock on its own connection (`commit_projection_outcomes`,
/// `lib.rs:13672`, also DEFERRED). SQLite refuses the promotion WITHOUT invoking
/// the busy handler, so no `busy_timeout` value could absorb it — see
/// [`tc57_mechanism_held_write_lock_upgrade_is_unretryable_busy_5`].
///
/// `#[ignore]` was REMOVED by Slice 21a-2 as its RED→GREEN step.
#[test]
fn tc57_repro_governed_write_loop_races_projection_worker() {
    let outcome = run_arm(true);
    assert!(
        outcome.embed_calls > 0,
        "non-vacuity: the projection worker must have embedded at least one row, \
         else there was no second writer and the race window never existed"
    );
    if let Some((i, err)) = &outcome.first_failure {
        panic!(
            "TC-57 REPRO: governed write {i} of {WRITES} failed with {err:?} after \
             {} ms (slowest OK write {} ms); SQLite-internal codes emitted: {:?}",
            outcome.failure_wall_ms, outcome.slowest_ok_ms, outcome.sqlite_codes
        );
    }
}

// ---------------------------------------------------------------------------
// Arm 2 — the control
// ---------------------------------------------------------------------------

/// **TC-57 CONTROL ARM.** The identical loop with `logical_id: None`.
///
/// With no `logical_id` the supersession block (`lib.rs:17791-17820`) is skipped
/// entirely, so the FIRST statement in the deferred transaction is the INSERT at
/// `lib.rs:17838`: the write lock is taken immediately, there is no earlier read
/// snapshot to invalidate, and contention degrades to plain `SQLITE_BUSY`, which
/// rusqlite's default 5 s busy timeout DOES retry.
///
/// If this arm ever fails, that is a FINDING, not a flake: it would refute the
/// "`Some`-only" half of the diagnosis. Report it; do not suppress it.
///
/// `#[ignore]` was REMOVED by Slice 21a-2 together with the repro arm.
#[test]
fn tc57_control_anonymous_write_loop_does_not_race() {
    let outcome = run_arm(false);
    assert!(
        outcome.embed_calls > 0,
        "non-vacuity: the projection worker must have embedded at least one row, \
         else this control is not comparable to the repro arm"
    );
    if let Some((i, err)) = &outcome.first_failure {
        panic!(
            "TC-57 CONTROL FAILED — this REFUTES the `Some`-only diagnosis: anonymous \
             write {i} of {WRITES} failed with {err:?} after {} ms (slowest OK write {} ms); \
             SQLite-internal codes emitted: {:?}",
            outcome.failure_wall_ms, outcome.slowest_ok_ms, outcome.sqlite_codes
        );
    }
}

// ---------------------------------------------------------------------------
// Mechanism pins — pure rusqlite, no engine
// ---------------------------------------------------------------------------
//
// The engine discards the rusqlite error (`lib.rs:4994-4996`) and
// `EngineError::Storage` is a unit variant with no `source()` payload
// (`lib.rs:3537`), so the NUMERIC extended code is unrecoverable through the
// public API. These three tests therefore pin the mechanism at the SQLite level,
// independently of the engine, so the characterization rests on captured values.
//
// There are TWO distinct un-retryable failures a read-then-upgrade transaction
// can take, and they must not be conflated:
//
//   * `SQLITE_BUSY_SNAPSHOT` (517) — the WAL advanced past the reader's snapshot
//     while it held the read lock. Pinned by the first test.
//   * plain `SQLITE_BUSY` (5) returned WITHOUT invoking the busy handler — the
//     write lock is held by someone else at the instant of promotion. SQLite
//     skips the busy handler here for deadlock avoidance (`sqlite3_busy_handler`
//     docs: "If SQLite determines that invoking the busy handler could result in
//     a deadlock, it will go ahead and return SQLITE_BUSY"). Pinned by the
//     second test.
//
// **The engine's measured failure is the SECOND one** — see the characterization
// doc. Both share the same root cause (a deferred transaction that reads before
// it writes) and neither is retryable by `busy_timeout`, so a fix that only
// targets 517 would be incomplete.

/// **Pin 1 — the stale-snapshot case yields extended code 517.**
///
///   * connection A: `BEGIN DEFERRED` → `SELECT` (takes the read snapshot)
///   * connection B: a full write transaction, COMMITTED in between
///   * connection A: `UPDATE` (must promote the stale snapshot)
///
/// `SQLITE_BUSY_SNAPSHOT` is 517 (`SQLITE_BUSY | (2 << 8)`). Its PRIMARY code is
/// 5, but `sqlite_extended_code_name` (`lib.rs:18152`) matches on the EXTENDED
/// value against primary constants, so 517 falls through to its
/// `_ => "SQLITE_UNKNOWN"` arm — the numeric value is lost to subscribers.
#[test]
fn tc57_mechanism_stale_snapshot_upgrade_is_busy_snapshot_517() {
    const SQLITE_BUSY_SNAPSHOT: i32 = 517;

    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("mechanism_snapshot.sqlite");
    let (a, b) = wal_pair(&path);
    // Explicit, generous busy timeouts so the result cannot be blamed on an
    // absent busy handler.
    a.busy_timeout(Duration::from_secs(5)).expect("busy timeout a");
    b.busy_timeout(Duration::from_secs(5)).expect("busy timeout b");

    // A: the deferred read snapshot — `commit_batch`'s `connection.transaction()`
    // (`lib.rs:17764`) followed by `prior_node_cursors_by_logical_id`
    // (`lib.rs:17799`).
    a.execute_batch("BEGIN DEFERRED;").expect("begin deferred");
    let seen: i64 = a.query_row("SELECT v FROM t WHERE id = 1", [], |r| r.get(0)).expect("select");
    assert_eq!(seen, 0);

    // B: an unrelated COMMITTED write — the projection worker's
    // `commit_projection_outcomes` (`lib.rs:13672`) on its own connection.
    b.execute_batch("BEGIN IMMEDIATE; INSERT INTO t(id, v) VALUES(2, 99); COMMIT;")
        .expect("b commits");

    // A: the upgrade — `UPDATE canonical_nodes SET superseded_at = ...`
    // (`lib.rs:17800-17804`).
    let started = Instant::now();
    let err = a.execute("UPDATE t SET v = 1 WHERE id = 1", []).expect_err("the upgrade must fail");
    let elapsed = started.elapsed();
    let sqlite_err =
        err.sqlite_error().unwrap_or_else(|| panic!("expected a SqliteFailure, got {err:?}"));

    assert_eq!(
        sqlite_err.extended_code, SQLITE_BUSY_SNAPSHOT,
        "the stale-snapshot upgrade must yield SQLITE_BUSY_SNAPSHOT (517); got {} ({err:?})",
        sqlite_err.extended_code
    );
    assert_eq!(
        sqlite_err.code,
        rusqlite::ErrorCode::DatabaseBusy,
        "517's PRIMARY code is SQLITE_BUSY"
    );
    assert_ne!(
        sqlite_err.extended_code,
        rusqlite::ffi::SQLITE_BUSY,
        "extended != primary — which is exactly why `sqlite_extended_code_name`, matching the \
         EXTENDED value against PRIMARY constants, cannot name 517"
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "517 is returned immediately: the 5 s busy handler is NOT consulted (took {elapsed:?})"
    );

    let _ = a.execute_batch("ROLLBACK;");
}

/// **Pin 2 — the case the ENGINE actually hits: plain `SQLITE_BUSY` (5), returned
/// IMMEDIATELY, with the busy handler never invoked.**
///
/// Same read-then-upgrade shape, but B is still HOLDING the write lock (it has
/// not committed) at the moment A promotes. A has a 5 s busy timeout AND a
/// counting busy handler installed; the assertion is that the handler is called
/// ZERO times and the call returns in well under the timeout. That is what makes
/// "`busy_timeout` cannot retry it" a measured statement rather than a slogan.
///
/// This matches the engine arm exactly: the repro reports `"SQLITE_BUSY"` (so the
/// extended code is 5, not 517) after `0 ms`.
#[test]
fn tc57_mechanism_held_write_lock_upgrade_is_unretryable_busy_5() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("mechanism_held.sqlite");
    let (a, b) = wal_pair(&path);

    // `Connection::busy_handler` takes a bare `fn` pointer, so the counter must
    // be a static rather than a captured `Arc`.
    PROMOTE_HANDLER_CALLS.store(0, Ordering::SeqCst);
    a.busy_handler(Some(promote_busy_handler)).expect("busy handler a");

    // A: deferred, READ first.
    a.execute_batch("BEGIN DEFERRED;").expect("begin deferred");
    let seen: i64 = a.query_row("SELECT v FROM t WHERE id = 1", [], |r| r.get(0)).expect("select");
    assert_eq!(seen, 0);

    // B: takes and HOLDS the write lock (uncommitted) — the projection worker
    // mid-`commit_projection_outcomes`.
    b.execute_batch("BEGIN IMMEDIATE; INSERT INTO t(id, v) VALUES(2, 99);").expect("b holds");

    // A: promote.
    let started = Instant::now();
    let err = a.execute("UPDATE t SET v = 1 WHERE id = 1", []).expect_err("the upgrade must fail");
    let elapsed = started.elapsed();
    let sqlite_err =
        err.sqlite_error().unwrap_or_else(|| panic!("expected a SqliteFailure, got {err:?}"));

    assert_eq!(
        sqlite_err.extended_code,
        rusqlite::ffi::SQLITE_BUSY,
        "a HELD write lock (as opposed to a stale snapshot) yields the PRIMARY code 5, \
         not 517 — got {} ({err:?})",
        sqlite_err.extended_code
    );
    assert_eq!(
        PROMOTE_HANDLER_CALLS.load(Ordering::SeqCst),
        0,
        "THE POINT: SQLite skips the busy handler when promoting a read transaction \
         (deadlock avoidance), so no `busy_timeout` value could ever have retried this"
    );
    assert!(
        elapsed < Duration::from_millis(500),
        "and it fails instantly rather than after any backoff (took {elapsed:?})"
    );

    let _ = a.execute_batch("ROLLBACK;");
    let _ = b.execute_batch("ROLLBACK;");
}

/// **Pin 3 — the CONTROL: the write-FIRST shape (`logical_id: None`) is
/// retryable.** The busy handler IS invoked, so contention degrades to a wait
/// instead of an error.
///
/// With no `logical_id` the supersession block (`lib.rs:17791-17820`) is skipped
/// and the transaction's first statement is the INSERT at `lib.rs:17838`: the
/// write lock is taken up front, there is no read lock to promote, and SQLite's
/// deadlock-avoidance rule does not apply.
#[test]
fn tc57_mechanism_control_write_first_is_retryable() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("mechanism_control.sqlite");
    let (a, b) = wal_pair(&path);

    WRITE_FIRST_HANDLER_CALLS.store(0, Ordering::SeqCst);
    b.busy_handler(Some(write_first_busy_handler)).expect("busy handler b");

    // A: deferred, but the FIRST statement is the WRITE.
    a.execute_batch("BEGIN DEFERRED;").expect("begin deferred");
    a.execute("UPDATE t SET v = 1 WHERE id = 1", []).expect("write-first acquire succeeds");

    // B (no read lock held) cannot get the write lock — and DOES go through the
    // busy handler.
    let b_err = b
        .execute_batch("BEGIN IMMEDIATE; INSERT INTO t(id, v) VALUES(2, 99); COMMIT;")
        .expect_err("b must be locked out while a holds the write lock");
    let b_extended = b_err.sqlite_error().map(|e| e.extended_code).unwrap_or(-1);

    assert_ne!(b_extended, 517, "the write-first shape never produces SQLITE_BUSY_SNAPSHOT");
    assert_eq!(b_extended, rusqlite::ffi::SQLITE_BUSY, "it degrades to plain SQLITE_BUSY (5)");
    assert!(
        WRITE_FIRST_HANDLER_CALLS.load(Ordering::SeqCst) > 0,
        "and unlike the promote case, the busy handler IS consulted — so a `busy_timeout` \
         would have absorbed this contention instead of surfacing an error"
    );

    a.execute_batch("COMMIT;").expect("a commits");
}

/// Two WAL connections on one fresh database, seeded with a single row. Shared by
/// the three mechanism pins so the only variable between them is the lock
/// choreography.
fn wal_pair(path: &std::path::Path) -> (rusqlite::Connection, rusqlite::Connection) {
    let a = rusqlite::Connection::open(path).expect("open a");
    a.pragma_update(None, "journal_mode", "WAL").expect("wal a");
    a.execute_batch("CREATE TABLE t(id INTEGER PRIMARY KEY, v INTEGER NOT NULL);").expect("create");
    a.execute("INSERT INTO t(id, v) VALUES(1, 0)", []).expect("seed");
    let b = rusqlite::Connection::open(path).expect("open b");
    b.pragma_update(None, "journal_mode", "WAL").expect("wal b");
    (a, b)
}

// --- busy-handler instrumentation (fn pointers; see the call sites) ----------

static PROMOTE_HANDLER_CALLS: AtomicUsize = AtomicUsize::new(0);
static WRITE_FIRST_HANDLER_CALLS: AtomicUsize = AtomicUsize::new(0);

/// Counts invocations and ALWAYS asks SQLite to retry, so "called zero times"
/// cannot be explained away by a handler that declined on its first call.
fn promote_busy_handler(_attempts: i32) -> bool {
    PROMOTE_HANDLER_CALLS.fetch_add(1, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(5));
    true
}

/// Counts invocations and gives up after a BOUNDED number of retries, so the
/// control test can never hang.
fn write_first_busy_handler(_attempts: i32) -> bool {
    let n = WRITE_FIRST_HANDLER_CALLS.fetch_add(1, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(5));
    n < 20
}
