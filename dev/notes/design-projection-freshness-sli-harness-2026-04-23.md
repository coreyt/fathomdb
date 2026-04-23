# Design: Projection Freshness SLI Harness

**Date:** 2026-04-23
**Status:** Draft
**Motivation:** Close the write-to-read/index latency gap found during stress and performance review
**Related:** `dev/notes/managed-vector-projection-followups-2026-04-23.md`,
`dev/notes/design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md`

---

## Problem

FathomDB has functional coverage for canonical reads, FTS projection, and
managed vector projection, but it does not yet publish a direct freshness
measurement for:

1. write acknowledgement to canonical retrieval;
2. write acknowledgement to FTS search visibility;
3. write acknowledgement to vector/semantic search visibility.

Today we can infer B1 and B2 from transaction structure, and we have functional
tests for B3 after explicit vector drains or test-only `auto_drain_vector`.
That is not enough for a production claim. We need an automated harness that
records latency distributions and makes the freshness contract explicit.

---

## Current State

Canonical rows and FTS rows are committed in the writer transaction. Vector
projection is queued in that same transaction for configured vector kinds, then
materialized when `drain_vector_projection` runs. The current
`VectorProjectionActor` owns lifecycle/drop order but does not yet run embedding
ticks in the background without an explicit drain path.

Existing performance and stress tests measure useful adjacent properties:

- concurrent readers under write load;
- adaptive/search p99 under writers;
- vector functional query behavior;
- integrity and semantics after stress.

They do not measure "write N, then observe the same N through canonical/FTS/vec"
as a latency SLI.

---

## Goals

- Produce p50, p95, p99, max, and timeout counts for B1, B2, and B3 freshness.
- Measure from the caller-visible successful write return, not from transaction
  begin.
- Exercise both Rust and Python-facing workloads where practical.
- Keep the harness deterministic enough for CI smoke and configurable enough
  for longer benchmark runs.
- Distinguish "not visible yet" from query errors, projection degradation, and
  vector embedder failures.
- Emit machine-readable JSON lines so CI can archive and compare results.

## Non-Goals

- Replacing Criterion throughput benchmarks.
- Proving a hard real-time SLA.
- Running network-backed embedders in CI.
- Treating test-only `auto_drain_vector=true` as a production freshness result.

---

## Contract Under Test

### B1: canonical freshness

After `write()` returns `Ok`, a new read transaction should be able to retrieve
the active canonical row immediately. A read transaction that started before
the write may keep its older snapshot; the SLI only uses fresh reads after
write acknowledgement.

Expected result: near-zero polling delay. Any miss after write acknowledgement
is a correctness failure unless the read intentionally reuses an old transaction.

### B2: FTS freshness

For chunk FTS and property FTS, a new `text_search` after write acknowledgement
should see the write immediately because FTS rows are maintained in the writer
transaction.

Expected result: near-zero polling delay. A non-zero delay indicates query
snapshot reuse, test harness error, or a projection bug.

### B3: vector/semantic freshness

For managed vector projection, write acknowledgement means durable work was
queued, not that embedding has completed. Semantic visibility depends on the
projection worker or explicit drain path.

Expected result:

- with explicit drain: bounded by drain duration;
- with future background scheduler: bounded by worker polling, embed latency,
  and queue depth;
- with no embedder or unavailable embedder: canonical writes still pass, vector
  freshness times out and status reports pending or failed work.

---

## Harness Shape

Add a Rust integration test module:

```text
crates/fathomdb/tests/projection_freshness.rs
```

The test helper should run three modes:

```text
mode = canonical_fts_only
mode = vector_explicit_drain
mode = vector_background
```

`vector_background` is initially ignored until the background scheduler design
lands. Keeping the mode in the design lets the same harness become the
acceptance test for that feature.

For each iteration:

1. Generate a unique logical id and unique search token.
2. Submit a write with:
   - one active node;
   - one chunk containing the unique token;
   - property text containing a second unique token if property FTS is enabled.
3. Record `write_ack_at = Instant::now()` immediately after `submit_write`
   returns.
4. Poll canonical query until the logical id appears.
5. Poll `text_search(unique_token)` until the logical id appears.
6. If vector mode is enabled, poll `semantic_search(unique_token)` until the
   logical id appears.
7. Record per-surface elapsed time from `write_ack_at`.

Polling should use a small bounded sleep, e.g. 1ms for canonical/FTS and 5ms
for vector, with per-surface timeout defaults:

```text
canonical_timeout_ms = 100
fts_timeout_ms       = 100
vector_timeout_ms    = 5000
```

Each result line:

```json
{
  "surface": "canonical|fts|semantic",
  "iteration": 42,
  "latency_us": 713,
  "polls": 1,
  "status": "visible|timeout|error|degraded",
  "write_label": "freshness-42"
}
```

At the end, emit a summary line:

```json
{
  "suite": "projection_freshness",
  "mode": "vector_explicit_drain",
  "iterations": 100,
  "canonical": {"p50_us": 120, "p95_us": 350, "p99_us": 900, "timeouts": 0},
  "fts": {"p50_us": 180, "p95_us": 450, "p99_us": 1100, "timeouts": 0},
  "semantic": {"p50_us": 6200, "p95_us": 18000, "p99_us": 42000, "timeouts": 0}
}
```

---

## Deterministic Embedder

Use an in-process deterministic embedder in Rust tests. It should map text to a
stable small vector where a query containing the unique token is nearest to the
matching chunk.

Do not use the built-in Candle model in CI freshness tests. Model load and
first-use cache behavior are useful for separate operational benchmarks, but
they add noise to this SLI.

---

## Thresholds

Initial CI smoke thresholds should be conservative:

| Surface | Smoke threshold |
|---|---:|
| canonical p99 | <= 50ms |
| FTS p99 | <= 50ms |
| semantic explicit-drain p99 | <= 500ms |
| semantic background p99 | design target <= 2s, ignored until scheduler lands |

For release benchmarking, store results without failing on tight thresholds at
first. After two or three stable runs on the reference runner, promote the
observed envelope into `dev/production-acceptance-bar.md`.

---

## Python Harness

Add a Python smoke test after Rust proves the core behavior:

```text
python/tests/test_projection_freshness.py
```

Scope:

- canonical and FTS freshness through public Python APIs;
- vector freshness only when a Python callable embedder exists or when the
  built-in embedder is available in the wheel environment;
- otherwise assert that status exposes pending vector work rather than silently
  appearing fresh.

The Python test should not duplicate the full Rust histogram. Its purpose is
API-path coverage and user-facing behavior.

---

## CLI / Script Integration

Add an opt-in script:

```text
scripts/run-freshness-benchmarks.sh
```

Suggested flags:

```text
FATHOM_FRESHNESS_ITERATIONS=1000
FATHOM_FRESHNESS_MODE=all
FATHOM_FRESHNESS_JSONL=target/freshness.jsonl
```

The script should run outside normal unit-test paths unless explicitly invoked
by the benchmark workflow.

---

## Failure Interpretation

### Canonical timeout

Treat as release-blocking. It means the write acknowledgement no longer implies
canonical visibility to a fresh read.

### FTS timeout

Treat as release-blocking for chunk/property FTS. It means the synchronous
projection contract regressed.

### Semantic timeout

Interpret by mode:

- explicit drain mode: likely projection actor/drain regression;
- background mode: scheduler lag, embedder error, or queue starvation;
- unavailable embedder mode: expected timeout, but status must report pending or
  failed work.

---

## Acceptance Criteria

- Rust freshness harness emits per-iteration JSON lines and summary JSON.
- Canonical and FTS p99 thresholds pass in CI smoke mode.
- Explicit-drain semantic freshness passes with a deterministic embedder.
- Background semantic freshness test exists as ignored or feature-gated until
  the scheduler lands.
- `dev/production-acceptance-bar.md` is updated after reference-run numbers are
  available.

