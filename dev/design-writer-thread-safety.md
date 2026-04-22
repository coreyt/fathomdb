# Design: Writer Thread Safety Hardening

## Purpose

Address three verified production-readiness findings related to the writer
thread and write channel: panic resilience (C-1), backpressure (C-2), and
request size validation (H-5).

---

## C-1. Writer Thread Panic Recovery

### Current State

`crates/fathomdb-engine/src/writer/mod.rs`

The writer thread's main loop calls `resolve_and_apply()` without
`catch_unwind`. If that function panics, the thread terminates silently.
Any caller already waiting on `reply_rx.recv()` (line 287) blocks
indefinitely — there is no timeout, no error reply, and no recovery path.
Subsequent `submit()` calls detect the dead thread via the closed channel
and return `WriterRejected`, but the already-blocked caller hangs forever.

### Design

**1. Wrap `resolve_and_apply` in `catch_unwind`.**

```rust
let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
    resolve_and_apply(&conn, &msg.prepared, &msg.options)
}));

let reply = match result {
    Ok(Ok(receipt)) => Ok(receipt),
    Ok(Err(engine_err)) => Err(engine_err),
    Err(_panic) => {
        // Log the panic. Do NOT re-panic — the writer loop must survive.
        Err(EngineError::WriterRejected(
            "writer thread panic during resolve_and_apply".into(),
        ))
    }
};

let _ = msg.reply_tx.send(reply);
```

`AssertUnwindSafe` is acceptable here because:
- The `Connection` is reset by the transaction rollback that SQLite
  performs automatically when the statement-execution stack unwinds.
- `PreparedWrite` is a read-only input consumed by value.
- The writer thread must continue serving subsequent requests even after
  a single-request panic.

**2. Add a timeout to `reply_rx.recv()` in `submit()`.**

Replace `reply_rx.recv()` with `reply_rx.recv_timeout(Duration::from_secs(30))`.
If the timeout fires, return `EngineError::WriterRejected("write timed out
waiting for writer thread reply")`.

30 seconds is generous for any single SQLite write transaction. If the
writer is truly stuck (not panicked but hung on disk I/O or a lock), the
timeout prevents caller threads from accumulating forever.

**3. Detect writer thread death in `submit()`.**

Before sending on the channel, check if the writer thread's `JoinHandle`
has completed. If so, return `WriterRejected` immediately rather than
sending a message that will never be read. This requires storing the
`JoinHandle` in the `Writer` struct.

### Considerations

- `catch_unwind` does not catch stack overflows or `abort`-on-panic
  configurations. Document that the engine requires `panic = "unwind"` in
  the release profile.
- After a caught panic, the SQLite connection may have an open transaction
  that was not committed. SQLite automatically rolls back when the
  statement handle is dropped, so no explicit rollback is needed. However,
  adding an explicit `conn.execute_batch("ROLLBACK")` with an
  `unwrap_or(())` guard is defensive.

---

## C-2. Bounded Write Channel

### Current State

`crates/fathomdb-engine/src/writer/mod.rs`

`mpsc::channel()` creates an unbounded channel. Under sustained write load,
the in-memory queue grows without limit. There is no backpressure signal
to callers.

### Design

Switch to `std::sync::mpsc::sync_channel(capacity)` with a configurable
capacity (default: 256).

```rust
let (sender, receiver) = mpsc::sync_channel::<WriteMessage>(256);
```

`sync_channel` blocks the sender when the channel is full, providing
natural backpressure. This means `submit()` becomes a blocking call when
the writer is saturated — which is the correct behavior. Callers slow down
proportionally to the writer's throughput.

### Capacity choice

- 256 messages at ~10 KB each = ~2.5 MB maximum queue. Well within
  acceptable memory bounds.
- Under normal single-writer patterns (the expected case for a local
  agent datastore), the channel rarely has more than a few messages
  queued.
- The capacity should be a `WriterOptions` field so operators can tune it
  if needed.

### Interaction with C-1

If the writer thread panics and is recovered via `catch_unwind`, blocked
senders on a full `sync_channel` will resume normally when the writer
drains the next message. If the writer thread dies completely, blocked
senders will get a `SendError` and return `WriterRejected`.

---

## H-5. WriteRequest Size Validation

### Current State

`crates/fathomdb-engine/src/writer/mod.rs`

`WriteRequest` contains 11 `Vec` fields with no length limits. A single
request with 100K nodes allocates hundreds of megabytes in
`prepare_write()` before reaching the writer thread.

### Design

Add a `WriteRequestLimits` struct with per-field maximums, checked at the
top of `prepare_write()`:

```rust
pub struct WriteRequestLimits {
    pub max_nodes: usize,          // default: 10_000
    pub max_edges: usize,          // default: 10_000
    pub max_chunks: usize,         // default: 50_000
    pub max_retires: usize,        // default: 10_000
    pub max_runtime_items: usize,  // default: 10_000
    pub max_operational: usize,    // default: 10_000
    pub max_total_items: usize,    // default: 100_000
}
```

Validation in `prepare_write()`:

```rust
fn validate_request_size(req: &WriteRequest, limits: &WriteRequestLimits)
    -> Result<(), EngineError>
{
    if req.nodes.len() > limits.max_nodes {
        return Err(EngineError::InvalidWrite(
            format!("request contains {} nodes, limit is {}", req.nodes.len(), limits.max_nodes)
        ));
    }
    // ... similar for each field ...

    let total = req.nodes.len() + req.edges.len() + req.chunks.len()
        + req.node_retires.len() + req.edge_retires.len()
        + req.runs.len() + req.steps.len() + req.actions.len()
        + req.vec_inserts.len() + req.operational_writes.len()
        + req.optional_backfills.len();
    if total > limits.max_total_items {
        return Err(EngineError::InvalidWrite(
            format!("request contains {} total items, limit is {}", total, limits.max_total_items)
        ));
    }
    Ok(())
}
```

These limits are not just DoS protection — they also bound the transaction
duration. A 100K-item write holds `BEGIN IMMEDIATE` for the entire
insertion, blocking all other writers. Keeping transactions short is a
correctness concern for a local concurrent datastore.

### Where limits live

`WriteRequestLimits` should be a field on `EngineOptions` (or a new
`WriterOptions`) so the application can configure them at engine
construction time. The defaults should be safe for typical agent workloads.

---

## Test Plan

- **C-1:** Unit test that submits a write whose `resolve_and_apply` will
  panic (e.g. inject a poison flag). Verify the caller receives
  `WriterRejected`, not a hang. Verify a subsequent write succeeds.
- **C-2:** Stress test that submits writes faster than the writer can
  process them. Verify memory stays bounded (no OOM) and that senders
  block rather than queue indefinitely.
- **H-5:** Unit test that constructs a `WriteRequest` exceeding each
  limit. Verify `prepare_write()` returns `InvalidWrite` without
  touching the database.
