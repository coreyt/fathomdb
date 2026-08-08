//! 0.8.20 Slice 20 (R-20-DR) — `dense_readiness` + the ATOMIC READINESS FLIP.
//!
//! Acceptance signal (plan §3, verbatim): *"readiness never reports ready with
//! pending embeds; flip is atomic under concurrent write."* Design of record:
//! `dev/design/record-lifecycle-protocol/projection-registry-and-async-embed.md`
//! §3 (the split staleness contract) and §4.1 invariant 1 (the flip).
//!
//! Two falsifiable properties, both asserted on RAW TABLES rather than on a
//! derived status:
//!
//! - **P1** — with the projection scheduler frozen and a vector-kind row
//!   written, `read.projections` reports `dense_readiness = embedding` AND the
//!   raw `_fathomdb_vector_rows` / `vector_default` rows are absent. Unfreeze,
//!   drain, and it flips to `ready` WITH the raw vector row present.
//! - **P2** — under a CONCURRENT writer there is no interleaving in which
//!   readiness reads `ready` while a vector row is absent. The forbidden torn
//!   state (`ready` without the vector) is unobservable; the only torn state
//!   seen is the tolerated one (`embedding` without the vector).
//!
//! Why the raw tables and not a status verb: `projection_status` maps
//! `_ => UpToDate`, so a MISSING terminal row and a healthy one are
//! indistinguishable there — a status-level assertion would have passed against
//! the TC-45 bug part (a) of this slice fixed. Every assertion below reads
//! `_fathomdb_projection_terminal` / `_fathomdb_vector_rows` / `canonical_nodes`
//! directly.
//!
//! **No Slice 20 schema step.** Readiness is DERIVED from the existing pending-
//! projection-work predicate (the same one `drain`/`wait_for_idle` use), so this
//! feature does not require its own migration. A stored flag is exactly what
//! could tear; later unrelated migrations do not change that invariant.

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    DenseReadiness, Engine, InitialState, PreparedWrite, ProjectionRole, ProjectionSpec,
    ProjectionVector, SourceId,
};
use fathomdb_schema::SQLITE_SUFFIX;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A deterministic in-process embedder with a tunable per-call delay, so the
/// window between "row written" and "vector committed" is real and observable.
#[derive(Clone, Debug)]
struct DelayEmbedder {
    identity: EmbedderIdentity,
    delay: Duration,
    fail: bool,
}

impl DelayEmbedder {
    fn new(delay: Duration) -> Self {
        Self { identity: EmbedderIdentity::new("deterministic", "rev-a", 384), delay, fail: false }
    }

    /// Always errors — drives rows to the `failed` projection terminal, which
    /// records NO vector row. Used by the non-vacuity guard below.
    fn failing() -> Self {
        Self { fail: true, ..Self::new(Duration::ZERO) }
    }
}

impl Embedder for DelayEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        if !self.delay.is_zero() {
            thread::sleep(self.delay);
        }
        if self.fail {
            return Err(EmbedderError::Failed { message: "deterministic failure".to_string() });
        }
        let mut v = vec![0.0_f32; self.identity.dimension as usize];
        v[0] = 1.0;
        Ok(v)
    }
}

fn db_path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn roles(rs: &[ProjectionRole]) -> BTreeSet<ProjectionRole> {
    rs.iter().copied().collect()
}

/// A caller-authored `searchable→vector` spec. `dense_readiness` is `None` —
/// a caller never authors it (it is engine-set read metadata).
fn vector_spec(name: &str) -> ProjectionSpec {
    ProjectionSpec {
        name: name.to_string(),
        roles: roles(&[ProjectionRole::Searchable]),
        fts: None,
        vector: Some(ProjectionVector { embedder: None, dense_readiness: None }),
        source: None,
    }
}

fn node(logical_id: &str, body_json: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body_json.to_string(),
        source_id: SourceId::new("test:fixture").expect("source id"),
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

/// An ANONYMOUS (no `logical_id`) vector-kind write.
///
/// Why the P2 stream uses this rather than the governed `node` helper: when this
/// suite was written, a tight loop of GOVERNED writes racing the async projection
/// worker intermittently failed with `EngineError::Storage` — reproducible at the
/// Slice-20 BASELINE (`9db32765`) with no Slice-20 code in the build, and never
/// with anonymous writes. It was a PRE-EXISTING write-path defect, out of
/// R-20-DR's scope, so this suite routed around it.
///
/// **That race was FIXED in 0.8.20 Slice 21 (ledger `TC-57`)**: `commit_batch`
/// now opens its transaction with `BEGIN IMMEDIATE`, taking the WAL write lock
/// before the supersession SELECT, so the governed path never has to promote a
/// read lock to a write lock.
///
/// The mechanism recorded here previously was WRONG, and correcting it matters
/// because a misnamed mechanism is how the next reader scopes an inert fix. It
/// was **plain `SQLITE_BUSY` (5) with the busy handler invoked ZERO times** —
/// SQLite skips the handler on a lock promotion for deadlock avoidance, so the
/// error came back in 0 ms against rusqlite's 5 000 ms default and no
/// `busy_timeout` value could ever have absorbed it. It was **not**
/// `SQLITE_BUSY_SNAPSHOT` (517), which is a second, narrower exit of the same
/// shape. The repro rate at baseline `41a81c17` was **10/10**, not the "7 of 8"
/// this comment used to claim. Characterized in
/// `dev/design/0.8.20-tc57-write-race-characterization.md` and pinned by
/// `tests/tc57_governed_write_race.rs`.
///
/// The helper STAYS. Readiness is indifferent to `logical_id` — the vector
/// projection consumes the same rows either way — so these tests assert exactly
/// the same atomicity property with anonymous writes, and rewriting them to
/// governed writes would buy no coverage while re-coupling this suite to the
/// write path's concurrency behaviour. `TC-57`'s own suite owns that property now.
fn anon_node(body_json: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body_json.to_string(),
        source_id: SourceId::new("test:fixture").expect("source id"),
        logical_id: None,
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

/// The readiness the engine reports for projection `name`, via the governed
/// `read.projections` path.
fn readiness(engine: &Engine, name: &str) -> Option<DenseReadiness> {
    engine
        .read_projections()
        .expect("read_projections")
        .into_iter()
        .find(|s| s.name == name)
        .and_then(|s| s.vector)
        .and_then(|v| v.dense_readiness)
}

/// A raw READ-ONLY connection to the live database file. The engine's exclusive
/// hold is a lock FILE, not a SQLite lock, so this observes committed WAL state
/// while the engine is open. `mode=ro` (never `immutable=1`) so WAL frames are
/// visible.
fn ro(path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only")
}

/// Raw: does `_fathomdb_vector_rows` carry a row for this write cursor? This is
/// the vector-at-rest oracle for the flip.
fn vector_row_exists(conn: &rusqlite::Connection, cursor: i64) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM _fathomdb_vector_rows WHERE write_cursor = ?1",
        [cursor],
        |r| r.get::<_, i64>(0),
    )
    .expect("vector row probe")
        > 0
}

/// Raw: the `vector_default` (vec0) rowid count for this cursor — the SECOND
/// at-rest oracle, so the assertion cannot pass off the bookkeeping table alone.
fn vec0_row_exists(conn: &rusqlite::Connection, cursor: i64) -> bool {
    conn.query_row("SELECT COUNT(*) FROM vector_default WHERE rowid = ?1", [cursor], |r| {
        r.get::<_, i64>(0)
    })
    .unwrap_or(0)
        > 0
}

fn active_cursor(conn: &rusqlite::Connection, logical_id: &str) -> i64 {
    conn.query_row(
        "SELECT write_cursor FROM canonical_nodes
         WHERE logical_id = ?1 AND superseded_at IS NULL",
        [logical_id],
        |r| r.get::<_, i64>(0),
    )
    .expect("active cursor")
}

/// §4.1 invariant 1, stated as SQL: the number of vector-kind rows that reached
/// the `up_to_date` projection terminal but carry NO vector row. Must ALWAYS be
/// zero — the terminal and the vector INSERT are written in one transaction, so
/// a non-zero count is a torn `ready`-without-vector.
fn terminals_missing_vectors(conn: &rusqlite::Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*)
         FROM _fathomdb_projection_terminal t
         JOIN canonical_nodes n ON n.write_cursor = t.write_cursor
         JOIN _fathomdb_vector_kinds k ON k.kind = n.kind
         LEFT JOIN _fathomdb_vector_rows v ON v.write_cursor = t.write_cursor
         WHERE t.state = 'up_to_date' AND v.write_cursor IS NULL",
        [],
        |r| r.get::<_, i64>(0),
    )
    .expect("torn-terminal probe")
}

/// The same LEFT JOIN as [`terminals_missing_vectors`] with the
/// `state = 'up_to_date'` filter DROPPED — i.e. rows at ANY projection terminal
/// that carry no vector. The non-vacuity control: it must be able to return a
/// non-zero count (the `failed` terminals do exactly that), proving the join
/// shape genuinely finds terminal-without-vector rows and that the zero the
/// atomicity assertion sees is a real zero, not a query that can never match.
fn any_terminal_missing_vectors(conn: &rusqlite::Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*)
         FROM _fathomdb_projection_terminal t
         JOIN canonical_nodes n ON n.write_cursor = t.write_cursor
         JOIN _fathomdb_vector_kinds k ON k.kind = n.kind
         LEFT JOIN _fathomdb_vector_rows v ON v.write_cursor = t.write_cursor
         WHERE v.write_cursor IS NULL",
        [],
        |r| r.get::<_, i64>(0),
    )
    .expect("any-terminal probe")
}

/// The number of vector-kind rows at or below `max_cursor` with NO vector row.
/// Must be zero whenever readiness reads `ready`.
fn unembedded_at_or_below(conn: &rusqlite::Connection, max_cursor: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*)
         FROM canonical_nodes n
         JOIN _fathomdb_vector_kinds k ON k.kind = n.kind
         LEFT JOIN _fathomdb_vector_rows v ON v.write_cursor = n.write_cursor
         WHERE n.write_cursor <= ?1 AND v.write_cursor IS NULL",
        [max_cursor],
        |r| r.get::<_, i64>(0),
    )
    .expect("unembedded probe")
}

fn max_node_cursor(conn: &rusqlite::Connection) -> i64 {
    conn.query_row("SELECT COALESCE(MAX(write_cursor), 0) FROM canonical_nodes", [], |r| {
        r.get::<_, i64>(0)
    })
    .expect("max cursor")
}

// ===========================================================================
// P1 — "readiness never reports ready with pending embeds"
// ===========================================================================

/// Freeze the scheduler, write a vector-kind row, and readiness must read
/// `embedding` WHILE the raw vector tables are empty for that cursor. Unfreeze
/// + drain and it flips to `ready` WITH both raw vector rows present.
#[test]
fn readiness_reads_embedding_while_embeds_are_outstanding_then_flips_to_ready() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "readiness_p1");
    let opened =
        Engine::open_with_embedder_for_test(&path, Arc::new(DelayEmbedder::new(Duration::ZERO)))
            .expect("open");
    let engine = &opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");

    // Declare the projection BEFORE any pending work exists: `configure_projections`
    // drains first, so declaring it under a frozen scheduler with outstanding
    // work would (correctly) time out.
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");

    // Baseline: an empty corpus has nothing outstanding, so it is READY.
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Ready),
        "an empty corpus has no outstanding embeds"
    );

    // Freeze the async projection scheduler, then write a vector-kind row. The
    // row is committed and FTS-retrievable; its embedding is outstanding.
    engine.set_projection_scheduler_frozen_for_test(true);
    engine.write(&[node("N1", r#"{"summary":"a dense meaning"}"#)]).expect("write");

    let conn = ro(&path);
    let cursor = active_cursor(&conn, "N1");

    // RAW oracle first — the vector genuinely is not there yet.
    assert!(
        !vector_row_exists(&conn, cursor),
        "precondition: the vector row must be absent while the scheduler is frozen"
    );
    assert!(!vec0_row_exists(&conn, cursor), "precondition: the vec0 row must be absent too");

    // The acceptance signal: readiness must NOT say ready.
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Embedding),
        "readiness must report `embedding` while an embed is outstanding, never `ready`"
    );

    // Let the worker finish, then the flip.
    engine.set_projection_scheduler_frozen_for_test(false);
    engine.drain(30_000).expect("drain");

    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Ready),
        "readiness must flip to `ready` once the embed lands"
    );
    let conn = ro(&path);
    assert!(vector_row_exists(&conn, cursor), "the vector row is at rest once readiness is ready");
    assert!(vec0_row_exists(&conn, cursor), "the vec0 row is at rest once readiness is ready");
    assert_eq!(terminals_missing_vectors(&conn), 0, "§4.1: no terminal without its vector");

    opened.engine.close().unwrap();
}

// ===========================================================================
// P2 — "flip is atomic under concurrent write"
// ===========================================================================

/// The forbidden torn state is UNOBSERVABLE. A background writer streams
/// vector-kind rows while the observer repeatedly samples (a) the raw
/// high-water cursor, (b) readiness, and (c) the raw vector tables. Two
/// invariants are checked on every sample:
///
///   1. `ready` ⟹ every vector-kind row at or below the cursor sampled BEFORE
///      the readiness read has its vector row. (Sampling the cursor first is
///      what makes this sound under concurrency: rows can only be ADDED after
///      the sample, and vector rows are never removed, so a later re-read can
///      only be more complete.)
///   2. Unconditionally: no row carries an `up_to_date` projection terminal
///      without its vector row — the direct §4.1 invariant-1 statement, valid at
///      every instant regardless of interleaving.
///
/// It also asserts the TOLERATED torn state is genuinely reachable (`embedding`
/// with vectors outstanding was observed at least once), so a run in which the
/// worker always won the race cannot pass vacuously.
#[test]
fn atomic_flip_never_exposes_ready_without_the_vector_under_concurrent_write() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "readiness_p2");
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(DelayEmbedder::new(Duration::from_millis(1))),
    )
    .expect("open");
    let engine = &opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");

    const WRITES: usize = 300;
    let writer_done = AtomicBool::new(false);
    let saw_embedding = AtomicBool::new(false);
    let saw_ready = AtomicBool::new(false);

    thread::scope(|scope| {
        scope.spawn(|| {
            for i in 0..WRITES {
                engine
                    .write(&[anon_node(&format!(r#"{{"summary":"meaning {i}"}}"#))])
                    .expect("concurrent write");
                // Yield so the projection worker actually interleaves: without
                // this the writer monopolises the writer lock and the observer
                // mostly samples a settled corpus, weakening the race.
                thread::sleep(Duration::from_millis(1));
            }
            writer_done.store(true, Ordering::SeqCst);
        });

        let conn = ro(&path);
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut samples = 0_u32;
        loop {
            // (a) high-water cursor FIRST, (b) readiness SECOND — see the doc
            // comment: this ordering is what makes invariant 1 sound.
            let max_cursor = max_node_cursor(&conn);
            let observed = readiness(engine, "summary");

            match observed {
                Some(DenseReadiness::Ready) => {
                    saw_ready.store(true, Ordering::SeqCst);
                    assert_eq!(
                        unembedded_at_or_below(&conn, max_cursor),
                        0,
                        "FORBIDDEN TORN STATE: readiness read `ready` while a vector row \
                         at or below cursor {max_cursor} was absent"
                    );
                }
                Some(DenseReadiness::Embedding) => {
                    saw_embedding.store(true, Ordering::SeqCst);
                }
                Some(DenseReadiness::Unavailable) => {
                    panic!("the fixture has a usable dense runtime, so it cannot be unavailable")
                }
                None => panic!("a declared vector projection must always carry a readiness"),
            }

            // Holds at EVERY instant, in either readiness state.
            assert_eq!(
                terminals_missing_vectors(&conn),
                0,
                "FORBIDDEN TORN STATE: an `up_to_date` terminal exists without its vector row"
            );

            samples += 1;
            // Keep sampling until the writer has finished, a healthy number of
            // samples is in, AND the corpus has settled to `ready` at least once
            // — otherwise the branch that carries the forbidden-torn-state
            // assertion could never execute and the test would be vacuous.
            if writer_done.load(Ordering::SeqCst)
                && samples > 200
                && saw_ready.load(Ordering::SeqCst)
            {
                break;
            }
            assert!(Instant::now() < deadline, "observer timed out");
            thread::sleep(Duration::from_millis(1));
        }
    });

    assert!(
        saw_embedding.load(Ordering::SeqCst),
        "non-vacuity: the tolerated torn state (`embedding`) must have been observed at least \
         once, else the race window never opened"
    );
    assert!(
        saw_ready.load(Ordering::SeqCst),
        "non-vacuity: `ready` must have been observed at least once, else the branch carrying \
         the forbidden-torn-state assertion never executed"
    );

    engine.drain(60_000).expect("drain");
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Ready),
        "the corpus settles to ready once every embed lands"
    );
    let conn = ro(&path);
    assert_eq!(unembedded_at_or_below(&conn, max_node_cursor(&conn)), 0);
    assert_eq!(terminals_missing_vectors(&conn), 0);

    opened.engine.close().unwrap();
}

/// **Non-vacuity guard for P2's torn-state detector, and the documented failure
/// boundary.** A terminally-FAILED embed records a `failed` projection terminal
/// with NO vector row. That gives a real terminal-without-vector row, so:
///
///   1. the un-filtered join ([`any_terminal_missing_vectors`]) returns > 0 —
///      proving the LEFT JOIN shape P2 relies on genuinely FINDS such rows, and
///      that P2's zeroes are real zeroes rather than a query that can never
///      match (the Slice-25 vacuous-green lesson);
///   2. the `state = 'up_to_date'` filtered form still returns 0 — a failed row
///      is NOT a torn write. §4.1 forbids a torn `ready`-WITHOUT-vector, i.e. an
///      `up_to_date` terminal with no vector; a `failed` terminal is a terminal
///      decision that there will never be a vector.
///
/// With this test's usable dense runtime, it also pins the failure boundary:
/// once failure is terminal, nothing is outstanding, so readiness returns to
/// `ready` even though that row has no vector. Reporting `embedding` forever
/// would be a lie (the row will never embed); the failure stays observable
/// through the `projection_failures` collection.
#[test]
fn a_failed_embed_is_not_a_torn_write_and_the_detector_is_not_vacuous() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "readiness_failed");
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(DelayEmbedder::failing()))
        .expect("open");
    let engine = &opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    engine.set_projection_retry_delays_for_test(&[0, 0, 0]);
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");

    engine.write(&[anon_node(r#"{"summary":"will fail projection"}"#)]).expect("write");
    engine.drain(30_000).expect("drain");

    let conn = ro(&path);
    assert!(
        any_terminal_missing_vectors(&conn) > 0,
        "non-vacuity: the terminal-without-vector detector MUST be able to fire"
    );
    assert_eq!(
        terminals_missing_vectors(&conn),
        0,
        "a `failed` terminal is not a torn write — no `up_to_date` terminal lacks its vector"
    );
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Ready),
        "a TERMINALLY-failed embed is not outstanding work; readiness returns to `ready`"
    );

    opened.engine.close().unwrap();
}

// ===========================================================================
// fix-1 (codex §9 [P2]) — the probe's EDGE arm must mirror the SCHEDULER
// ===========================================================================

/// Raw: is `kind` registered as a vector kind? The fixture below depends on
/// `edge_fact` being ABSENT, so the precondition is asserted, not assumed.
fn vector_kind_registered(conn: &rusqlite::Connection, kind: &str) -> bool {
    conn.query_row("SELECT COUNT(*) FROM _fathomdb_vector_kinds WHERE kind = ?1", [kind], |r| {
        r.get::<_, i64>(0)
    })
    .expect("vector kind probe")
        > 0
}

/// **The stuck-`embedding`-forever defect.** `next_pending_projection_jobs` —
/// the SCHEDULER, and therefore the authority on what will actually be embedded
/// — joins `_fathomdb_vector_kinds` on `'edge_fact'` in its edge arm. So a live
/// edge body whose `edge_fact` kind is unregistered is NEVER scheduled, never
/// embedded, and never gains a `_fathomdb_projection_terminal` row.
///
/// If the pending-work probe's edge arm omits that same join it counts such a
/// row as outstanding FOREVER, so `dense_readiness` reports `embedding` for a
/// corpus in which nothing will ever happen — the exact mirror image of
/// R-20-DR's property, and a false report on brand-new governed metadata.
///
/// The same predicate backs `wait_for_idle`, hence the shipped public
/// `Engine::drain`, so the drain half is asserted here too: idle must be
/// reported rather than timing out into [`EngineError::Scheduler`].
///
/// **Fixture reachability.** The state is a live edge with a body written while
/// `edge_fact` was not a vector kind — e.g. a database whose edges predate the
/// G11 edge-vector pipeline (which is what auto-registers `edge_fact`), carried
/// forward by migration. `_fathomdb_vector_kinds` has no delete path, so the
/// fixture is built the way the sibling TC-33 edge tests build theirs: a second
/// connection to the live file inserts the canonical row directly.
#[test]
fn readiness_is_ready_when_a_live_edge_body_can_never_be_scheduled() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "readiness_unschedulable_edge");
    let opened =
        Engine::open_with_embedder_for_test(&path, Arc::new(DelayEmbedder::new(Duration::ZERO)))
            .expect("open");
    let engine = &opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");

    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Ready),
        "precondition: an empty corpus has no outstanding embeds"
    );

    // A LIVE canonical edge WITH a body, under a kind the vector pipeline does
    // not serve. Not superseded, still valid (`t_invalid` NULL), no terminal.
    let raw = rusqlite::Connection::open(&path).expect("raw writer");
    raw.execute(
        "INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id, body)
         VALUES(9000001, 'mentions', 'a', 'b', 'an edge body nothing will embed')",
        [],
    )
    .expect("insert a live edge body");

    // Fixture preconditions — asserted, so the test cannot pass by building the
    // wrong database.
    let conn = ro(&path);
    assert!(
        !vector_kind_registered(&conn, "edge_fact"),
        "fixture: `edge_fact` must NOT be a registered vector kind"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM canonical_edges ce
             LEFT JOIN _fathomdb_projection_terminal pt ON pt.write_cursor = ce.write_cursor
             WHERE ce.body IS NOT NULL
               AND ce.superseded_at IS NULL
               AND ce.t_invalid IS NULL
               AND pt.write_cursor IS NULL",
            [],
            |r| r.get::<_, i64>(0)
        )
        .expect("fixture probe"),
        1,
        "fixture: exactly one live, un-terminated edge body must exist"
    );

    // The scheduler's edge arm JOINs `_fathomdb_vector_kinds` on 'edge_fact',
    // so this row is not, and can never become, outstanding work.
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Ready),
        "readiness must NOT report `embedding` for an edge body the scheduler will never \
         schedule — that is a permanent false `embedding`"
    );

    // Same predicate, shipped surface: `drain` must report idle, not time out.
    engine.drain(5_000).expect("drain must report idle when no schedulable embed is outstanding");

    opened.engine.close().unwrap();
}

/// **Precision guard on the fix above** — do not overcorrect into reporting
/// `ready` while a genuinely schedulable edge embed is outstanding. Same fixture
/// shape, one difference: `edge_fact` IS registered, so the scheduler WILL pick
/// the row up. Readiness must read `embedding` until it lands.
#[test]
fn readiness_still_reports_embedding_for_a_schedulable_edge_body() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "readiness_schedulable_edge");
    let opened =
        Engine::open_with_embedder_for_test(&path, Arc::new(DelayEmbedder::new(Duration::ZERO)))
            .expect("open");
    let engine = &opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");
    // Register `edge_fact` exactly as the G11 edge write path does.
    engine.configure_vector_kind_for_test("edge_fact").expect("edge vector kind");

    engine.set_projection_scheduler_frozen_for_test(true);
    let raw = rusqlite::Connection::open(&path).expect("raw writer");
    raw.execute(
        "INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id, body)
         VALUES(9000001, 'mentions', 'a', 'b', 'an edge body that WILL embed')",
        [],
    )
    .expect("insert a live edge body");

    let conn = ro(&path);
    assert!(
        vector_kind_registered(&conn, "edge_fact"),
        "fixture: `edge_fact` must be a registered vector kind here"
    );
    assert!(
        !vector_row_exists(&conn, 9_000_001),
        "precondition: the edge vector must be absent while the scheduler is frozen"
    );

    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Embedding),
        "readiness must still report `embedding` while a SCHEDULABLE edge embed is outstanding"
    );

    opened.engine.close().unwrap();
}

// ===========================================================================
// `dense_readiness` is ENGINE-SET read metadata, not caller config
// ===========================================================================

/// A caller-supplied `dense_readiness` is INERT: the engine stores nothing and
/// honours nothing, and always reports the DERIVED truth. Asserted the
/// falsifiable way round — declare `Ready` while an embed is genuinely
/// outstanding and the engine must still say `Embedding`.
#[test]
fn caller_supplied_dense_readiness_is_inert_engine_reports_derived_truth() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "readiness_inert");
    let opened =
        Engine::open_with_embedder_for_test(&path, Arc::new(DelayEmbedder::new(Duration::ZERO)))
            .expect("open");
    let engine = &opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");

    // Declare with a LIE: `ready` while nothing is embedded yet.
    let mut lying = vector_spec("summary");
    lying.vector =
        Some(ProjectionVector { embedder: None, dense_readiness: Some(DenseReadiness::Ready) });
    engine.configure_projections(&[lying.clone()], &[]).expect("configure accepts it inertly");

    engine.set_projection_scheduler_frozen_for_test(true);
    engine.write(&[node("N1", r#"{"summary":"a dense meaning"}"#)]).expect("write");

    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Embedding),
        "the caller's `ready` must NOT be honoured — the engine reports the derived truth"
    );

    // And the inverse lie is equally inert.
    engine.set_projection_scheduler_frozen_for_test(false);
    engine.drain(30_000).expect("drain");
    let mut lying_other_way = vector_spec("summary");
    lying_other_way.vector =
        Some(ProjectionVector { embedder: None, dense_readiness: Some(DenseReadiness::Embedding) });
    engine.configure_projections(&[lying_other_way], &[]).expect("configure");
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Ready),
        "the caller's `embedding` must NOT be honoured either"
    );

    opened.engine.close().unwrap();
}

/// Readiness is NOT part of the declaration, so supplying it cannot make an
/// otherwise-identical re-apply look like a change (it must still diff to a
/// no-op) and cannot be mistaken for a destructive delta.
#[test]
fn readiness_is_not_part_of_the_declaration_reapply_is_a_no_op() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "readiness_noop");
    let opened =
        Engine::open_with_embedder_for_test(&path, Arc::new(DelayEmbedder::new(Duration::ZERO)))
            .expect("open");
    let engine = &opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");

    // Feed `read.projections` output STRAIGHT back in — the fix-4 read→configure
    // round-trip, which now carries the engine-set readiness field.
    let read_back = engine.read_projections().expect("read_projections");
    assert_eq!(read_back.len(), 1);
    assert_eq!(
        read_back[0].vector.as_ref().and_then(|v| v.dense_readiness),
        Some(DenseReadiness::Ready),
        "read output carries the engine-set readiness"
    );
    let again = engine.configure_projections(&read_back, &[]).expect("re-apply read output");
    assert!(again.unchanged, "read.projections output must re-apply as an idempotent no-op");

    opened.engine.close().unwrap();
}

// ===========================================================================
// No default-behaviour drift (plan §11 item 2 — `dense_readiness` is OPT-IN)
// ===========================================================================

/// A caller who never declares a vector projection sees IDENTICAL behaviour:
/// no readiness field is produced anywhere, the same rows are embedded, and no
/// No Slice 20 schema step was needed (readiness is derived).
#[test]
fn default_path_is_unchanged_when_no_vector_projection_is_declared() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "readiness_default");
    let opened =
        Engine::open_with_embedder_for_test(&path, Arc::new(DelayEmbedder::new(Duration::ZERO)))
            .expect("open");
    let engine = &opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");

    // No `configure_projections` at all — the pre-slice default path.
    engine.write(&[node("N1", r#"{"summary":"a dense meaning"}"#)]).expect("write");
    engine.drain(30_000).expect("drain");

    assert!(engine.read_projections().expect("read_projections").is_empty());

    let conn = ro(&path);
    let cursor = active_cursor(&conn, "N1");
    assert!(vector_row_exists(&conn, cursor), "the default embed path still embeds");
    assert_eq!(terminals_missing_vectors(&conn), 0);

    // A projection declared WITHOUT the vector sub-object carries no readiness
    // at all — the field is scoped to `searchable→vector` and nothing else.
    engine
        .configure_projections(
            &[ProjectionSpec {
                name: "status".to_string(),
                roles: roles(&[ProjectionRole::Filterable]),
                fts: None,
                vector: None,
                source: None,
            }],
            &[],
        )
        .expect("configure filterable");
    let status = engine
        .read_projections()
        .expect("read_projections")
        .into_iter()
        .find(|s| s.name == "status")
        .expect("status projection");
    assert!(status.vector.is_none(), "a non-vector projection carries no readiness");

    opened.engine.close().unwrap();
}
