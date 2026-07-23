//! 0.8.20 Slice 15c (TC-33) fix-6 — the ENGINE half of the edge-vector cleanup:
//! after the step-23 `canonical_edges` recreate drops the edge sidecar rows, the
//! engine must prune the now-orphaned vec0 `vector_default` rows on open.
//!
//! **The finding (codex §9 P1).** Vector search reads candidates DIRECTLY from
//! `vector_default` (`... embedding_bin MATCH ... ORDER BY distance LIMIT top_k`)
//! and only then hydrates them through `canonical_edges` / `canonical_nodes`. An
//! orphaned edge vector row — whose `canonical_edges` row the step-23 recreate
//! dropped — still occupies a top-K candidate slot and is discarded at hydration,
//! so an upgraded DB silently returns too few / no vector results even though
//! valid node vectors sit just below the cutoff.
//!
//! `vector_default` is a vec0 virtual table created by the ENGINE's dim-aware
//! `ensure_vector_partition` (it does not exist in the migration SQL's context),
//! so the migration cannot delete its rows. The migration deletes the
//! `_fathomdb_vector_rows` SIDECAR (covered by the schema-crate
//! `tc33_fix6_edge_vector_sidecar` test); the engine then prunes any
//! `vector_default` row that has no sidecar entry — exactly the dropped edges —
//! right after `ensure_vector_partition`, one-time and crash-retryable via a
//! durable completion marker.
//!
//! **This test reproduces the POST-step-23 orphan state directly** (delete an
//! edge's `canonical_edges` row + its sidecar row, leaving its vec0 row orphaned;
//! re-arm the prune by clearing its marker) rather than round-tripping a real
//! v22->v23 upgrade, because building a v22 vec0 fixture by hand is brittle. The
//! injection is byte-equivalent to what step 23 leaves behind.
//!
//! Assertions are on RAW `vector_default` / `_fathomdb_vector_rows` contents (the
//! property is at-rest): the orphaned edge vec0 row is gone AND the node vec0 row
//! plus its sidecar survive (proving the prune is scoped to orphans, not node
//! recall).

use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite, SourceId};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

const PRUNE_MARKER_KEY: &str = "tc33_edge_vector_prune_complete";

#[derive(Clone, Debug)]
struct DeterministicEmbedder {
    identity: EmbedderIdentity,
    vector: Vector,
}

impl DeterministicEmbedder {
    fn new(dim: u32) -> Self {
        let mut vector = vec![0.0_f32; dim as usize];
        vector[0] = 1.0;
        Self { identity: EmbedderIdentity::new("fix6-test", "rev-a", dim), vector }
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

fn count(conn: &Connection, sql: &str, cursor: i64) -> u64 {
    conn.query_row(sql, [cursor], |row| row.get(0)).expect("count query")
}

fn write_fact_edge(engine: &Engine, logical: &str, body: &str) -> i64 {
    let edge = PreparedWrite::Edge {
        kind: "works_for".to_string(),
        from: "bob".to_string(),
        to: "acme".to_string(),
        source_id: SourceId::new("doc-1").expect("source id"),
        logical_id: Some(logical.to_string()),
        body: Some(body.to_string()),
        t_valid: Some(1_577_836_800), // 2020-01-01T00:00:00Z
        t_invalid: None,
        confidence: Some(0.9),
        extractor_model_id: Some("stub-extractor-v1".to_string()),
        temporal_fallback: None,
    };
    engine.write(&[edge]).expect("write edge").row_cursors[0] as i64
}

fn write_doc_node(engine: &Engine, body: &str, source: &str) -> i64 {
    engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: body.to_string(),
            source_id: SourceId::new(source).expect("source id"),
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
            valid_from: None,
            valid_until: None,
        }])
        .expect("write node")
        .cursor as i64
}

/// **RED keystone.** An orphaned edge vec0 row (its `canonical_edges` row and
/// sidecar deleted, as step 23 leaves it) must be pruned on the next engine open,
/// while the node's vec0 row + sidecar are untouched. Against pre-fix-6 code the
/// engine never prunes, so the orphan survives reopen and keeps consuming a KNN
/// candidate slot.
#[test]
fn fix6_orphaned_edge_vector_is_pruned_on_open() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "fix6_prune");
    let embedder = Arc::new(DeterministicEmbedder::new(8));

    // Seed a vector-projected node AND a vector-projected fact-edge.
    let (edge_cursor, node_cursor) = {
        let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
        opened.engine.configure_vector_kind_for_test("doc").expect("vector kind doc");
        opened.engine.configure_vector_kind_for_test("edge_fact").expect("vector kind edge_fact");
        let node_cursor = write_doc_node(&opened.engine, "node body survives", "S-node");
        let edge_cursor = write_fact_edge(&opened.engine, "edge-fact-1", "Bob works for Acme");
        opened.engine.drain(10_000).expect("drain");
        opened.engine.close().unwrap();
        (edge_cursor, node_cursor)
    };

    // Sanity: both are vector-projected (vec0 row + sidecar row) before injection.
    {
        let conn = Connection::open(&path).expect("open sqlite");
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM vector_default WHERE rowid = ?1", edge_cursor),
            1,
            "seed: edge must have a vec0 row (cursor {edge_cursor})"
        );
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM vector_default WHERE rowid = ?1", node_cursor),
            1,
            "seed: node must have a vec0 row (cursor {node_cursor})"
        );
    }

    // Inject the POST-step-23 orphan state: the recreate drops the edge's
    // `canonical_edges` row and (fix-6 migration half) its `_fathomdb_vector_rows`
    // sidecar row, leaving its vec0 `vector_default` row orphaned. Re-arm the
    // one-time engine prune by clearing its completion marker.
    {
        let conn = Connection::open(&path).expect("open sqlite");
        conn.execute("DELETE FROM canonical_edges WHERE write_cursor = ?1", [edge_cursor])
            .expect("drop edge canonical row");
        conn.execute("DELETE FROM _fathomdb_vector_rows WHERE write_cursor = ?1", [edge_cursor])
            .expect("drop edge vector sidecar");
        conn.execute("DELETE FROM _fathomdb_open_state WHERE key = ?1", [PRUNE_MARKER_KEY])
            .expect("clear prune marker to re-arm");
        // Confirm the injection really left an orphan for the prune to find.
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM vector_default WHERE rowid = ?1", edge_cursor),
            1,
            "injection: the orphaned edge vec0 row must still be present pre-prune"
        );
    }

    // Reopen — the engine runs the one-time edge-vector prune after
    // ensure_vector_partition.
    {
        let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("reopen");
        opened.engine.close().unwrap();
    }

    let conn = Connection::open(&path).expect("open sqlite");
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM vector_default WHERE rowid = ?1", edge_cursor),
        0,
        "the orphaned edge vec0 row (cursor {edge_cursor}) must be pruned on open — an \
         orphan consumes a KNN candidate slot and is discarded at hydration"
    );
    // Guard against a vacuous "delete everything" prune: the node vector must
    // survive, in BOTH the vec0 table and its sidecar.
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM vector_default WHERE rowid = ?1", node_cursor),
        1,
        "the node's vec0 row (cursor {node_cursor}) must survive — the prune targets \
         only sidecar-less orphans, never node recall"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM _fathomdb_vector_rows WHERE write_cursor = ?1",
            node_cursor
        ),
        1,
        "the node's vector sidecar (cursor {node_cursor}) must survive the prune"
    );
}

/// The prune is a no-op on a healthy corpus (every vec0 row has a sidecar entry):
/// it must not delete a single vector row, so recall / eu7 fidelity is unchanged
/// on any DB that never dropped edges. Re-arms the marker and reopens; the node
/// vec0 row must still be present.
#[test]
fn fix6_prune_is_a_noop_without_orphans() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "fix6_noop");
    let embedder = Arc::new(DeterministicEmbedder::new(8));

    let node_cursor = {
        let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
        opened.engine.configure_vector_kind_for_test("doc").expect("vector kind doc");
        let node_cursor = write_doc_node(&opened.engine, "healthy node body", "S-node");
        opened.engine.drain(10_000).expect("drain");
        opened.engine.close().unwrap();
        node_cursor
    };

    // Re-arm the prune WITHOUT creating any orphan.
    {
        let conn = Connection::open(&path).expect("open sqlite");
        conn.execute("DELETE FROM _fathomdb_open_state WHERE key = ?1", [PRUNE_MARKER_KEY])
            .expect("clear prune marker");
    }

    let before = {
        let conn = Connection::open(&path).expect("open sqlite");
        count(&conn, "SELECT COUNT(*) FROM vector_default WHERE rowid >= ?1", 0)
    };

    {
        let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("reopen");
        opened.engine.close().unwrap();
    }

    let conn = Connection::open(&path).expect("open sqlite");
    let after = count(&conn, "SELECT COUNT(*) FROM vector_default WHERE rowid >= ?1", 0);
    assert_eq!(before, after, "the prune must delete NOTHING when there are no orphans");
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM vector_default WHERE rowid = ?1", node_cursor),
        1,
        "the healthy node vec0 row must survive a re-armed prune (cursor {node_cursor})"
    );
}
