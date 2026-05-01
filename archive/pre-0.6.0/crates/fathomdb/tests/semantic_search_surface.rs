#![cfg(feature = "sqlite-vec")]
#![allow(
    clippy::expect_used,
    clippy::missing_panics_doc,
    clippy::panic,
    clippy::doc_markdown
)]

//! Pack F1 tests for the Rust-only `semantic_search` and
//! `raw_vector_search` builder entry points.
//!
//! `semantic_search(text, limit)` takes a natural-language query, embeds it
//! via the db-wide active profile embedder, and runs KNN against
//! `vec_<kind>`. `raw_vector_search(vec, limit)` skips the embedder and
//! runs KNN against `vec_<kind>` directly with a caller-supplied vector.
//!
//! Error semantics (design doc Query API):
//!   - Hard error: `NoEmbeddingConfigured`, `KindNotVectorIndexed`, `DimensionMismatch`.
//!   - Degrade to empty + `was_degraded=true`: stale schema, embedder unavailable.

use std::sync::Arc;
use std::time::Duration;

use fathomdb::{
    BatchEmbedder, ChunkInsert, ChunkPolicy, EmbedderChoice, EmbedderError, Engine, EngineError,
    EngineOptions, NodeInsert, QueryEmbedder, QueryEmbedderIdentity, WriteRequest,
};
use tempfile::TempDir;

const DIM: usize = 4;
const KIND: &str = "KnowledgeItem";

// ── Deterministic test embedder ───────────────────────────────────────────
//
// Returns a fixed vector for a given text so that write-time (batch) and
// read-time (query) embedding produce the SAME vector, guaranteeing the
// memex tripwire test can find its seeded chunk.
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
        // Same vector for every text that contains "Acme" (case-insensitive),
        // distinct vector otherwise. Enough to make "Acme Corp" at write time
        // and "Acme" at read time collide.
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
        Err(EmbedderError::Unavailable(
            "test: forced unavailable".to_owned(),
        ))
    }
    fn identity(&self) -> QueryEmbedderIdentity {
        self.identity.clone()
    }
    fn max_tokens(&self) -> usize {
        512
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

struct Harness {
    _dir: TempDir,
    engine: Engine,
    embedder: Arc<DeterministicEmbedder>,
}

fn open_with_embedder(embedder: Arc<dyn QueryEmbedder>) -> (TempDir, Engine) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let mut opts = EngineOptions::new(&db_path);
    opts.vector_dimension = Some(DIM);
    opts.embedder = EmbedderChoice::InProcess(embedder);
    let engine = Engine::open(opts).expect("engine opens");
    (dir, engine)
}

fn open_plain() -> Harness {
    let embedder = Arc::new(DeterministicEmbedder::new());
    let (dir, engine) = open_with_embedder(embedder.clone());
    Harness {
        _dir: dir,
        engine,
        embedder,
    }
}

fn configure_embedding(engine: &Engine, embedder: &dyn QueryEmbedder) {
    engine
        .admin()
        .service()
        .configure_embedding(embedder, true)
        .expect("configure_embedding");
}

fn configure_vec_kind(engine: &Engine, kind: &str) {
    engine
        .admin()
        .service()
        .configure_vec_kind(kind, fathomdb::VectorSource::Chunks)
        .expect("configure_vec_kind");
}

fn drain(engine: &Engine, embedder: &dyn BatchEmbedder) {
    engine
        .admin()
        .service()
        .drain_vector_projection(embedder, Duration::from_secs(5))
        .expect("drain");
}

fn write_node_with_chunk(engine: &Engine, logical_id: &str, kind: &str, text: &str) {
    engine
        .writer()
        .submit(WriteRequest {
            label: "seed".to_owned(),
            nodes: vec![NodeInsert {
                row_id: format!("row-{logical_id}"),
                logical_id: logical_id.to_owned(),
                kind: kind.to_owned(),
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
        })
        .expect("write");
}

// ── Tests ────────────────────────────────────────────────────────────────

#[test]
fn test_semantic_search_no_embedding_configured_errors() {
    let h = open_plain();
    // Note: no configure_embedding called; active profile does not exist.
    let err = h
        .engine
        .query(KIND)
        .semantic_search("anything", 5)
        .execute()
        .expect_err("must hard-error when no active profile");
    match err {
        EngineError::EmbedderNotConfigured => {}
        other => panic!("expected EmbedderNotConfigured, got {other:?}"),
    }
}

#[test]
fn test_semantic_search_kind_not_indexed_errors() {
    let h = open_plain();
    configure_embedding(&h.engine, h.embedder.as_ref());
    // No configure_vec_kind called.
    let err = h
        .engine
        .query(KIND)
        .semantic_search("anything", 5)
        .execute()
        .expect_err("must hard-error when kind not indexed");
    match err {
        EngineError::KindNotVectorIndexed { kind } => assert_eq!(kind, KIND),
        other => panic!("expected KindNotVectorIndexed, got {other:?}"),
    }
}

#[test]
fn test_semantic_search_stale_kind_returns_empty_degraded() {
    let h = open_plain();
    configure_embedding(&h.engine, h.embedder.as_ref());
    configure_vec_kind(&h.engine, KIND);
    // Force the kind's schema state to 'stale' via raw SQL.
    let db_path = h.engine.coordinator().database_path().to_path_buf();
    let conn = rusqlite::Connection::open(&db_path).expect("reopen");
    conn.execute(
        "UPDATE vector_index_schemas SET state = 'stale' WHERE kind = ?1",
        rusqlite::params![KIND],
    )
    .expect("mark stale");
    drop(conn);

    let rows = h
        .engine
        .query(KIND)
        .semantic_search("Acme", 5)
        .execute()
        .expect("stale must not error");
    assert!(rows.hits.is_empty());
    assert!(rows.was_degraded, "stale kind must mark was_degraded=true");
}

#[test]
fn test_semantic_search_embedder_unavailable_returns_empty_degraded() {
    let embedder = Arc::new(UnavailableEmbedder::new());
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let mut opts = EngineOptions::new(&db_path);
    opts.vector_dimension = Some(DIM);
    opts.embedder = EmbedderChoice::InProcess(embedder.clone());
    let engine = Engine::open(opts).expect("engine opens");
    configure_embedding(&engine, embedder.as_ref());
    configure_vec_kind(&engine, KIND);

    let rows = engine
        .query(KIND)
        .semantic_search("Acme", 5)
        .execute()
        .expect("embedder unavailable must not error");
    assert!(rows.hits.is_empty());
    assert!(
        rows.was_degraded,
        "embedder unavailable must mark was_degraded=true"
    );
}

#[test]
fn test_semantic_search_end_to_end_memex_tripwire() {
    let h = open_plain();
    configure_embedding(&h.engine, h.embedder.as_ref());
    configure_vec_kind(&h.engine, KIND);
    // Write a node + chunk with "Acme Corp" as chunk text.
    write_node_with_chunk(&h.engine, "ki-acme", KIND, "Acme Corp");
    // Drain the projection actor to produce the vec row via the SAME embedder
    // that the engine uses for read-time embedding.
    drain(&h.engine, h.embedder.as_ref());

    // Query with "Acme" — the embedder returns the same vector for any text
    // containing "acme", so this must hit.
    let rows = h
        .engine
        .query(KIND)
        .semantic_search("Acme", 5)
        .execute()
        .expect("semantic_search executes");
    assert!(
        !rows.hits.is_empty(),
        "expected >=1 hit for memex tripwire, got {:?}",
        rows.hits
    );
    assert!(!rows.was_degraded, "end-to-end happy path must not degrade");
    let hit = &rows.hits[0];
    assert_eq!(hit.node.logical_id, "ki-acme");
    assert!(
        hit.vector_distance.is_some(),
        "vector hits must carry vector_distance"
    );
}

#[test]
fn test_raw_vector_search_dimension_mismatch() {
    let h = open_plain();
    configure_embedding(&h.engine, h.embedder.as_ref());
    configure_vec_kind(&h.engine, KIND);
    // Profile dim=DIM; supply a vec of DIM+1.
    let err = h
        .engine
        .query(KIND)
        .raw_vector_search(vec![0.1_f32; DIM + 1], 5)
        .execute()
        .expect_err("must hard-error on dimension mismatch");
    match err {
        EngineError::DimensionMismatch { expected, actual } => {
            assert_eq!(expected, DIM);
            assert_eq!(actual, DIM + 1);
        }
        other => panic!("expected DimensionMismatch, got {other:?}"),
    }
}

#[test]
fn test_raw_vector_search_happy_path() {
    let h = open_plain();
    configure_embedding(&h.engine, h.embedder.as_ref());
    configure_vec_kind(&h.engine, KIND);
    write_node_with_chunk(&h.engine, "ki-acme", KIND, "Acme Corp");
    drain(&h.engine, h.embedder.as_ref());

    // The deterministic embedder maps "Acme Corp" → [1.0, 0.0, 0.0, 0.0].
    // Supply that vector directly; we must get the seeded node as a hit.
    let rows = h
        .engine
        .query(KIND)
        .raw_vector_search(vec![1.0_f32, 0.0, 0.0, 0.0], 5)
        .execute()
        .expect("raw_vector_search executes");
    assert!(!rows.hits.is_empty(), "expected >=1 hit");
    assert!(!rows.was_degraded);
    assert_eq!(rows.hits[0].node.logical_id, "ki-acme");
}
