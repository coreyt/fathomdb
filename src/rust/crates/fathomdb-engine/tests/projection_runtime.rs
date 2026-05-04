use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::lifecycle::ProjectionStatus;
use fathomdb_engine::{Engine, EngineError, EngineOpenError, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

#[derive(Clone, Debug)]
struct SleepingEmbedder {
    identity: EmbedderIdentity,
    vector: Vector,
    delay: Duration,
    fail: bool,
}

impl SleepingEmbedder {
    fn success(delay: Duration) -> Self {
        Self {
            identity: EmbedderIdentity::new("deterministic", "rev-a", 384),
            vector: unit_vector(384),
            delay,
            fail: false,
        }
    }

    fn failing(delay: Duration) -> Self {
        Self { fail: true, ..Self::success(delay) }
    }
}

impl Embedder for SleepingEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        thread::sleep(self.delay);
        if self.fail {
            Err(EmbedderError::Failed { message: "deterministic failure".to_string() })
        } else {
            Ok(self.vector.clone())
        }
    }
}

#[derive(Clone, Debug)]
struct RoutedEmbedder {
    identity: EmbedderIdentity,
}

impl RoutedEmbedder {
    fn new(dimension: u32) -> Self {
        Self { identity: EmbedderIdentity::new("routed", "rev-a", dimension) }
    }
}

impl Embedder for RoutedEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, text: &str) -> Result<Vector, EmbedderError> {
        let mut vector = vec![0.0_f32; self.identity.dimension as usize];
        vector[match text {
            "semantic-query" | "vector-only document" => 0,
            "hybrid" | "hybrid retrieval document" => 1,
            _ => 2,
        }] = 1.0;
        Ok(vector)
    }
}

fn fixture_path(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    (dir, path)
}

fn current_test_binary() -> std::path::PathBuf {
    std::env::current_exe().expect("test binary path")
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> bool {
    let started = Instant::now();
    loop {
        if child.try_wait().expect("poll child").is_some() {
            return true;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_status(
    engine: &Engine,
    kind: &str,
    expected: ProjectionStatus,
    timeout: Duration,
) -> ProjectionStatus {
    let started = Instant::now();
    loop {
        let observed = engine.projection_status_for_test(kind).expect("projection status");
        if observed == expected {
            return observed;
        }
        assert!(
            started.elapsed() < timeout,
            "timed out waiting for {expected:?}; saw {observed:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn ac_024a_second_open_rejects_quickly_with_pending_vector_work() {
    let (dir, path) = fixture_path("pending_lock");
    let embedder = Arc::new(SleepingEmbedder::success(Duration::from_millis(5)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
    opened.engine.set_projection_scheduler_frozen_for_test(true);

    for i in 0..100 {
        opened
            .engine
            .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: format!("doc {i}") }])
            .expect("write");
    }

    let mut child = Command::new(current_test_binary())
        .arg("--exact")
        .arg("child_second_open_locked_with_pending_vector")
        .arg("--ignored")
        .env("FATHOMDB_TEST_DB_PATH", &path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn child");

    let started = Instant::now();
    assert!(wait_with_timeout(&mut child, Duration::from_secs(1)));
    assert!(started.elapsed() <= Duration::from_secs(1));
    assert!(child.wait().expect("child status").success());
    drop(opened);
    drop(dir);
}

#[test]
#[ignore]
fn child_second_open_locked_with_pending_vector() {
    let path = std::env::var_os("FATHOMDB_TEST_DB_PATH").expect("db path");
    let started = Instant::now();
    let err = Engine::open(path).expect_err("second open must fail");
    assert!(started.elapsed() <= Duration::from_secs(1));
    assert!(matches!(err, EngineOpenError::DatabaseLocked { .. }));
}

#[test]
fn ac_025_drop_with_pending_vector_work_returns_promptly() {
    let (_dir, path) = fixture_path("drop_pending");
    let embedder = Arc::new(SleepingEmbedder::success(Duration::from_millis(5)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
    opened.engine.set_projection_scheduler_frozen_for_test(true);

    for i in 0..1_000 {
        opened
            .engine
            .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: format!("doc {i}") }])
            .expect("write");
    }

    let started = Instant::now();
    drop(opened);
    assert!(started.elapsed() <= Duration::from_secs(30));
}

#[test]
fn ac_029_canonical_writes_complete_under_projection_stall() {
    let (_dir, path) = fixture_path("projection_stall");
    let embedder = Arc::new(SleepingEmbedder::success(Duration::from_millis(1)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    let baseline_started = Instant::now();
    for i in 0..1_000 {
        opened
            .engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: format!("baseline {i}"),
            }])
            .expect("baseline write");
    }
    let baseline = baseline_started.elapsed();
    opened.engine.drain(30_000).expect("baseline drain");

    opened.engine.set_projection_scheduler_frozen_for_test(true);
    let stalled_started = Instant::now();
    for i in 0..1_000 {
        opened
            .engine
            .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: format!("stalled {i}") }])
            .expect("stalled write");
    }
    let stalled = stalled_started.elapsed();

    assert!(stalled <= baseline.mul_f32(1.5), "baseline={baseline:?} stalled={stalled:?}");
}

#[test]
fn ac_031_hybrid_search_surfaces_vector_soft_fallback_when_projection_lags() {
    let (_dir, path) = fixture_path("soft_fallback");
    let embedder = Arc::new(SleepingEmbedder::success(Duration::from_millis(1)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
    opened.engine.set_projection_scheduler_frozen_for_test(true);

    let receipt = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "phase nine hybrid search".to_string(),
        }])
        .expect("write");

    let result = opened.engine.search("hybrid").expect("search");
    assert_eq!(result.results, vec!["phase nine hybrid search".to_string()]);
    assert_eq!(result.projection_cursor + 1, receipt.cursor);
    assert_eq!(
        result.soft_fallback,
        Some(fathomdb_engine::SoftFallback { branch: fathomdb_engine::SoftFallbackBranch::Vector })
    );
}

#[test]
fn hybrid_search_returns_vector_results_when_text_branch_has_no_match() {
    let (_dir, path) = fixture_path("vector_materialization");
    let embedder = Arc::new(RoutedEmbedder::new(3));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "vector-only document".to_string(),
        }])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let result = opened.engine.search("semantic-query").expect("search");
    assert_eq!(result.results, vec!["vector-only document".to_string()]);
    assert_eq!(result.soft_fallback, None);
}

#[test]
fn hybrid_search_deduplicates_rows_seen_by_text_and_vector_branches() {
    let (_dir, path) = fixture_path("hybrid_dedup");
    let embedder = Arc::new(RoutedEmbedder::new(3));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "hybrid retrieval document".to_string(),
        }])
        .expect("write");
    opened.engine.drain(10_000).expect("drain");

    let result = opened.engine.search("hybrid").expect("search");
    assert_eq!(result.results, vec!["hybrid retrieval document".to_string()]);
    assert_eq!(result.soft_fallback, None);
}

#[test]
fn ac_032a_drain_succeeds_when_timeout_is_sufficient() {
    let (_dir, path) = fixture_path("drain_success");
    let embedder = Arc::new(SleepingEmbedder::success(Duration::from_millis(20)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    let mut last_cursor = 0;
    for i in 0..10 {
        last_cursor = opened
            .engine
            .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: format!("doc {i}") }])
            .expect("write")
            .cursor;
    }

    let started = Instant::now();
    opened.engine.drain(10_000).expect("drain");
    assert!(started.elapsed() <= Duration::from_secs(10));
    assert!(opened.engine.search("doc").unwrap().projection_cursor >= last_cursor);
}

#[test]
fn ac_032b_drain_returns_typed_timeout_when_work_does_not_finish() {
    let (_dir, path) = fixture_path("drain_timeout");
    let embedder = Arc::new(SleepingEmbedder::success(Duration::from_millis(250)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");

    for i in 0..10 {
        opened
            .engine
            .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: format!("doc {i}") }])
            .expect("write");
    }

    let started = Instant::now();
    let err = opened.engine.drain(100).expect_err("drain must time out");
    let elapsed = started.elapsed();
    assert_eq!(err, EngineError::Scheduler);
    assert!(elapsed <= Duration::from_millis(150), "elapsed={elapsed:?}");
}

#[test]
fn ac_033_provenance_growth_is_bounded_and_oldest_first() {
    let (_dir, path) = fixture_path("retention");
    let opened = Engine::open(&path).expect("open");
    opened.engine.set_provenance_row_cap_for_test(Some(10));
    opened
        .engine
        .write(&[PreparedWrite::AdminSchema {
            name: "audit".to_string(),
            kind: "append_only_log".to_string(),
            schema_json: "{}".to_string(),
            retention_json: "{}".to_string(),
        }])
        .expect("register collection");

    let mut crossed = false;
    for i in 0..30 {
        opened
            .engine
            .write(&[PreparedWrite::OpStore {
                collection: "audit".to_string(),
                record_key: format!("{i:03}"),
                schema_id: None,
                body: format!(r#"{{"i":{i}}}"#),
            }])
            .expect("append mutation");
        let row_count = opened.engine.provenance_row_count_for_test().expect("row count");
        if row_count >= 10 {
            crossed = true;
            assert!(row_count <= 11, "row_count={row_count}");
        }
    }

    assert!(crossed, "fixture must cross the configured cap");
    assert_eq!(
        opened.engine.oldest_provenance_record_key_for_test("audit").unwrap(),
        Some("020".to_string())
    );
}

#[test]
fn ac_063a_exhausted_projection_failure_is_recorded_once_and_vector_stays_absent() {
    let (_dir, path) = fixture_path("projection_failure");
    let embedder = Arc::new(SleepingEmbedder::failing(Duration::from_millis(1)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
    opened.engine.set_projection_retry_delays_for_test(&[0, 0, 0]);

    let cursor = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "will fail projection".to_string(),
        }])
        .expect("write")
        .cursor;

    opened.engine.drain(10_000).expect("drain");
    assert_eq!(
        wait_for_status(&opened.engine, "doc", ProjectionStatus::Failed, Duration::from_secs(2)),
        ProjectionStatus::Failed
    );
    assert_eq!(opened.engine.projection_failure_count_for_test(cursor).unwrap(), 1);
    assert!(!opened.engine.has_vector_for_cursor_for_test(cursor).unwrap());
    assert!(opened.engine.search("will").unwrap().projection_cursor >= cursor);
}

#[test]
fn ac_063b_restart_does_not_retry_terminal_projection_failures() {
    let (_dir, path) = fixture_path("projection_failure_restart");
    let failing = Arc::new(SleepingEmbedder::failing(Duration::from_millis(1)));
    let opened = Engine::open_with_embedder_for_test(&path, failing).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("vector kind");
    opened.engine.set_projection_retry_delays_for_test(&[0, 0, 0]);

    let cursor = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "still failed after restart".to_string(),
        }])
        .expect("write")
        .cursor;
    opened.engine.drain(10_000).expect("drain");
    opened.engine.close().expect("close");

    let good = Arc::new(SleepingEmbedder::success(Duration::from_millis(1)));
    let reopened = Engine::open_with_embedder_for_test(&path, good).expect("reopen");
    thread::sleep(Duration::from_millis(200));

    assert_eq!(reopened.engine.projection_failure_count_for_test(cursor).unwrap(), 1);
    assert!(!reopened.engine.has_vector_for_cursor_for_test(cursor).unwrap());
    assert_eq!(
        reopened.engine.projection_status_for_test("doc").unwrap(),
        ProjectionStatus::Failed
    );
}

#[test]
fn projection_status_is_tracked_per_kind_not_just_global_cursor() {
    let (_dir, path) = fixture_path("projection_status_per_kind");
    let embedder = Arc::new(SleepingEmbedder::success(Duration::from_millis(1)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("doc vector kind");
    opened.engine.configure_vector_kind_for_test("note").expect("note vector kind");

    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "note".to_string(),
            body: "already projected".to_string(),
        }])
        .expect("note write");
    opened.engine.drain(10_000).expect("note drain");
    opened.engine.set_projection_scheduler_frozen_for_test(true);
    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "still pending".to_string(),
        }])
        .expect("doc write");

    assert_eq!(
        opened.engine.projection_status_for_test("note").unwrap(),
        ProjectionStatus::UpToDate
    );
    assert_eq!(opened.engine.projection_status_for_test("doc").unwrap(), ProjectionStatus::Pending);
}

fn unit_vector(dimension: usize) -> Vector {
    let mut values = vec![0.0_f32; dimension];
    if dimension > 0 {
        values[0] = 1.0;
    }
    values
}
