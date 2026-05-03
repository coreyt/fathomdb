---
title: 0.6.0 Test Plan
date: 2026-05-01
target_release: 0.6.0
desc: AC id to test id to layer mapping for the 0.6.0 rewrite
blast_radius: acceptance.md; src/rust/crates/*/tests; src/python/tests; src/ts/tests; CI workflows
status: locked
---

# Test Plan

This file binds every `acceptance.md` criterion to an executable test identity,
layer, owner, fixture family, and scaffold location.

## ID convention

Every AC has exactly one primary test id by deterministic transform:

- `AC-001` -> `T-001`
- `AC-003a` -> `T-003a`
- `AC-048b` -> `T-048b`

Additional narrow tests may suffix the primary id, for example
`T-035a-open-wal-replay`, but the unsuffixed id remains the traceability
anchor. No test id is valid without an AC back-reference.

## Suite Map

| AC ids           | Layer                | Owning package                                        | Fixture family                                                                                        | Scaffold path                                                                                                                          |
| ---------------- | -------------------- | ----------------------------------------------------- | ----------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| AC-001..AC-010   | integration          | `fathomdb-engine` + bindings                          | lifecycle subscriber, diagnostics, counters, profiling, projection status                             | `src/rust/crates/fathomdb-engine/tests/lifecycle_observability.rs`; binding mirrors under `src/python/tests/` and `src/ts/tests/`      |
| AC-011a..AC-020  | perf                 | `fathomdb-engine`                                     | 1M chunk/vector corpora, seeded benchmark DB, deterministic embedder, read-mix generator              | `src/rust/crates/fathomdb-engine/tests/perf_gates.rs`                                                                                  |
| AC-021..AC-025   | integration          | `fathomdb-engine`                                     | concurrent reader/admin DDL, open/close fd accounting, second-open lock, pending-vector shutdown      | `src/rust/crates/fathomdb-engine/tests/lifecycle_reliability.rs`                                                                       |
| AC-026..AC-028c  | integration          | `fathomdb-cli` + `fathomdb-engine`                    | WAL-only export, shadow corruption, recovery/excise source fixtures                                   | `src/rust/crates/fathomdb-cli/tests/recovery_cli.rs`                                                                                   |
| AC-029..AC-033   | integration/soak     | `fathomdb-engine`                                     | frozen scheduler, deterministic drain jobs, provenance retention workload                             | `src/rust/crates/fathomdb-engine/tests/projection_runtime.rs`                                                                          |
| AC-034a..AC-035c | soak                 | `fathomdb-engine`                                     | power-cut harness, OS-crash VM harness, 1GB recovery DB, open-path corruption matrix                  | `src/rust/crates/fathomdb-engine/tests/durability_soak.rs`                                                                             |
| AC-035d..AC-045  | integration/security | `fathomdb-cli`, Python, TypeScript                    | no-listen syscall capture, netns-deny-egress, FTS injection, safe-export manifest, doctor/recover CLI | `src/rust/crates/fathomdb-cli/tests/operator_cli.rs`; `src/python/tests/test_public_surface.py`; `src/ts/tests/public-surface.test.ts` |
| AC-046a..AC-050c | unit/integration     | `fathomdb-schema`, `fathomdb-engine`, release scripts | n-to-n+k migration DB, poison migration, 0.5 fixture, AST scanner, changelog/removal fixtures         | `src/rust/crates/fathomdb-schema/tests/migrations.rs`; `src/rust/crates/fathomdb-engine/tests/compatibility.rs`                        |
| AC-051a..AC-056  | integration/release  | release scripts + package managers                    | cargo/pip skew fixtures, co-tag/version files, registry-installed wheel smoke                         | `dev/release/tests/` and CI release workflow checks                                                                                    |
| AC-057a..AC-060b | integration          | Rust facade, Python, TypeScript                       | public-surface introspection, cursor read/write fixture, typed error matrix, JSON Schema payloads     | `src/rust/crates/fathomdb/tests/public_surface.rs`; `src/python/tests/test_public_surface.py`; `src/ts/tests/public-surface.test.ts`   |
| AC-061a..AC-063c | integration          | `fathomdb-engine` + `fathomdb-cli`                    | op-store append/latest collections, registry lifecycle, projection failure/restart/regenerate         | `src/rust/crates/fathomdb-engine/tests/op_store.rs`; `src/rust/crates/fathomdb-cli/tests/recovery_cli.rs`                              |

## Required Fixtures

The lock-blocking fixture set from `acceptance.md` is implemented once and
reused across suites:

- 1M chunk-row corpus with FTS5 + `vec0` indexes
- 1GB seeded DB
- open-path corruption matrix: WAL replay, header probe, schema probe,
  embedder-profile corruption
- power-cut and OS-crash harnesses
- shadow-table and page-corruption injection tools
- deterministic slow CTE, poison operation, mixed retrieval stress, read-mix,
  compressed provenance workload, vector/FTS 100-query suites
- AST scanner, removal-detect linter, cargo/pip skew fixtures, synthetic
  changelog fixtures, netns-deny-egress, and no-listen syscall capture

## Implementation Order

1. Replace scaffold property tests with failing tests for public surface,
   locking, cursor, and migration contracts.
2. Add CLI recovery/doctor tests before implementing CLI verbs.
3. Add durability/perf/soak harnesses behind explicit CI labels so normal
   `agent-verify` can stay fast while release gates still execute the full
   acceptance set.

   **Gate boundary — agent-verify vs check.sh AGENT_LONG=1:**
   - `scripts/agent-verify.sh` (fast local loop) runs lint → typecheck →
     unit/integration tests EXCLUDING long-run variants. It does NOT
     exercise: AC-021's 60 s spec-conforming window, AC-059b's
     ~1000-iteration cursor-race fixture
     (`cursor_read_after_write::projection_cursor_bounds_observed_row_count`).
     `agent-verify` runs the AC-021 5 s smoke variant only; the smoke run
     does not satisfy AC-021's measurement protocol on its own.
   - `scripts/check.sh` with `AGENT_LONG=1` (full evidence gate) runs
     everything `agent-verify` runs PLUS the long-run variants: AC-021's
     60 s spec-conforming window AND AC-059b's ~1000-iteration race
     fixture. AC-059b has no smoke variant — its evidence comes
     exclusively from this gate.

   A reviewer reading just this section should be able to attribute each
   long-run AC's evidence to a single command.

4. Keep thresholds in `acceptance.md`; tests read or restate those parameters
   but do not invent new gates.

## Component Scaffold Targets

The implementation module tree for `fathomdb-engine/src/` should be:

- `runtime/` for `Engine.open`, `Engine.close`, config, lock lifecycle, and
  open reports
- `writer/` and `reader/` for connection ownership and typed execution
- `migrations/`, `errors/`, `op_store/`, `embedder/`, `scheduler/`, `vector/`,
  `projections/`, `retrieval/`, and `lifecycle/` matching `architecture.md`
- `recovery/` only for engine helpers consumed by the CLI; no SDK recovery
  surface

The existing 0.6.0 scaffold crates (`fathomdb-cli`, `fathomdb-embedder-api`,
`fathomdb-embedder`, and `src/ts`) should be shaped in place rather than
created from scratch.
