# Additional Stress Tests Plan

## Summary

Implement two new observability stress tests with strict TDD:

1. `telemetry_snapshot_is_monotonic_under_load`
2. `observability_feedback_remains_live_under_load`

The implementation covers:

- Rust engine integration
- Python core public API
- Python harness/SDK wrapper layer
- TypeScript SDK harness layer

## TDD Sequence

For each surface, follow this order:

1. Add failing test code only.
2. Run only the new target and capture the failure.
3. Implement the minimum code or helper needed to pass.
4. Re-run the new target.
5. Re-run the surrounding stress or harness target.
6. Only after tests pass, update weekly CI wiring if needed.
7. Finish with a cross-surface validation pass.

Required execution order:

1. Rust scale telemetry test
2. Rust tracing load test
3. Python core telemetry test
4. Python core feedback stress test
5. Python harness telemetry test
6. Python harness feedback stress test
7. TypeScript harness telemetry scenario
8. TypeScript harness feedback scenario
9. CI wiring

## Planned Code Changes

### Rust

- Extend `crates/fathomdb/tests/scale.rs` with ignored telemetry monotonic
  stress coverage.
- Extend `crates/fathomdb-engine/tests/tracing_events.rs` with a tracing
  load test under the `tracing` feature.
- Keep helpers local unless helper extraction is necessary.

### Python core

- Extend `python/tests/test_stress.py` with:
  - telemetry monotonic stress test
  - feedback liveness stress test
- Reuse the existing write and duration helpers.
- Add small helpers for snapshot collection and event bookkeeping.

### Python harness/SDK

- Add a dedicated observability stress module under `python/tests/examples/`.
- Use `examples.harness.engine_factory.open_engine(...)` so the wrapper path
  is exercised.
- Keep these stress tests separate from the normal baseline harness scenario
  count.

### TypeScript SDK

- Add two observability scenarios in
  `typescript/apps/sdk-harness/src/scenarios/`:
  - telemetry stress
  - feedback lifecycle stress
- Update `typescript/apps/sdk-harness/src/app.ts` to expose a new
  `observability` mode rather than changing `baseline`.
- Update `typescript/apps/sdk-harness/test/app.test.ts` for the new mode.
- Make the observability harness path non-skippable by staging a real native
  `.node` artifact from the Rust `node` build output and setting
  `FATHOMDB_NATIVE_BINDING` during the observability test.
- Leave mocked package unit tests largely unchanged.
- In telemetry assertions, require monotonic operation totals and non-negative
  cache activity rather than monotonic cache samples.
- Include success-summary detail in the harness result so the observed totals
  are written in test output.

## CI And Validation

### Weekly workflow updates

- Python stress job continues to run `python/tests/test_stress.py`
- Rust scale job continues to run ignored stress tests in `scale.rs`
- Add a tracing-feature robustness step or job for the Rust tracing load test
- Add a TypeScript SDK harness robustness step or job for the
  `observability` mode

### Validation targets

- Rust:
  - `cargo test -p fathomdb --test scale -- --ignored`
  - tracing-enabled test target covering `tracing_events.rs`
- Python core:
  - `pytest python/tests/test_stress.py`
- Python harness:
  - targeted `pytest` for the new observability harness module
- TypeScript:
  - harness test target for the `observability` mode

## Acceptance Criteria

- All new tests are written test-first.
- New tests pass locally with short duration overrides.
- Existing stress and harness tests remain green.
- Weekly CI includes the new observability coverage without moving it into
  per-PR suites.
- Weekly and local stress runs write end-of-test summaries with the tracked
  counts so operators can inspect observed activity without adding a second
  benchmark harness.
- No surface relies on undocumented behavior:
  - TypeScript feedback only asserts `started` and `finished` or `failed`
  - Python feedback may assert `slow` or `heartbeat`
  - Rust feedback equivalent is the tracing load test, not a fabricated
    callback API

## Assumptions

- “Python” means the real public Python API in `python/tests/test_stress.py`.
- “Python SDK” means the harness wrapper layer under `python/examples/harness`
  with tests under `python/tests/examples/`.
- “TypeScript SDK” means the real SDK harness, not mocked package tests.
- Existing stress tests remain behaviorally unchanged unless a helper
  extraction is needed for safety.
