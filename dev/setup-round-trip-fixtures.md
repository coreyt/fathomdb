# Setup: Round-Trip Fixtures

## Purpose

This document tracks the test scaffolding needed to prove the new runtime layers
with real SQLite roundtrips instead of wiring-only tests.

This is a companion to:

- [design-read-execution.md](./design-read-execution.md)
- [design-typed-write.md](./design-typed-write.md)
- [design-repair-provenance-primitives.md](./design-repair-provenance-primitives.md)

## Current Repository State

The repo already has useful smoke tests, but they are still mostly scaffold
tests:

- query compilation is tested
- typed write plus read execution roundtrips are now tested in Rust
- admin wiring is tested
- Go has one real bridge-backed temp-db e2e scenario for `trace`
- repair and excision still do not have real roundtrip coverage

The next phase needs fixtures that can support all three runtime layers without
duplicating setup logic across crates.

## Fixture Goals

1. Seed canonical rows using the same typed paths the engine will expose.
2. Support deterministic FTS roundtrips now.
3. Support vector-capability-on and vector-capability-off test modes later.
4. Support provenance and repair scenarios with multiple physical versions of
   one logical row.
5. Keep tests cheap enough for `cargo nextest` and `go test ./...`.

## Fixture Types

### 1. Rust Engine Builders

Add reusable test helpers for:

- temporary database creation
- engine open/bootstrap
- canonical seed data
- chunk and FTS seed data
- versioned row scenarios

These helpers should live close to the Rust black-box tests, not as a generic
testing framework crate.

### 2. Scenario Fixtures

Define a small set of named scenarios:

- `meeting_text_search`
- `versioned_node_repair`
- `missing_fts_projection`
- `source_ref_excision`
- later: `vector_profile_enabled`

The point is to give the runtime layers stable integration targets.

### 3. Bridge Fixtures

For Go e2e coverage, provide one reusable temporary-db scenario that can be
driven through the admin bridge for:

- `check`
- `trace`
- `rebuild`
- `excise`

That keeps Go tests focused on CLI and protocol behavior rather than SQL setup.

## Implementation Sequence

1. [ ] Add Rust test helpers for temporary DB creation and canonical seeding.
2. [ ] Convert the current black-box scaffold tests to use those helpers.
3. [~] Add one roundtrip read test and one repair/excision test in Rust.
       The read/write roundtrip exists; repair/excision coverage is still open.
4. [x] Add one Go e2e path that invokes the real admin bridge against a seeded
       temp DB.
5. [ ] Add vector-specific fixtures only after capability gating is real.

## Notes

- The current Go e2e path resolves the repo-local `sqlite3` binary through a
  shared test helper that reads `tooling/sqlite.env`, so `unixepoch()`-based
  seed SQL now runs against the repo-standard SQLite `3.46.0`.
- The next fixture improvement should remove handwritten SQL from the Go e2e
  seed path and replace it with reusable scenario setup.

## Done When

- new runtime work lands with scenario fixtures instead of handwritten SQL in
  each test
- Rust has true write/read/repair roundtrip tests
- Go has at least one bridge-backed e2e scenario against a real temp DB
