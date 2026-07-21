//! TC-33 fix-5 — close the whole `strftime`-leniency class (TC-44).
//!
//! `strftime('%s', ?)` was standing in as an ISO-8601 VALIDATOR, but SQLite's
//! date parser is MORE lenient than the declared "extractor boundary keeps
//! ISO-8601 with HARD REJECTION" contract (HITL-ratified 2026-07-21). Two
//! confirmed leaks, both accepted today, both the WRONG instant, neither NULL:
//!
//! 1. **Wire boundary — bare numeric strings read as Julian days / raw offsets.**
//!    `strftime('%s','2451545.0')` → `946728000` (SQLite reads the bare number
//!    as a Julian DAY = year 2000); `strftime('%s','0')` → a pre-year-0000
//!    epoch. So `"2451545.0"` and `"0"` are ACCEPTED and stored as unrelated
//!    instants, despite the hard-reject-ISO-8601 contract. A strict ISO-8601
//!    SHAPE gate must run BEFORE `strftime` and hard-reject these.
//!
//! 2. **Governed i64 boundary — pre-year-0000 epochs render NON-NULL.**
//!    `reject_unrenderable_edge_epoch` guarded on *renderability*, but
//!    `strftime(..., 'unixepoch')` renders a below-`MIN_RENDERABLE_EPOCH` value
//!    like `-62167219201` to `-001-12-31T23:59:59Z` (NON-NULL), so it slipped
//!    the renderability check even though it is outside the declared years
//!    0000..=9999 wire range. The guard must test the numeric MIN/MAX bounds
//!    directly, not renderability.
//!
//! Neither leak produces a NULL `t_invalid`, so the safety-critical
//! "no resurrection via NULL" property (HITL Note 1 / P3) was already intact;
//! this is a contract-completion/correctness fix, and it must not open a NULL
//! path.

use fathomdb_engine::{Engine, EngineError, ExtractDocument, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

// The inclusive SQLite-renderable epoch bounds (years 0000..=9999), pinned here
// independently of the engine constants. Verified directly against SQLite:
//   strftime('%Y-%m-%dT%H:%M:%SZ', -62167219200, 'unixepoch') = 0000-01-01T00:00:00Z
//   strftime('%Y-%m-%dT%H:%M:%SZ',  253402300799,'unixepoch') = 9999-12-31T23:59:59Z
//   strftime('%Y-%m-%dT%H:%M:%SZ', -62167219201, 'unixepoch') = -001-12-31T23:59:59Z (NON-NULL leak)
const MIN_RENDERABLE_EPOCH: i64 = -62_167_219_200; // 0000-01-01T00:00:00Z
const FIRST_UNRENDERABLE_BELOW: i64 = MIN_RENDERABLE_EPOCH - 1; // -62167219201, year -0001

// ---------------------------------------------------------------------------
// Wire-boundary infrastructure (mirrors tc33_edge_temporal_epoch.rs)
// ---------------------------------------------------------------------------

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/slice15_byo_llm")
}

fn stub_harness_cmd() -> Vec<String> {
    let script = fixture_dir().join("stub_harness.py");
    assert!(script.exists(), "stub harness must exist at {}", script.display());
    vec!["python3".to_string(), script.to_string_lossy().to_string()]
}

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn ingest(path: &std::path::Path, doc_id: &str) -> Result<u64, EngineError> {
    let opened = Engine::open_without_embedder_for_test(path).expect("open");
    let cmd_strings = stub_harness_cmd();
    let cmd_refs: Vec<&str> = cmd_strings.iter().map(|s| s.as_str()).collect();
    let docs = vec![ExtractDocument {
        source_doc_id: doc_id.to_string(),
        body: "irrelevant — the stub harness is fixture-keyed by source_doc_id".to_string(),
    }];
    opened.engine.ingest_with_extractor(&cmd_refs, &docs).map(|r| r.edges_written)
}

fn edge_count(path: &std::path::Path) -> i64 {
    let conn = Connection::open(path).unwrap();
    conn.query_row("SELECT COUNT(*) FROM canonical_edges", [], |r| r.get(0)).unwrap()
}

/// A non-ISO extractor timestamp that `strftime` is lenient enough to ACCEPT as
/// some unrelated instant must hard-reject at the extractor boundary, and NOT
/// ONE edge row may reach the table. The raw-table `count(*)` is the load-bearing
/// assertion: the property is at-rest, not in search results.
fn assert_wire_hard_reject(doc_id: &str, what: &str) {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "reject");

    let result = ingest(&path, doc_id);

    let err = match result {
        Ok(n) => panic!(
            "TC-33 fix-5: {what} ({doc_id}) is NOT ISO-8601 and MUST hard-reject at \
             the extractor boundary, but ingest succeeded and wrote {n} edge(s). \
             `strftime` accepted it as an unrelated instant — the shape gate must \
             reject it BEFORE `strftime` sees it."
        ),
        Err(e) => e,
    };

    let rendered = format!("{err:?}");
    assert!(
        rendered.contains("t_valid") || rendered.contains("t_invalid"),
        "TC-33 fix-5: rejection of {what} must name the offending temporal field; got {rendered}"
    );

    assert_eq!(
        edge_count(&path),
        0,
        "TC-33 fix-5: {what} ({doc_id}) was rejected, so NO edge row may reach \
         canonical_edges (raw at-rest table check)."
    );
}

/// A valid ISO-8601 shape must STILL pass the gate and store the CORRECT epoch —
/// proving the gate is not vacuously rejecting everything.
fn assert_wire_accepts(doc_id: &str, expected_epoch: i64) {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "accept");
    ingest(&path, doc_id).unwrap_or_else(|e| {
        panic!("TC-33 fix-5: valid ISO-8601 ({doc_id}) must ingest, got {e:?}")
    });

    let conn = Connection::open(&path).unwrap();
    let (value, sqlite_type): (i64, String) = conn
        .query_row(
            "SELECT t_valid, typeof(t_valid) FROM canonical_edges \
             WHERE kind = 'works_for' AND superseded_at IS NULL",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the works_for edge must exist");

    assert_eq!(sqlite_type, "integer", "t_valid must be stored as INTEGER epoch seconds");
    assert_eq!(
        value, expected_epoch,
        "TC-33 fix-5: valid ISO-8601 ({doc_id}) must normalise to the CORRECT epoch"
    );
}

// ---------------------------------------------------------------------------
// (a) Wire boundary — strict ISO-8601 SHAPE gate before strftime
// ---------------------------------------------------------------------------

/// `"2451545.0"` — SQLite reads the bare number as a JULIAN DAY (`strftime('%s',
/// '2451545.0')` = 946728000, i.e. year 2000). Not ISO-8601 → must hard-reject.
#[test]
fn tc33_fix5_julian_day_string_hard_rejects() {
    assert_wire_hard_reject("doc-tc33-julian-tvalid", "Julian-day string \"2451545.0\"");
}

/// `"0"` — a bare numeric string. `strftime('%s','0')` is a NON-NULL pre-year-0000
/// epoch, not ISO-8601 → must hard-reject.
#[test]
fn tc33_fix5_bare_zero_string_hard_rejects() {
    assert_wire_hard_reject("doc-tc33-zero-tvalid", "bare numeric string \"0\"");
}

/// Positive control — canonical `Z` datetime.
#[test]
fn tc33_fix5_iso_z_datetime_accepted() {
    assert_wire_accepts("doc-tc33-iso-z", 1_742_463_000); // 2025-03-20T09:30:00Z
}

/// Positive control — numeric UTC offset.
#[test]
fn tc33_fix5_iso_offset_datetime_accepted() {
    assert_wire_accepts("doc-tc33-iso-offset", 1_742_463_000); // 2025-03-20T09:30:00+00:00
}

/// Positive control — date-only.
#[test]
fn tc33_fix5_iso_date_only_accepted() {
    assert_wire_accepts("doc-tc33-iso-dateonly", 1_742_428_800); // 2025-03-20 (midnight UTC)
}

/// Positive control — space-separated date/time (SQLite-compatible ISO variant).
#[test]
fn tc33_fix5_iso_space_separated_accepted() {
    assert_wire_accepts("doc-tc33-iso-spacesep", 1_742_463_000); // 2025-03-20 09:30:00
}

// ---------------------------------------------------------------------------
// (b) Governed i64 boundary — numeric MIN/MAX range check, not renderability
// ---------------------------------------------------------------------------

fn edge_with_temporal(t_valid: Option<i64>, t_invalid: Option<i64>) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: "A".to_string(),
        to: "B".to_string(),
        source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
        logical_id: Some("edge:tc33-fix5".to_string()),
        body: None,
        t_valid,
        t_invalid,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

fn govern_edge_count(path: &std::path::Path) -> i64 {
    let conn = Connection::open(path).unwrap();
    conn.query_row("SELECT COUNT(*) FROM canonical_edges", [], |r| r.get(0)).unwrap()
}

/// A `t_invalid` one second BELOW `MIN_RENDERABLE_EPOCH` (year -0001) renders
/// NON-NULL (`-001-12-31T23:59:59Z`), so the old renderability guard let it
/// through even though it is outside the declared years 0000..=9999. The numeric
/// bounds check must reject it at the governed write boundary — NO row stored.
#[test]
fn tc33_fix5_pre_year_0000_epoch_is_rejected() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "pre_year0");
    let engine = Engine::open(&path).expect("open").engine;

    let err = engine.write(&[edge_with_temporal(None, Some(FIRST_UNRENDERABLE_BELOW))]).expect_err(
        "TC-33 fix-5: a pre-year-0000 epoch renders NON-NULL and slipped the \
             renderability guard; the numeric MIN/MAX bounds check must reject it",
    );

    match &err {
        EngineError::InvalidArgument { msg } => assert!(
            msg.contains("t_invalid"),
            "TC-33 fix-5: rejection must name the offending field `t_invalid`; got {msg}"
        ),
        other => panic!("TC-33 fix-5: must be a typed InvalidArgument, not {other:?}"),
    }

    engine.close().unwrap();
    assert_eq!(
        govern_edge_count(&path),
        0,
        "TC-33 fix-5: a below-range epoch was rejected, so NO edge row may reach canonical_edges"
    );
}

/// Precision guard: the FIRST renderable instant (`0000-01-01T00:00:00Z`,
/// `MIN_RENDERABLE_EPOCH` exactly) must STILL be STORABLE. An over-tight bound
/// that rejected this would break a legitimate boundary value.
#[test]
fn tc33_fix5_min_renderable_epoch_is_storable() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "min_renderable");
    let engine = Engine::open(&path).expect("open").engine;

    engine
        .write(&[edge_with_temporal(Some(MIN_RENDERABLE_EPOCH), None)])
        .expect("TC-33 fix-5: the min renderable epoch (0000-01-01T00:00:00Z) must be storable");

    engine.close().unwrap();
    assert_eq!(govern_edge_count(&path), 1, "the boundary-min edge must be written");
}
