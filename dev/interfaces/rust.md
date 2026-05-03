---
title: Rust Public Interface
date: 2026-04-24
target_release: 0.6.0
desc: Public Rust surface (traits, functions, types, errors) for 0.6.0
blast_radius: src/rust/crates/fathomdb; design/engine.md; design/bindings.md; design/errors.md; design/lifecycle.md
status: locked
---

# Rust Interface

This file owns Rust-visible symbol spelling and result shape. Cross-binding
parity rules remain owned by `design/bindings.md`.

## Support posture

The Rust facade is stable public Rust contract in 0.6.0 and is the
ground-truth source for engine-side type names. It is not part of the
Python/TypeScript five-verb SDK parity set tested by AC-057a; Rust keeps the
facade shape below unless a successor ADR expands it.

## Public surface

Rust exposes:

- `Engine::open(...) -> Result<OpenedEngine, EngineOpenError>`
- `Engine::write(...) -> Result<WriteReceipt, EngineError>`
- `Engine::search(...) -> Result<SearchResult, EngineError>`
- `Engine::close(...) -> Result<(), EngineError>`

`OpenedEngine` contains:

- `engine`
- `report`

`report` is the `OpenReport` owned by `design/engine.md`.

## Engine-attached instrumentation / control methods

These are public instance methods, not extra top-level SDK verbs:

- `Engine::drain(timeout_ms: u64) -> Result<(), EngineError>`
- `Engine::counters() -> CounterSnapshot`
- `Engine::set_profiling(enabled: bool) -> Result<(), EngineError>`
- `Engine::set_slow_threshold_ms(value: u64) -> Result<(), EngineError>`
- `Engine::subscribe(&self, subscriber: Arc<dyn lifecycle::Subscriber>) -> Subscription`

`subscribe` owns host-subscriber attachment and may carry heartbeat-cadence
options. The payload semantics remain owned by `design/lifecycle.md` and
`design/migrations.md`.

## Caller-visible data shapes

- `WriteReceipt` has exactly one public field: `cursor`
- `SearchResult` exposes `projection_cursor`
- hybrid fallback, when present, exposes a typed branch enum whose values are
  owned by `design/retrieval.md`
- counter/profile/stress payload shapes are owned by `design/lifecycle.md`

## Errors

Rust exposes typed open/runtime errors without message parsing:

- `EngineOpenError`
- `EngineError`

Canonical leaf mapping lives in `design/errors.md`. This file adopts those
types without renaming them.

## Non-presence

The Rust runtime surface does not expose recovery verbs. Recovery remains CLI
only per `design/recovery.md` and `design/bindings.md`.
