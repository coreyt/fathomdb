//! 0.8.20 Slice 15b fix-3 (R-20-NV) — two codex §9 [P2] correctness findings on
//! the validity-aware search path landed by fix-2.
//!
//! fix-2 made every search hydration site apply the validity predicate. It
//! applied it in the RIGHT PLACE for correctness-of-inclusion (no expired row
//! can leak) but in the WRONG PLACE for correctness-of-RECALL, and it resolved
//! the instant TWICE for a graph-arm query.
//!
//! **F1 — validity must participate BEFORE the vector cutoff.** The vector arm
//! is two-stage: phase 1 is a bit-KNN over the `vector_default` vec0 table
//! (`LIMIT TOP_K_BIT_CANDIDATES`), phase 2 is an ordinary rowid JOIN that
//! reranks by exact L2. fix-2 filtered validity while HYDRATING the phase-2
//! output, i.e. AFTER the `LIMIT final_limit` cutoff. So if the `final_limit`
//! nearest neighbours are all out-of-window they consume every slot, get
//! dropped at hydration, and the valid rows ranked just below the cutoff are
//! never considered — a default search returns too few hits, or NONE at all,
//! for a query that should return plenty. This is a silent RECALL defect: no
//! error, no soft-fallback signal, just a short result set.
//!
//! `F1` pins the fixed behaviour: with the ten nearest neighbours expired and
//! two valid rows immediately below them, a default search must still return
//! the two valid rows.
//!
//! **F2 — one query, one instant.** For the default view (`valid_as_of ==
//! None`) the graph arm re-resolved *now* instead of reusing the instant
//! already resolved for the vector/text arms. A query that straddles a validity
//! boundary could then have its arms disagree about which side of the boundary
//! they are on, so a boundary-window node's inclusion became a race against the
//! wall clock. R-20-NV requires `:now` to bind ONCE PER QUERY — not per row,
//! and not per arm.
//!
//! `F2` is asserted structurally, by COUNTING CLOCK READS rather than by racing
//! the clock: `clock_reads_for_test()` meters the single function that reads
//! wall time on this path, so "the arms cannot disagree" becomes "the clock was
//! read exactly once". No wall-clock assertion, no sleep, deterministic.
//!
//! Both tests read RAW TABLES to prove they are non-vacuous — a row that was
//! never projected into `vector_default` is also "not returned", which would
//! make F1 pass on entirely broken code.

use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{
    clock_reads_for_test, Engine, InitialState, PreparedWrite, SearchResult, SourceId,
    SEARCH_RERANK_LIMIT,
};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// A rank-ordered embedder — the seam that makes "nearest N" controllable
// ---------------------------------------------------------------------------

/// Every seeded body starts with a two-digit rank marker `rNN`. The embedder
/// maps that marker onto a monotonically increasing offset in dimension 1, so
/// the exact-L2 phase-2 ordering is EXACTLY the rank order: `r01` is the
/// nearest neighbour of the query, `r12` the farthest.
///
/// Dimension 0 is 1.0 everywhere (including the query), so every row is
/// sign-quantized identically and the phase-1 bit-KNN admits ALL of them into
/// the candidate pool. The test therefore isolates the phase-2 cutoff, which is
/// where the defect lives — it is not accidentally testing bit-KNN recall.
#[derive(Clone, Debug)]
struct RankedEmbedder;

fn rank_of(text: &str) -> f32 {
    // `rNN ...` → NN. The query string carries no marker → rank 0 (the origin).
    let bytes = text.as_bytes();
    if bytes.len() >= 3
        && bytes[0] == b'r'
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
    {
        return text[1..3].parse::<f32>().unwrap_or(0.0);
    }
    0.0
}

impl Embedder for RankedEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new("ranked", "rev-a", 8)
    }
    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        let mut v = vec![0.0_f32; 8];
        v[0] = 1.0;
        v[1] = rank_of(text) * 0.01;
        Ok(v)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Upper bound of a window that closed in 1970 — comfortably in the PAST for
/// any real test clock, so the DEFAULT view hides it without pinning
/// `valid_as_of`.
const FAR_PAST_UNTIL: i64 = 2_000;

/// `clock_reads_for_test()` meters a PROCESS-WIDE counter, so a delta is only
/// meaningful while no other test in this binary is searching. Every test in
/// this file holds this lock for the whole of its engine work — including F1,
/// which reads no counter but does drive the clock. Without it the harness's
/// default intra-binary parallelism would make F2 flaky rather than wrong.
static CLOCK_METER: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Take the meter lock, ignoring poisoning from an unrelated failing test (a
/// poisoned mutex must not turn one failure into three).
fn clock_meter_guard() -> std::sync::MutexGuard<'static, ()> {
    CLOCK_METER.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn node_win(
    logical_id: &str,
    body: &str,
    valid_from: Option<i64>,
    valid_until: Option<i64>,
) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: body.to_string(),
        source_id: SourceId::new("test:s15b-fix3").expect("test source id"),
        logical_id: Some(logical_id.to_string()),
        state: InitialState::Active,
        reason: None,
        valid_from,
        valid_until,
    }
}

fn bodies(result: &SearchResult) -> Vec<String> {
    let mut out: Vec<String> = result.results.iter().map(|h| h.body.clone()).collect();
    out.sort();
    out
}

/// RAW-TABLE oracle — how many rows actually reached the vector partition. F1
/// is only meaningful if every seeded row is a real vector candidate.
fn vector_row_count(path: &std::path::Path) -> i64 {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only");
    conn.query_row("SELECT count(*) FROM vector_default", [], |r| r.get::<_, i64>(0))
        .expect("count vector rows")
}

/// RAW-TABLE oracle — the windows really are on disk as authored.
fn expired_row_count(path: &std::path::Path) -> i64 {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only");
    conn.query_row(
        "SELECT count(*) FROM canonical_nodes
         WHERE superseded_at IS NULL AND valid_until IS NOT NULL AND valid_until <= ?1",
        [FAR_PAST_UNTIL],
        |r| r.get::<_, i64>(0),
    )
    .expect("count expired rows")
}

// ---------------------------------------------------------------------------
// F1 — validity must participate before the vector `LIMIT` cutoff
// ---------------------------------------------------------------------------

/// **fix-3 keystone (F1).** The `SEARCH_RERANK_LIMIT` nearest vector
/// neighbours are ALL expired; two valid rows sit immediately below the cutoff.
/// A default search must still return those two valid rows.
///
/// Before the fix the expired rows consumed every slot in the phase-2 `LIMIT`
/// and were then dropped at hydration, so this search returned ZERO hits.
#[test]
fn expired_nearest_neighbours_do_not_starve_valid_vector_hits() {
    let _meter = clock_meter_guard();
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("f1_cutoff{SQLITE_SUFFIX}"));

    // The ten nearest neighbours (r01..r10) are expired; r11/r12 are unbounded
    // and therefore valid at every instant. Ten == SEARCH_RERANK_LIMIT, so the
    // pre-fix cutoff is filled entirely with rows that cannot be hydrated.
    assert_eq!(SEARCH_RERANK_LIMIT, 10, "this fixture is sized to the production cutoff");
    let mut batch: Vec<PreparedWrite> = Vec::new();
    for i in 1..=10 {
        batch.push(node_win(
            &format!("EXPIRED{i:02}"),
            &format!("r{i:02} telemetry rollup"),
            None,
            Some(FAR_PAST_UNTIL),
        ));
    }
    batch.push(node_win("VALID11", "r11 telemetry rollup", None, None));
    batch.push(node_win("VALID12", "r12 telemetry rollup", None, None));

    {
        let opened = Engine::open_with_embedder_for_test(&path, Arc::new(RankedEmbedder))
            .expect("open for seed");
        opened.engine.configure_vector_kind_for_test("doc").expect("vector kind doc");
        opened.engine.write(&batch).expect("seed write");
        opened.engine.drain(10_000).expect("drain");
        opened.engine.close().expect("close");
    }

    // RAW-TABLE oracle: all twelve rows really are vector candidates, and the
    // ten expired windows really are on disk. Without this the test could pass
    // on code that simply never indexed anything.
    assert_eq!(vector_row_count(&path), 12, "every seeded row must be a vector candidate");
    assert_eq!(expired_row_count(&path), 10, "ten expired windows must be on disk");

    let opened =
        Engine::open_with_embedder_for_test(&path, Arc::new(RankedEmbedder)).expect("reopen");
    let engine = &opened.engine;
    // Isolate the VECTOR arm: the text arm already filters validity correctly
    // (fix-2), and fusing it in would mask the vector-arm recall defect.
    engine.set_vector_stage_only_for_test(true);

    let hits = engine.search("telemetry").expect("search");
    assert_eq!(
        bodies(&hits),
        vec!["r11 telemetry rollup".to_string(), "r12 telemetry rollup".to_string()],
        "valid rows ranked just below the cutoff must not be starved by expired \
         nearer neighbours — validity has to participate BEFORE the LIMIT"
    );

    opened.engine.close().unwrap();
}

// ---------------------------------------------------------------------------
// F2 — one query, one instant, every arm
// ---------------------------------------------------------------------------

/// **fix-3 (F2 / R-20-NV `:now` binds once per query).** A default-view search
/// with the graph arm enabled must read the wall clock EXACTLY ONCE. Two reads
/// mean the arms can land on opposite sides of a validity boundary.
///
/// This meters the clock instead of racing it: no sleep, no wall-clock
/// assertion, and it fails for ANY arm that re-reads the clock — including one
/// added in the future.
#[test]
fn default_view_search_reads_the_clock_once_per_query() {
    let _meter = clock_meter_guard();
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("f2_instant{SQLITE_SUFFIX}"));

    {
        let opened = Engine::open_with_embedder_for_test(&path, Arc::new(RankedEmbedder))
            .expect("open for seed");
        opened.engine.configure_vector_kind_for_test("doc").expect("vector kind doc");
        opened
            .engine
            .write(&[
                node_win("A", "r01 telemetry rollup", None, None),
                node_win("B", "r02 telemetry digest", None, None),
            ])
            .expect("seed write");
        opened.engine.drain(10_000).expect("drain");
        opened.engine.close().expect("close");
    }

    let opened =
        Engine::open_with_embedder_for_test(&path, Arc::new(RankedEmbedder)).expect("reopen");
    let engine = &opened.engine;

    // `use_graph_arm = true` with the DEFAULT view (`valid_as_of == None`) is
    // exactly the condition under which the graph arm used to re-resolve now.
    let before = clock_reads_for_test();
    engine.search_reranked("telemetry", None, 0, true, 0.3, 0).expect("graph-arm search");
    let after = clock_reads_for_test();

    assert_eq!(
        after - before,
        1,
        "a default-view query must resolve its validity instant ONCE and share it \
         with every arm (text, vector, graph); {} reads means the arms can \
         disagree across a validity boundary",
        after - before
    );

    opened.engine.close().unwrap();
}

/// F2 control — a search with the graph arm OFF also reads the clock exactly
/// once, so the assertion above cannot pass merely because the graph arm was
/// skipped.
#[test]
fn default_view_search_without_graph_arm_also_reads_the_clock_once() {
    let _meter = clock_meter_guard();
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("f2_control{SQLITE_SUFFIX}"));

    {
        let opened = Engine::open_with_embedder_for_test(&path, Arc::new(RankedEmbedder))
            .expect("open for seed");
        opened.engine.configure_vector_kind_for_test("doc").expect("vector kind doc");
        opened
            .engine
            .write(&[node_win("A", "r01 telemetry rollup", None, None)])
            .expect("seed write");
        opened.engine.drain(10_000).expect("drain");
        opened.engine.close().expect("close");
    }

    let opened =
        Engine::open_with_embedder_for_test(&path, Arc::new(RankedEmbedder)).expect("reopen");
    let engine = &opened.engine;

    let before = clock_reads_for_test();
    engine.search("telemetry").expect("search");
    let after = clock_reads_for_test();

    assert_eq!(after - before, 1, "the two-arm path also resolves the instant exactly once");

    opened.engine.close().unwrap();
}
