//! OPP-12 record-lifecycle Phase-1 (0.8.19 Slice 5) — existence-axis tests.
//!
//! Covers `dev/design/0.8.19-slice-0-opp12-phase1-design.md` §2 (existence-axis
//! state machine) + `dev/plans/plan-0.8.19.md` §2 (R-EX-1/R-EX-2):
//!   * R-EX-1 — `PreparedWrite::Node` carries a create-time `state`
//!     (`InitialState ∈ {pending, active}`) + advisory `reason`; both round-trip
//!     to `canonical_nodes`. `deleted`/`purged` are NOT creatable (the create-time
//!     subset is the typed rejection).
//!   * R-EX-2 — default reads (search / read.* / graph traversal) are
//!     `active`-only. A `pending` node (the only creatable non-active state) is
//!     absent from every default read path; an `active` node is present; the
//!     filter is a NO-OP on an all-active corpus (eu7 no-op basis).
//!
//! Slice 5 has no `transition`/`purge` verbs (Slice 10), so `pending` — the
//! create-time non-active state — is the exclusion probe for the `deleted` case
//! (both are excluded by the identical `state = 'active'` predicate).

use std::path::Path;
use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, InitialState, LifecycleState, PreparedWrite, TraversalDirection};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

/// A dimension-8 embedder that returns the SAME unit vector for every text, so
/// both node versions land identical embeddings in `vector_default` and the
/// phase-1 bit-KNN necessarily returns both candidate rowids (distance 0). This
/// isolates the retrieval-site exclusion (not scoring quality) as the variable
/// under test in `r_ex_2_vector_search_excludes_superseded_node_version`.
#[derive(Clone, Debug)]
struct ConstantEmbedder;

impl Embedder for ConstantEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new("const", "rev-a", 8)
    }
    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        let mut v = vec![0.0_f32; 8];
        v[0] = 1.0;
        Ok(v)
    }
}

fn active_node(kind: &str, body: &str, logical_id: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: kind.to_string(),
        body: body.to_string(),
        source_id: None,
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Active,
        reason: None,
    }
}

fn pending_node(kind: &str, body: &str, logical_id: &str, reason: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: kind.to_string(),
        body: body.to_string(),
        source_id: None,
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Pending,
        reason: Some(reason.to_string()),
    }
}

fn open(name: &str) -> (TempDir, fathomdb_engine::OpenedEngine) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    let opened = Engine::open(path).unwrap();
    (dir, opened)
}

fn read_state_reason(path: &Path, logical_id: &str) -> (String, Option<String>) {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only");
    conn.query_row(
        "SELECT state, reason FROM canonical_nodes \
         WHERE logical_id = ?1 AND superseded_at IS NULL",
        [logical_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
    )
    .expect("row present")
}

/// R-EX-1 — the create-time `state`/`reason` round-trip to `canonical_nodes`.
#[test]
fn r_ex_1_state_and_reason_round_trip() {
    let (dir, opened) = open("rex1_roundtrip");
    let path = dir.path().join(format!("rex1_roundtrip{SQLITE_SUFFIX}"));
    let engine = &opened.engine;

    engine.write(&[active_node("doc", "active body", "act1")]).expect("write active");
    engine
        .write(&[pending_node("doc", "pending body", "pen1", "awaiting-review")])
        .expect("write pending");

    // Active node lands state='active', reason NULL (the back-compat default,
    // value-identical to the migration step-20 column DEFAULT).
    assert_eq!(read_state_reason(&path, "act1"), ("active".to_string(), None));
    // Pending node lands state='pending' with its advisory reason stored verbatim.
    assert_eq!(
        read_state_reason(&path, "pen1"),
        ("pending".to_string(), Some("awaiting-review".to_string()))
    );
}

/// R-EX-1 — `deleted`/`purged` are NOT creatable: the create-time subset
/// ([`InitialState`]) is the typed rejection (they are unrepresentable), while the
/// full [`LifecycleState`] still round-trips its on-disk spelling.
#[test]
fn r_ex_1_deleted_and_purged_not_creatable() {
    assert_eq!(InitialState::from_create_str("active"), Some(InitialState::Active));
    assert_eq!(InitialState::from_create_str("pending"), Some(InitialState::Pending));
    // The delete-family + unknowns are NOT a create-time state → typed rejection.
    assert_eq!(InitialState::from_create_str("deleted"), None);
    assert_eq!(InitialState::from_create_str("purged"), None);
    assert_eq!(InitialState::from_create_str("bogus"), None);

    // On-disk spelling parity for the full existence axis.
    assert_eq!(InitialState::Active.as_str(), "active");
    assert_eq!(InitialState::Pending.as_str(), "pending");
    assert_eq!(LifecycleState::Deleted.as_str(), "deleted");
    assert_eq!(LifecycleState::Purged.as_str(), "purged");
    assert_eq!(LifecycleState::from_str_opt("deleted"), Some(LifecycleState::Deleted));
    assert_eq!(LifecycleState::from_str_opt("nope"), None);
    // The create-time subset is a strict subset of the full axis.
    assert_eq!(InitialState::Pending.to_lifecycle_state(), LifecycleState::Pending);
    assert_eq!(InitialState::Active.to_lifecycle_state(), LifecycleState::Active);
}

/// R-EX-2 — a `pending` node is absent from default `search` and `read.*`; the
/// `active` node sharing the same FTS token is present.
#[test]
fn r_ex_2_pending_absent_from_default_search_and_read() {
    let (_dir, opened) = open("rex2_exclusion");
    let engine = &opened.engine;

    // Both bodies carry the shared FTS token `zephyrunique` so both are FTS
    // candidates; only the active one may survive the `state = 'active'` filter.
    engine
        .write(&[active_node("doc", "zephyrunique active payload", "act1")])
        .expect("write active");
    engine
        .write(&[pending_node("doc", "zephyrunique pending payload", "pen1", "quarantine")])
        .expect("write pending");

    let hits = engine.search("zephyrunique").expect("search");
    let bodies: Vec<&str> = hits.results.iter().map(|h| h.body.as_str()).collect();
    assert!(
        bodies.iter().any(|b| b.contains("active payload")),
        "the active node must be returned by default search, got: {bodies:?}"
    );
    assert!(
        !bodies.iter().any(|b| b.contains("pending payload")),
        "the pending node must be EXCLUDED from default search, got: {bodies:?}"
    );

    // read.* point lookup — pending excluded, active present.
    assert!(engine.read_get("act1").expect("read_get active").is_some());
    assert!(
        engine.read_get("pen1").expect("read_get pending").is_none(),
        "read.get must not surface a pending node"
    );

    // read.list — pending excluded from the kind listing.
    let listed: Vec<String> = engine
        .read_list("doc", &[], 100)
        .expect("read_list")
        .into_iter()
        .map(|n| n.logical_id)
        .collect();
    assert!(listed.contains(&"act1".to_string()), "read.list must include the active node");
    assert!(
        !listed.contains(&"pen1".to_string()),
        "read.list must exclude the pending node, got: {listed:?}"
    );
}

/// R-EX-2 — graph traversal is `active`-only: a `pending` neighbor is excluded;
/// the identical graph with an `active` neighbor surfaces it (the no-op control).
#[test]
fn r_ex_2_graph_traversal_excludes_pending_neighbor() {
    // Pending neighbor — excluded.
    let (_d1, o1) = open("rex2_graph_pending");
    let e1 = &o1.engine;
    e1.write(&[active_node("doc", "root node", "root")]).unwrap();
    e1.write(&[pending_node("doc", "neighbor node", "nbr", "quarantine")]).unwrap();
    e1.write(&[PreparedWrite::Edge {
        kind: "link".to_string(),
        from: "root".to_string(),
        to: "nbr".to_string(),
        source_id: None,
        logical_id: Some("e1".to_string()),
        body: None,
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }])
    .unwrap();
    let neighbors: Vec<String> = e1
        .graph_neighbors("root", 1, TraversalDirection::Outgoing)
        .unwrap()
        .into_iter()
        .map(|n| n.logical_id)
        .collect();
    assert!(
        !neighbors.contains(&"nbr".to_string()),
        "a pending neighbor must be excluded from graph traversal, got: {neighbors:?}"
    );

    // No-op control: the SAME graph with an ACTIVE neighbor surfaces it.
    let (_d2, o2) = open("rex2_graph_active");
    let e2 = &o2.engine;
    e2.write(&[active_node("doc", "root node", "root")]).unwrap();
    e2.write(&[active_node("doc", "neighbor node", "nbr")]).unwrap();
    e2.write(&[PreparedWrite::Edge {
        kind: "link".to_string(),
        from: "root".to_string(),
        to: "nbr".to_string(),
        source_id: None,
        logical_id: Some("e1".to_string()),
        body: None,
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }])
    .unwrap();
    let neighbors2: Vec<String> = e2
        .graph_neighbors("root", 1, TraversalDirection::Outgoing)
        .unwrap()
        .into_iter()
        .map(|n| n.logical_id)
        .collect();
    assert!(
        neighbors2.contains(&"nbr".to_string()),
        "an active neighbor must be surfaced by graph traversal, got: {neighbors2:?}"
    );
}

/// R-EX-2 (fix-1, codex §9) — a SUPERSEDED node version must not be recalled
/// through the VECTOR search arm. Node supersession is tombstone-then-insert
/// (`commit_batch`): the prior `canonical_nodes` row is kept (same
/// `write_cursor`, `state = 'active'`, `superseded_at` set) and — unlike the
/// edge path (fix-30) — its stale `vector_default` row is NOT pruned. So the
/// phase-1 bit-KNN still surfaces the OLD cursor; the vector-arm node hydration
/// query must therefore additionally guard `superseded_at IS NULL` (co-located
/// with the pre-existing `state = 'active'`) to complete the "enforce the
/// exclusion at every retrieval site" contract (design §2). Before the guard
/// this test is RED (the stale body leaks); after it is GREEN.
#[test]
fn r_ex_2_vector_search_excludes_superseded_node_version() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("rex2_vector_supersede{SQLITE_SUFFIX}"));
    let opened =
        Engine::open_with_embedder_for_test(&path, Arc::new(ConstantEmbedder)).expect("open");
    let engine = &opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("configure vector kind");

    // v1 — the OLD version under logical_id "L"; vector-index it.
    engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "quokka original stale body".to_string(),
            source_id: None,
            logical_id: Some("L".to_string()),
            state: InitialState::Active,
            reason: None,
        }])
        .expect("write v1");
    engine.drain(10_000).expect("drain v1");

    // v2 — re-ingest the SAME logical_id with a CHANGED body (a supersession).
    // The v1 canonical row is tombstoned (superseded_at set) but its
    // vector_default row survives; v2 gets a fresh, distinct write_cursor.
    engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "quokka revised fresh body".to_string(),
            source_id: None,
            logical_id: Some("L".to_string()),
            state: InitialState::Active,
            reason: None,
        }])
        .expect("write v2");
    engine.drain(10_000).expect("drain v2");

    let hits = engine.search("quokka").expect("search");
    let bodies: Vec<&str> = hits.results.iter().map(|h| h.body.as_str()).collect();

    // The CURRENT version must be recalled (no-op control: the vector arm still
    // surfaces the live node).
    assert!(
        bodies.iter().any(|b| b.contains("revised fresh body")),
        "the current node version must be recalled via vector search, got: {bodies:?}"
    );
    // The SUPERSEDED version must NOT leak through the vector arm.
    assert!(
        !bodies.iter().any(|b| b.contains("original stale body")),
        "a superseded node version must be EXCLUDED from vector search \
         (vector hydration must guard `superseded_at IS NULL`), got: {bodies:?}"
    );

    opened.engine.close().unwrap();
}

/// R-EX-2 — the `state = 'active'` predicate is a NO-OP on an all-active corpus:
/// every default-active node is returned exactly as before the existence axis.
#[test]
fn r_ex_2_no_op_on_all_active_corpus() {
    let (_dir, opened) = open("rex2_noop");
    let engine = &opened.engine;
    for i in 0..5 {
        engine
            .write(&[active_node("doc", &format!("commonterm doc number {i}"), &format!("id{i}"))])
            .unwrap();
    }
    let hits = engine.search("commonterm").expect("search");
    assert_eq!(
        hits.results.iter().filter(|h| h.body.contains("commonterm")).count(),
        5,
        "all five active nodes must be returned (state='active' is a no-op on an all-active corpus)"
    );
}
