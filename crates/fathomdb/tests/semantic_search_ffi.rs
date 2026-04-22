#![cfg(feature = "sqlite-vec")]
#![allow(
    clippy::expect_used,
    clippy::missing_panics_doc,
    clippy::panic,
    clippy::doc_markdown
)]

//! Pack F1.5 integration tests for the admin FFI drain binding and the
//! EngineError render paths for the three Pack F1-added variants.
//!
//! `FfiQueryStep::SemanticSearch` / `RawVectorSearch` parse + lowering
//! tests live inline in `ffi_types.rs`'s test module (because the
//! `ffi_types` module is gated on `python` / `node` feature and test
//! binaries linking against napi or pyo3 require the host runtime).

use std::sync::Arc;

use fathomdb::admin_ffi::drain_vector_projection_json;
use fathomdb::{
    EmbedderChoice, EmbedderError, Engine, EngineError, EngineOptions, QueryEmbedder,
    QueryEmbedderIdentity,
};
use tempfile::TempDir;

const DIM: usize = 4;
const KIND: &str = "KnowledgeItem";

#[derive(Debug)]
struct TestEmbedder {
    identity: QueryEmbedderIdentity,
}

impl TestEmbedder {
    fn new() -> Self {
        Self {
            identity: QueryEmbedderIdentity {
                model_identity: "test".to_owned(),
                model_version: "1".to_owned(),
                dimension: DIM,
                normalization_policy: "none".to_owned(),
            },
        }
    }
}

impl QueryEmbedder for TestEmbedder {
    fn embed_query(&self, _text: &str) -> Result<Vec<f32>, EmbedderError> {
        Ok(vec![1.0, 0.0, 0.0, 0.0])
    }
    fn identity(&self) -> QueryEmbedderIdentity {
        self.identity.clone()
    }
    fn max_tokens(&self) -> usize {
        512
    }
}

fn open_engine_with_embedder() -> (TempDir, Engine) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let mut opts = EngineOptions::new(&db_path);
    opts.vector_dimension = Some(DIM);
    opts.embedder = EmbedderChoice::InProcess(Arc::new(TestEmbedder::new()));
    let engine = Engine::open(opts).expect("engine opens");
    (dir, engine)
}

#[test]
fn test_map_engine_error_covers_kind_not_vector_indexed() {
    // Display-render assertion: the matching arms in `node_types.rs` and
    // `python.rs` are validated at compile time (exhaustive match) plus
    // by `cargo build -p fathomdb --features node`, which today fails
    // without the F1.5 additions. This test documents the wire message.
    let err = EngineError::KindNotVectorIndexed {
        kind: "Meeting".to_owned(),
    };
    let rendered = err.to_string();
    assert!(
        rendered.contains("not vector-indexed"),
        "unexpected render: {rendered}"
    );
    assert!(rendered.contains("Meeting"));
}

#[test]
fn test_map_engine_error_covers_dimension_mismatch() {
    let err = EngineError::DimensionMismatch {
        expected: 384,
        actual: 4,
    };
    let rendered = err.to_string();
    assert!(
        rendered.contains("dimension mismatch"),
        "unexpected render: {rendered}"
    );
    assert!(rendered.contains("384"));
    assert!(rendered.contains('4'));
}

#[test]
fn test_admin_drain_vector_projection_json_empty_queue() {
    let (_dir, engine) = open_engine_with_embedder();
    engine
        .admin()
        .service()
        .configure_embedding(&TestEmbedder::new(), true)
        .expect("configure_embedding");
    engine
        .admin()
        .service()
        .configure_vec_kind(KIND, fathomdb::VectorSource::Chunks)
        .expect("configure_vec_kind");

    let request = r#"{"timeout_ms": 500}"#;
    let response =
        drain_vector_projection_json(&engine, request).expect("drain_vector_projection_json");
    let parsed: serde_json::Value =
        serde_json::from_str(&response).expect("parse drain report json");
    assert_eq!(parsed["incremental_processed"].as_u64(), Some(0));
    assert_eq!(parsed["backfill_processed"].as_u64(), Some(0));
    assert_eq!(parsed["failed"].as_u64(), Some(0));
    assert_eq!(parsed["discarded_stale"].as_u64(), Some(0));
}

#[test]
fn test_admin_drain_vector_projection_json_requires_embedder() {
    // Engine opened with EmbedderChoice::None — drain should report
    // EmbedderNotConfigured so Python/TS SDKs never end up dispatching
    // on an identity-less engine.
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let mut opts = EngineOptions::new(&db_path);
    opts.vector_dimension = Some(DIM);
    let engine = Engine::open(opts).expect("engine opens");

    let err = drain_vector_projection_json(&engine, r#"{"timeout_ms": 100}"#)
        .expect_err("expected error when engine has no embedder");
    let rendered = err.to_string();
    assert!(
        rendered.contains("embedder not configured"),
        "unexpected render: {rendered}"
    );
}
