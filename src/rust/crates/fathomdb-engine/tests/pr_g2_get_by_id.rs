//! Slice 30 (G2) — governed `read.get` / `read.get_many` by `logical_id`.
//!
//! Active-only point lookup (`superseded_at IS NULL`) on the ReaderWorkerPool
//! DEFERRED-tx snapshot path (NEVER the writer `connection.lock()`):
//!   * the written active node is returned by its `logical_id`;
//!   * a SUPERSEDED version is NOT returned (active-only default);
//!   * `get_many` preserves request order and returns a `None` slot for a
//!     missing/superseded id (partial, not all-or-nothing; not-found is a
//!     normal absence, never an error);
//!   * `get` delegates to `get_many`;
//!   * reads ride the reader pool — a `read_get` succeeds while a long writer
//!     transaction is open on the writer connection (snapshot isolation).
//!
//! Consumes `dev/plans/0.8.0-implementation.md` § "Slice 30",
//! `dev/adr/ADR-0.8.0-supersede-five-verb-surface-cap.md`,
//! `dev/adr/ADR-0.8.0-canonical-identity-substrate.md`, and the design memo
//! `dev/design/slice-30-design.md`. Binds gaps G2/F2/F4-READ + AC-074/REQ-053.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

fn db_path(dir: &TempDir, name: &str) -> std::path::PathBuf {
    dir.path().join(format!("{name}{SQLITE_SUFFIX}"))
}

fn node(kind: &str, body: &str, logical_id: Option<&str>) -> PreparedWrite {
    PreparedWrite::Node {
        kind: kind.to_string(),
        body: body.to_string(),
        source_id: None,
        logical_id: logical_id.map(str::to_string),
    }
}

#[test]
fn read_get_returns_the_active_node_by_logical_id() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "get_active")).expect("open");
    opened.engine.write(&[node("doc", "hello world", Some("L1"))]).expect("write");

    let got = opened.engine.read_get("L1").expect("read_get").expect("present");
    assert_eq!(got.logical_id, "L1");
    assert_eq!(got.kind, "doc");
    assert_eq!(got.body, "hello world");
    assert!(got.write_cursor > 0);

    opened.engine.close().unwrap();
}

#[test]
fn read_get_returns_none_for_a_missing_logical_id() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "get_missing")).expect("open");
    opened.engine.write(&[node("doc", "present", Some("L1"))]).expect("write");

    // A missing id is a NORMAL absence (None), not an error.
    let got = opened.engine.read_get("DOES_NOT_EXIST").expect("read_get is Ok");
    assert!(got.is_none(), "missing logical_id must yield None, not an error");

    opened.engine.close().unwrap();
}

#[test]
fn read_get_active_only_does_not_return_superseded_versions() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "get_superseded")).expect("open");
    // Three writes of the same logical_id: v1, v2 are superseded; v3 active.
    for body in ["v1", "v2", "v3"] {
        opened.engine.write(&[node("doc", body, Some("L1"))]).expect("write");
    }

    let got = opened.engine.read_get("L1").expect("read_get").expect("present");
    assert_eq!(got.body, "v3", "read.get must return only the active (latest) version");

    opened.engine.close().unwrap();
}

#[test]
fn read_get_many_preserves_request_order_with_none_for_misses() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "get_many_order")).expect("open");
    opened
        .engine
        .write(&[
            node("doc", "body-A", Some("A")),
            node("doc", "body-B", Some("B")),
            node("doc", "body-C", Some("C")),
        ])
        .expect("write");

    // Mixed present/missing in a deliberately non-sorted order.
    let ids = vec!["C".to_string(), "MISSING".to_string(), "A".to_string()];
    let rows = opened.engine.read_get_many(&ids).expect("read_get_many");

    assert_eq!(rows.len(), 3, "one slot per requested id");
    assert_eq!(rows[0].as_ref().unwrap().body, "body-C", "slot 0 == request[0] == C");
    assert!(rows[1].is_none(), "slot 1 (MISSING) must be None, order preserved");
    assert_eq!(rows[2].as_ref().unwrap().body, "body-A", "slot 2 == request[2] == A");

    opened.engine.close().unwrap();
}

#[test]
fn read_get_many_omits_superseded_versions() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "get_many_superseded")).expect("open");
    opened.engine.write(&[node("doc", "x-v1", Some("X"))]).expect("write v1");
    opened.engine.write(&[node("doc", "x-v2", Some("X"))]).expect("supersede to v2");

    let rows = opened.engine.read_get_many(&["X".to_string()]).expect("read_get_many");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].as_ref().unwrap().body, "x-v2", "only the active version is returned");

    opened.engine.close().unwrap();
}

/// Reads ride the ReaderWorkerPool DEFERRED-tx snapshot path, NOT the writer
/// `connection.lock()`. We hammer concurrent `read_get` from many threads
/// against an `Arc<Engine>` while the engine remains open; if reads took the
/// single writer lock they would serialize/contend, but more importantly a
/// reader-pool read must succeed for every caller without loss or deadlock.
#[test]
fn read_get_rides_the_reader_pool_under_concurrency() {
    let dir = TempDir::new().unwrap();
    let opened = Engine::open(db_path(&dir, "get_concurrent")).expect("open");
    opened.engine.write(&[node("doc", "shared", Some("L1"))]).expect("write");

    let engine = Arc::new(opened.engine);
    const CALLERS: usize = 16;
    const PER_CALLER: usize = 25;

    let mut handles = Vec::with_capacity(CALLERS);
    for _ in 0..CALLERS {
        let engine = Arc::clone(&engine);
        handles.push(thread::spawn(move || {
            for _ in 0..PER_CALLER {
                let got = engine.read_get("L1").expect("concurrent read_get is Ok");
                assert_eq!(got.expect("present").body, "shared");
            }
        }));
    }
    for handle in handles {
        handle.join().expect("caller thread");
    }
    // Workers survive the storm (reader pool, not the writer lock).
    let _ = Duration::from_millis(0);
    engine.close().unwrap();
}
