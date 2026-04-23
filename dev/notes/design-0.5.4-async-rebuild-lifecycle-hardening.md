# Design: Async Rebuild Lifecycle Hardening (0.5.4 candidate)

**Release:** 0.5.4 candidate
**Status:** Draft 2026-04-22
**Findings covered:** Review findings 3, 4, and 5; partially finding 6
**Breaking:** No public API break intended

---

## Problem

The async FTS property rebuild path has three lifecycle/correctness risks:

1. Engine shutdown can hang if external Rust code keeps an `Arc<AdminService>`
   after dropping `EngineRuntime`. That clone owns a rebuild request sender, so
   the rebuild actor's receiver never closes and `Drop` can block in `join()`.
2. Rebuild scans active nodes with `LIMIT/OFFSET` over a mutable table. Concurrent
   upserts/deletes before the current offset can shift rows and cause unchanged
   nodes to be skipped from staging. The final swap then deletes live FTS rows and
   inserts only staging rows.
3. Async registration can persist `PENDING` and fail to enqueue when the bounded
   channel is full. Startup recovery intentionally preserves `PENDING`, and there
   is no durable requeue mechanism, so a first-registration rebuild can remain
   unavailable indefinitely.

---

## Current State Anchors

| Area | Current behavior |
|---|---|
| Runtime field order | `EngineRuntime` stores `_rebuild_sender` and `_rebuild`; comments assume dropping sender closes actor channel. |
| Public admin clone | `AdminHandle::service()` returns `Arc<AdminService>`, and `AdminService` stores `Option<SyncSender<RebuildRequest>>`. |
| Actor loop | `for req in receiver` exits only when all sender clones are dropped. |
| Batch scan | `run_rebuild` uses `ORDER BY logical_id LIMIT ? OFFSET ?`. |
| Double write | Writer double-writes changed nodes into staging while state is `PENDING`, `BUILDING`, or `SWAPPING`. |
| Dropped enqueue | `register_fts_property_schema_async` uses `try_send`; on failure it logs and leaves state `PENDING`. |
| Restart recovery | `recover_interrupted_rebuilds` marks `BUILDING` and `SWAPPING` failed, not `PENDING`. |

---

## Goals

- Engine drop must not depend on public `AdminService` clones being dropped.
- Async rebuild should be resumable and durably driven by database state.
- `PENDING` should mean queued or discoverable, never permanently orphaned.
- Rebuild enumeration must not skip rows under concurrent writes.
- Shutdown must be bounded and observable.
- Keep admin registration fast for large databases.

## Non-Goals

- Building a general job scheduler.
- Supporting parallel rebuilds for the same kind.
- Changing the public admin API shape unless required for safety.
- Guaranteeing an async rebuild survives process kill mid-batch without restart
  marking the in-flight work failed.

---

## Design

### 1. Replace channel-close shutdown with explicit cancellation

Do not rely on dropping all request senders. External `AdminService` clones make
that unbounded.

Add a runtime-owned shutdown token:

```rust
struct RebuildActor {
    thread_handle: Option<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
}
```

Actor loop:

```rust
loop {
    if shutdown.load(Ordering::Acquire) {
        break;
    }

    match receiver.recv_timeout(Duration::from_millis(250)) {
        Ok(req) => run_rebuild_with_cancellation(&mut conn, &req, &shutdown),
        Err(RecvTimeoutError::Timeout) => process_pending_rebuilds(&mut conn, &shutdown),
        Err(RecvTimeoutError::Disconnected) => process_pending_rebuilds(&mut conn, &shutdown),
    }
}
```

`Drop for RebuildActor`:

1. Set `shutdown = true`.
2. Join with normal `join()` after the actor observes cancellation.
3. If the actor is inside a long rebuild, it checks the token between batches and
   before swap.
4. If cancellation interrupts a rebuild, mark the state `FAILED` with
   `engine shutdown interrupted rebuild` and clear staging for that kind.

Rationale:

- Public sender clones no longer control actor lifetime.
- Shutdown is explicit and testable.
- The actor can stop even while request senders remain alive.

### 2. Separate public admin handle from actor lifetime

Keep `AdminService` usable for ordinary calls while the engine is alive, but make
rebuild submission fail cleanly after shutdown begins.

Introduce:

```rust
struct RebuildClient {
    sender: SyncSender<RebuildRequest>,
    shutdown: Arc<AtomicBool>,
}
```

`AdminService` stores `Option<RebuildClient>` instead of raw sender.

`RebuildClient::try_submit(req)` returns:

- `Ok(Submitted)` when enqueued.
- `Ok(PersistedPending)` when channel is full but durable pending state exists.
- `Err(EngineError::Bridge("engine is shutting down"))` when shutdown is set.

This preserves fast async registration while making the state explicit in logs
and tests.

### 3. Make `PENDING` durable work, not merely channel state

Treat `fts_property_rebuild_state` as the source of truth.

Actor startup and idle polling must call:

```sql
SELECT kind, schema_id
FROM fts_property_rebuild_state
WHERE state = 'PENDING'
ORDER BY started_at, kind
LIMIT 16;
```

For each row:

- Attempt an atomic claim:

```sql
UPDATE fts_property_rebuild_state
SET state = 'BUILDING', last_progress_at = ?
WHERE kind = ? AND schema_id = ? AND state = 'PENDING';
```

- Only run the rebuild if one row was updated.
- If a newer schema_id superseded the queued request, skip the stale request.

`register_fts_property_schema_async` still tries to send a prompt to wake the
actor, but correctness no longer depends on that prompt. If the channel is full,
the row remains discoverable by polling.

### 4. Keep restart recovery semantics precise

Update comments and behavior so terms match:

- `PENDING` is durable queued work and should survive restart.
- `BUILDING` and `SWAPPING` were in-flight at crash and should be marked
  `FAILED` with staging cleanup.
- Startup actor polling immediately picks up surviving `PENDING` rows.

This preserves the existing unit-test intent that `PENDING` survives restart,
but removes the orphaned-work risk.

### 5. Replace `OFFSET` pagination with keyset pagination

Current scan:

```sql
SELECT logical_id, properties
FROM nodes
WHERE kind = ? AND superseded_at IS NULL
ORDER BY logical_id
LIMIT ? OFFSET ?;
```

New scan:

```sql
SELECT logical_id, properties
FROM nodes
WHERE kind = ?
  AND superseded_at IS NULL
  AND logical_id > ?
ORDER BY logical_id
LIMIT ?;
```

Loop state:

```rust
let mut last_logical_id = String::new();
loop {
    let batch = read_batch_after(&last_logical_id, batch_size);
    if batch.is_empty() { break; }
    last_logical_id = batch.last().unwrap().0.clone();
    write_staging(batch);
}
```

Why this is correct enough:

- Deletes/upserts before `last_logical_id` cannot shift later rows out of the
  scan window.
- New or changed rows during `PENDING/BUILDING/SWAPPING` are covered by writer
  double-write to staging.
- A newly inserted logical ID lower than `last_logical_id` is covered by
  double-write because the schema and rebuild state already exist.
- Final swap still happens in one transaction.

### 6. Make final swap validate expected state

Before deleting live rows, assert this request still owns the rebuild:

```sql
SELECT state, schema_id
FROM fts_property_rebuild_state
WHERE kind = ?;
```

Proceed only when:

- `schema_id` equals request schema_id.
- `state` is `SWAPPING`.
- shutdown is not requested.

If a newer schema registration superseded this one, abort this run without
modifying live FTS rows; the newer `PENDING` row will be picked up separately.

### 7. Tokenizer alignment for async swap

The final swap must use the same table-creation helper described in
`design-0.5.4-projection-identity-and-tokenizer-hardening.md`. The actor should
resolve tokenizer for the kind before creating a missing table and should verify
shape compatibility before inserting staging rows.

---

## Compatibility

Public registration calls remain additive and fast. Existing `PENDING` rows in
older databases become actionable after upgrade because actor startup polling
will process them.

Operational semantics change:

- A channel-full async registration is no longer a silent manual-retry condition.
- Engine shutdown can mark an in-flight rebuild `FAILED`; callers can re-register
  or rely on repair/rebuild commands.
- Startup will process old `PENDING` rows, which may create background work on
  first open after upgrade.

---

## Test Plan

Add shutdown tests:

- Hold `let svc = engine.admin().service(); drop(engine);` and assert drop
  returns within a timeout.
- Hold multiple `AdminService` clones and verify actor thread exits.
- Attempt async registration after shutdown starts and assert clean error.

Add durable pending tests:

- Seed a `PENDING` row manually, open engine, and assert actor processes it to
  `COMPLETE` or `FAILED` rather than leaving it unchanged.
- Fill the channel or inject a `try_send` failure, then assert polling processes
  the persisted row.
- Verify `BUILDING` and `SWAPPING` are still marked `FAILED` on restart.

Add keyset rebuild tests:

- Seed > initial batch size nodes.
- During rebuild, upsert/delete a node with logical ID before the current cursor.
- Assert all active unchanged nodes are present after final swap.
- Insert a new node with logical ID lower than current cursor during rebuild and
  assert double-write includes it.

Add supersession tests:

- Register schema A async, then schema B async while A is building.
- Assert A does not swap over B and final FTS rows match schema B.

---

## HITL Gates

No immediate question blocks this design. Ask for human input during
implementation if any of these tradeoffs becomes material:

- Whether shutdown should finish the current rebuild instead of cancelling after
  the current batch.
- Whether channel-full registration should return success with `PENDING` or a
  warning/error surface to SDK callers.
- Whether startup processing of old `PENDING` rows should happen automatically
  or require an explicit repair command.
