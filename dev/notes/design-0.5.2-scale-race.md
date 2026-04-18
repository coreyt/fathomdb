# Design: scale.rs heavy-writer panic surface (0.5.2 Item 1)

**Release:** 0.5.2
**Scope item:** Item 1 from `dev/notes/0.5.2-scope.md`
**Breaking:** No (test-only change; no runtime API change)

---

## Problem

`crates/fathomdb/tests/scale.rs:1494` calls
`handle.join().expect("heavy writer joins")` on spawned heavy-writer
threads at the tail of
`property_fts_rebuild_then_search_remains_correct_after_heavy_writes`.
Under sustained parallel test load (`cargo test --workspace` with
`--test-threads >= 4`) a writer thread can panic from SQLite WAL lock
contention or background-write assertion timing. `.expect()`:

- loses the thread's panic payload (replaces it with its own static string)
- doesn't identify which of N writer threads panicked
- produces a test failure message that says nothing about whether this
  is an engine bug, a test-isolation bug, or an I/O stall

Today the test passes reliably in isolation and under `--test-threads=4`
or smaller, but anyone running the full suite sees intermittent
unexplained failures.

---

## Current state (anchored to 0.5.1 HEAD)

`crates/fathomdb/tests/scale.rs:1490` (test body, abridged):

```rust
let handles: Vec<JoinHandle<...>> = /* three writer threads */;

// ... rebuild runs concurrently ...

for handle in handles {
    handle.join().expect("heavy writer joins");
}
```

The 2026-04-18 investigation agent confirmed:

- 3/3 isolated runs pass
- `cargo test -p fathomdb --test scale -- --test-threads=4` passes
- Shared state review: unique tempfiles per run, no global mutex
  contention, `COUNTER: AtomicU64` is the only shared static and is
  strictly additive
- WAL lock acquisition at `crates/fathomdb-engine/src/admin/fts.rs:280`
  uses an `IMMEDIATE` transaction that blocks up to 5 s on busy timeout

Root cause hypothesis from the investigation: one of the three heavy
writer threads can panic under sustained contention (writer-actor
internal assertion, SQLite busy timeout exhaustion, or a genuine engine
bug under stress). `.expect()` propagates the panic payload lost, and
the main test thread fails with `panicked at "heavy writer joins"`.

---

## Goal

Make test failures produce an actionable diagnostic:

- identify which writer thread panicked
- preserve the panic payload (message or downcastable type)
- distinguish "writer thread panicked" from "writer thread returned
  `Err(...)`" from "test timed out"

No attempt to fix the underlying contention; that is a separate
investigation.

---

## Design

### New helper: `join_writer_or_diagnose`

Location: top of `crates/fathomdb/tests/scale.rs` (test-crate-local; no
engine API change).

```rust
/// Join a heavy-writer thread handle and produce a useful assertion
/// failure if the thread panicked, instead of re-raising the opaque
/// `JoinError` that `.expect()` would produce.
///
/// Arguments:
/// * `label` — human-readable identifier for the writer (e.g. `"writer-0"`).
/// * `handle` — the `JoinHandle` returned by `thread::spawn`.
fn join_writer_or_diagnose<T>(
    label: &str,
    handle: thread::JoinHandle<T>,
) -> T {
    match handle.join() {
        Ok(value) => value,
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                (*s).to_owned()
            } else if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else {
                format!("<non-string panic payload: {:p}>", &*payload)
            };
            panic!("{label} thread panicked: {msg}");
        }
    }
}
```

### Call-site change

Before:

```rust
for handle in handles {
    handle.join().expect("heavy writer joins");
}
```

After:

```rust
for (idx, handle) in handles.into_iter().enumerate() {
    let label = format!("writer-{idx}");
    let _ = join_writer_or_diagnose(&label, handle);
}
```

The `let _` is intentional — writers return a `Result` whose `Err`
variant is already logged via `tracing` inside the writer loop (see
scale.rs:1452-1458, which panics inside the writer on error). The join
helper's job is panic surface only.

### TDD approach

1. **Red test for the helper** (new test in scale.rs, not gated):

   ```rust
   #[test]
   fn join_writer_or_diagnose_surfaces_string_panic() {
       let h = std::thread::spawn(|| panic!("boom-42"));
       let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
           join_writer_or_diagnose("writer-x", h);
       }));
       let msg = err.unwrap_err().downcast::<String>().unwrap();
       assert!(msg.contains("writer-x"), "{msg}");
       assert!(msg.contains("boom-42"), "{msg}");
   }
   ```

   Fails until the helper is added (fn not defined).

2. **Green**: add helper; red test passes.

3. **Refactor**: replace the `handle.join().expect(...)` loop in the
   existing heavy-writer test with the helper.

4. **Regression gate**: run
   `cargo test -p fathomdb property_fts_rebuild_then_search_remains_correct -- --nocapture`
   three times. Each run still passes in isolation.

---

## Out of scope

- Fixing the underlying contention that causes the panic. The investigation
  could not reproduce it reliably; we need a repro before we can fix.
- Adding a `tracing` span around the join. Writers already have their own
  spans; the helper only runs after all writers return.
- Timing out the join. Thread-level timeouts in std Rust require extra
  machinery (channels + `recv_timeout`); not worth the complexity for a
  test diagnostic.

---

## Acceptance

- Running the test in isolation: pass.
- Running the full workspace suite: if the flake fires, the failure
  message now reads
  `writer-N thread panicked: <original payload>` instead of
  `panicked at "heavy writer joins"`.
- The new unit test for the helper passes on first run and on rerun.
- No other scale.rs test is modified.

---

## Cypher enablement note

N/A. Test-only change.
