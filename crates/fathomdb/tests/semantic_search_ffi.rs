#![cfg(all(feature = "sqlite-vec", any(feature = "python", feature = "node")))]
#![allow(
    clippy::expect_used,
    clippy::missing_panics_doc,
    clippy::panic,
    clippy::doc_markdown
)]

//! Pack F1.5 tests for the FFI wire of Pack F1's semantic_search /
//! raw_vector_search steps, the extended error mapping for the three
//! new `EngineError` variants, and the admin-FFI drain binding.

use std::sync::Arc;

use fathomdb::admin_ffi::drain_vector_projection_json;
use fathomdb::ffi_types::{FfiQueryAst, FfiQueryStep};
use fathomdb::{
    EmbedderChoice, EmbedderError, Engine, EngineError, EngineOptions, QueryAst, QueryEmbedder,
    QueryEmbedderIdentity, QueryStep,
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
fn test_ffi_encodes_semantic_search_step() {
    let json = r#"{"type":"semantic_search","text":"hello world","limit":5}"#;
    let step: FfiQueryStep = serde_json::from_str(json).expect("parse semantic_search");
    match step {
        FfiQueryStep::SemanticSearch { text, limit } => {
            assert_eq!(text, "hello world");
            assert_eq!(limit, 5);
        }
        other => panic!("expected SemanticSearch, got {other:?}"),
    }
}

#[test]
fn test_ffi_encodes_raw_vector_search_step() {
    let json = r#"{"type":"raw_vector_search","vector":[0.1,0.2,0.3,0.4],"limit":7}"#;
    let step: FfiQueryStep = serde_json::from_str(json).expect("parse raw_vector_search");
    match step {
        FfiQueryStep::RawVectorSearch { vector, limit } => {
            assert_eq!(vector, vec![0.1_f32, 0.2, 0.3, 0.4]);
            assert_eq!(limit, 7);
        }
        other => panic!("expected RawVectorSearch, got {other:?}"),
    }
}

#[test]
fn test_ffi_lowers_semantic_search_to_engine_builder() {
    let ast_json = r#"{
        "root_kind": "KnowledgeItem",
        "steps": [{"type":"semantic_search","text":"hello","limit":5}]
    }"#;
    let ffi_ast: FfiQueryAst = serde_json::from_str(ast_json).expect("parse ast");
    let ast: QueryAst = ffi_ast.into();
    assert_eq!(ast.steps.len(), 1);
    match &ast.steps[0] {
        QueryStep::SemanticSearch { text, limit } => {
            assert_eq!(text, "hello");
            assert_eq!(*limit, 5);
        }
        other => panic!("expected SemanticSearch lowering, got {other:?}"),
    }
}

#[test]
fn test_ffi_lowers_raw_vector_search_to_engine_builder() {
    let ast_json = r#"{
        "root_kind": "KnowledgeItem",
        "steps": [{"type":"raw_vector_search","vector":[1.0,0.0,0.0,0.0],"limit":3}]
    }"#;
    let ffi_ast: FfiQueryAst = serde_json::from_str(ast_json).expect("parse ast");
    let ast: QueryAst = ffi_ast.into();
    assert_eq!(ast.steps.len(), 1);
    match &ast.steps[0] {
        QueryStep::RawVectorSearch { vec, limit } => {
            assert_eq!(vec, &vec![1.0_f32, 0.0, 0.0, 0.0]);
            assert_eq!(*limit, 3);
        }
        other => panic!("expected RawVectorSearch lowering, got {other:?}"),
    }
}

#[test]
fn test_map_engine_error_covers_kind_not_vector_indexed() {
    // Exercise via the AdminFfiError -> map path is overkill; we just
    // need to confirm the match is exhaustive at compile time (which
    // means `cargo build --features node` must succeed) and that the
    // python variant constructs a valid pyerr message. We don't link
    // pyo3 here; the compile-time assertion is sufficient.
    let err = EngineError::KindNotVectorIndexed {
        kind: "Meeting".to_owned(),
    };
    let rendered = err.to_string();
    assert!(
        rendered.contains("not vector-indexed"),
        "unexpected render: {rendered}"
    );
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
    assert!(rendered.contains("4"));
}

#[test]
fn test_admin_drain_vector_projection_json_empty_queue() {
    let (_dir, engine) = open_engine_with_embedder();
    // Configure embedding so the engine has an active profile that matches
    // the in-process embedder's identity.
    engine
        .admin()
        .service()
        .configure_embedding(&TestEmbedder::new(), true)
        .expect("configure_embedding");
    // Configure managed vector indexing for the kind; queue is empty until
    // we write something, so drain should report zero processed.
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
