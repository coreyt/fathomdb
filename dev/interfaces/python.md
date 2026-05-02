---
title: Python Public Interface
date: 2026-04-24
target_release: 0.6.0
desc: Public Python surface for 0.6.0
blast_radius: src/python/; design/bindings.md; design/errors.md; design/lifecycle.md; design/engine.md
status: locked
---

# Python Interface

This file owns Python-visible symbol spelling and attribute casing.
Cross-binding parity remains owned by `design/bindings.md`.

## Runtime surface

The canonical runtime verbs available to Python callers are:

- `Engine.open(...)`
- `engine.write(...)`
- `engine.search(...)`
- `engine.close()`
- `admin.configure(...)`

`Engine.open(...)` returns the engine handle plus the structured open report
owned by `design/engine.md`.

`Engine.open(path, *, config=None, **engine_config)` accepts the
engine-owned knobs from `design/engine.md` in snake_case:

- `embedder_pool_size`
- `scheduler_runtime_threads`
- `provenance_row_cap`
- `embedder_call_timeout_ms`
- `slow_threshold_ms`

The keyword form and `EngineConfig` object form are equivalent. Python
executor usage remains caller-owned and is not an engine config field.

## Engine-attached instrumentation / control

These are public instance methods, not extra top-level SDK verbs:

- `engine.drain(timeout_s=...)`
- `engine.counters()`
- `engine.set_profiling(enabled=...)`
- `engine.set_slow_threshold_ms(value=...)`

Subscriber attachment is provided by:

- `engine.attach_logging_subscriber(logger, *, heartbeat_interval_ms=None)`

The helper maps engine events into Python `logging.LogRecord`s with the stable
`fathomdb` payload described by `design/bindings.md`.

## Caller-visible data shapes

- `WriteReceipt.cursor`
- `SearchResult.projection_cursor`
- `SearchResult.soft_fallback.branch`

`soft_fallback.branch` uses the typed values owned by `design/retrieval.md`.

## Errors

Python exposes one catch-all base class plus one concrete subclass per canonical
row in `design/errors.md`.

Examples of caller-visible subclasses:

- `DatabaseLockedError`
- `CorruptionError`
- `MigrationError`
- `IncompatibleSchemaVersionError`
- `EmbedderIdentityMismatchError`
- `EmbedderDimensionMismatchError`
- `SchemaValidationError`
- `OverloadedError`
- `ClosingError`

Payload fields remain typed attributes; callers do not dispatch on message
text.

## Non-presence

Python does not expose recovery verbs or doctor-only flags. In particular,
there is no SDK equivalent of `recover`, `check-integrity`, `--quick`,
`--full`, or `--round-trip`. See `design/recovery.md`.
