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
//! **No schema step.** `SCHEMA_VERSION` stays 24 (asserted below). Re-enqueueing
//! embed work inside ONE live database is runtime reconfiguration, not a
//! cross-version data migration (HITL 2026-07-21, cf. TC-46).

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    DenseReadiness, Engine, InitialState, PreparedWrite, ProjectionRole, ProjectionSpec,
    ProjectionVector, SourceId,
};
use fathomdb_schema::SQLITE_SUFFIX;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
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
}

impl CountingEmbedder {
    fn new() -> Self {
        Self::with_identity(EmbedderIdentity::new("deterministic", "rev-a", 384))
    }

    fn with_identity(identity: EmbedderIdentity) -> Self {
        Self { identity, calls: Arc::new(AtomicUsize::new(0)) }
    }
}

impl Embedder for CountingEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
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
    assert_eq!(
        fathomdb_schema::SCHEMA_VERSION,
        24,
        "Slice 20c re-enqueues within one live DB — no schema step"
    );

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

/// **The live-embedder gate, and the graceful-GRAFT that pays for it.**
///
/// With `EmbedderChoice::None` there is no dense arm at all, so declaring
/// `searchable→vector` must PERSIST and DEFER (Q6a graceful-absent, exactly like
/// `rankable`) rather than enqueue embeds that could only retry to a `failed`
/// terminal and pollute `projection_failures`. That deferral is only honest if
/// the work actually grafts on later — so the second half of this test reopens
/// the SAME database WITH an embedder, re-applies the SAME spec, and requires the
/// backfill to run and the vectors to land at rest.
#[test]
fn a_declaration_without_a_live_embedder_defers_then_grafts_on_reapply() {
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
            Some(DenseReadiness::Ready),
            "with no live embedder there is no dense arm, so nothing is outstanding"
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

    // The identical spec — the shipped graceful-graft contract: re-applying an
    // already-persisted declaration grafts the deferred build on.
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("re-apply");
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Embedding),
        "the deferred backfill grafts on in a session that HAS an embedder"
    );

    engine.drain(30_000).expect("drain flushes the grafted backfill");
    assert_eq!(readiness(engine, "summary"), Some(DenseReadiness::Ready));
    let conn = ro(&path);
    let cursor = active_cursor(&conn, "N1");
    assert!(vector_row_exists(&conn, cursor), "the grafted backfill landed at rest");
    assert!(vec0_row_exists(&conn, cursor), "…including the vec0 row");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "exactly the one deferred row was embedded");

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
