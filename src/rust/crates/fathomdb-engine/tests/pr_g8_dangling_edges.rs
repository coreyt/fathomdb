//! Slice 20 (G8 / F10) — dangling-edge flag-and-count: an additive,
//! default-non-rejecting referential check at write time that surfaces, on
//! `WriteReceipt.dangling_edge_endpoints`, how many edge endpoints point at a
//! non-existent **or superseded** canonical node (active node = `superseded_at
//! IS NULL` carrying that `logical_id`).
//!
//! Consumes the design memo `dev/design/slice-20-g8-design.md` and the G0
//! substrate `dev/adr/ADR-0.8.0-canonical-identity-substrate.md` (SIGNED
//! 2026-06-03). The probe is `logical_id`-alone against the step-12 partial
//! index `canonical_nodes_logical_active_idx` (no node-kind: `canonical_edges`
//! stores only the edge's own kind). Both endpoints are probed independently;
//! the check is a cross-row post-row-insert pass inside `commit_batch`'s open
//! tx, so a same-batch later-inserted node is NOT flagged. Default is
//! flag-and-count (commit anyway); strict-mode rollback is deferred (band 22).
//!
//! No bound G8/dangling/F10 id exists in `dev/acceptance.md` (locked 0.6.0,
//! max AC-073); these tests bind to the F10/G8 capability label from
//! `dev/design/0.8.0-agent-memory-fit.md` §4 (row G8) / §7.

use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::{migrate, SQLITE_SUFFIX};
use rusqlite::Connection;
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn node(kind: &str, body: &str, logical_id: Option<&str>) -> PreparedWrite {
    PreparedWrite::Node {
        kind: kind.to_string(),
        body: body.to_string(),
        source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
        logical_id: logical_id.map(str::to_string),
        state: fathomdb_engine::InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

fn edge(kind: &str, from: &str, to: &str, logical_id: Option<&str>) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: kind.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
        logical_id: logical_id.map(str::to_string),
        body: None,
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

/// (a) — an edge with ONE missing endpoint (the other endpoint is a live node)
/// increments `dangling_edge_endpoints` by exactly 1.
#[test]
fn s20_edge_to_one_missing_endpoint_counts_one() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "one_missing")).expect("open");

    // A live node `A`, then an edge A -> MISSING. Only the `to` endpoint dangles.
    let receipt = opened
        .engine
        .write(&[node("doc", "a", Some("A")), edge("rel", "A", "MISSING", None)])
        .expect("write");

    assert_eq!(
        receipt.dangling_edge_endpoints, 1,
        "exactly one endpoint (MISSING) dangles; the live node A does not"
    );
    opened.engine.close().unwrap();
}

/// (b) — cross-row: an edge whose endpoints are nodes inserted LATER in the same
/// batch is NOT flagged. This is the case a single-row pre-insert `validate_write`
/// hook would get wrong; the post-row-insert pass sees the fully-populated nodes.
#[test]
fn s20_same_batch_later_inserted_node_is_not_flagged() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "same_batch")).expect("open");

    // Edge FIRST, then both its endpoints — exactly the bulk-loader ordering.
    let receipt = opened
        .engine
        .write(&[
            edge("rel", "N1", "N2", None),
            node("doc", "n1", Some("N1")),
            node("doc", "n2", Some("N2")),
        ])
        .expect("write");

    assert_eq!(
        receipt.dangling_edge_endpoints, 0,
        "same-batch later-inserted endpoints must not be flagged (cross-row)"
    );
    opened.engine.close().unwrap();
}

/// (c) — an edge to a SUPERSEDED node (its active version tombstoned, no active
/// version remains) counts as dangling. The G0 write path never leaves a
/// logical_id with zero active versions, so we construct that state via raw SQL
/// (mirrors `pr_g0_identity.rs`'s direct-index tests), then probe via a write.
#[test]
fn s20_edge_to_superseded_node_counts() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "superseded");

    // 1) Write an active node `S` through the engine.
    {
        let opened = Engine::open(&path).expect("open");
        opened.engine.write(&[node("doc", "s-v1", Some("S"))]).expect("write S");
        opened.engine.close().unwrap();
    }
    // 2) Tombstone S's only active version (no re-insert) — now S has zero active
    //    versions. The engine must be closed to take the raw connection.
    {
        let conn = Connection::open(&path).expect("open sqlite");
        let n = conn
            .execute(
                "UPDATE canonical_nodes SET superseded_at = 999
                 WHERE logical_id = 'S' AND superseded_at IS NULL",
                [],
            )
            .expect("tombstone S");
        assert_eq!(n, 1, "exactly one active S row tombstoned");
    }
    // 3) Write an edge -> S; S has no active version, so it dangles.
    {
        let opened = Engine::open(&path).expect("reopen");
        let receipt =
            opened.engine.write(&[edge("rel", "S", "S", None)]).expect("write edge to superseded");
        assert_eq!(
            receipt.dangling_edge_endpoints, 2,
            "both endpoints reference superseded S (no active version) -> 2"
        );
        opened.engine.close().unwrap();
    }
}

/// (d) — default FLAG-AND-COUNT commits the batch: the dangling edge row is
/// present on disk after the write, and the receipt still carries the count
/// (the check never rejects by default).
#[test]
fn s20_default_flag_and_count_commits_the_batch() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "flag_and_count");
    {
        let opened = Engine::open(&path).expect("open");
        let receipt = opened
            .engine
            .write(&[edge("rel", "GHOST_A", "GHOST_B", None)])
            .expect("write must not reject");
        assert_eq!(receipt.dangling_edge_endpoints, 2, "both endpoints dangle");
        opened.engine.close().unwrap();
    }
    // The edge committed despite dangling endpoints (flag-and-count, not reject).
    let conn = Connection::open(&path).expect("open sqlite");
    let committed: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canonical_edges WHERE from_id = 'GHOST_A' AND to_id = 'GHOST_B'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(committed, 1, "the dangling edge must still be committed (flag-and-count)");
}

/// (e) — the count is the SUM over both endpoints, probed independently: an edge
/// with BOTH endpoints missing contributes 2, and a clean edge contributes 0.
#[test]
fn s20_count_is_sum_over_both_endpoints() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "sum_both")).expect("open");

    // Two live nodes, then: one fully-clean edge (0) + one fully-dangling edge (2).
    let receipt = opened
        .engine
        .write(&[
            node("doc", "p", Some("P")),
            node("doc", "q", Some("Q")),
            edge("rel", "P", "Q", None),   // clean: 0
            edge("rel", "X1", "X2", None), // both missing: 2
        ])
        .expect("write");

    assert_eq!(
        receipt.dangling_edge_endpoints, 2,
        "clean edge contributes 0, both-missing edge contributes 2 -> sum 2"
    );
    opened.engine.close().unwrap();
}

/// (g) [O(N) precompute equivalence] — a batch with MULTIPLE active logical edges
/// plus a LATER same-`(logical_id, kind)` edge that tombstones an EARLIER one. The
/// superseded earlier edge carries a dangling endpoint (`GHOST`); only the
/// final-active edge's (clean) endpoints are probed. This pins the last-index
/// precompute that replaced the per-edge `batch[i+1..]` scan: an earlier edge is
/// skipped iff a strictly-later same-key edge exists. A precompute that recorded
/// the FIRST index (or otherwise mis-ordered) would skip the final-active edge and
/// probe the superseded `GHOST` endpoint instead -> count would be 1, not 0.
#[test]
fn s20_on_supersession_skips_earlier_keeps_final_active() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "precompute_equiv")).expect("open");

    // Live node A. Then:
    //  e1: (logical_id=E, kind=rel)  A -> GHOST   <- earlier, dangling `to`, superseded by e3
    //  e2: (logical_id=F, kind=rel)  A -> A       <- a *second* active logical edge (clean)
    //  e3: (logical_id=E, kind=rel)  A -> A       <- LATER same (E,rel): tombstones e1, final-active, clean
    let receipt = opened
        .engine
        .write(&[
            node("doc", "a", Some("A")),
            edge("rel", "A", "GHOST", Some("E")),
            edge("rel", "A", "A", Some("F")),
            edge("rel", "A", "A", Some("E")),
        ])
        .expect("write");

    assert_eq!(
        receipt.dangling_edge_endpoints, 0,
        "earlier (E,rel) edge is in-batch-superseded by the later one and skipped; \
         the final-active edge + the (F,rel) edge are clean -> 0 (last-index precompute)"
    );
    opened.engine.close().unwrap();
}

/// (f) [latency / plan gate] — the per-endpoint EXISTS probe hits the step-12
/// partial index `canonical_nodes_logical_active_idx` (leading column
/// `logical_id`, partial predicate `superseded_at IS NULL`) and does NOT do a
/// `SCAN canonical_nodes`. This is the write-latency guard for the cross-row pass.
#[test]
fn s20_endpoint_probe_hits_partial_index_no_scan() {
    let dir = TempDir::new().unwrap();
    let conn = Connection::open(db_path(&dir, "plan")).expect("open sqlite");
    migrate(&conn).expect("migrate to head");

    let plan: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "EXPLAIN QUERY PLAN
                 SELECT 1 FROM canonical_nodes WHERE logical_id = ?1 AND superseded_at IS NULL LIMIT 1",
            )
            .expect("prepare EXPLAIN");
        let rows = stmt
            .query_map(["any"], |row| row.get::<_, String>(3))
            .expect("query plan")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect plan");
        rows
    };
    let detail = plan.join(" | ");

    assert!(
        detail.contains("canonical_nodes_logical_active_idx"),
        "probe must use the partial index canonical_nodes_logical_active_idx; plan: {detail}"
    );
    assert!(
        !detail.contains("SCAN canonical_nodes"),
        "probe must not full-scan canonical_nodes; plan: {detail}"
    );
}

/// Legacy baseline — a `logical_id = None` (byte-identical 0.7.x path) batch of
/// nodes + edges still writes unchanged (no panic, count well-defined). NULL
/// endpoints are not matchable by logical_id, so a NULL-keyed edge counts its
/// endpoints as dangling (the intended, informational legacy consequence).
#[test]
fn s20_legacy_null_logical_id_batch_writes_with_defined_count() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "legacy")).expect("open");

    // Legacy nodes carry NULL logical_id; the edge's endpoints ("l0"/"l1") match
    // no active logical_id, so both dangle -> count 2, but the write succeeds.
    let receipt = opened
        .engine
        .write(&[node("doc", "l0", None), node("doc", "l1", None), edge("rel", "l0", "l1", None)])
        .expect("legacy batch writes without panic");

    assert_eq!(
        receipt.dangling_edge_endpoints, 2,
        "legacy NULL-logical_id endpoints are not matchable -> both dangle (intended)"
    );
    opened.engine.close().unwrap();
}
