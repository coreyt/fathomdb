//! 0.8.20 Slice 15c (TC-33 fix-2) — edge temporal validity must be enforced on
//! the ordinary-search READ path, not only on graph traversal and projection.
//!
//! **The finding (codex §9 [P2]).** TC-33 defines `edge_validity_sql` — the ONE
//! read predicate for "is this edge valid at `:now`" (`t_invalid IS NULL OR
//! t_invalid > :now`) — and already applies it on the graph-traversal and
//! projection paths. But the ordinary-search edge-body FTS branch
//! (`search_index_edges` JOIN `canonical_edges`) and the vector-projected
//! edge-fact hydration path both gated on `superseded_at IS NULL` ALONE. So a
//! body-bearing edge written with `t_invalid` in the PAST (an expired /
//! invalidated edge) still MATCHed a query and surfaced its body through
//! `search()`, even though every temporal-aware read verb treats it as gone.
//! This is the edge-search twin of the Slice 15b "validity enforced on
//! traversal, not on search" gap — now on the edge read path.
//!
//! **Contract.** These tests assert on ACTUAL `search()` results: an expired
//! edge body must NOT appear, and — so the filter is not vacuously excluding
//! everything — a still-valid edge (`t_invalid` NULL, and separately a FUTURE
//! `t_invalid`) with the same MATCHed token MUST appear.

use std::time::{Duration, Instant};

use fathomdb_engine::{Engine, PreparedWrite, SearchResult};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

/// A far-future epoch (year ~2096): renderable, strictly `> :now`, and does not
/// read the wall clock in the test (so the fixture stays deterministic).
const FUTURE_EPOCH: i64 = 4_000_000_000;
/// A far-past epoch (1970-01-01T00:16:40Z): renderable and strictly `<= :now`.
const PAST_EPOCH: i64 = 1_000;

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn node(logical_id: &str, body: &str) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body.to_string(),
        source_id: fathomdb_engine::SourceId::new("SRC-TC33-FIX2").expect("test source id"),
        logical_id: Some(logical_id.to_string()),
        state: fathomdb_engine::InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

fn edge(
    from: &str,
    to: &str,
    logical_id: &str,
    body: &str,
    t_invalid: Option<i64>,
) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: fathomdb_engine::SourceId::new("SRC-TC33-FIX2").expect("test source id"),
        logical_id: Some(logical_id.to_string()),
        body: Some(body.to_string()),
        t_valid: None,
        t_invalid,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

/// Poll `search` until the whole write batch has been projected (`cursor`) AND a
/// hit whose body contains `must_contain` is present, so the "expired absent"
/// assertion is meaningful (the expired edge, written in the SAME batch, is
/// definitely projected too) rather than merely not-yet-indexed.
fn search_until_present(
    engine: &Engine,
    query: &str,
    min_cursor: u64,
    must_contain: &str,
) -> SearchResult {
    let started = Instant::now();
    loop {
        let result = engine.search(query).expect("search");
        let present = result.results.iter().any(|h| h.body.contains(must_contain));
        if result.projection_cursor >= min_cursor && present {
            return result;
        }
        if started.elapsed() > Duration::from_secs(10) {
            return result;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The core defect: an expired edge (`t_invalid` in the past) must NOT surface
/// through ordinary search, while a still-valid edge with the SAME MATCHed token
/// must — proving the validity filter is applied and not vacuous.
#[test]
fn tc33_fix2_expired_edge_is_excluded_from_search_valid_edges_survive() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tc33_fix2_search");
    let opened = Engine::open(&path).expect("open");

    // Shared FTS token "tc33edgevalidity" matches all three edge bodies; the
    // distinguishing words let each assertion target one edge.
    let receipt = opened
        .engine
        .write(&[
            node("anna", "anna the first entity"),
            node("bob", "bob the second entity"),
            node("carol", "carol the third entity"),
            node("dave", "dave the fourth entity"),
            node("erin", "erin the fifth entity"),
            node("frank", "frank the sixth entity"),
            // EXPIRED — invalidated in the past. Must NOT surface.
            edge("anna", "bob", "edge-ab", "tc33edgevalidity expiredfact alpha", Some(PAST_EPOCH)),
            // VALID (open-ended, t_invalid NULL). Must surface.
            edge("carol", "dave", "edge-cd", "tc33edgevalidity validnullfact beta", None),
            // VALID (invalidation in the future). Must surface.
            edge(
                "erin",
                "frank",
                "edge-ef",
                "tc33edgevalidity validfuturefact gamma",
                Some(FUTURE_EPOCH),
            ),
        ])
        .expect("write");

    let result =
        search_until_present(&opened.engine, "tc33edgevalidity", receipt.cursor, "validnullfact");

    let bodies: Vec<&str> = result.results.iter().map(|h| h.body.as_str()).collect();

    // Sibling assertions FIRST: the still-valid edges must be present, so a bare
    // "expired absent" pass cannot be a vacuous "everything excluded".
    assert!(
        bodies.iter().any(|b| b.contains("validnullfact")),
        "a still-valid edge (t_invalid NULL) with the matched token MUST surface; got: {bodies:?}"
    );
    assert!(
        bodies.iter().any(|b| b.contains("validfuturefact")),
        "a still-valid edge (t_invalid in the future) with the matched token MUST surface; got: {bodies:?}"
    );

    // The finding: the expired edge body must NOT appear on ANY search arm.
    assert!(
        !bodies.iter().any(|b| b.contains("expiredfact")),
        "TC-33 fix-2: an EXPIRED edge (t_invalid in the past) must NOT surface its body \
         through ordinary search — edge validity is not enforced on the search read path; \
         got: {bodies:?}"
    );

    opened.engine.close().unwrap();
}
