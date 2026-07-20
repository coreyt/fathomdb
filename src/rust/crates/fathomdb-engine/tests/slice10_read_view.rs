//! 0.8.20 Slice 10b — R-20-RV: `ReadView` / read modes.
//!
//! Binds `dev/plans/plan-0.8.20.md` §3 R-20-RV: *read-mode matrix test;
//! `include_superseded` returns history; **default view unchanged (no silent
//! behaviour drift)***.
//!
//! Three load-bearing properties:
//!
//! - **P1 default unchanged** — `ReadView::default()` is the STRICT view and
//!   reproduces the pre-slice result of every read verb exactly. This is the
//!   load-bearing constraint of the requirement, so it is ASSERTED (against the
//!   raw table, where the property is data-at-rest) and not assumed.
//! - **P2 relax flags compose independently** — each flag drops exactly one
//!   conjunct, and the four existence combinations produce the four distinct
//!   row sets a truth table predicts.
//! - **P3 uniform application** — the view applies on ALL FIVE read verbs, and
//!   inside `graph_neighbors` on ALL THREE directions at ALL THREE CTE
//!   positions (anchor, recursive join, final projection). "Works on `read_get`
//!   but silently not on one `graph_neighbors` direction" is the exact failure
//!   mode this requirement exists to prevent, so the matrix covers every cell
//!   rather than a representative sample.
//!
//! Validity (R-20-NV) lives in `slice10_node_validity.rs`; this file exercises
//! the existence axis and pins that the default view still sees everything it
//! saw before schema step 22 landed.

use fathomdb_engine::{
    Engine, InitialState, LifecycleState, NodeRecord, PreparedWrite, ReadView, SourceId,
    TraversalDirection,
};
use fathomdb_schema::SQLITE_SUFFIX;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn source() -> SourceId {
    SourceId::new("test:fixture").expect("test source id")
}

fn node(kind: &str, body: &str, logical_id: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: kind.to_string(),
        body: body.to_string(),
        source_id: source(),
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Active,
        reason: None,
    }
}

fn edge(from: &str, to: &str, logical_id: &str) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: source(),
        logical_id: Some(logical_id.to_string()),
        body: None,
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

fn open(name: &str) -> (TempDir, PathBuf, fathomdb_engine::OpenedEngine) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    let opened = Engine::open(path.clone()).unwrap();
    (dir, path, opened)
}

/// The four existence views, by name, for matrix assertions.
fn strict() -> ReadView {
    ReadView::default()
}
fn with_superseded() -> ReadView {
    ReadView { include_superseded: true, ..ReadView::default() }
}
fn with_inactive() -> ReadView {
    ReadView { include_inactive: true, ..ReadView::default() }
}
fn with_both() -> ReadView {
    ReadView { include_superseded: true, include_inactive: true, ..ReadView::default() }
}

fn bodies(rows: &[NodeRecord]) -> Vec<String> {
    let mut out: Vec<String> = rows.iter().map(|r| r.body.clone()).collect();
    out.sort();
    out
}

fn ids(rows: &[NodeRecord]) -> Vec<String> {
    let mut out: Vec<String> = rows.iter().map(|r| r.logical_id.clone()).collect();
    out.sort();
    out
}

/// Raw table probe — the data-at-rest oracle the view predicates are checked
/// against. Returns `(logical_id, body, superseded_at IS NULL, state)` rows.
fn raw_nodes(path: &Path) -> Vec<(Option<String>, String, bool, String)> {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only");
    let mut stmt = conn
        .prepare(
            "SELECT logical_id, body, superseded_at IS NULL, state
             FROM canonical_nodes ORDER BY write_cursor",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)? == 1,
                r.get::<_, String>(3)?,
            ))
        })
        .unwrap();
    rows.flatten().collect()
}

// ---------------------------------------------------------------------------
// P1 — the default view is unchanged
// ---------------------------------------------------------------------------

/// **R-20-RV keystone.** `ReadView::default()` reproduces the shipped
/// active-and-current-only semantics on every one of the five verbs.
///
/// The expectation is derived from the RAW TABLE (which rows are actually
/// current + active on disk), not from a second call to the engine — a
/// search-based or self-referential assertion would pass on broken code.
#[test]
fn r_20_rv_default_view_is_unchanged_on_all_five_verbs() {
    let (_dir, path, opened) = open("rv_default");
    let engine = &opened.engine;

    engine.write(&[node("doc", "alpha v1", "A"), node("doc", "beta", "B")]).expect("seed");
    // Supersede A, so a historical row exists on disk.
    engine.write(&[node("doc", "alpha v2", "A")]).expect("supersede A");
    // Take B out of the active state.
    engine.transition("B", LifecycleState::Deleted, Some("test".into())).expect("delete B");
    engine.write(&[node("doc", "gamma", "C"), edge("A", "C", "E-AC")]).expect("seed C");
    engine.drain(5_000).expect("drain");

    // Ground truth from the raw table: rows that are current AND active.
    let raw = raw_nodes(&path);
    let mut expected_visible: Vec<String> = raw
        .iter()
        .filter(|(_, _, current, state)| *current && state == "active")
        .map(|(_, body, _, _)| body.clone())
        .collect();
    expected_visible.sort();
    assert_eq!(
        expected_visible,
        vec!["alpha v2".to_string(), "gamma".to_string()],
        "fixture precondition: on disk exactly `alpha v2` and `gamma` are current+active \
         (`alpha v1` is superseded, `beta` is deleted)"
    );

    // read_get / read_get_many
    assert_eq!(
        engine.read_get("A", &strict()).unwrap().map(|r| r.body),
        Some("alpha v2".to_string()),
        "default read_get must return the CURRENT version"
    );
    assert!(
        engine.read_get("B", &strict()).unwrap().is_none(),
        "default read_get must not return a deleted node"
    );
    let many = engine.read_get_many(&["A".into(), "B".into(), "C".into()], &strict()).unwrap();
    assert_eq!(
        many.iter().map(|s| s.as_ref().map(|r| r.body.as_str())).collect::<Vec<_>>(),
        vec![Some("alpha v2"), None, Some("gamma")],
        "default read_get_many must be current+active only, in request order"
    );

    // read_list / read_list_filter
    let listed = engine.read_list("doc", &[], 100, &strict()).unwrap();
    assert_eq!(bodies(&listed), expected_visible, "default read_list must match the raw oracle");
    let filtered = engine
        .read_list_filter("doc", &fathomdb_engine::Filter { terms: vec![] }, 100, &strict())
        .unwrap();
    assert_eq!(bodies(&filtered), expected_visible, "default read_list_filter must match too");

    // graph_neighbors — A→C, both current+active.
    let neighbors =
        engine.graph_neighbors("A", 1, TraversalDirection::Outgoing, &strict()).unwrap();
    assert_eq!(
        bodies(&neighbors),
        vec!["gamma".to_string()],
        "default graph_neighbors must traverse to the current+active neighbor only"
    );
}

// ---------------------------------------------------------------------------
// P2 — the relax flags, and their composition
// ---------------------------------------------------------------------------

/// `include_superseded` returns history — the requirement's named acceptance.
/// `read_list` is the enumerating verb, so it is where history is observable.
#[test]
fn r_20_rv_include_superseded_returns_history() {
    let (_dir, _path, opened) = open("rv_history");
    let engine = &opened.engine;
    engine.write(&[node("doc", "v1", "A")]).expect("v1");
    engine.write(&[node("doc", "v2", "A")]).expect("v2");
    engine.write(&[node("doc", "v3", "A")]).expect("v3");
    engine.drain(5_000).expect("drain");

    assert_eq!(
        bodies(&engine.read_list("doc", &[], 100, &strict()).unwrap()),
        vec!["v3".to_string()],
        "the strict view sees only the current version"
    );
    assert_eq!(
        bodies(&engine.read_list("doc", &[], 100, &with_superseded()).unwrap()),
        vec!["v1".to_string(), "v2".to_string(), "v3".to_string()],
        "include_superseded must return the FULL history, not just one extra row"
    );
}

/// With `include_superseded` a `logical_id` matches several rows, so the
/// point-lookup slot must resolve DETERMINISTICALLY — to the most recent
/// version — rather than to whichever row the scan happened to reach last.
#[test]
fn r_20_rv_point_lookup_under_include_superseded_is_deterministic() {
    let (_dir, _path, opened) = open("rv_point_determinism");
    let engine = &opened.engine;
    engine.write(&[node("doc", "v1", "A")]).expect("v1");
    engine.write(&[node("doc", "v2", "A")]).expect("v2");
    engine.write(&[node("doc", "v3", "A")]).expect("v3");
    engine.drain(5_000).expect("drain");

    for _ in 0..5 {
        assert_eq!(
            engine.read_get("A", &with_superseded()).unwrap().map(|r| r.body),
            Some("v3".to_string()),
            "read_get under include_superseded must always resolve to the newest version"
        );
    }
}

/// `include_inactive` relaxes `state = 'active'` and NOTHING else.
#[test]
fn r_20_rv_include_inactive_returns_non_active_states() {
    let (_dir, _path, opened) = open("rv_inactive");
    let engine = &opened.engine;
    engine.write(&[node("doc", "kept", "A"), node("doc", "dropped", "B")]).expect("seed");
    engine.transition("B", LifecycleState::Deleted, Some("test".into())).expect("delete B");
    engine.drain(5_000).expect("drain");

    assert_eq!(
        bodies(&engine.read_list("doc", &[], 100, &strict()).unwrap()),
        vec!["kept".to_string()]
    );
    assert_eq!(
        bodies(&engine.read_list("doc", &[], 100, &with_inactive()).unwrap()),
        vec!["dropped".to_string(), "kept".to_string()],
        "include_inactive must surface the deleted node"
    );
    assert_eq!(
        engine.read_get("B", &with_inactive()).unwrap().map(|r| r.body),
        Some("dropped".to_string()),
        "include_inactive must apply on the point-lookup verb too"
    );
}

/// **The read-mode matrix.** Four existence views over a corpus holding one row
/// of each (current|superseded) × (active|inactive) class. Each flag must drop
/// exactly one conjunct, so the four views yield four distinct, predicted sets.
#[test]
fn r_20_rv_existence_flags_compose_independently() {
    let (_dir, _path, opened) = open("rv_matrix");
    let engine = &opened.engine;

    // A: current + active.
    engine.write(&[node("doc", "current-active", "A")]).expect("A");
    // B: superseded + active (v1 superseded by v2, both active).
    engine.write(&[node("doc", "superseded-active", "B")]).expect("B v1");
    engine.write(&[node("doc", "current-active-b", "B")]).expect("B v2");
    // C: current + inactive.
    engine.write(&[node("doc", "current-inactive", "C")]).expect("C");
    engine.transition("C", LifecycleState::Deleted, Some("test".into())).expect("delete C");
    engine.drain(5_000).expect("drain");

    let matrix = [
        (strict(), vec!["current-active", "current-active-b"]),
        (with_superseded(), vec!["current-active", "current-active-b", "superseded-active"]),
        (with_inactive(), vec!["current-active", "current-active-b", "current-inactive"]),
        (
            with_both(),
            vec!["current-active", "current-active-b", "current-inactive", "superseded-active"],
        ),
    ];

    for (view, expected) in matrix {
        let got = bodies(&engine.read_list("doc", &[], 100, &view).unwrap());
        let mut want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        want.sort();
        assert_eq!(
            got, want,
            "read-mode matrix cell {view:?} must yield exactly the predicted row set"
        );
    }
}

// ---------------------------------------------------------------------------
// P3 — uniform application across verbs, directions and CTE positions
// ---------------------------------------------------------------------------

/// The view must reach the BFS **anchor**: a non-active root is unresolvable in
/// the strict view and resolvable once `include_inactive` is set — in EVERY
/// direction.
#[test]
fn r_20_rv_graph_neighbors_view_applies_at_the_anchor_in_every_direction() {
    let (_dir, _path, opened) = open("rv_bfs_anchor");
    let engine = &opened.engine;
    engine
        .write(&[
            node("doc", "root", "R"),
            node("doc", "out", "O"),
            node("doc", "inc", "I"),
            edge("R", "O", "E-RO"),
            edge("I", "R", "E-IR"),
        ])
        .expect("seed");
    engine.transition("R", LifecycleState::Deleted, Some("test".into())).expect("delete root");
    engine.drain(5_000).expect("drain");

    for direction in
        [TraversalDirection::Outgoing, TraversalDirection::Incoming, TraversalDirection::Both]
    {
        assert!(
            engine.graph_neighbors("R", 2, direction, &strict()).unwrap().is_empty(),
            "{direction:?}: a non-active root must not resolve in the strict view"
        );
        assert!(
            !engine.graph_neighbors("R", 2, direction, &with_inactive()).unwrap().is_empty(),
            "{direction:?}: include_inactive must reach the BFS ANCHOR — if this passes for \
             Outgoing but not here, the relax flag is not uniform across directions"
        );
    }
}

/// The view must reach the BFS **recursive join**, not merely the anchor and
/// the final projection. Proven by traversing THROUGH a non-active intermediate
/// node: if the relax flag only reached the projection, the far node `E` would
/// still be unreachable because the frontier could never expand past `D`.
#[test]
fn r_20_rv_graph_neighbors_view_applies_at_the_recursive_join() {
    let (_dir, _path, opened) = open("rv_bfs_join");
    let engine = &opened.engine;
    engine
        .write(&[
            node("doc", "root", "R"),
            node("doc", "middle", "D"),
            node("doc", "far", "E"),
            edge("R", "D", "E-RD"),
            edge("D", "E", "E-DE"),
        ])
        .expect("seed");
    // Only the INTERMEDIATE node is non-active. Root and far node stay active.
    engine.transition("D", LifecycleState::Deleted, Some("test".into())).expect("delete middle");
    engine.drain(5_000).expect("drain");

    let strict_hops = engine
        .graph_neighbors("R", 3, TraversalDirection::Outgoing, &strict())
        .expect("strict traversal");
    assert!(
        strict_hops.is_empty(),
        "strict view: a non-active intermediate blocks the frontier, so neither D nor E is \
         reachable; got {:?}",
        bodies(&strict_hops)
    );

    let relaxed_hops = engine
        .graph_neighbors("R", 3, TraversalDirection::Outgoing, &with_inactive())
        .expect("relaxed traversal");
    assert_eq!(
        bodies(&relaxed_hops),
        vec!["far".to_string(), "middle".to_string()],
        "include_inactive must apply at the RECURSIVE JOIN: reaching `far` is only possible \
         if the frontier expanded THROUGH the non-active `middle`"
    );
}

/// The view must reach the BFS **final projection**. Proven with an ACTIVE
/// intermediate and a non-active far node: the frontier reaches the far node
/// regardless, so only the projection predicate can exclude it.
#[test]
fn r_20_rv_graph_neighbors_view_applies_at_the_final_projection() {
    let (_dir, _path, opened) = open("rv_bfs_projection");
    let engine = &opened.engine;
    engine
        .write(&[
            node("doc", "root", "R"),
            node("doc", "middle", "D"),
            node("doc", "far", "E"),
            edge("R", "D", "E-RD"),
            edge("D", "E", "E-DE"),
        ])
        .expect("seed");
    engine.transition("E", LifecycleState::Deleted, Some("test".into())).expect("delete far");
    engine.drain(5_000).expect("drain");

    assert_eq!(
        bodies(&engine.graph_neighbors("R", 3, TraversalDirection::Outgoing, &strict()).unwrap()),
        vec!["middle".to_string()],
        "strict view: the non-active far node must be excluded by the final projection"
    );
    assert_eq!(
        bodies(
            &engine
                .graph_neighbors("R", 3, TraversalDirection::Outgoing, &with_inactive())
                .unwrap()
        ),
        vec!["far".to_string(), "middle".to_string()],
        "include_inactive must apply at the FINAL PROJECTION"
    );
}

/// Full direction × view matrix on a symmetric graph, so no direction can carry
/// a stale predicate that the single-direction tests above would miss.
#[test]
fn r_20_rv_graph_neighbors_matrix_over_all_directions_and_views() {
    let (_dir, _path, opened) = open("rv_bfs_matrix");
    let engine = &opened.engine;
    engine
        .write(&[
            node("doc", "root", "R"),
            node("doc", "out-active", "OA"),
            node("doc", "out-inactive", "OI"),
            node("doc", "in-active", "IA"),
            node("doc", "in-inactive", "II"),
            edge("R", "OA", "E-R-OA"),
            edge("R", "OI", "E-R-OI"),
            edge("IA", "R", "E-IA-R"),
            edge("II", "R", "E-II-R"),
        ])
        .expect("seed");
    engine.transition("OI", LifecycleState::Deleted, Some("t".into())).expect("delete OI");
    engine.transition("II", LifecycleState::Deleted, Some("t".into())).expect("delete II");
    engine.drain(5_000).expect("drain");

    let cases = [
        (TraversalDirection::Outgoing, strict(), vec!["OA"]),
        (TraversalDirection::Outgoing, with_inactive(), vec!["OA", "OI"]),
        (TraversalDirection::Incoming, strict(), vec!["IA"]),
        (TraversalDirection::Incoming, with_inactive(), vec!["IA", "II"]),
        (TraversalDirection::Both, strict(), vec!["IA", "OA"]),
        (TraversalDirection::Both, with_inactive(), vec!["IA", "II", "OA", "OI"]),
    ];

    for (direction, view, expected) in cases {
        let got = ids(&engine.graph_neighbors("R", 1, direction, &view).unwrap());
        let mut want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        want.sort();
        assert_eq!(got, want, "graph_neighbors matrix cell ({direction:?}, {view:?})");
    }
}

/// `read_list_filter` lowers to `read_list`, so it must inherit the view rather
/// than silently dropping it on the way through the filter-lowering path.
#[test]
fn r_20_rv_read_list_filter_inherits_the_view() {
    let (_dir, _path, opened) = open("rv_filter_inherits");
    let engine = &opened.engine;
    engine.write(&[node("doc", "{\"n\":1}", "A")]).expect("v1");
    engine.write(&[node("doc", "{\"n\":2}", "A")]).expect("v2");
    engine.drain(5_000).expect("drain");

    let filter = fathomdb_engine::Filter { terms: vec![] };
    assert_eq!(
        engine.read_list_filter("doc", &filter, 100, &strict()).unwrap().len(),
        1,
        "strict view through read_list_filter"
    );
    assert_eq!(
        engine.read_list_filter("doc", &filter, 100, &with_superseded()).unwrap().len(),
        2,
        "read_list_filter must pass the view down to read_list, not drop it"
    );
}

/// A relaxed view must not disturb result SHAPE or the `limit` contract.
#[test]
fn r_20_rv_relaxed_views_respect_limit_and_record_shape() {
    let (_dir, _path, opened) = open("rv_limit");
    let engine = &opened.engine;
    for i in 0..6 {
        engine.write(&[node("doc", &format!("body {i}"), "A")]).expect("write");
    }
    engine.drain(5_000).expect("drain");

    let rows = engine.read_list("doc", &[], 3, &with_superseded()).unwrap();
    assert_eq!(rows.len(), 3, "limit must still bound a relaxed listing");
    for row in &rows {
        assert_eq!(row.logical_id, "A");
        assert!(!row.body.is_empty());
        assert!(row.write_cursor > 0);
    }
}

/// R-20-RV is an existence/validity selector ONLY. Relaxing the view must never
/// resurrect a row that no longer exists on disk.
#[test]
fn r_20_rv_relaxation_never_invents_rows_absent_from_disk() {
    let (_dir, path, opened) = open("rv_no_invention");
    let engine = &opened.engine;
    engine.write(&[node("doc", "one", "A"), node("doc", "two", "B")]).expect("seed");
    engine.write(&[node("doc", "one v2", "A")]).expect("supersede");
    engine.transition("B", LifecycleState::Deleted, Some("t".into())).expect("delete B");
    engine.drain(5_000).expect("drain");

    let on_disk = raw_nodes(&path).len();
    let widest = engine
        .read_list(
            "doc",
            &[],
            1000,
            &ReadView {
                include_superseded: true,
                include_inactive: true,
                include_out_of_window: true,
                valid_as_of: None,
            },
        )
        .unwrap();
    assert_eq!(
        widest.len(),
        on_disk,
        "the fully-relaxed view must return exactly the rows that exist in canonical_nodes \
         — no more (it is a filter, never a source)"
    );
}
