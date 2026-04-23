# Additional Stress Tests

## Summary

Add two new observability-focused stress tests across the real public
surfaces:

1. `telemetry_snapshot_is_monotonic_under_load`
2. `observability_feedback_remains_live_under_load`

The implementation must cover:

- Rust engine integration
- Python core public API
- Python harness/SDK wrapper layer
- TypeScript SDK harness layer

These tests extend existing stress coverage. Existing stress tests should
remain behaviorally unchanged except for small helper extraction if
necessary.

## Goals

- Verify telemetry snapshots remain safe and monotonic under concurrent load.
- Verify feedback and tracing surfaces remain live under concurrent load.
- Keep all observability stress coverage in weekly robustness suites rather
  than per-PR jobs.
- Preserve current public APIs.

## Telemetry Stress Design

### Test name

`telemetry_snapshot_is_monotonic_under_load`

### Shared behavior

- Open the engine with telemetry counters explicitly enabled.
- Seed initial data before starting concurrent work.
- Run concurrent readers and writers plus one dedicated telemetry sampler.
- Record sampled telemetry snapshots throughout the run.
- Stop after a configurable duration.
- Run a final integrity check.

### Shared assertions

- No worker errors.
- No thread or task hangs.
- At least one read, one write, and multiple telemetry samples complete.
- All sampled counters are monotonic nondecreasing:
  - queries
  - writes
  - written rows
  - errors
  - admin ops
- Cache fields stay non-negative and show activity, but are not asserted
  monotonic under concurrent sampling.
- Final snapshot shows reads and writes occurred.
- Final snapshot shows zero errors.
- Final integrity check passes.

### Surface placement

- Rust: `crates/fathomdb/tests/scale.rs`
- Python core: `python/tests/test_stress.py`
- Python harness/SDK: `python/tests/examples/`
- TypeScript SDK: `typescript/apps/sdk-harness/src/scenarios/`

### Surface-specific decisions

#### Rust

- Implement as an ignored weekly stress test in `scale.rs`.
- Reuse the current `Arc<Engine>` concurrent workload pattern.
- Sample `engine.telemetry_snapshot()` from a dedicated thread.

#### Python core

- Extend `python/tests/test_stress.py`.
- Sample `engine.telemetry_snapshot()` from a dedicated thread.
- Reuse the existing write helper and duration env var.

#### Python harness/SDK

- Use `examples.harness.engine_factory.open_engine(...)` so the wrapper path
  is exercised.
- Keep these as separate tests under `python/tests/examples/` rather than
  adding long-running scenarios to the normal harness mode count.

#### TypeScript SDK

- Implement as real integration scenarios in the SDK harness.
- Use repeated real SDK operations in the harness mode to exercise the native
  binding and feedback plumbing.
- One scenario samples `telemetrySnapshot()` across repeated writes and reads.
- The observability harness test must fail hard if the native binding is not
  available.
- The harness test stages a real `.node` artifact by copying the built Rust
  library (`libfathomdb.so` or `.dylib`) to a temporary `fathomdb.node` path
  and setting `FATHOMDB_NATIVE_BINDING`.
- Return a normal harness result with detail text so the harness summary shows
  the observed totals.

### TypeScript non-negotiable rule

Do not implement the stress tests in the mocked package unit tests under
`typescript/packages/fathomdb/test/`. The real home is the SDK harness
because the test must exercise the native binding.

## Feedback And Tracing Stress Design

### Test name

`observability_feedback_remains_live_under_load`

### Shared behavior

- Use aggressive feedback timing where the surface supports it.
- Attach callbacks to writes, reads, and admin operations.
- Record events by operation id.
- Verify event ordering and callback suppression behavior.
- Run a final integrity check.

### Shared assertions

- No worker errors.
- No callback-induced deadlocks.
- Many operations emit lifecycle events.
- For each observed operation id:
  - first event is `started`
  - last event is `finished` or `failed`
  - elapsed time is nondecreasing
- A callback that throws once does not fail the underlying operation, and
  later events for that operation are suppressed.
- Final integrity check passes.

### Surface placement

- Rust tracing load test: `crates/fathomdb-engine/tests/tracing_events.rs`
- Python core feedback stress: `python/tests/test_stress.py`
- Python harness/SDK feedback stress: `python/tests/examples/`
- TypeScript SDK feedback stress: `typescript/apps/sdk-harness/src/scenarios/`

### Surface-specific decisions

#### Rust

- Implement as a tracing-under-load test using the existing `tracing`
  subscriber capture path.
- This is the Rust observability equivalent for feedback coverage.
- Do not invent a new response-cycle callback abstraction for Rust.

#### Python core

- Run concurrent writes, reads, `check_integrity`, and `trace_source` with
  `progress_callback` and short `FeedbackConfig`.
- Require `started` and `finished`, plus at least some `slow` or `heartbeat`
  events across the run.

#### Python harness/SDK

- Repeat the same idea through the telemetry wrapper layer to prove the
  harness path forwards feedback correctly under load.

#### TypeScript SDK

- Only require `started` and `finished` or `failed`.
- Do not require `slow` or `heartbeat`.
- This matches the current synchronous behavior documented in
  `typescript/packages/fathomdb/src/feedback.ts`.

## Duration And Execution Model

### Defaults

- Local default duration: 5 seconds
- Weekly CI duration: 60 seconds

### Environment variables

- Python core and harness: reuse `FATHOM_PY_STRESS_DURATION_SECONDS`
- Rust scale tests: reuse `FATHOM_RUST_STRESS_DURATION_SECONDS`
- Rust tracing load test: add `FATHOM_RUST_TRACING_STRESS_DURATION_SECONDS`
- TypeScript SDK harness: add `FATHOM_TS_STRESS_DURATION_SECONDS`

## Non-goals

- No production API changes
- No benchmark-grade reporting or threshold enforcement on success
- Success-path test summaries are allowed and now emitted so weekly runs show
  the observed counts directly
- No attempt to make TypeScript emit `slow` or `heartbeat` before async
  bindings exist
