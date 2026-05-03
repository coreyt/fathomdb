//! Surface-level assertions for engine-attached instrumentation methods
//! pinned by `dev/interfaces/rust.md` § Engine-attached instrumentation.

use std::sync::Arc;

use fathomdb_engine::lifecycle::{Event, Subscriber};
use fathomdb_engine::{CounterSnapshot, Engine, Subscription};
use tempfile::TempDir;

struct NoopSubscriber;

impl Subscriber for NoopSubscriber {
    fn on_event(&self, _event: &Event) {}
}

fn fixture() -> (TempDir, Engine) {
    let dir = TempDir::new().unwrap();
    let engine =
        Engine::open(dir.path().join("instrumentation.sqlite")).expect("engine open").engine;
    (dir, engine)
}

#[test]
fn drain_returns_ok_on_open_engine() {
    let (_dir, engine) = fixture();
    engine.drain(0).expect("drain stub returns Ok");
}

#[test]
fn counters_returns_snapshot_carrier() {
    let (_dir, engine) = fixture();
    let _: CounterSnapshot = engine.counters();
}

#[test]
fn set_profiling_accepts_bool() {
    let (_dir, engine) = fixture();
    engine.set_profiling(true).expect("set_profiling stub");
    engine.set_profiling(false).expect("set_profiling stub");
}

#[test]
fn set_slow_threshold_accepts_u64() {
    let (_dir, engine) = fixture();
    engine.set_slow_threshold_ms(0).expect("set_slow_threshold_ms stub");
    engine.set_slow_threshold_ms(1_000).expect("set_slow_threshold_ms stub");
}

#[test]
fn subscribe_returns_subscription_carrier() {
    let (_dir, engine) = fixture();
    let _: Subscription = engine.subscribe(Arc::new(NoopSubscriber));
}
