//! 0.8.20 Slice 23 (`R-20-SV`) — **an `fts` or `vector` sub-object declared
//! WITHOUT the `searchable` role is an INVALID SPEC and is REJECTED.**
//!
//! ## The ruling this implements
//!
//! HITL, **2026-07-24** (`dev/plans/plan-0.8.20.md` §11 item 4): option **(b)
//! REJECT** — *"it is a meaningless config; fail-fast matches the hard-reject
//! philosophy, and additive strictness is safe pre-1.0"*, to be implemented *"at
//! the next `configure_projections` slice"*. That is this slice. It **overturns**
//! the shipped 15d fix-4 accept-and-round-trip position, which is why the tests
//! that pinned the old behaviour move to a legacy back door rather than
//! disappear (see below).
//!
//! ## Why the sub-object cannot confer the role
//!
//! `searchable→FTS` and `searchable→vector` are **tier labels**, not roles
//! (`ProjectionRole` has exactly three members, HITL-ratified S8): the
//! `fts`/`vector` sub-objects **SELECT a sub-target of `searchable`; they do not
//! confer one** — the wording is `StoredProjection::wants_vector`'s own doc
//! comment, and both engine predicates (`wants_property_fts`, `wants_vector`)
//! are conjunctions with `roles.contains(Searchable)`. So without the role the
//! sub-object selects a sub-target of a projection that does not exist: it
//! builds nothing, embeds nothing, and enrols nothing. It is the "meaningless
//! config" the ruling names.
//!
//! The reject therefore keys on the **absence of `searchable`**, and on nothing
//! else — `rankable` and `filterable` are orthogonal axes that neither supply
//! nor substitute for it.
//!
//! ## The error family — settled, not chosen here
//!
//! `EngineError::WriteValidation`, per **decision #18** (0.8.20 Slice 22,
//! `R-20-VC`): the write-SHAPE boundary is ONE family. This is a SHAPE rejection
//! — the submitted spec's shape is malformed — so it takes that family. The
//! surviving `InvalidArgument { msg }` rejections in the same function are the
//! projection/drop **NAME** rejections, whose message names the offending value;
//! they are untouched and are pinned below so the boundary cannot drift.
//!
//! **Known diagnostic cost, accepted (TC-95/TC-98).** `WriteValidation` is a UNIT
//! variant, so when `configure_projections` is handed a LIST of specs the refusal
//! cannot name WHICH spec was invalid — strictly worse than its name-rejecting
//! siblings in the same function. Deferred by the HITL until after this slice;
//! recorded, not worked around.

use fathomdb_engine::{
    Engine, EngineError, InitialState, PreparedWrite, ProjectionFts, ProjectionRole,
    ProjectionSpec, ProjectionVector, SourceId,
};
use fathomdb_schema::SQLITE_SUFFIX;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn db_path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn roles(rs: &[ProjectionRole]) -> BTreeSet<ProjectionRole> {
    rs.iter().copied().collect()
}

fn spec(name: &str, rs: &[ProjectionRole], fts: bool, vector: bool) -> ProjectionSpec {
    ProjectionSpec {
        name: name.to_string(),
        roles: roles(rs),
        fts: fts.then_some(ProjectionFts { tokenizer: None }),
        vector: vector.then_some(ProjectionVector { embedder: None, dense_readiness: None }),
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

/// A raw READ-ONLY connection to the live file. `mode=ro`, never `immutable=1`.
fn ro(path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only")
}

/// The registry rows at rest, as `(name, roles, fts_tokenizer, vector_declared)`.
/// The reject's "total no-op" contract is asserted HERE, on the durable table,
/// not on a returned struct.
fn registry_rows(path: &Path) -> Vec<(String, String, Option<String>, i64)> {
    let conn = ro(path);
    let mut stmt = conn
        .prepare(
            "SELECT name, roles, fts_tokenizer, vector_declared
             FROM _fathomdb_projection_registry ORDER BY name",
        )
        .expect("prepare registry probe");
    let v: Vec<(String, String, Option<String>, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .expect("registry query")
        .map(|r| r.expect("registry row"))
        .collect();
    v
}

fn eav_values(path: &Path, attr_name: &str) -> Vec<String> {
    let conn = ro(path);
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

/// Seed ONE `_fathomdb_projection_registry` row on a raw RW connection, i.e.
/// through the back door the shipped 15d code used before this slice's reject.
/// This is the ONLY way to reach the LEGACY population — a database that
/// declared `fts`/`vector` without `searchable` while the engine still accepted
/// it — now that the public verb refuses to create it.
///
/// The INSERT shape is `persist_projection_row`'s, verbatim
/// (`name, roles, fts_tokenizer, vector_embedder, vector_declared`); `roles` is
/// the compact sorted comma-separated list `roles_to_storage` writes, and
/// `fts_tokenizer` is NULL for "no `fts` sub-object" / `''` for
/// "`fts` sub-object, engine-default tokenizer".
fn seed_legacy_registry_row(
    path: &Path,
    name: &str,
    roles_csv: &str,
    fts_tokenizer: Option<&str>,
    vector_declared: bool,
) {
    let conn = rusqlite::Connection::open(path).expect("open rw");
    conn.execute(
        "INSERT INTO _fathomdb_projection_registry
             (name, roles, fts_tokenizer, vector_embedder, vector_declared)
         VALUES(?1, ?2, ?3, NULL, ?4)
         ON CONFLICT(name) DO UPDATE SET
             roles = excluded.roles,
             fts_tokenizer = excluded.fts_tokenizer,
             vector_embedder = excluded.vector_embedder,
             vector_declared = excluded.vector_declared",
        rusqlite::params![name, roles_csv, fts_tokenizer, i64::from(vector_declared)],
    )
    .expect("seed legacy registry row");
}

// ===========================================================================
// The reject itself
// ===========================================================================

/// An `fts` sub-object with no `searchable` role — `WriteValidation`.
#[test]
fn an_fts_sub_object_without_the_searchable_role_is_rejected() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "sv_fts_no_searchable")).unwrap();
    let engine = &opened.engine;

    let err = engine
        .configure_projections(&[spec("status", &[ProjectionRole::Filterable], true, false)], &[])
        .expect_err(
            "R-20-SV (HITL 2026-07-24, plan §11 item 4): an `fts` sub-object without the \
             `searchable` role is an INVALID SPEC and must be REJECTED, not accepted-inert",
        );
    assert_eq!(
        err,
        EngineError::WriteValidation,
        "decision #18 settles the write-SHAPE boundary on ONE family; this is a shape reject"
    );

    opened.engine.close().unwrap();
}

/// A `vector` sub-object with no `searchable` role — `WriteValidation`. This is
/// the shape `TC-71` made inert; the ruling makes it unconstructible.
#[test]
fn a_vector_sub_object_without_the_searchable_role_is_rejected() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "sv_vector_no_searchable")).unwrap();
    let engine = &opened.engine;

    let err = engine
        .configure_projections(&[spec("status", &[ProjectionRole::Filterable], false, true)], &[])
        .expect_err("a `vector` sub-object without the `searchable` role is an invalid spec");
    assert_eq!(err, EngineError::WriteValidation);

    opened.engine.close().unwrap();
}

/// **The reject keys on the ABSENCE of `searchable`, and on nothing else.**
/// `rankable` and `filterable` are orthogonal axes: neither supplies the role
/// nor substitutes for it, so every non-`searchable` role set is refused
/// identically. Without this the fix could be mis-implemented as "reject only
/// when `filterable`", leaving `{rankable} + vector` — the exact shape the
/// 15e fix-2 finding-2 delta test used — silently accepted.
#[test]
fn the_reject_is_keyed_on_searchable_alone_not_on_the_other_roles() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "sv_role_axes")).unwrap();
    let engine = &opened.engine;

    for (label, rs) in [
        ("filterable-only", vec![ProjectionRole::Filterable]),
        ("rankable-only", vec![ProjectionRole::Rankable]),
        ("filterable+rankable", vec![ProjectionRole::Filterable, ProjectionRole::Rankable]),
    ] {
        for (fts, vector) in [(true, false), (false, true), (true, true)] {
            let err = engine
                .configure_projections(&[spec("status", &rs, fts, vector)], &[])
                .expect_err(&format!(
                    "{label} with fts={fts} vector={vector} declares a sub-target of a \
                     `searchable` projection that does not exist"
                ));
            assert_eq!(err, EngineError::WriteValidation, "{label} fts={fts} vector={vector}");
        }
    }

    opened.engine.close().unwrap();
}

/// **The CONTROL — the reject must not over-reach.** With `searchable` present
/// the identical sub-objects are valid, build, and round-trip. A reject that
/// also refused these would be a far worse regression than the config it closes.
#[test]
fn the_searchable_role_makes_the_same_sub_objects_valid() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "sv_control_valid");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;
    engine.write(&[node("N1", r#"{"summary":"a dense meaning"}"#)]).unwrap();

    let s = spec("summary", &[ProjectionRole::Filterable, ProjectionRole::Searchable], true, true);
    let delta = engine
        .configure_projections(std::slice::from_ref(&s), &[])
        .expect("CONTROL: with the `searchable` role the same sub-objects are a VALID spec");
    assert_eq!(delta.built, vec!["summary".to_string()]);
    assert_eq!(delta.deferred, vec!["summary".to_string()], "the vector sub-target still defers");

    let back = engine.read_projections().expect("read_projections");
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].name, "summary");
    assert!(back[0].fts.is_some() && back[0].vector.is_some(), "both sub-objects round-trip");
    assert_eq!(back[0].roles, roles(&[ProjectionRole::Filterable, ProjectionRole::Searchable]));

    // …and `searchable` with NO sub-object at all is still valid: the reject is
    // about a sub-object without the role, never about the role without a
    // sub-object.
    engine
        .configure_projections(&[spec("title", &[ProjectionRole::Searchable], false, false)], &[])
        .expect("CONTROL: a bare `searchable` declaration is unaffected");

    opened.engine.drain(5_000).unwrap();
    opened.engine.close().unwrap();
    assert_eq!(eav_values(&path, "summary"), vec!["a dense meaning".to_string()]);
}

// ===========================================================================
// A rejected request is a TOTAL no-op
// ===========================================================================

/// The fix-6 "a rejected request is a total no-op" contract extends to this
/// reject: `apply_projection_config` validates the WHOLE request up front, so
/// one invalid spec anywhere in the list aborts before any write — the valid
/// sibling in the same call must NOT be applied, and a pre-existing registry
/// must be untouched.
#[test]
fn a_rejected_spec_makes_the_whole_request_a_total_no_op() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "sv_total_noop");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;
    engine.write(&[node("N1", r#"{"summary":"a meaning","status":"open"}"#)]).unwrap();

    // A live, valid registry first, so "untouched" is falsifiable.
    engine
        .configure_projections(&[spec("summary", &[ProjectionRole::Searchable], true, false)], &[])
        .expect("seed a valid projection");
    let before = registry_rows(&path);
    assert_eq!(before.len(), 1, "fixture: exactly one live projection");

    // A batch whose FIRST spec is valid and SECOND is not.
    let err = engine
        .configure_projections(
            &[
                spec("title", &[ProjectionRole::Filterable], false, false),
                spec("status", &[ProjectionRole::Filterable], true, false),
            ],
            &["summary".to_string()],
        )
        .expect_err("the invalid second spec rejects the request");
    assert_eq!(err, EngineError::WriteValidation);

    assert_eq!(
        registry_rows(&path),
        before,
        "a rejected request is a TOTAL no-op: the valid sibling was not registered and the \
         named drop did not apply"
    );
    assert!(
        eav_values(&path, "title").is_empty(),
        "…and the valid sibling built no EAV rows either"
    );
    assert_eq!(
        eav_values(&path, "summary"),
        vec!["a meaning".to_string()],
        "…while the pre-existing projection's rows survive the refused drop"
    );

    opened.engine.drain(5_000).unwrap();
    opened.engine.close().unwrap();
}

// ===========================================================================
// The boundary the reject must NOT cross — NAME rejections stay InvalidArgument
// ===========================================================================

/// **The `InvalidArgument` / `WriteValidation` boundary inside one function.**
/// `dev/design/errors.md` now states it explicitly: projection/drop **NAME**
/// rejections keep `InvalidArgument { msg }` — their message names the offending
/// value and is the caller's only handle on it — while the fts/vector-without-
/// `searchable` **SHAPE** reject is `WriteValidation` per decision #18. Pinned
/// here so neither side drifts onto the other.
///
/// (The `"projection '<name>' declares no roles"` refusal is a SHAPE rejection
/// that still returns `InvalidArgument`. It is shipped behaviour, out of this
/// slice's scope, and is asserted as-is — flagged in `errors.md`, not
/// retro-classified.)
#[test]
fn name_rejections_keep_invalid_argument_while_the_shape_reject_is_write_validation() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "sv_boundary")).unwrap();
    let engine = &opened.engine;

    let bad_name = engine
        .configure_projections(&[spec("a\\b", &[ProjectionRole::Filterable], false, false)], &[])
        .expect_err("a backslash name is still refused");
    assert!(
        matches!(bad_name, EngineError::InvalidArgument { ref msg }
            if msg.contains("invalid projection attribute name") && msg.contains('b')),
        "a NAME rejection must keep the message that names the offending value, got {bad_name:?}"
    );

    let bad_drop = engine
        .configure_projections(&[], &["a\"b".to_string()])
        .expect_err("a quote in a drop name is still refused");
    assert!(
        matches!(bad_drop, EngineError::InvalidArgument { .. }),
        "a DROP-NAME rejection stays InvalidArgument, got {bad_drop:?}"
    );

    let dup = engine
        .configure_projections(
            &[
                spec("status", &[ProjectionRole::Filterable], false, false),
                spec("status", &[ProjectionRole::Filterable], false, false),
            ],
            &[],
        )
        .expect_err("a duplicated name in one request is still refused");
    assert!(
        matches!(dup, EngineError::InvalidArgument { ref msg } if msg.contains("status")),
        "a duplicate-NAME rejection stays InvalidArgument, got {dup:?}"
    );

    let no_roles = engine
        .configure_projections(&[spec("status", &[], false, false)], &[])
        .expect_err("an empty role set is still refused");
    assert!(
        matches!(no_roles, EngineError::InvalidArgument { ref msg } if msg.contains("no roles")),
        "SHIPPED, deliberately unchanged by this slice: the empty-roles refusal is a SHAPE \
         rejection that still returns InvalidArgument. Retro-classifying it is not R-20-SV's \
         job; it is flagged in dev/design/errors.md, got {no_roles:?}"
    );

    // …and the shape reject, in the same function, in the same session.
    assert_eq!(
        engine
            .configure_projections(
                &[spec("status", &[ProjectionRole::Filterable], false, true)],
                &[]
            )
            .expect_err("the shape reject"),
        EngineError::WriteValidation,
    );

    opened.engine.close().unwrap();
}

// ===========================================================================
// The LEGACY population — readable, reportable, no longer re-appliable
// ===========================================================================

/// **The legacy round-trip.** Databases that declared `fts`/`vector` without
/// `searchable` while the engine ACCEPTED it still exist; this slice must not
/// make them unreadable. `read_projections` is a pure read and rejects nothing,
/// so the legacy row is reported **verbatim** — but feeding that output straight
/// back into `configure_projections` (the shipped fix-4 read→configure
/// round-trip) now RAISES.
///
/// That asymmetry is the honest, documented consequence of the ruling and is
/// pinned here rather than left to be discovered: for the legacy population the
/// round-trip is broken BY DESIGN, and the remedy is to add the `searchable`
/// role or drop the sub-object.
#[test]
fn a_legacy_registry_row_reads_back_verbatim_but_no_longer_re_applies() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "sv_legacy_round_trip");
    let opened = Engine::open(path.clone()).unwrap();
    opened.engine.write(&[node("N1", r#"{"status":"open"}"#)]).unwrap();
    opened.engine.close().unwrap();

    // The legacy state, written the way the shipped pre-Slice-23 engine wrote it.
    seed_legacy_registry_row(&path, "status", "filterable", Some(""), true);

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    let back = engine.read_projections().expect("a legacy row must still be READABLE");
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].name, "status");
    assert_eq!(back[0].roles, roles(&[ProjectionRole::Filterable]));
    assert!(back[0].fts.is_some(), "the legacy `fts` sub-object is reported verbatim");
    assert!(back[0].vector.is_some(), "…and the legacy `vector` sub-object too");

    // …but it can no longer be re-applied.
    let err = engine
        .configure_projections(&back, &[])
        .expect_err("re-applying the legacy shape now raises");
    assert_eq!(
        err,
        EngineError::WriteValidation,
        "BREAKING, by design: for the legacy fts/vector-without-`searchable` population the \
         read.projections -> configure_projections round-trip no longer closes. Add the \
         `searchable` role or drop the sub-object"
    );

    // The remedy closes it again, and the row is otherwise untouched.
    let mut fixed = back[0].clone();
    fixed.roles.insert(ProjectionRole::Searchable);
    engine
        .configure_projections(&[fixed], &[])
        .expect("adding the `searchable` role makes the legacy declaration valid again");

    opened.engine.drain(5_000).unwrap();
    opened.engine.close().unwrap();
}
