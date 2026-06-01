//! PR-9 item 1 — engine-side embed serialization.
//!
//! The projection pool runs `PROJECTION_WORKERS` (2) workers. PR-9 serializes
//! the embed *call* engine-side so the shared `Arc<dyn Embedder>` is invoked
//! by at most one worker at a time, while commit/IO stays parallel.
//!
//! Rationale is SAFETY, not throughput: the engine accepts arbitrary
//! caller-supplied embedders (pyo3 / napi bridges) that are `Sync` only by
//! contract and may not be truly concurrency-safe; serializing engine-side
//! makes the projection robust to them. (Throughput is ~neutral — candle
//! fans every forward onto one process-wide rayon pool, so concurrent
//! forwards share it rather than getting 2x; the PR-9 pre-flight also
//! confirmed concurrent CandleBge embeds neither wedge nor corrupt.)
//!
//! This test proves serialization mechanically with a fast mock embedder
//! that records the maximum number of `embed()` calls in flight at once.
//!
//! RED (no guard): two workers embed concurrently → max in flight == 2.
//! GREEN (guard):  embeds run one at a time   → max in flight == 1.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use fathomdb_embedder_api::{Embedder, EmbedderError, EmbedderIdentity, Vector};
use fathomdb_engine::lifecycle::ProjectionStatus;
use fathomdb_engine::{Engine, PreparedWrite};
use fathomdb_schema::SQLITE_SUFFIX;
use tempfile::TempDir;

const DIM: u32 = 8;

fn unit_vector() -> Vector {
    let mut v = vec![0.0_f32; DIM as usize];
    v[0] = 1.0;
    v
}

/// Records peak concurrency of `embed()`. Each call bumps an in-flight
/// counter, holds it for `delay` (so overlapping calls are observable),
/// and updates the shared `max_in_flight` high-water mark.
#[derive(Debug)]
struct ConcurrencyProbeEmbedder {
    identity: EmbedderIdentity,
    in_flight: AtomicUsize,
    max_in_flight: Arc<AtomicUsize>,
    delay: Duration,
}

impl ConcurrencyProbeEmbedder {
    fn new(max_in_flight: Arc<AtomicUsize>, delay: Duration) -> Self {
        Self {
            identity: EmbedderIdentity::new("probe", "rev-a", DIM),
            in_flight: AtomicUsize::new(0),
            max_in_flight,
            delay,
        }
    }
}

impl Embedder for ConcurrencyProbeEmbedder {
    fn identity(&self) -> EmbedderIdentity {
        self.identity.clone()
    }

    fn embed(&self, _text: &str) -> Result<Vector, EmbedderError> {
        let cur = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_in_flight.fetch_max(cur, Ordering::SeqCst);
        thread::sleep(self.delay);
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        Ok(unit_vector())
    }
}

fn fixture_path(name: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join(format!("{name}{SQLITE_SUFFIX}"));
    (dir, path)
}

/// Item 1: the production projection path must embed one-at-a-time. With two
/// projection workers and a deliberately-slow embed, an unserialized engine
/// would show two concurrent `embed()` calls; the engine-side guard pins the
/// peak at exactly one.
#[test]
fn embeds_are_serialized_engine_side() {
    let (_dir, path) = fixture_path("pr9_serialize");
    let max_in_flight = Arc::new(AtomicUsize::new(0));
    let embedder =
        Arc::new(ConcurrencyProbeEmbedder::new(max_in_flight.clone(), Duration::from_millis(50)));
    let opened = Engine::open_with_embedder_for_test(&path, embedder).expect("open");
    let engine = opened.engine;
    engine.configure_vector_kind_for_test("doc").expect("vector kind");

    // A worker grabs a whole PROJECTION_COMMIT_BATCH (16) at once, and the
    // dispatcher enqueues up to PROJECTION_INFLIGHT_LIMIT (32) per scan — so
    // we write > one batch (40 docs) to ensure both workers pick up work and
    // would embed concurrently if the engine permitted it.
    let nodes: Vec<PreparedWrite> = (0..40)
        .map(|i| PreparedWrite::Node {
            kind: "doc".to_string(),
            body: format!("serialize-doc-{i}"),
            source_id: None,
        })
        .collect();
    engine.write(&nodes).expect("write");
    engine.drain(30_000).expect("drain");
    assert_eq!(
        engine.projection_status_for_test("doc").expect("status"),
        ProjectionStatus::UpToDate,
        "all docs must project successfully"
    );

    let peak = max_in_flight.load(Ordering::SeqCst);
    assert_eq!(
        peak, 1,
        "embeds must run one at a time engine-side (PR-9 oversubscription fix); \
         observed {peak} concurrent embed() calls"
    );
}
