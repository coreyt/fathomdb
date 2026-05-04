use std::time::{Duration, Instant};

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::TempDir;

#[derive(Clone, Debug)]
struct FixedEmbedder;

impl Embedder for FixedEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new("deterministic", "rev-a", 384)
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        let mut values = vec![0.0_f32; 384];
        values[0] = 1.0;
        Ok(values)
    }
}

fn open_fixture(name: &str) -> (TempDir, fathomdb_engine::OpenedEngine) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    let opened = Engine::open(path).unwrap();
    (dir, opened)
}

#[test]
fn ac_059a_projection_cursor_is_monotonic_non_decreasing() {
    let (_dir, opened) = open_fixture("monotonic");
    let mut previous = opened.engine.search("doc").unwrap().projection_cursor;

    for i in 0..1_000_u32 {
        if i % 10 == 0 {
            opened
                .engine
                .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: format!("doc {i}") }])
                .unwrap();
        }
        let current = opened.engine.search("doc").unwrap().projection_cursor;
        assert!(current >= previous);
        previous = current;
    }
}

#[test]
fn ac_059b_write_cursor_is_satisfied_by_projection_cursor_and_queryable() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("satisfied{SQLITE_SUFFIX}"));
    let opened = Engine::open_with_embedder_for_test(&path, Arc::new(FixedEmbedder)).unwrap();
    opened.engine.configure_vector_kind_for_test("doc").unwrap();

    let write_cursor = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "findable phase seven document".to_string(),
        }])
        .unwrap()
        .cursor;

    let started = Instant::now();
    loop {
        let result = opened.engine.search("findable").unwrap();
        if result.projection_cursor >= write_cursor {
            assert_eq!(result.results, vec!["findable phase seven document".to_string()]);
            assert!(opened.engine.has_vector_for_cursor_for_test(write_cursor).unwrap());
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "projection_cursor never satisfied write cursor"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn failed_commit_does_not_publish_projection_cursor() {
    let (_dir, opened) = open_fixture("failed_cursor");

    let committed = opened
        .engine
        .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: "first".to_string() }])
        .unwrap()
        .cursor;

    let err = opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "force_storage_failure_for_test".to_string(),
            body: "allowed".to_string(),
        }])
        .expect("test-like node kind is still user data");
    assert_eq!(err.cursor, committed + 1);

    opened.engine.force_next_commit_failure_for_test();
    let err = opened
        .engine
        .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: "must fail".to_string() }])
        .expect_err("forced storage failure should fail after validation");
    assert_eq!(err, fathomdb_engine::EngineError::Storage);

    let after_failure = opened.engine.search("first").unwrap().projection_cursor;
    assert_eq!(after_failure, committed + 1);
}

#[test]
fn concurrent_search_does_not_observe_speculative_failed_cursor() {
    let (_dir, opened) = open_fixture("failed_cursor_race");
    let committed = opened
        .engine
        .write(&[PreparedWrite::Node { kind: "doc".to_string(), body: "first".to_string() }])
        .unwrap()
        .cursor;

    let engine = Arc::new(opened.engine);
    let barrier = Arc::new(Barrier::new(2));

    let writer_engine = Arc::clone(&engine);
    let writer_barrier = Arc::clone(&barrier);
    let writer = thread::spawn(move || {
        writer_barrier.wait();
        writer_engine.force_next_commit_failure_for_test();
        writer_engine
            .write(&[PreparedWrite::Node {
                kind: "doc".to_string(),
                body: "must fail".to_string(),
            }])
            .expect_err("forced storage failure should fail")
    });

    barrier.wait();
    let observed = engine.search("first").unwrap().projection_cursor;
    let err = writer.join().unwrap();

    assert_eq!(err, fathomdb_engine::EngineError::Storage);
    assert_eq!(observed, committed);
    assert_eq!(engine.search("first").unwrap().projection_cursor, committed);
}
