//! Pack H: admin introspection APIs + batch `configure_vec`.
//!
//! These tests exercise the new read-side surfaces that let clients
//! detect per-kind vector/FTS configuration drift without fathomdb
//! needing to know about their kind list.
#![allow(clippy::expect_used, clippy::panic)]

use std::time::Duration;

use fathomdb_engine::{
    BatchEmbedder, ChunkInsert, ChunkPolicy, EmbedderError, EngineRuntime, NodeInsert,
    ProvenanceMode, QueryEmbedder, QueryEmbedderIdentity, TelemetryLevel, VectorSource,
    WriteRequest,
};

// ── Fake embedder ───────────────────────────────────────────────────────────

#[derive(Debug)]
struct FakeEmbedder {
    dimension: usize,
}

impl FakeEmbedder {
    fn new() -> Self {
        Self { dimension: 4 }
    }
}

impl QueryEmbedder for FakeEmbedder {
    fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedderError> {
        let mut v = vec![0.0_f32; self.dimension];
        #[allow(clippy::cast_precision_loss)]
        {
            v[0] = text.len() as f32;
        }
        Ok(v)
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

fn seed_kind_with_chunks(engine: &EngineRuntime, kind: &str, count: u32) {
    for i in 0..count {
        let logical_id = format!("{kind}:{i}");
        let chunk_id = format!("chunk-{kind}-{i}");
        let mut req = empty_write(&format!("seed-{kind}-{i}"));
        req.nodes.push(NodeInsert {
            row_id: format!("row-{kind}-{i}"),
            logical_id: logical_id.clone(),
            kind: kind.to_owned(),
            properties: format!(r#"{{"index":{i}}}"#),
            source_ref: Some("test".to_owned()),
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
            content_ref: None,
        });
        req.chunks.push(ChunkInsert {
            id: chunk_id,
            node_logical_id: logical_id,
            text_content: format!("chunk body {i}"),
            byte_start: None,
            byte_end: None,
            content_hash: None,
        });
        engine.writer().submit(req).expect("write node+chunk");
    }
}

// ── capabilities ────────────────────────────────────────────────────────────

#[test]
fn test_capabilities_reports_sqlite_vec_feature() {
    let caps = fathomdb_engine::AdminService::capabilities();
    let expected = cfg!(feature = "sqlite-vec");
    assert_eq!(caps.sqlite_vec, expected);
    assert_eq!(caps.fathomdb_version, env!("CARGO_PKG_VERSION"));
    assert!(caps.schema_version >= 24);
}

#[test]
fn test_capabilities_lists_fts_tokenizer_presets() {
    let caps = fathomdb_engine::AdminService::capabilities();
    // Every preset name from TOKENIZER_PRESETS must appear.
    for (name, _) in fathomdb_engine::TOKENIZER_PRESETS {
        assert!(
            caps.fts_tokenizers.iter().any(|t| t == name),
            "missing preset {name} in {:?}",
            caps.fts_tokenizers
        );
    }
}

#[test]
fn test_capabilities_reports_builtin_embedder_when_feature_enabled() {
    let caps = fathomdb_engine::AdminService::capabilities();
    let builtin = caps
        .embedders
        .get("builtin")
        .expect("builtin entry present");
    let expected_available = cfg!(feature = "default-embedder");
    assert_eq!(builtin.available, expected_available);
    if expected_available {
        assert_eq!(
            builtin.model_identity.as_deref(),
            Some("BAAI/bge-small-en-v1.5")
        );
        assert_eq!(builtin.dimensions, Some(384));
    }
}

// ── current_config ──────────────────────────────────────────────────────────

#[test]
fn test_current_config_empty_on_fresh_engine() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let svc = engine.admin().service();

    let cfg = svc.current_config().expect("current_config");
    assert!(cfg.active_embedding_profile.is_none());
    assert!(cfg.vec_kinds.is_empty());
    // fts_kinds may be empty (no FTS profiles registered yet).
    assert!(cfg.fts_kinds.is_empty());
    assert_eq!(cfg.work_queue.pending_incremental, 0);
    assert_eq!(cfg.work_queue.pending_backfill, 0);
    assert_eq!(cfg.work_queue.inflight, 0);
    assert_eq!(cfg.work_queue.failed, 0);
    assert_eq!(cfg.work_queue.discarded, 0);
}

#[test]
fn test_current_config_reflects_configure_embedding_and_configure_vec_kind() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let svc = engine.admin().service();
    let embedder = FakeEmbedder::new();

    svc.configure_embedding(&embedder, true)
        .expect("configure_embedding");
    seed_kind_with_chunks(&engine, "KnowledgeItem", 2);
    svc.configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure_vec_kind");

    let cfg = svc.current_config().expect("current_config");
    let profile = cfg
        .active_embedding_profile
        .expect("active embedding profile");
    assert_eq!(profile.model_identity, "test/model");
    assert_eq!(profile.dimensions, 4);

    let vec_kind = cfg
        .vec_kinds
        .get("KnowledgeItem")
        .expect("KnowledgeItem vec kind");
    assert!(vec_kind.enabled);
    assert_eq!(vec_kind.kind, "KnowledgeItem");
}

#[test]
fn test_current_config_aggregates_work_queue_counts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let svc = engine.admin().service();
    let embedder = FakeEmbedder::new();

    svc.configure_embedding(&embedder, true)
        .expect("configure_embedding");
    seed_kind_with_chunks(&engine, "KnowledgeItem", 3);
    svc.configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure_vec_kind");

    let cfg = svc.current_config().expect("current_config");
    // Three backfill rows enqueued (priority < 1000).
    assert_eq!(cfg.work_queue.pending_backfill, 3);
    assert_eq!(cfg.work_queue.pending_incremental, 0);
}

// ── describe_kind ───────────────────────────────────────────────────────────

#[test]
fn test_describe_kind_unconfigured() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let svc = engine.admin().service();

    let desc = svc.describe_kind("NotAKind").expect("describe_kind");
    assert_eq!(desc.kind, "NotAKind");
    assert!(desc.vec.is_none());
    assert!(desc.fts.is_none());
    assert_eq!(desc.chunk_count, 0);
    assert!(desc.vec_rows.is_none());
}

#[test]
fn test_describe_kind_configured_with_chunks() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let svc = engine.admin().service();
    let embedder = FakeEmbedder::new();

    svc.configure_embedding(&embedder, true)
        .expect("configure_embedding");
    seed_kind_with_chunks(&engine, "KnowledgeItem", 2);
    svc.configure_vec_kind("KnowledgeItem", VectorSource::Chunks)
        .expect("configure_vec_kind");

    let desc = svc.describe_kind("KnowledgeItem").expect("describe_kind");
    assert_eq!(desc.kind, "KnowledgeItem");
    let vec_cfg = desc.vec.expect("vec config");
    assert!(vec_cfg.enabled);
    assert_eq!(desc.chunk_count, 2);
    assert_eq!(desc.embedding_identity.as_deref(), Some("test/model"));
    // Before drain, vec_rows may be 0; after drain, should be 2.
    svc.drain_vector_projection(&embedder, Duration::from_secs(5))
        .expect("drain");
    let desc2 = svc.describe_kind("KnowledgeItem").expect("describe_kind 2");
    assert_eq!(desc2.vec_rows, Some(2));
}

// ── configure_vec_kinds batch ───────────────────────────────────────────────

#[test]
fn test_configure_vec_kinds_batch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let engine = open_engine(&dir);
    let svc = engine.admin().service();
    let embedder = FakeEmbedder::new();

    svc.configure_embedding(&embedder, true)
        .expect("configure_embedding");
    seed_kind_with_chunks(&engine, "KindA", 2);
    seed_kind_with_chunks(&engine, "KindB", 1);

    let outcomes = svc
        .configure_vec_kinds(&[
            ("KindA".to_owned(), VectorSource::Chunks),
            ("KindB".to_owned(), VectorSource::Chunks),
        ])
        .expect("configure_vec_kinds");
    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes[0].kind, "KindA");
    assert_eq!(outcomes[0].enqueued_backfill_rows, 2);
    assert!(!outcomes[0].was_already_enabled);
    assert_eq!(outcomes[1].kind, "KindB");
    assert_eq!(outcomes[1].enqueued_backfill_rows, 1);
    assert!(!outcomes[1].was_already_enabled);

    // Idempotent re-run: already enabled.
    let outcomes2 = svc
        .configure_vec_kinds(&[
            ("KindA".to_owned(), VectorSource::Chunks),
            ("KindB".to_owned(), VectorSource::Chunks),
        ])
        .expect("configure_vec_kinds idempotent");
    assert!(outcomes2[0].was_already_enabled);
    assert!(outcomes2[1].was_already_enabled);
}
