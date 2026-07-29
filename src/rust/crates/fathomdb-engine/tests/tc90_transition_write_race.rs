//! 0.8.20 Slice 23 (R-20-SV, leg 2) — **TC-90 characterization harness.**
//!
//! ## What this file is
//!
//! TC-90 (ledger `seq-128`, p1, `area: engine`) claims [`Engine::transition`] has
//! the SAME shape TC-57 fixed in `commit_batch`: it READS before it WRITES inside
//! a rusqlite `BEGIN DEFERRED` transaction, so the transaction must promote a read
//! lock to the WAL write lock while the async projection worker holds it.
//!
//! **CHARACTERIZATION ONLY — NO FIX** (steward `seq-136`: the fix lands at
//! 0.8.21). `src/rust/crates/fathomdb-engine/src/lib.rs` is byte-identical to the
//! baseline `94f09d7d` in the commit that lands this file. The written
//! characterization is `dev/design/0.8.20-tc90-tc91-characterization.md`; the
//! template it follows is `dev/design/0.8.20-tc57-write-race-characterization.md`.
//!
//! ## The shape IS there — quoted from `lib.rs` at `94f09d7d`
//!
//! ```text
//! lib.rs:8221   let tx = connection.transaction()...;                  // BEGIN DEFERRED
//! lib.rs:8227   let current: Option<(String, i64, String)> = tx.query_row(
//! lib.rs:8229       "SELECT state, write_cursor, body FROM canonical_nodes \
//! lib.rs:8230        WHERE logical_id = ?1 AND superseded_at IS NULL", ...)   // READ  -> read lock
//! lib.rs:8258   tx.execute(
//! lib.rs:8259       "UPDATE canonical_nodes SET state = ?1, reason = ?2 \
//! lib.rs:8260        WHERE logical_id = ?3 AND superseded_at IS NULL", ...)   // WRITE -> PROMOTE
//! ```
//!
//! That is structurally identical to `commit_batch`'s pre-TC-57-fix shape. What
//! differs is the MITIGATION already present at the call site, and it is that
//! difference — not the transaction shape — that the two loop arms measure.
//!
//! ## The two mitigations `transition` has and `commit_batch` did not
//!
//! 1. **`lib.rs:8217` — `self.drain(LIFECYCLE_DRAIN_TIMEOUT_MS)?` before the
//!    transaction.** Waits for `active_jobs == 0 && queued_jobs == 0` AND for the
//!    on-disk `database_has_pending_projection_work` predicate to go false
//!    (`lib.rs:1246-1273`), so at the instant `drain` returns there is no worker
//!    commit in flight and none pending.
//! 2. **`lib.rs:8219` — `self.connection.lock()`.** `write_inner` takes the SAME
//!    mutex (`lib.rs:5051`), so no in-process `Engine::write` can commit — and
//!    therefore no new projection job can be enqueued — while `transition` holds
//!    its transaction open.
//!
//! Neither is present in `commit_batch`. Together they mean the in-process race
//! window is NOT "the whole transaction" (as it was for TC-57) but only the gap
//! between `drain` returning and the connection mutex being acquired: a competing
//! `Engine::write` that wins the mutex in that gap can commit, wake the
//! dispatcher, and have a worker holding the write lock by the time `transition`
//! promotes.
//!
//! ## MEASURED at `94f09d7d` — the window is real, and it is NARROW
//!
//! Two independent sessions of 10 runs per arm; every arm replicated exactly.
//!
//! | arm | writers x burst | pace | embed | runs | failed |
//! |---|---|---|---|---|---|
//! | [`tc90_repro_transition_loop_races_projection_worker`] | 1 x 1 | 3 ms | 2 ms | 20 | **0/20** |
//! | [`tc90_control_transition_loop_without_second_writer`] | 1 x 1 | 3 ms | — | 20 | **0/20** |
//! | [`tc90_stress_transition_loop_under_saturating_burst_load`] | 2 x 24 | 1 ms | 0 ms | 20 | **20/20** |
//! | [`tc90_stress_control_without_second_writer`] | 2 x 24 | 1 ms | — | 20 | **0/20** |
//!
//! The TC-57-shaped paced arm does **not** reproduce: at a 2 ms embed the worker
//! cannot reach its commit inside the microseconds `transition` spends between
//! acquiring the connection mutex and promoting. **The stress arm reproduces
//! 10/10** — against a control that is **0/10** with the identical load.
//!
//! **Quote the reproduction rate, not the failure rate.** Two independent
//! sessions on the same commit and machine gave per-run means of **5.9** and
//! **3.8** failures per 40 transitions; the 10/10 reproduction replicated, the
//! rate did not. Pooled over N = 20 the range is **1–10 per 40** (≈ 2.5 %–25 %).
//! In all 20 runs: `attempted == 40`, `truncated == false`, zero `Scheduler` and
//! zero other variants — and since codex §9 round 4 those are **asserted**
//! ([`assert_protocol_ran`]), not merely reported.
//!
//! Had only the paced arm been run, this file would have reported a clean and
//! completely false NEGATIVE. The knob that decided it is
//! [`ArmConfig::embed_delay_ms`].
//!
//! ## Mechanism pins — deterministic, and deliberately NOT `#[ignore]`d
//!
//! The loop arms measure whether the window is *reachable*. They cannot say what
//! SQLite returns when it IS reached, because `transition` maps every rusqlite
//! error to the unit variant `EngineError::Storage` with `.map_err(|_| ...)` and —
//! unlike `write_inner`, which at least calls `emit_sqlite_internal_error`
//! (`lib.rs:5097`) — emits NO lifecycle event at all. The numeric code is not
//! merely opaque on this path, it is never produced.
//!
//! So the mechanism is pinned deterministically instead, by holding the write lock
//! from a second connection rather than racing for it:
//!
//! * [`tc90_mechanism_transition_sql_shape_on_real_schema_is_busy_5`] replays
//!   `transition`'s two statements VERBATIM against a real engine-created
//!   `canonical_nodes`, with a counting busy handler installed.
//! * [`tc90_mechanism_engine_transition_under_held_write_lock_fails_immediately`]
//!   drives the real [`Engine::transition`] under the same held lock.
//! * [`tc90_mechanism_control_engine_write_under_held_write_lock_survives`] drives
//!   the already-`BEGIN IMMEDIATE` [`Engine::write`] under the SAME held lock. It
//!   is the direct evidence for whether TC-57's R1 remedy transfers here.
//!
//! ## Test profile — which targets run in the DEFAULT `cargo test` gate
//!
//! Decided deliberately per target, not by reflex (codex §9 round 4 finding 3).
//! The gating question is TC-72: roughly 1 workspace run in 3 already fails on
//! plain `main`, so anything timing-sensitive added to the default profile makes
//! that worse and must earn its place.
//!
//! **`#[ignore]`d — all four LOOP arms.** They race a live projection worker
//! against live writer threads; their results are RATES, i.e. measurements, and
//! `cargo test --workspace` is not a stable signal for them. The pair is enabled
//! and disabled as one unit — a comparison with one arm running is worse than no
//! comparison. Run with `--ignored`.
//!
//! **LIVE in the default profile — all three `tc90_mechanism_*` pins**, on the
//! same basis TC-57's own `tc57_mechanism_*` pins run that way. None of them
//! races for anything: the contending write lock is TAKEN and HELD by a second
//! connection for the whole measured call, so the outcome is decided by SQLite's
//! documented promotion behaviour rather than by scheduling. Their wall-clock
//! assertions are one-sided bounds with large margins, not equalities:
//!
//! | pin | shape | timing assertion | measured | margin |
//! |---|---|---|---|---|
//! | 1 (`..._sql_shape_...`) | two connections, ONE thread, no sleeps | `< 500 ms` | 0 ms | fails instantly by construction — the busy handler is skipped |
//! | 2 (`..._engine_transition_...`) | blocker held on the calling thread | `< 2 000 ms` | 0 ms | same construction; no `drain` work is outstanding |
//! | 3 (`..._control_engine_write_...`) | blocker thread SIGNALS readiness, then holds 900 ms | `>= 450 ms` | 931–937 ms (N = 10); 934–941 ms under a 24-way CPU load | the writer does not start until the blocker has signalled that `BEGIN IMMEDIATE` returned, and the holder then SLEEPS 900 ms; the writer's 5 000 ms busy timeout leaves ~4.1 s of slack |
//!
//! Pin 3 is the only one with a thread, and it is the one deliberate call: it is
//! kept live because it is the load-bearing evidence that TC-57's R1 remedy
//! transfers (design doc §3.3), and a fix scoped against R-A would otherwise be
//! reasoning from an ignored test.
//!
//! Its thread is synchronised by a **readiness channel, not a sleep** (codex §9
//! round 5 finding 4). The earlier fixed 100 ms sleep only *assumed* the blocker
//! had won the scheduler; on a loaded box the writer could have started before
//! the lock was ever taken, and the `>= 450 ms` bound would then have failed on
//! a correct engine — precisely the kind of target TC-72 says must not be added
//! to the merge gate. With the signal, pin 3's lock is held for the whole
//! measured call exactly as pins 1 and 2's are. **If it ever does flake, gate it
//! — do not widen the bound**, which is the assertion that makes it mean
//! anything.

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, EngineError, InitialState, LifecycleState, PreparedWrite, SourceId};
use fathomdb_schema::SQLITE_SUFFIX;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// Deterministic in-process embedder with a per-call delay and a call counter.
/// Same shape as `tc57_governed_write_race.rs`'s, so the worker pressure is
/// comparable between the two characterizations.
///
/// A LIVE embedder (not the `default-embedder` cargo feature) for the reason
/// TC-57 records: `run_projection_job` returns `Deferred` and writes NOTHING when
/// `shared.embedder` is `None`, so without one there is no second writer and no
/// race can exist. `open_with_embedder_for_test` supplies one with no feature flag
/// and no network, so this target compiles to REAL tests in the default gate.
#[derive(Debug)]
struct CountingDelayEmbedder {
    identity: EmbedderIdentity,
    calls: Arc<AtomicUsize>,
    delay: Duration,
}

impl CountingDelayEmbedder {
    fn new(calls: Arc<AtomicUsize>, delay: Duration) -> Self {
        Self { identity: EmbedderIdentity::new("deterministic", "rev-a", 384), calls, delay }
    }
}

impl Embedder for CountingDelayEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        let mut v = vec![0.0_f32; self.identity.dimension as usize];
        v[0] = 1.0;
        Ok(v)
    }
}

fn governed_node(logical_id: &str, body_tag: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: format!(r#"{{"summary":"tc90 {body_tag}"}}"#),
        source_id: SourceId::new("test:fixture").expect("source id"),
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

/// Governed rows seeded up front and then flipped back and forth by the
/// transition loop.
const SUBJECTS: usize = 8;

/// `transition` calls per arm. Each is a full `active -> deleted` or
/// `deleted -> active` flip, so every one of them executes the read-then-upgrade
/// transaction under measurement.
const TRANSITIONS: usize = 120;

/// `transition` calls for the STRESS pairing. Lower than [`TRANSITIONS`] because
/// under saturating load each call can burn the full 30 s `drain` timeout, so the
/// arm is bounded by [`ArmConfig::budget`] and this is only an upper bound on what
/// it will attempt.
const STRESS_TRANSITIONS: usize = 40;

/// Wall-clock budget for the STRESS arms.
///
/// A safety valve, NOT a measurement parameter: it stops an arm that has hit the
/// §2.5 drain-starvation shape from running unbounded. Since codex §9 round 4
/// finding 1, hitting it is a hard failure — see [`assert_stress_protocol_ran`].
/// MEASURED at `94f09d7d`: a full 40-transition stress run completes in ~20-45 s,
/// so 120 s is roughly a 3x margin and truncation means something has genuinely
/// changed.
const STRESS_BUDGET: Duration = Duration::from_secs(120);

/// How one arm loads the engine. The arms differ ONLY in these fields, and each
/// pairing below changes exactly one of them relative to its control.
struct ArmConfig {
    label: &'static str,
    /// Enrol `doc` as a vector kind. THIS is the independent variable of the
    /// repro/control pairing: with it the projection worker embeds and commits on
    /// its own connection (a second writer exists); without it the worker never
    /// commits (no second writer).
    second_writer: bool,
    /// Competing `Engine::write` threads. They exist to keep re-arming the
    /// dispatcher in the only in-process gap `transition` leaves open — between
    /// `drain` returning idle and the connection mutex being acquired.
    writer_threads: usize,
    /// Rows per competing `Engine::write` call. A burst larger than
    /// `PROJECTION_COMMIT_BATCH` (16) makes the worker commit FULL 16-job
    /// transactions, which is the longest write-lock hold the worker ever takes —
    /// i.e. the widest target the promote window can be offered.
    burst: usize,
    /// Sleep between competing writes. `0` saturates; a few ms lets
    /// `transition`'s own `drain` reach idle instead of burning its timeout.
    writer_pace_ms: u64,
    /// How many `transition` calls to attempt.
    transitions: usize,
    /// Per-`embed()` sleep for the arm's embedder.
    ///
    /// This is the sharpest knob on the promote window. The worker can only hold
    /// the write lock `embed_delay_ms` AFTER the competing write that enqueued its
    /// job released the connection mutex, whereas `transition` reaches its
    /// promoting `UPDATE` microseconds after acquiring that same mutex. Setting it
    /// to **0** is therefore the tightest interleaving the in-process shape admits,
    /// and any negative result must be measured there, not only at TC-57's 1-2 ms.
    embed_delay_ms: u64,
    /// Wall-clock budget for the transition loop.
    ///
    /// This is NOT decoration. MEASURED at `94f09d7d`: with `writer_pace_ms: 0`
    /// and two burst-24 writer threads, `transition`'s own `drain`
    /// (`lib.rs:8217`) never reaches idle, so every call burns the full
    /// `LIFECYCLE_DRAIN_TIMEOUT_MS` (30 s) and returns `EngineError::Scheduler` —
    /// an unbounded arm, and a finding in its own right. The loop therefore stops
    /// at the budget and reports `attempted` / `truncated` rather than hanging.
    ///
    /// It is a safety valve, **not** a licence to measure less. Every arm asserts
    /// `attempted == transitions` and `!truncated`, so hitting this budget is a
    /// hard failure that reports itself rather than a quietly shorter run
    /// (codex §9 round 4 finding 1).
    budget: Duration,
}

/// The outcome of one arm.
struct ArmOutcome {
    label: &'static str,
    /// How many `transition` calls were issued. Non-vacuity: every arm asserts
    /// this equals the protocol's transition count, not merely that it is
    /// non-zero — an arm that issued one proves nothing either.
    attempted: usize,
    /// The loop hit its wall-clock budget before issuing every transition.
    /// Asserted `false` by every arm; see [`assert_stress_protocol_ran`].
    truncated: bool,
    /// First `EngineError::Storage` observed, as `(index, error)`. This is the
    /// TC-90 signal; `Scheduler` and everything else are counted separately.
    first_storage_failure: Option<(usize, EngineError)>,
    /// Distinct non-`Storage`, non-`Scheduler` error variants seen, for diagnosis.
    other_error_kinds: Vec<String>,
    storage_errors: usize,
    scheduler_errors: usize,
    other_errors: usize,
    /// Writes the competing thread landed. The competing thread is what creates
    /// the drain-returns / mutex-acquired gap the repro arm targets.
    competing_writes: usize,
    competing_write_errors: usize,
    /// Embeds performed. This IS the independent variable: `> 0` means a second
    /// writer existed, `== 0` means it did not.
    embed_calls: usize,
    slowest_ok_ms: u128,
    failure_wall_ms: u128,
}

impl ArmOutcome {
    fn report(&self) {
        println!(
            "TC90 arm={} attempted={} truncated={} storage_errors={} scheduler_errors={} \
             other_errors={} competing_writes={} competing_write_errors={} embed_calls={} \
             slowest_ok_ms={} failure_wall_ms={} other_error_kinds={:?}",
            self.label,
            self.attempted,
            self.truncated,
            self.storage_errors,
            self.scheduler_errors,
            self.other_errors,
            self.competing_writes,
            self.competing_write_errors,
            self.embed_calls,
            self.slowest_ok_ms,
            self.failure_wall_ms,
            self.other_error_kinds,
        );
    }
}

/// Run one arm of the comparison.
///
/// Within a repro/control pairing the ONLY field that differs is
/// [`ArmConfig::second_writer`] — embedder, seed rows, competing-write load,
/// pacing and transition loop are identical.
fn run_arm(config: &ArmConfig) -> ArmOutcome {
    let label = config.label;
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join(format!("tc90_{label}{SQLITE_SUFFIX}"));
    let calls = Arc::new(AtomicUsize::new(0));
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(CountingDelayEmbedder::new(
            calls.clone(),
            Duration::from_millis(config.embed_delay_ms),
        )),
    )
    .expect("open");
    let engine = Arc::new(opened.engine);
    if config.second_writer {
        engine.configure_vector_kind_for_test("doc").expect("vector kind");
    }

    // Seed the subjects the transition loop flips. Written BEFORE the competing
    // thread starts so the loop never transitions a row that does not exist.
    for s in 0..SUBJECTS {
        engine
            .write(&[governed_node(&format!("tc90-subject-{s}"), &format!("subject {s}"))])
            .expect("seed write");
    }
    engine.drain(60_000).expect("seed drain");

    // The competing writer. Its job is to keep offering `transition` the one
    // in-process window that exists: `drain` returns idle, this thread wins the
    // connection mutex, commits, wakes the dispatcher, and a worker is holding the
    // write lock by the time `transition` promotes.
    let stop = Arc::new(AtomicBool::new(false));
    let competing_writes = Arc::new(AtomicUsize::new(0));
    let competing_errors = Arc::new(AtomicUsize::new(0));
    let burst = config.burst;
    let pace = config.writer_pace_ms;
    let mut writers = Vec::with_capacity(config.writer_threads);
    for t in 0..config.writer_threads {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        let writes = Arc::clone(&competing_writes);
        let errors = Arc::clone(&competing_errors);
        writers.push(std::thread::spawn(move || {
            let mut i = 0_usize;
            while !stop.load(Ordering::Relaxed) {
                let batch: Vec<PreparedWrite> = (0..burst)
                    .map(|b| {
                        governed_node(
                            &format!("tc90-load-{label}-{t}-{i}-{b}"),
                            &format!("load {label} {t} {i} {b}"),
                        )
                    })
                    .collect();
                match engine.write(&batch) {
                    Ok(_) => {
                        writes.fetch_add(burst, Ordering::SeqCst);
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::SeqCst);
                    }
                }
                i += 1;
                // Paced, not saturating, on the primary arms: `transition`'s own
                // `drain` must be able to reach idle, or every call would fail with
                // `Scheduler` (a drain timeout) and the arm would measure the wrong
                // thing — MEASURED at `0`: 4 of 5 attempts were `Scheduler`.
                if pace > 0 {
                    std::thread::sleep(Duration::from_millis(pace));
                }
            }
        }));
    }

    let mut attempted = 0_usize;
    let mut first_storage_failure = None;
    let mut storage_errors = 0_usize;
    let mut scheduler_errors = 0_usize;
    let mut other_errors = 0_usize;
    let mut other_error_kinds: Vec<String> = Vec::new();
    let mut slowest_ok_ms = 0_u128;
    let mut failure_wall_ms = 0_u128;
    let mut truncated = false;
    // Per-subject state tracked from OBSERVED outcomes, never assumed.
    //
    // A failed `transition` rolls back, so the subject keeps its previous state.
    // Deriving the next target from the loop index instead would then aim a flip
    // at the state the row is already in — a self-loop, which the legal-transition
    // table correctly refuses with `IllegalTransition`. That refusal is a CASCADE
    // ARTIFACT of the harness, not a defect, and it would contaminate the very
    // counter under measurement. Advancing only on success keeps `storage_errors`
    // clean.
    let mut subject_states = [LifecycleState::Active; SUBJECTS];
    let loop_deadline = Instant::now() + config.budget;
    for i in 0..config.transitions {
        if Instant::now() >= loop_deadline {
            truncated = true;
            break;
        }
        let slot = i % SUBJECTS;
        let subject = format!("tc90-subject-{slot}");
        let to_state = match subject_states[slot] {
            LifecycleState::Active => LifecycleState::Deleted,
            _ => LifecycleState::Active,
        };
        let started = Instant::now();
        let result = engine.transition(&subject, to_state, None);
        let elapsed = started.elapsed().as_millis();
        attempted += 1;
        match result {
            Ok(()) => {
                subject_states[slot] = to_state;
                slowest_ok_ms = slowest_ok_ms.max(elapsed);
            }
            Err(err) => match err {
                EngineError::Storage => {
                    storage_errors += 1;
                    if first_storage_failure.is_none() {
                        failure_wall_ms = elapsed;
                        first_storage_failure = Some((i, err));
                    }
                }
                EngineError::Scheduler => scheduler_errors += 1,
                other => {
                    other_errors += 1;
                    let kind = format!("{other:?}");
                    if !other_error_kinds.contains(&kind) {
                        other_error_kinds.push(kind);
                    }
                }
            },
        }
    }

    stop.store(true, Ordering::Relaxed);
    for writer in writers {
        let _ = writer.join();
    }
    let _ = engine.drain(60_000);
    let embed_calls = calls.load(Ordering::SeqCst);
    let _ = engine.close();

    let outcome = ArmOutcome {
        label,
        attempted,
        truncated,
        first_storage_failure,
        other_error_kinds,
        storage_errors,
        scheduler_errors,
        other_errors,
        competing_writes: competing_writes.load(Ordering::SeqCst),
        competing_write_errors: competing_errors.load(Ordering::SeqCst),
        embed_calls,
        slowest_ok_ms,
        failure_wall_ms,
    };
    outcome.report();
    outcome
}

// ---------------------------------------------------------------------------
// Arm 1 — the repro
// ---------------------------------------------------------------------------

/// **TC-90 REPRO ARM (TC-57-shaped, paced).** A loop of `Engine::transition` calls
/// against a live projection worker.
///
/// **MEASURED 0/20 at `94f09d7d`** (two sessions of 10) — 120 transitions,
/// 122-147 competing writes,
/// ~175 embeds per run. This arm does NOT reproduce TC-90; the stress arm does.
/// It is kept because it is the arm directly comparable to TC-57's, and because a
/// fix must keep it green.
///
/// `#[ignore]`d for the same reason TC-57's pair was: this is a characterization
/// instrument whose baseline result is a MEASUREMENT, not a merge-gate invariant,
/// and `cargo test --workspace` is not a stable signal for concurrency tests
/// anyway (ledger TC-72: ~1 run in 3 fails on plain `main`). The pair is enabled
/// and disabled as one unit — a comparison with one arm running is worse than no
/// comparison. The eventual 0.8.21 fix removes both attributes as its RED->GREEN
/// step, exactly as Slice 21a-2 did for TC-57.
#[test]
#[ignore = "TC-90 characterization arm — run explicitly with --ignored; see the design doc"]
fn tc90_repro_transition_loop_races_projection_worker() {
    let outcome = run_arm(&ArmConfig {
        label: "repro",
        second_writer: true,
        writer_threads: 1,
        burst: 1,
        writer_pace_ms: 3,
        embed_delay_ms: 2,
        transitions: TRANSITIONS,
        budget: Duration::from_secs(120),
    });
    assert!(
        outcome.embed_calls > 0,
        "non-vacuity: the projection worker must have embedded at least one row, else there \
         was no second writer and the promote window never existed"
    );
    assert_protocol_ran(&outcome, TRANSITIONS);
    if let Some((i, err)) = &outcome.first_storage_failure {
        panic!(
            "TC-90 REPRO: transition {i} of {} failed with {err:?} after {} ms \
             (slowest OK transition {} ms); storage={} scheduler={} other={} kinds={:?}",
            outcome.attempted,
            outcome.failure_wall_ms,
            outcome.slowest_ok_ms,
            outcome.storage_errors,
            outcome.scheduler_errors,
            outcome.other_errors,
            outcome.other_error_kinds,
        );
    }
}

// ---------------------------------------------------------------------------
// Arm 2 — the control
// ---------------------------------------------------------------------------

/// **TC-90 CONTROL ARM.** The identical loop and the identical competing write
/// load, with `doc` NOT enrolled as a vector kind — so the worker never produces
/// a `Success` outcome, never commits, and there is no second writer at all.
///
/// This isolates the second writer as the necessary precondition, which is the
/// same isolation TC-57 §4.2 performed. If this arm ever fails, the failure is
/// NOT a promote race and must be reported as a separate finding.
///
/// **MEASURED 0/20 at `94f09d7d`** (two sessions of 10), `embed_calls == 0` in
/// every run.
#[test]
#[ignore = "TC-90 characterization arm — run explicitly with --ignored; see the design doc"]
fn tc90_control_transition_loop_without_second_writer() {
    let outcome = run_arm(&ArmConfig {
        label: "control",
        second_writer: false,
        writer_threads: 1,
        burst: 1,
        writer_pace_ms: 3,
        embed_delay_ms: 2,
        transitions: TRANSITIONS,
        budget: Duration::from_secs(120),
    });
    assert_eq!(
        outcome.embed_calls, 0,
        "the control's defining property: with no vector kind enrolled the worker performs no \
         embed and therefore never commits — there is no second writer"
    );
    assert_protocol_ran(&outcome, TRANSITIONS);
    if let Some((i, err)) = &outcome.first_storage_failure {
        panic!(
            "TC-90 CONTROL FAILED — this is a FINDING, not a flake: transition {i} of {} \
             failed with {err:?} after {} ms with NO second writer present; \
             storage={} scheduler={} other={} kinds={:?}",
            outcome.attempted,
            outcome.failure_wall_ms,
            outcome.storage_errors,
            outcome.scheduler_errors,
            outcome.other_errors,
            outcome.other_error_kinds,
        );
    }
}

// ---------------------------------------------------------------------------
// Arms 3 and 4 — the STRESS pairing
// ---------------------------------------------------------------------------
//
// The paced pair above mirrors TC-57's shape. It is not, on its own, enough to
// support a NEGATIVE: "0/10 at this load" says nothing about a wider window. The
// stress pair widens every knob that could plausibly open one:
//
//   * `burst: 24` > `PROJECTION_COMMIT_BATCH` (16), so the worker commits FULL
//     16-job transactions — the longest write-lock hold it ever takes, i.e. the
//     widest target the promotion can be offered.
//   * `writer_threads: 2` == `PROJECTION_WORKERS`, so both workers can be
//     committing while the transition loop runs.
//   * `writer_pace_ms: 1`, near-saturating: the competing threads contend for the
//     connection mutex almost continuously, which maximises the chance one of them
//     wins the mutex in the gap between `transition`'s `drain` returning and its
//     own acquisition. MEASURED: at `writer_pace_ms: 0` the load is so heavy that
//     `transition`'s own `drain` NEVER reaches idle and every call burns the full
//     30 s timeout into `EngineError::Scheduler` (5 attempts in 180 s, 4 of them
//     `Scheduler`) — an arm that cannot measure the promote race at all.
//   * `embed_delay_ms: 0`: the worker commits microseconds after the competing
//     write releases the connection mutex, rather than 1-2 ms after. This is the
//     knob that MATTERS — see [`ArmConfig::embed_delay_ms`].
//
// A `Scheduler` error here is NOT the defect under test — it is `transition`'s
// own `drain` burning its 30 s timeout under saturating load. It is counted and
// reported SEPARATELY for exactly that reason, and it is a finding in its own
// right (see the design doc).

/// **The protocol, asserted rather than described.** ALL FOUR arms call this
/// BEFORE their `storage_errors` oracle.
///
/// codex §9 round 4 finding 1. The first draft of the stress arms asserted only
/// `attempted > 0`, which is far too weak for what the design doc claims they
/// measure, and no arm asserted its `Scheduler` / other-variant counts at all.
/// Under that bar a run that issued **one** transition, or truncated at its
/// wall-clock budget, or was dominated by `Scheduler` drain timeouts, would still
/// satisfy the 0.8.21 green bar in design doc §4 — without ever having measured
/// the promote race the doc documents. **An acceptance bar is only as good as
/// what it refuses to accept**, so every parameter of the protocol is pinned
/// here.
///
/// These are not aspirational numbers. All four conditions are what the baseline
/// measurement at `94f09d7d` actually produced, in **every** one of the 10 runs
/// of each of the four arms: the full transition count attempted, never
/// truncated, `scheduler_errors == 0`, `other_errors == 0`. Re-measured after
/// this tightening — see design doc §2.2.
///
/// Note the deliberate consequence for [`ArmConfig::budget`]: truncation is now a
/// hard FAILURE rather than a silently shorter measurement. The budget remains a
/// safety valve against an unbounded arm — the `writer_pace_ms: 0` shape burns a
/// 30 s drain timeout per call (design doc §2.5) — but an arm that hits it has
/// not run the protocol, and must say so loudly instead of reporting a clean pass
/// over fewer transitions.
fn assert_protocol_ran(outcome: &ArmOutcome, expected_transitions: usize) {
    assert_eq!(
        outcome.attempted, expected_transitions,
        "non-vacuity: the arm must have issued the FULL protocol it claims to measure — \
         {} of {expected_transitions} transitions issued (truncated={})",
        outcome.attempted, outcome.truncated,
    );
    assert!(
        !outcome.truncated,
        "non-vacuity: the loop hit its wall-clock budget before issuing every transition, so \
         this run measured a SHORTER protocol than the one on the record. Do not loosen this \
         assertion to make it pass — a budget hit is itself a finding (design doc §2.5: under \
         saturating load `transition`'s own `drain` can burn 30 s per call).",
    );
    assert!(
        outcome.competing_writes > 0,
        "non-vacuity: the competing writer(s) must have landed rows, else nothing ever re-armed \
         the dispatcher and there was no load to compare against"
    );
    assert_eq!(
        outcome.scheduler_errors, 0,
        "the arm must be measuring the PROMOTE RACE, not drain starvation: {} of {} transitions \
         returned `EngineError::Scheduler` (a burnt 30 s drain timeout, design doc §2.5). A run \
         dominated by these has not measured TC-90 at all. `writer_pace_ms` is the knob — 1 ms is \
         near-saturating but still lets `drain` reach idle; 0 ms does not.",
        outcome.scheduler_errors, outcome.attempted,
    );
    assert_eq!(
        outcome.other_errors, 0,
        "the arm must produce no error variant other than `Storage`: {} seen, kinds={:?}. \
         `IllegalTransition` here would mean the harness's own subject-state tracking has \
         regressed into the self-loop cascade artifact described at `subject_states`, which \
         contaminates the counter under measurement.",
        outcome.other_errors, outcome.other_error_kinds,
    );
}

/// **TC-90 STRESS REPRO ARM — this is the arm that REPRODUCES.** The widest
/// promote window the in-process shape can be given.
///
/// **MEASURED 10/10 at `94f09d7d`, in each of TWO independent sessions.** Storage
/// failures per 40 transitions:
///
/// * session A (before the assertions were tightened): 3, 4, 10, 3, 6, 6, 5, 5, 8,
///   9 — mean **5.9**
/// * session B (after): 8, 4, 4, 1, 4, 2, 4, 4, 3, 4 — mean **3.8**
///
/// **The 10/10 reproduction replicated; the per-run rate did NOT** (5.9 vs 3.8,
/// same commit, same machine). Pooled range **1-10 per 40**. Judge this arm — and
/// the eventual 0.8.21 fix — on the reproduction rate; do not quote a percentage.
///
/// `attempted == 40`, `truncated == false`, `scheduler_errors == 0` and
/// `other_errors == 0` in all 20 runs, so the signal is the promote race and
/// nothing else — and [`assert_protocol_ran`] now PINS that rather than trusting
/// it. First failure landed as early as index 0 and as late as index 20; the
/// failing call returned in 9-84 ms, essentially all of which is its own `drain`.
#[test]
#[ignore = "TC-90 characterization arm — run explicitly with --ignored; see the design doc"]
fn tc90_stress_transition_loop_under_saturating_burst_load() {
    let outcome = run_arm(&ArmConfig {
        label: "stress",
        second_writer: true,
        writer_threads: 2,
        burst: 24,
        writer_pace_ms: 1,
        embed_delay_ms: 0,
        transitions: STRESS_TRANSITIONS,
        budget: STRESS_BUDGET,
    });
    assert!(
        outcome.embed_calls > 0,
        "non-vacuity: the projection worker must have embedded at least one row, else there \
         was no second writer and the promote window never existed"
    );
    assert_protocol_ran(&outcome, STRESS_TRANSITIONS);
    assert_eq!(
        outcome.storage_errors,
        0,
        "TC-90 STRESS: {} of {} transitions failed with `EngineError::Storage` \
         (first at index {:?} after {} ms) — the promote race IS reachable; \
         scheduler={} other={} kinds={:?}",
        outcome.storage_errors,
        outcome.attempted,
        outcome.first_storage_failure.as_ref().map(|(i, _)| *i),
        outcome.failure_wall_ms,
        outcome.scheduler_errors,
        outcome.other_errors,
        outcome.other_error_kinds,
    );
}

/// **TC-90 STRESS CONTROL.** Identical saturating load, no vector kind — so no
/// worker commit and no second writer. Same single-variable pairing as the paced
/// arms, which is what lets any stress-arm failure be attributed to the worker
/// rather than to the load.
///
/// **MEASURED 0/20 at `94f09d7d`** (two sessions of 10), 1920-1968 competing
/// writes per run and
/// `embed_calls == 0`. The load alone does not break `transition`.
#[test]
#[ignore = "TC-90 characterization arm — run explicitly with --ignored; see the design doc"]
fn tc90_stress_control_without_second_writer() {
    let outcome = run_arm(&ArmConfig {
        label: "stress_control",
        second_writer: false,
        writer_threads: 2,
        burst: 24,
        writer_pace_ms: 1,
        embed_delay_ms: 0,
        transitions: STRESS_TRANSITIONS,
        budget: STRESS_BUDGET,
    });
    assert_eq!(
        outcome.embed_calls, 0,
        "the control's defining property: no vector kind ⇒ no embed ⇒ no worker commit"
    );
    assert_protocol_ran(&outcome, STRESS_TRANSITIONS);
    assert_eq!(
        outcome.storage_errors,
        0,
        "TC-90 STRESS CONTROL FAILED — a FINDING, not a flake: {} storage errors over {} \
         transitions with NO second writer present; scheduler={} other={} kinds={:?}",
        outcome.storage_errors,
        outcome.attempted,
        outcome.scheduler_errors,
        outcome.other_errors,
        outcome.other_error_kinds,
    );
}

// ---------------------------------------------------------------------------
// Mechanism pins — deterministic contention, no race
// ---------------------------------------------------------------------------

static TRANSITION_HANDLER_CALLS: AtomicUsize = AtomicUsize::new(0);

/// Counts invocations and ALWAYS asks SQLite to retry, so "invoked zero times"
/// cannot be explained by a handler that declined on its first call.
fn transition_busy_handler(_attempts: i32) -> bool {
    TRANSITION_HANDLER_CALLS.fetch_add(1, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(5));
    true
}

/// Build a real engine database with one governed `active` row, then close it, so
/// the mechanism pins can drive real `canonical_nodes` SQL rather than a synthetic
/// table.
fn seeded_engine_db(dir: &TempDir, name: &str, logical_id: &str) -> std::path::PathBuf {
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    let calls = Arc::new(AtomicUsize::new(0));
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(CountingDelayEmbedder::new(calls, Duration::ZERO)),
    )
    .expect("open");
    opened.engine.write(&[governed_node(logical_id, "mechanism seed")]).expect("seed");
    opened.engine.drain(60_000).expect("drain");
    opened.engine.close().expect("close");
    path
}

/// **Pin 1 — `transition`'s two statements, replayed VERBATIM on a real
/// `canonical_nodes`, against a HELD write lock.**
///
/// The SQL is copied character-for-character from `lib.rs:8229-8230` and
/// `lib.rs:8259-8260`. A counting busy handler with a 5 s timeout is installed on
/// the promoting connection, so "the busy handler cannot save this" is a measured
/// statement and not an inference from TC-57.
#[test]
fn tc90_mechanism_transition_sql_shape_on_real_schema_is_busy_5() {
    let dir = TempDir::new().expect("tempdir");
    let path = seeded_engine_db(&dir, "tc90_mech_sql", "tc90-mech");

    let a = rusqlite::Connection::open(&path).expect("open a");
    let b = rusqlite::Connection::open(&path).expect("open b");
    TRANSITION_HANDLER_CALLS.store(0, Ordering::SeqCst);
    a.busy_timeout(Duration::from_secs(5)).expect("busy timeout a");
    a.busy_handler(Some(transition_busy_handler)).expect("busy handler a");

    // B is the projection worker mid-`commit_projection_outcomes`: it HOLDS the
    // WAL write lock, uncommitted.
    b.execute_batch("BEGIN IMMEDIATE;").expect("b takes the write lock");
    b.execute(
        "INSERT OR IGNORE INTO _fathomdb_projection_terminal(write_cursor, state) VALUES(?1, ?2)",
        rusqlite::params![9_999_999_i64, "up_to_date"],
    )
    .expect("b writes under its lock");

    // A is `Engine::transition`: BEGIN DEFERRED, then READ, then PROMOTE.
    a.execute_batch("BEGIN DEFERRED;").expect("begin deferred");
    let state: String = a
        .query_row(
            "SELECT state, write_cursor, body FROM canonical_nodes \
             WHERE logical_id = ?1 AND superseded_at IS NULL",
            rusqlite::params!["tc90-mech"],
            |r| r.get::<_, String>(0),
        )
        .expect("the transition read must find the seeded row");
    assert_eq!(state, "active", "fixture sanity: the seeded row is active");

    let started = Instant::now();
    let err = a
        .execute(
            "UPDATE canonical_nodes SET state = ?1, reason = ?2 \
             WHERE logical_id = ?3 AND superseded_at IS NULL",
            rusqlite::params!["deleted", None::<String>, "tc90-mech"],
        )
        .expect_err("the promotion must fail while B holds the write lock");
    let elapsed = started.elapsed();
    let sqlite_err =
        err.sqlite_error().unwrap_or_else(|| panic!("expected a SqliteFailure, got {err:?}"));

    println!(
        "TC90-MECH sql_shape primary={:?} extended={} handler_calls={} elapsed_ms={}",
        sqlite_err.code,
        sqlite_err.extended_code,
        TRANSITION_HANDLER_CALLS.load(Ordering::SeqCst),
        elapsed.as_millis(),
    );

    assert_eq!(
        sqlite_err.code,
        rusqlite::ErrorCode::DatabaseBusy,
        "the primary code of a refused promotion is SQLITE_BUSY"
    );
    assert_eq!(
        sqlite_err.extended_code,
        rusqlite::ffi::SQLITE_BUSY,
        "and the EXTENDED code is plain 5, not SQLITE_BUSY_SNAPSHOT (517) — the same \
         correction TC-57 §0 had to make. Got {}",
        sqlite_err.extended_code
    );
    assert_eq!(
        TRANSITION_HANDLER_CALLS.load(Ordering::SeqCst),
        0,
        "THE POINT: SQLite skips the busy handler when promoting a read transaction \
         (deadlock avoidance), so no `busy_timeout` value could retry `transition` either"
    );
    assert!(
        elapsed < Duration::from_millis(500),
        "and it fails instantly rather than after any backoff (took {elapsed:?})"
    );

    let _ = a.execute_batch("ROLLBACK;");
    let _ = b.execute_batch("ROLLBACK;");
}

/// **Pin 2 — the real [`Engine::transition`] under the same HELD write lock.**
///
/// Deterministic: the lock is taken by a second connection and held for the whole
/// call, so nothing here depends on winning a race. This is the engine-level
/// counterpart to pin 1 and it is what makes "the shape reaches SQLite" a
/// measurement rather than a code reading.
///
/// Note what is NOT asserted: the numeric code. `transition` maps every rusqlite
/// error through `.map_err(|_| EngineError::Storage)` and — unlike `write_inner`
/// (`lib.rs:5097`) — never calls `emit_sqlite_internal_error`, so on this path the
/// code is not merely opaque to the host, it is never emitted at all. Pin 1 is
/// where the code comes from.
#[test]
fn tc90_mechanism_engine_transition_under_held_write_lock_fails_immediately() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join(format!("tc90_mech_engine{SQLITE_SUFFIX}"));
    let calls = Arc::new(AtomicUsize::new(0));
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(CountingDelayEmbedder::new(calls, Duration::ZERO)),
    )
    .expect("open");
    let engine = &opened.engine;
    engine.write(&[governed_node("tc90-held", "held-lock subject")]).expect("seed");
    engine.drain(60_000).expect("drain");

    let blocker = rusqlite::Connection::open(&path).expect("open blocker");
    blocker.execute_batch("BEGIN IMMEDIATE;").expect("blocker takes the write lock");
    blocker
        .execute(
            "INSERT OR IGNORE INTO _fathomdb_projection_terminal(write_cursor, state) \
             VALUES(?1, ?2)",
            rusqlite::params![9_999_998_i64, "up_to_date"],
        )
        .expect("blocker writes under its lock");

    let started = Instant::now();
    let result = engine.transition("tc90-held", LifecycleState::Deleted, None);
    let elapsed = started.elapsed();
    println!(
        "TC90-MECH engine_transition result={:?} elapsed_ms={}",
        result.as_ref().err(),
        elapsed.as_millis()
    );

    assert!(
        matches!(result, Err(EngineError::Storage)),
        "TC-90: `Engine::transition` under a held WAL write lock returns the opaque unit \
         variant `EngineError::Storage`; got {result:?}"
    );
    assert!(
        elapsed < Duration::from_millis(2_000),
        "and it returns IMMEDIATELY — the busy handler is skipped for a promotion, so no \
         backoff happens (took {elapsed:?})"
    );

    let _ = blocker.execute_batch("ROLLBACK;");
    drop(blocker);
    let _ = engine.close();
}

/// **Pin 3 — the CONTROL, and the direct evidence on whether TC-57's R1 remedy
/// transfers.**
///
/// [`Engine::write`] ALREADY opens `BEGIN IMMEDIATE` (`lib.rs:18637`, the Slice
/// 21a-2 fix). Under the identical held-write-lock contention that pin 2 shows
/// killing `transition`, an `IMMEDIATE` transaction goes through the busy handler
/// instead of being refused: it WAITS for the lock and then succeeds.
///
/// The blocker releases after [`HOLD`], well inside rusqlite's default 5 000 ms
/// busy timeout, so the wait is bounded and the test cannot hang.
///
/// **The blocker's readiness is SIGNALLED, never slept for** (codex §9 round 5
/// finding 4). An earlier draft slept a fixed 100 ms and then *assumed* the
/// blocker had taken the lock. That assumption is a race: on a loaded box the
/// writer could reach `BEGIN IMMEDIATE` before the blocker ever acquired the
/// WAL write lock, and then the `elapsed >= HOLD / 2` assertion would fail on a
/// perfectly correct engine. The blocker now sends on a channel only *after* its
/// `BEGIN IMMEDIATE` has returned and it has written under the lock, and the
/// main thread blocks on that before it starts timing. The lock therefore
/// genuinely **is** held for the whole measured call — the same property the
/// other two pins get from being single-threaded — so this pin observes the
/// contention rather than racing for it.
#[test]
fn tc90_mechanism_control_engine_write_under_held_write_lock_survives() {
    /// How long the blocker holds the write lock. Must be comfortably under
    /// rusqlite's 5 000 ms default busy timeout so the waiting writer succeeds.
    const HOLD: Duration = Duration::from_millis(900);
    /// Upper bound on waiting for the blocker's readiness signal. Generous: it
    /// exists only so a blocker that never starts fails LOUDLY instead of
    /// hanging the suite. It is not a timing assertion.
    const READY_TIMEOUT: Duration = Duration::from_secs(30);

    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join(format!("tc90_mech_control{SQLITE_SUFFIX}"));
    let calls = Arc::new(AtomicUsize::new(0));
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(CountingDelayEmbedder::new(calls, Duration::ZERO)),
    )
    .expect("open");
    let engine = &opened.engine;
    engine.write(&[governed_node("tc90-control-seed", "control seed")]).expect("seed");
    engine.drain(60_000).expect("drain");

    // Readiness SIGNAL, not a sleep. `ready_tx` fires only once the blocker's
    // `BEGIN IMMEDIATE` has returned and it has written under the lock, so when
    // `recv_timeout` below returns, the WAL write lock IS held. `HOLD` starts at
    // that same instant, so the writer's wait is bounded below by `HOLD` minus
    // only the microseconds it takes to reach its own `BEGIN` — no scheduling
    // race decides whether this pin passes.
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
    let holder = {
        let path = path.clone();
        std::thread::spawn(move || {
            let blocker = rusqlite::Connection::open(&path).expect("open blocker");
            blocker.execute_batch("BEGIN IMMEDIATE;").expect("blocker takes the write lock");
            blocker
                .execute(
                    "INSERT OR IGNORE INTO _fathomdb_projection_terminal(write_cursor, state) \
                     VALUES(?1, ?2)",
                    rusqlite::params![9_999_997_i64, "up_to_date"],
                )
                .expect("blocker writes under its lock");
            // The lock is now HELD. Only now may the writer start.
            ready_tx.send(()).expect("blocker signals that it holds the write lock");
            std::thread::sleep(HOLD);
            blocker.execute_batch("ROLLBACK;").expect("blocker releases");
        })
    };
    ready_rx
        .recv_timeout(READY_TIMEOUT)
        .expect("the blocker must signal that it HOLDS the write lock before the writer starts");

    let started = Instant::now();
    let result = engine.write(&[governed_node("tc90-control-2", "control write")]);
    let elapsed = started.elapsed();
    let _ = holder.join();
    println!("TC90-MECH control_write ok={} elapsed_ms={}", result.is_ok(), elapsed.as_millis());

    assert!(
        result.is_ok(),
        "the `BEGIN IMMEDIATE` writer must SURVIVE the identical contention that refuses \
         `transition`'s promotion — got {result:?}"
    );
    assert!(
        elapsed >= HOLD / 2,
        "and it must have WAITED for the lock ({elapsed:?} < half of {HOLD:?}), which is the \
         proof the busy handler was consulted rather than skipped. The blocker SIGNALLED that \
         it held the lock before this call started and then held it for {HOLD:?}, so a short \
         elapsed here is an engine result, not a lost scheduling race"
    );

    let _ = engine.close();
}
