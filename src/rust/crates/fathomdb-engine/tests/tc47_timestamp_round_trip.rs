//! TC-47 — the calendar-impossible-DAY rollover residue behind the TC-44
//! `strftime`-leniency class (keystone terminal codex P2).
//!
//! TC-33 fix-5's [`is_iso8601_shape`] gate checks the FORMAT of a timestamp, not
//! its CALENDAR VALIDITY. SQLite's `strftime('%s', ?)` **rolls over** a
//! shape-valid but impossible DAY instead of returning NULL:
//!
//! - `strftime('%s','2025-02-30T00:00:00Z')` → the epoch for `2025-03-02`
//! - `strftime('%s','2025-04-31T00:00:00Z')` → the epoch for `2025-05-01`
//!
//! So a shape-valid Feb-30 passes the shape gate and is stored as a DIFFERENT
//! instant than the provider supplied — bypassing the hard-reject contract. (An
//! impossible MONTH like `2025-13-01` already NULLs out; only impossible DAYS
//! roll over, and impossible TIMES — `25:00:00`, `:60`, `:61` — also NULL out.)
//!
//! The fix is a round-trip check at the timestamp-normalisation write boundary:
//! the literal calendar DATE component of the input must survive SQLite's own
//! calendar math unchanged. Comparing the DATE component (not the raw string, and
//! not the UTC-rendered instant) is tz-invariant — a valid `+05:00` offset shifts
//! the instant but never the literal date field, so it is NOT false-rejected.
//!
//! This is a WRITE-BOUNDARY property: the assertion is the raw at-rest edge count
//! / stored `t_valid`, NOT a search result.

use fathomdb_engine::{Engine, EngineError, ExtractDocument};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

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

/// A shape-valid but calendar-impossible DAY must hard-reject at the extractor
/// boundary — NOT roll over to a neighbouring instant — and NOT ONE edge row may
/// reach the table. The raw-table `count(*)` is the load-bearing assertion.
fn assert_wire_hard_reject(doc_id: &str, what: &str) {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "reject");

    let result = ingest(&path, doc_id);

    let err = match result {
        Ok(n) => panic!(
            "TC-47: {what} ({doc_id}) is a calendar-impossible date and MUST hard-reject at the \
             extractor boundary, but ingest succeeded and wrote {n} edge(s). `strftime` ROLLED IT \
             OVER to a neighbouring instant instead of returning NULL — the round-trip DATE check \
             must reject it before it is stored."
        ),
        Err(e) => e,
    };

    let rendered = format!("{err:?}");
    assert!(
        rendered.contains("t_valid") || rendered.contains("t_invalid"),
        "TC-47: rejection of {what} must name the offending temporal field; got {rendered}"
    );

    assert_eq!(
        edge_count(&path),
        0,
        "TC-47: {what} ({doc_id}) was rejected, so NO edge row may reach canonical_edges \
         (raw at-rest table check)."
    );
}

/// A valid ISO-8601 timestamp must STILL pass and store the CORRECT epoch —
/// proving the round-trip check does not false-reject valid equivalent forms.
fn assert_wire_accepts(doc_id: &str, expected_epoch: i64) {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "accept");
    ingest(&path, doc_id)
        .unwrap_or_else(|e| panic!("TC-47: valid ISO-8601 ({doc_id}) must ingest, got {e:?}"));

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
        "TC-47: valid ISO-8601 ({doc_id}) must normalise to the CORRECT epoch (no false-reject)"
    );
}

// ---------------------------------------------------------------------------
// Calendar-impossible DAYS must hard-reject (RED today — they roll over)
// ---------------------------------------------------------------------------

/// `2025-02-30T00:00:00Z` — February has no 30th. `strftime` rolls it to
/// `2025-03-02`; the round-trip DATE check must reject it.
#[test]
fn tc47_feb30_rolls_over_and_must_hard_reject() {
    assert_wire_hard_reject("doc-tc47-feb30-tvalid", "impossible day \"2025-02-30T00:00:00Z\"");
}

/// `2025-04-31T00:00:00Z` — April has no 31st. `strftime` rolls it to
/// `2025-05-01`; the round-trip DATE check must reject it.
#[test]
fn tc47_apr31_rolls_over_and_must_hard_reject() {
    assert_wire_hard_reject("doc-tc47-apr31-tvalid", "impossible day \"2025-04-31T00:00:00Z\"");
}

/// The TC-44 Julian-day string `2451545.0` must STILL hard-reject (the round-trip
/// check is a superset backstop — it rejects this too, independent of the shape
/// gate).
#[test]
fn tc47_julian_day_string_still_hard_rejects() {
    assert_wire_hard_reject("doc-tc33-julian-tvalid", "Julian-day string \"2451545.0\"");
}

// ---------------------------------------------------------------------------
// Valid equivalent forms must NOT be false-rejected (the tz trap)
// ---------------------------------------------------------------------------

/// Positive control — canonical `Z` datetime.
#[test]
fn tc47_iso_z_datetime_accepted() {
    assert_wire_accepts("doc-tc33-iso-z", 1_742_463_000); // 2025-03-20T09:30:00Z
}

/// Positive control — `+00:00` offset (equivalent representation of `Z`).
#[test]
fn tc47_iso_offset_zero_accepted() {
    assert_wire_accepts("doc-tc33-iso-offset", 1_742_463_000); // 2025-03-20T09:30:00+00:00
}

/// Positive control — a NON-UTC `+05:00` offset. THE tz trap: a naive raw-string
/// or UTC-rendered round-trip would wrongly reject this; the DATE-component check
/// must accept it and store the correct (offset-shifted) epoch.
#[test]
fn tc47_iso_offset_plus_five_accepted() {
    assert_wire_accepts("doc-tc47-iso-offset5", 1_742_445_000); // 2025-03-20T09:30:00+05:00
}

/// Positive control — date-only (no time, no zone).
#[test]
fn tc47_iso_date_only_accepted() {
    assert_wire_accepts("doc-tc33-iso-dateonly", 1_742_428_800); // 2025-03-20 (midnight UTC)
}

/// Positive control — fractional seconds.
#[test]
fn tc47_iso_fractional_seconds_accepted() {
    assert_wire_accepts("doc-tc47-iso-frac", 1_742_463_000); // 2025-03-20T09:30:00.500Z
}
