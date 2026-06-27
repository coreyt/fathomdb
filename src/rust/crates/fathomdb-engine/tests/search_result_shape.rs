//! Asserts the public shape of `SearchResult` per `dev/interfaces/rust.md`
//! § Caller-visible data shapes.
//!
//! As of 0.8.8 EXP-OBS the struct is `#[non_exhaustive]` (additive-safe for the
//! deferred `QueryTrace.timings_ms` etc.), so external crates can READ its fields
//! but cannot construct it with a struct literal. These tests therefore assert
//! the shape on **engine-produced** `SearchResult` values — which also proves the
//! read-but-not-construct contract the `#[non_exhaustive]` attribute encodes.

use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, Explanation, OpenedEngine, SearchResult, SoftFallback};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

#[derive(Clone, Debug)]
struct FixedEmbedder;

impl Embedder for FixedEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new("deterministic", "rev-a", 8)
    }
    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        let mut v = vec![0.0_f32; 8];
        v[0] = 1.0;
        Ok(v)
    }
}

fn opened(name: &str) -> (TempDir, OpenedEngine) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).expect("open");
    (dir, opened)
}

#[test]
fn search_result_exposes_optional_soft_fallback_and_explanation() {
    let (_dir, opened) = opened("shape_default");
    let r: SearchResult = opened.engine.search("anything").expect("search");
    // The caller-visible fields are readable on engine-produced values.
    let _: u64 = r.projection_cursor;
    let _: Option<SoftFallback> = r.soft_fallback.clone();
    let _: Option<Explanation> = r.explanation.clone();
    // The default (explain=false) path fully suppresses the sidecar.
    assert!(r.explanation.is_none(), "default search must suppress the explanation sidecar");
    opened.engine.close().unwrap();
}

#[test]
fn search_explained_populates_the_explanation_sidecar() {
    let (_dir, opened) = opened("shape_explained");
    let r = opened
        .engine
        .search_explained("anything", None, 0, false, 0.3, 0)
        .expect("search_explained");
    assert!(r.explanation.is_some(), "search_explained must populate the explanation sidecar");
    opened.engine.close().unwrap();
}
