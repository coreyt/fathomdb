//! AC-040a engine half: `Engine::verify_embedder` returns a typed
//! match / mismatch report against the stored embedder profile.

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, VerifyEmbedderStatus};
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

fn db_path(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join(format!("verify{SQLITE_SUFFIX}"))
}

fn open_with(name: &str, revision: &str, dim: u32) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let identity = EmbedderIdentity::new(name, revision, dim);
    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder(identity));
    let opened =
        Engine::open_with_embedder_for_test(path.clone(), embedder).expect("open with embedder");
    opened.engine.close().expect("close");
    drop(opened);
    (dir, path)
}

#[test]
fn ac_040a_matching_identity_and_dimension_returns_match() {
    let (_dir, path) = open_with("model-x", "rev-1", 384);
    let opened = Engine::open_with_embedder_for_test(
        path,
        Arc::new(StubEmbedder(EmbedderIdentity::new("model-x", "rev-1", 384))),
    )
    .expect("reopen");
    let report = opened.engine.verify_embedder("model-x:rev-1", 384).expect("verify");
    assert_eq!(report.status, VerifyEmbedderStatus::Match);
    assert_eq!(report.stored_identity, "model-x:rev-1");
    assert_eq!(report.stored_dimension, 384);
    assert_eq!(report.supplied_identity, "model-x:rev-1");
    assert_eq!(report.supplied_dimension, 384);
}

#[test]
fn ac_040a_mismatched_identity_returns_identity_mismatch() {
    let (_dir, path) = open_with("model-x", "rev-1", 384);
    let opened = Engine::open_with_embedder_for_test(
        path,
        Arc::new(StubEmbedder(EmbedderIdentity::new("model-x", "rev-1", 384))),
    )
    .expect("reopen");
    let report = opened.engine.verify_embedder("model-y:rev-1", 384).expect("verify");
    assert_eq!(report.status, VerifyEmbedderStatus::IdentityMismatch);
}

#[test]
fn ac_040a_mismatched_dimension_returns_dimension_mismatch() {
    let (_dir, path) = open_with("model-x", "rev-1", 384);
    let opened = Engine::open_with_embedder_for_test(
        path,
        Arc::new(StubEmbedder(EmbedderIdentity::new("model-x", "rev-1", 384))),
    )
    .expect("reopen");
    let report = opened.engine.verify_embedder("model-x:rev-1", 512).expect("verify");
    assert_eq!(report.status, VerifyEmbedderStatus::DimensionMismatch);
    assert_eq!(report.stored_dimension, 384);
    assert_eq!(report.supplied_dimension, 512);
}

#[test]
fn ac_040a_both_mismatched_returns_both_mismatch() {
    let (_dir, path) = open_with("model-x", "rev-1", 384);
    let opened = Engine::open_with_embedder_for_test(
        path,
        Arc::new(StubEmbedder(EmbedderIdentity::new("model-x", "rev-1", 384))),
    )
    .expect("reopen");
    let report = opened.engine.verify_embedder("model-y:rev-2", 999).expect("verify");
    assert_eq!(report.status, VerifyEmbedderStatus::BothMismatch);
}
