# Design: Retrieval Robustness and Performance Gates

**Date:** 2026-04-23
**Status:** Draft
**Motivation:** Turn stress/performance review findings into release gates
**Related:** `dev/test-plan.md`, `dev/production-acceptance-bar.md`,
`dev/notes/design-projection-freshness-sli-harness-2026-04-23.md`

---

## Problem

FathomDB has many correctness and stress tests, but the current gates do not
fully answer the operational questions raised by the tokenizer and vector
projection work:

- Do reads keep working under concurrent writes, FTS rebuilds, and vector
  projection drains?
- Do WAL and file locks behave correctly with vector projection enabled?
- Does projection repair recover from chunk FTS, property FTS, and vector drift
  in one mixed workload?
- Do benchmarks include semantic/vector freshness and queue lag, not only query
  execution after pre-seeding?

This design adds test and benchmark gates that make those answers explicit.

---

## Goals

- Cover reliability and robustness for canonical, FTS, and vector projection in
  the same workload.
- Detect lock, WAL, and projection-corruption regressions before release.
- Add stable performance review outputs for:
  - write throughput under projection enqueue;
  - read p99 under write plus projection load;
  - semantic search query latency;
  - vector queue drain throughput and freshness.
- Keep short smoke gates suitable for CI and longer gates suitable for scheduled
  robustness workflows.

## Non-Goals

- Replacing SQLite's own corruption testing.
- Running network-backed embedders in release gates.
- Making every stress test mandatory on every pull request.
- Using arbitrary sleeps as proof of freshness when status or polling can give
  deterministic signals.

---

## Gate Matrix

| Gate | PR CI | Scheduled | Release review |
|---|---:|---:|---:|
| canonical + FTS freshness smoke | yes | yes | yes |
| explicit-drain semantic freshness smoke | yes, sqlite-vec | yes | yes |
| background semantic freshness | after scheduler lands | yes | yes |
| mixed retrieval stress | no | yes | yes |
| WAL/export with pending vector work | yes | yes | yes |
| lock contention with vector actor | yes | yes | yes |
| projection drift repair mixed mode | no | yes | yes |
| Criterion production paths | no | yes | yes |

---

## Test Set 1: Mixed Retrieval Stress

Add an ignored Rust stress test:

```text
crates/fathomdb/tests/scale.rs::mixed_retrieval_projections_under_load
```

Workload:

- 3 writer threads insert chunk-backed `Document` nodes with unique tokens;
- 2 writer threads upsert structured `Note` nodes with recursive property FTS;
- 1 retire thread retires a subset of older logical ids;
- vector indexing is enabled for `Document`;
- a deterministic embedder drains vector projection, either through explicit
  periodic drain or the future background scheduler;
- reader threads run:
  - canonical `nodes(kind).limit(...)`;
  - chunk `text_search`;
  - property `text_search`;
  - unified `search`;
  - `semantic_search`.

Assertions:

- no thread hangs;
- no query errors except expected degraded semantic results when embedder mode
  explicitly simulates unavailability;
- `check_integrity` passes;
- `check_semantics` reports no stale FTS/vector rows;
- vector status has no permanent pending work after final drain;
- p99 read latency under load stays below a configurable threshold.

Environment:

```text
FATHOM_RUST_STRESS_DURATION_SECONDS=30
FATHOM_RUST_STRESS_VECTOR_MODE=explicit_drain|background|unavailable
```

---

## Test Set 2: WAL And Export With Pending Vector Work

Add a Rust integration test:

```text
crates/fathomdb/tests/wal_vector_projection.rs
```

Cases:

1. Enable WAL, disable autocheckpoint, write vector-enabled chunks, do not drain
   projection, call `safe_export(force_checkpoint=false)`, open export, verify:
   - canonical rows are present;
   - vector queue rows are present;
   - no partially applied vec rows appear without their canonical chunks.
2. Repeat with `force_checkpoint=true` and an active long reader; verify the
   documented busy-checkpoint behavior.
3. Reopen the original DB after pending work, drain projection, verify semantic
   search works.

This catches regressions where backup/export includes canonical data but loses
queued vector projection work.

---

## Test Set 3: Lock Contention And Lifecycle

Add tests around existing `DatabaseLocked` behavior with vector projection
enabled:

```text
crates/fathomdb/tests/vector_lock_lifecycle.rs
```

Cases:

- opening a second engine on the same DB fails while the first has pending
  vector work;
- close joins writer/vector/rebuild actors and releases the lock;
- reopening after close can drain pending vector work;
- dropping an engine with pending vector work does not deadlock.

This extends lock coverage from the database core into the managed projection
runtime.

---

## Test Set 4: Projection Drift And Repair

Add a mixed corruption/repair test:

```text
crates/fathomdb-engine/tests/projection_repair_mixed.rs
```

Seed:

- chunk FTS rows;
- property FTS rows;
- vector rows;
- pending vector work.

Inject drift:

- delete one `fts_nodes` row;
- delete one per-kind property FTS row;
- delete one vector row;
- insert one stale vector row for a retired chunk if sqlite-vec allows it;
- leave one vector work row in `in_progress`.

Run repair:

- `check_integrity`;
- `check_semantics`;
- `rebuild_missing`;
- vector drain or vector repair hook.

Assertions:

- missing FTS/property FTS rows are repaired;
- stale vector rows are removed or reported by semantics;
- pending/in-progress vector work becomes drainable;
- no canonical rows are rewritten unexpectedly.

---

## Benchmark Additions

Extend `crates/fathomdb/benches/production_paths.rs` with:

```text
write_submit_vector_enabled_enqueue
admin_drain_vector_projection_100
query_execute_semantic_search
projection_freshness_explicit_drain
```

The semantic benchmark should use deterministic in-process embeddings, not the
network or the built-in model. The built-in model can have a separate optional
manual benchmark because first-load cost is operationally useful but noisy.

Output should include both Criterion measurements and one JSON summary from the
freshness harness.

---

## Suggested Thresholds

Initial review thresholds, subject to adjustment after reference-run data:

| Measurement | Threshold |
|---|---:|
| canonical freshness p99 | <= 50ms |
| FTS freshness p99 | <= 50ms |
| explicit-drain semantic freshness p99 | <= 500ms |
| semantic query p95 on seeded 250-row dataset | <= 250ms |
| drain 100 deterministic embeddings | <= 2s |
| mixed retrieval stress read p99 | <= max(10x baseline, 150ms) |

Treat these as review gates first. Promote to hard release gates only after the
reference runner has stable baselines.

---

## CI / Workflow Integration

Add a scheduled workflow or extend the existing benchmark/robustness workflow:

```text
.github/workflows/benchmark-and-robustness.yml
```

Stages:

1. Build with `sqlite-vec`.
2. Run non-ignored projection freshness smoke.
3. Run Criterion production paths.
4. Run ignored mixed stress with a bounded duration.
5. Upload:
   - Criterion report;
   - freshness JSONL;
   - mixed stress summary;
   - `check_integrity` / `check_semantics` final reports.

Release verification should ensure the workflow exists, is recent enough, and
is green before a production-ready claim.

---

## Observability Requirements

The tests should print concise summaries on success:

```text
mixed_retrieval_projections_under_load:
duration_seconds=30,writes=...,canonical_reads=...,fts_reads=...,
semantic_reads=...,drain_ticks=...,pending_vector=0,p99_read_us=...
```

On failure, include:

- thread group;
- operation kind;
- last error;
- projection status for affected kind;
- integrity/semantics counters.

This avoids "stress failed" without enough context to debug.

---

## Acceptance Criteria

- New tests cover WAL/export, lock lifecycle, mixed retrieval stress, and mixed
  projection repair with vector projection enabled.
- New benchmarks cover vector enqueue, vector drain, semantic search, and
  freshness.
- The benchmark/robustness workflow publishes freshness and stress artifacts.
- `dev/production-acceptance-bar.md` is updated with measured thresholds after
  reference-run data exists.

