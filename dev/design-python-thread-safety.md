# Python EngineCore Thread Safety — Design Note

Issue: #30 — `EngineCore` is `!Send`, panics when accessed from HTTP handler threads.

## Problem

The PyO3 `EngineCore` is marked `#[pyclass(unsendable)]`. Any Python application
that creates an `Engine` on one thread and accesses it from another gets a panic.
This blocks every standard Python HTTP server pattern (ThreadingHTTPServer, Flask,
FastAPI, WSGI).

## Investigation

### EngineRuntime is Send + Sync

Compiler-verified. The internal types are all thread-safe:

- `ReadPool`: `Vec<Mutex<Connection>>` — Mutex makes each connection Sync
- `WriterActor`: `SyncSender<WriteMessage>` + `Option<JoinHandle<()>>` — all Send + Sync
- `AdminHandle`: `Arc<AdminService>` — creates fresh connections per call, no stored connection
- No `unsafe impl Send/Sync` anywhere in the codebase
- No bare `rusqlite::Connection` stored as a field (they are always behind Mutex or created per-call)

### The code already releases the GIL

Nearly every `#[pymethods]` method uses `py.allow_threads()`:

```rust
pub fn execute_ast(&self, py: Python<'_>, ast_json: &str) -> PyResult<String> {
    let compiled = ...;
    let rows = py
        .allow_threads(|| self.engine.coordinator().execute_compiled_read(&compiled))
        .map_err(map_engine_error)?;
    ...
}
```

This means the code was designed for concurrent `&self` access across threads.
The `unsendable` marker contradicts the design.

### rusqlite::Connection is Send but not Sync

`rusqlite::Connection` implements `Send` (can be moved between threads) but not
`Sync` (cannot be shared via `&Connection`). FathomDB never stores a bare
Connection — they are all behind `Mutex` (ReadPool) or created per-call
(AdminService). The writer thread owns its connection privately via the spawned
thread closure.

## Options

### Option A: Remove `unsendable` (one-line fix)

Standard PyO3 pattern for types that are internally synchronized.

```rust
#[pyclass]  // was: #[pyclass(unsendable)]
pub struct EngineCore { engine: Engine }
```

**Pros:** Minimal change. Concurrent reads work immediately because
`allow_threads` is already called. Writes serialize naturally through the
channel.

**Cons:** PyO3 internally uses RefCell-like borrow tracking on `&self`, adding a
small runtime check per method call. If a future field breaks `Sync`, the error
surfaces as a confusing PyO3 trait-bound failure.

**Risk:** Low. This is removing an over-constraint. The GIL + `allow_threads` +
internal Mutex/channel already provide correct synchronization.

### Option B: `#[pyclass(frozen)]` with compile-time Sync guarantee

Pattern used by high-performance PyO3 libraries (pyo3-polars, robyn). `frozen`
tells PyO3 the type is never mutated through `&mut self`, so PyO3 skips internal
RefCell borrow tracking entirely. Since all EngineCore methods take `&self`
(never `&mut self`), this is a perfect fit.

```rust
#[pyclass(frozen)]  // was: #[pyclass(unsendable)]
pub struct EngineCore { engine: Engine }
```

PyO3's `frozen` requires `Sync` at compile time (since `&self` can be held from
multiple threads without GIL protection).

**Pros:** Zero runtime overhead for borrow checking. Compile-time guarantee that
Engine stays Sync. Documents intent: "this type is immutable and concurrent."

**Cons:** If a future field breaks `Sync`, the fix requires making it Sync (cannot
fall back to runtime checks). Slightly less conventional than bare `#[pyclass]`.

**Risk:** Low. All methods are already `&self`. The `frozen` attribute codifies
the existing invariant.

### Option C: `frozen` + explicit Send + Sync static assertion (recommended)

Same as Option B, but add a compile-time assertion in the engine crate so the
thread-safety contract is visible and protected at the source — not just at the
PyO3 boundary.

```rust
// crates/fathomdb-engine/src/runtime.rs
const _: () = {
    fn _assert_send_sync<T: Send + Sync>() {}
    fn _assertions() { _assert_send_sync::<EngineRuntime>(); }
};
```

```rust
// crates/fathomdb/src/python.rs
#[pyclass(frozen)]
pub struct EngineCore { engine: Engine }
```

**Pros:** Any future change that breaks Send or Sync (adding a Cell, Rc, or bare
Connection field) fails at compile time with a clear error in runtime.rs — not a
confusing PyO3 trait-bound error. Makes the design contract explicit for Rust
contributors who may not know about the Python binding.

**Cons:** Two-file change instead of one.

**Risk:** Lowest. Defensive engineering — protects the invariant at the layer
that owns it.

## Behavior After Fix (all options)

- Engine can be shared across Python threads without panic
- Concurrent reads work in parallel: each `allow_threads` block releases the GIL,
  another worker thread picks up a different ReadPool connection
- Writes serialize through the bounded SyncSender channel as designed
- Admin operations create ephemeral connections, safe from any thread
- Memex's ThreadingHTTPServer pattern works: HTTP workers query fathomdb
  concurrently while Telegram poller and scheduler run on other threads

## Files

- `crates/fathomdb/src/python.rs:35` — `#[pyclass(unsendable)]` to change
- `crates/fathomdb-engine/src/runtime.rs` — static assertion (Option C only)
