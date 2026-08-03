//! 0.8.20 Slice 21a-2 (R-20-CR) — **TC-57 fix: the WORKER-SIDE half, measured.**
//!
//! ## Why this file exists
//!
//! The TC-57 fix removes the caller-side promotion. Historically, the
//! characterization's biggest residual risk
//! (`dev/design/0.8.20-tc57-write-race-characterization.md` §7, "What I am unsure
//! about" item 3) is the *other* direction: `commit_projection_outcomes`
//! had been the worker-side promotion. TC-91 now gives the worker the same
//! `BEGIN IMMEDIATE` writer intent and reports exceptional commit failures.
//!
//! `tc57_governed_write_race.rs` observes caller writes; this file retains the
//! worker pressure instrument and now requires zero duplicate embeds.
//!
//! ## What a worker-side commit failure actually looks like — READ THIS
//!
//! It is **not** a `'failed'` terminal and **not** a `projection_failures` audit
//! row. Those two are written by the `ProjectionOutcome::Failure` arm, which
//! represents an **embed** failure. A commit failure instead rolls its terminal
//! transaction back, emits a lifecycle diagnostic, and leaves the canonical row
//! pending for the normal redispatch path.
//!
//! So the sharp instrument is **re-embeds**: every row in this fixture carries a
//! UNIQUE body, the embedder counts calls PER TEXT, and a text embedded twice is a
//! row that was dispatched twice. The `'failed'` terminal / `projection_failures` counts
//! are recorded too (they are the observable the slice brief named, and they must
//! stay at zero), and TC-91 adds explicit lifecycle-observation coverage for
//! exceptional commit failures.
//!
//! **Historical baseline:** before TC-91, `repeat_embeds` was useful only as a
//! relative number. MEASURED at
//! baseline `41a81c17`: ~104 of 200 rows are already embedded twice, on BOTH arms,
//! including the anonymous arm where zero caller errors occur. That pre-existing
//! ~50 % duplicate-dispatch rate is unrelated to TC-57 (it is present with no
//! write race at all) and is out of scope here — it is reported, not fixed. What
//! matters for the historical Part C comparison only; current assertions require
//! zero repeats.
//!
//! ## Equalized load — why every caller error is retried
//!
//! At baseline the governed arm dies on its third write, so an un-retried loop
//! would compare ~3 rows of worker pressure against ~200. Both arms here retry a
//! failed `Engine::write` up to [`MAX_ATTEMPTS`] times so that **the same number
//! of rows** reaches the worker before and after the fix, and the caller-error
//! count is reported separately rather than terminating the load.
//!
//! ## Throughput
//!
//! Characterization §7 uncertainty 2: holding the WAL write lock from `BEGIN`
//! rather than from the first write statement is a longer hold. Each arm sums the
//! wall time of its **successful** `Engine::write` calls only (`ok_write_wall_ms`)
//! so the number is a per-write cost comparison uncontaminated by retry overhead,
//! and reports the total alongside it.
//!
//! The anonymous arm is the CLEAN throughput comparison: it never fails at
//! baseline, so its before/after numbers contain no retries in either direction —
//! and since the fix is applied UNCONDITIONALLY it is exactly the arm where a
//! regression from taking the lock earlier would show up first. MEASURED, N=5
//! each: anonymous `ok_write_wall_ms` 410.0 → 415.2 mean (+1.3 %), anonymous
//! `total_wall_ms` 712.6 → 720.8 (+1.2 %) — i.e. no detectable cost.
//!
//! The governed arm's `ok_write_wall_ms` is NOT comparable across the fix (before,
//! it excludes ~98 failed attempts and counts retries that ran against a lock the
//! 5 ms backoff had already let go). Its honest number is `total_wall_ms`: 920.8 →
//! 720.2 mean, i.e. the fix is ~22 % FASTER end-to-end, because the failed-write
//! and backoff cycles are gone.

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, InitialState, PreparedWrite, SourceId};
use fathomdb_schema::SQLITE_SUFFIX;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Rows per arm. Large enough that the worker commits many times against a live
/// writer, small enough to run in a few seconds.
const ROWS: usize = 200;

/// Bounded — never an unbounded retry. A row that cannot land in this many tries
/// is counted as permanently lost and reported.
const MAX_ATTEMPTS: usize = 8;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// Deterministic, in-process, call-counting embedder with a 1 ms delay — the same
/// shape as `tc57_governed_write_race.rs`'s, so the two files put comparable
/// pressure on the worker.
#[derive(Debug)]
struct CountingDelayEmbedder {
    identity: EmbedderIdentity,
    calls: Arc<AtomicUsize>,
    /// Per-text call counts. `embed` is called with the row's extracted field, and
    /// every row in this fixture carries a UNIQUE body, so a text embedded twice
    /// is a row that was dispatched twice — i.e. a projection outcome that never
    /// became durable. This is what makes "re-embed" a falsifiable count rather
    /// than an inference from a total.
    texts: Arc<Mutex<HashMap<String, usize>>>,
}

impl CountingDelayEmbedder {
    fn new(calls: Arc<AtomicUsize>, texts: Arc<Mutex<HashMap<String, usize>>>) -> Self {
        Self { identity: EmbedderIdentity::new("deterministic", "rev-a", 384), calls, texts }
    }
}

impl Embedder for CountingDelayEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self
            .texts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(text.to_string())
            .or_insert(0) += 1;
        std::thread::sleep(Duration::from_millis(1));
        let mut v = vec![0.0_f32; self.identity.dimension as usize];
        v[0] = 1.0;
        Ok(v)
    }
}

fn node(i: usize, governed: bool) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: format!(r#"{{"summary":"meaning {i}"}}"#),
        source_id: SourceId::new("test:fixture").expect("source id"),
        logical_id: if governed { Some(format!("tc57-wp-{i}")) } else { None },
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

fn ro(path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only")
}

fn db_path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn count(conn: &rusqlite::Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(-1)
}

/// Everything one arm measured. Printed as a single greppable line by
/// [`LoadOutcome::report`] so a before/after comparison is a `grep`, not a
/// transcription.
struct LoadOutcome {
    label: &'static str,
    rows_written: usize,
    rows_lost: usize,
    caller_errors: usize,
    ok_write_wall_ms: u128,
    total_wall_ms: u128,
    embed_calls: usize,
    /// How many DISTINCT texts were embedded more than once. Each one is a row
    /// whose projection outcome was computed and then never became durable.
    repeated_texts: usize,
    /// Total embeds beyond the first for any text — i.e. total wasted embeds.
    repeat_embeds: usize,
    drain_ok: bool,
    drain_ms: u128,
    failure_audit_rows: i64,
    failed_terminals: i64,
    vector_rows: i64,
}

impl LoadOutcome {
    fn report(&self) {
        println!(
            "TC57-PARTC arm={} rows_written={} rows_lost={} caller_errors={} \
             embed_calls={} repeated_texts={} repeat_embeds={} failed_terminals={} \
             failure_audit_rows={} vector_rows={} drain_ok={} drain_ms={} \
             ok_write_wall_ms={} total_wall_ms={}",
            self.label,
            self.rows_written,
            self.rows_lost,
            self.caller_errors,
            self.embed_calls,
            self.repeated_texts,
            self.repeat_embeds,
            self.failed_terminals,
            self.failure_audit_rows,
            self.vector_rows,
            self.drain_ok,
            self.drain_ms,
            self.ok_write_wall_ms,
            self.total_wall_ms,
        );
    }
}

/// Run one equalized load arm against a live projection worker.
fn run_load(label: &'static str, governed: bool) -> LoadOutcome {
    let dir = TempDir::new().expect("tempdir");
    let path = db_path(&dir, if governed { "tc57_wp_governed" } else { "tc57_wp_anonymous" });
    let calls = Arc::new(AtomicUsize::new(0));
    let texts: Arc<Mutex<HashMap<String, usize>>> = Arc::new(Mutex::new(HashMap::new()));
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(CountingDelayEmbedder::new(calls.clone(), texts.clone())),
    )
    .expect("open");
    let engine = &opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");

    let mut rows_written = 0_usize;
    let mut rows_lost = 0_usize;
    let mut caller_errors = 0_usize;
    let mut ok_write_wall_ms = 0_u128;
    let loop_started = Instant::now();
    for i in 0..ROWS {
        let mut landed = false;
        for _ in 0..MAX_ATTEMPTS {
            let started = Instant::now();
            let result = engine.write(&[node(i, governed)]);
            let elapsed = started.elapsed().as_millis();
            match result {
                Ok(_) => {
                    ok_write_wall_ms += elapsed;
                    rows_written += 1;
                    landed = true;
                    break;
                }
                Err(_) => {
                    caller_errors += 1;
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
        if !landed {
            rows_lost += 1;
        }
        // Yield so the worker actually interleaves instead of being starved.
        std::thread::sleep(Duration::from_millis(1));
    }
    let total_wall_ms = loop_started.elapsed().as_millis();

    let drain_started = Instant::now();
    let drain_ok = engine.drain(60_000).is_ok();
    let drain_ms = drain_started.elapsed().as_millis();
    let embed_calls = calls.load(Ordering::SeqCst);
    let (repeated_texts, repeat_embeds) = {
        let seen = texts.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let repeated = seen.values().filter(|n| **n > 1).count();
        let extra: usize = seen.values().map(|n| n.saturating_sub(1)).sum();
        (repeated, extra)
    };
    opened.engine.close().expect("close");

    let conn = ro(&path);
    let outcome = LoadOutcome {
        label,
        rows_written,
        rows_lost,
        caller_errors,
        ok_write_wall_ms,
        total_wall_ms,
        embed_calls,
        repeated_texts,
        repeat_embeds,
        drain_ok,
        drain_ms,
        failure_audit_rows: count(
            &conn,
            "SELECT COUNT(*) FROM operational_mutations
             WHERE collection_name = 'projection_failures'",
        ),
        failed_terminals: count(
            &conn,
            "SELECT COUNT(*) FROM _fathomdb_projection_terminal WHERE state = 'failed'",
        ),
        vector_rows: count(&conn, "SELECT COUNT(*) FROM _fathomdb_vector_rows"),
    };
    outcome.report();
    outcome
}

// ---------------------------------------------------------------------------
// The measurements
// ---------------------------------------------------------------------------

/// **Part C — the worker-side risk, on the GOVERNED arm.**
///
/// This is the arm the fix changes semantically. Assertions are the ones that
/// must hold in BOTH directions of the risk: the worker must not start
/// terminating rows as `failed`, must not accumulate audit rows, and must not be
/// pushed into a re-embed storm by a writer that now holds the lock from `BEGIN`.
///
/// TC-91 makes duplicate worker dispatch a correctness regression: every
/// successful projection outcome must become durable on its first commit.
#[test]
fn tc57_worker_side_commit_pressure_governed() {
    let outcome = run_load("governed", true);

    assert!(
        outcome.embed_calls > 0 && outcome.vector_rows > 0,
        "non-vacuity: the worker must have embedded and committed real rows, else this \
         measurement says nothing about worker-side commit pressure"
    );
    assert_eq!(outcome.rows_lost, 0, "every row must land within {MAX_ATTEMPTS} attempts");
    assert_eq!(
        outcome.failed_terminals, 0,
        "the fix must not convert a caller-side write race into worker-side `failed` terminals"
    );
    assert_eq!(outcome.failure_audit_rows, 0, "nor into `projection_failures` audit rows");
    assert!(outcome.drain_ok, "drain must still reach idle");
    assert_eq!(
        outcome.repeat_embeds, 0,
        "worker outcomes must not be dropped and re-embedded ({} repeats over {} rows, {} texts repeated)",
        outcome.repeat_embeds, outcome.rows_written, outcome.repeated_texts
    );
}

/// **Part C — the ANONYMOUS arm, which is the clean throughput comparison.**
///
/// Anonymous writes never failed at baseline, so nothing here is contaminated by
/// retries in either direction. It is also the arm that pays for the fix being
/// UNCONDITIONAL rather than gated on `logical_id`: if taking the write lock at
/// `BEGIN` cost anything for a batch whose first statement is already the INSERT,
/// this is where it would appear.
#[test]
fn tc57_worker_side_commit_pressure_anonymous() {
    let outcome = run_load("anonymous", false);

    assert!(
        outcome.embed_calls > 0 && outcome.vector_rows > 0,
        "non-vacuity: the worker must have embedded and committed real rows"
    );
    assert_eq!(outcome.rows_lost, 0, "every row must land within {MAX_ATTEMPTS} attempts");
    assert_eq!(
        outcome.caller_errors, 0,
        "the anonymous pressure arm must not produce caller write errors"
    );
    assert_eq!(outcome.failed_terminals, 0, "no worker-side `failed` terminals");
    assert_eq!(outcome.failure_audit_rows, 0, "no `projection_failures` audit rows");
    assert!(outcome.drain_ok, "drain must still reach idle");
    assert_eq!(outcome.repeat_embeds, 0, "anonymous worker outcomes must not be re-embedded");
}
