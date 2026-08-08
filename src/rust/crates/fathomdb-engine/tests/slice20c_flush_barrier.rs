//! 0.8.20 Slice 20c (R-20-DR remainder) — **`drain` is the flush-to-readiness
//! barrier** (`api-surface.md` **C4**), and its *rider*: `drain` is a **barrier**
//! (wait-for-idle), **not** a trigger, so deferred/backfill rows MUST be
//! **enqueued on the same projection runtime `drain` waits on**.
//!
//! Steward ruling `seq-110` (HITL 2026-07-26): TC-55 = INSTRUMENTATION. The
//! barrier ships by REUSING the shipped `drain`, not by minting a governed
//! `flush_embeddings` command — so this suite adds ZERO net-new governed surface.
//!
//! ## The defect these tests close
//!
//! `configure_projections` persists a `searchable→vector` declaration, pushes the
//! name onto `ProjectionDelta::deferred` … and then drops that deferred work on
//! the floor: it never registers the kind in `_fathomdb_vector_kinds`, never
//! rewinds the readiness watermark, and never notifies the projection runtime.
//! Meanwhile `project_canonical_node_row` has already written a PERMANENT
//! `'up_to_date'` terminal for every row whose kind was not vector-registered at
//! write time.
//!
//! Both arms of `connection_has_pending_projection_work` — the ONE predicate
//! shared by `drain` (via `wait_for_idle`) and by `derive_dense_readiness` — key
//! off that terminal and off the `_fathomdb_vector_kinds` join. So on the broken
//! code the sequence *write rows → declare `searchable→vector`* yields
//! `drain() == Ok(())` **immediately** and `dense_readiness == "ready"` while NO
//! vector row exists and nothing will ever create one. That is a **false-ready
//! barrier** and a direct violation of R-20-DR's acceptance signal ("readiness
//! never reports ready with pending embeds").
//!
//! ## Why every assertion reads RAW TABLES
//!
//! A test that only reads readiness back PASSES on the broken code (it reports
//! `ready`, which is what a naive test would assert after a drain). The falsifying
//! oracle is `_fathomdb_vector_rows` + `vector_default` — the vectors at rest.
//!
//! **No Slice 20c schema step.** Re-enqueueing embed work inside ONE live
//! database is runtime reconfiguration, not a cross-version data migration
//! (HITL 2026-07-21, cf. TC-46). Later unrelated migrations do not affect that
//! boundary.
//!
//! ## Known: this suite sometimes reports ~30 s wall time (not a flake)
//!
//! Every assertion still passes; ONE `drain(30_000)` occasionally returns `Ok`
//! only at its deadline. The cause is a PRE-EXISTING missed wakeup in
//! `ProjectionRuntime::wait_for_idle`: it releases the state lock to run
//! `database_has_pending_projection_work` (which opens a fresh connection), and a
//! worker that finishes the LAST job inside that window does its
//! `state_cvar.notify_all()` before the waiter re-acquires. With no further
//! notification the waiter sleeps the whole remaining timeout, then re-probes at
//! the loop top, sees idle, and returns `Ok`. Adding Leg C widened the exposure
//! (two more concurrent engines) but did not create it — `wait_for_idle` is
//! untouched by this slice. Measured: capping the cvar wait at 25 ms removes the
//! stall in 6/6 runs versus ~40 % stalls without. Left unfixed on purpose (fix-1
//! scope is the drop inverse); reported for triage, where the same window is also
//! a SPURIOUS-TIMEOUT risk for callers who pass a short timeout.

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    DenseReadiness, Engine, InitialState, PreparedWrite, ProjectionRole, ProjectionSpec,
    ProjectionVector, SourceId,
};
use fathomdb_schema::SQLITE_SUFFIX;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A deterministic embedder that COUNTS its calls, so "no re-embed on an
/// idempotent re-apply" is a falsifiable assertion rather than an inference.
#[derive(Clone, Debug)]
struct CountingEmbedder {
    identity: EmbedderIdentity,
    calls: Arc<AtomicUsize>,
    /// fix-1 — a settable per-call delay. Default `0`, so every pre-existing
    /// test is unaffected. Leg C raises it AFTER its fixture has settled, which
    /// is what makes "`drain` does not wait on a dropped projection's work"
    /// falsifiable in BOTH directions: with work enqueued the drain must burn
    /// its (short) timeout, with none it returns on the first poll.
    delay_ms: Arc<AtomicU64>,
}

impl CountingEmbedder {
    fn new() -> Self {
        Self::with_identity(EmbedderIdentity::new("deterministic", "rev-a", 384))
    }

    fn with_identity(identity: EmbedderIdentity) -> Self {
        Self {
            identity,
            calls: Arc::new(AtomicUsize::new(0)),
            delay_ms: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl Embedder for CountingEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let delay = self.delay_ms.load(Ordering::SeqCst);
        if delay > 0 {
            std::thread::sleep(Duration::from_millis(delay));
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

/// A caller-authored `searchable→vector` spec (`dense_readiness` is engine-set
/// read metadata, never authored).
fn vector_spec(name: &str) -> ProjectionSpec {
    ProjectionSpec {
        name: name.to_string(),
        roles: roles(&[ProjectionRole::Searchable]),
        fts: None,
        vector: Some(ProjectionVector { embedder: None, dense_readiness: None }),
        source: None,
    }
}

/// An edge carrying a BODY — the only edge shape that enrols `'edge_fact'` in
/// `_fathomdb_vector_kinds` (`project_canonical_edge_row`, G11).
fn edge(logical_id: &str, from: &str, to: &str, body: &str) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: SourceId::new("test:fixture").expect("source id"),
        logical_id: Some(logical_id.to_string()),
        body: Some(body.to_string()),
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

fn node(kind: &str, logical_id: &str, body_json: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: kind.to_string(),
        body: body_json.to_string(),
        source_id: SourceId::new("test:fixture").expect("source id"),
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

fn readiness(engine: &Engine, name: &str) -> Option<DenseReadiness> {
    engine
        .read_projections()
        .expect("read_projections")
        .into_iter()
        .find(|s| s.name == name)
        .and_then(|s| s.vector)
        .and_then(|v| v.dense_readiness)
}

/// A raw READ-ONLY connection to the live file. The engine's exclusive hold is a
/// lock FILE, not a SQLite lock, so committed WAL state is visible. `mode=ro`,
/// never `immutable=1`.
fn ro(path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only")
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

/// AT-REST oracle #1 — the `_fathomdb_vector_rows` bookkeeping row.
fn vector_row_exists(conn: &rusqlite::Connection, cursor: i64) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM _fathomdb_vector_rows WHERE write_cursor = ?1",
        [cursor],
        |r| r.get::<_, i64>(0),
    )
    .expect("vector row probe")
        > 0
}

/// AT-REST oracle #2 — the vec0 row itself, so the assertion cannot pass off the
/// bookkeeping table alone.
fn vec0_row_exists(conn: &rusqlite::Connection, cursor: i64) -> bool {
    conn.query_row("SELECT COUNT(*) FROM vector_default WHERE rowid = ?1", [cursor], |r| {
        r.get::<_, i64>(0)
    })
    .unwrap_or(0)
        > 0
}

fn vector_kind_registered(conn: &rusqlite::Connection, kind: &str) -> bool {
    conn.query_row("SELECT COUNT(*) FROM _fathomdb_vector_kinds WHERE kind = ?1", [kind], |r| {
        r.get::<_, i64>(0)
    })
    .expect("vector kind probe")
        > 0
}

/// The readiness watermark (`_fathomdb_open_state`'s projection cursor). Used to
/// prove an idempotent re-apply does NOT rewind it.
fn projection_cursor(conn: &rusqlite::Connection) -> i64 {
    conn.query_row(
        "SELECT CAST(value AS INTEGER) FROM _fathomdb_open_state WHERE key = 'projection_cursor'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
}

/// Vector-eligible node rows carrying NO vector row. The direct statement of the
/// defect: while a `searchable→vector` projection is declared, after `drain()`
/// returns `Ok` this MUST be zero.
///
/// **Deliberately does NOT join `_fathomdb_vector_kinds`.** The sibling
/// Slice-20 probes do, and that join is exactly what makes them VACUOUS against
/// this defect: the bug IS that the declaration never registers the kind, so a
/// `_fathomdb_vector_kinds`-joined probe returns a hollow zero on the broken
/// code. `row_kind IN ('leaf','coverage')` is the engine's own vector-eligibility
/// predicate (`index_targets_for_row_kind`); `graph` rows are lexically
/// searchable but never embedded, so they are correctly excluded.
fn leaf_rows_without_vectors(conn: &rusqlite::Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*)
         FROM canonical_nodes n
         LEFT JOIN _fathomdb_vector_rows v ON v.write_cursor = n.write_cursor
         WHERE n.row_kind IN ('leaf', 'coverage') AND v.write_cursor IS NULL",
        [],
        |r| r.get::<_, i64>(0),
    )
    .expect("unembedded probe")
}

/// The same un-joined at-rest probe as [`leaf_rows_without_vectors`], narrowed to
/// ONE `canonical_nodes.kind`.
///
/// fix-2 needs the narrowed form because its fixtures deliberately hold a kind
/// that gets NO dense arm at all (a kind outside `resolve_source_type`'s locked
/// set), so the corpus-wide count is legitimately non-zero there. It still does
/// **not** join `_fathomdb_vector_kinds` — the registry is the thing under test.
fn leaf_rows_of_kind_without_vectors(conn: &rusqlite::Connection, kind: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*)
         FROM canonical_nodes n
         LEFT JOIN _fathomdb_vector_rows v ON v.write_cursor = n.write_cursor
         WHERE n.row_kind IN ('leaf', 'coverage')
           AND n.kind = ?1
           AND v.write_cursor IS NULL",
        [kind],
        |r| r.get::<_, i64>(0),
    )
    .expect("per-kind unembedded probe")
}

// ===========================================================================
// Leg A — the C4 rider: declaring a vector projection ENQUEUES the deferred
//         work onto the runtime `drain` waits on
// ===========================================================================

/// **The false-ready barrier.** Write rows FIRST, declare `searchable→vector`
/// SECOND — the ordinary "turn the dense arm on for an existing corpus" flow.
///
/// Post-conditions (all three fail at baseline):
///   1. immediately after the declaration, readiness reads `embedding` — the
///      backfill is outstanding, so the barrier must not report ready;
///   2. `drain(timeout)` FLUSHES that work and returns `Ok(())`;
///   3. after the drain, readiness reads `ready` **and the vector rows exist at
///      rest** (raw `_fathomdb_vector_rows` + `vector_default`).
///
/// At baseline (1) reads `ready`, and (3)'s at-rest assertions find nothing —
/// only an operator `rebuild` would ever have created those vectors.
#[test]
fn declaring_a_vector_projection_backfills_pre_existing_rows_and_drain_flushes_to_ready() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "flush_barrier_backfill");
    let embedder = CountingEmbedder::new();
    let calls = Arc::clone(&embedder.calls);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    // NOTE: `configure_vector_kind_for_test` is deliberately NOT called. That
    // test hook is the crutch every sibling suite leans on; using it here would
    // hide the exact defect under test (there is no production surface that
    // registers a vector kind — `slice-G0-design.md`:
    // `production_vector_kind_surface=[]`; `configure_projections`'
    // `searchable→vector` IS that surface).
    engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("write N1");
    engine.write(&[node("doc", "N2", r#"{"summary":"another meaning"}"#)]).expect("write N2");
    engine.drain(30_000).expect("baseline drain");

    let conn = ro(&path);
    let c1 = active_cursor(&conn, "N1");
    let c2 = active_cursor(&conn, "N2");

    // Fixture precondition, asserted rather than assumed: nothing is embedded and
    // the kind is not a vector kind. This is the state `configure_projections`
    // inherits.
    assert!(!vector_kind_registered(&conn, "doc"), "fixture: `doc` is not yet a vector kind");
    assert!(!vector_row_exists(&conn, c1), "fixture: N1 has no vector yet");
    assert!(!vector_row_exists(&conn, c2), "fixture: N2 has no vector yet");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "fixture: nothing has been embedded");

    // ---- the declaration: turn the dense arm on over an existing corpus ----
    let delta = engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");
    assert!(
        delta.deferred.contains(&"summary".to_string()),
        "the vector sub-target is reported as deferred work"
    );

    // (1) The barrier must NOT report ready: the deferred work is outstanding.
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Embedding),
        "FALSE-READY BARRIER: declaring `searchable→vector` over a corpus with un-embedded rows \
         must report `embedding` — the deferred backfill is outstanding work"
    );

    // (2) `drain` is the flush-to-readiness barrier (C4): it must settle that work.
    engine.drain(30_000).expect("drain must flush the declared backfill");

    // (3) …and the flip must be backed by vectors AT REST, not just a status flip.
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Ready),
        "after `drain()` returns Ok the dense arm is caught up"
    );
    let conn = ro(&path);
    assert!(vector_kind_registered(&conn, "doc"), "the declaration registered the vector kind");
    assert!(vector_row_exists(&conn, c1), "N1's vector must exist at rest once readiness is ready");
    assert!(vec0_row_exists(&conn, c1), "N1's vec0 row must exist at rest");
    assert!(vector_row_exists(&conn, c2), "N2's vector must exist at rest once readiness is ready");
    assert!(vec0_row_exists(&conn, c2), "N2's vec0 row must exist at rest");
    assert_eq!(
        leaf_rows_without_vectors(&conn),
        0,
        "no vector-eligible row may remain un-embedded once readiness reads ready"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2, "exactly the two backfilled rows were embedded");

    opened.engine.close().unwrap();
}

/// **Idempotence is load-bearing** (R-20-PR: "re-registration is a no-op").
/// Re-applying an already-satisfied `searchable→vector` declaration must not
/// rewind the readiness watermark, must not re-embed, and must not open a
/// spurious `embedding` window.
#[test]
fn reapplying_a_satisfied_vector_declaration_does_not_rewind_or_re_embed() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "flush_barrier_idempotent");
    let embedder = CountingEmbedder::new();
    let calls = Arc::clone(&embedder.calls);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("write N1");
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("first configure");
    engine.drain(30_000).expect("drain");
    assert_eq!(readiness(engine, "summary"), Some(DenseReadiness::Ready));

    let conn = ro(&path);
    let cursor_before = projection_cursor(&conn);
    let calls_before = calls.load(Ordering::SeqCst);
    assert_eq!(calls_before, 1, "exactly one backfill embed happened");
    assert!(cursor_before > 0, "the readiness watermark advanced past the backfilled row");

    // ---- the same declaration again ----
    let again = engine.configure_projections(&[vector_spec("summary")], &[]).expect("re-apply");
    assert!(again.unchanged, "an identical re-apply diffs to a no-op");

    // No spurious `embedding` window: the work was already satisfied, so there is
    // nothing to re-enqueue. Read readiness FIRST (before any drain) so a
    // transient re-enqueue would be caught.
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Ready),
        "an idempotent re-apply must not re-open the backfill"
    );

    let conn = ro(&path);
    assert_eq!(
        projection_cursor(&conn),
        cursor_before,
        "an idempotent re-apply must NOT rewind the readiness watermark"
    );

    engine.drain(30_000).expect("drain");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        calls_before,
        "an idempotent re-apply must NOT re-embed an already-embedded row"
    );

    opened.engine.close().unwrap();
}

/// Raw: how many `projection_failures` audit rows exist? The no-embedder path
/// must generate NONE — a declaration that queued doomed embeds would show up
/// here.
fn projection_failure_rows(conn: &rusqlite::Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM operational_mutations
         WHERE collection_name = 'projection_failures'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
}

/// **The live-embedder gate, and the boot graft that pays for it.**
///
/// With `EmbedderChoice::None` there is no dense arm at all, so declaring
/// `searchable→vector` must PERSIST and DEFER (Q6a graceful-absent, exactly like
/// `rankable`) rather than enqueue embeds that could only retry to a `failed`
/// terminal and pollute `projection_failures`. That deferral is only honest if
/// the work actually grafts on later — so the second half of this test reopens
/// the SAME database WITH an approved embedder, requires the boot graft to run,
/// then confirms a re-apply remains an idempotent no-op.
#[test]
fn a_declaration_without_a_live_embedder_defers_then_boot_grafts() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "flush_barrier_no_embedder");

    // ---- session 1: no embedder ----
    {
        let opened = Engine::open(path.clone()).expect("open without embedder");
        let engine = &opened.engine;
        engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("write");
        engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");

        assert_eq!(
            readiness(engine, "summary"),
            Some(DenseReadiness::Unavailable),
            "with no live embedder the dense arm is unavailable even when no work is outstanding"
        );
        engine.drain(5_000).expect("drain must not burn its timeout on a dead dense arm");

        let conn = ro(&path);
        assert!(
            !vector_kind_registered(&conn, "doc"),
            "a declaration with no live embedder must NOT enrol the kind"
        );
        assert_eq!(
            projection_failure_rows(&conn),
            0,
            "no doomed embeds may be queued, so no projection_failures audit rows"
        );
        opened.engine.close().unwrap();
    }

    // ---- session 2: SAME database, now WITH an embedder ----
    //
    // The embedder must carry the identity session 1 pinned into
    // `_fathomdb_embedder_profiles`, or the reopen fails closed on
    // `EmbedderIdentityMismatch` (ADR-0.6.0-vector-identity-embedder-owned).
    // Read it back from the file rather than hard-coding the pinned revision, so
    // this fixture cannot rot against a default-identity change.
    let stored_identity = {
        let conn = ro(&path);
        conn.query_row(
            "SELECT name, revision, dimension FROM _fathomdb_embedder_profiles
             WHERE profile = 'default'",
            [],
            |r| {
                Ok(EmbedderIdentity::new(
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, u32>(2)?,
                ))
            },
        )
        .expect("stored default embedder identity")
    };
    let embedder = CountingEmbedder::with_identity(stored_identity);
    let calls = Arc::clone(&embedder.calls);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("reopen");
    let engine = &opened.engine;

    // The boot path already atomically enrolled and notified the deferred work.
    // The identical spec remains a no-op after that repair.
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("re-apply");
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Ready),
        "the boot graft settles before an idempotent re-apply"
    );

    engine.drain(30_000).expect("drain confirms the boot-grafted backfill");
    assert_eq!(readiness(engine, "summary"), Some(DenseReadiness::Ready));
    let conn = ro(&path);
    let cursor = active_cursor(&conn, "N1");
    assert!(vector_row_exists(&conn, cursor), "the grafted backfill landed at rest");
    assert!(vec0_row_exists(&conn, cursor), "…including the vec0 row");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        91,
        "90 prospective-arm probe calls plus exactly one deferred row"
    );

    opened.engine.close().unwrap();
}

// ===========================================================================
// Leg B — the pinned barrier post-condition: `drain()` Ok ⟹ readiness `ready`
// ===========================================================================

/// **The `flush_embeddings()` semantics, pinned on the shipped `drain`.**
///
/// `drain(timeout)` returning `Ok(())` means the projection runtime is
/// quiescent; `dense_readiness` is derived from the SAME predicate
/// (`connection_has_pending_projection_work`). So the invariant
/// `drain() == Ok ⟹ dense_readiness == ready` must hold after EVERY mutation
/// shape — including the one that breaks it today (declare-after-write), which is
/// checked here with the at-rest oracle so it cannot pass vacuously.
#[test]
fn drain_ok_implies_dense_readiness_ready_across_mutation_shapes() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "flush_barrier_invariant");
    let embedder = CountingEmbedder::new();
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    let check = |label: &str| {
        engine.drain(30_000).unwrap_or_else(|e| panic!("{label}: drain must return Ok, got {e:?}"));
        assert_eq!(
            readiness(engine, "summary"),
            Some(DenseReadiness::Ready),
            "{label}: `drain()` returned Ok, so readiness MUST be `ready`"
        );
        let conn = ro(&path);
        assert_eq!(
            leaf_rows_without_vectors(&conn),
            0,
            "{label}: `drain()` returned Ok and readiness is `ready`, so no vector-eligible row \
             may lack its vector at rest"
        );
    };

    // Shape 1 — declare over an EMPTY corpus.
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");
    check("declare-on-empty");

    // Shape 2 — write AFTER the declaration.
    engine.write(&[node("doc", "N1", r#"{"summary":"written after declaring"}"#)]).expect("write");
    check("write-after-declare");

    // Shape 3 — the breaking one: rows written BEFORE the kind was ever a vector
    // kind, then re-declared. Uses a second engine so the corpus genuinely
    // pre-dates the declaration.
    opened.engine.close().unwrap();

    let path2 = db_path(&dir, "flush_barrier_invariant_2");
    let embedder2 = CountingEmbedder::new();
    let opened2 = Engine::open_with_embedder_for_test(&path2, Arc::new(embedder2)).expect("open 2");
    let engine2 = &opened2.engine;
    engine2.write(&[node("doc", "N1", r#"{"summary":"pre-existing"}"#)]).expect("write");
    engine2.drain(30_000).expect("pre-declaration drain");
    engine2.configure_projections(&[vector_spec("summary")], &[]).expect("configure 2");
    engine2.drain(30_000).expect("post-declaration drain must return Ok");
    assert_eq!(
        readiness(engine2, "summary"),
        Some(DenseReadiness::Ready),
        "declare-after-write: `drain()` returned Ok, so readiness MUST be `ready`"
    );
    let conn = ro(&path2);
    assert_eq!(
        leaf_rows_without_vectors(&conn),
        0,
        "declare-after-write: FALSE-READY — `drain()` returned Ok and readiness reads `ready`, \
         but a vector-eligible row has no vector at rest"
    );

    opened2.engine.close().unwrap();
}

// ===========================================================================
// Leg C (fix-1, codex §9 [P2] "Stop embedding after vector projection drops")
//         — the SYMMETRIC INVERSE of Leg A's enrolment
// ===========================================================================
//
// Leg A gave `_fathomdb_vector_kinds` its first governed-call-reachable
// enrolment path (before this slice the only writer for a NODE kind was the
// `#[doc(hidden)]` `configure_vector_kind_for_test` hook). Enrolment with no
// inverse is the defect: `drop`ping the last `searchable→vector` projection
// leaves the kind enrolled, so `project_canonical_node_row`'s
// `kind_is_vector_indexed` gate keeps enqueueing embeds and
// `connection_has_pending_projection_work` keeps making `drain` wait — for a
// projection the registry no longer declares.
//
// The inverse is deliberately NON-DESTRUCTIVE. The shipped `drop` arm
// (`clear_attribute_projection` + `remove_projection_row`) has NEVER touched
// vec0, `_fathomdb_vector_rows` or `_fathomdb_vector_kinds`, so "existing
// vectors survive a drop" is already the shipped contract. Un-enrolment removes
// ONE registry row and deletes no embedding, which PRESERVES that contract —
// asserted below rather than assumed.

/// How many `_fathomdb_vector_kinds` rows exist, total. Used to prove the
/// un-enrolment is scoped (it must not empty the table when `'edge_fact'` is in
/// it) without joining anything.
fn vector_kind_count(conn: &rusqlite::Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM _fathomdb_vector_kinds", [], |r| r.get::<_, i64>(0))
        .expect("vector kind count")
}

/// **Dropping the last `searchable→vector` projection stops the embedding.**
///
/// Post-conditions (each one fails at fix-1 baseline):
///   1. after the `drop` the kind is no longer enrolled in
///      `_fathomdb_vector_kinds`;
///   2. the vectors already at rest are UNTOUCHED (the non-destructive half —
///      this must keep passing, it is the shipped drop contract);
///   3. a subsequent write of the SAME kind enqueues NO embed: the embedder is
///      never called again and the row has no vector at rest;
///   4. `drain` does not wait on it — with the embedder slowed to 8s a 2s drain
///      still returns `Ok` promptly, because there is nothing outstanding;
///   5. a write of a BRAND-NEW kind after the drop does not re-enrol via the
///      late-enrolment path (`Engine::enrol_batch_vector_kinds`);
///   6. re-applying the same drop is an idempotent no-op;
///   7. RE-declaring re-enrols and backfills, so un-enrolment is reversible and
///      strands nothing.
#[test]
fn dropping_the_last_vector_projection_un_enrols_the_kind_and_stops_embedding() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "flush_barrier_drop_inverse");
    let embedder = CountingEmbedder::new();
    let calls = Arc::clone(&embedder.calls);
    let delay_ms = Arc::clone(&embedder.delay_ms);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    // ---- fixture: the dense arm is live and caught up ----
    engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("write N1");
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");
    engine.drain(30_000).expect("drain");
    assert_eq!(readiness(engine, "summary"), Some(DenseReadiness::Ready));

    let conn = ro(&path);
    let c1 = active_cursor(&conn, "N1");
    assert!(vector_kind_registered(&conn, "doc"), "fixture: the declaration enrolled `doc`");
    assert!(vector_row_exists(&conn, c1), "fixture: N1 is embedded");
    assert!(vec0_row_exists(&conn, c1), "fixture: N1's vec0 row exists");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "fixture: exactly one embed so far");

    // ---- the drop: remove the LAST `searchable→vector` declaration ----
    let delta = engine
        .configure_projections(&[], &["summary".to_string()])
        .expect("drop the vector projection");
    assert!(delta.dropped.contains(&"summary".to_string()), "the drop is reported");
    assert!(
        engine.read_projections().expect("read_projections").is_empty(),
        "the registry no longer declares any projection"
    );

    let conn = ro(&path);
    // (1) the inverse actually ran.
    assert!(
        !vector_kind_registered(&conn, "doc"),
        "ONE-WAY ENROLMENT: dropping the last `searchable→vector` declaration must un-enrol the \
         node kind it enrolled, or the write path keeps embedding for a projection the registry \
         no longer declares"
    );
    // (2) …and deleted NOTHING. This is the shipped drop contract.
    assert!(
        vector_row_exists(&conn, c1),
        "un-enrolment must NOT delete embeddings — the shipped `drop` arm leaves vectors at rest"
    );
    assert!(vec0_row_exists(&conn, c1), "…including the vec0 row");

    // ---- slow the embedder down so post-4 is falsifiable in both directions ----
    delay_ms.store(8_000, Ordering::SeqCst);

    // (3)+(4) a subsequent write of the SAME kind must enqueue nothing.
    engine.write(&[node("doc", "N2", r#"{"summary":"written after the drop"}"#)]).expect("N2");
    engine.drain(2_000).expect(
        "drain must not wait on work for a DROPPED projection — with the embedder at 8s a 2s \
         barrier can only return Ok if nothing was enqueued",
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "a write after the drop must not be embedded — the dense arm is no longer declared"
    );

    // (5) the late-enrolment path must not re-enrol a brand-new kind either.
    //
    // The kind is `note` rather than an arbitrary string because
    // `resolve_source_type` accepts only a LOCKED set
    // (email/article/paper/meeting/note/todo/doc + edge_fact) and errors on
    // anything else. An unmappable kind wedges the projection worker — a
    // PRE-EXISTING landed-20c defect on both enrolment paths, reproduced against
    // this file's parent commit and reported out-of-scope; it is not what this
    // test is pinning, so it is deliberately not stepped on here.
    engine.write(&[node("note", "M1", r#"{"summary":"a new kind after the drop"}"#)]).expect("M1");
    engine.drain(2_000).expect("drain: still nothing enqueued");
    let conn = ro(&path);
    assert!(
        !vector_kind_registered(&conn, "note"),
        "late enrolment must be gated on an ACTIVE declaration, not merely on 'kind unseen'"
    );
    assert!(!vector_kind_registered(&conn, "doc"), "…and must not re-enrol `doc` either");
    let c2 = active_cursor(&conn, "N2");
    assert!(!vector_row_exists(&conn, c2), "N2 has no vector: nothing was ever enqueued for it");
    assert!(!vec0_row_exists(&conn, c2), "…and no vec0 row");
    assert!(vector_row_exists(&conn, c1), "N1's pre-drop vector is still at rest");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "still exactly the one pre-drop embed");

    // (6) re-applying the same drop is an idempotent no-op.
    let again = engine.configure_projections(&[], &["summary".to_string()]).expect("re-drop");
    assert!(again.dropped.is_empty(), "dropping an absent projection is a no-op, not an error");
    let conn = ro(&path);
    assert!(!vector_kind_registered(&conn, "doc"), "re-drop keeps the kind un-enrolled");
    assert!(vector_row_exists(&conn, c1), "re-drop still deletes no embedding");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "re-drop embeds nothing");

    // (7) RE-declaring re-enrols and backfills — un-enrolment is reversible, and
    // the rows written while the arm was off are picked up, not stranded.
    delay_ms.store(0, Ordering::SeqCst);
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("re-declare");
    engine.drain(30_000).expect("drain the re-declared backfill");
    assert_eq!(readiness(engine, "summary"), Some(DenseReadiness::Ready));
    let conn = ro(&path);
    assert!(vector_kind_registered(&conn, "doc"), "re-declaring re-enrols the kind");
    assert!(vector_row_exists(&conn, c2), "the row written while the arm was off is backfilled");
    assert_eq!(
        leaf_rows_without_vectors(&conn),
        0,
        "after the re-declared backfill drains, no vector-eligible row lacks its vector"
    );

    opened.engine.close().unwrap();
}

/// **`'edge_fact'` is NOT the registry's to un-enrol.**
///
/// `project_canonical_edge_row` (G11) auto-registers `'edge_fact'` off the
/// presence of an edge BODY, unconditionally and independently of the projection
/// registry — that lifecycle predates this slice and is not keyed to any
/// `searchable→vector` declaration. So the inverse must be scoped to the NODE
/// kinds the registry mechanism enrols and must leave `'edge_fact'` alone;
/// otherwise a `drop` would silently kill the edge-fact dense arm as a side
/// effect.
#[test]
fn dropping_a_vector_projection_leaves_edge_fact_enrolled() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "flush_barrier_drop_edge_fact");
    let embedder = CountingEmbedder::new();
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("write N1");
    engine.write(&[node("doc", "N2", r#"{"summary":"another meaning"}"#)]).expect("write N2");
    engine.write(&[edge("E1", "N1", "N2", "N1 elaborates N2")]).expect("write E1");
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");
    engine.drain(30_000).expect("drain");

    let conn = ro(&path);
    assert!(vector_kind_registered(&conn, "doc"), "fixture: `doc` enrolled by the declaration");
    assert!(vector_kind_registered(&conn, "edge_fact"), "fixture: `edge_fact` enrolled by G11");
    assert_eq!(vector_kind_count(&conn), 2, "fixture: exactly `doc` + `edge_fact`");

    engine.configure_projections(&[], &["summary".to_string()]).expect("drop");

    let conn = ro(&path);
    assert!(!vector_kind_registered(&conn, "doc"), "the node kind is un-enrolled");
    assert!(
        vector_kind_registered(&conn, "edge_fact"),
        "`edge_fact` is auto-registered off edge BODIES by `project_canonical_edge_row`, not off \
         the projection registry — dropping a node projection must not end its lifecycle"
    );
    assert_eq!(vector_kind_count(&conn), 1, "exactly the node kind was removed");

    // The edge arm still works: a new edge body is still embedded after the drop.
    engine.write(&[edge("E2", "N2", "N1", "N2 is elaborated by N1")]).expect("write E2");
    engine.drain(30_000).expect("drain the edge body");
    let conn = ro(&path);
    assert!(vector_kind_registered(&conn, "edge_fact"), "the edge dense arm survived the drop");

    opened.engine.close().unwrap();
}

// ===========================================================================
// Leg D (fix-2, codex §9 [P1] "Don't enrol kinds the vector writer can't
//         commit") — enrolment must be RESTRICTED to commit-able node kinds
// ===========================================================================
//
// `commit_projection_outcomes` maps `kind -> source_type` through
// `resolve_source_type`, which accepts only a LOCKED vocabulary
// (email/article/paper/meeting/note/todo + the `doc` fixture coercion, plus
// `edge_fact` for edge bodies) and returns `EngineError::Storage` for anything
// else. `PreparedWrite::Node`, by contrast, accepts ANY non-empty `kind` — so a
// corpus can perfectly legitimately hold a `"invoice"` node.
//
// Both of Slice 20c's enrolment paths enrol off the kind actually present in
// `canonical_nodes`. Enrolling an unmappable kind makes the scheduler pick the
// row up, and the commit then FAILS before recording a terminal: the row stays
// pending forever, the scanner re-enqueues it forever, `drain` burns its whole
// timeout into `EngineError::Scheduler`, and readiness is stuck on `embedding`.
// That is a permanent LIVENESS wedge for the whole workspace — including the
// rows whose kinds ARE commit-able.
//
// The fix is the first of codex's two options: restrict ENROLMENT. A
// non-commit-able kind simply gets no dense arm, which is exactly its pre-slice
// status quo. It is deliberately NOT a new typed error and adds no governed
// surface.

/// **Declare-time enrolment must skip a kind the vector writer cannot commit.**
///
/// Post-conditions (1, 2 and 4 fail at fix-2 baseline):
///   1. `drain` returns `Ok` — the declaration did not wedge the runtime;
///   2. readiness reaches `ready`;
///   3. the commit-able kind still gets its dense arm (so the filter is not
///      "enrol nothing");
///   4. the non-commit-able kind is NOT enrolled and has no vector — no dense
///      arm, and no `projection_failures` audit noise either.
#[test]
fn a_kind_the_vector_writer_cannot_commit_is_not_enrolled_at_declaration_time() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "flush_barrier_uncommittable_declare");
    let embedder = CountingEmbedder::new();
    let calls = Arc::clone(&embedder.calls);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    // A corpus holding BOTH a commit-able kind and one outside the locked set.
    // `PreparedWrite::Node` accepts it: nothing in `validate_write` constrains
    // `kind` to `resolve_source_type`'s vocabulary.
    engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("write N1");
    engine
        .write(&[node("invoice", "I1", r#"{"summary":"payable in 30 days"}"#)])
        .expect("write I1");
    engine.drain(30_000).expect("baseline drain");

    let conn = ro(&path);
    let c_doc = active_cursor(&conn, "N1");
    let c_invoice = active_cursor(&conn, "I1");
    assert!(!vector_kind_registered(&conn, "doc"), "fixture: nothing enrolled yet");
    assert!(!vector_kind_registered(&conn, "invoice"), "fixture: nothing enrolled yet");

    // ---- the declaration ----
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");

    // (1) The wedge. At baseline `invoice` is enrolled, the scheduler picks the
    // row up, `commit_projection_outcomes` fails on `resolve_source_type`, no
    // terminal is ever written, and the scanner re-enqueues it forever.
    engine.drain(30_000).expect(
        "WEDGED: enrolling a node kind the vector writer cannot commit leaves the row pending \
         forever — no terminal is ever recorded, so `drain` burns its whole timeout",
    );

    // (2) …and the whole workspace's readiness is stuck with it.
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Ready),
        "a kind the vector writer cannot commit must not hold the corpus in `embedding` forever"
    );

    let conn = ro(&path);
    // (3) The commit-able kind is unaffected — the filter is not "enrol nothing".
    assert!(vector_kind_registered(&conn, "doc"), "the commit-able kind still gets its dense arm");
    assert!(vector_row_exists(&conn, c_doc), "…and its vector is at rest");
    assert!(vec0_row_exists(&conn, c_doc), "…including the vec0 row");
    assert_eq!(
        leaf_rows_of_kind_without_vectors(&conn, "doc"),
        0,
        "every `doc` row is embedded once readiness reads ready"
    );

    // (4) The non-commit-able kind gets NO dense arm. That is its pre-slice
    // status quo, reported through no new surface: no typed error, no new verb.
    assert!(
        !vector_kind_registered(&conn, "invoice"),
        "ENROLMENT MUST BE RESTRICTED TO COMMIT-ABLE KINDS: `resolve_source_type(\"invoice\")` is \
         `Err`, so an enrolled `invoice` row can never record a terminal"
    );
    assert!(!vector_row_exists(&conn, c_invoice), "the un-enrolled kind has no vector");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "exactly the one commit-able row was embedded");
    assert_eq!(
        projection_failure_rows(&conn),
        0,
        "a kind with no dense arm is not a FAILURE — it must not pollute the failure audit"
    );

    opened.engine.close().unwrap();
}

/// **Late enrolment must apply the SAME restriction.** Same defect, reached by
/// writing second instead of declaring second: `Engine::enrol_batch_vector_kinds`
/// enrols off the batch's kinds, so a post-declaration write of an unmappable
/// kind wedges the runtime just as thoroughly.
#[test]
fn a_kind_the_vector_writer_cannot_commit_is_not_late_enrolled_on_write() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "flush_barrier_uncommittable_late");
    let embedder = CountingEmbedder::new();
    let calls = Arc::clone(&embedder.calls);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    // Declare over an EMPTY corpus, so the declare-time path enrols nothing and
    // the write path is the only enrolment reachable below.
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");
    engine.drain(30_000).expect("declare-on-empty drain");

    engine
        .write(&[node("invoice", "I1", r#"{"summary":"payable in 30 days"}"#)])
        .expect("write I1");
    engine.drain(30_000).expect(
        "WEDGED: late-enrolling a node kind the vector writer cannot commit leaves the row pending \
         forever",
    );
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Ready),
        "a post-declaration write of an unmappable kind must not hold readiness in `embedding`"
    );

    let conn = ro(&path);
    let c_invoice = active_cursor(&conn, "I1");
    assert!(
        !vector_kind_registered(&conn, "invoice"),
        "LATE ENROLMENT MUST BE RESTRICTED TOO: the write path enrols off the batch's kinds, so it \
         needs the same commit-ability filter as the declare-time backfill"
    );
    assert!(!vector_row_exists(&conn, c_invoice), "the un-enrolled kind has no vector");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "nothing was embedded");

    // …and a commit-able kind written afterwards still late-enrols normally.
    engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("write N1");
    engine.drain(30_000).expect("drain the commit-able write");
    assert_eq!(readiness(engine, "summary"), Some(DenseReadiness::Ready));
    let conn = ro(&path);
    let c_doc = active_cursor(&conn, "N1");
    assert!(vector_kind_registered(&conn, "doc"), "the commit-able kind still late-enrols");
    assert!(vector_row_exists(&conn, c_doc), "…and its vector is at rest");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "exactly the one commit-able row was embedded");

    opened.engine.close().unwrap();
}

// ===========================================================================
// Leg E (fix-2, codex §9 [P2] "Backfill stranded rows when late-enrolling a
//         kind") — the write path's enrolment owes the same stranded-row
//         treatment the declare-time path performs
// ===========================================================================

/// Read back the `default` embedder identity a previous session pinned, so a
/// reopen cannot fail closed on `EmbedderIdentityMismatch`
/// (ADR-0.6.0-vector-identity-embedder-owned) and the fixture cannot rot against
/// a default-identity change.
fn stored_default_identity(path: &Path) -> EmbedderIdentity {
    let conn = ro(path);
    conn.query_row(
        "SELECT name, revision, dimension FROM _fathomdb_embedder_profiles
         WHERE profile = 'default'",
        [],
        |r| {
            Ok(EmbedderIdentity::new(
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u32>(2)?,
            ))
        },
    )
    .expect("stored default embedder identity")
}

/// **A LATE enrolment must un-strand the rows an earlier session left behind.**
///
/// The declare-time path (`enqueue_declared_vector_backfill`) already deletes the
/// permanent `'up_to_date'` terminals of rows that carry no vector and rewinds the
/// readiness watermark to reach them. The write path enrols the same kind through
/// a different door and, at fix-2 baseline, enqueues ONLY the row in its own
/// batch.
///
/// The shape is exactly codex's: a database persists a `searchable→vector`
/// declaration while opened WITHOUT an embedder (Q6a graceful-absent — it defers,
/// enrolling nothing), then reopens WITH one and writes the same kind BEFORE
/// re-applying the projection. At baseline the new row drains, readiness reports
/// `ready`, and the rows from the no-embedder session keep their terminals with no
/// vector — a FALSE READY, the exact defect class R-20-DR exists to eliminate.
///
/// The at-rest oracle is `leaf_rows_without_vectors`, keyed off
/// `canonical_nodes.row_kind` with **no** join to `_fathomdb_vector_kinds`: a
/// kind-registry join returns a hollow zero precisely when enrolment is the thing
/// at issue.
#[test]
fn late_enrolment_backfills_the_rows_an_earlier_session_stranded() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "flush_barrier_late_enrol_backfill");

    // ---- session 1: NO embedder. The declaration persists and DEFERS; every row
    // written under it takes a permanent `'up_to_date'` terminal with no vector.
    {
        let opened = Engine::open(path.clone()).expect("open without embedder");
        let engine = &opened.engine;
        engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");
        engine.write(&[node("doc", "N1", r#"{"summary":"stranded one"}"#)]).expect("write N1");
        engine.write(&[node("doc", "N2", r#"{"summary":"stranded two"}"#)]).expect("write N2");
        engine.drain(5_000).expect("drain must not burn its timeout on a dead dense arm");

        let conn = ro(&path);
        assert!(
            !vector_kind_registered(&conn, "doc"),
            "fixture: no live embedder ⇒ no dense arm ⇒ nothing enrolled"
        );
        assert_eq!(leaf_rows_without_vectors(&conn), 2, "fixture: two rows are stranded");
        opened.engine.close().unwrap();
    }

    let conn = ro(&path);
    let c1 = active_cursor(&conn, "N1");
    let c2 = active_cursor(&conn, "N2");
    let watermark_before = projection_cursor(&conn);
    assert!(
        watermark_before >= c2,
        "fixture: session 1's terminals carried the readiness watermark past both rows"
    );

    // ---- session 2: SAME database, now WITH an embedder. The projection is NOT
    // re-applied; the WRITE is what turns the dense arm on (late enrolment).
    let embedder = CountingEmbedder::with_identity(stored_default_identity(&path));
    let calls = Arc::clone(&embedder.calls);
    let delay_ms = Arc::clone(&embedder.delay_ms);
    // Slow the embedder so the post-write probes below are read BEFORE the worker
    // can advance the watermark again — otherwise "was it rewound?" is a race.
    delay_ms.store(1_500, Ordering::SeqCst);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("reopen");
    let engine = &opened.engine;

    engine.write(&[node("doc", "N3", r#"{"summary":"written before re-applying"}"#)]).expect("N3");

    let conn = ro(&path);
    assert!(vector_kind_registered(&conn, "doc"), "the write LATE-ENROLLED the kind");
    assert!(
        projection_cursor(&conn) < c1,
        "LATE ENROLMENT STRANDS ROWS: turning the dense arm on from the write path must run the \
         SAME stranded-row treatment the declare-time backfill does — delete the `'up_to_date'` \
         terminals of rows with no vector and REWIND the readiness watermark to reach them"
    );
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Embedding),
        "readiness must not read `ready` while pre-existing rows still lack their vectors"
    );

    delay_ms.store(0, Ordering::SeqCst);
    engine.drain(30_000).expect("drain flushes the late-enrolled backfill");

    assert_eq!(readiness(engine, "summary"), Some(DenseReadiness::Ready));
    let conn = ro(&path);
    assert_eq!(
        leaf_rows_without_vectors(&conn),
        0,
        "FALSE-READY: `drain()` returned Ok and readiness reads `ready`, but rows written in the \
         no-embedder session still have no vector at rest"
    );
    for (label, cursor) in [("N1", c1), ("N2", c2), ("N3", active_cursor(&conn, "N3"))] {
        assert!(vector_row_exists(&conn, cursor), "{label}'s vector must exist at rest");
        assert!(vec0_row_exists(&conn, cursor), "{label}'s vec0 row must exist at rest");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 3, "all three rows were embedded, exactly once each");

    // Idempotence: a further write of the SAME kind finds nothing stranded, so it
    // must not rewind the watermark or re-embed.
    let watermark_after = projection_cursor(&conn);
    engine.write(&[node("doc", "N4", r#"{"summary":"nothing left to un-strand"}"#)]).expect("N4");
    engine.drain(30_000).expect("drain");
    let conn = ro(&path);
    assert!(
        projection_cursor(&conn) >= watermark_after,
        "a write with nothing stranded must not rewind the readiness watermark"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 4, "only the new row was embedded");
    assert_eq!(leaf_rows_without_vectors(&conn), 0);

    opened.engine.close().unwrap();
}

// ===========================================================================
// Leg F (fix-4, codex §9 round 3 [P1] "the write is permanently lost") — an
//         ABSENT embedder is an ENVIRONMENT fact, not an embed failure, so it
//         must not record a TERMINAL
// ===========================================================================

/// The raw `_fathomdb_projection_terminal` state for one cursor.
///
/// Three values matter here and they are the whole finding:
///
/// - `Some("failed")` — what retry-exhaustion against an ABSENT embedder
///   recorded at baseline. No graft path reopens a `'failed'` terminal, and none
///   should: re-enqueueing one would loop a genuinely-failing row forever. So
///   the write is lost PERMANENTLY.
/// - `Some("up_to_date")` — the not-enqueued branch's terminal. For an
///   already-ENROLLED kind that is the state design-of-record §4.1 invariant 1
///   calls FORBIDDEN (a torn `ready` without the vector), which is why the
///   enqueue-side gate was rejected.
/// - `None` — PENDING. The row is still outstanding, readiness reads
///   `embedding`, and the NEXT live-embedder session's ORDINARY scheduler picks
///   it up. No graft, no new recovery machinery.
fn terminal_state(conn: &rusqlite::Connection, cursor: i64) -> Option<String> {
    conn.query_row(
        "SELECT state FROM _fathomdb_projection_terminal WHERE write_cursor = ?1",
        [cursor],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

/// The node-FTS shadow row for one cursor — the "the write is still lexically
/// searchable" half of the contract, read AT REST so it does not depend on FTS
/// query semantics.
fn fts_row_exists(conn: &rusqlite::Connection, cursor: i64) -> bool {
    conn.query_row("SELECT COUNT(*) FROM search_index WHERE write_cursor = ?1", [cursor], |r| {
        r.get::<_, i64>(0)
    })
    .expect("fts row probe")
        > 0
}

/// **A write made with no live embedder, over an ALREADY-ENROLLED kind, must
/// stay RECOVERABLE.**
///
/// fix-2 gated ENROLMENT on a live embedder, but `_fathomdb_vector_kinds` is
/// durable: once an embedder-backed session has enrolled `doc`, every later
/// session sees `kind_is_vector_indexed == true`. Reopening that database with
/// `EmbedderChoice::None` and writing the same kind therefore still enqueues.
/// At baseline the worker exhausts its retry ladder against
/// `shared.embedder == None`, records an `EmbedderNotConfiguredError`
/// `'failed'` terminal plus a `projection_failures` audit row — and since no
/// graft path reopens a `'failed'` terminal, reopening WITH an embedder leaves
/// that write **permanently** unembedded while readiness reports `ready`.
///
/// The fix keys on the ENVIRONMENTAL condition (`shared.embedder.is_none()`),
/// which cannot change within a session: that outcome records NEITHER the audit
/// row NOR a terminal, so the row simply stays PENDING and the next live-embedder
/// session's ordinary scheduler embeds it.
///
/// **Consumer-visible consequence, pinned deliberately below.** Because the row
/// stays outstanding, a no-embedder session over an already-enrolled corpus has
/// `dense_readiness == embedding` and `drain` burns its timeout into
/// `EngineError::Scheduler` — for that session, permanently. That is the honest
/// answer under §4.1 invariant 1 (the ONLY tolerable torn state is `embedding`
/// with the vector absent) and it is LOUD rather than silent. It is asserted
/// here so the behaviour cannot change back without a test saying so.
///
/// The at-rest oracles key off `canonical_nodes.row_kind` with **no** join to
/// `_fathomdb_vector_kinds` — the registry is precisely what is non-empty here,
/// so a joined probe would return a hollow zero.
#[test]
fn a_no_embedder_session_leaves_an_enrolled_kinds_write_recoverable() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "flush_barrier_no_embedder_recoverable");

    // ---- session 0: create the database with NO embedder, so the `default`
    // profile pins `default_embedder_identity()`. Every later session then uses
    // that same identity and the reopens cannot fail closed on
    // `EmbedderIdentityMismatch` (ADR-0.6.0-vector-identity-embedder-owned).
    Engine::open(path.clone()).expect("create").engine.close().unwrap();
    let identity = stored_default_identity(&path);

    // ---- session 1: WITH an embedder. This is what durably ENROLS `doc` into
    // `_fathomdb_vector_kinds`; from here on every session sees an enrolled kind.
    {
        let embedder = CountingEmbedder::with_identity(identity.clone());
        let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
        let engine = &opened.engine;
        engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");
        engine
            .write(&[node("doc", "N1", r#"{"summary":"embedded in session one"}"#)])
            .expect("write N1");
        engine.drain(30_000).expect("session 1 drain");

        let conn = ro(&path);
        assert!(vector_kind_registered(&conn, "doc"), "fixture: session 1 enrolled `doc`");
        assert_eq!(leaf_rows_without_vectors(&conn), 0, "fixture: session 1's row is embedded");
        opened.engine.close().unwrap();
    }
    let c1 = active_cursor(&ro(&path), "N1");

    // ---- session 2: the SAME database, reopened with `EmbedderChoice::None`.
    let c2 = {
        let opened = Engine::open(path.clone()).expect("reopen without embedder");
        let engine = &opened.engine;
        // Collapse the retry ladder for the BASELINE's benefit, not the fix's.
        // At baseline the `'failed'` terminal only lands after 0 + 1 + 4 + 16 s;
        // every probe below would then read a merely-unterminated row and pass
        // VACUOUSLY on the broken code. With no delays the baseline reaches its
        // terminal in milliseconds, so the assertions are genuinely falsifying.
        // Under the fix this seam is inert — an absent embedder never retries.
        engine.set_projection_retry_delays_for_test(&[]);

        // Fixture precondition, asserted rather than assumed: the enrolment
        // PERSISTED, so this session's write reaches `kind_is_vector_indexed`.
        assert!(
            vector_kind_registered(&ro(&path), "doc"),
            "fixture: the kind stays enrolled across the reopen — that is the whole finding"
        );

        engine
            .write(&[node("doc", "N2", r#"{"summary":"written with no dense arm"}"#)])
            .expect("write N2");

        // THE CONSUMER-VISIBLE CONSEQUENCE, pinned. An enrolled row with no
        // vector is outstanding, and this session has no way to satisfy it, so
        // the barrier must NOT clear. Baseline clears it by recording a
        // `'failed'` terminal — which is exactly how the write gets lost.
        let drained = engine.drain(3_000);
        assert!(
            matches!(drained, Err(fathomdb_engine::EngineError::Scheduler)),
            "NO-EMBEDDER SESSION: an enrolled row with no vector is outstanding and this session \
             cannot satisfy it, so `drain` must burn its timeout into `Scheduler` rather than \
             clear the barrier by terminating the row. Got: {drained:?}"
        );
        assert_eq!(
            readiness(engine, "summary"),
            Some(DenseReadiness::Unavailable),
            "an absent runtime remains unavailable even when durable work is pending"
        );

        let conn = ro(&path);
        let c2 = active_cursor(&conn, "N2");

        // The write is ACCEPTED and stays lexically searchable meanwhile.
        assert!(fts_row_exists(&conn, c2), "the write is accepted and still lexically searchable");

        // THE FINDING. Both of these are `'failed'`-side at baseline.
        assert_eq!(
            projection_failure_rows(&conn),
            0,
            "an ABSENT embedder is an ENVIRONMENT fact, not an embed failure — it must not \
             pollute the `projection_failures` audit"
        );
        assert_eq!(
            terminal_state(&conn, c2),
            None,
            "PERMANENTLY LOST WRITE: an `EmbedderNotConfiguredError` must record NO terminal. A \
             `'failed'` terminal is permanent by design (nothing reopens one, and nothing should \
             — that would loop a genuinely-failing row forever), so terminating here loses the \
             write. Leaving the row PENDING is what lets the next live-embedder session's \
             ORDINARY scheduler pick it up"
        );
        assert!(!vector_row_exists(&conn, c2), "fixture: no dense arm ⇒ no vector yet");
        assert!(vector_row_exists(&conn, c1), "session 1's vector is untouched");

        opened.engine.close().unwrap();
        c2
    };

    // ---- session 3: the SAME database, WITH an embedder again. NO re-apply and
    // NO further write — the ordinary scheduler is the whole recovery path.
    let embedder = CountingEmbedder::with_identity(identity);
    let calls = Arc::clone(&embedder.calls);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("reopen");
    let engine = &opened.engine;
    // `_fathomdb_vector_kinds` is non-empty on this database, so `Engine::open`
    // engages the 0.8.18 vector-equivalence probe, which embeds its reference set
    // AT OPEN (measured: 90 calls). Those are NOT projection work, so the count
    // below is a DELTA from this post-open baseline.
    let calls_at_open = calls.load(Ordering::SeqCst);

    engine.drain(30_000).expect("drain flushes the row the no-embedder session left pending");
    assert_eq!(readiness(engine, "summary"), Some(DenseReadiness::Ready));

    let conn = ro(&path);
    assert_eq!(
        leaf_rows_without_vectors(&conn),
        0,
        "PERMANENTLY LOST WRITE: `drain()` returned Ok and readiness reads `ready`, but the row \
         written in the no-embedder session still has no vector at rest"
    );
    assert!(vector_row_exists(&conn, c2), "the recovered row's vector must exist at rest");
    assert!(vec0_row_exists(&conn, c2), "…including its vec0 row");
    assert_eq!(
        calls.load(Ordering::SeqCst) - calls_at_open,
        1,
        "exactly the ONE outstanding row was embedded — the recovery must not re-embed session \
         1's row"
    );
    assert_eq!(
        projection_failure_rows(&conn),
        0,
        "the recovery leaves no failure audit behind either"
    );

    opened.engine.close().unwrap();
}

// ===========================================================================
// Leg G (fix-5, codex §9 round 4 [P1]) — the no-embedder NODE exclusion must
//         happen INSIDE the scheduler's scan, NOT after its `LIMIT`
// ===========================================================================

/// `PROJECTION_SCAN_FETCH`, restated here because it is private to the engine:
/// `PROJECTION_WORKERS (2) * PROJECTION_COMMIT_BATCH (16)`. It is the `LIMIT` the
/// dispatcher's one scan carries, and therefore the width of the window a
/// post-fetch filter can starve. The fixture asserts it is genuinely exceeded
/// rather than trusting this constant, so a change to it makes the test weaker
/// only by making the fixture assertion fail.
const PROJECTION_SCAN_FETCH: usize = 32;

/// The `write_cursor` of the live edge body carrying `logical_id`. The edge
/// half of [`active_cursor`]; edges live in their own table and carry their own
/// cursors from the SAME monotonic sequence, which is what lets the scheduler
/// `ORDER BY write_cursor` across the node/edge `UNION`.
fn active_edge_cursor(conn: &rusqlite::Connection, logical_id: &str) -> i64 {
    conn.query_row(
        "SELECT write_cursor FROM canonical_edges
         WHERE logical_id = ?1 AND superseded_at IS NULL",
        [logical_id],
        |r| r.get::<_, i64>(0),
    )
    .expect("active edge cursor")
}

/// How many vector-eligible NODE rows are pending (no terminal) below `cursor`.
/// The fixture precondition for Leg G: the scan window must be genuinely
/// over-subscribed by node rows before the edge body.
fn pending_node_rows_below(conn: &rusqlite::Connection, cursor: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*)
         FROM canonical_nodes n
         JOIN _fathomdb_vector_kinds k ON k.kind = n.kind
         LEFT JOIN _fathomdb_projection_terminal t ON t.write_cursor = n.write_cursor
         WHERE n.row_kind IN ('leaf', 'coverage')
           AND n.superseded_at IS NULL
           AND t.write_cursor IS NULL
           AND n.write_cursor < ?1",
        [cursor],
        |r| r.get::<_, i64>(0),
    )
    .expect("pending node rows below cursor")
}

/// Poll `probe` until it reports true or `timeout` elapses. Returns whether it
/// ever did. Used instead of `drain` because Leg G's session deliberately holds
/// permanently-pending node rows (fix-4), so `drain` can only ever time out
/// there — the question is whether the EDGE was scheduled, not whether the
/// session reached idle.
fn poll_until(timeout: Duration, mut probe: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if probe() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// **A pending EDGE body must stay reachable behind a full scan window of
/// no-embedder NODE rows.**
///
/// fix-4 excluded node jobs in a no-embedder session by filtering the vector
/// [`next_pending_projection_jobs`] returned — i.e. AFTER its `ORDER BY
/// write_cursor LIMIT PROJECTION_SCAN_FETCH`. The `LIMIT` therefore applied to
/// the UNFILTERED set: with more than `PROJECTION_SCAN_FETCH` pending node rows
/// ordered before a pending edge body, the one scan comes back entirely full of
/// node jobs, the filter drops all of them, the dispatcher sees an empty batch
/// and goes back to sleep with `pending_scan` already consumed — and the edge
/// body is never scheduled at all. Nothing later re-widens the window, because
/// the node rows are pending FOREVER in that session (that is fix-4's whole
/// point), so the starvation is permanent, not a delay.
///
/// Edges keep their SHIPPED no-embedder behaviour (`'edge_fact'` is
/// auto-registered by the edge write itself, un-gated on the embedder — G11,
/// OOS-13), so "scheduled" is observable as the edge body reaching a TERMINAL.
/// This test does NOT assert edges become recoverable; it asserts only that the
/// scheduler still REACHES them.
#[test]
fn a_pending_edge_body_survives_a_full_scan_window_of_no_embedder_node_rows() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "flush_barrier_scan_window_starvation");

    // ---- session 0: create with NO embedder so the `default` profile pins
    // `default_embedder_identity()` and the reopens cannot fail closed on
    // `EmbedderIdentityMismatch`.
    Engine::open(path.clone()).expect("create").engine.close().unwrap();
    let identity = stored_default_identity(&path);

    // ---- session 1: WITH an embedder — this is what durably ENROLS `doc`.
    {
        let embedder = CountingEmbedder::with_identity(identity);
        let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
        let engine = &opened.engine;
        engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");
        engine.write(&[node("doc", "SEED", r#"{"summary":"enrols the kind"}"#)]).expect("seed");
        engine.drain(30_000).expect("session 1 drain");
        assert!(vector_kind_registered(&ro(&path), "doc"), "fixture: session 1 enrolled `doc`");
        opened.engine.close().unwrap();
    }

    // ---- session 2: the SAME database with NO embedder. Every `doc` row it
    // writes is pending forever (fix-4); the edge body written AFTER them is the
    // row under test.
    let opened = Engine::open(path.clone()).expect("reopen without embedder");
    let engine = &opened.engine;
    // Collapse the retry ladder: an edge body with no embedder takes the SHIPPED
    // path (0 + 1 + 4 + 16 s, then a `'failed'` terminal). Without this the
    // "was it scheduled?" probe would have to outwait 21 s.
    engine.set_projection_retry_delays_for_test(&[]);

    // MORE than one scan window of node rows, all ordered BEFORE the edge.
    let node_rows = PROJECTION_SCAN_FETCH + 8;
    let batch: Vec<PreparedWrite> = (0..node_rows)
        .map(|i| node("doc", &format!("N{i}"), &format!(r#"{{"summary":"row {i}"}}"#)))
        .collect();
    engine.write(&batch).expect("write the node rows");
    engine
        .write(&[edge("E1", "N0", "N1", "the edge body that must not be starved")])
        .expect("edge");

    let conn = ro(&path);
    let edge_cursor = active_edge_cursor(&conn, "E1");
    assert!(
        vector_kind_registered(&conn, "edge_fact"),
        "fixture: an edge body auto-registers `'edge_fact'` (G11), so it IS schedulable work"
    );
    let pending_before = pending_node_rows_below(&conn, edge_cursor);
    assert!(
        pending_before > PROJECTION_SCAN_FETCH as i64,
        "fixture: the scan window must be over-subscribed by node rows ordered BEFORE the edge \
         body — that is the whole scenario. Pending node rows below the edge: {pending_before}, \
         scan window: {PROJECTION_SCAN_FETCH}"
    );

    // THE FINDING. The scheduler must still reach the edge body.
    let scheduled =
        poll_until(Duration::from_secs(20), || terminal_state(&ro(&path), edge_cursor).is_some());
    assert!(
        scheduled,
        "SCAN-WINDOW STARVATION: with more than PROJECTION_SCAN_FETCH ({PROJECTION_SCAN_FETCH}) \
         pending node rows ordered before it, the pending edge body at cursor {edge_cursor} was \
         NEVER scheduled. The no-embedder node exclusion must happen INSIDE \
         `next_pending_projection_jobs`' SQL so the `LIMIT` applies to the ALREADY-FILTERED set; \
         filtering after the fetch lets one full window of node rows hide every later job, and \
         `drain` then times out on that edge workload indefinitely"
    );

    // …and it reached it on the SHIPPED edge path, unchanged by this slice.
    assert_eq!(
        terminal_state(&ro(&path), edge_cursor),
        Some("failed".to_string()),
        "edges keep their shipped no-embedder behaviour (OOS-13): the retry ladder exhausts into \
         a `'failed'` terminal. fix-5 changes WHICH rows the scan returns, not what happens to \
         an edge once it is dispatched"
    );

    // fix-4 is intact: the NODE rows are still pending, not terminated.
    let conn = ro(&path);
    assert_eq!(
        pending_node_rows_below(&conn, edge_cursor),
        pending_before,
        "fix-4 stands: an absent embedder records NO terminal for a NODE row, so every one of \
         them is still pending and still recoverable by the next live-embedder session"
    );

    opened.engine.close().unwrap();
}

// ===========================================================================
// Leg H (fix-5, codex §9 round 4 [P2]) — a BOOT graft and the un-stranding it
//         owes must commit as ONE transaction
// ===========================================================================

/// A raw READ-WRITE connection to the live file, used to install the fault
/// injection below. Same basis as [`ro`]: the engine's exclusive hold is a lock
/// FILE, not a SQLite lock. Only ever used while NO engine is open on `path`.
fn rw(path: &Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(path).expect("open read-write");
    conn.busy_timeout(Duration::from_secs(10)).expect("busy_timeout");
    conn
}

/// **The crash window between the boot registry INSERT and the un-stranding.**
///
/// A boot graft does two things: it registers the kind in
/// `_fathomdb_vector_kinds`, and it repairs the rows that kind stranded
/// (`reenqueue_stranded_vector_rows` — delete their `'up_to_date'` terminals and
/// rewind the watermark). While the INSERT autocommits ahead of the repair's own
/// `BEGIN IMMEDIATE`, a failure in between leaves the kind REGISTERED with older
/// rows still holding `'up_to_date'` terminals and no vectors. That state is
/// self-sealing: on the next open `kind_is_vector_indexed` is already true, so
/// every future write skips the enrolment path — and therefore skips the
/// un-stranding — while `dense_readiness` reads `ready` for rows that will never
/// be embedded. Only a manual re-apply of the projection recovers it.
///
/// **How the window is pinned.** The failure is injected as a real SQLite
/// `BEFORE DELETE` trigger on `_fathomdb_projection_terminal` that `RAISE(ABORT)`s
/// — i.e. the repair genuinely fails, against a real engine and a real database,
/// at exactly the point a crash would truncate it. No process is killed and
/// nothing is mocked; what is reproduced is the resulting STATE, and then its
/// durable consequence (the second half of the test), which is what the finding
/// is actually about.
#[test]
fn a_boot_graft_whose_repair_fails_registers_nothing() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "flush_barrier_enrolment_atomicity");

    // ---- session A: NO embedder. The declaration persists and DEFERS (fix-2:
    // a kind is only enrolled by a session that has a live embedder), so these
    // rows take permanent `'up_to_date'` terminals with no vector: the stranded
    // set a later late enrolment owes a repair to.
    {
        let opened = Engine::open(path.clone()).expect("create without embedder");
        let engine = &opened.engine;
        engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");
        engine.write(&[node("doc", "N1", r#"{"summary":"stranded one"}"#)]).expect("N1");
        engine.write(&[node("doc", "N2", r#"{"summary":"stranded two"}"#)]).expect("N2");
        engine.drain(5_000).expect("no dense arm ⇒ nothing outstanding");
        opened.engine.close().unwrap();
    }
    let identity = stored_default_identity(&path);
    {
        let conn = ro(&path);
        assert!(
            !vector_kind_registered(&conn, "doc"),
            "fixture: a no-embedder session enrols nothing"
        );
        for id in ["N1", "N2"] {
            let cursor = active_cursor(&conn, id);
            assert_eq!(
                terminal_state(&conn, cursor),
                Some("up_to_date".to_string()),
                "fixture: {id} holds the permanent terminal that makes it STRANDABLE"
            );
            assert!(!vector_row_exists(&conn, cursor), "fixture: {id} has no vector");
        }
    }

    // ---- the fault injection, installed while no engine holds the database.
    rw(&path)
        .execute_batch(
            "CREATE TRIGGER fix5_repair_fails
             BEFORE DELETE ON _fathomdb_projection_terminal
             BEGIN SELECT RAISE(ABORT, 'fix-5: the un-stranding repair failed'); END",
        )
        .expect("install the repair-failure injection");

    // ---- session B: WITH an embedder. Open itself triggers the boot graft:
    // register `doc`, then repair N1/N2. The repair now fails, so open must not
    // expose a half-repaired runtime.
    let failed_open = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(CountingEmbedder::with_identity(identity.clone())),
    );
    assert!(
        failed_open.is_err(),
        "the faulted boot graft must fail open rather than expose half-repaired durable state"
    );
    assert!(
        !vector_kind_registered(&ro(&path), "doc"),
        "TORN BOOT GRAFT: the registry INSERT must roll back with the terminal/cursor repair"
    );

    // ---- remove the injection and let the engine heal itself.
    rw(&path).execute_batch("DROP TRIGGER fix5_repair_fails").expect("remove the injection");

    // ---- session C: the SAME database, WITH an embedder, no re-apply and no
    // ordinary write — the boot graft alone must enrol AND un-strand, because
    // nothing was left half-done behind it.
    let embedder = CountingEmbedder::with_identity(identity);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("reopen");
    let engine = &opened.engine;
    engine.drain(30_000).expect("drain flushes the boot-grafted backfill");

    assert_eq!(readiness(engine, "summary"), Some(DenseReadiness::Ready));
    let conn = ro(&path);
    assert_eq!(
        leaf_rows_without_vectors(&conn),
        0,
        "SELF-SEALED FALSE READY: `drain()` returned Ok and readiness reads `ready`, but the rows \
         the torn enrolment stranded still have no vector at rest. The torn state is invisible to \
         every later write precisely BECAUSE the kind is already registered — which is why the \
         two statements have to be atomic"
    );
    for id in ["N1", "N2"] {
        let cursor = active_cursor(&conn, id);
        assert!(vector_row_exists(&conn, cursor), "{id}'s vector must exist at rest");
        assert!(vec0_row_exists(&conn, cursor), "{id}'s vec0 row must exist at rest");
    }

    opened.engine.close().unwrap();
}
