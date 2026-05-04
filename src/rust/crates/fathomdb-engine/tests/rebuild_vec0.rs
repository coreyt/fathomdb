use std::sync::Arc;
use std::thread;
use std::time::Duration;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::lifecycle::ProjectionStatus;
use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
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
        Self { identity: EmbedderIdentity::new("vec0-rebuild", "rev-a", dim), vector }
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

fn wait_until<F: FnMut() -> bool>(mut predicate: F, timeout: Duration) -> bool {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if predicate() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    predicate()
}

#[test]
fn rebuild_vec0_resets_vector_rows_and_preserves_fts() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "rebuild_vec0");

    let embedder = Arc::new(DeterministicEmbedder::new(8));
    let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "fts content survives".to_string(),
        }])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");
    let pre = opened.engine.vector_row_count_for_test().expect("count");
    assert!(pre >= 1);

    let fts_count_before = {
        let connection = Connection::open(&path).expect("open sqlite");
        let total: i64 = connection
            .query_row("SELECT COUNT(*) FROM search_index", [], |row| row.get(0))
            .expect("count fts");
        total
    };
    assert!(fts_count_before >= 1);

    opened.engine.rebuild_vec0().expect("rebuild_vec0");
    assert!(wait_until(
        || opened
            .engine
            .projection_status_for_test("doc")
            .map(|s| s == ProjectionStatus::UpToDate)
            .unwrap_or(false),
        Duration::from_secs(10),
    ));

    let fts_count_after = {
        let connection = Connection::open(&path).expect("open sqlite");
        let total: i64 = connection
            .query_row("SELECT COUNT(*) FROM search_index", [], |row| row.get(0))
            .expect("count fts");
        total
    };
    assert_eq!(fts_count_after, fts_count_before, "FTS5 must be untouched by rebuild_vec0");
    assert!(opened.engine.vector_row_count_for_test().expect("count after") >= 1);
}
