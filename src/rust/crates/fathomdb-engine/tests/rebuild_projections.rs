use std::sync::Arc;
use std::thread;
use std::time::Duration;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::lifecycle::ProjectionStatus;
use fathomdb_engine::{ConsolidateAxis, Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

const SENTINEL: &[u8; 16] = b"FATHOMDB_SENT_42";

#[derive(Clone, Debug)]
struct DeterministicEmbedder {
    identity: EmbedderIdentity,
    vector: Vector,
}

impl DeterministicEmbedder {
    fn new(dim: u32) -> Self {
        let mut vector = vec![0.0_f32; dim as usize];
        vector[0] = 1.0;
        Self { identity: EmbedderIdentity::new("rebuild", "rev-a", dim), vector }
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

#[derive(Clone, Debug)]
struct FailingEmbedder {
    identity: EmbedderIdentity,
    fails: Arc<std::sync::atomic::AtomicUsize>,
}

impl FailingEmbedder {
    fn new(dim: u32) -> Self {
        Self {
            identity: EmbedderIdentity::new("rebuild", "rev-a", dim),
            fails: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
}

impl Embedder for FailingEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        self.fails.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Err(EmbedderError::Failed { message: "deterministic failure".to_string() })
    }
}

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn wait_until<F: FnMut() -> bool>(mut predicate: F, timeout: Duration) -> bool {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if predicate() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    predicate()
}

/// 0.8.12 Slice A: count edge-FTS shadow rows matching `term` in
/// `search_index_edges` (mirrors `edge_fts_count` in `consolidate_provider.rs`).
fn edge_fts_count(conn: &Connection, term: &str) -> u64 {
    conn.query_row(
        "SELECT COUNT(*) FROM search_index_edges WHERE search_index_edges MATCH ?1",
        [term],
        |r| r.get(0),
    )
    .expect("search_index_edges must exist and be queryable")
}

/// 0.8.12 Slice A: count `_fathomdb_vector_rows` rows for a given `write_cursor`.
fn vector_row_count(conn: &Connection, cursor: u64) -> u64 {
    conn.query_row(
        "SELECT COUNT(*) FROM _fathomdb_vector_rows WHERE write_cursor = ?1",
        [cursor],
        |r| r.get(0),
    )
    .expect("_fathomdb_vector_rows must exist and be queryable")
}

#[test]
fn ac_044_rebuild_projections_purges_sentinel_bytes() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "rebuild_sentinel");

    let embedder = Arc::new(DeterministicEmbedder::new(8));
    {
        let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
        opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
        opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: "canonical body alpha".to_string(),
                source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
                logical_id: None,
                state: fathomdb_engine::InitialState::Active,
                reason: None,
                valid_from: None,
                valid_until: None,
            }])
            .expect("write");
        opened.engine.drain(10_000).expect("drain");
    }

    {
        let connection = Connection::open(&path).expect("open sqlite");
        connection
            .execute(
                "INSERT INTO search_index(body, kind, write_cursor) VALUES(?1, 'doc', 9999)",
                rusqlite::params![std::str::from_utf8(SENTINEL).unwrap()],
            )
            .expect("inject sentinel");
        connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_row| Ok(()))
            .expect("checkpoint");
    }

    let raw_before = std::fs::read(&path).expect("read db");
    assert!(
        raw_before.windows(SENTINEL.len()).any(|window| window == SENTINEL),
        "sentinel was not actually written into the file"
    );

    {
        let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("reopen");
        opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
        opened.engine.rebuild_projections().expect("rebuild_projections");
        assert!(wait_until(
            || opened
                .engine
                .projection_status_for_test("doc")
                .map(|s| s == ProjectionStatus::UpToDate)
                .unwrap_or(false),
            Duration::from_secs(10),
        ));
        opened.engine.drain(10_000).expect("post-rebuild drain");
    }

    {
        let connection = Connection::open(&path).expect("open sqlite");
        connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_row| Ok(()))
            .expect("checkpoint");
        connection.execute("VACUUM", []).expect("vacuum");
    }

    let raw_after = std::fs::read(&path).expect("read db");
    assert!(
        !raw_after.windows(SENTINEL.len()).any(|window| window == SENTINEL),
        "sentinel still present in shadow-table pages after rebuild"
    );
}

#[test]
fn ac_063c_rebuild_projections_materializes_failed_terminal_rows() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "rebuild_failed");

    let failing = Arc::new(FailingEmbedder::new(8));
    let cursor = {
        let opened = Engine::open_with_embedder_for_test(&path, failing.clone()).expect("open");
        opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
        opened.engine.set_projection_retry_delays_for_test(&[0, 0, 0]);
        let receipt = opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: "failure body".to_string(),
                source_id: fathomdb_engine::SourceId::new("test:fixture").expect("test source id"),
                logical_id: None,
                state: fathomdb_engine::InitialState::Active,
                reason: None,
                valid_from: None,
                valid_until: None,
            }])
            .expect("write");
        assert!(wait_until(
            || opened
                .engine
                .projection_status_for_test("doc")
                .map(|s| s == ProjectionStatus::Failed)
                .unwrap_or(false),
            Duration::from_secs(10),
        ));
        assert_eq!(
            opened.engine.projection_failure_count_for_test(receipt.cursor).expect("failure count"),
            1
        );
        receipt.cursor
    };

    let healthy = Arc::new(DeterministicEmbedder::new(8));
    let opened = Engine::open_with_embedder_for_test(&path, healthy.clone()).expect("reopen");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
    opened.engine.rebuild_projections().expect("rebuild_projections");
    assert!(wait_until(
        || opened
            .engine
            .projection_status_for_test("doc")
            .map(|s| s == ProjectionStatus::UpToDate)
            .unwrap_or(false),
        Duration::from_secs(10),
    ));
    assert!(opened.engine.has_vector_for_cursor_for_test(cursor).expect("has_vector"));
}

/// 0.8.12 Slice A (R-CON-2 named default-ON blocker; Slice-20 codex §9 [P2]).
///
/// Graph traversal excludes an invalidated edge via a
/// `t_invalid IS NULL OR datetime(t_invalid) > datetime('now')` filter, but
/// (before this fix) the FTS/vec PROJECTION rebuild SQL does not — so a full
/// `rebuild_projections()` re-materializes an invalidated (non-superseded)
/// edge's FTS/vec shadows, re-surfacing a stale contradiction that
/// consolidation had already hidden. This test drives the REAL
/// `consolidate_with_provider` recency path (deterministic stub harness) to
/// invalidate the older of two competing fact-edges, rebuilds, and asserts
/// the invalidated edge stays absent from both `search_index_edges` and
/// `_fathomdb_vector_rows` while the still-valid edge survives the rebuild
/// (guard against a vacuous test that hides everything).
#[test]
fn slice_a_rebuild_projections_excludes_recency_invalidated_edge() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "rebuild_tinvalid");

    let embedder = Arc::new(DeterministicEmbedder::new(8));
    let (acme_cursor, globex_cursor) = {
        let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
        opened.engine.configure_vector_kind_for_test("edge_fact").expect("vector kind");

        let older = PreparedWrite::Edge {
            kind: "works_for".to_string(),
            from: "bob".to_string(),
            to: "acme".to_string(),
            source_id: fathomdb_engine::SourceId::new("doc-acme").expect("test source id"),
            logical_id: Some("edge-acme".to_string()),
            body: Some("Bob works for Acme".to_string()),
            t_valid: Some(1_546_300_800), /* 2019-01-01T00:00:00Z */
            t_invalid: None,
            confidence: Some(0.9),
            extractor_model_id: Some("stub-extractor-v1".to_string()),
            temporal_fallback: None,
        };
        let newer = PreparedWrite::Edge {
            kind: "works_for".to_string(),
            from: "bob".to_string(),
            to: "globex".to_string(),
            source_id: fathomdb_engine::SourceId::new("doc-globex").expect("test source id"),
            logical_id: Some("edge-globex".to_string()),
            body: Some("Bob works for Globex".to_string()),
            t_valid: Some(1_640_995_200), /* 2022-01-01T00:00:00Z */
            t_invalid: None,
            confidence: Some(0.8),
            extractor_model_id: Some("stub-extractor-v1".to_string()),
            temporal_fallback: None,
        };
        let receipt = opened.engine.write(&[older, newer]).expect("seed two competing edges");
        assert_eq!(receipt.row_cursors.len(), 2, "batch of two edges");
        let acme_cursor = receipt.row_cursors[0];
        let globex_cursor = receipt.row_cursors[1];

        opened.engine.drain(10_000).expect("drain seed writes");

        // Guard: both edges are materialized (FTS + vec) before consolidation.
        {
            let conn = Connection::open(&path).unwrap();
            assert_eq!(edge_fts_count(&conn, "acme"), 1, "seed: acme edge must be FTS-indexed");
            assert_eq!(edge_fts_count(&conn, "globex"), 1, "seed: globex edge must be FTS-indexed");
        }
        assert!(
            opened.engine.has_vector_for_cursor_for_test(acme_cursor).expect("has_vector acme"),
            "seed: acme edge must be vector-projected"
        );
        assert!(
            opened.engine.has_vector_for_cursor_for_test(globex_cursor).expect("has_vector globex"),
            "seed: globex edge must be vector-projected"
        );

        // Consolidate via the REAL provider path: the deterministic stub
        // harness keeps the latest `t_valid` (globex) and invalidates the
        // older competing edge (acme) at the winner's `t_valid` — a PAST
        // timestamp, so acme is immediately "ended" per the recency filter
        // (mirrors `apply_consolidate_verdicts`'s `ended` check).
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/slice15_consolidate/stub_consolidate_harness.py");
        assert!(
            script.exists(),
            "consolidate stub harness fixture must exist at {}",
            script.display()
        );
        let cmd = ["python3".to_string(), script.to_string_lossy().to_string()];
        let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        let axes = vec![ConsolidateAxis {
            subject_logical_id: "bob".to_string(),
            relation: "works_for".to_string(),
        }];
        let consolidate_receipt = opened
            .engine
            .consolidate_with_provider(&cmd_refs, &axes)
            .expect("consolidate_with_provider must succeed with stub harness");
        assert_eq!(consolidate_receipt.edges_invalidated, 1, "acme must be invalidated");
        assert_eq!(consolidate_receipt.edges_kept, 1, "globex must be kept");

        // The bug under test: a full rebuild truncates + re-materializes every
        // FTS/vec shadow from `canonical_edges` — this MUST NOT re-surface the
        // now-`t_invalid`-excluded acme edge. If the projection scheduler's
        // pending-work probe does not mirror the same filter, this `drain()`
        // would hang forever on a phantom-pending invalidated edge.
        opened.engine.rebuild_projections().expect("rebuild_projections");
        opened
            .engine
            .drain(10_000)
            .expect("post-rebuild drain must not hang on a phantom-pending invalidated edge");

        (acme_cursor, globex_cursor)
    };

    let conn = Connection::open(&path).unwrap();

    assert_eq!(
        edge_fts_count(&conn, "acme"),
        0,
        "rebuild must NOT re-surface the t_invalid-excluded edge in search_index_edges"
    );
    assert_eq!(
        edge_fts_count(&conn, "globex"),
        1,
        "rebuild must keep the still-valid edge in search_index_edges (guard against a vacuous test)"
    );

    assert_eq!(
        vector_row_count(&conn, acme_cursor),
        0,
        "rebuild must NOT re-materialize the invalidated edge's vector row"
    );
    assert_eq!(
        vector_row_count(&conn, globex_cursor),
        1,
        "rebuild must keep the still-valid edge's vector row (guard against a vacuous test)"
    );
}

/// 0.8.20 Slice 15d fix-2 [P2] — a full `rebuild_projections()` re-derives the
/// row-owned ATTRIBUTE store from the SAME active-and-non-superseded row set the
/// backfill uses (`state = 'active' AND superseded_at IS NULL`). Before this fix
/// the projector replay iterated EVERY canonical row (including a `pending` node
/// and a superseded prior version), so an operator rebuild re-surfaced attribute
/// values the write path / lifecycle transitions had correctly withheld or
/// purged — a truncate-then-repopulate round trip that broke the invariant.
/// Asserts on the RAW EAV table after rebuild; includes an active + a live-latest
/// value so the test is non-vacuous.
#[test]
fn slice15d_fix2_rebuild_projects_only_active_non_superseded_attributes() {
    fn eav_values(conn: &Connection, attr: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(
                "SELECT attr_value FROM canonical_attributes \
                 WHERE attr_name = ?1 ORDER BY attr_value",
            )
            .unwrap();
        stmt.query_map([attr], |r| r.get::<_, String>(0)).unwrap().map(|r| r.unwrap()).collect()
    }
    fn node(logical_id: &str, state: fathomdb_engine::InitialState, status: &str) -> PreparedWrite {
        PreparedWrite::Node {
            kind: "doc".to_string(),
            body: format!(r#"{{"status":"{status}"}}"#),
            source_id: fathomdb_engine::SourceId::new("test:fixture").expect("source id"),
            logical_id: Some(logical_id.to_string()),
            state,
            reason: None,
            valid_from: None,
            valid_until: None,
        }
    }

    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "rebuild_attr_lifecycle");
    let opened = Engine::open(path.clone()).expect("open");
    let engine = &opened.engine;

    let mut roles = std::collections::BTreeSet::new();
    roles.insert(fathomdb_engine::ProjectionRole::Filterable);
    roles.insert(fathomdb_engine::ProjectionRole::Searchable);
    engine
        .configure_projections(
            &[fathomdb_engine::ProjectionSpec {
                name: "status".to_string(),
                roles,
                fts: Some(fathomdb_engine::ProjectionFts { tokenizer: None }),
                vector: None,
            }],
            &[],
        )
        .expect("configure");

    // An active node, a pending node, and a node whose first version is superseded
    // by a rewrite (same logical_id).
    engine.write(&[node("A", fathomdb_engine::InitialState::Active, "active-val")]).unwrap();
    engine.write(&[node("P", fathomdb_engine::InitialState::Pending, "pending-val")]).unwrap();
    engine.write(&[node("S", fathomdb_engine::InitialState::Active, "old-val")]).unwrap();
    engine.write(&[node("S", fathomdb_engine::InitialState::Active, "new-val")]).unwrap();
    engine.drain(10_000).unwrap();

    // Full rebuild: truncate-all then projector replay.
    engine.rebuild_projections().expect("rebuild_projections");
    engine.drain(10_000).unwrap();
    opened.engine.close().unwrap();

    let conn = Connection::open(&path).unwrap();
    // EXACTLY the active + non-superseded values survive the rebuild.
    assert_eq!(
        eav_values(&conn, "status"),
        vec!["active-val".to_string(), "new-val".to_string()],
        "rebuild must re-derive ONLY active + non-superseded attributes"
    );
    // The pending and superseded values must NOT be re-surfaced.
    let pending_present: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canonical_attributes WHERE attr_value = 'pending-val'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pending_present, 0, "a pending node's attribute must not be re-surfaced by rebuild");
    let superseded_present: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM canonical_attributes WHERE attr_value = 'old-val'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        superseded_present, 0,
        "a superseded version's attribute must not be re-surfaced by rebuild"
    );
    // Property-FTS shadow mirrors the same set.
    let fts_pending: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM property_search_index WHERE property_search_index MATCH 'pending'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        fts_pending, 0,
        "pending value must not survive in property_search_index after rebuild"
    );
}
