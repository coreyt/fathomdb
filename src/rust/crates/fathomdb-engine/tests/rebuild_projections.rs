use std::sync::Arc;
use std::thread;
use std::time::Duration;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::lifecycle::ProjectionStatus;
use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use rusqlite::Connection;
use tempfile::TempDir;

const SENTINEL: &[u8; 16] = b"FATHOMDB_SENT_42";

#[derive(Clone, Debug)]
struct DeterministicEmbedder {
    identity: EmbedderIdentity,
    vector: Vector,
}

impl DeterministicEmbedder {
    fn new(dim: u32) -> Self {
        let mut vector = vec![0.0_f32; dim as usize];
        vector[0] = 1.0;
        Self { identity: EmbedderIdentity::new("rebuild", "rev-a", dim), vector }
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

#[derive(Clone, Debug)]
struct FailingEmbedder {
    identity: EmbedderIdentity,
    fails: Arc<std::sync::atomic::AtomicUsize>,
}

impl FailingEmbedder {
    fn new(dim: u32) -> Self {
        Self {
            identity: EmbedderIdentity::new("rebuild", "rev-a", dim),
            fails: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
}

impl Embedder for FailingEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        self.fails.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Err(EmbedderError::Failed { message: "deterministic failure".to_string() })
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
fn ac_044_rebuild_projections_purges_sentinel_bytes() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "rebuild_sentinel");

    let embedder = Arc::new(DeterministicEmbedder::new(8));
    {
        let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("open");
        opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
        opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: "canonical body alpha".to_string(),
            }])
            .expect("write");
        opened.engine.drain(10_000).expect("drain");
    }

    {
        let connection = Connection::open(&path).expect("open sqlite");
        connection
            .execute(
                "INSERT INTO search_index(body, kind, write_cursor) VALUES(?1, 'doc', 9999)",
                rusqlite::params![std::str::from_utf8(SENTINEL).unwrap()],
            )
            .expect("inject sentinel");
        connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_row| Ok(()))
            .expect("checkpoint");
    }

    let raw_before = std::fs::read(&path).expect("read db");
    assert!(
        raw_before.windows(SENTINEL.len()).any(|window| window == SENTINEL),
        "sentinel was not actually written into the file"
    );

    {
        let opened = Engine::open_with_embedder_for_test(&path, embedder.clone()).expect("reopen");
        opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
        opened.engine.rebuild_projections().expect("rebuild_projections");
        assert!(wait_until(
            || opened
                .engine
                .projection_status_for_test("doc")
                .map(|s| s == ProjectionStatus::UpToDate)
                .unwrap_or(false),
            Duration::from_secs(10),
        ));
        opened.engine.drain(10_000).expect("post-rebuild drain");
    }

    {
        let connection = Connection::open(&path).expect("open sqlite");
        connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_row| Ok(()))
            .expect("checkpoint");
        connection.execute("VACUUM", []).expect("vacuum");
    }

    let raw_after = std::fs::read(&path).expect("read db");
    assert!(
        !raw_after.windows(SENTINEL.len()).any(|window| window == SENTINEL),
        "sentinel still present in shadow-table pages after rebuild"
    );
}

#[test]
fn ac_063c_rebuild_projections_materializes_failed_terminal_rows() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "rebuild_failed");

    let failing = Arc::new(FailingEmbedder::new(8));
    let cursor = {
        let opened = Engine::open_with_embedder_for_test(&path, failing.clone()).expect("open");
        opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
        opened.engine.set_projection_retry_delays_for_test(&[0, 0, 0]);
        let receipt = opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: "failure body".to_string(),
            }])
            .expect("write");
        assert!(wait_until(
            || opened
                .engine
                .projection_status_for_test("doc")
                .map(|s| s == ProjectionStatus::Failed)
                .unwrap_or(false),
            Duration::from_secs(10),
        ));
        assert_eq!(
            opened.engine.projection_failure_count_for_test(receipt.cursor).expect("failure count"),
            1
        );
        receipt.cursor
    };

    let healthy = Arc::new(DeterministicEmbedder::new(8));
    let opened = Engine::open_with_embedder_for_test(&path, healthy.clone()).expect("reopen");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
    opened.engine.rebuild_projections().expect("rebuild_projections");
    assert!(wait_until(
        || opened
            .engine
            .projection_status_for_test("doc")
            .map(|s| s == ProjectionStatus::UpToDate)
            .unwrap_or(false),
        Duration::from_secs(10),
    ));
    assert!(opened.engine.has_vector_for_cursor_for_test(cursor).expect("has_vector"));
}
