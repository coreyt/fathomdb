use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use fathomdb_engine::lifecycle::{Event, EventCategory, EventSource};
use fathomdb_engine::Engine;
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
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

#[test]
fn ac_022a_close_releases_lock_for_sibling_process() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "lock_release");

    let opened = Engine::open(&path).expect("parent open");
    opened.engine.close().expect("parent close");

    let mut child = Command::new(current_test_binary())
        .arg("--exact")
        .arg("child_open_and_close")
        .arg("--ignored")
        .env("FATHOMDB_TEST_DB_PATH", &path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn child");

    assert!(wait_with_timeout(&mut child, Duration::from_secs(1)));
    assert!(child.wait().expect("child status").success());
}

#[test]
fn ac_022b_close_does_not_leak_file_descriptors() {
    // Run in a sibling child process so FD accounting is local to the
    // open/close window, not contaminated by parallel tests in the parent
    // test binary. P-FD-TOL = 5 per `dev/acceptance.md` parameter table.
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "fd_leak");
    let mut child = Command::new(current_test_binary())
        .arg("--exact")
        .arg("child_fd_leak_open_close")
        .arg("--ignored")
        .env("FATHOMDB_TEST_DB_PATH", &path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn child");
    assert!(wait_with_timeout(&mut child, Duration::from_secs(5)));
    assert!(child.wait().expect("child status").success());
}

#[test]
#[ignore]
fn child_fd_leak_open_close() {
    let path = std::env::var_os("FATHOMDB_TEST_DB_PATH").expect("db path");
    let before = fd_count();
    let opened = Engine::open(path).expect("child open");
    opened.engine.close().expect("child close");
    let after = fd_count();
    assert!(after <= before + 5, "fd leak: before={before} after={after}");
}

#[test]
fn ac_022c_process_exits_within_five_seconds_after_close_returns() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir, "exit_timing");
    let marker = dir.path().join("close-returned.marker");
    let mut child = Command::new(current_test_binary())
        .arg("--exact")
        .arg("child_open_close_print_after_close")
        .arg("--ignored")
        .env("FATHOMDB_TEST_DB_PATH", &path)
        .env("FATHOMDB_TEST_CLOSE_MARKER", &marker)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn child");

    let marker_wait = Instant::now();
    while !marker.exists() {
        assert!(marker_wait.elapsed() < Duration::from_secs(5), "close marker not written");
        thread::sleep(Duration::from_millis(10));
    }

    let started = Instant::now();
    assert!(wait_with_timeout(&mut child, Duration::from_secs(5)));
    assert!(started.elapsed() <= Duration::from_secs(5));
    assert!(child.wait().expect("child status").success());
}

#[test]
#[ignore]
fn child_open_and_close() {
    let path = std::env::var_os("FATHOMDB_TEST_DB_PATH").expect("db path");
    let opened = Engine::open(path).expect("child open");
    opened.engine.close().expect("child close");
}

#[test]
#[ignore]
fn child_open_close_print_after_close() {
    let path = std::env::var_os("FATHOMDB_TEST_DB_PATH").expect("db path");
    let marker = std::env::var_os("FATHOMDB_TEST_CLOSE_MARKER").expect("close marker");
    let opened = Engine::open(path).expect("child open");
    opened.engine.close().expect("child close");
    std::fs::write(marker, b"CLOSE_RETURNED").expect("write close marker");
}

#[cfg(unix)]
fn fd_count() -> usize {
    // `/dev/fd` exists on Linux (symlink to `/proc/self/fd`) and macOS (devfs).
    std::fs::read_dir("/dev/fd").expect("fd directory").count()
}

#[cfg(windows)]
fn fd_count() -> usize {
    // Windows has no per-process FD table; use the kernel handle count as a
    // proxy. Sufficient for before/after delta leak detection.
    extern "system" {
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn GetProcessHandleCount(
            h_process: *mut core::ffi::c_void,
            pdw_handle_count: *mut u32,
        ) -> i32;
    }
    let mut count: u32 = 0;
    let ok = unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut count) };
    assert!(ok != 0, "GetProcessHandleCount failed");
    count as usize
}

#[derive(Default)]
struct CapturingSubscriber {
    events: Mutex<Vec<Event>>,
}

impl fathomdb_engine::lifecycle::Subscriber for CapturingSubscriber {
    fn on_event(&self, event: &Event) {
        self.events.lock().unwrap().push(event.clone());
    }
}

// AC-021: Zero SQLITE_SCHEMA warnings under concurrent reads + admin DDL.
//
// `agent-verify` runs the 5 s smoke window. `scripts/check.sh` sets
// `AGENT_LONG=1` to run the full 60 s window from the AC fixture spec.
// Catalog: `dev/test-plan.md` § Implementation Order step 3.
#[test]
fn ac_021_zero_sqlite_schema_warnings_under_concurrent_reads_and_ddl() {
    use fathomdb_engine::PreparedWrite;
    use std::sync::atomic::{AtomicBool, Ordering};

    let dir = TempDir::new().unwrap();
    let opened = Engine::open(dir.path().join("schema_flood.sqlite")).expect("open");
    let engine = Arc::new(opened.engine);

    let sink = Arc::new(CapturingSubscriber::default());
    let _sub = engine.subscribe(sink.clone());

    let stop = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();

    for _ in 0..8 {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        handles.push(thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let _ = engine.search("hello");
            }
        }));
    }

    let ddl_handle = {
        let engine = Arc::clone(&engine);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            let mut tick: u64 = 0;
            while !stop.load(Ordering::Relaxed) {
                let name = format!("things_{}", tick % 4);
                let _ = engine.write(&[PreparedWrite::AdminSchema {
                    name,
                    kind: "latest_state".to_string(),
                    schema_json: "{}".to_string(),
                    retention_json: "{}".to_string(),
                }]);
                tick = tick.wrapping_add(1);
                thread::sleep(Duration::from_millis(1000));
            }
        })
    };

    let window_secs = if std::env::var_os("AGENT_LONG").is_some() { 60 } else { 5 };
    thread::sleep(Duration::from_secs(window_secs));
    stop.store(true, Ordering::Relaxed);

    for handle in handles {
        handle.join().expect("reader thread");
    }
    ddl_handle.join().expect("ddl thread");

    let captured = sink.events.lock().unwrap();
    let schema_errors = captured
        .iter()
        .filter(|e| e.source == EventSource::Engine && e.category == EventCategory::Error)
        .count();
    assert_eq!(
        schema_errors, 0,
        "expected zero SQLITE_SCHEMA error events under concurrent reads + DDL"
    );
}
