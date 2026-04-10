#![cfg(feature = "tracing")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tracing_subscriber::layer::SubscriberExt;

/// Captures tracing event messages into a shared `Vec<String>`.
struct CaptureLayer {
    messages: Arc<Mutex<Vec<String>>>,
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);
        self.messages.lock().unwrap().push(visitor.0);
    }
}

struct MessageVisitor(String);

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

fn captured_subscriber() -> (impl tracing::Subscriber, Arc<Mutex<Vec<String>>>) {
    let messages: Arc<Mutex<Vec<String>>> = Arc::default();
    let layer = CaptureLayer {
        messages: Arc::clone(&messages),
    };
    let subscriber = tracing_subscriber::registry().with(layer);
    (subscriber, messages)
}

fn global_messages() -> Arc<Mutex<Vec<String>>> {
    static GLOBAL_MESSAGES: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    let messages = GLOBAL_MESSAGES.get_or_init(|| {
        let messages: Arc<Mutex<Vec<String>>> = Arc::default();
        let layer = CaptureLayer {
            messages: Arc::clone(&messages),
        };
        let subscriber = tracing_subscriber::registry().with(layer);
        let _ = tracing::subscriber::set_global_default(subscriber);
        messages
    });
    Arc::clone(messages)
}

fn tracing_stress_duration() -> Duration {
    let seconds = std::env::var("FATHOM_RUST_TRACING_STRESS_DURATION_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1);
    Duration::from_secs(seconds)
}

#[allow(clippy::print_stderr)]
fn emit_success_summary(name: &str, metrics: &[(&str, String)]) {
    let rendered = metrics
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!("{name}: {rendered}");
}

fn spawn_tracing_load_workers(
    engine: Arc<fathomdb_engine::EngineRuntime>,
    errors: Arc<Mutex<Vec<String>>>,
) -> Vec<thread::JoinHandle<()>> {
    let mut handles = Vec::new();
    for thread_id in 0..4 {
        let engine = Arc::clone(&engine);
        let errors = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let deadline = std::time::Instant::now() + tracing_stress_duration();
            let mut iteration = 0usize;
            while std::time::Instant::now() < deadline {
                let request = fathomdb_engine::WriteRequest {
                    label: format!("trace-load-{thread_id}-{iteration}"),
                    nodes: vec![fathomdb_engine::NodeInsert {
                        row_id: fathomdb_engine::new_row_id(),
                        logical_id: format!("trace-doc-{thread_id}-{iteration}"),
                        kind: "note".to_owned(),
                        properties: "{}".to_owned(),
                        source_ref: Some(format!("trace-src-{thread_id}")),
                        upsert: false,
                        chunk_policy: fathomdb_engine::ChunkPolicy::Preserve,
                    }],
                    edges: vec![],
                    chunks: vec![],
                    node_retires: vec![],
                    edge_retires: vec![],
                    runs: vec![],
                    steps: vec![],
                    actions: vec![],
                    vec_inserts: vec![],
                    operational_writes: vec![],
                    optional_backfills: vec![],
                };
                if let Err(err) = engine.writer().submit(request) {
                    errors
                        .lock()
                        .unwrap()
                        .push(format!("writer[{thread_id}]: {err}"));
                    break;
                }
                iteration += 1;
            }
        }));
    }
    handles
}

fn wait_for_tracing_load(handles: Vec<thread::JoinHandle<()>>, errors: Arc<Mutex<Vec<String>>>) {
    for handle in handles {
        if handle.join().is_err() {
            errors
                .lock()
                .unwrap()
                .push("writer thread panicked".to_owned());
        }
    }
}

#[test]
fn engine_open_emits_lifecycle_events() {
    let (subscriber, messages) = captured_subscriber();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    tracing::subscriber::with_default(subscriber, || {
        let _engine = fathomdb_engine::EngineRuntime::open(
            &db_path,
            fathomdb_engine::ProvenanceMode::Warn,
            None,
            2,
            fathomdb_engine::TelemetryLevel::Counters,
        )
        .unwrap();
    });

    let msgs = messages.lock().unwrap();
    assert!(
        msgs.iter().any(|m| m.contains("engine open")),
        "expected 'engine open' event, got: {msgs:?}"
    );
}

/// Uses `set_global_default` because the writer thread is a separate OS thread
/// that does not inherit the test thread's scoped subscriber.
/// This works with nextest because each test runs in its own process.
#[test]
fn write_committed_emits_info_event() {
    let messages = global_messages();
    messages.lock().unwrap().clear();

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let engine = fathomdb_engine::EngineRuntime::open(
        &db_path,
        fathomdb_engine::ProvenanceMode::Warn,
        None,
        2,
        fathomdb_engine::TelemetryLevel::Counters,
    )
    .unwrap();

    let request = fathomdb_engine::WriteRequest {
        label: "test-write".to_owned(),
        nodes: vec![fathomdb_engine::NodeInsert {
            row_id: fathomdb_engine::new_row_id(),
            logical_id: fathomdb_engine::new_id(),
            kind: "note".to_owned(),
            properties: "{}".to_owned(),
            source_ref: Some("test".to_owned()),
            upsert: false,
            chunk_policy: fathomdb_engine::ChunkPolicy::Preserve,
        }],
        edges: vec![],
        chunks: vec![],
        node_retires: vec![],
        edge_retires: vec![],
        runs: vec![],
        steps: vec![],
        actions: vec![],
        vec_inserts: vec![],
        operational_writes: vec![],
        optional_backfills: vec![],
    };
    let _receipt = engine.writer().submit(request).unwrap();

    // Give the writer thread time to emit the event.
    drop(engine);

    let msgs = messages.lock().unwrap();
    assert!(
        msgs.iter().any(|m| m.contains("write committed")),
        "expected 'write committed' event, got: {msgs:?}"
    );
}

#[test]
fn tracing_events_continue_under_concurrent_load() {
    let duration = tracing_stress_duration();
    let messages = global_messages();
    messages.lock().unwrap().clear();

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let engine = Arc::new(
        fathomdb_engine::EngineRuntime::open(
            &db_path,
            fathomdb_engine::ProvenanceMode::Warn,
            None,
            4,
            fathomdb_engine::TelemetryLevel::Counters,
        )
        .unwrap(),
    );

    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let handles = spawn_tracing_load_workers(Arc::clone(&engine), Arc::clone(&errors));
    wait_for_tracing_load(handles, Arc::clone(&errors));

    let _ = engine.admin().service().check_integrity().unwrap();
    drop(engine);

    let errors = errors.lock().unwrap();
    assert!(
        errors.is_empty(),
        "errors during tracing load test: {errors:?}"
    );

    let msgs = messages.lock().unwrap();
    let engine_open_events = msgs.iter().filter(|m| m.contains("engine open")).count();
    let write_committed_events = msgs
        .iter()
        .filter(|m| m.contains("write committed"))
        .count();

    assert!(
        engine_open_events >= 1,
        "expected engine open events, got: {msgs:?}"
    );
    assert!(
        write_committed_events >= 5,
        "expected repeated write committed events, got: {msgs:?}"
    );

    emit_success_summary(
        "rust_tracing_stress",
        &[
            ("duration_seconds", duration.as_secs().to_string()),
            ("engine_open_events", engine_open_events.to_string()),
            ("write_committed_events", write_committed_events.to_string()),
            ("captured_messages", msgs.len().to_string()),
        ],
    );
}
