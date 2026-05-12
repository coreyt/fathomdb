use std::sync::Arc;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite, RebuildKind};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

#[derive(Clone, Debug)]
struct DeterministicEmbedder {
    identity: EmbedderIdentity,
    vector: Vector,
}

impl DeterministicEmbedder {
    fn new(dim: u32) -> Self {
        let mut vector = vec![0.0_f32; dim as usize];
        vector[0] = 1.0;
        Self { identity: EmbedderIdentity::new("rebuild-report", "rev-a", dim), vector }
    }
}

impl Embedder for DeterministicEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        Ok(self.vector.clone())
    }
}

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

#[test]
fn rebuild_projections_returns_structured_report() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "rebuild_report_projections");

    let embedder = Arc::new(DeterministicEmbedder::new(8));
    let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    // Write a small fixture of canonical rows so rebuild has real work.
    for i in 0..3 {
        opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: format!("payload {i}"),
                source_id: None,
            }])
            .expect("write");
    }
    opened.engine.drain(10_000).expect("drain");

    let report = opened.engine.rebuild_projections().expect("rebuild_projections");

    assert_eq!(report.kind, RebuildKind::Projections);
    assert!(
        report.rows_invalidated > 0,
        "rows_invalidated should be > 0 (was {})",
        report.rows_invalidated
    );
    assert!(
        report.rows_rebuilt > 0,
        "rows_rebuilt should be > 0 after rebuild_projections (was {})",
        report.rows_rebuilt
    );
    // projection_cursor_after is reset to 0 (per current rebuild semantics).
    assert_eq!(report.projection_cursor_after, 0);
}

#[test]
fn rebuild_vec0_returns_structured_report() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "rebuild_report_vec0");

    let embedder = Arc::new(DeterministicEmbedder::new(8));
    let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    for i in 0..2 {
        opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: format!("payload {i}"),
                source_id: None,
            }])
            .expect("write");
    }
    opened.engine.drain(10_000).expect("drain");

    let report = opened.engine.rebuild_vec0().expect("rebuild_vec0");

    assert_eq!(report.kind, RebuildKind::Vec0);
    assert!(
        report.rows_invalidated > 0,
        "rows_invalidated should be > 0 (was {})",
        report.rows_invalidated
    );
    assert_eq!(
        report.rows_rebuilt, 0,
        "vec0 rebuild does no synchronous re-insert; re-derivation is async via projection scheduler"
    );
    assert_eq!(report.projection_cursor_after, 0);
}
