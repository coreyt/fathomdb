# Engine Lifecycle Management — Design Note

Issue: missing explicit shutdown, WAL checkpoint, and resource cleanup across
Rust, Python, and Go layers.

## Problem

The engine has no explicit lifecycle management. When an `EngineRuntime` is
dropped (Rust) or garbage-collected (Python), cleanup is entirely implicit:

1. **Writer thread not joined.** `WriterActor` drops the `SyncSender` (closing
   the channel) but never calls `.join()` on the thread handle. If the process
   exits before the thread drains its last message, the SQLite connection may
   not close cleanly.

2. **No WAL checkpoint on close.** SQLite performs a passive checkpoint when the
   *last* connection closes, but only if it can acquire an exclusive lock. If
   reader connections are still open when the writer closes (or vice versa), no
   checkpoint occurs and the WAL file persists.

3. **No Python lifecycle control.** Python users cannot explicitly close the
   engine. There is no `close()`, no context manager (`with Engine(...)`), and
   no `atexit` registration. GC timing is non-deterministic; in CPython the
   destructor usually runs promptly, but this is an implementation detail, not a
   guarantee.

4. **No graceful drain.** There is no mechanism to wait for in-flight writes to
   complete before shutdown. The bounded channel (capacity 256) may contain
   queued writes that are silently dropped if the process exits.

### What is NOT at risk

Committed data is safe. SQLite's WAL design ensures that any transaction that
received a successful `COMMIT` is durable in the WAL file. Reopening the
database recovers WAL content automatically. The risk is limited to:

- writes queued in the channel but not yet committed
- stale main database file (WAL not checkpointed) after unclean shutdown
- WAL/shm file litter after crash

## Investigation

### What SQLite does on close

`sqlite3_close()` (used by rusqlite's `Drop`) performs a **passive checkpoint**
when it is the last connection. This transfers WAL content to the main database
file and deletes the WAL and shm files. If other connections are still open, the
checkpoint is skipped and the WAL persists.

**Implication:** shutdown ordering matters. If we close reader connections
before the writer connection, the writer's close will be the last connection and
SQLite will auto-checkpoint. The current `EngineRuntime` field order already
achieves this by accident: Rust drops fields in declaration order, and the
fields are `coordinator` (readers), `writer`, `admin`.

### What rusqlite exposes

`rusqlite::Connection` provides:
- `Drop` impl: calls `sqlite3_close()`, silently ignoring errors
- `fn close(self) -> Result<(), (Self, Error)>`: explicit close with error
  reporting (returns the connection back on failure)

### What PyO3 supports

- `__del__`: **not supported** by PyO3
- `__enter__` / `__exit__`: supported as regular `#[pymethods]`
- Rust `Drop` on `#[pyclass]`: runs when Python GC collects the object, but
  timing is non-deterministic
- `frozen` pyclass with interior mutability (`Mutex<Option<T>>`): fully
  supported, idiomatic for close-pattern on frozen types

### What Python users expect

Every major Python database library follows the same pattern:
- `sqlite3`: `conn.close()` + context manager for transactions
- `psycopg3`: `pool.close(timeout=5.0)` + context manager
- `SQLAlchemy`: `engine.dispose()` + pool context manager
- All: operations after close raise an error

### Rust shutdown consensus

From The Rust Book (ch.21), matklad's "Join Your Threads", `jod-thread` crate,
and rust-analyzer's `thread_worker`:
- Drop should join the worker thread (two-phase: drop sender, then join)
- If already panicking, suppress the worker's panic (avoid double-panic abort)
- Otherwise, propagate the worker's panic via `resume_unwind`

## Design

### Layer 1: WriterActor — Graceful Drain and Join on Drop

**Change:** Add a `Drop` impl to `WriterActor` that drains the channel and
joins the writer thread.

```rust
impl Drop for WriterActor {
    fn drop(&mut self) {
        // Phase 1: Close the channel by dropping the sender.
        // The writer thread's `for msg in receiver` loop will exit after
        // finishing any in-progress message.
        //
        // sender must be dropped BEFORE join, or we deadlock.
        // ManuallyDrop lets us drop it independently of the struct.
        unsafe { ManuallyDrop::drop(&mut self.sender) };

        // Phase 2: Join the writer thread.
        if let Some(handle) = self.thread_handle.take() {
            match handle.join() {
                Ok(()) => {}
                Err(payload) => {
                    if std::thread::panicking() {
                        // Already unwinding — suppress to avoid abort.
                        eprintln!("fathomdb-writer panicked during shutdown \
                                   (suppressed: caller already panicking)");
                    } else {
                        std::panic::resume_unwind(payload);
                    }
                }
            }
        }
    }
}
```

**Field change:** Wrap `sender` in `ManuallyDrop<SyncSender<WriteMessage>>` so
it can be dropped independently. This avoids wrapping it in `Option` which
would require `.as_ref().unwrap()` on every `send` call in the hot path.

```rust
pub struct WriterActor {
    sender: ManuallyDrop<SyncSender<WriteMessage>>,
    thread_handle: Option<thread::JoinHandle<()>>,
    provenance_mode: ProvenanceMode,
}
```

**Behavior:**
- Normal exit: sender drops, writer finishes current message, loop ends, thread
  exits, SQLite connection closes via rusqlite `Drop`, join succeeds.
- Writer panic: `catch_unwind` in `writer_loop` already handles per-message
  panics. A panic that escapes the loop (e.g. connection open failure) is
  propagated by `resume_unwind` in `Drop`.
- Process abort (`SIGKILL`, `abort()`): `Drop` does not run. SQLite WAL
  recovery handles this on next open — this is by design and cannot be
  improved.

**What this does NOT do:** It does not add an explicit WAL checkpoint. When the
writer thread exits and its `rusqlite::Connection` drops, `sqlite3_close()`
runs. If this is the last connection (readers already dropped due to field
ordering), SQLite performs its built-in passive checkpoint. No additional
checkpoint call is needed.

### Layer 2: EngineRuntime — Document Field Ordering Invariant

The current field order in `EngineRuntime` is:

```rust
pub struct EngineRuntime {
    coordinator: ExecutionCoordinator,  // readers — dropped first
    writer: WriterActor,               // writer — dropped second
    admin: AdminHandle,                // admin — dropped last
}
```

Rust drops fields in declaration order. This means readers close before the
writer, ensuring the writer's `sqlite3_close()` is the last connection and
triggers SQLite's automatic passive checkpoint.

**Change:** Add a doc comment codifying this invariant:

```rust
/// Core engine runtime.
///
/// # Drop order invariant
///
/// Fields are ordered so that `coordinator` (reader connections) drops before
/// `writer` (writer thread + connection). This ensures the writer's
/// `sqlite3_close()` is the last connection to the database, which triggers
/// SQLite's automatic passive WAL checkpoint and WAL/shm file cleanup.
/// Do not reorder these fields.
pub struct EngineRuntime {
    coordinator: ExecutionCoordinator,
    writer: WriterActor,
    admin: AdminHandle,
}
```

### Layer 3: Python — close() + Context Manager

**Rust side (`python.rs`):**

Change `EngineCore` to hold `Mutex<Option<Engine>>` instead of bare `Engine`.
Add `close()`, `__enter__`, and `__exit__` methods.

```rust
#[pyclass(frozen)]
pub struct EngineCore {
    engine: Mutex<Option<Engine>>,
}

#[pymethods]
impl EngineCore {
    /// Close the engine, flushing pending writes and releasing all resources.
    ///
    /// This method is idempotent — calling it on an already-closed engine is
    /// a no-op.
    pub fn close(&self, py: Python<'_>) {
        py.allow_threads(|| {
            let mut guard = self.engine.lock().expect("engine mutex poisoned");
            // Drop the engine (triggers WriterActor::drop -> join)
            let _ = guard.take();
        });
    }

    pub fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    pub fn __exit__(
        &self,
        py: Python<'_>,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_val: Option<&Bound<'_, PyAny>>,
        _exc_tb: Option<&Bound<'_, PyAny>>,
    ) -> bool {
        self.close(py);
        false  // do not suppress exceptions
    }
}
```

**Helper for all methods:**

```rust
impl EngineCore {
    fn with_engine<F, R>(&self, f: F) -> PyResult<R>
    where
        F: FnOnce(&Engine) -> PyResult<R>,
    {
        let guard = self.engine.lock().expect("engine mutex poisoned");
        match guard.as_ref() {
            Some(engine) => f(engine),
            None => Err(FathomError::new_err("engine is closed")),
        }
    }
}
```

Every existing method changes from `self.engine.coordinator()...` to
`self.with_engine(|engine| { engine.coordinator()... })`. The `Mutex` is
uncontended in normal use (single-threaded access to the Option check; the
actual I/O runs outside the lock via `allow_threads`).

**Python side (`_engine.py`):**

```python
class Engine:
    def close(self) -> None:
        """Close the engine, flushing pending writes and releasing resources.

        Idempotent — safe to call multiple times.
        """
        self._core.close()

    def __enter__(self) -> "Engine":
        return self

    def __exit__(self, *exc) -> bool:
        self.close()
        return False
```

**Post-close behavior:** Any method call after `close()` raises `FathomError:
engine is closed`. This matches `sqlite3.ProgrammingError` and
`psycopg3.PoolClosed` conventions.

### Layer 4: Crash Recovery (No Code Change Needed)

SQLite's WAL recovery is automatic and requires no engine changes:

- On next `open()`, SQLite detects a stale WAL file and replays committed
  transactions into the main database.
- The `SchemaManager::bootstrap()` call already runs after connection open,
  which triggers SQLite's recovery path.
- The `fathom-integrity` Go tool already inspects WAL files and recommends
  `PRAGMA wal_checkpoint(TRUNCATE)` when frames exceed the threshold.

**No explicit crash-recovery code is needed in the engine.** SQLite handles it.

### What This Design Explicitly Does NOT Do

1. **No explicit `PRAGMA wal_checkpoint` on close.** SQLite's built-in passive
   checkpoint on last-connection-close is sufficient. Adding an explicit
   checkpoint would add latency, could block on concurrent readers, and provides
   no additional durability guarantee (committed data is already safe in WAL).

2. **No `atexit` registration.** The `close()` method and context manager give
   Python users explicit control. Automatic `atexit` registration is
   surprising, may conflict with user-managed shutdown sequences, and is
   unnecessary when `Drop` serves as the safety net.

3. **No timeout on thread join.** The writer thread processes one message at a
   time with a 5-second SQLite busy timeout. In the worst case, `Drop` blocks
   for one message's processing time. This is acceptable for a shutdown path.
   Adding a timeout would leave the thread detached and potentially corrupt
   state.

4. **No explicit Go changes.** `fathom-integrity` does not hold persistent
   database connections. It inspects and repairs from the outside. No lifecycle
   changes are needed.

## Shutdown Sequence Summary

```
Python: engine.close()  /  with Engine(...) as db:  /  GC collects EngineCore
            |
            v
Rust:   EngineCore.close() -> Mutex<Option<Engine>>.take()
            |
            v
        Engine dropped -> EngineRuntime dropped
            |
            +-- coordinator dropped (ReadPool: N connections close)
            |       sqlite3_close() x N  (not last connection, no checkpoint)
            |
            +-- writer dropped (WriterActor::drop)
            |       1. ManuallyDrop::drop(sender)  -- closes channel
            |       2. writer_loop exits            -- finishes current msg
            |       3. rusqlite::Connection drops   -- sqlite3_close()
            |          LAST connection: SQLite passive checkpoint runs
            |          WAL + shm files deleted
            |       4. handle.join()                -- propagates panic if any
            |
            +-- admin dropped (AdminHandle: no persistent connections)
```

## Files To Change

| File | Change |
|------|--------|
| `crates/fathomdb-engine/src/writer.rs` | `ManuallyDrop` on sender, `Drop` impl |
| `crates/fathomdb-engine/src/runtime.rs` | Doc comment on field ordering invariant |
| `crates/fathomdb/src/python.rs` | `Mutex<Option<Engine>>`, `close()`, `__enter__/__exit__`, `with_engine` helper |
| `python/fathomdb/_engine.py` | `close()`, `__enter__/__exit__` |
| `python/tests/test_bindings.py` | Test: close() idempotency, post-close error, context manager |
| `crates/fathomdb-engine/src/runtime.rs` | Test: Drop joins writer thread (verify no thread leak) |

## Risks

| Risk | Mitigation |
|------|------------|
| `ManuallyDrop` requires `unsafe` in `Drop` | Single call site, well-understood pattern. Alternative (`Option`) penalizes every hot-path `send` with an unwrap. |
| `Mutex<Option<Engine>>` adds lock acquisition to every Python call | Uncontended mutex lock is ~20ns. The actual I/O (SQLite query) is orders of magnitude slower. Unmeasurable in practice. |
| `Drop::drop` blocks on thread join | Bounded by one message's processing time (writer processes messages one at a time). Acceptable for shutdown. |
| Reordering `EngineRuntime` fields breaks WAL checkpoint | Doc comment + integration test guard against this. |
