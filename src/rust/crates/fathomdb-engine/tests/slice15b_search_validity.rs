//! 0.8.20 Slice 15b fix-2 (R-20-NV / R-20-RV) — the validity window must govern
//! `search`, not just the five read verbs.
//!
//! Slice 10b scoped [`ReadView`] to `read_get` / `read_get_many` / `read_list` /
//! `read_list_filter` / `graph_neighbors` and deliberately left `search` out.
//! That was defensible only while NO SDK caller could author a window: the only
//! way to set `valid_from` / `valid_until` was raw SQL, so the gap was
//! unreachable. Slice 15b (TC-34) made authoring reachable from Rust, Python and
//! TypeScript — which turns a latent gap into a LIVE DEFECT. A caller can now
//! write a node whose window has already closed, watch `read_get` correctly hide
//! it, and still get it back from `search`.
//!
//! `dev/design/record-lifecycle-protocol/api-surface.md:50` always specified
//! `ReadView` as an optional argument on **`search`** alongside the read verbs,
//! so closing this is contract conformance, not scope creep.
//!
//! Load-bearing properties:
//!
//! - **S1 leak (the RED case)** — a node whose `valid_until` is in the PAST is
//!   not returned by a default `search`. Mirror: `valid_from` in the FUTURE.
//! - **S2 control** — a node whose window COVERS the instant IS returned, so S1
//!   cannot be vacuously green by matching nothing.
//! - **S3 no regression** — a node with a NULL/NULL window is returned by a
//!   default `search` exactly as before. This is the guard on the fact that the
//!   new predicate is a NO-OP on every corpus that never authored a window
//!   (step 22 back-filled NULL with no DEFAULT, and `validity_sql` treats NULL
//!   as unbounded).
//! - **S4 escape hatch** — `include_out_of_window` returns the hidden node, and
//!   `valid_as_of(t)` selects by instant. Both go through the BOUND `:now` seam,
//!   so every assertion here is deterministic: no wall clock, no sleep.
//! - **S5 data-at-rest oracle** — the window really is on disk. A search-based
//!   assertion alone can pass on broken code (a node that was never indexed is
//!   also "not returned"), so the raw table is read directly.
//! - **S6 both branches** — the leak is asserted on the node-body FTS branch
//!   (`search_index`) AND on the explicit text-only entry point, so a fix that
//!   patched one hydration site and missed another cannot pass.
//!
//! Windows are chosen far from the real clock (epoch 1000..2000 = 1970;
//! 4_000_000_000 = year 2096) so the DEFAULT-view assertions are unambiguous
//! without pinning `valid_as_of`.

use fathomdb_engine::{Engine, InitialState, PreparedWrite, ReadView, SearchResult, SourceId};
use fathomdb_schema::SQLITE_SUFFIX;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Epoch second comfortably in the FUTURE relative to any real test clock
/// (2096-10-02). A node whose `valid_from` is this has not opened yet.
const FAR_FUTURE: i64 = 4_000_000_000;

/// Upper bound of a window that closed in 1970 — comfortably in the PAST.
const FAR_PAST_UNTIL: i64 = 2_000;

fn node_win(
    logical_id: &str,
    body: &str,
    valid_from: Option<i64>,
    valid_until: Option<i64>,
) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body.to_string(),
        source_id: SourceId::new("test:s15b-fix2").expect("test source id"),
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Active,
        reason: None,
        valid_from,
        valid_until,
    }
}

fn db_path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

/// Seed a batch on a fresh engine, drain the projection queue, and CLOSE —
/// leaving the file free for the raw-table oracle.
fn seed(path: &Path, batch: &[PreparedWrite]) {
    let opened = Engine::open(path.to_path_buf()).expect("open for seed");
    opened.engine.write(batch).expect("seed write");
    opened.engine.drain(5_000).expect("drain");
    opened.engine.close().expect("close");
}

/// S5 — read the window back from the RAW TABLE. What is asserted is what is on
/// disk, not what an engine call claimed to store.
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

/// S5 — the row really did reach the FTS projection. Without this the leak
/// tests could pass because nothing was ever searchable in the first place.
fn indexed_bodies(path: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only");
    let mut stmt =
        conn.prepare("SELECT body FROM search_index ORDER BY write_cursor").expect("prepare");
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).expect("query");
    rows.flatten().collect()
}

/// The bodies a search returned, sorted for order-independent comparison.
fn bodies(result: &SearchResult) -> Vec<String> {
    let mut out: Vec<String> = result.results.iter().map(|h| h.body.clone()).collect();
    out.sort();
    out
}

/// A read view pinned to a fixed world-time instant — the deterministic `:now`
/// seam. The instant is BOUND, never `datetime('now')`.
fn at(instant: i64) -> ReadView {
    ReadView { valid_as_of: Some(instant), ..ReadView::default() }
}

/// A read view that relaxes validity entirely.
fn unfiltered() -> ReadView {
    ReadView { include_out_of_window: true, ..ReadView::default() }
}

// ---------------------------------------------------------------------------
// S1 / S2 — the leak and its control
// ---------------------------------------------------------------------------

/// **fix-2 keystone (S1 + S2 + S5).** A node whose window has already CLOSED
/// must not surface from a default `search`, while an open-windowed sibling
/// matching the same query must.
#[test]
fn expired_window_node_is_not_returned_by_default_search() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "expired_window");
    seed(
        &path,
        &[
            // Window closed in 1970 — must be hidden.
            node_win("EXPIRED", "quarterly telemetry report", None, Some(FAR_PAST_UNTIL)),
            // Unbounded — the control that keeps this test non-vacuous.
            node_win("ALWAYS", "quarterly telemetry summary", None, None),
        ],
    );

    // S5: the window is on disk, and BOTH bodies reached the FTS projection —
    // so a later absence is a filtering decision, not a missing index row.
    assert_eq!(window_of(&path, "EXPIRED"), (None, Some(FAR_PAST_UNTIL)));
    assert_eq!(window_of(&path, "ALWAYS"), (None, None));
    let indexed = indexed_bodies(&path);
    assert!(
        indexed.iter().any(|b| b.contains("telemetry report")),
        "expired node must be present in search_index (else the test is vacuous): {indexed:?}"
    );
    assert!(indexed.iter().any(|b| b.contains("telemetry summary")));

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    let hits = engine.search("telemetry").expect("search");
    assert_eq!(
        bodies(&hits),
        vec!["quarterly telemetry summary".to_string()],
        "a node whose valid_until is in the past must not leak through default search"
    );

    opened.engine.close().unwrap();
}

/// S1 mirror — a window that has not OPENED yet is equally hidden.
#[test]
fn future_window_node_is_not_returned_by_default_search() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "future_window");
    seed(
        &path,
        &[
            node_win("PENDING", "embargoed launch memo", Some(FAR_FUTURE), None),
            node_win("ALWAYS", "published launch note", None, None),
        ],
    );

    assert_eq!(window_of(&path, "PENDING"), (Some(FAR_FUTURE), None));
    assert!(indexed_bodies(&path).iter().any(|b| b.contains("embargoed launch memo")));

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    assert_eq!(
        bodies(&engine.search("launch").expect("search")),
        vec!["published launch note".to_string()],
        "a node whose valid_from is in the future must not leak through default search"
    );

    opened.engine.close().unwrap();
}

/// S2 — a window that COVERS the current instant is returned. Without this the
/// fix could pass by hiding every windowed node unconditionally.
#[test]
fn covering_window_node_is_returned_by_default_search() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "covering_window");
    // Opened in 1970, closes in 2096 — covers any real test clock.
    seed(&path, &[node_win("COVERING", "in force policy text", Some(1_000), Some(FAR_FUTURE))]);

    assert_eq!(window_of(&path, "COVERING"), (Some(1_000), Some(FAR_FUTURE)));

    let opened = Engine::open(path.clone()).unwrap();
    assert_eq!(
        bodies(&opened.engine.search("policy").expect("search")),
        vec!["in force policy text".to_string()],
        "a node valid at the current instant must still be returned"
    );
    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// S3 — the no-regression guard
// ---------------------------------------------------------------------------

/// **S3.** The corpus that has never authored a window must search
/// IDENTICALLY before and after the predicate is added. Every pre-existing row
/// carries NULL/NULL (step 22 back-filled NULL, no DEFAULT) and `validity_sql`
/// treats NULL as unbounded, so the conjunct is provably a no-op here.
///
/// The oracle is the raw table: it asserts the NULL/NULL premise directly
/// rather than assuming it.
#[test]
fn default_search_is_unchanged_on_a_corpus_with_no_authored_windows() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "no_windows");
    seed(
        &path,
        &[
            node_win("A", "alpha retrieval corpus", None, None),
            node_win("B", "beta retrieval corpus", None, None),
            node_win("C", "gamma retrieval corpus", None, None),
        ],
    );

    // The premise the no-op argument rests on, asserted rather than assumed.
    for id in ["A", "B", "C"] {
        assert_eq!(
            window_of(&path, id),
            (None, None),
            "a write that omits the window must land NULL/NULL — the no-op premise"
        );
    }

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    let expected = vec![
        "alpha retrieval corpus".to_string(),
        "beta retrieval corpus".to_string(),
        "gamma retrieval corpus".to_string(),
    ];
    assert_eq!(bodies(&engine.search("retrieval").expect("search")), expected);
    // The predicate must also be inert under an explicitly pinned instant and
    // under the relaxed view — a NULL/NULL row is valid at EVERY instant.
    assert_eq!(bodies(&engine.search_view("retrieval", &at(1)).expect("search")), expected);
    assert_eq!(
        bodies(&engine.search_view("retrieval", &at(FAR_FUTURE)).expect("search")),
        expected
    );
    assert_eq!(bodies(&engine.search_view("retrieval", &unfiltered()).expect("search")), expected);

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// S4 — the escape hatch
// ---------------------------------------------------------------------------

/// **S4.** `include_out_of_window` returns the hidden nodes again, and
/// `valid_as_of(t)` selects by instant through the BOUND `:now` seam.
#[test]
fn read_view_on_search_selects_by_instant_and_can_relax_validity() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "search_view");
    seed(
        &path,
        &[
            // [1000, 2000) — valid only in the early window.
            node_win("EARLY", "epoch alpha record", Some(1_000), Some(2_000)),
            // [3000, unbounded) — valid from the later instant onward.
            node_win("LATE", "epoch beta record", Some(3_000), None),
        ],
    );

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    // Default view: real now is past 2000 and past 3000, so only LATE is valid.
    assert_eq!(
        bodies(&engine.search("epoch").expect("search")),
        vec!["epoch beta record".to_string()]
    );

    // Pinned instant inside EARLY's window — only EARLY.
    assert_eq!(
        bodies(&engine.search_view("epoch", &at(1_500)).expect("search")),
        vec!["epoch alpha record".to_string()],
        "valid_as_of must select the node valid at the bound instant"
    );

    // Half-open: `== valid_until` is OUT, `== valid_from` is IN.
    assert!(
        bodies(&engine.search_view("epoch", &at(2_000)).expect("search")).is_empty(),
        "valid_until is EXCLUSIVE on the search path, as it is on the read verbs"
    );
    assert_eq!(
        bodies(&engine.search_view("epoch", &at(3_000)).expect("search")),
        vec!["epoch beta record".to_string()],
        "valid_from is INCLUSIVE on the search path"
    );

    // Between the windows — neither is valid.
    assert!(bodies(&engine.search_view("epoch", &at(2_500)).expect("search")).is_empty());

    // Relaxed — both come back regardless of window.
    assert_eq!(
        bodies(&engine.search_view("epoch", &unfiltered()).expect("search")),
        vec!["epoch alpha record".to_string(), "epoch beta record".to_string()],
        "include_out_of_window must return every node whatever its window"
    );

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// S6 — every search entry point, not just one
// ---------------------------------------------------------------------------

/// **S6.** The explicit text-only entry point takes the same predicate. A fix
/// that patched one hydration site and missed another must not pass.
#[test]
fn text_only_search_also_hides_out_of_window_nodes() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "text_only_window");
    seed(
        &path,
        &[
            node_win("EXPIRED", "retired runbook entry", None, Some(FAR_PAST_UNTIL)),
            node_win("ALWAYS", "current runbook entry", None, None),
        ],
    );

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    assert_eq!(
        bodies(&engine.search_text_only("runbook").expect("search_text_only")),
        vec!["current runbook entry".to_string()],
        "search_text_only must apply the same validity predicate as search"
    );

    // …and the same escape hatch reaches it.
    assert_eq!(
        bodies(&engine.search_text_only_view("runbook", &unfiltered()).expect("search")),
        vec!["current runbook entry".to_string(), "retired runbook entry".to_string()]
    );

    opened.engine.close().unwrap();
}

/// **S6.** The filtered / reranked / explained entry points share the same
/// choke point, so the predicate must reach them too.
#[test]
fn filtered_and_explained_search_hide_out_of_window_nodes() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "entry_points_window");
    seed(
        &path,
        &[
            node_win("EXPIRED", "obsolete migration guide", None, Some(FAR_PAST_UNTIL)),
            node_win("ALWAYS", "supported migration guide", None, None),
        ],
    );

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    let expected = vec!["supported migration guide".to_string()];
    assert_eq!(bodies(&engine.search_filtered("migration", None).expect("search")), expected);
    assert_eq!(
        bodies(&engine.search_reranked("migration", None, 0, false, 0.3, 0).expect("search")),
        expected
    );
    assert_eq!(
        bodies(&engine.search_explained("migration", None, 0, false, 0.3, 0).expect("search")),
        expected
    );

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// Scope guard — the existence axis is NOT honoured on search
// ---------------------------------------------------------------------------

/// fix-2 scopes the `search` `ReadView` to the VALIDITY axis only. The existence
/// flags stay the business of the read verbs: relaxing `superseded_at IS NULL`
/// on search would resurrect exactly the stale-content leak the Slice-15 fix-1
/// review closed. Silently IGNORING them would be the dead surface this fix
/// exists to remove, so they are refused with a typed error instead.
#[test]
fn search_refuses_a_view_that_relaxes_the_existence_axis() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "existence_refusal");
    seed(&path, &[node_win("A", "scope guard body", None, None)]);

    let opened = Engine::open(path.clone()).unwrap();
    let engine = &opened.engine;

    for view in [
        ReadView { include_superseded: true, ..ReadView::default() },
        ReadView { include_inactive: true, ..ReadView::default() },
    ] {
        let err = engine.search_view("scope", &view).expect_err("must refuse");
        assert!(
            matches!(err, fathomdb_engine::EngineError::InvalidArgument { .. }),
            "existence flags on a search view must be a TYPED refusal, never a silent ignore: {err:?}"
        );
    }

    // The validity axis alone is accepted.
    assert_eq!(
        bodies(&engine.search_view("scope", &unfiltered()).expect("search")),
        vec!["scope guard body".to_string()]
    );

    opened.engine.close().unwrap();
}
