//! PR-9 item 2 — ADR-0.6.0-embedder-protocol **Invariant 5** (per-`embed()`
//! watchdog) on the projection path.
//!
//! Context: EU-5f's `catch_unwind` guard (Finding A) catches a *panicking*
//! projection worker, but a *hung* (non-panicking) embed still parks a
//! worker forever — `drain` never reaches idle and wedges into
//! `EngineError::Scheduler`. Invariant 5 says every `embed()` runs under a
//! per-call deadline (default 30s, configurable); a timeout fails the call
//! with `EmbedderError::Timeout` and MUST NOT corrupt writer state.
//!
//! These tests drive the PRODUCTION path only (`engine.write` + `drain`),
//! with a configurable, lowered watchdog deadline so they need not wait 30s.
//!
//! RED before the watchdog exists:
//!   * `hung_embed_times_out_instead_of_wedging_drain` — `drain` wedges into
//!     `EngineError::Scheduler` because the parked worker never decrements
//!     `active_jobs`.
//!
//! GREEN after: the hung embed surfaces `Timeout`, the existing retry path
//! records a terminal failure, `drain` returns, and writer state is intact.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::lifecycle::ProjectionStatus;
use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

const DIM: u32 = 8;

fn unit_vector(dim: u32) -> Vector {
    let mut v = vec![0.0_f32; dim as usize];
    v[0] = 1.0;
    v
}

/// Embedder whose `embed()` blocks while `park` is true (simulating a hung
/// forward pass), then returns a valid unit vector once the test releases
/// it. `calls` counts entries so a test can assert the embed was actually
/// attempted. The park loop sleeps in small increments so releasing it is
/// observed promptly.
#[derive(Debug)]
struct ParkingEmbedder {
    identity: EmbedderIdentity,
    park: Arc<AtomicBool>,
    calls: AtomicU64,
}

impl ParkingEmbedder {
    fn new(park: Arc<AtomicBool>) -> Self {
        Self {
            identity: EmbedderIdentity::new("parking", "rev-a", DIM),
            park,
            calls: AtomicU64::new(0),
        }
    }
}

impl Embedder for ParkingEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        // Block while parked, simulating a hung forward pass. The watchdog
        // must abandon this call (finish + discard per Invariant 5); we never
        // abort the thread. The block is bounded (~20s, well above both the
        // lowered watchdog deadline and the test's drain timeout) purely as a
        // safety valve so a regression that removes the watchdog cannot hang
        // the test binary forever on a parked worker — it surfaces as a
        // wedged `drain` (the RED signal) instead.
        let mut waited = Duration::ZERO;
        let cap = Duration::from_secs(20);
        while self.park.load(Ordering::Relaxed) && waited < cap {
            thread::sleep(Duration::from_millis(10));
            waited += Duration::from_millis(10);
        }
        Ok(unit_vector(DIM))
    }
}

/// Embedder that hangs (parks while `park` is true) ONLY on bodies containing
/// "hang", and returns a valid vector immediately otherwise. Models an
/// embedder that wedges on some inputs but works on others — the case where a
/// consecutive-timeout breaker would keep resetting and never latch, leaking a
/// thread per hung input forever.
#[derive(Debug)]
struct SometimesHangEmbedder {
    identity: EmbedderIdentity,
    park: Arc<AtomicBool>,
}

impl SometimesHangEmbedder {
    fn new(park: Arc<AtomicBool>) -> Self {
        Self { identity: EmbedderIdentity::new("sometimes-hang", "rev-a", DIM), park }
    }
}

impl Embedder for SometimesHangEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        if text.contains("hang") {
            let mut waited = Duration::ZERO;
            let cap = Duration::from_secs(20);
            while self.park.load(Ordering::Relaxed) && waited < cap {
                thread::sleep(Duration::from_millis(10));
                waited += Duration::from_millis(10);
            }
        }
        Ok(unit_vector(DIM))
    }
}

/// Embedder that sleeps a fixed sub-budget delay then returns Ok — models a
/// legitimately-slow cold embed. The watchdog MUST NOT false-timeout this.
#[derive(Debug)]
struct SlowEmbedder {
    identity: EmbedderIdentity,
    delay: Duration,
}

impl SlowEmbedder {
    fn new(delay: Duration) -> Self {
        Self { identity: EmbedderIdentity::new("slow", "rev-a", DIM), delay }
    }
}

impl Embedder for SlowEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        thread::sleep(self.delay);
        Ok(unit_vector(DIM))
    }
}

fn fixture_path(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    (dir, path)
}

fn doc(body: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body.to_string(),
        source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
        logical_id: None,
        state: fathomdb_engine::InitialState::Active,
        reason: None,
    }
}

fn wait_for_status(
    engine: &Engine,
    kind: &str,
    expected: ProjectionStatus,
    timeout: Duration,
) -> bool {
    let started = Instant::now();
    loop {
        let observed = engine.projection_status_for_test(kind).expect("projection status");
        if observed == expected {
            return true;
        }
        if started.elapsed() >= timeout {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Invariant 5 core: a hung embed must surface a timeout so `drain` reaches
/// idle, rather than parking a worker until `drain` wedges into
/// `EngineError::Scheduler`. RED before the watchdog (drain wedges); GREEN
/// after (the job is recorded as a terminal failure and drain returns).
#[test]
fn hung_embed_times_out_instead_of_wedging_drain() {
    let (_dir, path) = fixture_path("pr9_watchdog_hang");
    let park = Arc::new(AtomicBool::new(true));
    let embedder = Arc::new(ParkingEmbedder::new(park.clone()));
    let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");

    // Lower the watchdog deadline + shorten retry backoff so the test runs
    // in well under a second rather than the 30s production default.
    engine.set_embed_timeout_ms_for_test(150);
    engine.set_projection_retry_delays_for_test(&[10, 10]);

    let receipt = engine.write(&[doc("hung-doc")]).expect("write");

    // RED (no watchdog): the worker parks forever, active_jobs stays > 0,
    // and drain times out into EngineError::Scheduler.
    // GREEN (watchdog): each attempt times out, the job is recorded as a
    // terminal EmbedderError failure, the scheduler reaches idle, drain Ok.
    let drained = engine.drain(8_000);
    assert!(
        drained.is_ok(),
        "drain must not wedge on a hung embed (Invariant 5 watchdog); got {drained:?}"
    );
    assert!(
        wait_for_status(&engine, "doc", ProjectionStatus::Failed, Duration::from_secs(2)),
        "the hung doc must be recorded as a terminal projection failure"
    );
    assert_eq!(
        engine.projection_failure_count_for_test(receipt.cursor).expect("failure count"),
        1,
        "the timed-out embed must record exactly one terminal failure"
    );
    assert!(
        !engine.has_vector_for_cursor_for_test(receipt.cursor).expect("has_vector"),
        "no vector should be stored for the timed-out doc"
    );

    // Release the park so the abandoned watchdog threads exit cleanly.
    park.store(false, Ordering::Relaxed);
    assert!(
        embedder.calls.load(Ordering::Relaxed) >= 1,
        "the embedder must have been called at least once"
    );
}

/// Invariant 5: a timed-out embed must NOT corrupt writer state — a
/// subsequent normal write projects cleanly. (ADR test: "mock embedder
/// sleeping > 30s ... does not corrupt subsequent writes".)
#[test]
fn timed_out_embed_does_not_corrupt_subsequent_writes() {
    let (_dir, path) = fixture_path("pr9_watchdog_nocorrupt");
    let park = Arc::new(AtomicBool::new(true));
    let embedder = Arc::new(ParkingEmbedder::new(park.clone()));
    let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    engine.set_embed_timeout_ms_for_test(150);
    engine.set_projection_retry_delays_for_test(&[10, 10]);

    // First doc hangs and times out.
    engine.write(&[doc("hung-doc")]).expect("write hung");
    engine.drain(8_000).expect("drain after hung embed must return");

    // Release the hang; a fresh write must project successfully — proving
    // the timed-out embed left the writer / projection cursor uncorrupted.
    park.store(false, Ordering::Relaxed);
    let receipt = engine.write(&[doc("healthy-doc")]).expect("write healthy");
    engine.drain(8_000).expect("drain healthy doc");
    assert!(
        wait_for_status(&engine, "doc", ProjectionStatus::UpToDate, Duration::from_secs(2)),
        "a normal write after a timed-out embed must project to UpToDate"
    );
    assert!(
        engine.has_vector_for_cursor_for_test(receipt.cursor).expect("has_vector"),
        "the post-timeout healthy doc must have a stored vector"
    );
}

/// PR-9 circuit breaker: a persistently-hung embedder must trip the breaker
/// once abandoned (timed-out) embed threads pile up to the threshold, after
/// which jobs fail fast WITHOUT attempting an embed — so abandoned watchdog
/// threads cannot leak without bound (one per timed-out job). RED before the
/// breaker exists: the circuit never opens and every doc spawns/abandons an
/// embed. GREEN after: the circuit latches and embed calls stop well short of
/// the doc count.
#[test]
fn persistent_hang_trips_circuit_breaker_and_bounds_thread_leak() {
    let (_dir, path) = fixture_path("pr9_circuit");
    let park = Arc::new(AtomicBool::new(true)); // hang for the whole test
    let embedder = Arc::new(ParkingEmbedder::new(park.clone()));
    let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    engine.set_embed_timeout_ms_for_test(80); // fast timeouts
                                              // Non-empty retry delays so the breaker is exercised on the MULTI-attempt
                                              // retry path (a single job retries here), not just at job entry — the
                                              // per-attempt re-check must bail mid-retry once the breaker latches.
    engine.set_projection_retry_delays_for_test(&[10, 10]);
    engine.set_embed_circuit_threshold_for_test(4); // latch at 4 live embed threads

    let nodes: Vec<PreparedWrite> = (0..40).map(|i| doc(&format!("hang-{i}"))).collect();
    engine.write(&nodes).expect("write");
    engine.drain(20_000).expect("drain must return despite the hung embedder");

    assert!(
        engine.embed_circuit_open_for_test(),
        "circuit breaker must latch open once abandoned embed threads reach the threshold"
    );
    let calls = embedder.calls.load(Ordering::Relaxed);
    assert!(
        calls <= 16,
        "breaker must stop further embed attempts (bounding abandoned threads) \
         even across the retry path; saw {calls} embed calls across 40 docs"
    );

    // Release the park so the abandoned watchdog threads exit cleanly.
    park.store(false, Ordering::Relaxed);
}

/// PR-9 circuit breaker, intermittent-hang case (codex BLOCK remediation): an
/// embedder that hangs on SOME inputs and returns on others must STILL latch
/// the breaker as its abandoned threads pile up — a consecutive-timeout design
/// would reset on every interleaved success and leak forever. With the
/// live-thread-count breaker, the accumulating hung threads trip it regardless.
#[test]
fn intermittent_hang_still_trips_circuit_breaker() {
    let (_dir, path) = fixture_path("pr9_circuit_intermittent");
    let park = Arc::new(AtomicBool::new(true));
    let embedder = Arc::new(SometimesHangEmbedder::new(park.clone()));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    engine.set_embed_timeout_ms_for_test(80);
    engine.set_projection_retry_delays_for_test(&[10, 10]);
    engine.set_embed_circuit_threshold_for_test(4);

    // Alternate hanging and healthy docs: the healthy ones return Ok (and would
    // reset a consecutive-timeout counter), the hanging ones leak threads.
    let nodes: Vec<PreparedWrite> = (0..40)
        .map(|i| if i % 2 == 0 { doc(&format!("hang-{i}")) } else { doc(&format!("ok-{i}")) })
        .collect();
    engine.write(&nodes).expect("write");
    engine.drain(20_000).expect("drain must return despite intermittent hangs");

    assert!(
        engine.embed_circuit_open_for_test(),
        "breaker must latch from accumulating abandoned threads even when \
         healthy embeds are interleaved (it must not reset on success)"
    );

    park.store(false, Ordering::Relaxed);
}

/// Invariant 5: the watchdog must NOT false-timeout a legitimately-slow
/// (under-budget) embed. A 100ms embed under a generous deadline succeeds.
#[test]
fn slow_but_under_budget_embed_succeeds() {
    let (_dir, path) = fixture_path("pr9_watchdog_slow_ok");
    let embedder = Arc::new(SlowEmbedder::new(Duration::from_millis(100)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    engine.set_embed_timeout_ms_for_test(5_000);

    let receipt = engine.write(&[doc("slow-doc")]).expect("write");
    engine.drain(8_000).expect("drain slow-but-under-budget doc");
    assert!(
        wait_for_status(&engine, "doc", ProjectionStatus::UpToDate, Duration::from_secs(2)),
        "a slow-but-under-budget embed must succeed, not false-timeout"
    );
    assert!(
        engine.has_vector_for_cursor_for_test(receipt.cursor).expect("has_vector"),
        "the slow-but-under-budget doc must have a stored vector"
    );
}
