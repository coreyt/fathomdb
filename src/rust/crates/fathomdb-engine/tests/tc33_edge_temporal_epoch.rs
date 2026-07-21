//! TC-33 — edge temporal representation is INTEGER epoch seconds, with a
//! HARD-REJECT normalisation boundary at the BYO-LLM extractor seam.
//!
//! Ratified by HITL 2026-07-21 (`dev/plans/plan-0.8.20.md` §9 decision 3):
//! `canonical_edges.t_valid` / `t_invalid` become **INTEGER epoch seconds** in
//! storage and on the governed SDK surface; the **BYO-LLM extractor boundary
//! keeps ISO-8601**, normalised engine-side with hard rejection.
//!
//! # Why this slice exists: fail-open is the defect
//!
//! An unparseable timestamp must NEVER coerce to SQL `NULL`, because a NULL
//! `t_invalid` reads as **"still valid"** (`fathomdb-schema/src/lib.rs`, step
//! 14 comment; relied on at every edge read site). So junk silently
//! **resurrects an invalidated edge**.
//!
//! The polarity matters and is easy to get backwards:
//!
//! - **Before this slice the behaviour failed CLOSED by accident.**
//!   `datetime('not a date')` → NULL; `NULL > datetime('now')` → NULL;
//!   `t_invalid IS NULL` → false; the whole disjunct is falsy ⇒ the junk row
//!   silently VANISHED from every read. Wrong, but conservative.
//! - **A naive migration INVERTS that polarity.** Unparseable → `NULL` in an
//!   INTEGER column ⇒ `t_invalid IS NULL` ⇒ "still valid" ⇒ an invalidated edge
//!   is RESURRECTED. That is the hazard these tests exist to prevent.
//!
//! Defence in depth, all four layers asserted here:
//!
//! 1. `strftime('%s', ?1)` normalisation on a BOUND parameter, with a typed
//!    error when the result is NULL (`P1`, `P2`).
//! 2. Schema-level type CHECKs so the invariant is STRUCTURAL, not upheld by
//!    call sites (`P4`). Note `NOT NULL` would be WRONG — NULL legitimately
//!    means "still valid" and that semantic must survive.
//! 3. These RED tests, proving malformed input fails LOUDLY (`P1`–`P3`).
//! 4. `temporal_fallback` re-grounded through the SAME normalisation on BOTH
//!    sides of the comparison (`P5`).

use fathomdb_engine::{Engine, ExtractDocument};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Test infrastructure (mirrors slice15_byo_llm_ingest.rs)
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

/// Ingest one fixture document through the stub extractor harness.
fn ingest(path: &std::path::Path, doc_id: &str) -> Result<u64, fathomdb_engine::EngineError> {
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

/// Assert that a malformed/ill-typed extractor timestamp is rejected LOUDLY and
/// that NOT ONE edge row reaches the table. The second half is the load-bearing
/// part: a rejection that still wrote a row with NULL `t_invalid` would be
/// exactly the fail-open this slice exists to remove.
fn assert_hard_reject(doc_id: &str, what: &str) {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "reject");

    let result = ingest(&path, doc_id);

    let err = match result {
        Ok(n) => panic!(
            "TC-33 P1/P2: {what} ({doc_id}) MUST hard-reject at the extractor \
             boundary, but ingest succeeded and wrote {n} edge(s). Fail-open: an \
             unparseable timestamp that becomes NULL t_invalid reads as \"still \
             valid\" and RESURRECTS an invalidated edge."
        ),
        Err(e) => e,
    };

    // The rejection must be a TYPED, diagnosable error naming the offending
    // value — not a generic Storage error and not a silent coercion.
    let rendered = format!("{err:?}");
    assert!(
        rendered.contains("t_valid") || rendered.contains("t_invalid"),
        "TC-33 P1: rejection of {what} must name the offending temporal field so \
         the caller can diagnose it; got {rendered}"
    );

    assert_eq!(
        edge_count(&path),
        0,
        "TC-33 P3: {what} ({doc_id}) was rejected, so NO edge row may reach \
         canonical_edges. A surviving row would carry a NULL t_invalid, which \
         reads as \"still valid\"."
    );
}

// ---------------------------------------------------------------------------
// P1 — a malformed ISO-8601 STRING hard-rejects
// ---------------------------------------------------------------------------

/// `strftime('%s', 'not a date')` is NULL. Under the pre-TC-33 TEXT column that
/// junk was stored verbatim and then silently vanished from reads. Under an
/// INTEGER column a naive migration would store NULL ⇒ "still valid". Neither
/// is acceptable: it must fail at the write boundary.
#[test]
fn tc33_p1_malformed_t_valid_string_hard_rejects() {
    assert_hard_reject("doc-tc33-bad-tvalid", "malformed t_valid string \"not a date\"");
}

#[test]
fn tc33_p1_malformed_t_invalid_string_hard_rejects() {
    assert_hard_reject(
        "doc-tc33-bad-tinvalid",
        "malformed t_invalid string \"yesterday sometime\"",
    );
}

/// The empty string is the most likely junk value a real LLM emits for
/// "no timestamp" — and `strftime('%s', '')` is NULL, so it would coerce
/// straight to "still valid". JSON `null` is the ONLY sanctioned way to say
/// "unknown"; `""` is malformed.
#[test]
fn tc33_p1_empty_string_timestamp_hard_rejects() {
    assert_hard_reject("doc-tc33-empty-tvalid", "empty-string t_valid");
}

// ---------------------------------------------------------------------------
// P2 — a present-but-NON-STRING timestamp hard-rejects (pre-existing fail-open)
// ---------------------------------------------------------------------------

/// **This fail-open predates the TC-33 migration.** The parse site reads
/// `edge.get("t_invalid").and_then(|v| v.as_str())`, and `as_str()` returns
/// `None` for a JSON NUMBER. So an extractor emitting
/// `"t_invalid": 1710000000` — a perfectly plausible mistake, and exactly the
/// epoch representation TC-33 moves storage to — had its invalidation SILENTLY
/// DISCARDED and the edge stored as "still valid".
///
/// The extractor boundary is ISO-8601 STRINGS ONLY. A present, non-null,
/// non-string timestamp is a protocol violation and must be rejected, never
/// coerced.
#[test]
fn tc33_p2_numeric_t_invalid_hard_rejects_instead_of_coercing_to_null() {
    assert_hard_reject(
        "doc-tc33-numeric-tinvalid",
        "JSON-number t_invalid 1710000000 (as_str() → None ⇒ silently NULL today)",
    );
}

#[test]
fn tc33_p2_boolean_t_valid_hard_rejects() {
    assert_hard_reject("doc-tc33-bool-tvalid", "JSON-boolean t_valid");
}

#[test]
fn tc33_p2_object_t_valid_hard_rejects() {
    assert_hard_reject("doc-tc33-object-tvalid", "JSON-object t_valid");
}

// ---------------------------------------------------------------------------
// P3 — a well-formed ISO-8601 string normalises to INTEGER epoch seconds
// ---------------------------------------------------------------------------

/// The positive case. `doc-temporal` carries `t_valid = "2020-01-01T00:00:00Z"`.
/// After TC-33 it is STORED as the integer `1577836800`, not as text.
///
/// This is the assertion `slice15_byo_llm_ingest.rs` used to spell as
/// *"t_valid must be preserved from extract response"* — "preserved" is exactly
/// the contract TC-33 changes, so it becomes "normalised equivalently".
#[test]
fn tc33_p3_wellformed_iso_normalises_to_integer_epoch() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "normalise");
    ingest(&path, "doc-temporal").expect("well-formed ISO-8601 must ingest");

    let conn = Connection::open(&path).unwrap();
    let (value, sqlite_type): (i64, String) = conn
        .query_row(
            "SELECT t_valid, typeof(t_valid) FROM canonical_edges \
             WHERE kind = 'works_for' AND superseded_at IS NULL",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("'works_for' edge must exist");

    assert_eq!(
        sqlite_type, "integer",
        "TC-33: canonical_edges.t_valid must be stored as INTEGER epoch seconds"
    );
    assert_eq!(value, 1_577_836_800, "TC-33: 2020-01-01T00:00:00Z normalises to epoch 1577836800");
}

// ---------------------------------------------------------------------------
// P4 — the schema CHECK makes junk UNSTORABLE (structural, not call-site)
// ---------------------------------------------------------------------------

/// Layer 2 of the defence. TC-28 already records an invariant held only by call
/// sites; this must not repeat that. Even a caller that bypasses the engine's
/// normalisation cannot get a TEXT timestamp into the column.
///
/// **`NOT NULL` would be WRONG here** — NULL legitimately means "still valid".
/// The correct structural spelling is a TYPE check, which makes junk unstorable
/// while preserving NULL-means-still-valid.
#[test]
fn tc33_p4_schema_check_rejects_text_timestamp() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "check");
    let _opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let conn = Connection::open(&path).unwrap();

    for column in ["t_valid", "t_invalid"] {
        let sql = format!(
            "INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id, {column}) \
             VALUES(1, 'k', 'a', 'b', 'not a date')"
        );
        let err = conn.execute(&sql, []).expect_err(&format!(
            "TC-33 P4: the schema must make a TEXT canonical_edges.{column} \
             UNSTORABLE — the invariant has to be structural, not upheld by call sites"
        ));
        let rendered = format!("{err}");
        assert!(
            rendered.to_lowercase().contains("constraint"),
            "TC-33 P4: {column} rejection must be a CHECK-constraint violation; got {rendered}"
        );
    }
}

/// NULL must STILL be storable and must still mean "still valid". This is the
/// precision guard on P4: a `NOT NULL` constraint would pass "junk is
/// unstorable" while destroying the shipped semantic.
#[test]
fn tc33_p4_null_timestamp_remains_storable_and_means_still_valid() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "nullok");
    let _opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let conn = Connection::open(&path).unwrap();

    conn.execute(
        "INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id, t_valid, t_invalid) \
         VALUES(1, 'k', 'a', 'b', NULL, NULL)",
        [],
    )
    .expect("TC-33 P4: NULL must remain storable — NULL means \"still valid\"");

    let still_valid: i64 = conn
        .query_row("SELECT COUNT(*) FROM canonical_edges WHERE t_invalid IS NULL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(still_valid, 1, "NULL t_invalid must still read as \"still valid\"");
}

/// **The headline polarity test.** Junk must never be able to resurrect an
/// edge that has already been invalidated.
///
/// An edge invalidated in the past (`t_invalid` < now) is invisible. If junk
/// could land in that column as NULL, `t_invalid IS NULL` would fire and the
/// edge would come BACK. The CHECK makes that transition unreachable.
#[test]
fn tc33_p4_junk_cannot_resurrect_an_invalidated_edge() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "resurrect");
    let _opened = Engine::open_without_embedder_for_test(&path).expect("open");
    let conn = Connection::open(&path).unwrap();

    // An edge invalidated at epoch 1577836800 (2020-01-01) — long past.
    conn.execute(
        "INSERT INTO canonical_edges(write_cursor, kind, from_id, to_id, logical_id, t_invalid) \
         VALUES(1, 'k', 'a', 'b', 'edge:1', 1577836800)",
        [],
    )
    .expect("insert invalidated edge");

    // Try to overwrite it with junk. This is the resurrection vector.
    let err = conn
        .execute(
            "UPDATE canonical_edges SET t_invalid = 'not a date' WHERE logical_id = 'edge:1'",
            [],
        )
        .expect_err(
            "TC-33 P4: writing junk over a past t_invalid must be REJECTED. If it \
             coerced to NULL the edge would read as \"still valid\" again — an \
             invalidated edge silently resurrected.",
        );
    assert!(format!("{err}").to_lowercase().contains("constraint"));

    // Still invalidated, still an integer, still in the past.
    let (t_invalid, sqlite_type): (i64, String) = conn
        .query_row(
            "SELECT t_invalid, typeof(t_invalid) FROM canonical_edges WHERE logical_id = 'edge:1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(t_invalid, 1_577_836_800, "the invalidation must survive the rejected write");
    assert_eq!(sqlite_type, "integer");
}

// ---------------------------------------------------------------------------
// P5 — temporal_fallback re-grounded through the SAME normalisation
// ---------------------------------------------------------------------------

/// `temporal_fallback` is matched by comparing the edge's `t_valid` against the
/// `substituted_t_valid` values on the ELPS warnings envelope. That comparison
/// was a RAW BYTE-FOR-BYTE STRING MATCH, with `.unwrap_or(false)` on the miss
/// path — so if only the edge side is normalised, the set NEVER matches, every
/// fallback edge silently becomes a TRUSTED edge, and NOTHING fails to compile.
///
/// This flag is the only thing excluding untrustworthy-time edges from graph
/// BFS and graph seeding, so a silent regression here is a correctness bug in
/// the graph arm.
///
/// The fixture deliberately spells the two sides DIFFERENTLY —
/// `2025-03-20T09:30:00+00:00` on the edge, `2025-03-20T09:30:00Z` on the
/// warning. They are the SAME INSTANT. The old string match MISSED this pair;
/// normalising both sides through one function fixes it.
#[test]
fn tc33_p5_temporal_fallback_still_flags_after_normalisation() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "fallback");
    ingest(&path, "doc-tc33-fallback-tzform").expect("fallback fixture must ingest");

    let conn = Connection::open(&path).unwrap();
    let (fallback, t_valid): (Option<i64>, i64) = conn
        .query_row(
            "SELECT temporal_fallback, t_valid FROM canonical_edges \
             WHERE kind = 'works_for' AND superseded_at IS NULL",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the fallback edge must exist");

    assert_eq!(
        t_valid, 1_742_463_000,
        "both spellings normalise to the same instant (2025-03-20T09:30:00 UTC)"
    );
    assert_eq!(
        fallback,
        Some(1),
        "TC-33 P5: temporal_fallback must STILL be flagged after normalisation. \
         Both sides must go through the SAME normalisation before comparison — \
         otherwise the set never matches, .unwrap_or(false) fires, and every \
         fallback edge silently becomes a trusted edge with no compile error."
    );
}
