//! Pack D: async durable vector projection worker.
#![cfg(feature = "sqlite-vec")]
#![allow(clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use fathomdb_engine::{
    BatchEmbedder, ChunkInsert, ChunkPolicy, EmbedderError, EngineRuntime, NodeInsert,
    ProvenanceMode, QueryEmbedderIdentity, TelemetryLevel, VectorSource, WriteRequest,
};

// ── Embedders ────────────────────────────────────────────────────────────────

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
                v[0] = t.len() as f32;
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

#[derive(Debug)]
struct FailingEmbedder;

impl BatchEmbedder for FailingEmbedder {
    fn batch_embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        Err(EmbedderError::Unavailable("test: no embedder".to_owned()))
    }

    fn identity(&self) -> QueryEmbedderIdentity {
        QueryEmbedderIdentity {
            model_identity: "test/model".to_owned(),
            model_version: "v1".to_owned(),
            dimension: 4,
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

fn make_write_request(
    label: &str,
    nodes: Vec<NodeInsert>,
    chunks: Vec<ChunkInsert>,
) -> WriteRequest {
    WriteRequest {
        label: label.to_owned(),
        nodes,
        node_retires: vec![],
        edges: vec![],
        edge_retires: vec![],
        chunks,
        runs: vec![],
        steps: vec![],
        actions: vec![],
        optional_backfills: vec![],
        vec_inserts: vec![],
        operational_writes: vec![],
    }
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

fn seed_kind_with_chunks(engine: &EngineRuntime, kind: &str, count: u32) {
    for i in 0..count {
        let logical_id = format!("{kind}:{i}");
        let chunk_id = format!("chunk-{kind}-{i}");
        engine
            .writer()
            .submit(make_write_request(
                &format!("seed-{kind}-{i}"),
                vec![NodeInsert {
                    row_id: format!("row-{kind}-{i}"),
                    logical_id: logical_id.clone(),
                    kind: kind.to_owned(),
                    properties: format!(r#"{{"index":{i}}}"#),
                    source_ref: Some("test".to_owned()),
                    upsert: false,
                    chunk_policy: ChunkPolicy::Preserve,
                    content_ref: None,
                }],
                vec![ChunkInsert {
                    id: chunk_id,
                    node_logical_id: logical_id,
                    text_content: format!("chunk body {i}"),
                    byte_start: None,
                    byte_end: None,
                    content_hash: None,
                }],
            ))
            .expect("write node+chunk");
    }
}

fn vec_row_count(db_path: &std::path::Path, kind: &str) -> i64 {
    let conn = rusqlite::Connection::open(db_path).expect("reopen");
    let table = fathomdb_schema::vec_kind_table_name(kind);
    conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .expect("query vec count")
}

fn pending_work_count(db_path: &std::path::Path, kind: &str) -> i64 {
    let conn = rusqlite::Connection::open(db_path).expect("reopen");
    conn.query_row(
        "SELECT count(*) FROM vector_projection_work WHERE kind = ?1 AND state = 'pending'",
        rusqlite::params![kind],
        |r| r.get(0),
    )
    .expect("query work count")
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn test_drain_backfill_produces_vec_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let db_path = dir.path().join("test.db");

    let _profile_id = seed_active_profile(&db_path, 4);
    seed_kind_with_chunks(&engine, "KnowledgeItem", 5);

    let svc = engine.admin().service();
    svc.configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure");

    let embedder = Arc::new(FakeEmbedder::new());
    let report = svc
        .drain_vector_projection(embedder.as_ref(), Duration::from_secs(5))
        .expect("drain");

    assert!(
        report.backfill_processed >= 5,
        "expected >=5 backfill processed, got {report:?}"
    );
    assert_eq!(vec_row_count(&db_path, "KnowledgeItem"), 5);
    assert_eq!(pending_work_count(&db_path, "KnowledgeItem"), 0);
}

#[test]
fn test_incremental_priority_beats_backfill() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let db_path = dir.path().join("test.db");

    let profile_id = seed_active_profile(&db_path, 4);
    seed_kind_with_chunks(&engine, "KnowledgeItem", 80);

    let svc = engine.admin().service();
    svc.configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure");

    // Seed one additional incremental row at priority=1000 on an arbitrary chunk.
    let raw = rusqlite::Connection::open(&db_path).expect("reopen raw");
    let chosen_chunk: String = raw
        .query_row(
            "SELECT chunk_id FROM vector_projection_work WHERE kind = 'KnowledgeItem' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("pick a chunk");
    // Bump this one to priority 1000 (incremental).
    raw.execute(
        "UPDATE vector_projection_work SET priority = 1000 WHERE chunk_id = ?1 AND embedding_profile_id = ?2",
        rusqlite::params![chosen_chunk, profile_id],
    )
    .expect("bump priority");
    drop(raw);

    // Single tick should process incremental first.
    let embedder = Arc::new(FakeEmbedder::new());
    let report = svc
        .drain_vector_projection_single_tick(embedder.as_ref())
        .expect("drain tick");

    assert!(
        report.incremental_processed >= 1,
        "expected >=1 incremental, got {report:?}"
    );
    // In a single tick with INCREMENTAL_BATCH=64, all 80 backfill rows
    // shouldn't drain completely — but at minimum the incremental row must
    // have been covered.
    let chunk_after = rusqlite::Connection::open(&db_path).expect("reopen");
    let table = fathomdb_schema::vec_kind_table_name("KnowledgeItem");
    let has_incremental: i64 = chunk_after
        .query_row(
            &format!("SELECT count(*) FROM {table} WHERE chunk_id = ?1"),
            rusqlite::params![chosen_chunk],
            |r| r.get(0),
        )
        .expect("query vec for incremental chunk");
    assert_eq!(
        has_incremental, 1,
        "incremental chunk must have a vec row after one tick"
    );
}

#[test]
fn test_stale_canonical_hash_discards_embedding() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let db_path = dir.path().join("test.db");

    let profile_id = seed_active_profile(&db_path, 4);
    seed_kind_with_chunks(&engine, "KnowledgeItem", 1);

    let svc = engine.admin().service();
    svc.configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure");

    // Corrupt the canonical_hash stored in the single work row.
    let raw = rusqlite::Connection::open(&db_path).expect("reopen raw");
    raw.execute(
        "UPDATE vector_projection_work SET canonical_hash = 'deadbeef' \
         WHERE kind = 'KnowledgeItem' AND embedding_profile_id = ?1",
        rusqlite::params![profile_id],
    )
    .expect("corrupt hash");
    drop(raw);

    let embedder = Arc::new(FakeEmbedder::new());
    let _ = svc
        .drain_vector_projection(embedder.as_ref(), Duration::from_secs(5))
        .expect("drain");

    // The row must now be discarded, and no vec row written.
    let raw = rusqlite::Connection::open(&db_path).expect("reopen raw");
    let (discarded, total_work): (i64, i64) = raw
        .query_row(
            "SELECT \
               SUM(CASE WHEN state = 'discarded' THEN 1 ELSE 0 END), \
               count(*) \
             FROM vector_projection_work WHERE kind = 'KnowledgeItem'",
            [],
            |r| Ok((r.get::<_, Option<i64>>(0)?.unwrap_or(0), r.get(1)?)),
        )
        .expect("count states");
    assert_eq!(total_work, 1);
    assert_eq!(discarded, 1, "the stale-hash row must be discarded");
    assert_eq!(vec_row_count(&db_path, "KnowledgeItem"), 0);
}

#[test]
fn test_embedder_unavailable_keeps_rows_pending() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let db_path = dir.path().join("test.db");

    let _profile_id = seed_active_profile(&db_path, 4);
    seed_kind_with_chunks(&engine, "KnowledgeItem", 3);

    let svc = engine.admin().service();
    svc.configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure");

    let embedder = Arc::new(FailingEmbedder);
    let report = svc
        .drain_vector_projection_single_tick(embedder.as_ref())
        .expect("drain tick");
    assert!(
        report.embedder_unavailable_ticks >= 1,
        "expected embedder unavailable tick, got {report:?}"
    );

    let raw = rusqlite::Connection::open(&db_path).expect("reopen raw");
    let (pending, inflight, attempt_sum, has_err): (i64, i64, i64, i64) = raw
        .query_row(
            "SELECT \
               SUM(CASE WHEN state = 'pending' THEN 1 ELSE 0 END), \
               SUM(CASE WHEN state = 'inflight' THEN 1 ELSE 0 END), \
               COALESCE(SUM(attempt_count), 0), \
               SUM(CASE WHEN last_error IS NOT NULL THEN 1 ELSE 0 END) \
             FROM vector_projection_work WHERE kind = 'KnowledgeItem'",
            [],
            |r| {
                Ok((
                    r.get::<_, Option<i64>>(0)?.unwrap_or(0),
                    r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                ))
            },
        )
        .expect("count states");

    assert_eq!(pending, 3, "rows must be reverted to pending");
    assert_eq!(inflight, 0, "no rows should be left inflight");
    assert!(
        attempt_sum >= 3,
        "attempt_count should have been incremented: {attempt_sum}"
    );
    assert!(has_err >= 3, "last_error should be set: {has_err}");
    assert_eq!(vec_row_count(&db_path, "KnowledgeItem"), 0);
}

#[test]
fn test_drop_order_no_panic() {
    let dir = tempfile::tempdir().expect("tempdir");
    {
        let engine = open_engine(&dir);
        let db_path = dir.path().join("test.db");
        let _ = seed_active_profile(&db_path, 4);
        seed_kind_with_chunks(&engine, "KnowledgeItem", 2);
        let svc = engine.admin().service();
        svc.configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
            .expect("configure");
        // Leave the work rows pending and drop the engine.
    }
    // If we reach here, drop succeeded without panic or hang.
}
