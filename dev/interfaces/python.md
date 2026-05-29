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

`Engine.open(...)` returns the engine handle. The structured open report owned
by `design/engine.md` is accessible after open via `engine.open_report()` (see
Engine-attached instrumentation / control below).

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

- `engine.open_report()`
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

## Default embedder

`Engine.open(path, use_default_embedder=True)` opts into the engine's
default embedder (`fathomdb-bge-small-en-v1.5`). On first use, weights
are downloaded from HuggingFace and cached under
`~/.cache/fathomdb/embedders/`; subsequent opens hit the warm cache. See
`dev/adr/ADR-0.7.1-default-embedder-weight-fetch.md` for the network-
surface scope (opt-in only; sha256-verified; visible via
`OpenReport.embedder_events`). The default (`use_default_embedder=False`)
opens without an embedder; subsequent vector writes fail with
`EmbedderNotConfiguredError`.

`OpenReport` carries four embedder-related fields surfaced by EU-6:
`embedder_download_ms`, `embedder_events`, `embedder_mean_centering_required`,
and `embedder_mean_vec_pinned`. Each entry in `embedder_events` is a
`dict` keyed by `"kind"` (`"DefaultEmbedderDownload"`,
`"DefaultEmbedderCacheHit"`, or `"MeanVecPinned"`) with a variant-
specific payload in snake_case.

### Shipped feature axis (EU-6 FIX-1)

Released wheels published to PyPI are compiled with the `default-embedder`
Cargo feature ON, so `use_default_embedder=True`
materialises a real bge-small embedder against the published artifact
without any extra install step. The no-feature build path is preserved
as a CI sanity check (informational wheel-size signal on the minimal-
deps tree), not a shipped artifact — there is no
`pip install fathomdb[no-default-embedder]` extra in 0.7.1.

The `test-hooks` Cargo feature is dev-only and never ships: methods
like `_write_vector_for_test` and `_configure_vector_kind_for_test` do
not exist on installed wheels. They are exposed only when the editable
binding is rebuilt with `--features test-hooks` (the
`src/python/tests/conftest.py` session fixture does this for the
pytest suite). End-user callers should not rely on these symbols.

### Custom embedder implementations (deferred to 0.8.x)

Supplying a custom Python `Embedder` implementation requires a PyO3
callback bridge subject to ADR-0.6.0-embedder-protocol Invariant 3 (no
`pyo3-log` emission during `embed()`). That bridge is a multi-slice
campaign deferred to 0.8.x. In 0.7.1 the binding surface is binary:
`use_default_embedder=True` (engine's bge-small) or `False` (no embedder;
vector writes fail with `EmbedderNotConfiguredError`).

## Non-presence

Python does not expose recovery verbs or doctor-only flags. In particular,
there is no SDK equivalent of `recover`, `check-integrity`, `--quick`,
`--full`, or `--round-trip`. See `design/recovery.md`.
