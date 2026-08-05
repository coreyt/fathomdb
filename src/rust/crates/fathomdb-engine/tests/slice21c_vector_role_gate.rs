//! 0.8.20 Slice 21c (`R-20-CR`, leg 3 / ledger **TC-71**) — **the dense arm is
//! gated on the `searchable` ROLE, not on the bare `vector` sub-object.**
//!
//! ## The defect these tests close
//!
//! `vector_projection_declared` — the corpus-wide "is the dense arm declared?"
//! predicate that gates all three enrolment paths — answered
//! `EXISTS(… WHERE vector_declared = 1)` and never read the `roles` column. So a
//! projection declared `{roles: [filterable], vector: {}}` — a combination the
//! shipped Slice-15d contract documents as **inert but round-trippable**
//! (`vector_subobject_is_stored_not_built`) — turned the dense arm ON in any
//! session with a live embedder: it enrolled the node kinds, backfilled the
//! corpus, and made every later write of those kinds enqueue an embedding.
//!
//! **Severity.** This is wasted embed work plus unexpected vectors at rest for a
//! projection meant to be inert. It is **not** a false-ready and **not** data
//! loss — no reader is wrong, and nothing is deleted. It is pinned here because
//! "declaring `filterable` silently starts embedding your whole corpus" is a
//! surprising, billable side effect of a declaration documented as doing nothing.
//!
//! ## Why the shipped suites do not catch it
//!
//! `slice15d_projection_registry::vector_subobject_is_stored_not_built` opens
//! with plain `Engine::open` — **no embedder** — so every dense-arm path it
//! could have tripped is short-circuited by the `runtime_embedder.is_none()`
//! precondition; it passes **vacuously**. It also declares
//! `{searchable, vector}`, which is legitimately vector-declared, so it never
//! even exercises the offending shape. The only shipped test of the offending
//! shape is TypeScript (`src/ts/tests/slice15d-projection-registry.test.ts`,
//! "vector sub-object round-trips"), and it likewise opens without an embedder
//! and asserts round-trip fidelity only, never at-rest inertness.
//!
//! Every test below therefore opens with a **live** [`CountingEmbedder`] and
//! asserts on RAW TABLES (`_fathomdb_vector_kinds`, `_fathomdb_vector_rows`,
//! `vector_default`) plus the embedder's call count. A returned
//! [`fathomdb_engine::ProjectionDelta`] is NOT a falsifying oracle here: the
//! delta is identical before and after the fix (that is the point of the
//! round-trip pin below).
//!
//! ## The three gated paths, and the fourth pin
//!
//! `vector_projection_declared` has exactly four call sites, covering three
//! paths; each has a test here:
//!
//! - **(a) forward backfill** — `enqueue_declared_vector_backfill`'s entry guard
//!   and `apply_projection_config`'s `vector_declared_after` fork:
//!   [`a_filterable_vector_declaration_backfills_nothing_and_embeds_nothing`].
//! - **(b) drop inverse** — the `vector_declared_before` snapshot, which decides
//!   whether `unenrol_registry_vector_node_kinds` fires. Making the predicate
//!   role-aware flips the forward AND inverse arms at once, so the demotion
//!   `{searchable, vector}` → `{filterable, vector}` must now UN-ENROL:
//!   [`demoting_a_searchable_vector_projection_to_filterable_un_enrols_the_kind`]
//!   and [`dropping_the_last_searchable_vector_projection_un_enrols_past_an_inert_sibling`].
//! - **(c) late enrolment** — `Engine::vector_kind_needs_enrolment`, the write
//!   path's fourth guard:
//!   [`a_write_under_a_filterable_vector_declaration_enqueues_no_embedding`].
//!
//! And the boundary that stops the fix from over-reaching: the projection must
//! become INERT, not DISAPPEAR —
//! [`a_filterable_vector_declaration_round_trips_verbatim_without_an_embedder`]
//! plus the round-trip assertions inside path (a)'s test.
//!
//! ## 0.8.20 Slice 23 (`R-20-SV`) — the ROUTE changed; the coverage did not
//!
//! The HITL ruled on 2026-07-24 that this suite's shape is an INVALID SPEC, and
//! Slice 23 rejects it with `EngineError::WriteValidation`. The shape is
//! therefore no longer constructible through `configure_projections` — but it is
//! still at rest in every database that declared it while the engine accepted
//! it, which is the population TC-71 exists for. Each test below now builds that
//! state through the documented legacy back door
//! ([`declare_legacy_filterable_vector`]) and asserts the reject in place on the
//! way past. **Every oracle is unchanged**; nothing here was deleted or
//! weakened.

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
use std::time::Duration;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers (the `slice20c_flush_barrier` shapes; kept local so this suite is
// self-contained and its oracles cannot drift with another file's edits)
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

/// **The shape under test**: a `vector` sub-object WITHOUT the `searchable`
/// role. Documented inert-but-round-trippable; before TC-71 it turned the dense
/// arm on.
///
/// **0.8.20 Slice 23 (`R-20-SV`) — this spec is no longer ACCEPTED.** It is kept
/// so every test below can assert the reject in place (see
/// [`declare_legacy_filterable_vector`]).
fn filterable_vector_spec(name: &str) -> ProjectionSpec {
    ProjectionSpec {
        name: name.to_string(),
        roles: roles(&[ProjectionRole::Filterable]),
        fts: None,
        vector: Some(ProjectionVector { embedder: None, dense_readiness: None }),
        source: None,
    }
}

/// The VALID half of the shape under test — `filterable`, no sub-object. This
/// part `configure_projections` still accepts, and it is what builds the EAV
/// rows the "inert, NOT absent" assertions read.
fn filterable_only_spec(name: &str) -> ProjectionSpec {
    ProjectionSpec {
        name: name.to_string(),
        roles: roles(&[ProjectionRole::Filterable]),
        fts: None,
        vector: None,
        source: None,
    }
}

/// **0.8.20 Slice 23 (`R-20-SV`) — THE LEGACY BACK DOOR, and why this suite now
/// uses it.**
///
/// The HITL ruled on **2026-07-24** (`dev/plans/plan-0.8.20.md` §11 item 4,
/// option (b)) that an `fts`/`vector` sub-object declared WITHOUT the
/// `searchable` role is an **INVALID SPEC**; Slice 23 implements that reject as
/// `EngineError::WriteValidation` (pinned in
/// `tests/slice23_spec_validation_reject.rs`). So the shape TC-71 is about is no
/// longer constructible through the public verb.
///
/// It has NOT stopped existing. It survives as the AT-REST state of every
/// database that declared it while the engine still accepted it — which is
/// precisely the affected population TC-71 was raised for, and precisely the
/// population `tc71_fix1_inert_enrolment_reconcile.rs` reconciles. Deleting this
/// coverage would retire the defect class while its population is still live, so
/// the coverage is KEPT and only the ROUTE moves: declare the valid `filterable`
/// half through the verb, then set `vector_declared = 1` with the raw UPDATE the
/// pre-Slice-23 `persist_projection_row` would have written for the same
/// declaration. **Every ORACLE below is unchanged** — raw
/// `_fathomdb_vector_kinds` / `_fathomdb_vector_rows` / `vector_default` rows
/// plus the embedder call count.
///
/// It also asserts the reject IN PLACE, so each test states why it takes the
/// back door and pins the front door shut at the same time.
fn declare_legacy_filterable_vector(
    engine: &Engine,
    path: &Path,
    name: &str,
) -> fathomdb_engine::ProjectionDelta {
    // Front door, pinned shut.
    assert_eq!(
        engine.configure_projections(&[filterable_vector_spec(name)], &[]).expect_err(
            "R-20-SV: a `vector` sub-object without `searchable` is now an invalid spec"
        ),
        fathomdb_engine::EngineError::WriteValidation,
    );
    // Valid half through the verb — this is what builds the EAV projection.
    let delta = engine
        .configure_projections(&[filterable_only_spec(name)], &[])
        .expect("the `filterable` half is still a valid declaration");
    legacy_add_vector_subobject(path, name);
    delta
}

/// Set `vector_declared = 1` on an EXISTING registry row through a raw RW
/// connection — the legacy half of [`declare_legacy_filterable_vector`], split
/// out for the tests that need it applied to a row declared some other way.
/// This is the exact column `persist_projection_row` set for the same
/// declaration before Slice 23 refused it.
fn legacy_add_vector_subobject(path: &Path, name: &str) {
    let conn = rusqlite::Connection::open(path).expect("open rw");
    let n = conn
        .execute(
            "UPDATE _fathomdb_projection_registry SET vector_declared = 1 WHERE name = ?1",
            [name],
        )
        .expect("legacy vector sub-object");
    assert_eq!(n, 1, "the registry row must exist before the legacy sub-object is added");
}

/// The legitimate dense-arm declaration — the control. Every assertion that the
/// inert shape does nothing is paired with this one still doing everything.
fn searchable_vector_spec(name: &str) -> ProjectionSpec {
    ProjectionSpec {
        name: name.to_string(),
        roles: roles(&[ProjectionRole::Searchable]),
        fts: None,
        vector: Some(ProjectionVector { embedder: None, dense_readiness: None }),
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

/// AT-REST oracle #2 — the vec0 row itself, so no assertion can pass off the
/// bookkeeping table alone.
fn vec0_row_exists(conn: &rusqlite::Connection, cursor: i64) -> bool {
    conn.query_row("SELECT COUNT(*) FROM vector_default WHERE rowid = ?1", [cursor], |r| {
        r.get::<_, i64>(0)
    })
    .unwrap_or(0)
        > 0
}

/// AT-REST oracle #3 — the enrolment itself. This is the row TC-71's predicate
/// decides whether to write.
fn vector_kind_registered(conn: &rusqlite::Connection, kind: &str) -> bool {
    conn.query_row("SELECT COUNT(*) FROM _fathomdb_vector_kinds WHERE kind = ?1", [kind], |r| {
        r.get::<_, i64>(0)
    })
    .expect("vector kind probe")
        > 0
}

/// The EAV values at rest for `attr_name` — the "inert, NOT absent" oracle. A
/// `filterable` projection must still store its value even though it embeds
/// nothing.
fn eav_values(conn: &rusqlite::Connection, attr_name: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT attr_value FROM canonical_attributes WHERE attr_name = ?1 ORDER BY attr_value",
        )
        .expect("prepare eav probe");
    let v: Vec<String> = stmt
        .query_map([attr_name], |r| r.get::<_, String>(0))
        .expect("eav query")
        .map(|r| r.expect("eav row"))
        .collect();
    v
}

// ===========================================================================
// (a) forward backfill — the declare-time door
// ===========================================================================

/// **Path (a).** Declaring `{roles: [filterable], vector: {}}` over an EXISTING
/// corpus, in a session with a LIVE embedder, must enrol nothing, backfill
/// nothing and embed nothing — while still persisting and reporting the
/// declaration exactly as the Slice-15d contract says.
///
/// Post-conditions (1-4 fail at the TC-71 baseline):
///   1. the node kind is NOT enrolled in `_fathomdb_vector_kinds`;
///   2. the embedder is never called;
///   3. nothing is at rest in `_fathomdb_vector_rows` / `vector_default`;
///   4. `drain` does not wait on it (embedder slowed to 8 s, a 2 s barrier still
///      returns `Ok`);
///   5. **INERT, NOT ABSENT** — the `filterable` value IS stored in the EAV
///      table, the delta still reports the name `built` (filterable wants EAV)
///      AND `deferred` (the `vector` sub-object defers), and
///      `read_projections` round-trips the declaration verbatim. This is the
///      boundary between fixing TC-71 and breaking the Slice-15d contract.
#[test]
fn a_filterable_vector_declaration_backfills_nothing_and_embeds_nothing() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc71_forward_backfill");
    let embedder = CountingEmbedder::new();
    let calls = Arc::clone(&embedder.calls);
    let delay_ms = Arc::clone(&embedder.delay_ms);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    // Corpus FIRST, declaration SECOND — the flow that makes the declare-time
    // backfill door (`enqueue_declared_vector_backfill`) run over real rows.
    engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("write N1");

    // Slow the embedder BEFORE the declaration, so post-4 is falsifiable: on the
    // broken code the declaration enqueues N1 and the 2 s drain below cannot
    // finish it.
    delay_ms.store(8_000, Ordering::SeqCst);

    let spec = filterable_vector_spec("summary");
    let delta = declare_legacy_filterable_vector(engine, &path, "summary");

    // (5a) INERT, NOT ABSENT — the `filterable` half still builds.
    assert_eq!(
        delta.built,
        vec!["summary".to_string()],
        "the `filterable` role still builds its EAV projection"
    );
    assert!(
        delta.deferred.is_empty(),
        "0.8.20 Slice 23 (R-20-SV): the declaration that reaches the verb no longer CARRIES the \
         `vector` sub-object — it is refused. The legacy half is added at rest, exactly as a \
         pre-Slice-23 database carries it, and is still REPORTED by `read_projections` below \
         (post-5b) — the round-trip contract this assertion originally protected"
    );
    assert!(!delta.unchanged, "a fresh declaration is not a no-op");

    // The governed call that runs the SAME declare-time fork the old
    // `{filterable, vector}` declaration ran (`vector_declared_after` +
    // `enqueue_declared_vector_backfill`'s entry guard), now with the legacy row
    // in place. This is the door path (a) is about.
    let after = engine
        .configure_projections(&[], &[])
        .expect("an empty request still runs the declare-time fork");
    assert!(after.unchanged, "…and diffs to a no-op");

    // (1) THE assertion, and it is checked FIRST: enrolment happens inside the
    // `configure_projections` write transaction, so it is settled and race-free
    // the instant that call returns — no barrier needed, no timing proxy.
    let conn = ro(&path);
    let c1 = active_cursor(&conn, "N1");
    assert!(
        !vector_kind_registered(&conn, "doc"),
        "TC-71: the dense arm must be gated on the `searchable` ROLE. A `vector` sub-object \
         without it must not enrol the node kind"
    );

    // (4) nothing was enqueued, so the barrier returns immediately.
    engine.drain(2_000).expect(
        "`drain` must not wait on a declaration with no `searchable` role — with the embedder at \
         8 s a 2 s barrier can only return Ok if nothing was enqueued",
    );

    // (2) the embedder was never asked for anything.
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "TC-71: `{{roles:[filterable], vector:true}}` is documented INERT, so declaring it must \
         not spend a single embed call"
    );

    let conn = ro(&path);
    // (3) and nothing at rest.
    assert!(
        !vector_row_exists(&conn, c1),
        "no `_fathomdb_vector_rows` row for an inert projection"
    );
    assert!(!vec0_row_exists(&conn, c1), "…and no `vector_default` row either");

    // (5b) INERT, NOT ABSENT — the value IS at rest, and the declaration round-trips.
    assert_eq!(
        eav_values(&conn, "summary"),
        vec!["a dense meaning".to_string()],
        "the `filterable` value must still be stored at rest — the fix makes the projection \
         inert, it must not make it disappear"
    );
    let back = engine.read_projections().expect("read_projections");
    assert_eq!(
        back,
        vec![ProjectionSpec {
            vector: Some(ProjectionVector {
                embedder: None,
                dense_readiness: Some(DenseReadiness::Ready),
            }),
            ..spec.clone()
        }],
        "the declaration round-trips verbatim (plus the engine-set readiness), exactly as the \
         Slice-15d `vector_subobject_is_stored_not_built` contract requires"
    );

    // The CONTROL: the same corpus, the same session, the `searchable` role — the
    // dense arm must still turn on. Without this the suite would pass on a fix
    // that simply disabled enrolment.
    delay_ms.store(0, Ordering::SeqCst);
    engine
        .configure_projections(&[searchable_vector_spec("meaning")], &[])
        .expect("declare a real `searchable→vector` projection");
    engine.drain(30_000).expect("drain the real backfill");
    assert!(calls.load(Ordering::SeqCst) > 0, "CONTROL: `searchable→vector` still embeds");
    let conn = ro(&path);
    assert!(
        vector_kind_registered(&conn, "doc"),
        "CONTROL: `searchable→vector` still enrols the node kind"
    );
    assert!(vector_row_exists(&conn, c1), "CONTROL: the corpus is still backfilled");
    assert!(vec0_row_exists(&conn, c1), "CONTROL: …with a real vec0 row");

    opened.engine.close().unwrap();
}

// ===========================================================================
// (c) late enrolment — the write-path door
// ===========================================================================

/// **Path (c).** `Engine::vector_kind_needs_enrolment` is the write path's
/// fourth guard, and it asks the same predicate. Declaring the inert shape on an
/// EMPTY corpus leaves the declare-time door with nothing to do, so a subsequent
/// write is the ONLY thing that can enrol the kind — which isolates this site.
///
/// Post-conditions (all fail at the TC-71 baseline):
///   1. the write does not enrol `doc`;
///   2. the embedder is never called;
///   3. the row carries no vector at rest;
///   4. `drain` does not wait on it.
#[test]
fn a_write_under_a_filterable_vector_declaration_enqueues_no_embedding() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc71_late_enrolment");
    let embedder = CountingEmbedder::new();
    let calls = Arc::clone(&embedder.calls);
    let delay_ms = Arc::clone(&embedder.delay_ms);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    // Declaration FIRST, on an EMPTY corpus: `enqueue_declared_vector_backfill`
    // finds no `canonical_nodes` rows, so it enrols nothing regardless of the
    // predicate. Whatever happens next is the WRITE path's doing alone.
    // (0.8.20 Slice 23 — via the legacy back door; see
    // [`declare_legacy_filterable_vector`].)
    declare_legacy_filterable_vector(engine, &path, "summary");
    let conn = ro(&path);
    assert!(
        !vector_kind_registered(&conn, "doc"),
        "fixture: an empty corpus enrols nothing at declare time, on either code path"
    );

    delay_ms.store(8_000, Ordering::SeqCst);
    engine
        .write(&[node("doc", "N1", r#"{"summary":"written after the declaration"}"#)])
        .expect("N1");

    // (1) THE assertion, checked FIRST: `enrol_batch_vector_kinds` runs its
    // `BEGIN IMMEDIATE`…`COMMIT` inside `write`, so the enrolment is settled and
    // race-free the instant that call returns — no barrier, no timing proxy.
    let conn = ro(&path);
    let c1 = active_cursor(&conn, "N1");
    assert!(
        !vector_kind_registered(&conn, "doc"),
        "TC-71 (late enrolment): the write-path door must require the `searchable` role too, or \
         the declare-time gate is trivially routed around by writing after declaring"
    );

    // (4) nothing enqueued ⇒ the barrier returns well inside the embed delay.
    engine.drain(2_000).expect(
        "`drain` must not wait on a write made under a declaration with no `searchable` role",
    );

    // (2)
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "TC-71 (late enrolment): a write under `{{roles:[filterable], vector:true}}` must not \
         enqueue an embedding"
    );
    let conn = ro(&path);
    // (3)
    assert!(!vector_row_exists(&conn, c1), "no vector row for a write under an inert projection");
    assert!(!vec0_row_exists(&conn, c1), "…and no vec0 row");

    // CONTROL: promote to `searchable` and the very same write path enrols. (A
    // role ADD is non-destructive, so no drop is needed.)
    delay_ms.store(0, Ordering::SeqCst);
    engine
        .configure_projections(
            &[ProjectionSpec {
                roles: roles(&[ProjectionRole::Filterable, ProjectionRole::Searchable]),
                ..filterable_vector_spec("summary")
            }],
            &[],
        )
        .expect("promote to filterable+searchable");
    engine.drain(30_000).expect("drain the promoted backfill");
    let conn = ro(&path);
    assert!(vector_kind_registered(&conn, "doc"), "CONTROL: adding `searchable` enrols the kind");
    assert!(vector_row_exists(&conn, c1), "CONTROL: and backfills the row written while inert");
    assert!(calls.load(Ordering::SeqCst) > 0, "CONTROL: the embedder ran once promoted");

    opened.engine.close().unwrap();
}

// ===========================================================================
// (b) drop inverse — `vector_declared_before`
// ===========================================================================

/// **Path (b), the transition the fix CREATES.** Making the predicate role-aware
/// flips the forward and inverse arms of `apply_projection_config` at once: for
/// `{searchable, vector}` → `{filterable, vector}` the "before" read is `true`
/// and the "after" read becomes `false`, so the drop inverse
/// (`unenrol_registry_vector_node_kinds`) now FIRES on a DEMOTION, not only on a
/// literal drop. That is a real semantic consequence and is pinned here.
///
/// (The demotion is expressed as drop-then-redeclare in ONE call because
/// REMOVING the `searchable` role is a destructive change — the documented
/// drop-then-rebuild-fresh pattern, drops applying first.)
///
/// Post-conditions (1 and 3-4 fail at the TC-71 baseline):
///   1. the demotion un-enrols the node kind;
///   2. it deletes NO embedding — the shipped non-destructive drop contract;
///   3. a subsequent write of that kind embeds nothing;
///   4. `drain` does not wait on it;
///   5. re-promoting re-enrols and backfills, so the demotion strands nothing.
#[test]
fn demoting_a_searchable_vector_projection_to_filterable_un_enrols_the_kind() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc71_demotion_inverse");
    let embedder = CountingEmbedder::new();
    let calls = Arc::clone(&embedder.calls);
    let delay_ms = Arc::clone(&embedder.delay_ms);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    // ---- fixture: a real dense arm, caught up ----
    engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("write N1");
    engine.configure_projections(&[searchable_vector_spec("summary")], &[]).expect("configure");
    engine.drain(30_000).expect("drain");
    let conn = ro(&path);
    let c1 = active_cursor(&conn, "N1");
    assert!(vector_kind_registered(&conn, "doc"), "fixture: the declaration enrolled `doc`");
    assert!(vector_row_exists(&conn, c1), "fixture: N1 is embedded");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "fixture: exactly one embed so far");

    // ---- the demotion: same name, `searchable` replaced by `filterable`, the
    //      `vector` sub-object retained ----
    //
    // 0.8.20 Slice 23 (`R-20-SV`) — the demotion's TARGET spec
    // (`{filterable} + vector`) is now an invalid spec, so the demotion runs in
    // the two steps a pre-Slice-23 database's registry ended up in: the
    // drop-then-redeclare of the VALID half through the verb — which is the call
    // that takes the `vector_declared_before && !after` TRANSITION arm this test
    // is about — followed by the legacy `vector_declared = 1` the old engine
    // would have persisted in the same call. Splitting them is STRICTER: the
    // surviving legacy sub-object must not resurrect the arm afterwards either,
    // which is asserted below.
    assert_eq!(
        engine
            .configure_projections(&[filterable_vector_spec("summary")], &["summary".to_string()])
            .expect_err("R-20-SV: the demoted shape is no longer a valid spec"),
        fathomdb_engine::EngineError::WriteValidation,
    );
    let delta = engine
        .configure_projections(&[filterable_only_spec("summary")], &["summary".to_string()])
        .expect("drop-then-redeclare is the documented path for a destructive role change");
    assert!(delta.dropped.contains(&"summary".to_string()), "the drop half is reported");
    assert!(delta.built.contains(&"summary".to_string()), "the fresh `filterable` half is built");
    legacy_add_vector_subobject(&path, "summary");
    // …and a LATER governed call must not let the surviving `vector_declared = 1`
    // row switch the arm back on.
    engine.configure_projections(&[], &[]).expect("a later governed call is a no-op");

    let conn = ro(&path);
    // (1) THE assertion.
    assert!(
        !vector_kind_registered(&conn, "doc"),
        "TC-71: demoting the LAST `searchable→vector` projection to `filterable`+`vector` must \
         un-enrol the node kind. Before the fix the surviving `vector_declared = 1` row made the \
         post-state read `declared`, so the inverse never fired and the write path kept embedding \
         for a projection that no longer puts anything on the dense arm"
    );
    // (2) the shipped non-destructive contract still holds.
    assert!(
        vector_row_exists(&conn, c1),
        "un-enrolment must delete NO embedding — the shipped drop arm leaves vectors at rest"
    );
    assert!(vec0_row_exists(&conn, c1), "…including the vec0 row");
    assert_eq!(
        eav_values(&conn, "summary"),
        vec!["a dense meaning".to_string()],
        "the demoted `filterable` projection still stores its value at rest"
    );

    // (3)+(4) writes under the demoted declaration embed nothing.
    delay_ms.store(8_000, Ordering::SeqCst);
    engine.write(&[node("doc", "N2", r#"{"summary":"written after the demotion"}"#)]).expect("N2");
    engine.drain(2_000).expect("`drain` must not wait on a demoted projection's writes");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "a write after the demotion must not be embedded — no `searchable` role, no dense arm"
    );
    let conn = ro(&path);
    let c2 = active_cursor(&conn, "N2");
    assert!(!vector_row_exists(&conn, c2), "N2 has no vector: nothing was ever enqueued for it");
    assert!(!vector_kind_registered(&conn, "doc"), "…and the kind stayed un-enrolled");

    // (5) re-promoting is reversible and strands nothing.
    delay_ms.store(0, Ordering::SeqCst);
    engine
        .configure_projections(&[searchable_vector_spec("summary")], &["summary".to_string()])
        .expect("re-promote");
    engine.drain(30_000).expect("drain the re-promoted backfill");
    let conn = ro(&path);
    assert!(vector_kind_registered(&conn, "doc"), "re-promoting re-enrols the kind");
    assert!(
        vector_row_exists(&conn, c2),
        "the row written while the arm was off is backfilled, not stranded"
    );

    opened.engine.close().unwrap();
}

/// **Path (b), the mixed registry.** The predicate is corpus-wide, so the drop
/// inverse only fires when the LAST dense-arm declaration goes away. Before
/// TC-71 an inert `{filterable, vector}` sibling kept `vector_declared = 1`
/// present in the registry and therefore MASKED the drop of the real
/// `searchable→vector` projection: the post-state read `declared`, the inverse
/// never ran, and the write path kept embedding with no searchable declaration
/// left anywhere.
///
/// Post-conditions 1, 3 and 4 fail at the TC-71 baseline.
#[test]
fn dropping_the_last_searchable_vector_projection_un_enrols_past_an_inert_sibling() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc71_masked_inverse");
    let embedder = CountingEmbedder::new();
    let calls = Arc::clone(&embedder.calls);
    let delay_ms = Arc::clone(&embedder.delay_ms);
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(embedder)).expect("open");
    let engine = &opened.engine;

    engine
        .write(&[node("doc", "N1", r#"{"summary":"a dense meaning","tag":"alpha"}"#)])
        .expect("write N1");
    // TWO projections: one real dense arm, one inert `vector` sub-object.
    // (0.8.20 Slice 23 — the inert sibling now only reaches the registry through
    // the legacy back door; the real dense arm is declared normally.)
    engine
        .configure_projections(
            &[searchable_vector_spec("summary"), filterable_only_spec("tag")],
            &[],
        )
        .expect("configure both");
    engine.drain(30_000).expect("drain");
    // The sibling's legacy `vector` sub-object, added at rest once the real arm
    // is settled. Deliberately AFTER the drain: a raw RW connection opened while
    // embed work is in flight perturbs the dispatcher's timing, and this suite's
    // embed-count oracles must not depend on that.
    legacy_add_vector_subobject(&path, "tag");
    let conn = ro(&path);
    let c1 = active_cursor(&conn, "N1");
    assert!(vector_kind_registered(&conn, "doc"), "fixture: the SEARCHABLE one enrolled `doc`");
    assert!(vector_row_exists(&conn, c1), "fixture: N1 is embedded");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "fixture: exactly one embed so far");

    // Drop ONLY the searchable one. The inert sibling survives.
    engine
        .configure_projections(&[], &["summary".to_string()])
        .expect("drop the real vector projection");
    let back = engine.read_projections().expect("read_projections");
    assert_eq!(back.len(), 1, "the inert sibling survives the drop");
    assert_eq!(back[0].name, "tag");
    assert!(back[0].vector.is_some(), "…with its `vector` sub-object intact");

    let conn = ro(&path);
    // (1) THE assertion.
    assert!(
        !vector_kind_registered(&conn, "doc"),
        "TC-71: an INERT `{{filterable, vector}}` sibling must not mask the drop of the last \
         `searchable→vector` projection. Before the fix its surviving `vector_declared = 1` row \
         made the post-state read `declared`, so `unenrol_registry_vector_node_kinds` never ran"
    );
    // (2) nothing deleted.
    assert!(vector_row_exists(&conn, c1), "the drop still deletes no embedding");
    assert!(vec0_row_exists(&conn, c1), "…including the vec0 row");

    // (3)+(4)
    delay_ms.store(8_000, Ordering::SeqCst);
    engine.write(&[node("doc", "N2", r#"{"summary":"after","tag":"beta"}"#)]).expect("N2");
    engine.drain(2_000).expect("`drain` must not wait: no searchable declaration remains");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "with no `searchable→vector` declaration left, a write must embed nothing"
    );
    let conn = ro(&path);
    assert!(!vector_row_exists(&conn, active_cursor(&conn, "N2")), "N2 has no vector");
    assert_eq!(
        eav_values(&conn, "tag"),
        vec!["alpha".to_string(), "beta".to_string()],
        "the inert sibling keeps projecting its VALUES throughout — inert, not absent"
    );

    opened.engine.close().unwrap();
}

// ===========================================================================
// The boundary — inert, NOT absent
// ===========================================================================

/// The Rust equivalent of `src/ts/tests/slice15d-projection-registry.test.ts`'s
/// "vector sub-object round-trips" case, which had no Rust counterpart. Opened
/// WITHOUT an embedder — the same vacuous configuration the shipped
/// `vector_subobject_is_stored_not_built` uses — purely to pin that TC-71 did
/// not change the persisted/reported shape of the declaration on the path the
/// SDK conformance tests exercise.
#[test]
fn a_filterable_vector_declaration_round_trips_verbatim_without_an_embedder() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc71_round_trip_no_embedder");
    let opened = Engine::open(path.clone()).expect("open");
    let engine = &opened.engine;
    engine.write(&[node("doc", "N1", r#"{"summary":"a dense meaning"}"#)]).expect("write N1");

    let spec = filterable_vector_spec("summary");
    let delta = declare_legacy_filterable_vector(engine, &path, "summary");
    assert_eq!(delta.built, vec!["summary".to_string()]);

    assert_eq!(
        engine.read_projections().expect("read_projections"),
        vec![ProjectionSpec {
            vector: Some(ProjectionVector {
                embedder: None,
                dense_readiness: Some(DenseReadiness::Ready),
            }),
            ..spec.clone()
        }],
        "the `vector` sub-object persists verbatim, plus the engine-set readiness — READS are \
         unaffected by Slice 23's reject, which is a WRITE-path spec validation"
    );

    // 0.8.20 Slice 23 (`R-20-SV`) — what re-applying now does, stated as a fact
    // so the legacy population's operators are not left to discover it. The
    // idempotent re-apply that used to diff to a no-op RAISES, because the spec
    // it would re-apply is the invalid one…
    assert_eq!(
        engine
            .configure_projections(std::slice::from_ref(&spec), &[])
            .expect_err("re-applying the legacy shape now raises"),
        fathomdb_engine::EngineError::WriteValidation,
    );
    // …and re-declaring only its VALID half is a DESTRUCTIVE change (it removes
    // the stored `vector` sub-object), so it is refused too. A legacy row has
    // exactly two remedies: ADD the `searchable` role (non-destructive), or name
    // it in `drop`.
    assert!(
        matches!(
            engine.configure_projections(&[filterable_only_spec("summary")], &[]),
            Err(fathomdb_engine::EngineError::ProjectionDestructive { .. })
        ),
        "re-declaring the valid half alone removes the stored `vector` sub-object"
    );
    let promoted = ProjectionSpec {
        roles: roles(&[ProjectionRole::Filterable, ProjectionRole::Searchable]),
        ..spec.clone()
    };
    engine
        .configure_projections(std::slice::from_ref(&promoted), &[])
        .expect("REMEDY 1: adding the `searchable` role is non-destructive and accepted");
    let again = engine
        .configure_projections(std::slice::from_ref(&promoted), &[])
        .expect("re-apply the promoted spec");
    assert!(again.unchanged, "re-registering the same VALID spec still diffs to a no-op");

    opened.engine.drain(5_000).unwrap();
    opened.engine.close().unwrap();

    let conn = ro(&path);
    assert_eq!(
        eav_values(&conn, "summary"),
        vec!["a dense meaning".to_string()],
        "the value is at rest — inert means no EMBEDDING, not no projection"
    );
}
