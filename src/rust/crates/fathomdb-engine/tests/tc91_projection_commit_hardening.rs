//! TC-91 regression coverage for projection-worker terminal commits.

use fathomdb_embedder::EmbedderEvent;
use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::lifecycle::{Event, EventCategory, EventSource, Phase, Subscriber};
use fathomdb_engine::{Engine, InitialState, PreparedWrite, SourceId, MEAN_VEC_PIN_THRESHOLD};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use tempfile::TempDir;

#[derive(Default)]
struct EventSink {
    events: Mutex<Vec<Event>>,
}

impl Subscriber for EventSink {
    fn on_event(&self, event: &Event) {
        self.events.lock().expect("event sink lock").push(event.clone());
    }
}

struct CountingEmbedder {
    calls: Arc<AtomicUsize>,
}

struct PanicOnceEmbedder {
    panicked: AtomicBool,
    calls: Arc<AtomicUsize>,
}

struct MeanCenteringEmbedder;

impl Embedder for MeanCenteringEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new("fathomdb-bge-small-en-v1.5", "tc91", 384)
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        Ok(vec![1.0; 384])
    }
}

impl Embedder for PanicOnceEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new("tc91-panic-commit", "v1", 384)
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(self.panicked.swap(true, Ordering::SeqCst), "intentional first projection panic");
        Ok(vec![1.0; 384])
    }
}

impl Embedder for CountingEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        EmbedderIdentity::new("tc91-commit", "v1", 384)
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![1.0; 384])
    }
}

fn node() -> PreparedWrite {
    numbered_node(0)
}

fn numbered_node(index: usize) -> PreparedWrite {
    PreparedWrite::Node {
        kind: "doc".to_string(),
        body: format!(r#"{{"summary":"projection commit rollback {index}"}}"#),
        source_id: SourceId::new("test:tc91-projection-commit").expect("source id"),
        logical_id: Some(format!("tc91-projection-commit-{index}")),
        state: InitialState::Active,
        reason: None,
        valid_from: None,
        valid_until: None,
    }
}

/// A failed projection commit is observable and leaves its source row pending;
/// state cleanup then redispatches it rather than turning it into an embed failure.
#[test]
fn tc91_projection_commit_busy_is_reported_and_redispatched() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("tc91-projection-commit.db");
    let calls = Arc::new(AtomicUsize::new(0));
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(CountingEmbedder { calls: Arc::clone(&calls) }),
    )
    .expect("open");
    let sink = Arc::new(EventSink::default());
    let _subscription = opened.engine.subscribe(Arc::clone(&sink) as Arc<dyn Subscriber>);
    opened.engine.configure_vector_kind_for_test("doc").expect("configure vector kind");

    opened.engine.force_next_projection_commit_failure_for_test();
    let receipt = opened.engine.write(&[node()]).expect("caller write remains durable");
    opened.engine.drain(10_000).expect("redispatch reaches idle");

    assert!(opened.engine.has_vector_for_cursor_for_test(receipt.cursor).expect("vector state"));
    assert_eq!(
        opened.engine.projection_failure_count_for_test(receipt.cursor).expect("failure audit"),
        0,
        "a worker storage failure is not an exhausted embed failure"
    );
    assert!(
        calls.load(Ordering::SeqCst) >= 2,
        "the failed commit leaves the durable row pending for redispatch"
    );
    assert!(
        sink.events.lock().expect("event sink lock").iter().any(|event| {
            event.phase == Phase::Failed
                && event.source == EventSource::SqliteInternal
                && event.category == EventCategory::Error
                && event.code == Some("SQLITE_BUSY")
        }),
        "the commit error must reach the lifecycle subscriber"
    );
}

/// A non-SQLite rusqlite error is an engine storage diagnostic, not a synthetic
/// SQLite event, and it preserves the same non-terminal redispatch semantics.
#[test]
fn tc91_projection_commit_storage_error_is_reported_and_redispatched() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("tc91-projection-storage.db");
    let calls = Arc::new(AtomicUsize::new(0));
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(CountingEmbedder { calls: Arc::clone(&calls) }),
    )
    .expect("open");
    let sink = Arc::new(EventSink::default());
    let _subscription = opened.engine.subscribe(Arc::clone(&sink) as Arc<dyn Subscriber>);
    opened.engine.configure_vector_kind_for_test("doc").expect("configure vector kind");

    opened.engine.force_next_projection_storage_failure_for_test();
    let receipt = opened.engine.write(&[node()]).expect("caller write remains durable");
    opened.engine.drain(10_000).expect("redispatch reaches idle");

    assert!(opened.engine.has_vector_for_cursor_for_test(receipt.cursor).expect("vector state"));
    assert_eq!(
        opened.engine.projection_failure_count_for_test(receipt.cursor).expect("failure audit"),
        0
    );
    assert!(calls.load(Ordering::SeqCst) >= 2, "the failed commit must be redispatched");
    let events = sink.events.lock().expect("event sink lock");
    assert!(events.iter().any(|event| {
        event.phase == Phase::Failed
            && event.source == EventSource::Engine
            && event.category == EventCategory::Error
            && event.code == Some("StorageError")
    }));
    assert!(
        !events.iter().any(|event| event.source == EventSource::SqliteInternal),
        "non-SQLite failures must not be mislabeled as SQLite internals"
    );
}

/// If the worker panics, its existing `ProjectionPanic` terminal is still
/// durable only on a successful commit. A failed panic-terminal commit is
/// observed and later retried normally rather than silently terminalized.
#[test]
fn tc91_panic_terminal_commit_failure_is_reported_and_redispatched() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("tc91-projection-panic.db");
    let calls = Arc::new(AtomicUsize::new(0));
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(PanicOnceEmbedder { panicked: AtomicBool::new(false), calls: Arc::clone(&calls) }),
    )
    .expect("open");
    let sink = Arc::new(EventSink::default());
    let _subscription = opened.engine.subscribe(Arc::clone(&sink) as Arc<dyn Subscriber>);
    opened.engine.configure_vector_kind_for_test("doc").expect("configure vector kind");

    opened.engine.force_next_projection_commit_failure_for_test();
    let receipt = opened.engine.write(&[node()]).expect("caller write remains durable");
    opened.engine.drain(10_000).expect("normal redispatch reaches idle");

    assert!(opened.engine.has_vector_for_cursor_for_test(receipt.cursor).expect("vector state"));
    assert_eq!(
        opened.engine.projection_failure_count_for_test(receipt.cursor).expect("failure audit"),
        0,
        "the failed ProjectionPanic terminal must not become durable"
    );
    assert!(calls.load(Ordering::SeqCst) >= 2, "the non-terminal row must retry normally");
    assert!(sink.events.lock().expect("event sink lock").iter().any(|event| {
        event.phase == Phase::Failed
            && event.source == EventSource::SqliteInternal
            && event.category == EventCategory::Error
            && event.code == Some("SQLITE_BUSY")
    }));
}

/// The failed threshold-crossing transaction must not consume the shared mean
/// accumulator. Its redispatch is the one commit that pins and emits the event.
#[test]
fn tc91_projection_commit_failure_at_mean_pin_is_rollback_safe() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("tc91-projection-mean.db");
    let opened =
        Engine::open_with_embedder_for_test(&path, Arc::new(MeanCenteringEmbedder)).expect("open");
    opened.engine.configure_vector_kind_for_test("doc").expect("configure vector kind");

    let before_threshold: Vec<PreparedWrite> =
        (0..(MEAN_VEC_PIN_THRESHOLD as usize - 1)).map(numbered_node).collect();
    opened.engine.write(&before_threshold).expect("pre-threshold write");
    opened.engine.drain(20_000).expect("pre-threshold drain");
    assert!(
        opened.engine.drain_mean_centering_events_for_test().expect("event drain").is_empty(),
        "the accumulator must not pin before its threshold"
    );

    opened.engine.force_next_projection_commit_failure_for_test();
    opened
        .engine
        .write(&[numbered_node(MEAN_VEC_PIN_THRESHOLD as usize)])
        .expect("threshold write");
    opened.engine.drain(20_000).expect("threshold redispatch drain");

    let pin_events = opened
        .engine
        .drain_mean_centering_events_for_test()
        .expect("event drain")
        .into_iter()
        .filter(|event| matches!(event, EmbedderEvent::MeanVecPinned { .. }))
        .count();
    assert_eq!(
        pin_events, 1,
        "a rolled-back threshold candidate must not consume or double-publish the pin"
    );
}

/// A failed worker commit remains recoverable when shutdown wins the race before
/// cleanup: reopen derives work from the durable missing terminal, not an
/// in-memory retry entry.
#[test]
fn tc91_projection_commit_failure_survives_stop_and_reopen() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("tc91-projection-reopen.db");
    let calls = Arc::new(AtomicUsize::new(0));
    let opened = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(CountingEmbedder { calls: Arc::clone(&calls) }),
    )
    .expect("open");
    let engine = &opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("configure vector kind");
    let reported = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let stopping = Arc::new(Barrier::new(2));
    engine.pause_projection_commit_failure_cleanup_for_test(
        Arc::clone(&reported),
        Arc::clone(&release),
    );
    engine.acknowledge_projection_stop_for_test(Arc::clone(&stopping));
    engine.force_next_projection_commit_failure_for_test();
    let receipt = engine.write(&[node()]).expect("caller write remains durable");
    reported.wait();

    std::thread::scope(|scope| {
        scope.spawn(|| engine.close().expect("close"));
        stopping.wait();
        release.wait();
    });
    drop(opened);

    let recovered = Engine::open_with_embedder_for_test(
        &path,
        Arc::new(CountingEmbedder { calls: Arc::clone(&calls) }),
    )
    .expect("reopen");
    recovered.engine.drain(10_000).expect("reopen redispatch reaches idle");
    assert!(
        recovered.engine.has_vector_for_cursor_for_test(receipt.cursor).expect("vector state"),
        "the pending canonical row must be reconstructed and committed after reopen"
    );
    assert_eq!(
        recovered.engine.projection_failure_count_for_test(receipt.cursor).expect("failure audit"),
        0
    );
}
