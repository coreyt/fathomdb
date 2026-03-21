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
- writer wiring is tested
- admin wiring is tested
- black-box tests do not yet prove true read/write/repair roundtrips

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

1. Add Rust test helpers for temporary DB creation and canonical seeding.
2. Convert the current black-box scaffold tests to use those helpers.
3. Add one roundtrip read test and one repair/excision test in Rust.
4. Add one Go e2e path that invokes the real admin bridge against a seeded temp
   DB.
5. Add vector-specific fixtures only after capability gating is real.

## Done When

- new runtime work lands with scenario fixtures instead of handwritten SQL in
  each test
- Rust has true write/read/repair roundtrip tests
- Go has at least one bridge-backed e2e scenario against a real temp DB
