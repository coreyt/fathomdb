# Design: Vector Projection Background Scheduler

**Date:** 2026-04-23
**Status:** Draft
**Motivation:** Make managed semantic indexing catch up without explicit drain calls
**Related:** `dev/notes/design-projection-freshness-sli-harness-2026-04-23.md`,
`dev/notes/managed-vector-projection-handoff-2026-04-22.md`

---

## Problem

Managed vector projection now has durable queue state, writer-side enqueue, and
explicit drain APIs. That is enough for deterministic tests and admin-driven
catch-up, but it does not yet give ordinary applications a production
"write eventually appears in semantic search" path unless they call
`drain_vector_projection` or use test-only `auto_drain_vector`.

The current `VectorProjectionActor` starts a thread, preserves drop-order
discipline, and idles. It does not own an embedder resolver and therefore cannot
run projection ticks on its own.

This leaves B3 freshness ambiguous in production: the write commits and vector
work is durable, but no background worker necessarily applies that work.

---

## Decision

Wire the existing `VectorProjectionActor` to the engine's configured embedder
and let it process `vector_projection_work` in the background.

The scheduler should remain conservative:

- canonical writes never wait for embedding;
- the worker uses the same embedder identity as read-time `semantic_search`;
- incremental work is prioritized over backfill;
- embedder failures do not poison canonical state;
- all vector table writes still go through `WriterActor`.

---

## Goals

- Make vector-enabled ordinary writes eventually visible to `semantic_search`
  without explicit admin drain.
- Preserve the identity invariant: projection and query use the same embedder.
- Avoid starving fresh incremental writes behind large backfills.
- Add status and telemetry sufficient to explain vector lag.
- Keep shutdown and drop order clean.
- Keep a deterministic explicit-drain path for tests and admin tools.

## Non-Goals

- Supporting per-kind embedding engines.
- Running embedding inside the canonical write transaction.
- Adding cross-kind semantic fanout.
- Making vector freshness equivalent to synchronous FTS freshness.
- Removing `drain_vector_projection`; it remains useful for tests, operators,
  and one-shot maintenance.

---

## Current State Anchors

| Area | Current behavior |
|---|---|
| Queue | `vector_projection_work` is durable and populated by `configure_vec_kind` backfill plus writer incremental enqueue. |
| Actor | `VectorProjectionActor` owns a thread but idle ticks are no-ops. |
| Explicit drain | `AdminService::drain_vector_projection` calls `run_tick` with a supplied embedder. |
| Test sync path | `EngineOptions::auto_drain_vector=true` drains after writes, explicitly marked test-only. |
| Apply path | `run_tick` claims work and writes results through `WriterActor`, preserving single-writer semantics. |

---

## Scheduler Model

### Embedder ownership

`EngineRuntime::open` already receives `Option<Arc<dyn QueryEmbedder>>` for
read-time semantic search. The vector actor needs a batch-capable view of that
same embedder.

Add an internal adapter equivalent to the existing `AutoDrainBatchAdapter`, but
owned by the runtime:

```rust
struct QueryEmbedderBatchAdapter {
    inner: Arc<dyn QueryEmbedder>,
}

impl BatchEmbedder for QueryEmbedderBatchAdapter {
    fn batch_embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        texts.iter().map(|text| self.inner.embed_query(text)).collect()
    }
}
```

If the concrete embedder also implements a faster batch interface, defer that
optimization. A sequential adapter is enough to make the worker correct and
keeps the identity story simple.

### Actor startup

Change actor startup from:

```rust
VectorProjectionActor::start(&writer)
```

to:

```rust
VectorProjectionActor::start(Arc<WriterActor>, AdminHandle, Option<Arc<dyn BatchEmbedder>>)
```

or an equivalent shape that avoids reference cycles. The actor needs:

- writer handle for claim/apply;
- admin service or narrow projection service handle for `run_tick`;
- optional batch embedder.

When no embedder is configured, the actor should idle and expose status
`embedder_configured=false`. It should not mark rows failed simply because no
embedder exists.

### Tick loop

The loop should process on either wakeup or periodic interval:

```text
on startup:
  recover stuck in_progress vector work to pending/failed according to age

loop:
  wait for Wakeup or interval
  if no embedder: continue
  run up to MAX_TICKS_PER_WAKE
  sleep/backoff if no work or embedder unavailable
```

Suggested defaults:

```text
idle_poll_ms = 250
max_ticks_per_wake = 4
embedder_unavailable_backoff_ms = min(30_000, 2^attempt * 250)
```

`run_tick` already encodes priority:

- claim incremental rows first (`priority >= 1000`, batch 64);
- if none, claim backfill rows (batch 32).

Keep that behavior.

### Wakeups

Writer enqueue should notify the actor after commit. The durable queue is the
correctness boundary, so losing a wakeup is acceptable; the periodic tick will
catch up.

Add:

```rust
VectorProjectionActor::wake()
```

The writer should not own the actor directly. A lightweight `VectorProjectionWakeup`
sender can be held by `Engine` or `AdminService`; after `submit_write` returns,
`Engine::submit_write` can call `wake()` if the write receipt indicates vector
work was enqueued.

If plumbing a receipt field is too invasive, a wakeup after every successful
write is acceptable for v1. It is cheap and harmless because the actor will find
no work.

### Work recovery

On startup, any `vector_projection_work` row left in `in_progress` state from a
crashed process should be returned to `pending` if retryable, or marked failed
if it exceeded retry limits. This mirrors the FTS rebuild recovery posture.

Add fields if missing:

```text
claimed_at
updated_at
last_error
attempt_count
```

If the table already has some of these fields, reuse them.

---

## Status And Telemetry

Extend `get_vec_index_status(kind)` with:

```text
pending_incremental_rows
pending_backfill_rows
in_progress_rows
failed_rows
last_projection_started_at
last_projection_completed_at
last_projection_error
oldest_pending_age_ms
worker_running
worker_has_embedder
```

Extend telemetry counters:

```text
vector_projection_ticks_total
vector_projection_rows_applied_total
vector_projection_rows_failed_total
vector_projection_rows_discarded_total
vector_projection_embedder_unavailable_total
vector_projection_queue_oldest_pending_age_ms
```

These counters are required to answer "is semantic search stale because nothing
is configured, because the worker is offline, because embedding fails, or
because the queue is deep?"

---

## Error Handling

### Embedder unavailable

Do not fail canonical writes. Leave work pending or mark retry metadata.
Background worker backs off and logs warn-level events with kind, row count, and
error class.

### Invalid embedder output

Wrong dimensions or non-finite values should mark the claimed work failed with
a clear `last_error`. This is a configuration/model correctness issue, not
transient queue lag.

### Canonical drift

If the chunk hash no longer matches, discard the stale work. A new write should
have enqueued newer work if the kind remains vector-enabled.

### SQLite contention

The worker uses `WriterActor`, so contention is mostly queueing behind normal
writes. If writer calls time out, leave work pending and retry later.

---

## Tests

Add Rust tests under:

```text
crates/fathomdb-engine/tests/vector_projection_background_scheduler.rs
crates/fathomdb/tests/projection_freshness.rs
```

Required cases:

1. Background worker applies an incremental write without explicit drain.
2. Background worker prioritizes a new incremental row ahead of existing
   backfill rows.
3. No configured embedder leaves work pending and does not mark rows failed.
4. Embedder unavailable backs off and keeps canonical writes successful.
5. Engine close joins the worker without hanging while work is pending.
6. Reopen recovers `in_progress` work.
7. Freshness harness `vector_background` mode passes with deterministic
   in-process embedder.

Python and TypeScript should get smoke coverage after Rust:

- open with callable/in-process embedder when available;
- configure vector kind;
- write normal chunk;
- eventually `semantic_search` returns without explicit drain.

If callable embedders are not available for a language yet, keep the language
test scoped to status fields and explicit drain.

---

## Compatibility

`auto_drain_vector` remains test-only. Production applications should rely on
the background scheduler and use status/telemetry to monitor lag.

`drain_vector_projection` remains public. It should coexist with the background
worker by using the same writer claim semantics. If both run at once, one will
claim rows and the other will see fewer or no rows.

---

## Acceptance Criteria

- With a configured in-process deterministic embedder, a normal write to a
  vector-enabled kind becomes visible to `semantic_search` without explicit
  drain.
- Incremental work is processed before backfill in a mixed queue.
- Canonical writes succeed when the embedder is unavailable.
- Status shows pending or failed vector work with enough detail to diagnose lag.
- `projection_freshness` reports B3 background p99 in the benchmark workflow.

