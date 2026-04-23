#![cfg(feature = "sqlite-vec")]
#![allow(
    clippy::expect_used,
    clippy::missing_panics_doc,
    clippy::panic,
    clippy::doc_markdown
)]

//! Pack F1.75 dispatch tests.
//!
//! Drives an AST containing `QueryStep::SemanticSearch` /
//! `QueryStep::RawVectorSearch` through `compile_query` +
//! `ExecutionCoordinator::execute_compiled_read`. Without dispatch wiring,
//! compile treats the new variants as no-ops and the executor returns a
//! plain node scan — these tests pin the hard/degrade error semantics
//! through the FFI path.

use std::sync::Arc;

use fathomdb::{
    EmbedderChoice, EmbedderError, Engine, EngineError, EngineOptions, QueryEmbedder,
    QueryEmbedderIdentity,
};
use fathomdb_query::{QueryAst, QueryStep, compile_query};
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

fn open_engine_without_embedder() -> (TempDir, Engine) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let mut opts = EngineOptions::new(&db_path);
    opts.vector_dimension = Some(DIM);
    let engine = Engine::open(opts).expect("engine opens");
    (dir, engine)
}

fn semantic_ast() -> QueryAst {
    QueryAst {
        root_kind: KIND.to_owned(),
        steps: vec![QueryStep::SemanticSearch {
            text: "anything".to_owned(),
            limit: 5,
        }],
        expansions: vec![],
        edge_expansions: vec![],
        final_limit: None,
    }
}

fn raw_vector_ast(vec: Vec<f32>) -> QueryAst {
    QueryAst {
        root_kind: KIND.to_owned(),
        steps: vec![QueryStep::RawVectorSearch { vec, limit: 5 }],
        expansions: vec![],
        edge_expansions: vec![],
        final_limit: None,
    }
}

#[test]
fn test_compile_semantic_search_produces_carrier() {
    let ast = semantic_ast();
    let compiled = compile_query(&ast).expect("compile_query");
    assert!(
        compiled.semantic_search.is_some(),
        "compile_query must emit a CompiledSemanticSearch carrier for \
         QueryStep::SemanticSearch; got None — compile is still a no-op"
    );
    let carrier = compiled.semantic_search.as_ref().expect("carrier present");
    assert_eq!(carrier.root_kind, KIND);
    assert_eq!(carrier.text, "anything");
    assert_eq!(carrier.limit, 5);
}

#[test]
fn test_compile_raw_vector_search_produces_carrier() {
    let ast = raw_vector_ast(vec![0.1, 0.2, 0.3, 0.4]);
    let compiled = compile_query(&ast).expect("compile_query");
    assert!(
        compiled.raw_vector_search.is_some(),
        "compile_query must emit a CompiledRawVectorSearch carrier for \
         QueryStep::RawVectorSearch; got None — compile is still a no-op"
    );
    let carrier = compiled
        .raw_vector_search
        .as_ref()
        .expect("carrier present");
    assert_eq!(carrier.root_kind, KIND);
    assert_eq!(carrier.limit, 5);
    assert_eq!(carrier.vec, vec![0.1, 0.2, 0.3, 0.4]);
}

#[test]
fn test_semantic_dispatch_no_embedder_profile_errors() {
    // Engine has no active profile configured. Compile + dispatch must
    // surface EmbedderNotConfigured, not degrade to an empty result.
    let (_dir, engine) = open_engine_with_embedder();
    let ast = semantic_ast();
    let compiled = compile_query(&ast).expect("compile_query");
    let err = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect_err("must hard-error when no active profile");
    match err {
        EngineError::EmbedderNotConfigured => {}
        other => panic!("expected EmbedderNotConfigured, got {other:?}"),
    }
}

#[test]
fn test_semantic_dispatch_kind_not_indexed_errors() {
    let (_dir, engine) = open_engine_with_embedder();
    engine
        .admin()
        .service()
        .configure_embedding(&TestEmbedder::new(), true)
        .expect("configure_embedding");
    let ast = semantic_ast();
    let compiled = compile_query(&ast).expect("compile_query");
    let err = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect_err("must hard-error when kind not indexed");
    match err {
        EngineError::KindNotVectorIndexed { kind } => assert_eq!(kind, KIND),
        other => panic!("expected KindNotVectorIndexed, got {other:?}"),
    }
}

#[test]
fn test_raw_vector_dispatch_dimension_mismatch_errors() {
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

    // Wrong dimension (3 != DIM=4).
    let ast = raw_vector_ast(vec![0.1, 0.2, 0.3]);
    let compiled = compile_query(&ast).expect("compile_query");
    let err = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect_err("must hard-error on dimension mismatch");
    match err {
        EngineError::DimensionMismatch { expected, actual } => {
            assert_eq!(expected, DIM);
            assert_eq!(actual, 3);
        }
        other => panic!("expected DimensionMismatch, got {other:?}"),
    }
}

#[test]
fn test_semantic_dispatch_embedder_unavailable_degrades() {
    // Engine opened without an in-process embedder. After the profile is
    // configured (by a one-shot embedder that has the right identity), the
    // coordinator should degrade because `query_embedder` is unset.
    let (_dir, engine) = open_engine_without_embedder();
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

    let ast = semantic_ast();
    let compiled = compile_query(&ast).expect("compile_query");
    let rows = engine
        .coordinator()
        .execute_compiled_read(&compiled)
        .expect("embedder unavailable must degrade, not error");
    assert!(rows.nodes.is_empty());
    assert!(
        rows.was_degraded,
        "embedder unavailable must mark was_degraded=true"
    );
}
