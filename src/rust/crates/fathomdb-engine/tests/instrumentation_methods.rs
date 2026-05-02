//! Surface-level assertions for engine-attached instrumentation methods
//! pinned by `dev/interfaces/rust.md` § Engine-attached instrumentation.

use fathomdb_engine::{CounterSnapshot, Engine, Subscription};

fn fixture() -> Engine {
    Engine::open("instrumentation.sqlite").expect("scaffold open").engine
}

#[test]
fn drain_returns_ok_on_open_engine() {
    let engine = fixture();
    engine.drain(0).expect("drain stub returns Ok");
}

#[test]
fn counters_returns_snapshot_carrier() {
    let engine = fixture();
    let _: CounterSnapshot = engine.counters();
}

#[test]
fn set_profiling_accepts_bool() {
    let engine = fixture();
    engine.set_profiling(true).expect("set_profiling stub");
    engine.set_profiling(false).expect("set_profiling stub");
}

#[test]
fn set_slow_threshold_accepts_u64() {
    let engine = fixture();
    engine.set_slow_threshold_ms(0).expect("set_slow_threshold_ms stub");
    engine.set_slow_threshold_ms(1_000).expect("set_slow_threshold_ms stub");
}

#[test]
fn subscribe_returns_subscription_carrier() {
    let engine = fixture();
    let _: Subscription = engine.subscribe();
}
