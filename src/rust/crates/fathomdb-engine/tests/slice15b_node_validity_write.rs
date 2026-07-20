//! 0.8.20 Slice 15b — TC-34: the node-validity WRITE-SIDE authoring path.
//!
//! Slice 10b shipped node validity read-only: `canonical_nodes.valid_from` /
//! `valid_until` (schema step 22), the `ReadView::validity_sql` predicate and
//! the `crossed_boundary_since` hook. NO write path set the columns, so the only
//! way to author a window was raw SQL — which is exactly what the Slice-10
//! suites still do. A window a caller can filter on but can never set is dead
//! surface.
//!
//! This suite closes that gap. `PreparedWrite::Node` grows two optional
//! INTEGER-epoch-second fields, `valid_from` and `valid_until`, exactly
//! symmetric with how `PreparedWrite::Edge` already carries `t_valid` /
//! `t_invalid`. ZERO new commands: fields only.
//!
//! Load-bearing properties:
//!
//! - **W1 round trip** — a window authored THROUGH the engine is visible at an
//!   instant inside it and invisible outside it. The `:now` seam
//!   (`ReadView::valid_as_of`) is a bound parameter, so every assertion below is
//!   deterministic: no wall clock, no sleep.
//! - **W2 half-open** — `[valid_from, valid_until)` survives the write path
//!   unchanged: `== valid_from` is IN, `== valid_until` is OUT.
//! - **W3 unbounded sides** — one bound set and the other omitted means
//!   unbounded on the omitted side, on both sides independently.
//! - **W4 no default drift** — omitting BOTH fields lands NULL/NULL on disk, so
//!   every write that predates this slice keeps its always-valid semantics. The
//!   oracle is the RAW TABLE, not a read verb: a read-based assertion here would
//!   pass on broken code (a row wrongly written with `valid_from = 0` is ALSO
//!   visible under a default view, and the defect would only surface for a
//!   caller reading at a pre-1970 instant).
//! - **W5 typed refusal** — a window that can never match anything
//!   (`valid_from >= valid_until`) is rejected with a typed error rather than
//!   silently stored, and the refusal rejects the WHOLE batch.
//! - **W6 boundary hook end-to-end** — `crossed_boundary_since` reports a
//!   crossing for a window authored through the SDK rather than through raw SQL.
//!
//! Raw-table reads happen on a CLOSED database (the engine holds the file), so
//! every test that needs the at-rest oracle writes, drains, closes, asserts on
//! disk, then reopens for the read-path assertions — the same shape the Slice-10
//! suites use.

use fathomdb_engine::{
    Engine, EngineError, InitialState, NodeRecord, PreparedWrite, ReadView, SourceId,
};
use fathomdb_schema::SQLITE_SUFFIX;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A node write carrying an explicit validity window. `None`/`None` is the
/// back-compat shape (unbounded on both sides).
fn node_win(
    logical_id: &str,
    body: &str,
    valid_from: Option<i64>,
    valid_until: Option<i64>,
) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body.to_string(),
        source_id: SourceId::new("test:slice15b").expect("test source id"),
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Active,
        reason: None,
        valid_from,
        valid_until,
    }
}

/// A node write that omits the window entirely — the pre-slice shape.
fn node(logical_id: &str, body: &str) -> PreparedWrite {
    node_win(logical_id, body, None, None)
}

fn db_path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

/// Seed a batch on a fresh engine, drain, and CLOSE — leaving the file free for
/// the raw-table oracle.
fn seed(path: &Path, batch: &[PreparedWrite]) {
    let opened = Engine::open(path.to_path_buf()).expect("open for seed");
    opened.engine.write(batch).expect("seed write");
    opened.engine.drain(5_000).expect("drain");
    opened.engine.close().expect("close");
}

/// Read the window back from the RAW TABLE — the data-at-rest oracle. What is
/// asserted is what is on disk, not what an engine call claimed to store.
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

/// Count canonical rows carrying `logical_id` — the "nothing was written" oracle
/// for the refusal tests.
fn row_count(path: &Path, logical_id: &str) -> i64 {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only");
    conn.query_row(
        "SELECT COUNT(*) FROM canonical_nodes WHERE logical_id = ?1",
        [logical_id],
        |r| r.get(0),
    )
    .expect("count")
}

/// A read view pinned to a fixed world-time instant. This is the deterministic
/// `:now` seam — the instant is BOUND, never `datetime('now')`.
fn at(instant: i64) -> ReadView {
    ReadView { valid_as_of: Some(instant), ..ReadView::default() }
}

fn ids(rows: &[NodeRecord]) -> Vec<String> {
    let mut out: Vec<String> = rows.iter().map(|r| r.logical_id.clone()).collect();
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// W1 — the round trip
// ---------------------------------------------------------------------------

/// **TC-34 keystone.** Author a window through the engine, then read at an
/// instant INSIDE it (row present) and at instants OUTSIDE it on both sides (row
/// absent). No raw SQL sets the window; no wall clock is consulted.
#[test]
fn tc34_authored_window_round_trips_through_read_view() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc34_round_trip");
    seed(&path, &[node_win("WINDOWED", "bounded body", Some(1000), Some(2000))]);

    // The window is on disk exactly as authored — no coercion, no clock.
    assert_eq!(
        window_of(&path, "WINDOWED"),
        (Some(1000), Some(2000)),
        "authored window must land verbatim in canonical_nodes"
    );

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    // INSIDE — present on the point lookup and on the list.
    assert!(
        engine.read_get("WINDOWED", &at(1500)).unwrap().is_some(),
        "node must be visible at an instant inside its window"
    );
    assert_eq!(ids(&engine.read_list("doc", &[], 100, &at(1500)).unwrap()), vec!["WINDOWED"]);

    // OUTSIDE, before — absent.
    assert!(
        engine.read_get("WINDOWED", &at(500)).unwrap().is_none(),
        "node must be invisible before valid_from"
    );
    assert!(engine.read_list("doc", &[], 100, &at(500)).unwrap().is_empty());

    // OUTSIDE, after — absent.
    assert!(
        engine.read_get("WINDOWED", &at(2500)).unwrap().is_none(),
        "node must be invisible at/after valid_until"
    );
    assert!(engine.read_list("doc", &[], 100, &at(2500)).unwrap().is_empty());

    // The escape hatch still relaxes the conjunct entirely.
    let relaxed = ReadView { include_out_of_window: true, ..at(2500) };
    assert!(
        engine.read_get("WINDOWED", &relaxed).unwrap().is_some(),
        "include_out_of_window must still surface an out-of-window authored row"
    );
}

// ---------------------------------------------------------------------------
// W2 — half-open boundaries survive the write path
// ---------------------------------------------------------------------------

/// `[valid_from, valid_until)`: the lower bound is INCLUSIVE and the upper bound
/// EXCLUSIVE. Asserted on a window authored through the write path, so a write
/// that silently shifted a bound by one second would fail here.
#[test]
fn tc34_authored_window_is_half_open() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc34_half_open");
    seed(&path, &[node_win("HALFOPEN", "boundary body", Some(1000), Some(2000))]);

    let opened = Engine::open(path).unwrap();
    let engine = &opened.engine;

    assert!(
        engine.read_get("HALFOPEN", &at(999)).unwrap().is_none(),
        "one second before valid_from is OUT"
    );
    assert!(
        engine.read_get("HALFOPEN", &at(1000)).unwrap().is_some(),
        "exactly valid_from is IN (lower bound inclusive)"
    );
    assert!(
        engine.read_get("HALFOPEN", &at(1999)).unwrap().is_some(),
        "one second before valid_until is IN"
    );
    assert!(
        engine.read_get("HALFOPEN", &at(2000)).unwrap().is_none(),
        "exactly valid_until is OUT (upper bound exclusive)"
    );
}

// ---------------------------------------------------------------------------
// W3 — unbounded sides
// ---------------------------------------------------------------------------

/// One bound authored, the other omitted ⇒ unbounded on the omitted side. Both
/// directions, plus the raw-table oracle confirming the omitted side is NULL and
/// not some zero/sentinel coercion.
#[test]
fn tc34_authored_window_supports_unbounded_sides() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc34_unbounded");
    seed(
        &path,
        &[
            // Valid from 1000 onwards, forever.
            node_win("FROM_ONLY", "from only", Some(1000), None),
            // Valid since the beginning of time, until 2000.
            node_win("UNTIL_ONLY", "until only", None, Some(2000)),
        ],
    );

    assert_eq!(
        window_of(&path, "FROM_ONLY"),
        (Some(1000), None),
        "omitted valid_until must land NULL, not a sentinel"
    );
    assert_eq!(
        window_of(&path, "UNTIL_ONLY"),
        (None, Some(2000)),
        "omitted valid_from must land NULL, not a sentinel"
    );

    let opened = Engine::open(path).unwrap();
    let engine = &opened.engine;

    // FROM_ONLY: out before 1000, in at 1000 and at an absurdly distant instant.
    assert!(engine.read_get("FROM_ONLY", &at(999)).unwrap().is_none());
    assert!(engine.read_get("FROM_ONLY", &at(1000)).unwrap().is_some());
    assert!(engine.read_get("FROM_ONLY", &at(i64::MAX / 2)).unwrap().is_some());

    // UNTIL_ONLY: in at 0 and just before 2000, out at 2000.
    assert!(engine.read_get("UNTIL_ONLY", &at(0)).unwrap().is_some());
    assert!(engine.read_get("UNTIL_ONLY", &at(1999)).unwrap().is_some());
    assert!(engine.read_get("UNTIL_ONLY", &at(2000)).unwrap().is_none());
}

// ---------------------------------------------------------------------------
// W4 — no default drift (RAW TABLE oracle)
// ---------------------------------------------------------------------------

/// **MUST-NOT-REGRESS.** A write that omits both fields lands NULL/NULL, which
/// is what every row predating this slice carries — so the row is valid at every
/// instant and the default view's behaviour is byte-stable.
#[test]
fn tc34_omitted_window_lands_null_null_and_stays_visible() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc34_default");
    seed(&path, &[node("PLAIN", "no window authored")]);

    assert_eq!(
        window_of(&path, "PLAIN"),
        (None, None),
        "a write omitting the window MUST land NULL/NULL — not 0, not now(), not a sentinel"
    );

    let opened = Engine::open(path).unwrap();
    let engine = &opened.engine;

    // Valid at every instant, including the extremes of the bound parameter.
    for instant in [0_i64, 1, 1000, 2_000_000_000, i64::MAX] {
        assert!(
            engine.read_get("PLAIN", &at(instant)).unwrap().is_some(),
            "NULL/NULL row must be valid at instant {instant}"
        );
    }

    // And under the shipped default view (no `valid_as_of` — resolves to the
    // wall clock), which is the path every existing caller takes.
    assert!(
        engine.read_get("PLAIN", &ReadView::default()).unwrap().is_some(),
        "default-view visibility must be unchanged for a window-less write"
    );
    assert_eq!(
        ids(&engine.read_list("doc", &[], 100, &ReadView::default()).unwrap()),
        vec!["PLAIN"]
    );
}

// ---------------------------------------------------------------------------
// W5 — typed refusal of an unsatisfiable window
// ---------------------------------------------------------------------------

/// A window whose lower bound is at or above its upper bound can never match any
/// instant under a half-open predicate, so accepting it silently would write a
/// row that no default read can ever return. It is a TYPED refusal.
#[test]
fn tc34_unsatisfiable_window_is_a_typed_refusal() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc34_invalid");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    // Strictly inverted.
    let inverted = engine.write(&[node_win("BAD", "inverted", Some(2000), Some(1000))]);
    assert!(
        matches!(inverted, Err(EngineError::InvalidArgument { .. })),
        "an inverted window must be a typed InvalidArgument refusal, got {inverted:?}"
    );

    // Empty (from == until) — half-open, so this matches nothing either.
    let empty = engine.write(&[node_win("BAD", "empty", Some(1500), Some(1500))]);
    assert!(
        matches!(empty, Err(EngineError::InvalidArgument { .. })),
        "an empty half-open window must be a typed refusal, got {empty:?}"
    );

    // The refusal must name the offending values, or a caller cannot act on it.
    let Err(EngineError::InvalidArgument { msg }) =
        engine.write(&[node_win("BAD", "inverted", Some(2000), Some(1000))])
    else {
        panic!("expected InvalidArgument");
    };
    assert!(msg.contains("2000") && msg.contains("1000"), "message must name the bounds: {msg}");

    engine.close().expect("close");
    assert_eq!(row_count(&path, "BAD"), 0, "a refused write must not land a row");
}

/// The refusal rejects the WHOLE batch: validation runs before any insert, so a
/// good row sharing a batch with a bad one is not partially committed.
#[test]
fn tc34_unsatisfiable_window_rejects_the_whole_batch() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc34_batch");
    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    let result = engine
        .write(&[node("GOOD", "well formed"), node_win("BAD", "inverted", Some(2000), Some(1000))]);
    assert!(matches!(result, Err(EngineError::InvalidArgument { .. })));

    engine.close().expect("close");
    assert_eq!(row_count(&path, "GOOD"), 0, "batch rejection must not commit the sibling row");
    assert_eq!(row_count(&path, "BAD"), 0);
}

/// A single bound is never unsatisfiable — only the PAIR can be. Neither
/// one-sided form may be refused, however extreme its value.
#[test]
fn tc34_single_bound_is_never_refused() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc34_one_sided");
    let opened = Engine::open(path).unwrap();
    opened
        .engine
        .write(&[
            node_win("A", "from only", Some(i64::MAX), None),
            node_win("B", "until only", None, Some(i64::MIN)),
        ])
        .expect("a one-sided window can never be unsatisfiable and must be accepted");
}

// ---------------------------------------------------------------------------
// W6 — the boundary hook, end-to-end on an SDK-authored window
// ---------------------------------------------------------------------------

/// `crossed_boundary_since` was shipped in Slice 10b but could only ever be
/// exercised against raw-SQL fixtures. With the write path in place it works
/// end-to-end on a window authored through the engine.
#[test]
fn tc34_crossed_boundary_since_works_on_an_authored_window() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc34_crossing");
    let opened = Engine::open(path).unwrap();
    let engine = &opened.engine;

    engine
        .write(&[
            // Opens inside the interrogated interval (1000, 3000].
            node_win("OPENED", "became valid", Some(2000), None),
            // Closes inside it.
            node_win("CLOSED", "became invalid", None, Some(2500)),
            // Opens AND closes inside it — reports both boundaries.
            node_win("BOTH", "opened and closed", Some(1500), Some(2800)),
            // No window: can never cross a boundary.
            node("NEVER", "no window"),
            // Entirely outside the interval.
            node_win("OUTSIDE", "far future", Some(9000), Some(9500)),
        ])
        .unwrap();
    engine.drain(5_000).expect("drain");

    let crossings = engine.crossed_boundary_since(1000, &at(3000)).unwrap();
    let mut named: Vec<(String, Option<i64>, Option<i64>)> = crossings
        .iter()
        .map(|c| (c.node.logical_id.clone(), c.became_valid_at, c.became_invalid_at))
        .collect();
    named.sort();

    assert_eq!(
        named,
        vec![
            ("BOTH".to_string(), Some(1500), Some(2800)),
            ("CLOSED".to_string(), None, Some(2500)),
            ("OPENED".to_string(), Some(2000), None),
        ],
        "the hook must name WHICH boundary each SDK-authored window crossed"
    );
}

// ---------------------------------------------------------------------------
// Interaction with the existing axes
// ---------------------------------------------------------------------------

/// The validity window is orthogonal to G0 supersession: re-ingesting a
/// `logical_id` with a NEW window supersedes the prior row, and the new window
/// is the one the default view honours.
#[test]
fn tc34_window_is_per_version_under_supersession() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc34_supersede");
    {
        let opened = Engine::open(path.clone()).unwrap();
        opened.engine.write(&[node_win("V", "first", Some(1000), Some(2000))]).unwrap();
        opened.engine.write(&[node_win("V", "second", Some(3000), Some(4000))]).unwrap();
        opened.engine.drain(5_000).expect("drain");
        opened.engine.close().expect("close");
    }

    // The surviving active row carries the SECOND window.
    assert_eq!(window_of(&path, "V"), (Some(3000), Some(4000)));

    let opened = Engine::open(path).unwrap();
    let engine = &opened.engine;

    // The first window no longer governs the active row.
    assert!(engine.read_get("V", &at(1500)).unwrap().is_none());
    let current = engine.read_get("V", &at(3500)).unwrap().expect("active version visible");
    assert_eq!(current.body, "second");
}
