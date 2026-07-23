//! TC-33 fix-1 — an edge epoch that SQLite cannot render back to ISO-8601 must
//! be UNSTORABLE at the governed write boundary (codex §9 P2, 2026-07-21).
//!
//! # The fail-open this closes (outbound polarity of TC-33)
//!
//! Storage and the governed SDK surface (`PreparedWrite::Edge`) carry INTEGER
//! epoch seconds and accept an arbitrary `i64`. The consolidation path renders
//! each candidate edge's `t_valid`/`t_invalid` back to ISO-8601 for the LLM via
//! `strftime(..., 'unixepoch')`, which only covers years 0000..=9999. For an
//! epoch OUTSIDE that range (`> 253402300799`, i.e. year ≥ 10000, or before year
//! 0), `strftime` returns NULL — and the render site's `and_then` chain then
//! sent a silent `null` for a timestamp that is actually stored NON-NULL.
//!
//! That is the SAME fail-open TC-33 removes, just outbound: a `null` `t_invalid`
//! reads as "still valid", and because the consolidation reference stub echoes
//! the winner's `t_valid` straight back as the verdict's `t_invalid`, the `null`
//! round-trips through the inbound normaliser as "still valid" — an invalidated
//! edge silently resurrected via the consolidation path.
//!
//! Inbound ISO normalisation can NEVER mint such an epoch (a 4-digit-year ISO
//! string maxes at 9999), so the ONLY ingress is the governed integer surface.
//! The structural fix therefore lives at that write boundary: an unrenderable
//! epoch is rejected before it can be stored, mirroring the inbound hard-reject.

use fathomdb_engine::{Engine, EngineError, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

// The inclusive SQLite-`strftime('%s','unixepoch')`-renderable epoch bounds
// (years 0000..=9999). Duplicated here from the engine constants so the test
// pins the numbers independently of the implementation.
const MAX_RENDERABLE_EPOCH: i64 = 253_402_300_799; // 9999-12-31T23:59:59Z
const FIRST_UNRENDERABLE_ABOVE: i64 = MAX_RENDERABLE_EPOCH + 1; // year 10000

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn edge_with_temporal(t_valid: Option<i64>, t_invalid: Option<i64>) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: "link".to_string(),
        from: "A".to_string(),
        to: "B".to_string(),
        source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
        logical_id: Some("edge:tc33-fix1".to_string()),
        body: None,
        t_valid,
        t_invalid,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

fn edge_count(path: &std::path::Path) -> i64 {
    let conn = Connection::open(path).unwrap();
    conn.query_row("SELECT COUNT(*) FROM canonical_edges", [], |r| r.get(0)).unwrap()
}

/// Write `edge` through the governed surface, assert it is rejected LOUDLY with
/// a typed `InvalidArgument` naming the offending field, and assert NOT ONE edge
/// row reaches the table. A surviving row would carry an epoch that renders to
/// `null` on the consolidation wire — the resurrection vector this fix removes.
fn assert_unrenderable_rejected(name: &str, edge: PreparedWrite, expect_field: &str) {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, name);
    let engine = Engine::open(&path).expect("open").engine;

    let result = engine.write(&[edge]);

    let err = match result {
        Ok(_) => panic!(
            "TC-33 fix-1: an unrenderable epoch MUST be rejected at the governed \
             write boundary, but the write succeeded. Such an epoch renders to \
             NULL on the consolidation wire, which reads as \"still valid\" and \
             silently resurrects an invalidated edge."
        ),
        Err(e) => e,
    };

    match &err {
        EngineError::InvalidArgument { msg } => assert!(
            msg.contains(expect_field),
            "TC-33 fix-1: rejection must name the offending field `{expect_field}` \
             so the caller can diagnose it; got {msg}"
        ),
        other => panic!(
            "TC-33 fix-1: an unrenderable epoch must be a typed InvalidArgument \
             naming the field and the bound, not {other:?}"
        ),
    }

    engine.close().unwrap();

    assert_eq!(
        edge_count(&path),
        0,
        "TC-33 fix-1: an unrenderable epoch was rejected, so NO edge row may reach \
         canonical_edges — a surviving row renders to NULL on the wire (\"still valid\")."
    );
}

/// `t_invalid` at year 10000 (one second past the renderable max) must be
/// rejected. This is the headline resurrection vector: an invalidated edge whose
/// `t_invalid` renders to `null` reads as "still valid" again.
#[test]
fn tc33_fix1_unrenderable_t_invalid_is_rejected() {
    assert_unrenderable_rejected(
        "unrend_tinvalid",
        edge_with_temporal(None, Some(FIRST_UNRENDERABLE_ABOVE)),
        "t_invalid",
    );
}

/// `t_valid` at year 10000 must be rejected too — the render site loses it the
/// same way, and the reference stub echoes a lost `t_valid` back as a verdict.
#[test]
fn tc33_fix1_unrenderable_t_valid_is_rejected() {
    assert_unrenderable_rejected(
        "unrend_tvalid",
        edge_with_temporal(Some(FIRST_UNRENDERABLE_ABOVE), None),
        "t_valid",
    );
}

/// Precision guard: the LAST renderable instant (9999-12-31T23:59:59Z) must
/// still be STORABLE. An over-tight bound that rejected this would break a
/// legitimate far-future validity window.
#[test]
fn tc33_fix1_max_renderable_epoch_is_storable() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "max_renderable");
    let engine = Engine::open(&path).expect("open").engine;

    engine
        .write(&[edge_with_temporal(Some(0), Some(MAX_RENDERABLE_EPOCH))])
        .expect("TC-33 fix-1: the max renderable epoch (9999-12-31T23:59:59Z) must be storable");

    engine.close().unwrap();
    assert_eq!(edge_count(&path), 1, "the boundary-max edge must be written");
}
