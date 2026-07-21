//! 0.8.20 Slice 15d (R-20-PR / R-20-EAV) — the projection registry (C-1
//! co-land) + EAV / property-FTS store.
//!
//! The Phase-2 keystone (Slices 20 and 25 depend on it). `configure_projections`
//! is a DECLARATIVE, IDEMPOTENT apply: the engine is the sole projection
//! authority and diffs the supplied specs against a durable registry, backfilling
//! the difference in one transaction. `read.projections` introspects current
//! state. The EAV store (`canonical_attributes`) + property-FTS
//! (`property_search_index`) are net-new — before step 24 there is no attribute
//! store and no property-FTS, only `body`-FTS + vector.
//!
//! Acceptance signals (plan §3, falsifiable, offline):
//!
//! - **R-20-PR** — re-registration is a no-op (idempotent diff → empty delta); a
//!   role add builds exactly that projection and a `drop` drops exactly that one;
//!   omission does NOT drop; boot re-derive is crash-safe + idempotent; an
//!   incompatible/destructive change requires explicit `drop`.
//! - **R-20-EAV** — property-level filter AND property-FTS search return correct
//!   rows (asserted on the RAW projected tables where the value is at rest);
//!   `body`-FTS behaviour is UNCHANGED (no silent drift).

use fathomdb_engine::{
    Engine, EngineError, InitialState, LifecycleState, ProjectionFts, ProjectionRole,
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

/// A projection spec with the given roles and optional FTS/vector sub-objects
/// (each with the engine-default tokenizer/embedder).
fn spec(name: &str, rs: &[ProjectionRole], fts: bool, vector: bool) -> ProjectionSpec {
    ProjectionSpec {
        name: name.to_string(),
        roles: roles(rs),
        fts: fts.then_some(ProjectionFts { tokenizer: None }),
        vector: vector.then_some(ProjectionVector { embedder: None }),
    }
}

/// A governed node write carrying a JSON body (the source the attribute store
/// derives from). `logical_id` makes it lifecycle-addressable and re-writable.
fn node(logical_id: &str, source: &str, body_json: &str) -> fathomdb_engine::PreparedWrite {
    fathomdb_engine::PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body_json.to_string(),
        source_id: SourceId::new(source).expect("source id"),
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

/// A governed node write in an explicit create-time lifecycle state (the
/// `node` helper hardcodes `Active`; this one exercises `Pending`).
fn node_state(
    logical_id: &str,
    source: &str,
    body_json: &str,
    state: InitialState,
) -> fathomdb_engine::PreparedWrite {
    fathomdb_engine::PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body_json.to_string(),
        source_id: SourceId::new(source).expect("source id"),
        logical_id: Some(logical_id.to_string()),
        state,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

/// The single active (`superseded_at IS NULL`) write_cursor for a logical_id —
/// the cursor the property projections key on. Read mid-session (WAL readers see
/// committed data).
fn active_cursor(path: &Path, logical_id: &str) -> i64 {
    let conn = ro(path);
    conn.query_row(
        "SELECT write_cursor FROM canonical_nodes \
         WHERE logical_id = ?1 AND superseded_at IS NULL",
        [logical_id],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
}

fn ro(path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only")
}

/// Raw EAV rows for one attribute: `(attr_value)` ordered — the data-at-rest
/// oracle for `filterable`.
fn eav_values(path: &Path, attr_name: &str) -> Vec<String> {
    let conn = ro(path);
    let mut stmt = conn
        .prepare(
            "SELECT attr_value FROM canonical_attributes WHERE attr_name = ?1 ORDER BY attr_value",
        )
        .unwrap();
    let v: Vec<String> = stmt
        .query_map([attr_name], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    v
}

/// write_cursors whose EAV value for `attr_name` equals `value` — the raw
/// `filterable` equality result.
fn eav_filter(path: &Path, attr_name: &str, value: &str) -> Vec<i64> {
    let conn = ro(path);
    let mut stmt = conn
        .prepare(
            "SELECT write_cursor FROM canonical_attributes
             WHERE attr_name = ?1 AND attr_value = ?2 ORDER BY write_cursor",
        )
        .unwrap();
    stmt.query_map([attr_name, value], |r| r.get::<_, i64>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

/// write_cursors whose property-FTS row for `attr_name` MATCHes `query` — the
/// raw `searchable→FTS` result.
fn property_fts_match(path: &Path, attr_name: &str, query: &str) -> Vec<i64> {
    let conn = ro(path);
    let mut stmt = conn
        .prepare(
            "SELECT write_cursor FROM property_search_index
             WHERE attr_name = ?1 AND property_search_index MATCH ?2 ORDER BY write_cursor",
        )
        .unwrap();
    stmt.query_map([attr_name, query], |r| r.get::<_, i64>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn property_fts_rowcount(path: &Path, attr_name: &str) -> i64 {
    let conn = ro(path);
    conn.query_row(
        "SELECT COUNT(*) FROM property_search_index WHERE attr_name = ?1",
        [attr_name],
        |r| r.get(0),
    )
    .unwrap()
}

/// The body-FTS oracle: the raw `search_index` / `search_index_v2` row counts.
/// Used to prove body-FTS behaviour does not drift when a projection is declared.
fn body_fts_counts(path: &Path) -> (i64, i64) {
    let conn = ro(path);
    let a: i64 = conn.query_row("SELECT COUNT(*) FROM search_index", [], |r| r.get(0)).unwrap();
    let b: i64 = conn.query_row("SELECT COUNT(*) FROM search_index_v2", [], |r| r.get(0)).unwrap();
    (a, b)
}

// ===========================================================================
// R-20-PR — registry semantics
// ===========================================================================

/// Configure a spec, then read it back verbatim (round trip through the durable
/// registry).
#[test]
fn configure_and_read_projections_round_trip() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "round_trip")).unwrap();
    let engine = &opened.engine;

    let s = spec("status", &[ProjectionRole::Filterable, ProjectionRole::Searchable], true, false);
    engine.configure_projections(std::slice::from_ref(&s), &[]).unwrap();

    let back = engine.read_projections().unwrap();
    assert_eq!(back, vec![s], "read.projections must round-trip the declared spec verbatim");
}

/// **Keystone.** Re-registering the identical spec diffs to an empty delta — a
/// no-op. This is the CQRS drift guard: applying the same declaration twice must
/// not rebuild or churn.
#[test]
fn idempotent_reregistration_is_a_noop() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "idempotent")).unwrap();
    let engine = &opened.engine;
    engine.write(&[node("N1", "src:1", r#"{"status":"open"}"#)]).unwrap();

    let s = spec("status", &[ProjectionRole::Filterable], false, false);
    let first = engine.configure_projections(std::slice::from_ref(&s), &[]).unwrap();
    assert!(!first.unchanged, "first apply builds the projection");
    assert_eq!(first.built, vec!["status".to_string()]);

    let second = engine.configure_projections(std::slice::from_ref(&s), &[]).unwrap();
    assert!(second.unchanged, "identical re-registration must diff to a no-op");
    assert!(second.built.is_empty() && second.dropped.is_empty() && second.deferred.is_empty());
}

/// A role add builds EXACTLY that projection; an explicit drop drops EXACTLY that
/// one; omission does NOT drop.
#[test]
fn role_add_builds_and_explicit_drop_drops_exactly_one() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "add_drop");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;
    engine.write(&[node("N1", "src:1", r#"{"status":"open","title":"hello world"}"#)]).unwrap();

    // filterable-only on `status`.
    engine
        .configure_projections(&[spec("status", &[ProjectionRole::Filterable], false, false)], &[])
        .unwrap();
    // Adding `searchable`+fts to `status` builds property-FTS for it.
    let d = engine
        .configure_projections(
            &[spec(
                "status",
                &[ProjectionRole::Filterable, ProjectionRole::Searchable],
                true,
                false,
            )],
            &[],
        )
        .unwrap();
    assert_eq!(d.built, vec!["status".to_string()], "the role add rebuilds exactly `status`");

    // Add a SECOND projection `title` (searchable/fts).
    engine
        .configure_projections(&[spec("title", &[ProjectionRole::Searchable], true, false)], &[])
        .unwrap();

    // OMISSION does not drop: re-declaring only `title` must leave `status` alone.
    let omit = engine
        .configure_projections(&[spec("title", &[ProjectionRole::Searchable], true, false)], &[])
        .unwrap();
    assert!(omit.dropped.is_empty(), "omitting `status` must NOT drop it (C3)");
    assert_eq!(
        engine.read_projections().unwrap().len(),
        2,
        "both projections still declared after an omission"
    );

    // Explicit drop of `status` removes exactly it (and its EAV/property-FTS).
    let drop = engine.configure_projections(&[], &["status".to_string()]).unwrap();
    assert_eq!(drop.dropped, vec!["status".to_string()]);
    let remaining: Vec<String> =
        engine.read_projections().unwrap().into_iter().map(|s| s.name).collect();
    assert_eq!(remaining, vec!["title".to_string()], "only `status` dropped");

    opened.engine.drain(5_000).unwrap();
    opened.engine.close().unwrap();
    assert!(eav_values(&path, "status").is_empty(), "dropped attr's EAV rows are gone");
    assert_eq!(property_fts_rowcount(&path, "status"), 0, "dropped attr's property-FTS rows gone");
    // `title` survives.
    assert_eq!(property_fts_rowcount(&path, "title"), 1, "the un-dropped projection is untouched");
}

/// An incompatible/DESTRUCTIVE change to a live projection without an explicit
/// `drop` is REFUSED with the destructive delta surfaced; naming it in `drop`
/// lets the caller consciously rebuild.
#[test]
fn destructive_change_requires_explicit_drop() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "destructive")).unwrap();
    let engine = &opened.engine;
    engine.write(&[node("N1", "src:1", r#"{"status":"open"}"#)]).unwrap();

    engine
        .configure_projections(
            &[spec(
                "status",
                &[ProjectionRole::Filterable, ProjectionRole::Searchable],
                true,
                false,
            )],
            &[],
        )
        .unwrap();

    // Removing the `searchable` role is destructive → refused without a drop.
    let err = engine
        .configure_projections(&[spec("status", &[ProjectionRole::Filterable], false, false)], &[])
        .unwrap_err();
    match err {
        EngineError::ProjectionDestructive { name, .. } => assert_eq!(name, "status"),
        other => panic!("expected ProjectionDestructive, got {other:?}"),
    }
    // The live projection is UNCHANGED after the refusal.
    assert_eq!(
        engine.read_projections().unwrap()[0].roles,
        roles(&[ProjectionRole::Filterable, ProjectionRole::Searchable]),
        "a refused destructive change must not partially apply"
    );

    // Naming it in `drop` lets it rebuild fresh.
    let ok = engine
        .configure_projections(
            &[spec("status", &[ProjectionRole::Filterable], false, false)],
            &["status".to_string()],
        )
        .unwrap();
    assert_eq!(ok.dropped, vec!["status".to_string()]);
    assert_eq!(
        engine.read_projections().unwrap()[0].roles,
        roles(&[ProjectionRole::Filterable]),
        "the explicit drop+re-declare rebuilds with the reduced role set"
    );
}

/// `rankable` is graceful-absent (Q6a): declaring it is legal, builds nothing,
/// errors never, and is reported as deferred.
#[test]
fn rankable_is_graceful_deferred_never_blocking() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "rankable");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;
    engine.write(&[node("N1", "src:1", r#"{"importance":"high"}"#)]).unwrap();

    let d = engine
        .configure_projections(
            &[spec("importance", &[ProjectionRole::Rankable], false, false)],
            &[],
        )
        .unwrap();
    assert!(d.built.is_empty(), "rankable builds no same-transaction projection");
    assert_eq!(d.deferred, vec!["importance".to_string()], "rankable is reported deferred");

    opened.engine.drain(5_000).unwrap();
    opened.engine.close().unwrap();
    assert!(eav_values(&path, "importance").is_empty(), "rankable-only writes no EAV value");
}

/// The `searchable→vector` sub-object is STORED (so Slice 20 attaches
/// `dense_readiness` to it) but 15d builds NO embedding / property-FTS for it.
#[test]
fn vector_subobject_is_stored_not_built() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "vector_stored");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;
    engine.write(&[node("N1", "src:1", r#"{"summary":"a dense meaning"}"#)]).unwrap();

    // searchable with a VECTOR sub-target only (no fts).
    let s = spec("summary", &[ProjectionRole::Searchable], false, true);
    let d = engine.configure_projections(std::slice::from_ref(&s), &[]).unwrap();
    assert_eq!(d.deferred, vec!["summary".to_string()], "the vector sub-target defers to Slice 20");

    // The vector sub-object round-trips through read.projections (Slice 20 hangs
    // dense_readiness off exactly this).
    assert_eq!(engine.read_projections().unwrap(), vec![s], "vector sub-object persists verbatim");

    opened.engine.drain(5_000).unwrap();
    opened.engine.close().unwrap();
    // The VALUE is stored at rest (Slice 20 will embed it) but no property-FTS.
    assert_eq!(eav_values(&path, "summary"), vec!["a dense meaning".to_string()]);
    assert_eq!(
        property_fts_rowcount(&path, "summary"),
        0,
        "no property-FTS built for a vector-only"
    );
}

// ===========================================================================
// R-20-EAV — property filter + property-FTS + body-FTS invariance
// ===========================================================================

/// property-level FILTER returns correct rows, asserted on the RAW EAV table
/// where the value is at rest. Same-transaction: a write AFTER configure is
/// immediately in the EAV store.
#[test]
fn property_filter_returns_correct_rows() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "filter");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    // Two nodes exist BEFORE the projection (backfill target).
    engine.write(&[node("A", "src:a", r#"{"status":"open"}"#)]).unwrap();
    engine.write(&[node("B", "src:b", r#"{"status":"closed"}"#)]).unwrap();
    engine
        .configure_projections(&[spec("status", &[ProjectionRole::Filterable], false, false)], &[])
        .unwrap();

    // A node written AFTER configure is projected same-transaction.
    engine.write(&[node("C", "src:c", r#"{"status":"open"}"#)]).unwrap();
    // A node with NO `status` attribute contributes no row (absent ≠ empty).
    engine.write(&[node("D", "src:d", r#"{"other":"x"}"#)]).unwrap();

    opened.engine.drain(5_000).unwrap();
    opened.engine.close().unwrap();

    assert_eq!(
        eav_values(&path, "status"),
        vec!["closed".to_string(), "open".to_string(), "open".to_string()],
        "backfill + same-transaction writes populate the EAV store; the attribute-less node adds none"
    );
    // Equality filter: which cursors have status='open'? (A=1, C=3.)
    assert_eq!(eav_filter(&path, "status", "open"), vec![1, 3]);
    assert_eq!(eav_filter(&path, "status", "closed"), vec![2]);
}

/// property-FTS SEARCH returns correct rows, asserted on the RAW FTS table via a
/// MATCH query.
#[test]
fn property_fts_search_returns_correct_rows() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "pfts");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;
    engine.write(&[node("A", "src:a", r#"{"title":"the quick brown fox"}"#)]).unwrap();
    engine.write(&[node("B", "src:b", r#"{"title":"lazy dogs sleeping"}"#)]).unwrap();
    engine
        .configure_projections(&[spec("title", &[ProjectionRole::Searchable], true, false)], &[])
        .unwrap();
    engine.write(&[node("C", "src:c", r#"{"title":"a brown bear"}"#)]).unwrap();

    opened.engine.drain(5_000).unwrap();
    opened.engine.close().unwrap();

    // "brown" matches A (cursor 1) and C (cursor 3), not B.
    assert_eq!(property_fts_match(&path, "title", "brown"), vec![1, 3]);
    // "fox" matches only A.
    assert_eq!(property_fts_match(&path, "title", "fox"), vec![1]);
    // stemming (porter): "sleeping" matches B via "sleep".
    assert_eq!(property_fts_match(&path, "title", "sleep"), vec![2]);
}

/// **No silent drift.** Declaring a projection must NOT change `body`-FTS: the
/// `search_index` / `search_index_v2` row counts are byte-stable across a
/// configure. body-FTS and property-FTS are independent channels.
#[test]
fn body_fts_behaviour_is_unchanged_by_projection_config() {
    let dir = TempDir::new().unwrap();
    let base = db_path(&dir, "body_base");
    let with_proj = db_path(&dir, "body_proj");

    // Baseline DB: three nodes, NO projection.
    {
        let opened = Engine::open(base.clone()).unwrap();
        opened.engine.write(&[node("A", "src:a", r#"{"status":"open"}"#)]).unwrap();
        opened.engine.write(&[node("B", "src:b", r#"{"status":"closed"}"#)]).unwrap();
        opened.engine.write(&[node("C", "src:c", r#"{"status":"open"}"#)]).unwrap();
        opened.engine.drain(5_000).unwrap();
        opened.engine.close().unwrap();
    }
    // Same three nodes but WITH a projection declared and backfilled.
    {
        let opened = Engine::open(with_proj.clone()).unwrap();
        opened.engine.write(&[node("A", "src:a", r#"{"status":"open"}"#)]).unwrap();
        opened.engine.write(&[node("B", "src:b", r#"{"status":"closed"}"#)]).unwrap();
        opened
            .engine
            .configure_projections(
                &[spec(
                    "status",
                    &[ProjectionRole::Filterable, ProjectionRole::Searchable],
                    true,
                    false,
                )],
                &[],
            )
            .unwrap();
        opened.engine.write(&[node("C", "src:c", r#"{"status":"open"}"#)]).unwrap();
        opened.engine.drain(5_000).unwrap();
        opened.engine.close().unwrap();
    }

    assert_eq!(
        body_fts_counts(&base),
        body_fts_counts(&with_proj),
        "body-FTS (search_index / search_index_v2) must be byte-stable whether or not a \
         projection is declared — property projections are an independent channel"
    );
}

// ===========================================================================
// R-20-E1 co-land — erasure reaches the new projection tables
// ===========================================================================

/// The attribute store + property-FTS are ROW-OWNED: `erase_source` erases the
/// attribute VALUES at rest, not just the node body. An unregistered
/// content-storing table would leave PII on disk (the `search_index_v2` leak
/// class this registry closes).
#[test]
fn erase_source_reaches_attribute_projections() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "erase");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;
    engine.write(&[node("A", "src:secret", r#"{"title":"sensitive personal note"}"#)]).unwrap();
    engine.write(&[node("B", "src:other", r#"{"title":"unrelated public note"}"#)]).unwrap();
    engine
        .configure_projections(&[spec("title", &[ProjectionRole::Searchable], true, false)], &[])
        .unwrap();

    // Erase the anonymous provenance `src:secret`.
    engine.erase_source("src:secret").unwrap();
    opened.engine.drain(5_000).unwrap();
    opened.engine.close().unwrap();

    // The erased node's attribute VALUE is gone from BOTH projected tables.
    assert_eq!(
        eav_values(&path, "title"),
        vec!["unrelated public note".to_string()],
        "the erased node's EAV attribute value must not survive on disk"
    );
    assert!(
        property_fts_match(&path, "title", "sensitive").is_empty(),
        "the erased node's property-FTS row must not survive on disk"
    );
    assert_eq!(
        property_fts_match(&path, "title", "unrelated"),
        vec![2],
        "the un-erased node's property-FTS row survives"
    );
    // Raw on-disk grep: the erased body text is absent from the FTS content.
    let conn = ro(&path);
    let leaked: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canonical_attributes WHERE attr_value LIKE '%sensitive%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(leaked, 0, "no erased attribute value may remain at rest");
}

// ===========================================================================
// Slice 15d fix-1 — codex §9 [P2] correctness findings
// ===========================================================================

/// **fix-1 finding 1 [P2].** Non-string JSON scalar attribute values must
/// project. A JSON number and a JSON bool are common filterable inputs; the
/// original single-attribute projector read the extraction as `Option<String>`
/// and silently dropped any non-string (the type conversion failed and
/// `.unwrap_or(None)` treated it as absent), so a `score`/`flag` never populated
/// `canonical_attributes` / `property_search_index`. Asserted on the RAW tables
/// (property is at rest). Includes a string control so the test is non-vacuous.
#[test]
fn scalar_json_attributes_project_number_bool_and_string() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "scalars");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    engine
        .configure_projections(
            &[
                // number, searchable so we also exercise property-FTS on a number.
                spec(
                    "score",
                    &[ProjectionRole::Filterable, ProjectionRole::Searchable],
                    true,
                    false,
                ),
                spec("flag", &[ProjectionRole::Filterable], false, false),
                spec("label", &[ProjectionRole::Filterable], false, false),
            ],
            &[],
        )
        .unwrap();

    // One node carrying a JSON number, a JSON bool, and a JSON string.
    engine.write(&[node("N", "src:n", r#"{"score":3,"flag":true,"label":"open"}"#)]).unwrap();

    opened.engine.drain(5_000).unwrap();
    opened.engine.close().unwrap();

    // String control (proves the test is non-vacuous — this already worked).
    assert_eq!(
        eav_values(&path, "label"),
        vec!["open".to_string()],
        "string control still projects"
    );
    // Finding 1: a JSON NUMBER must project (was silently dropped).
    assert_eq!(
        eav_values(&path, "score"),
        vec!["3".to_string()],
        "a JSON number attribute must project into canonical_attributes"
    );
    // Finding 1: a JSON BOOL must project (was silently dropped).
    assert_eq!(
        eav_values(&path, "flag"),
        vec!["true".to_string()],
        "a JSON bool attribute must project into canonical_attributes"
    );
    // Equality filter over the projected number matches the owning cursor.
    assert_eq!(eav_filter(&path, "score", "3"), vec![1]);
    assert_eq!(eav_filter(&path, "flag", "true"), vec![1]);
    // The searchable number is in the property-FTS shadow too.
    assert_eq!(
        property_fts_match(&path, "score", "3"),
        vec![1],
        "a searchable JSON number must populate property_search_index"
    );
}

/// **fix-1 finding 2 [P2].** Superseded attribute-projection rows must be purged
/// on supersession, so the at-rest projection is active-only. Rewriting a node
/// with an existing `logical_id` marks the OLD canonical row `superseded_at` but
/// (pre-fix) never removed its `canonical_attributes` / `property_search_index`
/// rows, so a same-session property filter / property-FTS saw BOTH the stale and
/// the current value. Asserted on the RAW tables: zero rows reference the
/// superseded cursor.
#[test]
fn supersession_purges_stale_attribute_projection_rows() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "supersede_purge");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    engine
        .configure_projections(
            &[spec(
                "status",
                &[ProjectionRole::Filterable, ProjectionRole::Searchable],
                true,
                false,
            )],
            &[],
        )
        .unwrap();

    // Write L=open, then REWRITE the same logical_id L=closed (supersedes).
    engine.write(&[node("L", "src:l", r#"{"status":"open"}"#)]).unwrap();
    engine.write(&[node("L", "src:l", r#"{"status":"closed"}"#)]).unwrap();

    opened.engine.drain(5_000).unwrap();
    opened.engine.close().unwrap();

    let conn = ro(&path);
    // Identify the superseded and active cursors from canonical state (robust to
    // cursor numbering).
    let superseded_cursor: i64 = conn
        .query_row(
            "SELECT write_cursor FROM canonical_nodes \
             WHERE logical_id = 'L' AND superseded_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let active_cursor: i64 = conn
        .query_row(
            "SELECT write_cursor FROM canonical_nodes \
             WHERE logical_id = 'L' AND superseded_at IS NULL AND state = 'active'",
            [],
            |r| r.get(0),
        )
        .unwrap();

    // The active row shows only the current value.
    assert_eq!(
        eav_values(&path, "status"),
        vec!["closed".to_string()],
        "only the active value must survive at rest"
    );
    // ZERO rows reference the superseded cursor in EITHER projected table.
    let stale_eav: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canonical_attributes WHERE write_cursor = ?1",
            [superseded_cursor],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stale_eav, 0, "the superseded cursor's EAV rows must be purged");
    let stale_fts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM property_search_index WHERE write_cursor = ?1",
            [superseded_cursor],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stale_fts, 0, "the superseded cursor's property-FTS rows must be purged");

    // Property filter / property-FTS return ONLY the active value.
    assert_eq!(
        eav_filter(&path, "status", "open"),
        Vec::<i64>::new(),
        "the stale 'open' value must not filter-match after supersession"
    );
    assert_eq!(eav_filter(&path, "status", "closed"), vec![active_cursor]);
    assert_eq!(
        property_fts_match(&path, "status", "open"),
        Vec::<i64>::new(),
        "the stale 'open' property-FTS row must not match after supersession"
    );
    assert_eq!(property_fts_match(&path, "status", "closed"), vec![active_cursor]);
}

// ===========================================================================
// R-20-PR — boot re-derive (crash-safe + idempotent)
// ===========================================================================

/// **Boot re-derive keystone.** A DB whose registry row survives but whose
/// projection rows are missing (a crash window / restored registry) CONVERGES on
/// the next open: the engine re-drives the derived cache from canonical state.
#[test]
fn boot_rederive_converges_after_simulated_crash() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "rederive");
    {
        let opened = Engine::open(path.clone()).unwrap();
        opened.engine.write(&[node("A", "src:a", r#"{"title":"alpha meaning"}"#)]).unwrap();
        opened.engine.write(&[node("B", "src:b", r#"{"title":"beta meaning"}"#)]).unwrap();
        opened
            .engine
            .configure_projections(
                &[spec(
                    "title",
                    &[ProjectionRole::Filterable, ProjectionRole::Searchable],
                    true,
                    false,
                )],
                &[],
            )
            .unwrap();
        opened.engine.drain(5_000).unwrap();
        opened.engine.close().unwrap();
    }

    // Precondition: the projection was built.
    assert_eq!(eav_values(&path, "title").len(), 2, "precondition: projection populated");

    // Simulate a crash that lost the derived cache but kept the durable registry:
    // wipe the EAV + property-FTS rows directly, leaving the registry row intact.
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute("DELETE FROM canonical_attributes", []).unwrap();
        conn.execute("DELETE FROM property_search_index", []).unwrap();
        let regcount: i64 = conn
            .query_row("SELECT COUNT(*) FROM _fathomdb_projection_registry", [], |r| r.get(0))
            .unwrap();
        assert_eq!(regcount, 1, "the durable registry row survives the simulated crash");
    }
    assert!(eav_values(&path, "title").is_empty(), "simulated-crash precondition: cache is empty");

    // Reopen — boot re-derive must rebuild the derived cache idempotently.
    {
        let opened = Engine::open(path.clone()).unwrap();
        opened.engine.drain(5_000).unwrap();
        opened.engine.close().unwrap();
    }
    assert_eq!(
        eav_values(&path, "title"),
        vec!["alpha meaning".to_string(), "beta meaning".to_string()],
        "boot re-derive must rebuild the EAV store from canonical state"
    );
    assert_eq!(
        property_fts_match(&path, "title", "beta"),
        vec![2],
        "boot re-derive must rebuild the property-FTS shadow too"
    );

    // Idempotent: a SECOND reopen must not double the rows.
    {
        let opened = Engine::open(path.clone()).unwrap();
        opened.engine.close().unwrap();
    }
    assert_eq!(
        eav_values(&path, "title").len(),
        2,
        "boot re-derive is idempotent — a second open must not duplicate rows"
    );
}

// ===========================================================================
// R-20-EAV — fix-2: the at-rest projection is lifecycle-gated
// (projected ⟺ active ∧ non-superseded), maintained across the write path AND
// every legal `transition` move. The backfill only projects
// `state = 'active' AND superseded_at IS NULL`; the write path and lifecycle
// transitions must track EXACTLY that row set.
// ===========================================================================

/// **fix-2 [P2] — write-path gate + promote.** A node created `Pending` is
/// quarantined out of the canonical read model, so its declared attributes must
/// NOT reach the EAV / property-FTS store (the backfill's `state = 'active'`
/// rule). Pre-fix the write path projected UNCONDITIONALLY, so a same-session
/// property filter / property-FTS saw a pending node's values. Promotion
/// (`pending → active`) must then PROJECT the withheld attribute. Asserted on the
/// RAW tables (property is at rest), mid-session (WAL readers see committed data).
#[test]
fn pending_node_is_not_projected_until_promoted() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "pending_gate");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    engine
        .configure_projections(
            &[spec(
                "status",
                &[ProjectionRole::Filterable, ProjectionRole::Searchable],
                true,
                false,
            )],
            &[],
        )
        .unwrap();

    // A PENDING node carrying a declared attribute.
    engine
        .write(&[node_state("P", "src:p", r#"{"status":"quarantined"}"#, InitialState::Pending)])
        .unwrap();
    engine.drain(5_000).unwrap();

    // fix-2: a pending node must NOT project — it is hidden by the canonical read
    // model, and the property store must not surface it.
    assert!(
        eav_values(&path, "status").is_empty(),
        "a pending node must NOT project attributes into canonical_attributes"
    );
    assert_eq!(
        property_fts_rowcount(&path, "status"),
        0,
        "a pending node must NOT populate property_search_index"
    );

    // Promote: pending → active projects the previously-withheld attribute.
    engine.transition("P", LifecycleState::Active, None).unwrap();
    engine.drain(5_000).unwrap();
    let cursor = active_cursor(&path, "P");

    assert_eq!(
        eav_values(&path, "status"),
        vec!["quarantined".to_string()],
        "promotion (pending → active) must project the withheld attribute"
    );
    assert_eq!(
        eav_filter(&path, "status", "quarantined"),
        vec![cursor],
        "promoted node's attribute must be filter-matchable"
    );
    assert_eq!(
        property_fts_match(&path, "status", "quarantined"),
        vec![cursor],
        "promotion must populate property_search_index"
    );

    opened.engine.close().unwrap();
}

/// **fix-2 [P2] — soft-delete purge + undelete re-project.** A node written
/// `Active` projects at write (the control — non-vacuous). A soft-delete
/// (`active → deleted`) takes it OUT of the backfill's `state = 'active'` set, so
/// its attribute / property-FTS rows must be PURGED at rest (the property tables
/// carry no read-side state filter, exactly as with supersession in fix-1). An
/// undelete (`deleted → active`) must RE-PROJECT. Asserted on the RAW tables,
/// mid-session.
#[test]
fn active_delete_purges_and_undelete_reprojects() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "delete_undelete");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    engine
        .configure_projections(
            &[spec(
                "status",
                &[ProjectionRole::Filterable, ProjectionRole::Searchable],
                true,
                false,
            )],
            &[],
        )
        .unwrap();

    // CONTROL: an Active node projects at write (non-vacuous).
    engine.write(&[node("A", "src:a", r#"{"status":"live"}"#)]).unwrap();
    engine.drain(5_000).unwrap();
    let cursor = active_cursor(&path, "A");
    assert_eq!(
        eav_values(&path, "status"),
        vec!["live".to_string()],
        "control: an active node projects at write"
    );
    assert_eq!(
        property_fts_match(&path, "status", "live"),
        vec![cursor],
        "control: an active node populates property_search_index at write"
    );

    // Soft-delete: active → deleted must PURGE the attribute / property-FTS rows.
    engine.transition("A", LifecycleState::Deleted, Some("removed".to_string())).unwrap();
    engine.drain(5_000).unwrap();
    assert!(
        eav_values(&path, "status").is_empty(),
        "soft-delete (active → deleted) must purge canonical_attributes rows"
    );
    assert_eq!(
        property_fts_rowcount(&path, "status"),
        0,
        "soft-delete must purge property_search_index rows"
    );

    // Undelete: deleted → active must RE-PROJECT.
    engine.transition("A", LifecycleState::Active, None).unwrap();
    engine.drain(5_000).unwrap();
    let cursor = active_cursor(&path, "A");
    assert_eq!(
        eav_values(&path, "status"),
        vec!["live".to_string()],
        "undelete (deleted → active) must re-project the attribute"
    );
    assert_eq!(
        property_fts_match(&path, "status", "live"),
        vec![cursor],
        "undelete must re-populate property_search_index"
    );

    opened.engine.close().unwrap();
}

/// **fix-2 [P2] — reject is a no-op for the projection.** A pending node was
/// never projected (write-path gate), so rejecting it (`pending → deleted`) has
/// nothing to purge and must leave the projection empty (no spurious row, no
/// error). Confirms the unified rule (`projected ⟺ to_state == active`) is safe
/// on the never-projected arm.
#[test]
fn pending_reject_projects_nothing() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "reject");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    engine
        .configure_projections(&[spec("status", &[ProjectionRole::Filterable], false, false)], &[])
        .unwrap();

    engine
        .write(&[node_state("P", "src:p", r#"{"status":"spam"}"#, InitialState::Pending)])
        .unwrap();
    engine.drain(5_000).unwrap();
    assert!(eav_values(&path, "status").is_empty(), "pending write projects nothing");

    engine.transition("P", LifecycleState::Deleted, Some("rejected".to_string())).unwrap();
    engine.drain(5_000).unwrap();
    assert!(
        eav_values(&path, "status").is_empty(),
        "reject (pending → deleted) must leave the projection empty"
    );

    opened.engine.close().unwrap();
}

/// **fix-4 finding 1 [P2].** A projection `name` that `configure_projections`
/// ACCEPTS must be populatable. A name carrying a BACKSLASH was accepted by the
/// name validator (which guarded only empty / `"` / NUL) yet the write-path JSON
/// path `$."<name>"` never matched the body key, so the attribute silently NEVER
/// populated `canonical_attributes` — an accept-then-never-populate footgun.
///
/// Chosen fix = REJECT at the boundary: a name that cannot be safely used as a
/// SQLite JSON-path double-quoted key is refused with a typed
/// [`EngineError::InvalidArgument`] naming the offending name. The `drop` list is
/// validated by the same rule, so this asserts BOTH arms. RED against pre-fix-4
/// code (which returned `Ok`).
#[test]
fn backslash_projection_name_is_rejected() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "backslash_name");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    // A backslash-bearing name is refused at the spec boundary.
    let err = engine
        .configure_projections(&[spec("a\\b", &[ProjectionRole::Filterable], false, false)], &[])
        .unwrap_err();
    match err {
        EngineError::InvalidArgument { msg } => {
            assert!(
                msg.contains("a\\b"),
                "the typed refusal must name the offending projection name, got: {msg}"
            );
        }
        other => panic!("expected InvalidArgument for a backslash name, got {other:?}"),
    }

    // The same rule guards the `drop` list.
    let drop_err = engine.configure_projections(&[], &["c\\d".to_string()]).unwrap_err();
    assert!(
        matches!(drop_err, EngineError::InvalidArgument { .. }),
        "a backslash drop name must be refused too, got {drop_err:?}"
    );

    // Nothing was registered by the refused calls.
    assert!(
        engine.read_projections().unwrap().is_empty(),
        "a refused unsafe-name config must not partially register"
    );

    opened.engine.close().unwrap();
}
