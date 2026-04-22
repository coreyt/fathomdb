//! Pack E: write-path enqueue of incremental vector projection work.
//!
//! When a chunk is inserted under a vector-enabled kind AND an active
//! embedding profile exists, the canonical writer must enqueue a
//! `vector_projection_work` row with `priority = 1000` (incremental) in
//! the same transaction as the canonical commit.
#![cfg(feature = "sqlite-vec")]
#![allow(clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use fathomdb_engine::{
    BatchEmbedder, ChunkInsert, ChunkPolicy, EmbedderError, EngineRuntime, NodeInsert, NodeRetire,
    ProvenanceMode, QueryEmbedderIdentity, TelemetryLevel, VectorSource, WriteRequest,
};

// ── Embedder ────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct FakeEmbedder {
    dimension: usize,
}

impl FakeEmbedder {
    fn new() -> Self {
        Self { dimension: 4 }
    }
}

impl BatchEmbedder for FakeEmbedder {
    fn batch_embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        Ok(texts
            .iter()
            .map(|t| {
                let mut v = vec![0.0_f32; self.dimension];
                #[allow(clippy::cast_precision_loss)]
                {
                    v[0] = t.len() as f32;
                }
                v
            })
            .collect())
    }

    fn identity(&self) -> QueryEmbedderIdentity {
        QueryEmbedderIdentity {
            model_identity: "test/model".to_owned(),
            model_version: "v1".to_owned(),
            dimension: self.dimension,
            normalization_policy: "l2".to_owned(),
        }
    }

    fn max_tokens(&self) -> usize {
        512
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn open_engine(dir: &tempfile::TempDir) -> EngineRuntime {
    EngineRuntime::open(
        dir.path().join("test.db"),
        ProvenanceMode::Warn,
        None,
        2,
        TelemetryLevel::Counters,
        None,
    )
    .expect("open engine")
}

fn empty_write(label: &str) -> WriteRequest {
    WriteRequest {
        label: label.to_owned(),
        nodes: vec![],
        node_retires: vec![],
        edges: vec![],
        edge_retires: vec![],
        chunks: vec![],
        runs: vec![],
        steps: vec![],
        actions: vec![],
        optional_backfills: vec![],
        vec_inserts: vec![],
        operational_writes: vec![],
    }
}

fn write_node_and_chunk(
    engine: &EngineRuntime,
    kind: &str,
    logical_id: &str,
    chunk_id: &str,
    text: &str,
) {
    let mut req = empty_write(&format!("seed-{logical_id}"));
    req.nodes.push(NodeInsert {
        row_id: format!("row-{logical_id}"),
        logical_id: logical_id.to_owned(),
        kind: kind.to_owned(),
        properties: r#"{}"#.to_owned(),
        source_ref: Some("test".to_owned()),
        upsert: false,
        chunk_policy: ChunkPolicy::Preserve,
        content_ref: None,
    });
    req.chunks.push(ChunkInsert {
        id: chunk_id.to_owned(),
        node_logical_id: logical_id.to_owned(),
        text_content: text.to_owned(),
        byte_start: None,
        byte_end: None,
        content_hash: None,
    });
    engine.writer().submit(req).expect("write node+chunk");
}

fn seed_active_profile(db_path: &std::path::Path, dimensions: i64) -> i64 {
    let conn = rusqlite::Connection::open(db_path).expect("open raw");
    conn.execute(
        "INSERT INTO vector_embedding_profiles \
         (profile_name, model_identity, model_version, dimensions, normalization_policy, \
          max_tokens, active, activated_at, created_at) \
         VALUES ('test-profile', 'test/model', 'v1', ?1, 'l2', 512, 1, unixepoch(), unixepoch())",
        rusqlite::params![dimensions],
    )
    .expect("seed profile");
    conn.last_insert_rowid()
}

fn seed_vec_index_schema_direct(db_path: &std::path::Path, kind: &str) {
    let conn = rusqlite::Connection::open(db_path).expect("open raw");
    conn.execute(
        "INSERT INTO vector_index_schemas \
         (kind, enabled, source_mode, source_config_json, state, created_at, updated_at) \
         VALUES (?1, 1, 'chunks', NULL, 'fresh', unixepoch(), unixepoch())",
        rusqlite::params![kind],
    )
    .expect("seed vec_index_schemas");
}

fn pending_work_for(db_path: &std::path::Path, kind: &str) -> Vec<(String, i64, i64)> {
    let conn = rusqlite::Connection::open(db_path).expect("reopen");
    let mut stmt = conn
        .prepare(
            "SELECT chunk_id, priority, embedding_profile_id \
             FROM vector_projection_work \
             WHERE kind = ?1 AND state = 'pending' \
             ORDER BY chunk_id",
        )
        .expect("prepare");
    stmt.query_map(rusqlite::params![kind], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
    })
    .expect("query")
    .collect::<Result<Vec<_>, _>>()
    .expect("collect")
}

fn count_chunks(db_path: &std::path::Path, node_logical_id: &str) -> i64 {
    let conn = rusqlite::Connection::open(db_path).expect("reopen");
    conn.query_row(
        "SELECT count(*) FROM chunks WHERE node_logical_id = ?1",
        rusqlite::params![node_logical_id],
        |r| r.get(0),
    )
    .expect("count chunks")
}

fn vec_row_count(db_path: &std::path::Path, kind: &str) -> i64 {
    let conn = rusqlite::Connection::open(db_path).expect("reopen");
    let table = fathomdb_schema::vec_kind_table_name(kind);
    conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .expect("query vec count")
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn test_insert_chunk_on_vector_enabled_kind_enqueues_priority_1000() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let db_path = dir.path().join("test.db");

    let profile_id = seed_active_profile(&db_path, 4);

    // configure_vec_kind with no pre-existing chunks → no backfill rows.
    let svc = engine.admin().service();
    svc.configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure");
    assert_eq!(
        pending_work_for(&db_path, "KnowledgeItem").len(),
        0,
        "precondition: no work rows before write"
    );

    // Now write one chunk for that kind — writer must enqueue priority=1000.
    write_node_and_chunk(&engine, "KnowledgeItem", "k:1", "chunk-k-1", "hello");

    let rows = pending_work_for(&db_path, "KnowledgeItem");
    assert_eq!(
        rows.len(),
        1,
        "expected exactly one pending work row: {rows:?}"
    );
    let (chunk_id, priority, pid) = &rows[0];
    assert_eq!(chunk_id, "chunk-k-1");
    assert_eq!(*priority, 1000, "incremental priority must be 1000");
    assert_eq!(
        *pid, profile_id,
        "embedding_profile_id must be the active profile"
    );
}

#[test]
fn test_insert_chunk_on_non_enabled_kind_enqueues_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let db_path = dir.path().join("test.db");

    let _profile_id = seed_active_profile(&db_path, 4);
    // Do NOT call configure_vec_kind for the kind we'll write.

    write_node_and_chunk(&engine, "RandomKind", "r:1", "chunk-r-1", "hello");

    assert_eq!(
        pending_work_for(&db_path, "RandomKind").len(),
        0,
        "no work should be enqueued for non-enabled kinds"
    );
    // Canonical write must still commit.
    assert_eq!(count_chunks(&db_path, "r:1"), 1);
}

#[test]
fn test_insert_chunk_without_active_embedding_profile_enqueues_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let db_path = dir.path().join("test.db");

    // Seed vector_index_schemas directly — no embedding profile.
    seed_vec_index_schema_direct(&db_path, "KnowledgeItem");

    write_node_and_chunk(&engine, "KnowledgeItem", "k:1", "chunk-k-1", "hello");

    assert_eq!(
        pending_work_for(&db_path, "KnowledgeItem").len(),
        0,
        "no work without an active profile"
    );
    // Canonical write must still commit.
    assert_eq!(count_chunks(&db_path, "k:1"), 1);
}

#[test]
fn test_canonical_write_commits_when_embedder_absent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let db_path = dir.path().join("test.db");

    let _profile_id = seed_active_profile(&db_path, 4);
    let svc = engine.admin().service();
    svc.configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure");

    // No embedder is given at engine open; writer should still commit the
    // canonical row AND enqueue work.
    write_node_and_chunk(&engine, "KnowledgeItem", "k:1", "chunk-k-1", "hello");

    assert_eq!(
        count_chunks(&db_path, "k:1"),
        1,
        "canonical chunk must exist"
    );
    assert_eq!(
        pending_work_for(&db_path, "KnowledgeItem").len(),
        1,
        "work row must be enqueued even without a live embedder"
    );
}

#[test]
fn test_retire_chunk_removes_pending_work() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let db_path = dir.path().join("test.db");

    let _profile_id = seed_active_profile(&db_path, 4);
    let svc = engine.admin().service();
    svc.configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure");

    write_node_and_chunk(&engine, "KnowledgeItem", "k:1", "chunk-k-1", "hello");
    assert_eq!(pending_work_for(&db_path, "KnowledgeItem").len(), 1);

    // Retire the node — the corresponding pending work row must be cleared.
    let mut req = empty_write("retire");
    req.node_retires.push(NodeRetire {
        logical_id: "k:1".to_owned(),
        source_ref: Some("test".to_owned()),
    });
    engine.writer().submit(req).expect("retire");

    assert_eq!(
        pending_work_for(&db_path, "KnowledgeItem").len(),
        0,
        "pending work for retired chunks must be removed"
    );
}

#[test]
fn test_duplicate_enqueue_dedups() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let db_path = dir.path().join("test.db");

    let _profile_id = seed_active_profile(&db_path, 4);
    let svc = engine.admin().service();
    svc.configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure");

    write_node_and_chunk(&engine, "KnowledgeItem", "k:1", "chunk-k-1", "hello v1");

    // Upsert the same node/chunk (ChunkPolicy::Replace) — same chunk id
    // is re-inserted. Dedup should keep a single pending row.
    let mut req = empty_write("update");
    req.nodes.push(NodeInsert {
        row_id: "row-k-1-v2".to_owned(),
        logical_id: "k:1".to_owned(),
        kind: "KnowledgeItem".to_owned(),
        properties: r#"{}"#.to_owned(),
        source_ref: Some("test".to_owned()),
        upsert: true,
        chunk_policy: ChunkPolicy::Replace,
        content_ref: None,
    });
    req.chunks.push(ChunkInsert {
        id: "chunk-k-1".to_owned(),
        node_logical_id: "k:1".to_owned(),
        text_content: "hello v2".to_owned(),
        byte_start: None,
        byte_end: None,
        content_hash: None,
    });
    engine.writer().submit(req).expect("update");

    assert_eq!(
        pending_work_for(&db_path, "KnowledgeItem").len(),
        1,
        "duplicate enqueue must dedup to a single pending row"
    );
}

#[test]
fn test_drain_processes_incremental_from_write_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let db_path = dir.path().join("test.db");

    let _profile_id = seed_active_profile(&db_path, 4);
    let svc = engine.admin().service();
    svc.configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure");

    write_node_and_chunk(&engine, "KnowledgeItem", "k:1", "chunk-k-1", "hello");

    let embedder = Arc::new(FakeEmbedder::new());
    let report = svc
        .drain_vector_projection(embedder.as_ref(), Duration::from_secs(5))
        .expect("drain");
    assert!(
        report.incremental_processed >= 1,
        "expected at least one incremental processed, got {report:?}"
    );

    assert_eq!(vec_row_count(&db_path, "KnowledgeItem"), 1);
    assert_eq!(pending_work_for(&db_path, "KnowledgeItem").len(), 0);
}
