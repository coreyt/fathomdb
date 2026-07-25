//! TC-45 (plan-0.8.20 §11 item 7, HITL-RULED 2026-07-24) — a superseded edge
//! cursor MUST receive a `_fathomdb_projection_terminal` row so the shared
//! projection cursor can walk past it.
//!
//! **The defect.** `commit_batch`'s two supersession prune loops (the G0
//! `logical_id` path and the G11 `(from, to, kind)` triple path) called
//! `record_projection_terminal(.., "superseded")`. The terminal table's CHECK
//! (schema step 7) is `CHECK(state IN ('failed', 'up_to_date'))` and the writer
//! is `INSERT OR IGNORE`; under SQLite `OR IGNORE` SKIPS a CHECK-violating row
//! and returns NO error, so the terminal was SILENTLY DROPPED.
//!
//! **Why that is a permanent stall, not a transient gap.** Nothing else will
//! ever record that terminal:
//!
//! - `next_pending_projection_jobs`'s edge arm carries `AND
//!   canonical_edges.superseded_at IS NULL` (fix-31), so the embed scheduler
//!   never picks the superseded cursor up as work; and
//! - `database_has_pending_projection_work` carries the same exclusion, so
//!   `drain()` reports IDLE while the hole is still open.
//!
//! `advance_projection_cursor` walks `cursor + 1` forward ONLY while
//! `terminal_state_for_cursor(next)` is `Some(..)`, so it wedges at the hole
//! forever. `write_cursor` is a SINGLE global sequence shared by nodes AND
//! edges, so one superseded edge freezes the readiness watermark for every
//! LATER write too.
//!
//! **Test-design contract.** Both witnesses assert on RAW TABLE CONTENTS
//! (`SELECT ... FROM _fathomdb_projection_terminal WHERE write_cursor = ?`),
//! never on a derived status. `projection_status` maps every non-`'failed'`
//! terminal — and a MISSING terminal — through to `UpToDate`, so a
//! status-level assertion passes on the BROKEN code (vacuous green).
//!
//! **Fixture shape (load-bearing).** Both edges of a pair are written in ONE
//! batch. For edges `project_canonical_edge_row` sets `enqueue_vector =
//! body.is_some()`, so a BODY-BEARING edge gets NO write-time terminal — the
//! embed worker is supposed to record it after projection. Writing the pair in
//! one batch supersedes the first edge inside the same transaction, before any
//! worker can embed it, which is exactly the window the defect lives in. The
//! surviving edge is then embedded by `drain()` and DOES gain its terminal, so
//! the cursor-advance assertion isolates the hole at the superseded cursor.
//!
//! The sibling schema-side test
//! `fathomdb-schema/tests/tc33_fix4_projection_cursor_stall.rs` asserts the
//! CHECK's own behaviour (that `'superseded'` is rejected and `OR IGNORE`
//! swallows it). That remains true and is the direct evidence for this defect;
//! the fix here is code-side (pass the CHECK-valid token), with NO migration
//! and NO `SCHEMA_VERSION` bump.

use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct DeterministicEmbedder {
    identity: EmbedderIdentity,
    vector: Vector,
}

impl DeterministicEmbedder {
    fn new(dim: u32) -> Self {
        let mut vector = vec![0.0_f32; dim as usize];
        vector[0] = 1.0;
        Self { identity: EmbedderIdentity::new("tc45-test", "rev-a", dim), vector }
    }
}

impl Embedder for DeterministicEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        Ok(self.vector.clone())
    }
}

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

/// A body-bearing ("fact") edge. `body: Some(..)` is load-bearing: it is what
/// defers the readiness terminal to the async embed worker, which is the window
/// the supersession prune must close itself.
fn fact_edge(
    kind: &str,
    from: &str,
    to: &str,
    logical_id: Option<&str>,
    body: &str,
) -> PreparedWrite {
    PreparedWrite::Edge {
        kind: kind.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        source_id: fathomdb_engine::SourceId::new("tc45:fixture").expect("source id"),
        logical_id: logical_id.map(str::to_string),
        body: Some(body.to_string()),
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    }
}

/// Raw terminal rows for one cursor: the `state` token at rest, if any.
fn terminal_state(conn: &Connection, cursor: u64) -> Option<String> {
    conn.query_row(
        "SELECT state FROM _fathomdb_projection_terminal WHERE write_cursor = ?1",
        [cursor],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

fn terminal_count(conn: &Connection, cursor: u64) -> u64 {
    conn.query_row(
        "SELECT COUNT(*) FROM _fathomdb_projection_terminal WHERE write_cursor = ?1",
        [cursor],
        |r| r.get(0),
    )
    .expect("count terminal rows")
}

/// The durable projection cursor (`_fathomdb_open_state.projection_cursor`),
/// i.e. the readiness watermark `advance_projection_cursor` maintains. Absent
/// key means 0, matching `load_projection_cursor`.
fn projection_cursor(conn: &Connection) -> u64 {
    conn.query_row(
        "SELECT value FROM _fathomdb_open_state WHERE key = 'projection_cursor'",
        [],
        |r| r.get::<_, String>(0),
    )
    .map(|v| v.parse::<u64>().unwrap_or(0))
    .unwrap_or(0)
}

/// Drive one supersession pair through the engine and hand back
/// `(superseded_cursor, survivor_cursor, path_to_db)`.
fn run_pair(
    dir: &TempDir,
    name: &str,
    first: PreparedWrite,
    second: PreparedWrite,
) -> (u64, u64, std::path::PathBuf) {
    let path = db_path(dir, name);
    let embedder = Arc::new(DeterministicEmbedder::new(8));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");

    // ONE batch: the second write supersedes the first INSIDE the same
    // transaction, before the embed worker can terminal the first cursor.
    let receipt = opened.engine.write(&[first, second]).expect("write supersession pair");
    assert_eq!(receipt.row_cursors.len(), 2, "expected one cursor per edge");
    let superseded = receipt.row_cursors[0];
    let survivor = receipt.row_cursors[1];

    opened.engine.drain(10_000).expect("drain embed work");
    opened.engine.close().expect("close");

    (superseded, survivor, path)
}

/// Shared assertions for both supersession paths.
fn assert_terminal_and_advance(
    path: &std::path::Path,
    superseded: u64,
    survivor: u64,
    label: &str,
) {
    let conn = Connection::open(path).expect("open sqlite");

    // Guard against a vacuous fixture: the first edge must really have been
    // superseded, and the survivor must really still be active.
    let superseded_at: Option<i64> = conn
        .query_row(
            "SELECT superseded_at FROM canonical_edges WHERE write_cursor = ?1",
            [superseded],
            |r| r.get(0),
        )
        .expect("read superseded edge");
    assert!(
        superseded_at.is_some(),
        "{label}: fixture is vacuous — cursor {superseded} was never superseded"
    );
    let survivor_active: Option<i64> = conn
        .query_row(
            "SELECT superseded_at FROM canonical_edges WHERE write_cursor = ?1",
            [survivor],
            |r| r.get(0),
        )
        .expect("read survivor edge");
    assert!(
        survivor_active.is_none(),
        "{label}: fixture is vacuous — survivor cursor {survivor} is itself superseded"
    );
    // The survivor's terminal proves the embed worker ran, so a missing
    // terminal on `superseded` is the supersession hole and not a dead worker.
    assert_eq!(
        terminal_count(&conn, survivor),
        1,
        "{label}: fixture is vacuous — the surviving edge never gained a terminal, \
         so the embed worker did not run"
    );

    // (i) RAW-TABLE witness: the superseded cursor has a terminal row at rest.
    assert_eq!(
        terminal_count(&conn, superseded),
        1,
        "{label}: TC-45 — no _fathomdb_projection_terminal row for superseded cursor \
         {superseded}; the CHECK-violating 'superseded' token was swallowed by \
         INSERT OR IGNORE, and no scheduler will ever backfill it (the job query \
         excludes superseded edges)"
    );
    assert_eq!(
        terminal_state(&conn, superseded).as_deref(),
        Some("up_to_date"),
        "{label}: TC-45 — the recorded token must be the CHECK-valid 'up_to_date' \
         (schema step 7 allows only 'failed' / 'up_to_date'); a superseded cursor \
         needs no further projection work, which is what 'up_to_date' means"
    );

    // (ii) The readiness watermark walked PAST the superseded cursor instead of
    // wedging on it.
    let cursor = projection_cursor(&conn);
    assert!(
        cursor >= survivor,
        "{label}: TC-45 — projection cursor STALLED at {cursor}; it must advance to \
         at least {survivor} (past superseded cursor {superseded}). \
         advance_projection_cursor walks cursor+1 only while a terminal exists, so \
         the missing terminal wedges the shared node+edge readiness watermark forever"
    );
}

// ---------------------------------------------------------------------------
// G0 path — supersession keyed by `logical_id` alone
// (`prior_edge_cursors_by_logical_id`).
// ---------------------------------------------------------------------------

/// Two body-bearing edges sharing a `logical_id` but with DIFFERENT
/// `(from, to, kind)` triples, so ONLY the G0 prune loop fires and this witness
/// isolates that call site.
#[test]
fn tc45_g0_superseded_edge_cursor_gets_terminal_and_cursor_advances() {
    let dir = TempDir::new().unwrap();
    let (superseded, survivor, path) = run_pair(
        &dir,
        "tc45_g0",
        fact_edge("rel", "a", "b", Some("E1"), "Alice knows Bob"),
        // Same logical_id, different `to` ⇒ the triple differs ⇒ G11 finds
        // nothing and G0 alone supersedes the first cursor.
        fact_edge("rel", "a", "c", Some("E1"), "Alice knows Carol"),
    );
    assert_terminal_and_advance(&path, superseded, survivor, "G0/logical_id");
}

// ---------------------------------------------------------------------------
// G11 path — invalidate-not-accumulate keyed by the `(from, to, kind)` triple
// (`prior_edge_cursors_by_triple`).
// ---------------------------------------------------------------------------

/// Two body-bearing edges sharing `(from, to, kind)` and carrying NO
/// `logical_id`, so the G0 prune loop is skipped entirely (`if let Some(..)`)
/// and ONLY the G11 call site can record the terminal.
#[test]
fn tc45_g11_superseded_edge_cursor_gets_terminal_and_cursor_advances() {
    let dir = TempDir::new().unwrap();
    let (superseded, survivor, path) = run_pair(
        &dir,
        "tc45_g11",
        fact_edge("rel", "a", "b", None, "Alice knows Bob"),
        // Identical triple, no logical_id ⇒ G0 skipped, G11 supersedes.
        fact_edge("rel", "a", "b", None, "Alice no longer knows Bob"),
    );
    assert_terminal_and_advance(&path, superseded, survivor, "G11/triple");
}
