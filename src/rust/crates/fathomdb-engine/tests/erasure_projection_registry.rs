//! 0.8.20 Slice 5a (R-20-E1) — erasure completeness for the ROW-OWNED
//! projection registry.
//!
//! **Test-design contract (design `0.8.20-slice0-erasure-design.md` §3, Rule 1):
//! every erasure/rebuild witness in this file asserts on RAW TABLE CONTENTS.**
//! A test that issues a `search()` is INVALID as an erasure witness: both
//! `search_index_v2` read paths discard candidates lacking a live
//! `canonical_nodes` row (the BM25F path inner-joins for corpus stats; the
//! second path intersects an `active` set), so a search for the excised text
//! passes on the BROKEN code. The leak is data-at-rest and never surfaces in
//! results — so it must be witnessed by reopening the file with plain rusqlite
//! and counting rows / reading bodies.
//!
//! `search_index_v2` is a CONTENT-STORING FTS5 table (no `content=''`,
//! `fathomdb-schema/src/lib.rs` step 17) — it holds the document body verbatim.
//! Before this slice it was written by ONE site (`project_canonical_node_row`)
//! and deleted by ONE site (`purge_inner`), out of five that maintain
//! projections; excise, rebuild and the tokenizer reproject all missed it.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

#[derive(Clone, Debug)]
struct DeterministicEmbedder {
    identity: EmbedderIdentity,
    vector: Vector,
}

impl DeterministicEmbedder {
    fn new(dim: u32) -> Self {
        let mut vector = vec![0.0_f32; dim as usize];
        vector[0] = 1.0;
        Self { identity: EmbedderIdentity::new("registry-test", "rev-a", dim), vector }
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

fn write_node(engine: &Engine, body: &str, source_id: &str) -> u64 {
    engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: body.to_string(),
            source_id: fathomdb_engine::SourceId::new(source_id).expect("test source id"),
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        }])
        .expect("write")
        .cursor
}

fn count(conn: &Connection, sql: &str) -> u64 {
    conn.query_row(sql, [], |row| row.get(0)).expect("count query")
}

/// Every stored body in the content-storing `search_index_v2` FTS5 table.
fn v2_bodies(conn: &Connection) -> Vec<String> {
    let mut stmt =
        conn.prepare("SELECT body FROM search_index_v2 ORDER BY write_cursor").expect("prepare");
    let rows = stmt.query_map([], |row| row.get::<_, String>(0)).expect("query");
    rows.collect::<rusqlite::Result<Vec<_>>>().expect("collect")
}

/// RAW witness (Rule 1). After `excise_source`, the erased document body must be
/// absent from `search_index_v2` — the content-storing FTS5 table that keeps the
/// body verbatim. Asserted by reopening the file and reading the table, NOT by
/// searching (a search passes on the broken code; see the module docs).
#[test]
fn excise_source_clears_search_index_v2_raw() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "excise_v2_raw");
    let opened = Engine::open(&path).expect("open");

    let s1_a = write_node(&opened.engine, "erasable secret alpha", "S1");
    let s1_b = write_node(&opened.engine, "erasable secret beta", "S1");
    let s2_a = write_node(&opened.engine, "retained gamma", "S2");

    // Sanity: the bodies really are in the content-storing FTS5 table.
    {
        let conn = Connection::open(&path).expect("open sqlite");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM search_index_v2"), 3, "seed v2 rows");
        assert!(
            v2_bodies(&conn).iter().any(|b| b.contains("erasable secret")),
            "seed: v2 must store the body verbatim"
        );
    }

    opened.engine.excise_source("S1").expect("excise");
    opened.engine.close().unwrap();

    let conn = Connection::open(&path).expect("open sqlite");
    for cursor in [s1_a, s1_b] {
        let residue: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM search_index_v2 WHERE write_cursor = ?1",
                [cursor],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            residue, 0,
            "search_index_v2 residue for excised cursor {cursor}: the erased body is still on disk"
        );
    }
    let bodies = v2_bodies(&conn);
    assert!(
        !bodies.iter().any(|b| b.contains("erasable secret")),
        "erased body still stored verbatim in search_index_v2: {bodies:?}"
    );
    // Guard against a vacuous pass: the untouched source must survive.
    assert!(
        bodies.iter().any(|b| b.contains("retained gamma")),
        "non-excised source must survive in search_index_v2 (cursor {s2_a})"
    );
}

/// RAW witness. A rebuild reconstructs every row-owned projection from canonical
/// truth. `search_index_v2` was never truncated NOR repopulated by
/// `rebuild_shadow_state`, so injected divergence survives a rebuild forever:
/// an orphan row (no canonical row backs it) is not dropped, and a deleted row
/// is not restored.
#[test]
fn rebuild_repopulates_search_index_v2_raw() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "rebuild_v2_raw");

    let embedder = Arc::new(DeterministicEmbedder::new(8));
    let dropped_cursor = {
        let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
        opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
        let c1 = write_node(&opened.engine, "rebuild body alpha", "S1");
        write_node(&opened.engine, "rebuild body beta", "S1");
        write_node(&opened.engine, "rebuild body gamma", "S1");
        opened.engine.drain(10_000).expect("drain");
        opened.engine.close().unwrap();
        c1
    };

    // Inject divergence directly into the projection shadow: one ORPHAN row
    // (backed by no canonical row) and one MISSING row (canonical row present,
    // projection deleted). A rebuild must converge both back to canonical truth.
    {
        let conn = Connection::open(&path).expect("open sqlite");
        conn.execute(
            "INSERT INTO search_index_v2(kind, body, status, write_cursor)
             VALUES('doc', 'STALEV2ORPHANTOKEN', '', 999999)",
            [],
        )
        .expect("inject orphan");
        conn.execute("DELETE FROM search_index_v2 WHERE write_cursor = ?1", [dropped_cursor])
            .expect("drop one v2 row");
    }

    {
        let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("reopen");
        opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
        opened.engine.rebuild_projections().expect("rebuild_projections");
        opened.engine.drain(10_000).expect("post-rebuild drain");
        opened.engine.close().unwrap();
    }

    let conn = Connection::open(&path).expect("open sqlite");
    let active_nodes = count(&conn, "SELECT COUNT(*) FROM canonical_nodes WHERE state = 'active'");
    let v2_rows = count(&conn, "SELECT COUNT(*) FROM search_index_v2");
    assert_eq!(active_nodes, 3, "fixture: three active canonical nodes");
    assert_eq!(
        v2_rows, active_nodes,
        "post-rebuild search_index_v2 row count must equal the canonical node count"
    );
    let bodies = v2_bodies(&conn);
    assert!(
        !bodies.iter().any(|b| b.contains("STALEV2ORPHANTOKEN")),
        "rebuild must drop the orphan search_index_v2 row: {bodies:?}"
    );
    let restored: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM search_index_v2 WHERE write_cursor = ?1",
            [dropped_cursor],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(restored, 1, "rebuild must restore the deleted v2 row for cursor {dropped_cursor}");
}

/// RAW witness. The open-path tokenizer reproject
/// (`reproject_search_index_after_tokenizer_upgrade`) rewrites `search_index`
/// from canonical truth but never touched `search_index_v2` — which uses the
/// SAME tokenizer, so it is equally affected by a tokenizer-default upgrade.
/// Re-armed by deleting the durable completion marker; the reproject then
/// re-runs on the next open (it is idempotent + crash-retryable by design).
#[test]
fn tokenizer_reproject_covers_v2() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "tokenizer_v2_raw");

    {
        let opened = Engine::open(&path).expect("open");
        write_node(&opened.engine, "tokenizer body alpha", "S1");
        write_node(&opened.engine, "tokenizer body beta", "S1");
        opened.engine.close().unwrap();
    }

    // Simulate the post-tokenizer-upgrade state: the FTS projections are stale
    // (here: emptied + poisoned with an orphan) and the completion marker is
    // absent, so the next open re-runs the reproject.
    {
        let conn = Connection::open(&path).expect("open sqlite");
        conn.execute("DELETE FROM search_index", []).expect("empty search_index");
        conn.execute("DELETE FROM search_index_v2", []).expect("empty search_index_v2");
        conn.execute(
            "INSERT INTO search_index_v2(kind, body, status, write_cursor)
             VALUES('doc', 'STALETOKENIZERTOKEN', '', 999999)",
            [],
        )
        .expect("inject orphan");
        conn.execute(
            "DELETE FROM _fathomdb_open_state
             WHERE key = 'search_index_tokenizer_reproject_complete'",
            [],
        )
        .expect("clear reproject marker");
    }

    {
        let opened = Engine::open(&path).expect("reopen (runs the reproject)");
        opened.engine.close().unwrap();
    }

    let conn = Connection::open(&path).expect("open sqlite");
    let nodes = count(&conn, "SELECT COUNT(*) FROM canonical_nodes");
    assert_eq!(nodes, 2, "fixture: two canonical nodes");
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM search_index"),
        nodes,
        "control: the reproject already covers search_index"
    );
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM search_index_v2"),
        nodes,
        "the tokenizer reproject must also re-tokenize search_index_v2"
    );
    let bodies = v2_bodies(&conn);
    assert!(
        !bodies.iter().any(|b| b.contains("STALETOKENIZERTOKEN")),
        "the tokenizer reproject must clear stale search_index_v2 rows: {bodies:?}"
    );
    assert!(
        bodies.iter().any(|b| b.contains("tokenizer body alpha")),
        "the tokenizer reproject must repopulate v2 from canonical truth: {bodies:?}"
    );
}

/// The write path and the rebuild path must produce IDENTICAL edge projections.
/// Before this slice no edge projector function existed at all — edge projection
/// was inlined in `commit_batch` and the rebuild re-implemented a subset of it,
/// so a projector-replay rebuild silently dropped part of the edge projection
/// (notably the readiness terminal for body-less edges, which the write path
/// records and the rebuild truncated without restoring).
#[test]
fn rebuild_edge_projection_matches_write_path() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "rebuild_edge_parity");

    let embedder = Arc::new(DeterministicEmbedder::new(8));
    let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
    opened.engine.configure_vector_kind_for_test("edge_fact").expect("vector kind");

    let fact_edge = PreparedWrite::Edge {
        kind: "works_for".to_string(),
        from: "bob".to_string(),
        to: "acme".to_string(),
        source_id: fathomdb_engine::SourceId::new("doc-1").expect("test source id"),
        logical_id: Some("edge-fact-1".to_string()),
        body: Some("Bob works for Acme".to_string()),
        t_valid: Some("2020-01-01T00:00:00Z".to_string()),
        t_invalid: None,
        confidence: Some(0.9),
        extractor_model_id: Some("stub-extractor-v1".to_string()),
        temporal_fallback: None,
    };
    // A structural (body-less) edge: NO FTS row, NO vector row, but the write
    // path DOES record a `up_to_date` readiness terminal for its cursor.
    let plain_edge = PreparedWrite::Edge {
        kind: "mentions".to_string(),
        from: "bob".to_string(),
        to: "carol".to_string(),
        source_id: fathomdb_engine::SourceId::new("doc-1").expect("test source id"),
        logical_id: Some("edge-plain-1".to_string()),
        body: None,
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
    };
    let receipt = opened.engine.write(&[fact_edge, plain_edge]).expect("seed edges");
    assert_eq!(receipt.row_cursors.len(), 2);
    let fact_cursor = receipt.row_cursors[0];
    let plain_cursor = receipt.row_cursors[1];
    opened.engine.drain(10_000).expect("drain seed writes");

    let write_path = {
        let conn = Connection::open(&path).expect("open sqlite");
        EdgeProjectionSnapshot::read(&conn)
    };
    // Guard against a vacuous comparison of two empty snapshots.
    assert_eq!(write_path.edge_fts.len(), 1, "write path: fact edge must be FTS-projected");
    assert_eq!(write_path.vector_rows.len(), 1, "write path: fact edge must be vector-projected");
    assert!(
        write_path.terminal.iter().any(|(c, _)| *c == plain_cursor),
        "write path: body-less edge must carry a readiness terminal (cursor {plain_cursor})"
    );

    opened.engine.rebuild_projections().expect("rebuild_projections");
    opened.engine.drain(10_000).expect("post-rebuild drain");
    assert!(
        wait_until(
            || {
                let conn = match Connection::open(&path) {
                    Ok(c) => c,
                    Err(_) => return false,
                };
                conn.query_row(
                    "SELECT COUNT(*) FROM _fathomdb_vector_rows WHERE write_cursor = ?1",
                    [fact_cursor],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap_or(0)
                    == 1
            },
            Duration::from_secs(10),
        ),
        "post-rebuild edge vector projection never settled"
    );
    opened.engine.close().unwrap();

    let rebuilt = {
        let conn = Connection::open(&path).expect("open sqlite");
        EdgeProjectionSnapshot::read(&conn)
    };

    assert_eq!(rebuilt.edge_fts, write_path.edge_fts, "search_index_edges diverged after rebuild");
    assert_eq!(
        rebuilt.vector_rows, write_path.vector_rows,
        "_fathomdb_vector_rows diverged after rebuild"
    );
    assert_eq!(
        rebuilt.vec0_rowids, write_path.vec0_rowids,
        "vector_default diverged after rebuild"
    );
    assert_eq!(
        rebuilt.terminal, write_path.terminal,
        "_fathomdb_projection_terminal diverged after rebuild: the rebuild path does not replay \
         the write path's projector"
    );
}

/// Every row-owned edge projection, read raw and ordered for comparison.
#[derive(Debug, PartialEq, Eq)]
struct EdgeProjectionSnapshot {
    edge_fts: Vec<(u64, String, String)>,
    vector_rows: Vec<(u64, String)>,
    vec0_rowids: Vec<u64>,
    terminal: Vec<(u64, String)>,
}

impl EdgeProjectionSnapshot {
    fn read(conn: &Connection) -> Self {
        let edge_fts = conn
            .prepare(
                "SELECT write_cursor, kind, body FROM search_index_edges ORDER BY write_cursor",
            )
            .expect("prepare")
            .query_map([], |row| {
                Ok((row.get::<_, u64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            })
            .expect("query")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect");
        let vector_rows = conn
            .prepare("SELECT write_cursor, kind FROM _fathomdb_vector_rows ORDER BY write_cursor")
            .expect("prepare")
            .query_map([], |row| Ok((row.get::<_, u64>(0)?, row.get::<_, String>(1)?)))
            .expect("query")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect");
        let vec0_rowids = conn
            .prepare("SELECT rowid FROM vector_default ORDER BY rowid")
            .expect("prepare")
            .query_map([], |row| row.get::<_, u64>(0))
            .expect("query")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect");
        let terminal = conn
            .prepare(
                "SELECT write_cursor, state FROM _fathomdb_projection_terminal ORDER BY write_cursor",
            )
            .expect("prepare")
            .query_map([], |row| Ok((row.get::<_, u64>(0)?, row.get::<_, String>(1)?)))
            .expect("query")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect");
        Self { edge_fts, vector_rows, vec0_rowids, terminal }
    }
}
