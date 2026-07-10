//! Slice 5 (EXP-S KEYSTONE) — kind-tagged coexisting-index substrate.
//!
//! Covers plan-0.8.14 §2:
//!   * R-SUB-1 — row-kinds (leaf/coverage/graph) coexist in one store; a
//!     fixture writes >=2 distinct row_kinds and a query selects by row_kind.
//!   * R-SUB-2 — incremental multi-index write is deterministic: the same
//!     input produces byte-identical index state across two fresh DBs on the
//!     same CPU, after flushing the async projection to quiescence.
//!
//! ADR authority: `dev/adr/ADR-0.8.14-exp-s-kind-tagged-coexisting-index-substrate.md`
//! D1 (separate row_kind axis), D2 (per-kind index-target dispatch), D3
//! (flush-then-byte-compare determinism check).

use std::path::Path;
use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite, RowKind};
use rusqlite::Connection;
use tempfile::TempDir;

/// Deterministic hash-placement embedder (mirrors the one in
/// `batch_write_per_row_cursor.rs`). Fully deterministic: the same text always
/// yields the same vector, so two runs are byte-comparable.
#[derive(Debug)]
struct DeterministicEmbedder {
    identity: EmbedderIdentity,
    dim: u32,
}

impl DeterministicEmbedder {
    fn new(dim: u32) -> Self {
        Self { identity: EmbedderIdentity::new("deterministic", "exp-s-substrate", dim), dim }
    }
}

impl Embedder for DeterministicEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        let dim = self.dim as usize;
        let mut v = vec![0.0_f32; dim];
        let mut h: u64 = 0xcbf29ce4_84222325;
        for &b in text.as_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        for k in 0..6 {
            let coord = ((h >> (k * 8)) as usize) % dim;
            let sign = if (h >> (k * 8 + 7)) & 1 == 0 { 1.0 } else { -1.0 };
            v[coord] += sign * 0.5_f32;
        }
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
        for x in &mut v {
            *x /= norm;
        }
        Ok(v)
    }
}

fn fresh_engine(name: &str) -> (TempDir, Engine) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join(format!("{name}.sqlite"));
    let opened =
        Engine::open_with_embedder_for_test(&path, Arc::new(DeterministicEmbedder::new(768)))
            .expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("configure vector kind");
    (dir, opened.engine)
}

/// The fixed fixture written into every run: a mix of the three structural
/// row_kinds over a vector-indexed doc-type `kind`. Leaf rows go through the
/// normal `engine.write` path; coverage/graph rows go through the internal
/// row_kind writer.
fn write_fixture(engine: &Engine) {
    // Leaf rows (normal records) — batched normal write path.
    let leaves: Vec<PreparedWrite> = (0..6)
        .map(|i| PreparedWrite::Node {
            kind: "doc".to_string(),
            body: format!("leaf body {i} alpha bravo charlie token-{i}"),
            source_id: Some(format!("leaf-{i}")),
            logical_id: None,
            state: fathomdb_engine::InitialState::Active,
            reason: None,
        })
        .collect();
    engine.write(&leaves).expect("write leaf batch");

    // Coverage rows (searchable + embeddable per D2).
    for i in 0..3 {
        engine
            .write_canonical_row_with_kind_for_test(
                "doc",
                &format!("coverage summary {i} delta echo"),
                RowKind::Coverage,
            )
            .expect("write coverage row");
    }

    // Graph rows (FTS-only per D2 — not embedded).
    for i in 0..2 {
        engine
            .write_canonical_row_with_kind_for_test(
                "doc",
                &format!("graph structural {i} foxtrot golf"),
                RowKind::Graph,
            )
            .expect("write graph row");
    }
}

/// R-SUB-1 — the three row_kinds coexist in one store and the engine can
/// select rows by row_kind.
#[test]
fn r_sub_1_row_kinds_coexist_and_are_queryable_by_row_kind() {
    let (_dir, engine) = fresh_engine("row_kinds");
    write_fixture(&engine);
    engine.drain(15_000).expect("drain");

    let leaf = engine.canonical_rows_with_row_kind_for_test(RowKind::Leaf).expect("leaf rows");
    let coverage =
        engine.canonical_rows_with_row_kind_for_test(RowKind::Coverage).expect("coverage rows");
    let graph = engine.canonical_rows_with_row_kind_for_test(RowKind::Graph).expect("graph rows");

    assert_eq!(leaf.len(), 6, "expected 6 leaf rows, got {leaf:?}");
    assert_eq!(coverage.len(), 3, "expected 3 coverage rows, got {coverage:?}");
    assert_eq!(graph.len(), 2, "expected 2 graph rows, got {graph:?}");

    // The three sets are disjoint (a row has exactly one structural row_kind).
    for c in &coverage {
        assert!(!leaf.contains(c), "coverage cursor {c} leaked into leaf set");
        assert!(!graph.contains(c), "coverage cursor {c} leaked into graph set");
    }

    // >= 2 distinct row_kinds are present (the R-SUB-1 acceptance signal).
    let present = [leaf.is_empty(), coverage.is_empty(), graph.is_empty()]
        .iter()
        .filter(|empty| !**empty)
        .count();
    assert!(present >= 2, "expected >=2 distinct row_kinds populated, got {present}");
}

/// D2 — graph rows are FTS-projected but NOT vector-projected; leaf and
/// coverage rows over a vector-indexed kind ARE vector-projected. Verifies the
/// per-row_kind index-target dispatch actually routes writes to different
/// coexisting indexes.
#[test]
fn d2_index_targets_differ_by_row_kind() {
    let (_dir, engine) = fresh_engine("index_targets");
    write_fixture(&engine);
    engine.drain(15_000).expect("drain");

    let db_path = engine.path().to_path_buf();
    engine.close().expect("close");
    let conn = open_readonly(&db_path);

    // FTS: every row_kind is searchable -> 11 content rows (6 + 3 + 2).
    let fts_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM search_index", [], |r| r.get(0)).expect("fts count");
    assert_eq!(fts_count, 11, "all 11 rows (leaf+coverage+graph) must be FTS-indexed");

    // Vector: leaf(6) + coverage(3) are embedded; graph(2) is not -> 9 vec0 rows.
    let vec_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM vector_default", [], |r| r.get(0)).expect("vec count");
    assert_eq!(vec_count, 9, "only leaf+coverage rows (9) must be vector-indexed, not graph");
}

fn open_readonly(path: &Path) -> Connection {
    Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .expect("open read-only sqlite")
}

/// Serialize every coexisting index into a single byte string, in a fully
/// deterministic order. This is the D3 comparison surface: FTS content rows,
/// vec0 rows (incl. the sign-quantized embedding BLOB), the vector-rows
/// bookkeeping, the row_kind tags, and the projection-terminal readiness
/// cursors.
fn serialize_index_state(path: &Path) -> Vec<u8> {
    let conn = open_readonly(path);
    let mut out: Vec<u8> = Vec::new();

    // 1. FTS content rows (search_index) ordered by write_cursor.
    out.extend_from_slice(b"# search_index\n");
    let mut stmt = conn
        .prepare("SELECT write_cursor, kind, body FROM search_index ORDER BY write_cursor")
        .expect("prep fts");
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })
        .expect("fts rows");
    for row in rows {
        let (wc, kind, body) = row.expect("fts row");
        out.extend_from_slice(format!("{wc}|{kind}|{body}\n").as_bytes());
    }

    // 1b. F5 (Slice 10) — search_index_v2 content rows (kind/body/status) ordered
    // by write_cursor. Added to the write path in the same D2 dispatch seam, so
    // the R-SUB-2 determinism guarantee must extend to it.
    out.extend_from_slice(b"# search_index_v2\n");
    let mut stmt = conn
        .prepare(
            "SELECT write_cursor, kind, body, status FROM search_index_v2 ORDER BY write_cursor",
        )
        .expect("prep fts v2");
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .expect("fts v2 rows");
    for row in rows {
        let (wc, kind, body, status) = row.expect("fts v2 row");
        out.extend_from_slice(format!("{wc}|{kind}|{body}|{status}\n").as_bytes());
    }

    // 2. vec0 rows: rowid, source_type, kind, and the quantized embedding BLOB.
    out.extend_from_slice(b"# vector_default\n");
    let mut stmt = conn
        .prepare(
            "SELECT rowid, source_type, kind, embedding_bin FROM vector_default ORDER BY rowid",
        )
        .expect("prep vec");
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Vec<u8>>(3)?,
            ))
        })
        .expect("vec rows");
    for row in rows {
        let (rowid, st, kind, blob) = row.expect("vec row");
        out.extend_from_slice(format!("{rowid}|{st}|{kind}|").as_bytes());
        out.extend_from_slice(&blob);
        out.push(b'\n');
    }

    // 3. _fathomdb_vector_rows bookkeeping ordered by rowid.
    out.extend_from_slice(b"# _fathomdb_vector_rows\n");
    let mut stmt = conn
        .prepare("SELECT rowid, kind, write_cursor FROM _fathomdb_vector_rows ORDER BY rowid")
        .expect("prep vecrows");
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))
        .expect("vecrows rows");
    for row in rows {
        let (rowid, kind, wc) = row.expect("vecrows row");
        out.extend_from_slice(format!("{rowid}|{kind}|{wc}\n").as_bytes());
    }

    // 4. row_kind tags on canonical_nodes ordered by write_cursor.
    out.extend_from_slice(b"# canonical_nodes.row_kind\n");
    let mut stmt = conn
        .prepare("SELECT write_cursor, kind, row_kind FROM canonical_nodes ORDER BY write_cursor")
        .expect("prep rowkind");
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })
        .expect("rowkind rows");
    for row in rows {
        let (wc, kind, rk) = row.expect("rowkind row");
        out.extend_from_slice(format!("{wc}|{kind}|{rk}\n").as_bytes());
    }

    // 5. projection-terminal readiness cursors ordered by write_cursor.
    out.extend_from_slice(b"# _fathomdb_projection_terminal\n");
    let mut stmt = conn
        .prepare(
            "SELECT write_cursor, state FROM _fathomdb_projection_terminal ORDER BY write_cursor",
        )
        .expect("prep terminal");
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .expect("terminal rows");
    for row in rows {
        let (wc, state) = row.expect("terminal row");
        out.extend_from_slice(format!("{wc}|{state}\n").as_bytes());
    }

    out
}

/// R-SUB-2 (D3) — write the SAME fixed fixture into two fresh DBs, flush the
/// async projection to quiescence in BOTH, then assert the serialized index
/// state is BYTE-IDENTICAL. The flush (`drain`) is mandatory: the FTS-sync /
/// vector-async split would otherwise race the compare.
#[test]
fn r_sub_2_incremental_multi_index_write_is_deterministic() {
    let (dir_a, engine_a) = fresh_engine("determinism_a");
    write_fixture(&engine_a);
    engine_a.drain(30_000).expect("drain A to quiescence");
    let path_a = engine_a.path().to_path_buf();
    engine_a.close().expect("close A");
    let state_a = serialize_index_state(&path_a);

    let (dir_b, engine_b) = fresh_engine("determinism_b");
    write_fixture(&engine_b);
    engine_b.drain(30_000).expect("drain B to quiescence");
    let path_b = engine_b.path().to_path_buf();
    engine_b.close().expect("close B");
    let state_b = serialize_index_state(&path_b);

    assert!(!state_a.is_empty(), "serialized index state must be non-empty");
    assert_eq!(
        state_a.len(),
        state_b.len(),
        "index-state byte length differs across runs ({} vs {})",
        state_a.len(),
        state_b.len()
    );
    assert!(
        state_a == state_b,
        "incremental multi-index write is NOT deterministic: index state differs byte-for-byte \
         across two fresh DBs on the same CPU"
    );

    drop(dir_a);
    drop(dir_b);
}
