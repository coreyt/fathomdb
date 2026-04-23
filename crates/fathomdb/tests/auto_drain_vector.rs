//! Pack H: `auto_drain_vector` engine-open flag.
//!
//! Test-only: when true, canonical writes that enqueue vector projection
//! work synchronously drain the queue before returning. Production code
//! must NOT set this flag — it defeats the async worker's availability
//! contract — but it makes integration tests significantly simpler
//! (`write → assert semantic_search` with no separate drain step).
#![cfg(feature = "sqlite-vec")]
#![allow(clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use fathomdb::{
    BatchEmbedder, ChunkInsert, ChunkPolicy, EmbedderChoice, EmbedderError, Engine, EngineOptions,
    NodeInsert, QueryEmbedder, QueryEmbedderIdentity, VectorSource, WriteRequest,
};
use tempfile::TempDir;

const DIM: usize = 4;
const KIND: &str = "KnowledgeItem";

// Deterministic embedder: same vector for every text containing "acme".
#[derive(Debug, Clone)]
struct DeterministicEmbedder {
    identity: QueryEmbedderIdentity,
}

impl DeterministicEmbedder {
    fn new() -> Self {
        Self {
            identity: QueryEmbedderIdentity {
                model_identity: "test-deterministic".to_owned(),
                model_version: "1".to_owned(),
                dimension: DIM,
                normalization_policy: "none".to_owned(),
            },
        }
    }
    fn embed(text: &str) -> Vec<f32> {
        if text.to_ascii_lowercase().contains("acme") {
            vec![1.0, 0.0, 0.0, 0.0]
        } else {
            vec![0.0, 1.0, 0.0, 0.0]
        }
    }
}

impl QueryEmbedder for DeterministicEmbedder {
    fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedderError> {
        Ok(Self::embed(text))
    }
    fn identity(&self) -> QueryEmbedderIdentity {
        self.identity.clone()
    }
    fn max_tokens(&self) -> usize {
        512
    }
}

impl BatchEmbedder for DeterministicEmbedder {
    fn batch_embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        Ok(texts.iter().map(|t| Self::embed(t)).collect())
    }
    fn identity(&self) -> QueryEmbedderIdentity {
        self.identity.clone()
    }
    fn max_tokens(&self) -> usize {
        512
    }
}

#[derive(Debug)]
struct UnavailableEmbedder {
    identity: QueryEmbedderIdentity,
}

impl UnavailableEmbedder {
    fn new() -> Self {
        Self {
            identity: QueryEmbedderIdentity {
                model_identity: "test-deterministic".to_owned(),
                model_version: "1".to_owned(),
                dimension: DIM,
                normalization_policy: "none".to_owned(),
            },
        }
    }
}

impl QueryEmbedder for UnavailableEmbedder {
    fn embed_query(&self, _text: &str) -> Result<Vec<f32>, EmbedderError> {
        Err(EmbedderError::Unavailable("forced".to_owned()))
    }
    fn identity(&self) -> QueryEmbedderIdentity {
        self.identity.clone()
    }
    fn max_tokens(&self) -> usize {
        512
    }
}

fn open_with_auto_drain(embedder: Arc<dyn QueryEmbedder>, auto_drain: bool) -> (TempDir, Engine) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let mut opts = EngineOptions::new(&db_path);
    opts.vector_dimension = Some(DIM);
    opts.embedder = EmbedderChoice::InProcess(embedder);
    opts.auto_drain_vector = auto_drain;
    let engine = Engine::open(opts).expect("engine opens");
    (dir, engine)
}

fn configure(engine: &Engine, embedder: &dyn QueryEmbedder) {
    engine
        .admin()
        .service()
        .configure_embedding(embedder, true)
        .expect("configure_embedding");
    engine
        .admin()
        .service()
        .configure_vec_kind(KIND, VectorSource::Chunks)
        .expect("configure_vec_kind");
}

fn make_write(logical_id: &str, text: &str) -> WriteRequest {
    WriteRequest {
        label: "seed".to_owned(),
        nodes: vec![NodeInsert {
            row_id: format!("row-{logical_id}"),
            logical_id: logical_id.to_owned(),
            kind: KIND.to_owned(),
            properties: "{}".to_owned(),
            source_ref: Some("seed".to_owned()),
            upsert: false,
            chunk_policy: ChunkPolicy::Preserve,
            content_ref: None,
        }],
        node_retires: vec![],
        edges: vec![],
        edge_retires: vec![],
        chunks: vec![ChunkInsert {
            id: format!("chunk-{logical_id}"),
            node_logical_id: logical_id.to_owned(),
            text_content: text.to_owned(),
            byte_start: None,
            byte_end: None,
            content_hash: None,
        }],
        runs: vec![],
        steps: vec![],
        actions: vec![],
        optional_backfills: vec![],
        vec_inserts: vec![],
        operational_writes: vec![],
    }
}

#[test]
fn test_auto_drain_false_requires_explicit_drain() {
    let embedder = Arc::new(DeterministicEmbedder::new());
    let (_dir, engine) = open_with_auto_drain(embedder.clone(), false);
    configure(&engine, embedder.as_ref());

    // Use Engine::submit_write so baseline+auto_drain use the same entry point.
    engine
        .submit_write(make_write("ki-acme", "Acme Corp"))
        .expect("write");

    // No drain called. Vector projection work is still pending; vec_<KIND>
    // has no row yet, so semantic_search should return empty.
    let rows = engine
        .query(KIND)
        .semantic_search("Acme", 5)
        .execute()
        .expect("semantic_search executes");
    assert!(
        rows.hits.is_empty(),
        "expected empty hits before drain, got {} hits",
        rows.hits.len()
    );
}

#[test]
fn test_auto_drain_true_write_makes_chunks_searchable_sync() {
    let embedder = Arc::new(DeterministicEmbedder::new());
    let (_dir, engine) = open_with_auto_drain(embedder.clone(), true);
    configure(&engine, embedder.as_ref());

    engine
        .submit_write(make_write("ki-acme", "Acme Corp"))
        .expect("write");

    // No explicit drain call. Auto-drain must have run synchronously.
    let rows = engine
        .query(KIND)
        .semantic_search("Acme", 5)
        .execute()
        .expect("semantic_search executes");
    assert!(
        !rows.hits.is_empty(),
        "auto_drain_vector=true must make chunks searchable without explicit drain"
    );
    assert_eq!(rows.hits[0].node.logical_id, "ki-acme");
}

#[derive(Debug)]
struct FailingEmbedder {
    identity: QueryEmbedderIdentity,
}

impl FailingEmbedder {
    fn new() -> Self {
        Self {
            identity: QueryEmbedderIdentity {
                model_identity: "test-deterministic".to_owned(),
                model_version: "1".to_owned(),
                dimension: DIM,
                normalization_policy: "none".to_owned(),
            },
        }
    }
}

impl QueryEmbedder for FailingEmbedder {
    fn embed_query(&self, _text: &str) -> Result<Vec<f32>, EmbedderError> {
        Err(EmbedderError::Failed("forced failure".to_owned()))
    }
    fn identity(&self) -> QueryEmbedderIdentity {
        self.identity.clone()
    }
    fn max_tokens(&self) -> usize {
        512
    }
}

impl BatchEmbedder for FailingEmbedder {
    fn batch_embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        Err(EmbedderError::Failed("forced failure".to_owned()))
    }
    fn identity(&self) -> QueryEmbedderIdentity {
        self.identity.clone()
    }
    fn max_tokens(&self) -> usize {
        512
    }
}

#[test]
fn test_auto_drain_embedder_error_does_not_fail_write() {
    // Part A of Pack H.1 follow-up: a failing embedder (EmbedderError::Failed)
    // under auto_drain_vector=true must NOT propagate as a write error. The
    // tracing warn! is fired best-effort for debuggability; this test
    // intentionally does not assert on the tracing event (avoids adding a
    // tracing-test dep). Manual repro: RUST_LOG=warn.
    let embedder = Arc::new(FailingEmbedder::new());
    let (_dir, engine) = open_with_auto_drain(embedder.clone(), true);
    engine
        .admin()
        .service()
        .configure_embedding(embedder.as_ref(), true)
        .expect("configure_embedding");
    engine
        .admin()
        .service()
        .configure_vec_kind(KIND, VectorSource::Chunks)
        .expect("configure_vec_kind");

    // Write must succeed even though the embedder errors during the drain —
    // drain failure must never bubble up through `submit_write`.
    let receipt = engine.submit_write(make_write("ki-fail", "Acme Corp"));
    assert!(
        receipt.is_ok(),
        "submit_write must return Ok even when embedder fails during auto-drain: {receipt:?}"
    );
}

#[test]
fn test_auto_drain_true_still_non_blocking_when_embedder_unavailable() {
    let embedder = Arc::new(UnavailableEmbedder::new());
    let (_dir, engine) = open_with_auto_drain(embedder.clone(), true);
    engine
        .admin()
        .service()
        .configure_embedding(embedder.as_ref(), true)
        .expect("configure_embedding");
    engine
        .admin()
        .service()
        .configure_vec_kind(KIND, VectorSource::Chunks)
        .expect("configure_vec_kind");

    // Write must succeed even if embedder is unavailable — auto-drain runs
    // best-effort and does not bubble the embedder-unavailable degradation
    // as a canonical write error.
    engine
        .submit_write(make_write("ki-foo", "Acme Corp"))
        .expect("write must still succeed when embedder unavailable");
}
