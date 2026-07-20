//! 0.8.20 Slice 5 fix-2 (codex §9 [P2]) — the shared `erase_source` /
//! `excise_source` path must DRAIN BEFORE IT FREEZES.
//!
//! **The defect.** `erase_source_shared` froze the projection scanner and only
//! then called `drain`. `drain` → `wait_for_idle` consults
//! `database_has_pending_projection_work`, which reads the DATABASE, not the
//! in-memory queue: a row written moments earlier is "pending" until a worker
//! projects it. But the dispatcher loop parks while `state.frozen` is set, so it
//! can never scan and enqueue that row. The result is a deadlock-by-construction
//! that only ends when `LIFECYCLE_DRAIN_TIMEOUT_MS` (30s) elapses, after which
//! the caller gets `EngineError::Scheduler` — for the ordinary first-use
//! sequence "write a vector-indexed row, then erase it".
//!
//! `purge` already carries the correct ordering (settle every pending
//! projection UNFROZEN first, then freeze, then confirm idle, then erase) with a
//! comment naming this exact failure mode. This file pins that the shared source
//! erasure path has it too.
//!
//! **Why the fixture strands work deterministically.** `GatedEmbedder::embed`
//! blocks until the test releases it, so no projection job can complete before
//! the erasure call. The dispatcher can therefore enqueue at most
//! `PROJECTION_INFLIGHT_LIMIT` (2 workers × 16 commit batch = 32) rows before it
//! parks on the in-flight budget; the fixture writes far more than that, so a
//! large remainder is provably still UNSCANNED at the instant `erase_source` is
//! entered. On the broken ordering that remainder is unreachable forever. A
//! fixture that wrote only a row or two would race the dispatcher and could pass
//! on the broken code, which is worthless.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, EngineError, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

/// More than `PROJECTION_INFLIGHT_LIMIT` (32) so the dispatcher cannot have
/// enqueued the whole batch by the time the erasure freezes the scanner.
const SEEDED_ROWS: usize = 96;

/// How long the fixture holds every `embed()` call before releasing it. Long
/// enough that the erasure verb is unambiguously entered first; short enough
/// that the GREEN path stays well under a second of blocking.
const GATE_HOLD: Duration = Duration::from_millis(150);

/// The erasure must finish far faster than the 30s lifecycle drain timeout the
/// broken ordering waits out. This budget is the test's real signal.
const PROMPT_BUDGET: Duration = Duration::from_secs(15);

/// An embedder whose `embed()` parks until the test opens the gate. Holding the
/// workers guarantees a backlog of un-enqueued projection work.
#[derive(Debug)]
struct GatedEmbedder {
    identity: EmbedderIdentity,
    vector: Vector,
    open: Mutex<bool>,
    cvar: Condvar,
    /// Set once any embed actually ran, so the test can prove the fixture was
    /// exercised rather than silently no-op.
    embedded: AtomicBool,
}

impl GatedEmbedder {
    fn new(dim: u32) -> Self {
        let mut vector = vec![0.0_f32; dim as usize];
        vector[0] = 1.0;
        Self {
            identity: EmbedderIdentity::new("drain-order-test", "rev-a", dim),
            vector,
            open: Mutex::new(false),
            cvar: Condvar::new(),
            embedded: AtomicBool::new(false),
        }
    }

    fn release(&self) {
        if let Ok(mut open) = self.open.lock() {
            *open = true;
            self.cvar.notify_all();
        }
    }
}

impl Embedder for GatedEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        let mut open = self
            .open
            .lock()
            .map_err(|_| EmbedderError::Failed { message: "gate poisoned".to_string() })?;
        while !*open {
            open = self
                .cvar
                .wait(open)
                .map_err(|_| EmbedderError::Failed { message: "gate poisoned".to_string() })?;
        }
        self.embedded.store(true, Ordering::SeqCst);
        Ok(self.vector.clone())
    }
}

fn seed_rows(engine: &Engine, source_id: &str) {
    let writes: Vec<PreparedWrite> = (0..SEEDED_ROWS)
        .map(|i| PreparedWrite::Node {
            kind: "doc".to_string(),
            body: format!("drain ordering body {i}"),
            source_id: fathomdb_engine::SourceId::new(source_id).expect("test source id"),
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        })
        .collect();
    engine.write(&writes).expect("write");
}

/// codex §9 [P2] — `erase_source` immediately after writing vector-indexed rows
/// must succeed promptly, not stall for the full lifecycle drain timeout and
/// return `EngineError::Scheduler`.
#[test]
fn erase_source_drains_before_freezing() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join(format!("erase_drain_order{SQLITE_SUFFIX}"));

    let embedder = Arc::new(GatedEmbedder::new(8));
    let opened =
        Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open with embedder");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    seed_rows(&opened.engine, "S1");

    // Open the gate shortly AFTER the erasure verb has been entered, so the
    // verb's own ordering decides whether the backlog can still be projected.
    let releaser = {
        let embedder = embedder.clone();
        std::thread::spawn(move || {
            std::thread::sleep(GATE_HOLD);
            embedder.release();
        })
    };

    let started = Instant::now();
    let result = opened.engine.erase_source("S1");
    let elapsed = started.elapsed();
    releaser.join().expect("releaser thread");

    assert!(
        !matches!(result, Err(EngineError::Scheduler)),
        "erase_source froze the projection scanner before draining, so the dispatcher \
         could never enqueue the {SEEDED_ROWS} just-written rows and drain timed out \
         into Scheduler after {elapsed:?}"
    );
    let report = result.expect("erase_source must succeed");
    assert!(
        report.nodes_excised > 0,
        "fixture: the erasure must actually have removed the seeded rows, got {report:?}"
    );
    assert!(
        elapsed < PROMPT_BUDGET,
        "erase_source must complete promptly, not wait out the lifecycle drain timeout; \
         took {elapsed:?}"
    );
    assert!(
        embedder.embedded.load(Ordering::SeqCst),
        "fixture sanity: the gated embedder must have been exercised, otherwise the test \
         never created the backlog it claims to"
    );

    opened.engine.close().expect("close");
}

/// The `operator` spelling shares one implementation with `erase_source`, so it
/// inherits the same ordering. Pinned separately because the two verbs are
/// separately reachable and a future refactor could split the path.
#[cfg(feature = "operator")]
#[test]
fn excise_source_drains_before_freezing() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join(format!("excise_drain_order{SQLITE_SUFFIX}"));

    let embedder = Arc::new(GatedEmbedder::new(8));
    let opened =
        Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open with embedder");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    seed_rows(&opened.engine, "S1");

    let releaser = {
        let embedder = embedder.clone();
        std::thread::spawn(move || {
            std::thread::sleep(GATE_HOLD);
            embedder.release();
        })
    };

    let started = Instant::now();
    let result = opened.engine.excise_source("S1");
    let elapsed = started.elapsed();
    releaser.join().expect("releaser thread");

    assert!(
        !matches!(result, Err(EngineError::Scheduler)),
        "excise_source froze the projection scanner before draining; drain timed out into \
         Scheduler after {elapsed:?}"
    );
    result.expect("excise_source must succeed");
    assert!(elapsed < PROMPT_BUDGET, "excise_source must complete promptly; took {elapsed:?}");

    opened.engine.close().expect("close");
}
