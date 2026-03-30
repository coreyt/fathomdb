#![cfg(feature = "tracing")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

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
    let (subscriber, messages) = captured_subscriber();
    let _ = tracing::subscriber::set_global_default(subscriber);

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    let engine = fathomdb_engine::EngineRuntime::open(
        &db_path,
        fathomdb_engine::ProvenanceMode::Warn,
        None,
        2,
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
