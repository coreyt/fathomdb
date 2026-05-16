//! AC-040a engine half: `Engine::dump_profile` echoes the stored
//! embedder identity + dimension plus the registered vectorized kinds.

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::Engine;
use fathomdb_schema::SQLITE_SUFFIX;
use std::sync::Arc;
use tempfile::TempDir;

struct StubEmbedder(EmbedderIdentity);

impl Embedder for StubEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.0.clone()
    }
    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        Ok(vec![0.0; self.0.dimension as usize])
    }
}

#[test]
fn ac_040a_dump_profile_returns_stored_identity_and_dimension() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("profile{SQLITE_SUFFIX}"));
    let opened = Engine::open_with_embedder_for_test(
        path,
        Arc::new(StubEmbedder(EmbedderIdentity::new("model-a", "rev-7", 256))),
    )
    .expect("open with embedder");
    let report = opened.engine.dump_profile().expect("dump_profile");
    assert_eq!(report.embedder_identity, "model-a:rev-7");
    assert_eq!(report.embedder_dimension, 256);
    assert!(report.vectorized_kinds.is_empty(), "no vectorized kinds registered yet");
}

#[test]
fn ac_040a_dump_profile_lists_registered_vectorized_kinds() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("profile_kinds{SQLITE_SUFFIX}"));
    let opened = Engine::open_with_embedder_for_test(
        path,
        Arc::new(StubEmbedder(EmbedderIdentity::new("model-a", "rev-7", 256))),
    )
    .expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("register doc");
    opened.engine.configure_vector_kind_for_test("note").expect("register note");
    let report = opened.engine.dump_profile().expect("dump_profile");
    assert_eq!(report.vectorized_kinds, vec!["doc".to_string(), "note".to_string()]);
}
