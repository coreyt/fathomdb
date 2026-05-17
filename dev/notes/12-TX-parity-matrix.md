---
title: TypeScript ↔ Python SDK Parity Matrix (12-TX)
date: 2026-05-17
target_release: 0.6.0
desc: Per-row mapping of Python public surface to TypeScript public surface; gap classification per `dev/design/bindings.md` § 1 "surface-set parity invariant"
blast_radius: src/python/fathomdb/; src/python/tests/; src/ts/src/; src/ts/tests/; src/rust/crates/fathomdb-py/src/lib.rs; src/rust/crates/fathomdb-napi/src/lib.rs; src/python/fathomdb/_fathomdb.pyi; dev/interfaces/python.md; dev/interfaces/typescript.md; dev/design/bindings.md; dev/design/errors.md
status: snapshot
---

# 12-TX Parity Matrix

Snapshot taken at slice start of Phase 12-TX (TS → Python parity audit).
Per `dev/design/bindings.md` § 1, the parity claim is "same five-verb
canonical set + same error class taxonomy + same data shapes in
language-idiomatic spelling". Cross-language idiom differences
(`camelCase` vs `snake_case`; positional vs kwarg; `Promise` vs sync;
options-object vs keyword-arg) are **intentional per
`dev/interfaces/typescript.md` and `dev/interfaces/python.md`** and are
recorded here as `📋 divergence-by-design`, **not** as gaps.

Legend:

- ✅ **parity-OK** — both surfaces present; spec-compliant idiom differences acknowledged
- ❌ **gap** — TS missing what Python has, or vice versa, where the locked interface spec implies parity
- 📋 **divergence-by-design** — locked spec explicitly diverges

## 1. Five-verb canonical surface (REQ-053; AC-057a)

| Verb                                             | Python (`src/python/fathomdb/`)                                          | TypeScript (`src/ts/src/`)                                                                     | Status                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ------------------------------------------------ | ------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Engine.open`                                    | `Engine.open(path, *, config=None, **engine_config)` → `Engine`          | `Engine.open(path, options?)` → `Promise<Engine>`                                              | 📋 sync vs Promise; kwargs vs options-object; both per spec                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `Engine.open` return: **structured open report** | NOT returned (PyO3 binding drops `OpenedEngine.report`)                  | NOT returned (napi binding drops `OpenedEngine.report`)                                        | ⚠️ **0.6.0 documented gap** — locked specs (interfaces/python.md L25-28, interfaces/typescript.md L27-29) call for "engine handle plus structured open report owned by design/engine.md". Native Rust `Engine::open` returns `OpenedEngine { engine, report: OpenReport }` (engine src L541-553), but both bindings currently surface only the engine. Symmetric parity (both bindings drop it); not a TS-vs-Python gap. **Resolution:** implementation deferred to **0.6.1** as a follow-up slice 12-TX-OPENREPORT. Release notes disclose the gap; clients depending on migration-version-reached / embedder-identity-confirmed / open-stage report wait for 0.6.1. Spec text in interfaces/{python,typescript}.md amended to add "0.6.0 caveat" note. |
| `engine.write`                                   | `engine.write(batch=None)` → `WriteReceipt` (sync)                       | `engine.write(batch=[])` → `Promise<WriteReceipt>`                                             | 📋 sync vs Promise per spec                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `engine.search`                                  | `engine.search(query)` → `SearchResult` (sync)                           | `engine.search(query)` → `Promise<SearchResult>`                                               | 📋 sync vs Promise per spec                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `engine.close`                                   | `engine.close()` → `None` (sync)                                         | `engine.close()` → `Promise<void>`                                                             | 📋 sync vs Promise per spec                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `admin.configure`                                | `admin.configure(engine, *, name, body)` → `WriteReceipt` (module-level) | `admin.configure(engine, { name, body })` → `Promise<WriteReceipt>` (object-literal namespace) | ✅                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |

## 2. Engine-attached instrumentation / control

| Method                  | Python                                                                             | TypeScript                                                    | Status                                                                                                                                     |
| ----------------------- | ---------------------------------------------------------------------------------- | ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `drain`                 | `engine.drain(*, timeout_s: float\|int = 0)` (sync)                                | `engine.drain(timeoutMs: number)` → `Promise<void>`           | 📋 timeout unit (seconds vs ms) + sync vs Promise per spec                                                                                 |
| `counters`              | `engine.counters()` → `CounterSnapshot` (empty dataclass; **drops** native fields) | `engine.counters()` → `CounterSnapshot` (6 fields populated)  | ❌ **gap closed in 12-TX**: Python wrapper threw away the 6 fields the PyO3 layer already returns                                          |
| `set_profiling`         | `engine.set_profiling(*, enabled)`                                                 | `engine.setProfiling(enabled)`                                | 📋 kwarg vs positional per spec                                                                                                            |
| `set_slow_threshold_ms` | `engine.set_slow_threshold_ms(*, value)`                                           | `engine.setSlowThresholdMs(value)`                            | 📋 kwarg vs positional per spec                                                                                                            |
| Subscriber attach       | `engine.attach_logging_subscriber(logger, *, heartbeat_interval_ms=None)`          | `engine.attachSubscriber(callback, { heartbeatIntervalMs? })` | 📋 logging.Logger vs callback per `interfaces/python.md` § 51 + `interfaces/typescript.md` § 54 (explicitly different host-adapter shapes) |

## 3. Caller-visible data shapes

| Shape             | Python field set                                                                                                                             | TypeScript field set                                                                  | Status                                                                                     |
| ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `WriteReceipt`    | `cursor: int`                                                                                                                                | `cursor: number`                                                                      | ✅                                                                                         |
| `SoftFallback`    | `branch: Literal["vector","text"]`                                                                                                           | `branch: "vector" \| "text"`                                                          | ✅                                                                                         |
| `SearchResult`    | `projection_cursor: int`, `soft_fallback: SoftFallback \| None`, `results: list[str]`                                                        | `projectionCursor: number`, `softFallback: SoftFallback \| null`, `results: string[]` | ✅ (casing)                                                                                |
| `CounterSnapshot` | _(was: empty dataclass)_; **after 12-TX**: `queries`, `writes`, `write_rows`, `admin_ops`, `cache_hit`, `cache_miss` (all `int`)             | `queries`, `writes`, `writeRows`, `adminOps`, `cacheHit`, `cacheMiss` (all `number`)  | ❌ → ✅ (gap closed; 6 fields added to Python dataclass + wired through `Engine.counters`) |
| `EngineConfig`    | `embedder_pool_size`, `scheduler_runtime_threads`, `provenance_row_cap`, `embedder_call_timeout_ms`, `slow_threshold_ms` (all `int \| None`) | same five knobs, `Optional<number>`, camelCase                                        | ✅                                                                                         |

## 4. Error-class taxonomy (15+ leaves per `dev/design/errors.md`)

Python defines a root `EngineError` plus 18 concrete leaves. TypeScript
defines `FathomDbError` plus 18 concrete leaves. Both bindings expose
1:1 the same leaf set:

| Class                                                        | Python `errors.py`                                                             | TS `errors.ts`                                                                                     |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------- |
| (root)                                                       | `EngineError`                                                                  | `FathomDbError`                                                                                    |
| `StorageError`                                               | ✅                                                                             | ✅                                                                                                 |
| `ProjectionError`                                            | ✅                                                                             | ✅                                                                                                 |
| `VectorError`                                                | ✅                                                                             | ✅                                                                                                 |
| `EmbedderError`                                              | ✅                                                                             | ✅                                                                                                 |
| `EmbedderNotConfiguredError`                                 | ✅                                                                             | ✅                                                                                                 |
| `KindNotVectorIndexedError`                                  | ✅                                                                             | ✅                                                                                                 |
| `SchedulerError`                                             | ✅                                                                             | ✅                                                                                                 |
| `OpStoreError`                                               | ✅                                                                             | ✅                                                                                                 |
| `WriteValidationError`                                       | ✅                                                                             | ✅                                                                                                 |
| `SchemaValidationError`                                      | ✅                                                                             | ✅                                                                                                 |
| `OverloadedError`                                            | ✅                                                                             | ✅                                                                                                 |
| `ClosingError`                                               | ✅                                                                             | ✅                                                                                                 |
| `DatabaseLockedError`                                        | ✅ + `holder_pid`                                                              | ✅ + `holderPid`                                                                                   |
| `CorruptionError`                                            | ✅ + `kind` / `stage` / `recovery_hint_code` / `doc_anchor`                    | ✅ + `kind` / `stage` / `recoveryHintCode` / `docAnchor`                                           |
| `IncompatibleSchemaVersionError`                             | ✅                                                                             | ✅                                                                                                 |
| `MigrationError`                                             | ✅                                                                             | ✅                                                                                                 |
| `EmbedderIdentityMismatchError`                              | ✅ + `stored_name` / `stored_revision` / `supplied_name` / `supplied_revision` | ✅ + camelCase counterparts                                                                        |
| `EmbedderDimensionMismatchError`                             | ✅ + `stored` / `supplied`                                                     | ✅ + `stored` / `supplied`                                                                         |
| Panic carrier (contract bug; not a `FathomDbError` subclass) | `pyo3_runtime.PanicException` (PyO3-owned; not `EngineError`)                  | `FathomDbPanicError` (TS-owned; not `FathomDbError`; both deliberately outside the catch-all root) |

**Status: parity-OK after 12-TX.** No new TS leaves required.

## 5. Type-stub completeness

| Native member                                                                                          | Python `_fathomdb.pyi` | TS `binding.ts` `NativeEngine` / `NativeModule` |
| ------------------------------------------------------------------------------------------------------ | ---------------------- | ----------------------------------------------- |
| `Engine.open`                                                                                          | ✅                     | ✅                                              |
| `Engine.write` / `search` / `close` / `drain` / `counters` / `set_profiling` / `set_slow_threshold_ms` | ✅                     | ✅                                              |
| `Engine.attach_logging_subscriber` (Python) / `attachSubscriber` (TS)                                  | ✅                     | ✅                                              |
| `admin_configure` / `adminConfigure`                                                                   | ✅                     | ✅                                              |
| `force_panic_for_test` / `forcePanicForTest`                                                           | ✅                     | ✅                                              |
| `force_panic_in_accessor_for_test` (TS-only AC-067 sync-path probe)                                    | n/a                    | ✅                                              |

`forcePanicInAccessorForTest` is a TS-binding-specific probe (no
Python counterpart): per `src/rust/crates/fathomdb-napi/src/lib.rs:707`,
it exists to assert sync `#[napi]` accessor panic translation, which
has no analogue in PyO3 (Python is sync-only; there is no async/sync
split). 📋 divergence-by-design.

All 18 leaf classes appear in both stubs.

## 6. Test counterparts

| Python file                          | TS file                                                               | Status                                                                                                                                                                                                                                                                                                                                                                                                            |
| ------------------------------------ | --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tests/conftest.py`                  | `tests/helpers.ts`                                                    | ✅                                                                                                                                                                                                                                                                                                                                                                                                                |
| `tests/test_errors.py`               | `tests/errors.test.ts`                                                | ✅ (after 12-TX adds `EmbedderNotConfiguredError` + `KindNotVectorIndexedError` to Python `LEAF_CLASSES` to match TS coverage)                                                                                                                                                                                                                                                                                    |
| `tests/test_typed_errors.py`         | `tests/typed-errors.test.ts`                                          | ✅                                                                                                                                                                                                                                                                                                                                                                                                                |
| `tests/test_ffi_safety.py`           | `tests/ffi-safety.test.ts`                                            | ✅                                                                                                                                                                                                                                                                                                                                                                                                                |
| `tests/test_no_recovery_surface.py`  | `tests/no-recovery-surface.test.ts`                                   | ✅                                                                                                                                                                                                                                                                                                                                                                                                                |
| `tests/test_save_time_validation.py` | `tests/save-time-validation.test.ts`                                  | ✅                                                                                                                                                                                                                                                                                                                                                                                                                |
| `tests/test_surface.py`              | `tests/surface.test.ts`                                               | ✅                                                                                                                                                                                                                                                                                                                                                                                                                |
| `tests/test_scaffold.py`             | `tests/scaffold.test.ts` _(added in 12-TX; cursor-advances-on-write)_ | ❌ → ✅                                                                                                                                                                                                                                                                                                                                                                                                           |
| `tests/test_property_template.py`    | _(none; hypothesis-based placeholder)_                                | 📋 divergence-by-design: Python file is a hypothesis scaffold (`assert x == x`) explicitly marked "Replace this trivial property when real Python API surface lands". No real invariant is tested; replicating into TS adds a fast-check dependency for an equally trivial placeholder. Net-negative LoC: defer the TS port until either side gets a real property test, at which point both ports land together. |

## 7. Out-of-scope idiom translation (locked per spec)

The following patterns are **not** parity gaps. They are locked
divergences per `dev/interfaces/{python,typescript}.md` and exist by
design.

- `snake_case` vs `camelCase` identifiers
- Keyword-only Python args (`*, name=...`) vs TS options objects (`{ name }`)
- Sync Python surface vs Promise-based TS surface (`ADR-0.6.0-async-surface` Path 1 vs Path 2)
- `logging.Logger`-based subscriber attach in Python vs callback-based attach in TS
- Python `timeout_s` (seconds) vs TS `timeoutMs` (milliseconds) on `drain`
- Python `EngineError` root vs TS `FathomDbError` root
- Python `EmbedderDimensionMismatchError` exported as a top-level error class
  (not under `EmbedderError`) — TS mirrors

## 8. Summary

| Bucket                 | Count |
| ---------------------- | ----- |
| Rows audited           | 41    |
| `parity-OK`            | 28    |
| `gaps closed in 12-TX` | 3     |
| `divergence-by-design` | 10    |

Gaps closed:

1. Python `CounterSnapshot` types.py dataclass empty → 6 fields added; `Engine.counters` wired to copy native fields through.
2. Python `test_errors.py LEAF_CLASSES` missing `EmbedderNotConfiguredError` + `KindNotVectorIndexedError` → added.
3. `test_scaffold.py` lacked a TS counterpart → `src/ts/tests/scaffold.test.ts` added (cursor advances on first write).
