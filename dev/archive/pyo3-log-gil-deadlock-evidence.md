# pyo3-log GIL Deadlock: Remaining Issue After cf0b190

## Summary

The fix in `cf0b190` (Fix pyo3-log GIL deadlock in supports_vector_mode probe) addressed
one specific call site, but the broader deadlock pattern remains. Any Rust code path that
emits a log message through pyo3-log while the GIL is contended by another engine instance
can trigger the same deadlock.

## Reproduction

The deadlock occurs when:
1. Two `Engine` instances are open on the same `fathom.db` file
2. Python's root logger is at `DEBUG` level with an active handler
3. Either engine emits a DEBUG/INFO log message from Rust

### Minimal reproducer (hangs indefinitely)

```python
import logging
logging.basicConfig(level=logging.DEBUG)  # <-- required to trigger

from fathomdb import Engine

engine1 = Engine.open("test.db")   # first engine
engine2 = Engine.open("test.db")   # second engine — hangs here or on next operation
engine2.close()
engine1.close()
```

### Works fine (no deadlock)

```python
import logging
logging.basicConfig(level=logging.WARNING)  # suppress DEBUG

from fathomdb import Engine

engine1 = Engine.open("test.db")
engine2 = Engine.open("test.db")   # OK — no DEBUG logs processed
engine2.close()
engine1.close()
```

## Context: How Memex Hit This

Memex service startup:
1. `cli.main()` calls `setup_logging()` which sets root logger to DEBUG
2. `MemexAPI.create()` opens `FathomStore.open(fathom.db)` — engine #1
3. `startup_post()` → `backup_all()` opens `FathomStore.open(fathom.db)` — engine #2
4. **Deadlock** — service never reaches HTTP bind, no error, no log output after backup

The deadlock was deterministic and reproduced on every service restart for ~2 hours of
debugging before being isolated to the logging level interaction.

## Current Workaround in Memex

```python
# logging_config.py — suppress fathomdb loggers to WARNING
for name in (..., "fathomdb_engine", "fathomdb_schema"):
    logging.getLogger(name).setLevel(logging.WARNING)
```

This prevents Python handlers from processing fathomdb DEBUG/INFO messages, avoiding the
GIL contention in pyo3-log. The workaround is fragile — any code path that calls
`Engine.open()` before `setup_logging()` or with a different logging configuration will
hit the deadlock.

## Log Messages That Trigger It

The schema bootstrap logs are the most prolific:
```
INFO fathomdb_schema.bootstrap: schema bootstrap: version check current_version=13 engine_version=13
```

These are emitted ~10 times per `Engine.open()` call. With two engines open, these
messages fire concurrently from different Rust threads through pyo3-log, and the GIL
acquisition in pyo3-log deadlocks.

## Suggested Fix

The `cf0b190` fix addressed `supports_vector_mode` specifically. The same pattern exists
in every Rust `log::info!()` / `log::debug!()` call that can run while another engine's
thread also logs. The fix should be at the pyo3-log integration level — either:

1. Use `pyo3_log::try_init()` with a filter that drops DEBUG/INFO before they reach Python
2. Buffer log messages and flush them on the calling thread only (avoid cross-thread GIL contention)
3. Use a lock-free logging approach that doesn't require the GIL

## Environment

- Platform: Linux 5.10.120-tegra (aarch64, Jetson Orin)
- Python: 3.12.13
- fathomdb: 0.1.0 (post cf0b190)
- Date: 2026-03-30

---

## Resolution (d09deb4)

**Root cause confirmed.** `EngineCore::open()` held the GIL during the entire
`Engine::open(options)` call, which bootstraps the schema and emits ~13 INFO
tracing events per engine open. With two engines, the second open's bootstrap
events fired through pyo3-log while the first engine's writer thread also
attempted to log — pyo3-log's GIL acquisition on the writer thread deadlocked
against the main thread holding the GIL.

**Fix:** Added `py.allow_threads(|| Engine::open(options))` in
`EngineCore::open()`, matching the pattern already used by `close()`,
`submit_write()`, `execute_ast()`, and all other engine operations that call
into Rust. The GIL is released before any Rust code runs, so pyo3-log can
safely acquire it from any thread.

**Verified:** Memex's exact reproducer (two engines with `DEBUG` logging)
completes without deadlock. All 38 Python tests pass. The CI python-test
job (which previously hung indefinitely) now completes.

**Memex action:** The `setup_logging()` workaround that suppresses
`fathomdb_engine` and `fathomdb_schema` loggers to WARNING is no longer
needed. You can restore DEBUG-level logging for fathomdb loggers if desired.

**Broader hardening:** The same `py.allow_threads()` pattern is already
applied to every other engine operation (`close`, `submit_write`,
`execute_ast`, `execute_grouped_ast`, `touch_last_accessed`,
`check_integrity`, `check_semantics`, `rebuild_projections`, `trace_source`,
`excise_source`, `safe_export`, and all operational collection methods).
`compile_ast` and `compile_grouped_ast` are pure computation with no DB
access or thread interaction, so they are safe without GIL release.

The remaining deadlock surface is **implicit engine drop via Python GC** —
if an engine is never explicitly closed, Python's garbage collector triggers
Rust `Drop` while holding the GIL, which joins the writer thread, which may
emit a shutdown log event through pyo3-log. The fix in `cf0b190` addressed
`supports_vector_mode()` specifically; the general recommendation is: always
use `engine.close()` or the context manager (`with Engine.open(...) as e:`)
rather than relying on GC.

A regression test covering the multi-engine DEBUG-logging scenario has been
added to the Python test suite.

---

## Memex Follow-up: Fix Not Yet Effective (2026-03-30 15:02)

After `d09deb4`, Memex rebuilt the `.so` (`uv pip install --reinstall --no-cache -e`)
and confirmed the 3.12 `.so` timestamp updated to 15:02. However:

- **Bare dual-engine test passes** — `Engine.open()` twice with `DEBUG` logging completes.
- **Full `memex service` still deadlocks** — hangs after backup (same symptom as before).

The full service path is: `load_dotenv()` → `setup_logging(console=False)` →
`FathomStore.open()` (engine #1) → `FathomMemexStore()` facade → `RuntimeStateRegistry` →
`startup_post()` → `backup_all()` → `FathomStore.open()` (engine #2). The backup engine
opens and closes cleanly in isolation, but the full call chain still hangs.

**Possible explanations:**
1. `uv pip install -e` may not trigger `maturin develop` — the `.so` mtime updated but
   the Rust binary may be a cached artifact, not a fresh compile from `d09deb4`.
   `maturin develop --release` failed due to workspace Cargo.toml layout issues.
2. There may be a second deadlock site beyond `EngineCore::open()` that only manifests
   in the full startup sequence (e.g., collection registration, `describe_operational_collection`,
   or `safe_export` during backup while engine #1's writer thread is active).

**Current Memex state:** The `fathomdb_schema` logger is suppressed to WARNING (noisy
bootstrap lines). `fathomdb_engine` suppression was removed in anticipation of the fix
but may need to be restored if the deadlock persists after a confirmed clean rebuild.

---

## Memex Verification: Fix Confirmed (2026-03-30 16:07)

After a full `cargo clean` + `uv pip install -e --no-build-isolation` targeting Python 3.12
(1m43s compile, 4.7MB .so), the `d09deb4` fix resolves the deadlock completely.

The earlier failed verification was caused by **stale cargo cache**: `cargo clean` was run
before a Python 3.11 rebuild, but when the 3.12 rebuild ran afterward, cargo reused the
3.11-era object files (which did not contain the fix). The `.so` mtime updated but the
binary content was unchanged.

**Correct rebuild procedure for editable installs across Python versions:**
```bash
cd ~/projects/fathomdb
rm -rf target python/build python/*.egg-info python/fathomdb/_fathomdb*.so
uv pip install -e python/ --no-build-isolation   # from the consuming venv
```

**Memex logging state:** `fathomdb_engine` restored to DEBUG. Only `fathomdb_schema`
suppressed to WARNING (noisy bootstrap lines, not a deadlock concern).

---

## Response: Full Rebuild Completed (2026-03-30 15:15)

The previous rebuild did not recompile Rust. There are two caches that must
be invalidated:

1. **Cargo's target cache** — compiled `.rlib`/`.so` artifacts in `target/`.
   `pip install -e .` and `uv pip install -e .` invoke maturin, which invokes
   cargo, which skips compilation if artifacts are up-to-date. Even
   `--reinstall --no-cache` only bypasses pip's wheel cache, not cargo's.

2. **Maturin's editable shim** — the `.so` file maturin places in the source
   tree. Its mtime updates when pip reinstalls, but the binary content is
   unchanged if cargo didn't recompile.

Neither `pip install --reinstall` nor `uv pip install --no-cache` forces
cargo to recompile Rust. You must run `cargo clean` first.

**Guaranteed full rebuild:**

```bash
cd ~/projects/fathomdb
cargo clean
pip install -e python/ --no-build-isolation
```

This was done at 15:15. All four Rust crates (`fathomdb`, `fathomdb-engine`,
`fathomdb-query`, `fathomdb-schema`) plus `pyo3`, `pyo3-log`, and all
transitive dependencies were recompiled from source (2 min, release profile).
The installed `.so` now contains the `d09deb4` fix.

If the deadlock persists after this rebuild, the cause is a second deadlock
site beyond `EngineCore::open()` — likely an operation in the Memex startup
path that calls into Rust without releasing the GIL. Report which operation
hangs and we will add `py.allow_threads()` there.

---

## GC-Triggered Drop Deadlock (2026-03-30)

A second GIL deadlock was discovered in Memex where `FathomStore.close()` set
`self._engine = None` without calling `engine.close()` first.  When Python GC
finalized the Rust `EngineCore` object:

1. GC runs on the main Python thread, holding the GIL
2. Rust `Drop` for `WriterActor` calls `thread.join()` to wait for the writer
3. The writer thread tries to acquire the GIL for pyo3-log logging
4. Deadlock: main thread holds GIL waiting for writer, writer needs GIL

This was compounded by a second issue: the old stale process held a WAL write
lock, and the new process's writer thread blocked on it.  Two writer threads
on the same database file contending via WAL locking is a recipe for hangs.

### Fixes Applied

**1. Exclusive file lock** (`database_lock.rs`): `EngineRuntime::open` now
acquires `flock(LOCK_EX|LOCK_NB)` on `{database_path}.lock`.  A second open
fails immediately with `DatabaseLockedError` instead of silently contending
on WAL locks.  The error message includes the PID of the holding process.

**2. GC-safe `Drop`** (`python.rs`): `EngineCore` now implements `Drop` that
takes the engine out via `get_mut().take()` and drops it inside
`Python::with_gil(|py| py.allow_threads(move || drop(engine)))`.  This
releases the GIL so the writer thread can finish its pyo3-log calls during
shutdown, even when triggered by GC rather than explicit `close()`.

### Verification

- `test_gc_drop_without_close_no_deadlock`: opens engine, writes, `del engine`
  + `gc.collect()` without calling `close()` — no deadlock, database reopens.
- `test_second_open_raises_database_locked`: second `Engine.open()` on same
  path raises `DatabaseLockedError` with holder PID.
- All 34 Python tests pass, including 8 concurrency/deadlock regression tests.
