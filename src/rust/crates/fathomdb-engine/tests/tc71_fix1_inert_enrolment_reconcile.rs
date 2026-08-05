//! 0.8.20 Slice 21 fix-1 (codex §9 round 1 `[P2]`, ledger **TC-71**) —
//! **reconcile the vector kinds an ALREADY-AFFECTED database enrolled before the
//! role gate landed.**
//!
//! ## The defect these tests close
//!
//! Slice 21c made `vector_projection_declared` require the `searchable` ROLE, so
//! a `{roles: [filterable], vector: {}}` declaration no longer turns the dense
//! arm on. That closes the FORWARD doors only. For a database that already ran
//! the old code, `_fathomdb_vector_kinds` ALREADY contains the node kinds, and
//! nothing on the upgrade path removes them:
//!
//! - `Engine::vector_kind_needs_enrolment` returns early (`Ok(false)`) as soon as
//!   `kind_is_vector_indexed` is true, so it never reaches the new role-aware
//!   predicate — the fix cannot see an existing registration;
//! - `project_canonical_node_row` gates the embed enqueue SOLELY on registry
//!   membership (`targets.vector && kind_is_vector_indexed(..)`), never on
//!   `vector_projection_declared`.
//!
//! So upgrading did **not** stop the billable, unexpected embeddings for exactly
//! the population TC-71 was raised for. The only un-enrol path
//! (`unenrol_registry_vector_node_kinds`) fires inside `apply_projection_config`
//! on the `declared_before && !declared_after` edge, which a user who never calls
//! `configure_projections` again never reaches.
//!
//! ## The fix, and the trap it must not fall into
//!
//! A boot-time reconciliation un-enrols the node kinds — but ONLY when the
//! registry demonstrably governs the dense arm and demonstrably declares no
//! `searchable→vector` projection. All three of:
//!
//!   1. `_fathomdb_projection_registry` EXISTS; and
//!   2. at least one row has `vector_declared = 1` (someone actually wrote a
//!      `vector` sub-object — this is what identifies the affected population);
//!      and
//!   3. NO projection satisfies `StoredProjection::wants_vector`.
//!
//! Condition 2 is load-bearing and is what [`a_registry_with_no_vector_subobject_leaves_a_pre_registry_enrolment_untouched`]
//! and [`a_database_with_no_projection_registry_is_left_untouched_on_reopen`]
//! guard. The naive rule "`!vector_projection_declared` ⇒ un-enrol" would fire on
//! every LEGACY database — the predicate answers `false` when the registry is
//! absent or empty — and would silently switch off a dense arm enrolled by other
//! means. That would be a far worse regression than TC-71 itself, so the two
//! no-op tests below are treated as first-class assertions, not filler.
//!
//! `'edge_fact'` is auto-registered by `project_canonical_edge_row` (G11),
//! independently of the projection registry, and must never be un-enrolled —
//! pinned in every reconciling case here.
//!
//! Every test opens with a **live** `CountingEmbedder` and asserts on RAW TABLES
//! (`_fathomdb_vector_kinds`, `_fathomdb_vector_rows`, `vector_default`) plus the
//! embedder's call count. No returned struct is a falsifying oracle for "no
//! embedding happened".

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    Engine, InitialState, PreparedWrite, ProjectionRole, ProjectionSpec, ProjectionVector, SourceId,
};
use fathomdb_schema::SQLITE_SUFFIX;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers (the `slice21c_vector_role_gate` / `slice20c_flush_barrier` shapes;
// kept local so this suite's oracles cannot drift with another file's edits)
// ---------------------------------------------------------------------------

/// A deterministic embedder that COUNTS its calls. "No embed work happened" is
/// the central assertion of this suite, and a call count is the only way to
/// state it as a fact rather than infer it from absent rows.
#[derive(Clone, Debug)]
struct CountingEmbedder {
    identity: EmbedderIdentity,
    calls: Arc<AtomicUsize>,
    /// A settable per-call delay, so "`drain` returned promptly because nothing
    /// was enqueued" is falsifiable in BOTH directions: with the embedder slowed
    /// to 8 s, a 2 s barrier can only return `Ok` if the queue was empty.
    delay_ms: Arc<AtomicU64>,
}

impl CountingEmbedder {
    fn new() -> Self {
        Self {
            identity: EmbedderIdentity::new("deterministic", "rev-a", 384),
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

/// The shape that identifies the affected population: a `vector` sub-object
/// WITHOUT the `searchable` role.
///
/// **0.8.20 Slice 23 (`R-20-SV`) — no longer ACCEPTED by the verb.** Kept so the
/// reject can be asserted in place; see [`declare_legacy_filterable_vector`].
fn filterable_vector_spec(name: &str) -> ProjectionSpec {
    ProjectionSpec {
        name: name.to_string(),
        roles: roles(&[ProjectionRole::Filterable]),
        fts: None,
        vector: Some(ProjectionVector { embedder: None, dense_readiness: None }),
        source: None,
    }
}

/// **0.8.20 Slice 23 (`R-20-SV`) — THE LEGACY BACK DOOR.**
///
/// The HITL ruled on 2026-07-24 (`plan-0.8.20.md` §11 item 4, option (b)) that
/// this suite's shape is an INVALID SPEC, and Slice 23 rejects it with
/// `EngineError::WriteValidation` (pinned in
/// `tests/slice23_spec_validation_reject.rs`). That makes the shape
/// unconstructible through the public verb — which is exactly why this suite
/// matters MORE, not less: it is the suite about databases that already hold the
/// shape at rest, and after Slice 23 a raw registry write is the ONLY way to
/// build that state. So the fixture route moves and every ORACLE
/// (`_fathomdb_vector_kinds`, `_fathomdb_vector_rows`, `vector_default`, the
/// embedder call count) is unchanged.
///
/// Declares the VALID `filterable` half through the verb, then sets
/// `vector_declared = 1` with the raw UPDATE the pre-Slice-23
/// `persist_projection_row` wrote for the same declaration — and asserts the
/// front door is shut on the way past.
fn declare_legacy_filterable_vector(engine: &Engine, path: &Path, name: &str) {
    assert_eq!(
        engine.configure_projections(&[filterable_vector_spec(name)], &[]).expect_err(
            "R-20-SV: a `vector` sub-object without `searchable` is now an invalid spec"
        ),
        fathomdb_engine::EngineError::WriteValidation,
    );
    engine
        .configure_projections(&[filterable_only_spec(name)], &[])
        .expect("the `filterable` half is still a valid declaration");
    let conn = rusqlite::Connection::open(path).expect("open rw");
    let n = conn
        .execute(
            "UPDATE _fathomdb_projection_registry SET vector_declared = 1 WHERE name = ?1",
            [name],
        )
        .expect("legacy vector sub-object");
    assert_eq!(n, 1, "the registry row must exist before the legacy sub-object is added");
}

/// The legitimate dense-arm declaration — the control.
fn searchable_vector_spec(name: &str) -> ProjectionSpec {
    ProjectionSpec {
        name: name.to_string(),
        roles: roles(&[ProjectionRole::Searchable]),
        fts: None,
        vector: Some(ProjectionVector { embedder: None, dense_readiness: None }),
        source: None,
    }
}

/// A projection with NO `vector` sub-object at all — `vector_declared = 0`, so
/// condition 2 of the reconciliation is FALSE for it.
fn filterable_only_spec(name: &str) -> ProjectionSpec {
    ProjectionSpec {
        name: name.to_string(),
        roles: roles(&[ProjectionRole::Filterable]),
        fts: None,
        vector: None,
        source: None,
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

/// A raw READ-ONLY connection to the live file. `mode=ro`, never `immutable=1`.
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

fn vector_row_exists(conn: &rusqlite::Connection, cursor: i64) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM _fathomdb_vector_rows WHERE write_cursor = ?1",
        [cursor],
        |r| r.get::<_, i64>(0),
    )
    .expect("vector row probe")
        > 0
}

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

/// The number of embeddings at rest — the "nothing further changed" oracle for
/// idempotence, and immune to the equivalence probe (which writes to
/// `_fathomdb_embed_probe`, never to `_fathomdb_vector_rows`).
fn vector_row_count(conn: &rusqlite::Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM _fathomdb_vector_rows", [], |r| r.get::<_, i64>(0))
        .expect("vector row count")
}

/// Every enrolled kind, sorted — so "completely untouched" can be asserted as a
/// whole-table equality rather than one probe at a time.
fn enrolled_kinds(conn: &rusqlite::Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT kind FROM _fathomdb_vector_kinds ORDER BY kind")
        .expect("prepare kinds probe");
    let v: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("kinds query")
        .map(|r| r.expect("kind row"))
        .collect();
    v
}

/// The embedder calls spent SINCE `baseline`.
///
/// Absolute counts are not usable across a reopen: the 0.8.18 #5
/// vector-equivalence probe (`run_vector_equivalence_probe`) re-embeds its 45
/// compiled-in references at any open where `_fathomdb_vector_kinds` is
/// non-empty (90 on the open that also populates the baseline table, 45 on each
/// later one). That is shipped, unrelated behaviour, so every "the write embedded
/// / did not embed" assertion here is a DELTA taken from a baseline snapshot
/// captured immediately after `open` returned.
fn calls_since(calls: &AtomicUsize, baseline: usize) -> usize {
    calls.load(Ordering::SeqCst) - baseline
}

/// Physically REMOVE `_fathomdb_projection_registry` from a closed database.
///
/// Schema step 24 creates the table, and migrations are gated on `user_version`,
/// which is already 24 here — so the table stays absent across the reopen. This
/// is the only way to reach the genuine pre-step-24 state (`sqlite_master` has no
/// registry row) from a test, and it is the state the `table_exists` guard in
/// `vector_projection_declared` / `load_projection_registry` exists for.
fn drop_projection_registry(path: &Path) {
    let conn = rusqlite::Connection::open(path).expect("open rw");
    conn.execute_batch("DROP TABLE IF EXISTS _fathomdb_projection_registry")
        .expect("drop registry table");
}

// ===========================================================================
// (1) THE UPGRADE SCENARIO — the codex [P2] itself
// ===========================================================================

/// **The finding.** A database that ran the OLD code under
/// `{roles: [filterable], vector: {}}` already holds `doc` in
/// `_fathomdb_vector_kinds`. Session 1 below builds exactly that state and
/// DEMONSTRATES the surviving harm (the write still embeds). Session 2 reopens on
/// the fixed engine and asserts the harm is over.
///
/// Post-conditions (1 and 3-5 fail at the fix-1 baseline):
///   1. reopening un-enrols `doc`;
///   2. it deletes NO embedding — the shipped non-destructive contract, mirrored
///      verbatim from the drop inverse;
///   3. a write AFTER the reopen enqueues nothing (embedder call count `0`);
///   4. …and leaves nothing at rest for that row;
///   5. `drain` does not wait on it (embedder slowed to 8 s, a 2 s barrier still
///      returns `Ok`).
#[test]
fn an_already_enrolled_inert_vector_kind_is_un_enrolled_on_reopen() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc71_fix1_upgrade");

    // ---- session 1: the state the OLD code produced ----
    let c1 = {
        let embedder = CountingEmbedder::new();
        let calls = Arc::clone(&embedder.calls);
        let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
        let engine = &opened.engine;

        declare_legacy_filterable_vector(engine, &path, "summary");
        // What the OLD `vector_projection_declared` did on that declaration:
        // enrol the node kind. The `#[doc(hidden)]` hook reproduces the resulting
        // at-rest state exactly (`INSERT OR REPLACE` into `_fathomdb_vector_kinds`
        // with `DEFAULT_VECTOR_PROFILE`) — it is the same row the old forward door
        // wrote.
        engine.configure_vector_kind_for_test("doc").expect("enrol as the old code did");

        engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("write N1");
        engine.drain(30_000).expect("drain");

        let conn = ro(&path);
        let c1 = active_cursor(&conn, "N1");
        // FIXTURE, and the finding restated as a fact: the role gate alone does
        // NOT stop this. `project_canonical_node_row` reads only the registry
        // membership, so the write embedded.
        assert!(
            vector_kind_registered(&conn, "doc"),
            "fixture: the affected database has `doc` enrolled"
        );
        assert!(vector_row_exists(&conn, c1), "fixture: and the old code embedded N1");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "fixture: exactly one embed in session 1");

        opened.engine.close().unwrap();
        c1
    };

    // ---- session 2: the upgrade. Reopening must self-heal. ----
    let embedder = CountingEmbedder::new();
    let calls = Arc::clone(&embedder.calls);
    let delay_ms = Arc::clone(&embedder.delay_ms);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("reopen");
    let engine = &opened.engine;
    let after_open = calls.load(Ordering::SeqCst);

    let conn = ro(&path);
    // (1) THE assertion. Reconciliation runs on the writer connection inside
    // `open`, before any reader or worker spawns, so it is settled and race-free
    // the instant `open` returns — no barrier, no timing proxy.
    assert!(
        !vector_kind_registered(&conn, "doc"),
        "fix-1 (codex §9 [P2]): the role gate only closed the FORWARD doors. A database that \
         already enrolled `doc` under `{{roles:[filterable], vector:{{}}}}` keeps embedding \
         forever, because `vector_kind_needs_enrolment` short-circuits on \
         `kind_is_vector_indexed` and `project_canonical_node_row` never consults the registry. \
         Upgrading must RECONCILE the enrolment away"
    );
    // (2) …without deleting anything, exactly like the drop inverse.
    assert!(
        vector_row_exists(&conn, c1),
        "the reconciliation must delete NO embedding — 'vectors already at rest survive' is the \
         shipped contract of the drop arm this mirrors"
    );
    assert!(vec0_row_exists(&conn, c1), "…including the vec0 row");

    // (3)+(4)+(5) the billable harm is actually over.
    delay_ms.store(8_000, Ordering::SeqCst);
    engine.write(&[node("doc", "N2", r#"{"summary":"written after the upgrade"}"#)]).expect("N2");
    engine
        .drain(2_000)
        .expect("`drain` must not wait: with the kind un-enrolled nothing was ever enqueued");
    assert_eq!(
        calls_since(&calls, after_open),
        0,
        "fix-1: after the upgrade a write under the inert declaration must not spend a single \
         embed call — this is TC-71's whole stated harm"
    );
    // …and the healing open itself spends nothing either. `doc` was the ONLY
    // enrolled kind, so once the reconciliation (which runs inside `open_locked`)
    // has removed it the 0.8.18 #5 equivalence probe finds `_fathomdb_vector_kinds`
    // empty and correctly does no work: there is no dense arm left to guard. This
    // pins the ORDERING — reconcile first, probe second — so the upgrade does not
    // spend 90 probe embeds on an arm it is in the middle of switching off.
    assert_eq!(
        after_open, 0,
        "the healing open must not spend embed calls either: with the inert enrolment reconciled \
         away before the vector-equivalence probe runs, there is no registered kind to guard"
    );
    let conn = ro(&path);
    let c2 = active_cursor(&conn, "N2");
    assert!(!vector_row_exists(&conn, c2), "N2 has no vector: nothing was enqueued for it");
    assert!(!vec0_row_exists(&conn, c2), "…and no vec0 row");
    assert!(!vector_kind_registered(&conn, "doc"), "…and the write did not re-enrol the kind");

    opened.engine.close().unwrap();
}

// ===========================================================================
// (2) THE TRAP — a legacy database must be left COMPLETELY untouched
// ===========================================================================

/// **The destructive failure mode, guarded.** `vector_projection_declared`
/// answers `false` when `_fathomdb_projection_registry` does not exist. Every
/// pre-step-24 database is in that state, and many have a legitimately working
/// dense arm enrolled by other means. A blanket
/// "`!vector_projection_declared` ⇒ un-enrol" would silently switch vector search
/// off for all of them.
///
/// This test builds a database with a working dense arm and NO registry table at
/// all, reopens it, and asserts the enrolment table is byte-for-byte the same and
/// the dense arm still embeds.
#[test]
fn a_database_with_no_projection_registry_is_left_untouched_on_reopen() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc71_fix1_legacy_no_registry");

    // ---- session 1: a legacy dense arm, enrolled with no registry involved ----
    let c1 = {
        let embedder = CountingEmbedder::new();
        let calls = Arc::clone(&embedder.calls);
        let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
        let engine = &opened.engine;
        engine.configure_vector_kind_for_test("doc").expect("legacy enrolment");
        engine.configure_vector_kind_for_test("edge_fact").expect("legacy edge enrolment");
        engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("write N1");
        engine.drain(30_000).expect("drain");
        let conn = ro(&path);
        let c1 = active_cursor(&conn, "N1");
        assert!(vector_row_exists(&conn, c1), "fixture: the legacy dense arm works");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "fixture: one embed");
        opened.engine.close().unwrap();
        c1
    };

    // Now make it a genuine pre-step-24 shape: no registry table in
    // `sqlite_master` at all.
    drop_projection_registry(&path);
    let conn = ro(&path);
    let before = enrolled_kinds(&conn);
    assert_eq!(
        before,
        vec!["doc".to_string(), "edge_fact".to_string()],
        "fixture: both kinds enrolled before the reopen"
    );
    drop(conn);

    // ---- session 2: the reopen must be a TOTAL no-op on the enrolment table ----
    let embedder = CountingEmbedder::new();
    let calls = Arc::clone(&embedder.calls);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("reopen");
    let engine = &opened.engine;
    let after_open = calls.load(Ordering::SeqCst);

    let conn = ro(&path);
    assert_eq!(
        enrolled_kinds(&conn),
        before,
        "THE TRAP: a LEGACY database (no `_fathomdb_projection_registry`) must be left completely \
         untouched. `vector_projection_declared` returns false for it, so a naive \
         '!declared ⇒ un-enrol' reconciliation would silently kill a working dense arm — a far \
         worse regression than TC-71 itself"
    );
    assert!(vector_row_exists(&conn, c1), "…and the existing embedding survives");
    drop(conn);

    // …and the dense arm still WORKS, which whole-table equality alone cannot show.
    engine.write(&[node("doc", "N2", r#"{"summary":"still embedding"}"#)]).expect("N2");
    engine.drain(30_000).expect("drain");
    assert_eq!(
        calls_since(&calls, after_open),
        1,
        "the legacy dense arm must still embed after the reopen"
    );
    let conn = ro(&path);
    assert!(vector_row_exists(&conn, active_cursor(&conn, "N2")), "…with a real vector at rest");

    opened.engine.close().unwrap();
}

// ===========================================================================
// (3) CONDITION 2 — a registry with no `vector` sub-object anywhere
// ===========================================================================

/// The same trap, one step less extreme and far more common after an upgrade:
/// the registry table EXISTS (step 24 creates it on every open) and is governed —
/// it holds a real `{filterable}` declaration — but NO row carries a `vector`
/// sub-object. The enrolment therefore came from somewhere the registry never
/// owned (a pre-registry era, or the `#[doc(hidden)]` hook several shipped suites
/// use, e.g. `slice15e_prekn_filterable`).
///
/// Condition 2 (`EXISTS(vector_declared = 1)`) is the ONLY thing separating this
/// from the reconciling case, so it is asserted on its own.
#[test]
fn a_registry_with_no_vector_subobject_leaves_a_pre_registry_enrolment_untouched() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc71_fix1_registry_no_vector");

    let c1 = {
        let embedder = CountingEmbedder::new();
        let calls = Arc::clone(&embedder.calls);
        let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
        let engine = &opened.engine;
        engine.configure_vector_kind_for_test("doc").expect("pre-registry enrolment");
        engine.configure_vector_kind_for_test("edge_fact").expect("edge enrolment");
        // A governed registry — but with NO `vector` sub-object anywhere.
        engine
            .configure_projections(&[filterable_only_spec("summary")], &[])
            .expect("declare a plain filterable projection");
        engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("write N1");
        engine.drain(30_000).expect("drain");
        let conn = ro(&path);
        let c1 = active_cursor(&conn, "N1");
        assert!(vector_row_exists(&conn, c1), "fixture: the dense arm works");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "fixture: one embed");
        opened.engine.close().unwrap();
        c1
    };

    let conn = ro(&path);
    let before = enrolled_kinds(&conn);
    assert_eq!(before, vec!["doc".to_string(), "edge_fact".to_string()], "fixture: both enrolled");
    drop(conn);

    let embedder = CountingEmbedder::new();
    let calls = Arc::clone(&embedder.calls);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("reopen");
    let engine = &opened.engine;
    let after_open = calls.load(Ordering::SeqCst);

    let conn = ro(&path);
    assert_eq!(
        enrolled_kinds(&conn),
        before,
        "CONDITION 2: a registry that declares NO `vector` sub-object anywhere is not the \
         TC-71-affected population, so a pre-registry enrolment under it must be left alone"
    );
    assert!(vector_row_exists(&conn, c1), "…and the existing embedding survives");
    drop(conn);

    engine.write(&[node("doc", "N2", r#"{"summary":"still embedding"}"#)]).expect("N2");
    engine.drain(30_000).expect("drain");
    assert_eq!(
        calls_since(&calls, after_open),
        1,
        "the dense arm must still embed after the reopen"
    );
    let conn = ro(&path);
    assert!(vector_row_exists(&conn, active_cursor(&conn, "N2")), "…with a real vector at rest");

    opened.engine.close().unwrap();
}

// ===========================================================================
// (4) HEALTHY NO-OP — a real `searchable→vector` registry
// ===========================================================================

/// Condition 3. A registry that DOES declare a `searchable→vector` projection is
/// the ordinary healthy shape: reopening must change nothing and the dense arm
/// must keep embedding. Without this the suite would pass on a reconciliation
/// that simply emptied `_fathomdb_vector_kinds` at every open.
#[test]
fn a_healthy_searchable_vector_registry_is_untouched_on_reopen() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc71_fix1_healthy");

    let c1 = {
        let embedder = CountingEmbedder::new();
        let calls = Arc::clone(&embedder.calls);
        let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
        let engine = &opened.engine;
        engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("write N1");
        engine
            .configure_projections(&[searchable_vector_spec("summary")], &[])
            .expect("declare a real dense arm");
        engine.drain(30_000).expect("drain");
        let conn = ro(&path);
        let c1 = active_cursor(&conn, "N1");
        assert!(vector_kind_registered(&conn, "doc"), "fixture: the declaration enrolled `doc`");
        assert!(vector_row_exists(&conn, c1), "fixture: N1 is embedded");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "fixture: one embed");
        opened.engine.close().unwrap();
        c1
    };

    let conn = ro(&path);
    let before = enrolled_kinds(&conn);
    drop(conn);

    let embedder = CountingEmbedder::new();
    let calls = Arc::clone(&embedder.calls);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("reopen");
    let engine = &opened.engine;
    let after_open = calls.load(Ordering::SeqCst);

    let conn = ro(&path);
    assert_eq!(
        enrolled_kinds(&conn),
        before,
        "CONDITION 3: a live `searchable→vector` declaration means the dense arm is WANTED — \
         reopening must not un-enrol anything"
    );
    assert!(vector_row_exists(&conn, c1), "…and nothing at rest is disturbed");
    drop(conn);

    engine.write(&[node("doc", "N2", r#"{"summary":"still embedding"}"#)]).expect("N2");
    engine.drain(30_000).expect("drain");
    assert_eq!(calls_since(&calls, after_open), 1, "the healthy dense arm keeps embedding");
    let conn = ro(&path);
    assert!(vector_row_exists(&conn, active_cursor(&conn, "N2")), "…with a real vector at rest");

    opened.engine.close().unwrap();
}

// ===========================================================================
// (5) `'edge_fact'` is NEVER un-enrolled — and (6) idempotence
// ===========================================================================

/// `project_canonical_edge_row` (G11) auto-registers `'edge_fact'` off the
/// presence of an edge BODY, unconditionally and independently of the projection
/// registry. That lifecycle is not the registry's to end, so the reconciliation
/// must exclude it — exactly as `unenrol_registry_vector_node_kinds` already
/// does for the drop inverse.
///
/// The same test carries IDEMPOTENCE (post-condition 6): after the reconciling
/// reopen, two further reopens must change nothing at all.
#[test]
fn edge_fact_survives_the_reconciliation_which_is_idempotent_across_reopens() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc71_fix1_edge_fact_idempotent");

    let c1 = {
        let embedder = CountingEmbedder::new();
        let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
        let engine = &opened.engine;
        declare_legacy_filterable_vector(engine, &path, "summary");
        engine.configure_vector_kind_for_test("doc").expect("enrol as the old code did");
        engine.configure_vector_kind_for_test("note").expect("a second affected node kind");
        engine.configure_vector_kind_for_test("edge_fact").expect("the G11 edge enrolment");
        engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("write N1");
        engine.drain(30_000).expect("drain");
        let conn = ro(&path);
        let c1 = active_cursor(&conn, "N1");
        assert_eq!(
            enrolled_kinds(&conn),
            vec!["doc".to_string(), "edge_fact".to_string(), "note".to_string()],
            "fixture: three enrolled kinds, two of them node kinds"
        );
        opened.engine.close().unwrap();
        c1
    };

    // ---- reopen 1: reconciles ----
    let rows_after_reconcile = {
        let embedder = CountingEmbedder::new();
        let opened =
            Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("reopen 1");
        let conn = ro(&path);
        assert_eq!(
            enrolled_kinds(&conn),
            vec!["edge_fact".to_string()],
            "the reconciliation un-enrols EVERY affected node kind and keeps `edge_fact`: G11 \
             auto-registers it off the presence of an edge body, independently of the projection \
             registry, so a node-projection reconciliation must not take the edge dense arm down"
        );
        assert!(vector_row_exists(&conn, c1), "…and deletes no embedding");
        let rows = vector_row_count(&conn);
        drop(conn);
        opened.engine.close().unwrap();
        rows
    };

    // ---- reopens 2 and 3: nothing further changes ----
    for round in 2..=3 {
        let embedder = CountingEmbedder::new();
        let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder))
            .unwrap_or_else(|e| panic!("reopen {round}: {e:?}"));
        let conn = ro(&path);
        assert_eq!(
            enrolled_kinds(&conn),
            vec!["edge_fact".to_string()],
            "IDEMPOTENCE: reopen {round} must converge, not oscillate or widen"
        );
        assert!(vector_row_exists(&conn, c1), "reopen {round}: still deletes no embedding");
        assert_eq!(
            vector_row_count(&conn),
            rows_after_reconcile,
            "IDEMPOTENCE: reopen {round} must neither delete nor create an embedding"
        );
        drop(conn);
        opened.engine.close().unwrap();
    }
}

// ===========================================================================
// (7) `configure_projections` reconciles too — not only the `before && !after` edge
// ===========================================================================

/// The un-enrol inverse in `apply_projection_config` is keyed to the
/// `declared_before && !declared_after` TRANSITION, so an affected database whose
/// user calls `configure_projections` again — with anything at all — still read
/// `before == false` and got no relief. Reconciling on that path too closes the
/// same gap for a long-lived process that never reopens.
///
/// The call here is an IDEMPOTENT re-apply of the very same inert spec, which is
/// the weakest possible trigger: `delta.unchanged` is `true`, nothing about the
/// registry moves, and the transition arm is provably inert.
#[test]
fn a_configure_projections_call_reconciles_an_already_enrolled_inert_kind() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc71_fix1_apply_reconcile");
    let embedder = CountingEmbedder::new();
    let calls = Arc::clone(&embedder.calls);
    let delay_ms = Arc::clone(&embedder.delay_ms);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    declare_legacy_filterable_vector(engine, &path, "summary");
    engine.configure_vector_kind_for_test("doc").expect("enrol as the old code did");
    engine.configure_vector_kind_for_test("edge_fact").expect("the G11 edge enrolment");

    let conn = ro(&path);
    assert!(vector_kind_registered(&conn, "doc"), "fixture: the affected state");
    drop(conn);

    // The weakest trigger there is. Before 0.8.20 Slice 23 this was an IDEMPOTENT
    // RE-APPLY of the inert spec; that spec is now rejected, so the trigger drops
    // to an EMPTY request — weaker still, and it makes the same point: the
    // transition arm is provably inert (`declared_before` is already `false`),
    // nothing about the registry moves, and the reconciliation must fire anyway.
    let delta = engine.configure_projections(&[], &[]).expect("empty request");
    assert!(delta.unchanged, "an empty request is still reported as a no-op to the caller");

    let conn = ro(&path);
    assert_eq!(
        enrolled_kinds(&conn),
        vec!["edge_fact".to_string()],
        "fix-1: `apply_projection_config` must reconcile an already-enrolled inert kind, not only \
         fire on the `declared_before && !declared_after` edge — otherwise a long-lived process \
         that calls `configure_projections` again gets no relief"
    );
    drop(conn);

    delay_ms.store(8_000, Ordering::SeqCst);
    engine.write(&[node("doc", "N1", r#"{"summary":"after the reconcile"}"#)]).expect("N1");
    engine.drain(2_000).expect("`drain` must not wait: the kind is un-enrolled");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "…and the write spends no embed call");

    // CONTROL: promoting to a real `searchable→vector` declaration in the SAME
    // session still turns the dense arm back on, so the reconciliation is not a
    // one-way kill switch.
    delay_ms.store(0, Ordering::SeqCst);
    engine
        .configure_projections(&[searchable_vector_spec("meaning")], &[])
        .expect("declare a real dense arm");
    engine.drain(30_000).expect("drain the real backfill");
    let conn = ro(&path);
    assert!(vector_kind_registered(&conn, "doc"), "CONTROL: a real declaration re-enrols");
    assert!(
        vector_row_exists(&conn, active_cursor(&conn, "N1")),
        "CONTROL: and backfills the row written while the arm was off"
    );
    assert!(calls.load(Ordering::SeqCst) > 0, "CONTROL: the embedder ran once promoted");

    opened.engine.close().unwrap();
}
