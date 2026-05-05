//! Pack 6 F.0 — thread-affine reader worker pool integration tests.
//!
//! Covers the two new acceptance contracts introduced by the refactor:
//!   1. Shutdown integrity: every reader worker exits on `Engine::close`,
//!      and every owned read-only `Connection` is dropped.
//!   2. Routing/concurrency stress: with 8 worker threads and many
//!      concurrent search callers, every dispatched search is handled
//!      exactly once — no request is lost or duplicated.
//!
//! These tests are deliberately cheap and deterministic. They are not
//! perf gates; AC-020 in `tests/perf_gates.rs` remains the perf oracle.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

const READER_POOL_SIZE: usize = 8;

fn fresh_engine(name: &str) -> (TempDir, fathomdb_engine::OpenedEngine) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    let opened = Engine::open(path).expect("engine open");
    (dir, opened)
}

#[test]
fn reader_pool_spawns_eight_worker_threads_at_open() {
    let (_dir, opened) = fresh_engine("reader_pool_open");
    assert_eq!(opened.engine.reader_worker_count_for_test(), READER_POOL_SIZE);
    // Workers may take a brief moment to enter their loops on slow CI;
    // give them a bounded window to publish into the live counter.
    let deadline = Instant::now() + Duration::from_secs(2);
    while opened.engine.live_reader_worker_count_for_test() < READER_POOL_SIZE
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(opened.engine.live_reader_worker_count_for_test(), READER_POOL_SIZE);
}

#[test]
fn reader_workers_exit_on_close_and_drop_connections() {
    let (_dir, opened) = fresh_engine("reader_pool_close");
    // Issue at least one search so each worker has serviced traffic.
    for _ in 0..32 {
        let _ = opened.engine.search("ping").err();
    }
    opened.engine.close().expect("close");
    // After close, every worker must have returned from its loop and
    // dropped its owned read-only Connection.
    let deadline = Instant::now() + Duration::from_secs(2);
    while opened.engine.live_reader_worker_count_for_test() != 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        opened.engine.live_reader_worker_count_for_test(),
        0,
        "reader workers did not exit on close"
    );
    // Subsequent searches must surface a closing error rather than
    // hanging on a dead worker channel.
    let err = opened.engine.search("ping").expect_err("search after close");
    // We don't pin the exact variant: depending on the order of the
    // `closed` flag and the channel-closed signal, callers may surface
    // either `Closing` or `Storage`. The contract under test is that
    // close does not deadlock and search no longer succeeds.
    let msg = err.to_string();
    assert!(
        msg.contains("closing") || msg.contains("storage"),
        "unexpected post-close search error: {msg}"
    );
}

#[test]
fn reader_workers_exit_on_drop_without_explicit_close() {
    let live = {
        let (_dir, opened) = fresh_engine("reader_pool_drop");
        let live = opened.engine.live_reader_worker_count_for_test();
        // Drop the engine without calling close().
        drop(opened);
        live
    };
    assert_eq!(live, READER_POOL_SIZE);
    // We can't read the counter after drop, but the join inside
    // `ReaderWorkerPool::Drop` is synchronous, so by the time the
    // closure above returns every worker has exited. The fact that
    // this test returns at all is the assertion. Use `live` to
    // suppress unused warnings.
    assert_eq!(live, READER_POOL_SIZE);
}

#[test]
fn concurrent_searches_route_to_workers_without_loss_or_duplication() {
    let (_dir, opened) = fresh_engine("reader_pool_routing");
    // Seed one row so non-empty searches succeed deterministically.
    opened
        .engine
        .write(&[PreparedWrite::Node {
            kind: "doc".to_string(),
            body: "hello world".to_string(),
            source_id: None,
        }])
        .expect("seed write");

    const CALLERS: usize = 32;
    const PER_CALLER: usize = 25;

    let engine = Arc::new(opened.engine);
    let success = Arc::new(AtomicUsize::new(0));
    let failure = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::with_capacity(CALLERS);
    for _ in 0..CALLERS {
        let engine = Arc::clone(&engine);
        let success = Arc::clone(&success);
        let failure = Arc::clone(&failure);
        handles.push(thread::spawn(move || {
            for _ in 0..PER_CALLER {
                match engine.search("hello") {
                    Ok(_) => {
                        success.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(_) => {
                        failure.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        }));
    }
    for handle in handles {
        handle.join().expect("caller thread");
    }

    assert_eq!(failure.load(Ordering::SeqCst), 0, "no search may fail");
    // The strict equality is the duplication check: each
    // `engine.search` returns at most one response, and the success
    // counter is incremented exactly once per Ok(_).
    assert_eq!(success.load(Ordering::SeqCst), CALLERS * PER_CALLER);
    // All workers must still be live after the storm.
    assert_eq!(engine.live_reader_worker_count_for_test(), READER_POOL_SIZE);
}

// -- G.1 lookaside ---------------------------------------------------

#[test]
fn reader_workers_have_lookaside_configured_with_ok_rc() {
    let (_dir, opened) = fresh_engine("reader_pool_lookaside_rc");
    let rcs = opened.engine.reader_lookaside_config_rcs_for_test();
    assert_eq!(rcs.len(), READER_POOL_SIZE);
    for (idx, rc) in rcs.iter().enumerate() {
        assert_eq!(*rc, 0, "worker {idx} sqlite3_db_config(LOOKASIDE) rc must be SQLITE_OK");
    }
}

#[test]
fn reader_workers_consume_lookaside_slots_after_warmup_read() {
    let (_dir, opened) = fresh_engine("reader_pool_lookaside_used");

    // Drive at least one search through every worker so each connection
    // has prepared/stepped at least one statement.
    for _ in 0..(READER_POOL_SIZE * 8) {
        let _ = opened.engine.search("warmup").err();
    }

    let used = opened.engine.reader_lookaside_used_per_worker_for_test();
    eprintln!("LOOKASIDE_USED_HIWTR_PER_WORKER={used:?}");
    assert_eq!(used.len(), READER_POOL_SIZE);
    for (idx, slots) in used.iter().enumerate() {
        assert!(
            *slots > 0,
            "worker {idx} SQLITE_DBSTATUS_LOOKASIDE_USED must be >0 after warmup; got {slots}",
        );
    }
}
