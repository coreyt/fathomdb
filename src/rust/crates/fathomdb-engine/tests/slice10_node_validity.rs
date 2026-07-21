//! 0.8.20 Slice 10b — R-20-NV: node validity (world-time).
//!
//! Binds `dev/plans/plan-0.8.20.md` §3 R-20-NV: *validity-window matrix;
//! `crossed_boundary_since` hook; **world-time only (`history_as_of` explicitly
//! OUT)***.
//!
//! Four load-bearing properties:
//!
//! - **P1 half-open window** — `[valid_from, valid_until)`: lower bound
//!   INCLUSIVE, upper bound EXCLUSIVE, NULL meaning UNBOUNDED on that side. The
//!   matrix walks both boundaries and both unbounded sides.
//! - **P2 the `:now` seam is BOUND** — `valid_as_of` is a bound parameter, not a
//!   `datetime('now')` SQL literal, so a fixed instant gives a fixed answer.
//!   Asserted by determinism across repeated calls and across wall-clock drift,
//!   which is the only externally observable consequence of the seam.
//! - **P3 no default drift** — a NULL/NULL row (every row predating schema step
//!   22) is valid at every instant, so the validity conjunct cannot change what
//!   the default view returns for pre-existing data.
//! - **P4 world-time only** — there is no transaction-time selector. The view
//!   answers "was this true in the world at T", never "did the database believe
//!   this at T".
//!
//! Windows are set by DIRECT SQL on a closed database, because this slice
//! deliberately ships NO write-side surface for authoring validity windows
//! (R-20-NV specifies the columns, the read selector and the boundary hook; a
//! write verb is not in scope and inventing one would grow the governed
//! surface). Setting them at rest is also the stricter oracle: the assertions
//! below read back against the raw table rather than through a second engine
//! call.

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

fn node(kind: &str, body: &str, logical_id: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: kind.to_string(),
        body: body.to_string(),
        source_id: SourceId::new("test:fixture").expect("test source id"),
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

fn edge(from: &str, to: &str, logical_id: &str) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: SourceId::new("test:fixture").expect("test source id"),
        logical_id: Some(logical_id.to_string()),
        body: None,
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

fn db_path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

/// Set a node's validity window directly on the closed database. The engine
/// ships no write verb for this (deliberately out of scope for the slice), and
/// writing at rest keeps the assertion honest: what is read back is what is on
/// disk, not what an engine call claimed to store.
fn set_window(path: &Path, logical_id: &str, from: Option<i64>, until: Option<i64>) {
    let conn = rusqlite::Connection::open(path).expect("open for window fixture");
    conn.execute(
        "UPDATE canonical_nodes SET valid_from = ?2, valid_until = ?3
         WHERE logical_id = ?1 AND superseded_at IS NULL",
        rusqlite::params![logical_id, from, until],
    )
    .expect("set validity window");
}

/// Read a window back from the raw table — the data-at-rest oracle.
fn window_of(path: &Path, logical_id: &str) -> (Option<i64>, Option<i64>) {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only");
    conn.query_row(
        "SELECT valid_from, valid_until FROM canonical_nodes
         WHERE logical_id = ?1 AND superseded_at IS NULL",
        [logical_id],
        |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?)),
    )
    .expect("row present")
}

fn at(instant: i64) -> ReadView {
    ReadView { valid_as_of: Some(instant), ..ReadView::default() }
}

fn ids(rows: &[NodeRecord]) -> Vec<String> {
    let mut out: Vec<String> = rows.iter().map(|r| r.logical_id.clone()).collect();
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// P1 — the validity-window matrix
// ---------------------------------------------------------------------------

/// **R-20-NV keystone: the validity-window matrix.** A bounded window
/// `[1000, 2000)` walked across both boundaries, plus both unbounded variants
/// and the fully-unbounded row, on `read_list`.
#[test]
fn r_20_nv_validity_window_matrix() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "nv_matrix");
    {
        let opened = Engine::open(path.clone()).unwrap();
        opened
            .engine
            .write(&[
                node("doc", "bounded", "BOUNDED"),
                node("doc", "open-start", "OPEN_START"),
                node("doc", "open-end", "OPEN_END"),
                node("doc", "unbounded", "UNBOUNDED"),
            ])
            .expect("seed");
        opened.engine.drain(5_000).expect("drain");
        opened.engine.close().expect("close");
    }
    set_window(&path, "BOUNDED", Some(1000), Some(2000));
    set_window(&path, "OPEN_START", None, Some(2000));
    set_window(&path, "OPEN_END", Some(1000), None);
    // UNBOUNDED keeps NULL/NULL from the migration.

    // Fixture is real, verified at rest.
    assert_eq!(window_of(&path, "BOUNDED"), (Some(1000), Some(2000)));
    assert_eq!(window_of(&path, "UNBOUNDED"), (None, None));

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    // (instant, expected visible ids) — the matrix.
    let matrix: [(i64, Vec<&str>); 6] = [
        (500, vec!["OPEN_START", "UNBOUNDED"]),
        (999, vec!["OPEN_START", "UNBOUNDED"]),
        // Lower bound INCLUSIVE: BOUNDED and OPEN_END switch on exactly at 1000.
        (1000, vec!["BOUNDED", "OPEN_END", "OPEN_START", "UNBOUNDED"]),
        (1999, vec!["BOUNDED", "OPEN_END", "OPEN_START", "UNBOUNDED"]),
        // Upper bound EXCLUSIVE: BOUNDED and OPEN_START switch off exactly at 2000.
        (2000, vec!["OPEN_END", "UNBOUNDED"]),
        (5000, vec!["OPEN_END", "UNBOUNDED"]),
    ];

    for (instant, expected) in matrix {
        let got = ids(&engine.read_list("doc", &[], 100, &at(instant)).unwrap());
        let mut want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        want.sort();
        assert_eq!(
            got, want,
            "validity matrix at instant {instant}: half-open [valid_from, valid_until) \
             means lower-INCLUSIVE and upper-EXCLUSIVE"
        );
    }
}

/// The window filters ALL FIVE read verbs, not just the listing ones.
#[test]
fn r_20_nv_validity_applies_to_every_read_verb() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "nv_all_verbs");
    {
        let opened = Engine::open(path.clone()).unwrap();
        opened
            .engine
            .write(&[node("doc", "root", "R"), node("doc", "expired", "X"), edge("R", "X", "E-RX")])
            .expect("seed");
        opened.engine.drain(5_000).expect("drain");
        opened.engine.close().expect("close");
    }
    set_window(&path, "X", Some(1000), Some(2000));

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    // Inside the window: X is visible everywhere.
    assert!(engine.read_get("X", &at(1500)).unwrap().is_some(), "read_get inside window");
    assert!(
        engine.read_get_many(&["X".into()], &at(1500)).unwrap()[0].is_some(),
        "read_get_many inside window"
    );
    assert_eq!(ids(&engine.read_list("doc", &[], 100, &at(1500)).unwrap()), vec!["R", "X"]);
    assert_eq!(
        ids(&engine
            .read_list_filter("doc", &fathomdb_engine::Filter { terms: vec![] }, 100, &at(1500))
            .unwrap()),
        vec!["R", "X"]
    );
    assert_eq!(
        ids(&engine.graph_neighbors("R", 1, TraversalDirection::Outgoing, &at(1500)).unwrap()),
        vec!["X"],
        "graph_neighbors inside window"
    );

    // Outside the window: X vanishes from every verb; R (unbounded) never does.
    assert!(engine.read_get("X", &at(5000)).unwrap().is_none(), "read_get outside window");
    assert!(
        engine.read_get_many(&["X".into()], &at(5000)).unwrap()[0].is_none(),
        "read_get_many outside window"
    );
    assert_eq!(ids(&engine.read_list("doc", &[], 100, &at(5000)).unwrap()), vec!["R"]);
    assert_eq!(
        ids(&engine
            .read_list_filter("doc", &fathomdb_engine::Filter { terms: vec![] }, 100, &at(5000))
            .unwrap()),
        vec!["R"]
    );
    assert!(
        engine.graph_neighbors("R", 1, TraversalDirection::Outgoing, &at(5000)).unwrap().is_empty(),
        "graph_neighbors outside window"
    );
}

/// Validity must apply at every BFS position in every direction — the same
/// uniformity requirement R-20-RV carries for the existence axis.
#[test]
fn r_20_nv_validity_applies_at_every_bfs_position_and_direction() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "nv_bfs");
    {
        let opened = Engine::open(path.clone()).unwrap();
        opened
            .engine
            .write(&[
                node("doc", "root", "R"),
                node("doc", "middle", "M"),
                node("doc", "far", "F"),
                edge("R", "M", "E-RM"),
                edge("M", "F", "E-MF"),
            ])
            .expect("seed");
        opened.engine.drain(5_000).expect("drain");
        opened.engine.close().expect("close");
    }
    // Only the INTERMEDIATE node is window-bounded — proves the recursive join.
    set_window(&path, "M", Some(1000), Some(2000));

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    assert_eq!(
        ids(&engine.graph_neighbors("R", 3, TraversalDirection::Outgoing, &at(1500)).unwrap()),
        vec!["F", "M"],
        "inside the window the frontier expands through M to F"
    );
    assert!(
        engine.graph_neighbors("R", 3, TraversalDirection::Outgoing, &at(5000)).unwrap().is_empty(),
        "outside the window M is invalid, so the RECURSIVE JOIN must block the frontier and \
         F must be unreachable too"
    );

    // Anchor position: a window-bounded ROOT is unresolvable outside its window,
    // in every direction.
    set_window(&path, "R", Some(1000), Some(2000));
    for direction in
        [TraversalDirection::Outgoing, TraversalDirection::Incoming, TraversalDirection::Both]
    {
        assert!(
            engine.graph_neighbors("R", 2, direction, &at(5000)).unwrap().is_empty(),
            "{direction:?}: a root outside its validity window must not resolve at the ANCHOR"
        );
    }
}

/// `include_out_of_window` relaxes validity entirely, and composes with the
/// existence flags without disturbing them.
#[test]
fn r_20_nv_include_out_of_window_relaxes_validity_only() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "nv_relax");
    {
        let opened = Engine::open(path.clone()).unwrap();
        opened
            .engine
            .write(&[node("doc", "expired", "X"), node("doc", "gone", "G")])
            .expect("seed");
        opened.engine.transition("G", LifecycleState::Deleted, Some("t".into())).expect("delete G");
        opened.engine.drain(5_000).expect("drain");
        opened.engine.close().expect("close");
    }
    set_window(&path, "X", Some(1000), Some(2000));

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    assert!(
        engine.read_list("doc", &[], 100, &at(5000)).unwrap().is_empty(),
        "at 5000 X is out of window and G is deleted"
    );

    // Relax validity only: X returns, G stays hidden (existence untouched).
    let out_of_window =
        ReadView { include_out_of_window: true, valid_as_of: Some(5000), ..ReadView::default() };
    assert_eq!(
        ids(&engine.read_list("doc", &[], 100, &out_of_window).unwrap()),
        vec!["X"],
        "include_out_of_window must relax VALIDITY ONLY — the deleted node must stay hidden"
    );

    // Relax both axes: everything.
    let widest = ReadView {
        include_superseded: true,
        include_inactive: true,
        include_out_of_window: true,
        valid_as_of: None,
    };
    assert_eq!(ids(&engine.read_list("doc", &[], 100, &widest).unwrap()), vec!["G", "X"]);
}

// ---------------------------------------------------------------------------
// P2 — the `:now` seam is a bound parameter
// ---------------------------------------------------------------------------

/// The `:now` seam: a FIXED `valid_as_of` gives a FIXED answer, repeatedly and
/// across wall-clock advance. If the instant were inlined as `datetime('now')`
/// the caller's instant could not govern the result at all.
#[test]
fn r_20_nv_valid_as_of_is_bound_so_results_are_deterministic() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "nv_bound");
    {
        let opened = Engine::open(path.clone()).unwrap();
        opened.engine.write(&[node("doc", "historic", "H")]).expect("seed");
        opened.engine.drain(5_000).expect("drain");
        opened.engine.close().expect("close");
    }
    // A window entirely in the PAST relative to any real wall clock: 1970.
    set_window(&path, "H", Some(1000), Some(2000));

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    // A caller-supplied instant INSIDE the historic window sees the row, every
    // time — a `datetime('now')` literal could never produce this.
    for _ in 0..5 {
        assert_eq!(
            ids(&engine.read_list("doc", &[], 100, &at(1500)).unwrap()),
            vec!["H"],
            "a bound past instant must deterministically select the historic window"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    // And the default (now) instant, which is far past 2000, never sees it.
    assert!(
        engine.read_list("doc", &[], 100, &ReadView::default()).unwrap().is_empty(),
        "the default instant resolves to wall-clock now, which is outside the 1970 window"
    );
}

// ---------------------------------------------------------------------------
// P3 — no default drift for pre-existing (NULL/NULL) rows
// ---------------------------------------------------------------------------

/// **The no-drift assertion.** Every row written without a window carries
/// NULL/NULL at rest and is therefore visible under the default view — the
/// state of every row in every shipped database before step 22.
#[test]
fn r_20_nv_unbounded_rows_are_visible_in_the_default_view() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "nv_no_drift");
    {
        let opened = Engine::open(path.clone()).unwrap();
        opened
            .engine
            .write(&[node("doc", "a", "A"), node("doc", "b", "B"), node("doc", "c", "C")])
            .expect("seed");
        opened.engine.drain(5_000).expect("drain");
        opened.engine.close().expect("close");
    }

    // Data-at-rest: no window was authored, so every row must be NULL/NULL.
    for id in ["A", "B", "C"] {
        assert_eq!(
            window_of(&path, id),
            (None, None),
            "a row written with no window must be unbounded on both sides at rest"
        );
    }

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    assert_eq!(
        ids(&engine.read_list("doc", &[], 100, &ReadView::default()).unwrap()),
        vec!["A", "B", "C"],
        "the validity conjunct must be a NO-OP on unbounded rows in the default view"
    );

    // ...at any instant the caller cares to name, including extremes.
    for instant in [i64::MIN, 0, 1, 4_102_444_800, i64::MAX] {
        assert_eq!(
            ids(&engine.read_list("doc", &[], 100, &at(instant)).unwrap()),
            vec!["A", "B", "C"],
            "unbounded rows must be valid at instant {instant}"
        );
    }
}

// ---------------------------------------------------------------------------
// P1/P4 — the crossed_boundary_since hook
// ---------------------------------------------------------------------------

/// The `crossed_boundary_since` hook reports nodes that entered or left validity
/// inside `(since, as_of]`, and says WHICH boundary was crossed.
#[test]
fn r_20_nv_crossed_boundary_since_reports_both_boundaries() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "nv_boundary");
    {
        let opened = Engine::open(path.clone()).unwrap();
        opened
            .engine
            .write(&[
                node("doc", "opened", "OPENED"),
                node("doc", "closed", "CLOSED"),
                node("doc", "both", "BOTH"),
                node("doc", "outside", "OUTSIDE"),
                node("doc", "unbounded", "UNBOUNDED"),
            ])
            .expect("seed");
        opened.engine.drain(5_000).expect("drain");
        opened.engine.close().expect("close");
    }
    // Interrogated interval will be (1000, 2000].
    set_window(&path, "OPENED", Some(1500), None); // became valid inside
    set_window(&path, "CLOSED", Some(0), Some(1500)); // became invalid inside
    set_window(&path, "BOTH", Some(1200), Some(1800)); // both inside
    set_window(&path, "OUTSIDE", Some(5000), Some(6000)); // neither inside
                                                          // UNBOUNDED stays NULL/NULL — cannot cross either boundary.

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    // `as_of` = the view's instant = 2000.
    let crossings = engine
        .crossed_boundary_since(
            1000,
            &ReadView {
                // Look at all rows regardless of whether they are valid NOW.
                include_out_of_window: false,
                valid_as_of: Some(2000),
                ..ReadView::default()
            },
        )
        .expect("crossed_boundary_since");

    let mut got: Vec<(String, Option<i64>, Option<i64>)> = crossings
        .iter()
        .map(|c| (c.node.logical_id.clone(), c.became_valid_at, c.became_invalid_at))
        .collect();
    got.sort();

    assert_eq!(
        got,
        vec![
            ("BOTH".to_string(), Some(1200), Some(1800)),
            ("CLOSED".to_string(), None, Some(1500)),
            ("OPENED".to_string(), Some(1500), None),
        ],
        "the hook must report exactly the nodes crossing a boundary in (1000, 2000], each \
         carrying the boundary(ies) it crossed; a window that opened AND closed inside the \
         interval reports both"
    );
}

/// Boundary crossings outside the interval are excluded, and the interval is
/// half-open below / closed above — `(since, as_of]`.
#[test]
fn r_20_nv_crossed_boundary_interval_is_since_exclusive_as_of_inclusive() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "nv_boundary_edges");
    {
        let opened = Engine::open(path.clone()).unwrap();
        opened
            .engine
            .write(&[node("doc", "at-since", "AT_SINCE"), node("doc", "at-asof", "AT_ASOF")])
            .expect("seed");
        opened.engine.drain(5_000).expect("drain");
        opened.engine.close().expect("close");
    }
    set_window(&path, "AT_SINCE", Some(1000), None); // exactly at `since`
    set_window(&path, "AT_ASOF", Some(2000), None); // exactly at `as_of`

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;
    let crossings = engine
        .crossed_boundary_since(1000, &ReadView { valid_as_of: Some(2000), ..ReadView::default() })
        .expect("crossed_boundary_since");
    let got: Vec<String> = crossings.iter().map(|c| c.node.logical_id.clone()).collect();

    assert_eq!(
        got,
        vec!["AT_ASOF".to_string()],
        "the interval is (since, as_of]: a boundary exactly at `since` is EXCLUDED, one \
         exactly at `as_of` is INCLUDED"
    );
}

/// A row with no window can never cross a boundary — so the hook is silent on
/// every pre-step-22 row, exactly as the no-drift property requires.
#[test]
fn r_20_nv_crossed_boundary_ignores_unbounded_rows() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "nv_boundary_unbounded");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;
    engine.write(&[node("doc", "a", "A"), node("doc", "b", "B")]).expect("seed");
    engine.drain(5_000).expect("drain");

    let crossings = engine
        .crossed_boundary_since(
            i64::MIN,
            &ReadView { valid_as_of: Some(i64::MAX), ..ReadView::default() },
        )
        .expect("crossed_boundary_since");
    assert!(
        crossings.is_empty(),
        "unbounded rows cannot cross a boundary even over the widest possible interval; got {:?}",
        crossings.iter().map(|c| &c.node.logical_id).collect::<Vec<_>>()
    );
}

/// The hook honours the view's EXISTENCE flags (a deleted node is out by
/// default, in with `include_inactive`) but NOT its validity conjunct — the
/// question is about crossings, not about being valid right now.
#[test]
fn r_20_nv_crossed_boundary_honours_existence_flags() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "nv_boundary_existence");
    {
        let opened = Engine::open(path.clone()).unwrap();
        opened
            .engine
            .write(&[node("doc", "live", "LIVE"), node("doc", "gone", "GONE")])
            .expect("seed");
        opened
            .engine
            .transition("GONE", LifecycleState::Deleted, Some("t".into()))
            .expect("delete");
        opened.engine.drain(5_000).expect("drain");
        opened.engine.close().expect("close");
    }
    set_window(&path, "LIVE", Some(1500), None);
    set_window(&path, "GONE", Some(1500), None);

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    let default_view = ReadView { valid_as_of: Some(2000), ..ReadView::default() };
    let got: Vec<String> = engine
        .crossed_boundary_since(1000, &default_view)
        .unwrap()
        .iter()
        .map(|c| c.node.logical_id.clone())
        .collect();
    assert_eq!(got, vec!["LIVE".to_string()], "the default view excludes the deleted node");

    let relaxed =
        ReadView { include_inactive: true, valid_as_of: Some(2000), ..ReadView::default() };
    let mut got: Vec<String> = engine
        .crossed_boundary_since(1000, &relaxed)
        .unwrap()
        .iter()
        .map(|c| c.node.logical_id.clone())
        .collect();
    got.sort();
    assert_eq!(
        got,
        vec!["GONE".to_string(), "LIVE".to_string()],
        "include_inactive must widen the hook's candidate set too"
    );
}

// ---------------------------------------------------------------------------
// P4 — world-time only
// ---------------------------------------------------------------------------

/// R-20-NV is world-time ONLY. `ReadView` must expose no transaction-time
/// selector: the type has exactly the four documented fields and nothing that
/// could answer "what did the database believe at T".
///
/// This is a compile-time pin — the exhaustive struct literal below stops
/// compiling the moment a fifth field (e.g. `history_as_of`) is added, forcing
/// that decision back through review rather than letting it arrive silently.
#[test]
fn r_20_nv_read_view_has_no_transaction_time_selector() {
    let ReadView {
        include_superseded: _,
        include_inactive: _,
        include_out_of_window: _,
        valid_as_of: _,
    } = ReadView::default();

    // The default is the strict view on every axis.
    let default = ReadView::default();
    assert!(!default.include_superseded);
    assert!(!default.include_inactive);
    assert!(!default.include_out_of_window);
    assert_eq!(default.valid_as_of, None, "None resolves to now, it is not a stored instant");
}

/// The edge temporal columns keep their ISO-8601 TEXT semantics: this slice
/// changes node validity only, and must not have disturbed edge traversal.
#[test]
fn r_20_nv_edge_temporal_filter_is_unchanged() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "nv_edge_untouched");
    let opened = Engine::open(path).unwrap();
    let engine = &opened.engine;
    engine
        .write(&[
            node("doc", "root", "R"),
            node("doc", "live", "L"),
            node("doc", "expired", "X"),
            edge("R", "L", "E-RL"),
            PreparedWrite::Edge {
                kind: "link".to_string(),
                from: "R".to_string(),
                to: "X".to_string(),
                source_id: SourceId::new("test:fixture").unwrap(),
                logical_id: Some("E-RX".to_string()),
                body: None,
                t_valid: None,
                // ISO-8601 TEXT, in the past — the shipped edge semantics.
                t_invalid: Some(946_684_800), // TC-33 epoch: 2000-01-01T00:00:00Z
                confidence: None,
                extractor_model_id: None,
                temporal_fallback: None,
            },
        ])
        .expect("seed");
    engine.drain(5_000).expect("drain");

    assert_eq!(
        ids(&engine
            .graph_neighbors("R", 1, TraversalDirection::Outgoing, &ReadView::default())
            .unwrap()),
        vec!["L"],
        "an edge whose ISO-8601 t_invalid is in the past must still not be traversed — the \
         node-validity work must not have disturbed the edge temporal filter"
    );
}
