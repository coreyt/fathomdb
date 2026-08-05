//! 0.8.20 Slice 22 (`R-20-VC`, leg 1 / ledger **TC-67**) — **declaring a
//! `searchable→vector` projection over a kind the vector writer can never commit
//! must REPORT, not fall silent.**
//!
//! ## The silence these tests close
//!
//! Slice 20c fix-2 restricted vector enrolment to the kinds `resolve_source_type`
//! maps (`email, article, paper, meeting, note, todo, doc, edge_fact`), because
//! enrolling any other kind is a permanent liveness wedge — the scheduler picks
//! the row up, `commit_projection_outcomes` fails before recording a terminal,
//! and `drain` burns its whole timeout forever. That fix is CORRECT and is not
//! touched here. What it left behind is a hole in the *reporting*:
//!
//! ```text
//! for kind in kinds.iter().filter(|kind| kind_is_vector_committable(kind)) {
//!     register_vector_kind(tx, kind)?;
//! }
//! ```
//!
//! A kind outside the vocabulary is dropped by that filter with **no enrolment,
//! no error, and nothing recorded**. Meanwhile the registry row IS persisted and
//! the projection's name IS pushed onto `ProjectionDelta::deferred`. So the
//! caller is told "deferred" and **cannot distinguish**:
//!
//! - *transient* — waiting on the embedder (Q6a graceful-absent, or an embed
//!   still in flight); it will get vectors later; from
//! - *permanent* — this kind is outside the locked Pack-1 partition-key
//!   vocabulary and will **never** get a vector, in this or any future session.
//!
//! Those two states are reported with the SAME `deferred` entry, which is the
//! defect. Per the HITL ruling the remedy is option **(c) REPORT**: the
//! `resolve_source_type` vocabulary is **NOT grown** and the Pack-1 D3
//! partition-key lock (`dev/design/0.7.0-vector-quant-pack1.md`) is **NOT
//! touched** — a new, typed, additive field on the delta carries the fact.
//!
//! ## What this suite pins
//!
//! 1. the unsupported kinds ARE reported, by kind, sorted and de-duplicated
//!    ([`an_uncommittable_kind_is_reported_not_silently_dropped`]);
//! 2. **readiness semantics are UNCHANGED** — an un-enrolled kind is not
//!    outstanding work, so `dense_readiness` still reads `ready` and `drain`
//!    still returns promptly. The report is additive INFORMATION, never a
//!    readiness change and never an error
//!    ([`readiness_semantics_are_unchanged_by_the_report`]);
//! 3. the report is EMPTY, not absent, when there is nothing to report
//!    ([`a_corpus_of_only_supported_kinds_reports_an_empty_list_not_absent`]);
//! 4. the report is INDEPENDENT of whether this session has an embedder, so a
//!    no-embedder session is not told a kind is permanently unsupported merely
//!    because nothing can embed right now — and conversely the permanent fact is
//!    still reported there
//!    ([`the_report_is_the_same_without_an_embedder_and_is_not_the_deferral`]);
//! 5. it is a STATE report, not a diff — an idempotent re-apply still carries it
//!    ([`the_report_is_state_not_diff_so_an_idempotent_reapply_still_carries_it`]),
//!    which is also the documented way to refresh the declare-time residual;
//! 6. it is scoped to the dense arm — no `searchable→vector` declaration, no
//!    report ([`no_vector_declaration_means_no_report`]);
//! 7. it does not break the shipped `read.projections` → `configure_projections`
//!    round-trip ([`the_read_configure_round_trip_still_holds`]).
//!
//! `"invoice"` is the established non-committable fixture kind
//! (`slice20c_flush_barrier` Leg D); `"entity"` is added because it is the
//! concrete consumer case — Memex's entity kinds sit outside the locked
//! vocabulary, and TC-67 exists so that consumer learns those rows get FTS and
//! lexical search but will never get vectors.

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    DenseReadiness, Engine, InitialState, PreparedWrite, ProjectionRole, ProjectionSpec,
    ProjectionVector, SourceId,
};
use fathomdb_schema::{SCHEMA_VERSION, SQLITE_SUFFIX};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers (the `slice20c_flush_barrier` / `slice21c_vector_role_gate` shapes;
// kept local so this suite's oracles cannot drift with another file's edits)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct CountingEmbedder {
    identity: EmbedderIdentity,
    calls: Arc<AtomicUsize>,
}

impl CountingEmbedder {
    fn new() -> Self {
        Self {
            identity: EmbedderIdentity::new("deterministic", "rev-a", 384),
            calls: Arc::new(AtomicUsize::new(0)),
        }
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

/// The real dense-arm declaration — `searchable` + a `vector` sub-object. Only
/// this shape puts anything on the dense arm (TC-71), so only this shape can
/// have unsupported kinds to report.
fn vector_spec(name: &str) -> ProjectionSpec {
    ProjectionSpec {
        name: name.to_string(),
        roles: roles(&[ProjectionRole::Searchable]),
        fts: None,
        vector: Some(ProjectionVector { embedder: None, dense_readiness: None }),
        source: None,
    }
}

/// A declaration with NO dense arm — the control for scoping (case 6).
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

fn vector_row_exists(conn: &rusqlite::Connection, cursor: i64) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM _fathomdb_vector_rows WHERE write_cursor = ?1",
        [cursor],
        |r| r.get::<_, i64>(0),
    )
    .expect("vector row probe")
        > 0
}

fn vector_kind_registered(conn: &rusqlite::Connection, kind: &str) -> bool {
    conn.query_row("SELECT COUNT(*) FROM _fathomdb_vector_kinds WHERE kind = ?1", [kind], |r| {
        r.get::<_, i64>(0)
    })
    .expect("vector kind probe")
        > 0
}

/// A non-commit-able kind is not a FAILURE — it must not pollute the audit.
fn projection_failure_rows(conn: &rusqlite::Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM operational_mutations
         WHERE collection_name = 'projection_failures'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
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

fn owned(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// The mixed corpus every case below builds on: ONE commit-able kind (`doc`,
/// coerced to `article` by `resolve_source_type`) and TWO that are permanently
/// outside the locked vocabulary. Two of them, written in non-alphabetical
/// order, so "sorted and de-duplicated" is a falsifiable claim rather than an
/// accident of insertion order. Each unsupported kind is written TWICE so a
/// per-ROW implementation would show a duplicate.
fn write_mixed_corpus(engine: &Engine) {
    engine.write(&[node("invoice", "I1", r#"{"summary":"payable in 30 days"}"#)]).expect("I1");
    engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("N1");
    engine.write(&[node("entity", "E1", r#"{"summary":"Alice, a person"}"#)]).expect("E1");
    engine.write(&[node("entity", "E2", r#"{"summary":"Bob, a person"}"#)]).expect("E2");
    engine.write(&[node("invoice", "I2", r#"{"summary":"paid"}"#)]).expect("I2");
}

// ===========================================================================
// 1. The report itself
// ===========================================================================

/// **THE defect.** A `searchable→vector` declaration over a corpus that contains
/// kinds the vector writer can never commit must name them.
///
/// Post-conditions (1 fails at the TC-67 baseline; the rest are the shipped
/// behaviour this must not disturb):
///   1. `vector_unsupported_kinds` names exactly `entity` and `invoice`, sorted
///      and de-duplicated, and does NOT name the commit-able `doc`;
///   2. the commit-able kind still gets its full dense arm;
///   3. the unsupported kinds are still NOT enrolled and carry no vector (the
///      Slice 20c fix-2 liveness restriction is untouched — TC-67 changes what
///      the engine SAYS, not what it does);
///   4. it is not an error and not a failure-audit entry.
#[test]
fn an_uncommittable_kind_is_reported_not_silently_dropped() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc67_report");
    let embedder = CountingEmbedder::new();
    let calls = Arc::clone(&embedder.calls);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    write_mixed_corpus(engine);
    engine.drain(30_000).expect("baseline drain");

    // (4) …and it is not an error.
    let delta = engine.configure_projections(&[vector_spec("summary")], &[]).expect(
        "declaring a vector projection over unsupported kinds is legal — REPORT, not error",
    );

    // (1) THE assertion.
    assert_eq!(
        delta.vector_unsupported_kinds,
        owned(&["entity", "invoice"]),
        "TC-67: the kinds `resolve_source_type` cannot map are dropped by \
         `enqueue_declared_vector_backfill`'s `kind_is_vector_committable` filter with no \
         enrolment, no error and nothing recorded. They must be REPORTED — by KIND, sorted and \
         de-duplicated — so a caller can tell `deferred`-because-transient from \
         will-never-be-embedded"
    );
    assert!(
        !delta.vector_unsupported_kinds.contains(&"doc".to_string()),
        "the report must not name a kind the vector writer CAN commit — that would be a false \
         permanent-unsupported claim about a kind that is about to be embedded"
    );
    // `deferred` is the field that CANNOT tell the two apart. Pinning it here
    // states the defect as a fact: the two axes are reported separately.
    assert_eq!(
        delta.deferred,
        owned(&["summary"]),
        "the `searchable→vector` sub-target still defers by name — unchanged by TC-67; the report \
         is a SEPARATE axis, not a replacement"
    );

    engine.drain(30_000).expect("drain the declared backfill");

    let conn = ro(&path);
    // (2) the commit-able kind is unaffected.
    assert!(vector_kind_registered(&conn, "doc"), "the commit-able kind still gets its dense arm");
    assert!(vector_row_exists(&conn, active_cursor(&conn, "N1")), "…and its vector is at rest");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "exactly the one commit-able row was embedded");

    // (3) the reported kinds are still un-enrolled — the report does NOT enrol.
    for (kind, lid) in [("invoice", "I1"), ("entity", "E1")] {
        assert!(
            !vector_kind_registered(&conn, kind),
            "TC-67 REPORTS the exclusion; it must not lift it. Enrolling `{kind}` would wedge the \
             projection worker forever (Slice 20c fix-2)"
        );
        assert!(!vector_row_exists(&conn, active_cursor(&conn, lid)), "…and no vector at rest");
    }
    // (4) not a failure either.
    assert_eq!(
        projection_failure_rows(&conn),
        0,
        "a kind with no dense arm is not a FAILURE — reporting it must not start polluting the \
         failure audit"
    );

    // The report is additive READ metadata; nothing about it needs a schema step.
    assert_eq!(SCHEMA_VERSION, 25, "Slice 45 adds the nested-source registry migration");

    opened.engine.close().unwrap();
}

// ===========================================================================
// 2. Readiness semantics are UNCHANGED — the DoD clause most likely to regress
// ===========================================================================

/// **The DoD's second clause, asserted on its own.** An un-enrolled kind is *not
/// outstanding work*: nothing will ever be embedded for it, so there is nothing
/// to wait for. `dense_readiness` must therefore still read `ready`, and `drain`
/// must still return promptly. The report is additive INFORMATION — it is
/// deliberately NOT a readiness state, NOT an error, and NOT a reason to hold the
/// corpus in `embedding`.
///
/// This is separated from case 1 because the tempting "fix" — treating an
/// unsupported kind as unfinished work — would pass a report-only assertion while
/// wedging every consumer whose corpus contains one.
#[test]
fn readiness_semantics_are_unchanged_by_the_report() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc67_readiness_unchanged");
    let embedder = CountingEmbedder::new();
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    // A corpus of ONLY unsupported kinds — the harshest case for readiness: the
    // dense arm is declared and NOTHING in the corpus can ever feed it.
    engine.write(&[node("invoice", "I1", r#"{"summary":"payable in 30 days"}"#)]).expect("I1");
    engine.write(&[node("entity", "E1", r#"{"summary":"Alice, a person"}"#)]).expect("E1");
    engine.drain(30_000).expect("baseline drain");

    let delta = engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");
    assert_eq!(
        delta.vector_unsupported_kinds,
        owned(&["entity", "invoice"]),
        "fixture: both kinds are reported"
    );

    // Nothing was enqueued, so the barrier returns without waiting on anything.
    engine.drain(30_000).expect(
        "`drain` must not wait on kinds that will never be embedded — the report is information, \
         not outstanding work",
    );
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Ready),
        "READINESS SEMANTICS ARE UNCHANGED (plan §3 R-20-VC DoD): an un-enrolled kind is not \
         outstanding work, so a corpus made ENTIRELY of unsupported kinds is `ready`, not \
         `embedding`. Reporting the kinds must not turn the report into a readiness state"
    );

    // And it stays `ready` across a re-apply and a further write of an
    // unsupported kind — the report never becomes sticky work.
    engine.write(&[node("entity", "E2", r#"{"summary":"Bob, a person"}"#)]).expect("E2");
    engine.drain(30_000).expect("drain after a further unsupported write");
    assert_eq!(
        readiness(engine, "summary"),
        Some(DenseReadiness::Ready),
        "writing MORE rows of an unsupported kind still leaves nothing outstanding"
    );

    let conn = ro(&path);
    assert_eq!(projection_failure_rows(&conn), 0, "…and still no failure-audit noise");

    opened.engine.close().unwrap();
}

// ===========================================================================
// 3. Empty, not absent
// ===========================================================================

/// A corpus of only supported kinds reports an EMPTY list. Stated as an explicit
/// `assert_eq!` against `Vec::<String>::new()` rather than `is_empty()`, because
/// "empty, not absent" is the round-trippability requirement: a caller must be
/// able to read the field unconditionally.
#[test]
fn a_corpus_of_only_supported_kinds_reports_an_empty_list_not_absent() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc67_empty_report");
    let embedder = CountingEmbedder::new();
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("N1");
    engine.write(&[node("note", "N2", r#"{"summary":"a second supported kind"}"#)]).expect("N2");
    engine.drain(30_000).expect("baseline drain");

    let delta = engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");
    assert_eq!(
        delta.vector_unsupported_kinds,
        Vec::<String>::new(),
        "with every kind commit-able the report is EMPTY — present and readable, never absent"
    );
    assert_eq!(delta.deferred, owned(&["summary"]), "the vector sub-target still defers by name");

    // An EMPTY corpus likewise: no rows, no kinds, empty report — not an error.
    let dir2 = TempDir::new().unwrap();
    let path2 = db_path(&dir2, "tc67_empty_corpus");
    let opened2 = Engine::open_with_embedder_for_test(&path2, Arc::new(CountingEmbedder::new()))
        .expect("open");
    let delta2 =
        opened2.engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");
    assert_eq!(
        delta2.vector_unsupported_kinds,
        Vec::<String>::new(),
        "an empty corpus has no unsupported kinds — empty report, no error"
    );

    engine.drain(30_000).expect("drain");
    opened.engine.close().unwrap();
    opened2.engine.close().unwrap();
}

// ===========================================================================
// 4. Independent of the embedder — do NOT conflate the two absences
// ===========================================================================

/// `enqueue_declared_vector_backfill` only runs `if dense_arm_live` (the Q6a
/// graceful-absent path). The report must NOT be computed inside that gate:
///
/// - **`deferred`** answers "is there a live embedder / outstanding work?" — a
///   TRANSIENT, per-session fact. It is reported identically in both sessions,
///   which is precisely why it cannot carry TC-67's information.
/// - **`vector_unsupported_kinds`** answers "can the vector writer EVER commit
///   this kind?" — a STATIC property of `resolve_source_type`'s locked
///   vocabulary, true in every session, embedder or not.
///
/// So a no-embedder session must report the SAME kinds (nothing about them is
/// session-dependent) and must NOT sweep the commit-able kinds in with them
/// merely because nothing can embed right now.
#[test]
fn the_report_is_the_same_without_an_embedder_and_is_not_the_deferral() {
    let dir = TempDir::new().unwrap();

    // (a) no embedder at all — `dense_arm_live == false`.
    let path_absent = db_path(&dir, "tc67_no_embedder");
    let absent = Engine::open(path_absent.clone()).expect("open without an embedder");
    write_mixed_corpus(&absent.engine);
    absent.engine.drain(30_000).expect("baseline drain");
    let delta_absent =
        absent.engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");

    // (b) the same corpus with a live embedder.
    let path_live = db_path(&dir, "tc67_live_embedder");
    let live = Engine::open_with_embedder_for_test(&path_live, Arc::new(CountingEmbedder::new()))
        .expect("open with an embedder");
    write_mixed_corpus(&live.engine);
    live.engine.drain(30_000).expect("baseline drain");
    let delta_live =
        live.engine.configure_projections(&[vector_spec("summary")], &[]).expect("configure");

    assert_eq!(
        delta_absent.vector_unsupported_kinds,
        owned(&["entity", "invoice"]),
        "TC-67: a session with NO embedder must still report the permanent fact. \
         `resolve_source_type`'s vocabulary is static, so `entity`/`invoice` will never be \
         embedded by any embedder in any session — computing the report inside the \
         `dense_arm_live` gate would hide exactly the information a graceful-absent caller needs"
    );
    assert_eq!(
        delta_absent.vector_unsupported_kinds, delta_live.vector_unsupported_kinds,
        "the report is embedder-INDEPENDENT: identical with and without a live embedder"
    );
    assert!(
        !delta_absent.vector_unsupported_kinds.contains(&"doc".to_string()),
        "and it must NOT claim the commit-able `doc` is permanently unsupported just because this \
         session cannot embed anything — that is the deferral, a different (transient) fact"
    );
    // The two axes, side by side: `deferred` is IDENTICAL across the two
    // sessions, so it alone can never distinguish them. That is the defect,
    // pinned as an executable fact.
    assert_eq!(delta_absent.deferred, delta_live.deferred);
    assert_eq!(delta_absent.deferred, owned(&["summary"]));

    absent.engine.close().unwrap();
    live.engine.drain(30_000).expect("drain");
    live.engine.close().unwrap();
}

// ===========================================================================
// 5. A STATE report, not a diff — and the residual's refresh path
// ===========================================================================

/// The other three vectors describe what THIS call changed. This one describes
/// the corpus as it stands, so an idempotent re-apply — `unchanged == true`,
/// `built`/`dropped`/`deferred` all empty — must still carry it.
///
/// That is not a nicety: the report is computed at DECLARE time from the kinds
/// already in `canonical_nodes`, so a non-committable kind written LATER does not
/// appear in a report the caller already holds (the documented declare-time
/// residual, `dev/design/0.8.20-tc67-unsupported-vector-kind-report.md` §5). The
/// refresh path is exactly this re-apply — a no-op that returns a current report
/// — which only works because the report is state-keyed rather than diff-keyed.
#[test]
fn the_report_is_state_not_diff_so_an_idempotent_reapply_still_carries_it() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc67_state_not_diff");
    let embedder = CountingEmbedder::new();
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("N1");
    engine.write(&[node("invoice", "I1", r#"{"summary":"payable in 30 days"}"#)]).expect("I1");
    engine.drain(30_000).expect("baseline drain");

    let spec = vector_spec("summary");
    let first =
        engine.configure_projections(std::slice::from_ref(&spec), &[]).expect("first declaration");
    assert_eq!(first.vector_unsupported_kinds, owned(&["invoice"]), "fixture: reported once");
    engine.drain(30_000).expect("drain the backfill");

    // Idempotent re-apply: a genuine no-op on every diff axis…
    let again = engine
        .configure_projections(std::slice::from_ref(&spec), &[])
        .expect("idempotent re-apply");
    assert!(again.unchanged, "re-registering the same spec is still a no-op (Slice 15d keystone)");
    assert!(again.built.is_empty() && again.dropped.is_empty() && again.deferred.is_empty());
    // …and the report is STILL there.
    assert_eq!(
        again.vector_unsupported_kinds,
        owned(&["invoice"]),
        "TC-67: the report is a STATE report, not a diff. A no-op re-apply must still carry it — \
         that is the documented refresh path for the declare-time residual, and it must not be \
         suppressed by `unchanged`"
    );

    // The residual, made concrete: a kind written AFTER the declaration is not in
    // the report the caller already holds, and a re-apply surfaces it.
    engine.write(&[node("entity", "E1", r#"{"summary":"Alice, a person"}"#)]).expect("E1");
    engine.drain(30_000).expect("drain the late write");
    assert_eq!(
        first.vector_unsupported_kinds,
        owned(&["invoice"]),
        "RESIDUAL: the delta the caller already holds is a snapshot; it does not learn about \
         `entity`, written after the declaration"
    );
    let refreshed =
        engine.configure_projections(std::slice::from_ref(&spec), &[]).expect("refresh re-apply");
    assert!(refreshed.unchanged, "the refresh costs nothing — it is still a no-op");
    assert_eq!(
        refreshed.vector_unsupported_kinds,
        owned(&["entity", "invoice"]),
        "…and the no-op re-apply is the refresh: it reports the corpus as it stands NOW"
    );

    opened.engine.close().unwrap();
}

// ===========================================================================
// 6. Scoped to the dense arm
// ===========================================================================

/// No `searchable→vector` declaration ⇒ nothing on the dense arm ⇒ nothing to be
/// unsupported FOR, so the report stays empty. Two arms: a declaration that never
/// had a dense arm, and one whose dense arm has just been dropped.
#[test]
fn no_vector_declaration_means_no_report() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc67_scoped_to_dense_arm");
    let embedder = CountingEmbedder::new();
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    write_mixed_corpus(engine);
    engine.drain(30_000).expect("baseline drain");

    // (a) a `filterable`-only declaration over the very same corpus.
    let delta = engine
        .configure_projections(&[filterable_only_spec("summary")], &[])
        .expect("filterable-only configure");
    assert_eq!(
        delta.vector_unsupported_kinds,
        Vec::<String>::new(),
        "no dense arm is declared, so there is nothing for an unsupported kind to be unsupported \
         FOR — reporting here would be noise on every non-vector call"
    );

    // (b) declare the dense arm (report populated), then drop it (report empty).
    let declared = engine
        .configure_projections(&[vector_spec("meaning")], &[])
        .expect("declare the dense arm");
    assert_eq!(
        declared.vector_unsupported_kinds,
        owned(&["entity", "invoice"]),
        "fixture: with a dense arm declared the report is populated"
    );
    engine.drain(30_000).expect("drain");
    let dropped =
        engine.configure_projections(&[], &["meaning".to_string()]).expect("drop the dense arm");
    assert!(dropped.dropped.contains(&"meaning".to_string()), "fixture: the drop is reported");
    assert_eq!(
        dropped.vector_unsupported_kinds,
        Vec::<String>::new(),
        "once the last `searchable→vector` declaration is gone the report goes quiet with it"
    );

    opened.engine.close().unwrap();
}

// ===========================================================================
// 7. The shipped read → configure round-trip
// ===========================================================================

/// `read.projections` output must feed straight back into
/// `configure_projections` — the twice-tested contract that made a hard-reject of
/// caller-supplied `dense_readiness` unacceptable in Slice 20
/// (`STATUS-0.8.20.md` §14.1). TC-67 must not break it.
///
/// It cannot, structurally: the new field lives on the DELTA, and
/// `configure_projections` accepts `&[ProjectionSpec]` — there is no inbound
/// direction for it at all. It is OUTPUT-ONLY, and this test states that
/// explicitly rather than leaving it to inference.
#[test]
fn the_read_configure_round_trip_still_holds() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc67_round_trip");
    let embedder = CountingEmbedder::new();
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    write_mixed_corpus(engine);
    engine.drain(30_000).expect("baseline drain");
    engine.configure_projections(&[vector_spec("summary")], &[]).expect("declare");
    engine.drain(30_000).expect("drain the backfill");

    // Read back — including the engine-set `dense_readiness` — and re-apply verbatim.
    let back = engine.read_projections().expect("read_projections");
    assert_eq!(back.len(), 1);
    assert_eq!(
        back[0].vector.as_ref().and_then(|v| v.dense_readiness),
        Some(DenseReadiness::Ready),
        "readiness is `ready` — unchanged by TC-67, even with unsupported kinds present"
    );

    let round_tripped =
        engine.configure_projections(&back, &[]).expect("read.projections output re-applies");
    assert!(
        round_tripped.unchanged,
        "the shipped read→configure round-trip is still a no-op — TC-67 adds an OUTPUT-only field \
         to the delta, and `configure_projections` takes specs, never a delta, so there is no \
         inbound direction it could break"
    );
    assert_eq!(
        round_tripped.vector_unsupported_kinds,
        owned(&["entity", "invoice"]),
        "…and the no-op still carries the report"
    );

    opened.engine.close().unwrap();
}
