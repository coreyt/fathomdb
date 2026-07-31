//! 0.8.20 Slice 23 characterization and Slice 40 regression coverage for
//! **TC-91**.
//!
//! TC-91 (ledger `seq-129`, p2) is two defects on the same code path, which this
//! file HYPOTHESISES — but does **not** establish — share one mechanism (see
//! "What those numbers DO and DO NOT prove" below, and design doc §5.5.2):
//!
//! * **(a)** roughly half of all embeds at baseline are DUPLICATES. It reproduces
//!   on the ANONYMOUS arm too, so it is not the governed-write race TC-57 fixed,
//!   and it did not go away with that fix. Harm class: wasted embed work.
//!   Invisible to correctness tests, because a duplicate embed produces the right
//!   answer — twice.
//! * **(b)** at the Slice 23 baseline, the projection worker discarded its
//!   commit result. Slice 40 propagates the result to the worker loop and starts
//!   the commit transaction with `BEGIN IMMEDIATE`.
//!
//! Slice 23 established the characterization at baseline `94f09d7d`. Slice 40
//! turns the bounded governed-rate and forced-contention arms into regressions;
//! the remaining timing-sensitive characterization arms stay opt-in. Written
//! characterization: `dev/design/0.8.20-tc90-tc91-characterization.md`.
//!
//! ## The durable point of (b), and why it is the most valuable thing here
//!
//! At baseline the two worker paths silently discarded
//! `commit_projection_outcomes` errors.
//!
//! A `'failed'` terminal and a `projection_failures` audit row are written by the
//! `ProjectionOutcome::Failure` arm (`lib.rs:13962-13984`), which represents an
//! **embed** failure. A **commit** failure never reaches that arm at all: the
//! whole transaction rolls back, the row keeps `terminal IS NULL`, the worker loop
//! sets `pending_scan = true` (`lib.rs:12663`), the dispatcher re-fetches the row
//! and it is embedded AGAIN.
//!
//! Therefore **counting `'failed'` terminal states was structurally incapable of
//! seeing this class of failure at baseline.**
//! [`tc91_forced_commit_contention_preserves_single_embed`] exercises the same
//! lock shape and requires the remedy to preserve every computed outcome.
//!
//! ## The measurement trap that had to be disarmed FIRST
//!
//! `run_vector_equivalence_probe` (`lib.rs:15309`) embeds the 45-probe fixture at
//! OPEN. Two different figures are on the record and BOTH are true, of different
//! opens: **90** embeds on the population open (populate 45 + check 45), **45** on
//! every open thereafter. Ledger `seq-125` (the TC-71 close) states the
//! consequence: *absolute embed counts are unusable as an oracle across a reopen*
//! — every embed assertion must be a DELTA from a post-open snapshot.
//!
//! **And TC-68 LANDED in Slice 22 (`572475f2`)**, adding the fingerprint verdict
//! cache (`probe_verification_is_cached`, `lib.rs:15566`). The ledger note calling
//! the 45 re-embeds something TC-68 "is scheduled to cache away" PREDATES that
//! landing, so the 45 must not be inherited as a constant.
//! [`tc91_probe_embed_cost_per_open_at_head`] MEASURES it at HEAD instead.
//!
//! Every duplicate figure below is therefore taken from a SINGLE open whose
//! embed log starts empty, and ingest embeds are separated from probe embeds by
//! text: this fixture's bodies all carry [`INGEST_MARKER`], which no probe text
//! contains. The two counts are reported separately, never summed.
//!
//! **MEASURED at HEAD (`94f09d7d`)** by [`tc91_probe_embed_cost_per_open_at_head`]:
//! open 1 (no vector kind) **0**; open 2 (populate + check) **90**; open 3, after
//! TC-68's verdict cache, **0**. The "45 on every open thereafter" figure is
//! therefore NO LONGER TRUE at HEAD — TC-68 took the steady-state reopen probe
//! cost to zero, not merely reduced it. `probe_calls == 0` is asserted in every
//! duplicate arm, so the separation is verified rather than assumed.
//!
//! ## MEASURED at `94f09d7d`
//!
//! These are the counts. What they do and do not license is the section that
//! follows the table — in particular they do **not** establish that (a) is (b).
//!
//! | arm | shape | runs | ingest duplicates / rows |
//! |---|---|---|---|
//! | [`tc91_duplicate_embed_rate_governed`] | 200 x 1-row writes, 1 ms apart | 10 | 102-106 / 200 (**52.1 %**) |
//! | [`tc91_duplicate_embed_rate_anonymous`] | same, `logical_id: None` | 10 | 103-110 / 200 (**52.8 %**) |
//! | [`tc91_control_unblocked_worker_commits`] | ONE 96-row batch, no concurrent writer | 10 | **0 / 96** |
//! | forced-contention baseline | same batch + held write lock | 10 | 80-96 / 96 |
//! | [`tc91_mechanism_duplicate_rate_versus_write_cadence`] | 60 rows, 1 ms vs 25 ms apart | 10 | 31-33 vs **0** |
//!
//! Governed and anonymous are indistinguishable, confirming the ledger claim that
//! this is not the TC-57 race.
//!
//! ## What those numbers DO and DO NOT prove — read this before quoting them
//!
//! Two claims are **measured**, and they are the deliverable:
//!
//! 1. The duplicate rate is **CADENCE-SENSITIVE** under this 1 ms embed / 1 ms
//!    write shape: 32.5/60 at 1 ms spacing, **0/60 at 25 ms**, 10/10, and 0/96 for
//!    a single batch with no concurrent writer. So the ~50 % is a property of the
//!    write **cadence**, not a constant of the scheduler, and quoting it without
//!    the cadence it was measured at is meaningless.
//! 2. An **externally held WAL write lock makes discarded worker commits
//!    invisible**: 86.4/96 duplicates while `'failed'` terminals and
//!    `projection_failures` rows are both ZERO, 10/10.
//!
//! The tempting third claim — *"(a) IS (b)"*, i.e. that a duplicate REQUIRES a
//! second writer holding the WAL write lock across the worker's commit — is a
//! **HYPOTHESIS strongly implicated by (1) + (2), not a measured identity**
//! (codex §9 round 4 finding 4). The gap is specific and worth naming, because it
//! is not an oversight:
//!
//! > **There is no direct instrumentation of worker commit failures in the
//! > baseline duplicate arms — because the defect under study is precisely that
//! > those failures are discarded by `let _ =`.** TC-91 **(b) is the reason
//! > TC-91 (a) cannot be closed by measurement.** The forced-lock experiment
//! > reproduces the mechanism, but it supplies the lock holder externally; it
//! > cannot show that every baseline duplicate came from the engine's own writer.
//!
//! The supporting code reading is real — `commit_batch` holds `BEGIN IMMEDIATE`
//! (`lib.rs:18637`, the TC-57 fix) while `commit_projection_outcomes`
//! (`lib.rs:13809`) is a DEFERRED read-then-upgrade — but it is a reading, and
//! §0 of the TC-57 characterization is the standing warning about confident
//! readings on this exact code path. Landing **R-C** (stop discarding the commit
//! result) is what would convert the hypothesis into a measurement; that is the
//! main reason R-C is recommended FIRST in the design doc §7.
//!
//! In EVERY one of the 50 runs above, `failed_terminals == 0` and
//! `failure_audit_rows == 0`.
//!
//! ## Test profile — which targets run in the DEFAULT `cargo test` gate
//!
//! Decided deliberately per target, not by reflex (codex §9 round 4 finding 3).
//! The gating question is TC-72: roughly 1 workspace run in 3 already fails on
//! plain `main`, so anything timing-sensitive added to the default profile makes
//! that worse and must earn its place.
//!
//! **`#[ignore]`d — timing-characterization arms:**
//! [`tc91_duplicate_embed_rate_anonymous`],
//! [`tc91_mechanism_duplicate_rate_versus_write_cadence`], and
//! [`tc91_control_unblocked_worker_commits`]. They remain measurements, not
//! merge-gate invariants. Run them with `--ignored`.
//!
//! **LIVE in the default profile:**
//! [`tc91_duplicate_embed_rate_governed`] is a bounded 48-row tight-cadence
//! regression and [`tc91_forced_commit_contention_preserves_single_embed`] holds
//! a real external WAL write lock. Each asserts exactly zero duplicate embeds.
//! [`tc91_probe_embed_cost_per_open_at_head`] is a measurement too, but it is
//! **sequential and deterministic**: three opens of one workspace, one write, every
//! `drain` awaited to idle, no second thread and no wall-clock assertion anywhere.
//! Its value comes from a checked-in fixture (`vector_equivalence_probes.txt`) and
//! a code path, so it has no run-to-run variance to flake on — and since it now
//! pins `0 / 90 / 0` exactly, leaving it live is what actually defends the
//! correction the design doc publishes. Gating it would put the corrected figure
//! behind a flag nobody runs.

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, InitialState, PreparedWrite, SourceId};
use fathomdb_schema::SQLITE_SUFFIX;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Present in every body this fixture writes and in no probe text, so ingest
/// embeds and probe embeds can be told apart WITHIN one open rather than inferred
/// from a total.
const INGEST_MARKER: &str = "tc91marker";

/// Rows per duplicate-rate arm.
const ROWS: usize = 200;

/// A bounded tight-cadence sample for the default regression gate. It is large
/// enough to reproduce the pre-fix defect without turning the gate into a
/// scheduler measurement suite.
const GATED_ROWS: usize = 48;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// Records the exact `text` of every `embed()` call.
///
/// A text embedded twice is a row that was DISPATCHED twice — i.e. an outcome
/// that was computed and then never became durable. Every row this fixture writes
/// carries a unique body, which is what makes "duplicate" a falsifiable count
/// rather than an inference from a total.
#[derive(Debug)]
struct RecordingEmbedder {
    identity: EmbedderIdentity,
    calls: Arc<AtomicUsize>,
    texts: Arc<Mutex<HashMap<String, usize>>>,
    delay: Duration,
}

impl RecordingEmbedder {
    fn new(log: &EmbedLog, delay: Duration) -> Self {
        Self {
            identity: EmbedderIdentity::new("deterministic", "rev-a", 384),
            calls: Arc::clone(&log.calls),
            texts: Arc::clone(&log.texts),
            delay,
        }
    }
}

impl Embedder for RecordingEmbedder {
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
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        let mut v = vec![0.0_f32; self.identity.dimension as usize];
        v[0] = 1.0;
        Ok(v)
    }
}

/// Shared embed log, so the counts survive the embedder being moved into the
/// engine.
#[derive(Clone, Default)]
struct EmbedLog {
    calls: Arc<AtomicUsize>,
    texts: Arc<Mutex<HashMap<String, usize>>>,
}

/// Ingest / probe split of an embed log, with duplicates counted per class.
#[derive(Debug, Default, Clone, Copy)]
struct EmbedSplit {
    /// Total `embed()` calls whose text carries [`INGEST_MARKER`].
    ingest_calls: usize,
    /// Distinct ingest texts — i.e. rows reached at least once.
    ingest_distinct: usize,
    /// Ingest embeds beyond the first for any text. THE duplicate count.
    ingest_duplicates: usize,
    /// Distinct ingest texts embedded more than once.
    ingest_repeated_texts: usize,
    /// Total `embed()` calls whose text does NOT carry [`INGEST_MARKER`] — the
    /// vector-equivalence probe, reported SEPARATELY and never summed in.
    probe_calls: usize,
}

impl EmbedLog {
    fn total(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn split(&self) -> EmbedSplit {
        let texts = self.texts.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut split = EmbedSplit::default();
        for (text, n) in texts.iter() {
            if text.contains(INGEST_MARKER) {
                split.ingest_calls += n;
                split.ingest_distinct += 1;
                split.ingest_duplicates += n.saturating_sub(1);
                if *n > 1 {
                    split.ingest_repeated_texts += 1;
                }
            } else {
                split.probe_calls += n;
            }
        }
        split
    }
}

fn node(i: usize, governed: bool) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: format!(r#"{{"summary":"{INGEST_MARKER} row {i} unique"}}"#),
        source_id: SourceId::new("test:fixture").expect("source id"),
        logical_id: if governed { Some(format!("tc91-{i}")) } else { None },
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

fn count(conn: &rusqlite::Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(-1)
}

/// The two counters a future agent would reach for to quantify worker
/// reliability at the Slice 23 baseline.
struct TerminalCensus {
    failed_terminals: i64,
    up_to_date_terminals: i64,
    failure_audit_rows: i64,
    vector_rows: i64,
    node_rows: i64,
}

fn census(path: &Path) -> TerminalCensus {
    let conn = ro(path);
    TerminalCensus {
        failed_terminals: count(
            &conn,
            "SELECT COUNT(*) FROM _fathomdb_projection_terminal WHERE state = 'failed'",
        ),
        up_to_date_terminals: count(
            &conn,
            "SELECT COUNT(*) FROM _fathomdb_projection_terminal WHERE state = 'up_to_date'",
        ),
        failure_audit_rows: count(
            &conn,
            "SELECT COUNT(*) FROM operational_mutations
             WHERE collection_name = 'projection_failures'",
        ),
        vector_rows: count(&conn, "SELECT COUNT(*) FROM _fathomdb_vector_rows"),
        node_rows: count(&conn, "SELECT COUNT(*) FROM canonical_nodes"),
    }
}

// ---------------------------------------------------------------------------
// TC-91 (a) — the baseline duplicate rate, with the probe separated out
// ---------------------------------------------------------------------------

struct DuplicateOutcome {
    label: &'static str,
    rows: usize,
    pace_ms: u64,
    split: EmbedSplit,
    total_embeds: usize,
    drain_ok: bool,
    census: TerminalCensus,
}

impl DuplicateOutcome {
    fn report(&self) {
        println!(
            "TC91-DUP arm={} rows={} pace_ms={} total_embeds={} ingest_calls={} \
             ingest_distinct={} ingest_duplicates={} ingest_repeated_texts={} probe_calls={} \
             dup_rate_pct={:.1} failed_terminals={} up_to_date_terminals={} \
             failure_audit_rows={} vector_rows={} node_rows={} drain_ok={}",
            self.label,
            self.rows,
            self.pace_ms,
            self.total_embeds,
            self.split.ingest_calls,
            self.split.ingest_distinct,
            self.split.ingest_duplicates,
            self.split.ingest_repeated_texts,
            self.split.probe_calls,
            100.0 * self.split.ingest_duplicates as f64 / self.rows as f64,
            self.census.failed_terminals,
            self.census.up_to_date_terminals,
            self.census.failure_audit_rows,
            self.census.vector_rows,
            self.census.node_rows,
            self.drain_ok,
        );
    }
}

/// One duplicate-rate arm, measured inside a SINGLE open.
///
/// The workspace is fresh, so `_fathomdb_vector_kinds` is EMPTY at open and the
/// vector-equivalence probe returns early having done ZERO embeds
/// (`lib.rs:15348-15353`). The kind is enrolled AFTER open. Every `embed()` in
/// this arm is therefore an ingest embed, and `probe_calls` is expected to be 0 —
/// which the arm asserts, so the separation is verified rather than assumed.
fn run_duplicate_arm(
    label: &'static str,
    governed: bool,
    rows: usize,
    pace_ms: u64,
) -> DuplicateOutcome {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join(format!("tc91_dup_{label}{SQLITE_SUFFIX}"));
    let log = EmbedLog::default();
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(RecordingEmbedder::new(&log, Duration::from_millis(1))),
    )
    .expect("open");
    let engine = &opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");

    for i in 0..rows {
        engine.write(&[node(i, governed)]).expect("write must succeed");
        // Yield so the worker interleaves rather than being starved. `pace_ms` is
        // the independent variable of the mechanism experiment below.
        std::thread::sleep(Duration::from_millis(pace_ms));
    }
    let drain_ok = engine.drain(120_000).is_ok();
    let split = log.split();
    let total_embeds = log.total();
    opened.engine.close().expect("close");

    let outcome = DuplicateOutcome {
        label,
        rows,
        pace_ms,
        split,
        total_embeds,
        drain_ok,
        census: census(&path),
    };
    outcome.report();
    outcome
}

/// Assertions shared by both duplicate arms. Non-vacuity first: an arm where the
/// worker never embedded, or never committed, says nothing about a duplicate rate.
fn assert_duplicate_arm(outcome: &DuplicateOutcome) {
    assert!(outcome.drain_ok, "non-vacuity: drain must reach idle, else work is still outstanding");
    assert_eq!(
        outcome.split.ingest_distinct, outcome.rows,
        "non-vacuity: every one of the {} unique rows must have been embedded at least once",
        outcome.rows
    );
    assert_eq!(
        outcome.census.vector_rows, outcome.rows as i64,
        "non-vacuity: every row must have reached a durable vector, else the duplicates would \
         be explained by rows that simply never landed"
    );
    assert_eq!(
        outcome.split.probe_calls, 0,
        "the probe MUST contribute zero embeds on a fresh workspace whose vector kind is \
         enrolled after open (`lib.rs:15348-15353`) — if this is non-zero the ingest/probe \
         separation is broken and every duplicate number below is contaminated"
    );
}

/// **TC-91 (a) — bounded GOVERNED regression.** `logical_id: Some(..)`.
#[test]
fn tc91_duplicate_embed_rate_governed() {
    let outcome = run_duplicate_arm("governed", true, GATED_ROWS, 1);
    assert_duplicate_arm(&outcome);
    assert_eq!(
        outcome.split.ingest_duplicates, 0,
        "TC-91 (a): {} duplicate embeds over {GATED_ROWS} tight-cadence rows ({} distinct rows \
         embedded more than once) — the worker transaction must preserve every computed outcome \
         exactly once",
        outcome.split.ingest_duplicates, outcome.split.ingest_repeated_texts,
    );
}

/// **TC-91 (a) — ANONYMOUS arm.** `logical_id: None`.
///
/// This arm is the load-bearing one for attribution. The anonymous write path has
/// NO read-then-upgrade window and never produced a caller error even before the
/// TC-57 fix, so a duplicate rate here cannot be caused by the governed-write
/// race. If both arms show the same rate, the cause is elsewhere.
#[test]
#[ignore = "TC-91 characterization arm — run explicitly with --ignored; see the design doc"]
fn tc91_duplicate_embed_rate_anonymous() {
    let outcome = run_duplicate_arm("anonymous", false, ROWS, 1);
    assert_duplicate_arm(&outcome);
    assert_eq!(
        outcome.split.ingest_duplicates, 0,
        "TC-91 (a): {} duplicate embeds over {ROWS} rows ({} distinct rows embedded more than \
         once) on the ANONYMOUS arm — so the cause is NOT the governed-write race TC-57 fixed",
        outcome.split.ingest_duplicates, outcome.split.ingest_repeated_texts,
    );
}

// ---------------------------------------------------------------------------
// TC-91 (a) — the MECHANISM experiment
// ---------------------------------------------------------------------------

/// Rows for the mechanism experiment. Fewer than [`ROWS`] because the spaced arm
/// pays `SPACED_PACE_MS` per row.
const MECHANISM_ROWS: usize = 60;

/// Write spacing for the SPACED arm. Comfortably longer than the 1 ms embed, so a
/// worker's commit for row *i* has finished before row *i+1*'s `Engine::write`
/// opens its `BEGIN IMMEDIATE` transaction.
const SPACED_PACE_MS: u64 = 25;

/// **The CADENCE experiment for TC-91 (a). The question it is aimed at — is the
/// duplicate rate caused by the ENGINE'S OWN WRITER holding the WAL write lock
/// across the worker's commit? — is one it narrows but does not answer.**
///
/// Two arms of the same governed load differing ONLY in write spacing:
///
/// * **tight** (`pace_ms: 1`) — a write lands roughly every millisecond, which is
///   also the embed latency, so `commit_projection_outcomes`' DEFERRED promotion
///   (`lib.rs:13809`, reads at `:13813` / `:13828` before its writes at `:13863`)
///   frequently lands while `commit_batch`'s `BEGIN IMMEDIATE` (`lib.rs:18637`)
///   holds the lock.
/// * **spaced** (`pace_ms: 25`) — the same rows, the same embedder, the same
///   worker; only the overlap is removed.
///
/// **What a collapse in the spaced arm does and does not establish.** It
/// establishes that the duplicate rate is CADENCE-SENSITIVE — the ~50 % is a
/// property of the write cadence, not a fixed constant of the scheduler. It does
/// **not**, on its own, establish that TC-91 (a) *is* TC-91 (b): spacing the
/// writes changes overlap, but nothing in this arm observes a worker commit
/// failing, because `let _ =` (`lib.rs:12711`) discards exactly that signal. Read
/// together with the forced-contention baseline arm
/// it strongly implicates the engine's own writer; see the module header's
/// "What those numbers DO and DO NOT prove" for the precise boundary.
///
/// `#[ignore]`d with the other concurrency arms; it is a measurement, not a
/// merge-gate invariant. See the module header's test-profile inventory.
#[test]
#[ignore = "TC-91 characterization arm — run explicitly with --ignored; see the design doc"]
fn tc91_mechanism_duplicate_rate_versus_write_cadence() {
    let tight = run_duplicate_arm("mech_tight", true, MECHANISM_ROWS, 1);
    assert_duplicate_arm(&tight);
    let spaced = run_duplicate_arm("mech_spaced", true, MECHANISM_ROWS, SPACED_PACE_MS);
    assert_duplicate_arm(&spaced);

    println!(
        "TC91-MECH tight_duplicates={} spaced_duplicates={} rows={MECHANISM_ROWS}",
        tight.split.ingest_duplicates, spaced.split.ingest_duplicates
    );

    assert!(
        tight.split.ingest_duplicates > 0,
        "non-vacuity: the TIGHT arm must reproduce the duplicate rate, else there is no effect \
         to explain (duplicates={})",
        tight.split.ingest_duplicates
    );
    assert_eq!(
        spaced.split.ingest_duplicates, 0,
        "TC-91 (a) CADENCE SENSITIVITY: spacing writes to {SPACED_PACE_MS} ms took duplicates \
         from {} to {} over {MECHANISM_ROWS} identical rows. The variable this arm manipulates \
         is write SPACING; the HYPOTHESISED mechanism it implicates — not one it observes — is \
         how often the engine's own `BEGIN IMMEDIATE` writer holds the WAL write lock across \
         the worker's DEFERRED commit promotion. This arm cannot see a worker commit fail, \
         because `let _ =` (`lib.rs:12711`) discards exactly that signal, so it establishes \
         neither the identity nor the uniqueness of the source. If this ever becomes non-zero, \
         the cadence sensitivity itself has changed and the design doc's mechanism section must \
         be re-opened.",
        tight.split.ingest_duplicates, spaced.split.ingest_duplicates,
    );
}

// ---------------------------------------------------------------------------
// The probe's cost at HEAD — the trap, disarmed by measurement
// ---------------------------------------------------------------------------

/// Probes in the committed `vector_equivalence_probes.txt` fixture — the
/// non-empty, non-comment lines `vector_equivalence_probes()` parses
/// (`lib.rs:15242-15251`). Deterministic: the fixture is checked in, so this is a
/// property of the tree, not of the run.
const PROBE_FIXTURE_SIZE: usize = 45;

/// What the POPULATION open costs: `probe_populate_or_check` populates the
/// baseline (one embed per probe) and then immediately checks against it (one
/// more each).
const POPULATION_OPEN_PROBE_EMBEDS: usize = PROBE_FIXTURE_SIZE * 2;

/// **What the vector-equivalence probe actually costs per open at HEAD.**
///
/// Not `#[ignore]`d — see the module header's test-profile inventory. It is
/// sequential and fully deterministic (no threads, no racing writer, every
/// `drain` awaited), and it is the figure every other embed measurement in this
/// repo has to subtract.
///
/// Three opens of ONE workspace, each measured as a DELTA (ledger `seq-125`:
/// absolute embed counts are unusable as an oracle across a reopen):
///
/// 1. **open 1** — fresh workspace, no vector kind yet ⇒ the probe returns early
///    at `lib.rs:15351` having embedded nothing. The kind is enrolled after open.
/// 2. **open 2** — a vector kind now exists and `_fathomdb_embed_probe` is empty
///    ⇒ `probe_populate_or_check` (`lib.rs:15384-15389`) POPULATES 45 and then
///    CHECKS 45.
/// 3. **open 3** — the TC-68 fingerprint verdict cache (`lib.rs:15566`) decides
///    whether the 45 re-embeds happen again.
///
/// **All three opens are pinned EXACTLY** (`0 / 90 / 0`), which is a change from
/// this file's first draft — codex §9 round 4 finding 2. The first draft asserted
/// only `open3 < open2`, while the design doc published a measured `0 / 90 / 0`
/// as a *correction* to a stale on-the-record figure of 45 re-embeds per open. A
/// regression back to 45 would have passed `45 < 90` silently, leaving the doc
/// publishing a zero the tree no longer honoured. When a characterization's whole
/// value is the number, the number is what must be asserted.
#[test]
fn tc91_probe_embed_cost_per_open_at_head() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join(format!("tc91_probe{SQLITE_SUFFIX}"));

    // --- open 1: fresh workspace, kind enrolled AFTER open ------------------
    let log1 = EmbedLog::default();
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(RecordingEmbedder::new(&log1, Duration::ZERO)),
    )
    .expect("open 1");
    let open1_probe_embeds = log1.total();
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
    opened.engine.write(&[node(0, true)]).expect("seed write");
    opened.engine.drain(60_000).expect("drain 1");
    opened.engine.close().expect("close 1");

    // --- open 2: vector kind present, probe table empty ---------------------
    let log2 = EmbedLog::default();
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(RecordingEmbedder::new(&log2, Duration::ZERO)),
    )
    .expect("open 2");
    let open2_probe_embeds = log2.total();
    opened.engine.close().expect("close 2");

    // --- open 3: probe table populated; TC-68 cache decides -----------------
    let log3 = EmbedLog::default();
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(RecordingEmbedder::new(&log3, Duration::ZERO)),
    )
    .expect("open 3");
    let open3_probe_embeds = log3.total();
    opened.engine.close().expect("close 3");

    println!(
        "TC91-PROBE open1_no_vector_kind={open1_probe_embeds} open2_populate_then_check=\
         {open2_probe_embeds} open3_after_tc68_cache={open3_probe_embeds}"
    );

    assert_eq!(
        open1_probe_embeds, 0,
        "a fresh workspace with no registered vector kind must cost ZERO probe embeds at open \
         (`lib.rs:15348-15353`)"
    );
    assert_eq!(
        open2_probe_embeds, POPULATION_OPEN_PROBE_EMBEDS,
        "the POPULATION open must cost exactly {POPULATION_OPEN_PROBE_EMBEDS} = populate \
         {PROBE_FIXTURE_SIZE} + check {PROBE_FIXTURE_SIZE} \
         (`probe_populate_or_check`, `lib.rs:15384-15389`); measured {open2_probe_embeds}. \
         If you deliberately changed `vector_equivalence_probes.txt`, this constant AND the \
         figure quoted in `dev/design/0.8.20-tc90-tc91-characterization.md` §5.2 must both move \
         with it — the doc publishes this number as a correction other agents subtract."
    );
    assert_eq!(
        open3_probe_embeds, 0,
        "TC-68's fingerprint verdict cache (landed Slice 22, `572475f2`) took the steady-state \
         reopen probe cost to ZERO, not merely to 'cheaper'; measured open2={open2_probe_embeds} \
         open3={open3_probe_embeds}. This is asserted EXACTLY, on purpose: the design doc §5.2 \
         corrects a stale on-the-record figure of 45 re-embeds per open, and a weaker \
         `open3 < open2` bar would let a regression back to 45 pass while the doc still \
         published 0. A wrong correction is worse than the stale figure it replaces."
    );
}

// ---------------------------------------------------------------------------
// TC-91 (b) — the silently discarded worker commit
// ---------------------------------------------------------------------------

struct CommitLossOutcome {
    label: &'static str,
    split: EmbedSplit,
    drain_ok: bool,
    census: TerminalCensus,
    blocked_ms: u128,
}

impl CommitLossOutcome {
    fn report(&self) {
        println!(
            "TC91-LOSS arm={} rows={} blocked_ms={} ingest_calls={} ingest_distinct={} \
             ingest_duplicates={} ingest_repeated_texts={} failed_terminals={} \
             up_to_date_terminals={} failure_audit_rows={} vector_rows={} drain_ok={}",
            self.label,
            LOSS_ROWS,
            self.blocked_ms,
            self.split.ingest_calls,
            self.split.ingest_distinct,
            self.split.ingest_duplicates,
            self.split.ingest_repeated_texts,
            self.census.failed_terminals,
            self.census.up_to_date_terminals,
            self.census.failure_audit_rows,
            self.census.vector_rows,
            self.drain_ok,
        );
    }
}

/// Rows for the forced-commit-failure arms. Comfortably more than
/// `PROJECTION_INFLIGHT_LIMIT` (32) so several worker commits are attempted while
/// the write lock is held.
const LOSS_ROWS: usize = 96;

/// How long the external connection HOLDS the WAL write lock. Long enough that
/// many worker commits are attempted inside the window.
const HOLD: Duration = Duration::from_millis(1_500);

/// Force worker commit failures by holding the WAL write lock from OUTSIDE the
/// engine, then measure what the engine reports.
///
/// The blocker is an ordinary second connection on the same file running `BEGIN
/// IMMEDIATE`. That is exactly the contention TC-90's mechanism pins characterize,
/// applied in the opposite direction: `commit_projection_outcomes` (`lib.rs:13809`)
/// opens rusqlite's DEFERRED default and reads (`lib.rs:13813`, `lib.rs:13828`)
/// before it writes, so its promotion is refused with plain `SQLITE_BUSY` and the
/// busy handler is never consulted. `let _ =` then discards the whole thing.
///
/// `blocker = false` is the CONTROL: identical load, no external lock.
fn run_commit_loss_arm(label: &'static str, blocker: bool) -> CommitLossOutcome {
    let dir = TempDir::new().expect("tempdir");
    let path: PathBuf = dir.path().join(format!("tc91_loss_{label}{SQLITE_SUFFIX}"));
    let log = EmbedLog::default();
    let opened = Engine::open_with_embedder_for_test(
        &path,
        // A slow embedder widens the window in which the worker is mid-batch and
        // therefore about to attempt a commit.
        Arc::new(RecordingEmbedder::new(&log, Duration::from_millis(15))),
    )
    .expect("open");
    let engine = &opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");

    let batch: Vec<PreparedWrite> = (0..LOSS_ROWS).map(|i| node(i, true)).collect();
    engine.write(&batch).expect("write must succeed");

    let blocked_ms = if blocker {
        let started = Instant::now();
        let conn = rusqlite::Connection::open(&path).expect("open blocker");
        conn.execute_batch("BEGIN IMMEDIATE;").expect("blocker takes the WAL write lock");
        conn.execute(
            "INSERT OR IGNORE INTO _fathomdb_projection_terminal(write_cursor, state) \
             VALUES(?1, ?2)",
            rusqlite::params![9_999_996_i64, "up_to_date"],
        )
        .expect("blocker writes under its lock");
        std::thread::sleep(HOLD);
        conn.execute_batch("ROLLBACK;").expect("blocker releases");
        drop(conn);
        started.elapsed().as_millis()
    } else {
        0
    };

    let drain_ok = engine.drain(120_000).is_ok();
    let split = log.split();
    opened.engine.close().expect("close");

    let outcome = CommitLossOutcome { label, split, drain_ok, census: census(&path), blocked_ms };
    outcome.report();
    outcome
}

/// **TC-91 (b) — forced contention must preserve the computed outcomes.**
///
/// A second connection holds the WAL write lock while the worker attempts to
/// commit. The worker must wait through SQLite's busy handler and then commit its
/// already-computed outcomes, rather than losing them and redispatching rows for
/// a second embed.
///
#[test]
fn tc91_forced_commit_contention_preserves_single_embed() {
    let outcome = run_commit_loss_arm("blocked", true);

    assert!(
        outcome.blocked_ms >= HOLD.as_millis(),
        "non-vacuity: the blocker must actually have held the write lock ({} ms < {} ms)",
        outcome.blocked_ms,
        HOLD.as_millis()
    );
    assert!(outcome.drain_ok, "the engine must reach idle after the lock is released");
    assert_eq!(
        outcome.census.vector_rows, LOSS_ROWS as i64,
        "every row must land after the contention is released"
    );
    assert_eq!(
        outcome.split.ingest_duplicates, 0,
        "TC-91 (b): {} computed outcomes were lost during forced write contention and embedded \
         again; the worker commit must wait and preserve each outcome exactly once",
        outcome.split.ingest_duplicates,
    );
    assert_eq!(
        outcome.census.failed_terminals, 0,
        "a successfully retried worker commit is not an embed failure"
    );
    assert_eq!(
        outcome.census.failure_audit_rows, 0,
        "a successfully retried worker commit must not create an embed-failure audit row"
    );
}

/// **TC-91 (b) CONTROL.** Identical load, no external lock.
///
/// Establishes the baseline duplicate count the blocked arm is read against, and
/// confirms the two counters are zero for the ordinary reason (nothing failed) as
/// well as for the pathological one — which is precisely why they cannot
/// discriminate, and precisely why a future agent must not use them.
#[test]
#[ignore = "TC-91 characterization arm — run explicitly with --ignored; see the design doc"]
fn tc91_control_unblocked_worker_commits() {
    let outcome = run_commit_loss_arm("unblocked", false);

    assert!(outcome.drain_ok, "drain must reach idle");
    assert_eq!(
        outcome.split.ingest_distinct, LOSS_ROWS,
        "non-vacuity: every row must have been embedded at least once"
    );
    assert_eq!(
        outcome.census.vector_rows, LOSS_ROWS as i64,
        "non-vacuity: every row must have reached a durable vector"
    );
    assert_eq!(
        outcome.census.failed_terminals, 0,
        "the control's point: the counters read ZERO here for the ORDINARY reason"
    );
    assert_eq!(outcome.census.failure_audit_rows, 0, "same, for the audit rows");
}
