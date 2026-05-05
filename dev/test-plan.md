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

| AC ids           | Layer                | Owning package                                        | Fixture family                                                                                                                            | Scaffold path                                                                                                                          |
| ---------------- | -------------------- | ----------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| AC-001..AC-010   | integration          | `fathomdb-engine` + bindings                          | lifecycle subscriber, diagnostics, counters, profiling, projection status                                                                 | `src/rust/crates/fathomdb-engine/tests/lifecycle_observability.rs`; binding mirrors under `src/python/tests/` and `src/ts/tests/`      |
| AC-011a..AC-020  | perf                 | `fathomdb-engine`                                     | 1M chunk/vector corpora, seeded benchmark DB, deterministic embedder, read-mix generator, projection-freshness harness, drain-100 harness | `src/rust/crates/fathomdb-engine/tests/perf_gates.rs`                                                                                  |
| AC-021..AC-025   | integration          | `fathomdb-engine`                                     | concurrent reader/admin DDL, open/close fd accounting, second-open lock, pending-vector shutdown                                          | `src/rust/crates/fathomdb-engine/tests/lifecycle_reliability.rs`                                                                       |
| AC-026..AC-028c  | integration          | `fathomdb-cli` + `fathomdb-engine`                    | WAL-only export, shadow corruption, recovery/excise source fixtures                                                                       | `src/rust/crates/fathomdb-cli/tests/recovery_cli.rs`                                                                                   |
| AC-029..AC-033   | integration/soak     | `fathomdb-engine`                                     | frozen scheduler, deterministic drain jobs, provenance retention workload                                                                 | `src/rust/crates/fathomdb-engine/tests/projection_runtime.rs`                                                                          |
| AC-034a..AC-035c | soak                 | `fathomdb-engine`                                     | power-cut harness, OS-crash VM harness, 1GB recovery DB, open-path corruption matrix                                                      | `src/rust/crates/fathomdb-engine/tests/durability_soak.rs`                                                                             |
| AC-035d..AC-045  | integration/security | `fathomdb-cli`, Python, TypeScript                    | no-listen syscall capture, netns-deny-egress, FTS injection, safe-export manifest, doctor/recover CLI                                     | `src/rust/crates/fathomdb-cli/tests/operator_cli.rs`; `src/python/tests/test_public_surface.py`; `src/ts/tests/public-surface.test.ts` |
| AC-046a..AC-050c | unit/integration     | `fathomdb-schema`, `fathomdb-engine`, release scripts | n-to-n+k migration DB, poison migration, 0.5 fixture, AST scanner, changelog/removal fixtures                                             | `src/rust/crates/fathomdb-schema/tests/migrations.rs`; `src/rust/crates/fathomdb-engine/tests/compatibility.rs`                        |
| AC-051a..AC-056  | integration/release  | release scripts + package managers                    | cargo/pip skew fixtures, co-tag/version files, registry-installed wheel smoke                                                             | `dev/release/tests/` and CI release workflow checks                                                                                    |
| AC-057a..AC-060b | integration          | Rust facade, Python, TypeScript                       | public-surface introspection, cursor read/write fixture, typed error matrix, JSON Schema payloads                                         | `src/rust/crates/fathomdb/tests/public_surface.rs`; `src/python/tests/test_public_surface.py`; `src/ts/tests/public-surface.test.ts`   |
| AC-061a..AC-063c | integration          | `fathomdb-engine` + `fathomdb-cli`                    | op-store append/latest collections, registry lifecycle, projection failure/restart/regenerate                                             | `src/rust/crates/fathomdb-engine/tests/op_store.rs`; `src/rust/crates/fathomdb-cli/tests/recovery_cli.rs`                              |

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
     (`cursor_read_after_write::projection_cursor_bounds_observed_row_count`),
     and AC-020's sequential-vs-8-reader perf comparison once that
     read-mix fixture is landed. `agent-verify` runs the AC-021 5 s
     smoke variant only; the smoke run
     does not satisfy AC-021's measurement protocol on its own.
   - `scripts/check.sh` with `AGENT_LONG=1` (full evidence gate) runs
     everything `agent-verify` runs PLUS the long-run variants: AC-021's
     60 s spec-conforming window, AC-059b's ~1000-iteration race
     fixture, and AC-020's sequential-vs-8-reader comparison once the
     read-mix fixture is protocol-complete. AC-059b has no smoke
     variant — its evidence comes exclusively from this gate.

   A reviewer reading just this section should be able to attribute each
   long-run AC's evidence to a single command.

4. Keep thresholds in `acceptance.md`; tests read or restate those parameters
   but do not invent new gates.

## Current Perf Attribution

- `src/rust/crates/fathomdb-engine/tests/perf_gates.rs` currently binds
  AC-017 and AC-018 with active runtime measurements that run under
  `scripts/agent-verify.sh`.
- AC-012 / AC-013 / AC-019 protocol-complete fixture harnesses landed
  in Pack D (Phase 9) at `tests/perf_gates.rs`. All three are
  `long_run_enabled()`-gated (early-return without `AGENT_LONG=1`) and
  evidenced exclusively by `scripts/check.sh` with `AGENT_LONG=1`.
  - **AC-012** seeder: deterministic seeded-LCG token stream over a
    1024-token vocabulary with Zipfian (s=1.0) frequency distribution;
    chunk bodies ≈ 500 B (55–75 tokens); held-out query token band
    indices [10%, 50%) of vocab — i.e. the 50th–90th percentile term-
    frequency band per ADR-0.6.0-text-query-latency-gates. Warmup pass
    discarded; second pass measured (P-PERF-SAMPLES = 1,000).
  - **AC-013** seeder: same chunk-body generator wired through a
    deterministic-but-varying 768-d embedder (`VaryingEmbedder`,
    FNV-1a hash projected to 6 sparse coordinates with unit-norm) so
    vec0 ANN search returns distinct k=10 neighbors. Held-out query
    body set drawn from the same distribution with a different LCG
    seed (mirrors ADR-0.6.0-retrieval-latency-gates "held-out slice").
  - **AC-019** workload: re-runs AC-013's protocol immediately
    preceding the stress pass (per acceptance.md AC-019 wording),
    captures `baseline_p99`, then runs `AC019_THREADS=8` concurrent
    reader threads × `AC019_QUERIES_PER_THREAD=250` mixed FTS5 +
    vector + canonical reads. Tail latency captured via a bounded
    32-bucket power-of-two `LatencyHistogram` (microseconds) per the
    plan's "no unbounded `Vec<Duration>`" instruction.
  - **Scale knobs.** Canonical scale is 1,000,000 chunk rows / 768-d
    vectors (ADR). The CI runner sets `AC_FULL_SCALE=1` to honor it;
    `AC012_CORPUS_N` / `AC013_CORPUS_N` env vars override per host
    (AC-007a/b runner-pin precedent). Default scale on this aarch64
    Tegra dev runner: AC-012 N=100,000; AC-013 N=50,000. Budgets
    pinned to ADR values; do not relax.
  - **Pack D dev-runner status (aarch64 Linux 5.15-tegra,
    2026-05-05).** AC-012 at default N=100,000: p50=29.7 ms, p99=85
    ms — p50 RED vs ADR 20 ms; p99 GREEN. AC-013 at N=10,000 (full
    AGENT_LONG run not feasible on this host): p50=33 ms, p99=48 ms
    — both GREEN against ADR 50 / 200 ms. AC-013 at N=50,000 not
    measured: vec0 single-row insertion path on this host took
    1,800 s wall-clock to seed 10,000 vectors (≈ 5.5 inserts/sec)
    — engine-surface scaling gap, FLAGGED for orchestrator and Pack
    7. AC-019 inherits the AC-013 seed cost; not measured at N=50k.
    Canonical CI x86_64 tier-1 runner re-measurement is required to
    close the perf gates.
  - **Deferral.** AC-012 / AC-013 / AC-019 are **DEFERRED for 0.6.0**
    as of Pack D close (2026-05-05) — paralleling AC-020. Harnesses
    are protocol-complete and `AGENT_LONG`-gated; canonical-runner
    measurement + the vec0 bulk-seed engine-surface gap are early
    Pack 7 work per
    `dev/plans/0.6.0-Phase-9-Pack-7-canonical-perf-measurement.md`.
    0.6.0 ships with these three gates documented as DEFERRED, not
    weakened. Budgets stay pinned at ADR values.
- AC-020 is **DEFERRED for 0.6.0** as of Pack 6.G close (2026-05-04).
  Implemented as a long-run-only env-gated harness; the documented read
  mix is 50% vector-only semantic queries (`semantic-*`) and 50% hybrid
  queries (`hybrid-*`) over a pre-drained vector-indexed fixture, 50
  rounds per reader thread. Evidenced by `scripts/check.sh` with
  `AGENT_LONG=1`; `agent-verify.sh` executes only the early-return path
  and is not evidence for the AC.

  Latest measured medians (N=5, current `0.6.0-rewrite` tip, this host
  Linux 5.15-tegra ~3× slower in absolute ms than the Pack 5 reference
  machine): seq=563 ms, conc=161 ms, bound=105 ms, speedup=3.530×
  (required ≥ 5.0× packet rule / ≥ 5.33× test bound). AC-017 + AC-018
  stay green throughout. Best individual run on this host (G.1 run 5):
  seq=503 / conc=118 / speedup=4.263× — single-run only, not a gate.

  **Deferral rationale.** Pack 5 + Pack 6 + Pack 6.G falsified every
  canonical-SQLite lever measurable on the AC-020 read-only fixture
  (mutex track via B.1 / C.1; parse-cost track via E.1; pool-topology
  track via F.0 KEPT but does not close; allocator-arena via G.1
  LANDED INCONCLUSIVE; page-cache via G.3.5 SKIP_G4 — `cache_used` =
  3.35% / `delta_miss_rate` = 0.023% leaves no headroom; WAL atomics +
  checkpoint = 0% under stack-aware classification). Residual
  `page_cache` 6.29% conc share is from `pcache1` mutex acquires on
  every page-fetch (hit-path), which canonical SQLite cannot eliminate
  without `SQLITE_CONFIG_PCACHE2` custom allocator install. That,
  WAL2, reader/writer physical separation, and vendor-SQLite swap
  are all Pack 7 territory — see
  `dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md` §13.

  AC-020 stays RED in CI; the test bound (`tests/perf_gates.rs:245`)
  is unchanged. 0.6.0 ships with the gate documented as DEFERRED, not
  weakened. Pack 7 reopens the gate when a measured fix lands.

  Evidence trail:
  - `dev/notes/performance-whitepaper-notes.md` (§4 kept ledger
    incl. F.0 / G.1; §5 reverted ledger; §11 Pack 5 narrative;
    §12 Pack 6 + Pack 6.G synthesis).
  - `dev/plan/0.6.0-Phase-9-Pack-5-performance-diagnostics.md` (DoE
    plan, §12 audit trail, §13 Pack 7 proposed work).
  - `dev/plan/runs/F0-thread-affine-readers-output.json` (F.0
    topology + numbers).
  - `dev/plan/runs/G0-wal-checkpoint-telemetry-output.json` (stack-
    aware symbol classification).
  - `dev/plan/runs/G1-reader-lookaside-output.json` (lookaside
    LANDED INCONCLUSIVE).
  - `dev/plan/runs/G3_5-cache-pressure-telemetry-output.json`
    (page-cache lever falsified).
  - `dev/plan/runs/STATUS.md` (Pack 5 / 6 / 6.G close-state board).

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
