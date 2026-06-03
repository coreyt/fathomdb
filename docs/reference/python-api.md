# Python API

Module: `fathomdb`. Authoritative spec:
[`dev/interfaces/python.md`](https://github.com/coreyt/fathomdb/blob/0.6.0-rewrite/dev/interfaces/python.md).

## Top-level

```python
from fathomdb import (
    Engine,
    EngineConfig,
    SearchHit,
    SearchResult,
    SoftFallback,
    SoftFallbackBranch,
    WriteReceipt,
    CounterSnapshot,
    admin,
    errors,
)
```

## `Engine`

### `Engine.open(path, *, config=None, **engine_config) -> Engine`

Open or create a FathomDB database at `path`. Returns the engine
handle.

- `path` (`str`) — path to the SQLite DB file.
- `config` (`EngineConfig | None`) — pre-built config object.
- `**engine_config` — per-knob keyword arguments (see
  [config](config.md)). Mutually exclusive with `config`.

Raises `EngineError` subclasses on failure: `DatabaseLockedError`,
`CorruptionError`, `IncompatibleSchemaVersionError`,
`MigrationError`, `EmbedderIdentityMismatchError`,
`EmbedderDimensionMismatchError`. See [errors](errors.md).

> **0.6.0 caveat.** The PyO3 binding returns only the engine handle.
> The structured open report
> (`migration_version_reached`, `embedder_identity_confirmed`, open-
> stage data) defined in `dev/design/engine.md` is populated on the
> Rust side but dropped at the binding boundary. Surfacing it defers
> to **0.6.1** (slice `12-TX-OPENREPORT`). Clients depending on
> open-report data should pin to 0.6.1 when it ships. See
> [release notes](../release-notes/0.6.0.md).

### `engine.write(batch=None) -> WriteReceipt`

Enqueue a batch of canonical rows. Synchronous; blocks until the
writer thread has accepted the batch.

- `batch` (`list[Any] | None`) — caller-shaped canonical rows.
  Defaults to `[]`.
- Returns: `WriteReceipt(cursor: int)`. The cursor advances
  monotonically across writes.

### `engine.search(query) -> SearchResult`

Run hybrid retrieval (FTS5 + vector) for `query`.

- `query` (`str`).
- Returns: `SearchResult(projection_cursor: int, soft_fallback:
  SoftFallback | None, results: list[SearchHit])`. Each
  [`SearchHit`](#searchhit) carries the matched record's `id`, `kind`,
  `body`, a per-branch `score`, and the `branch` that produced it.

### `engine.close() -> None`

Release SQLite handles, join the writer thread, drain the scheduler,
release the on-disk lock. Idempotent.

### `engine.drain(*, timeout_s=0) -> None`

Block until in-flight writes drain or `timeout_s` elapses. Argument
unit is **seconds** (TS counterpart uses milliseconds).

### `engine.counters() -> CounterSnapshot`

Snapshot of engine-internal counters. See
[`CounterSnapshot`](#countersnapshot) below.

### `engine.set_profiling(*, enabled: bool) -> None`

Toggle per-operation profiling.

### `engine.set_slow_threshold_ms(*, value: int) -> None`

Set the slow-query threshold for profiling event emission.

### `engine.attach_logging_subscriber(logger, *, heartbeat_interval_ms=None) -> None`

Bind engine events into a Python `logging.Logger`. Engine events are
mapped to `logging.LogRecord` with the stable `fathomdb` payload.

### Properties

- `engine.path` (`str`) — DB path supplied to `open`.
- `engine.config` (`EngineConfig`) — resolved config.

## `admin.configure`

```python
from fathomdb import admin

receipt = admin.configure(engine, name="my-schema", body=schema_json)
```

`admin.configure(engine, *, name: str, body: str) -> WriteReceipt`.

Submit an admin schema configuration. The writer thread applies
it; the returned cursor places the apply in the global write order.

## Data shapes

### `WriteReceipt`

```python
@dataclass(frozen=True)
class WriteReceipt:
    cursor: int
```

### `SearchResult`

```python
@dataclass(frozen=True)
class SearchResult:
    projection_cursor: int
    soft_fallback: SoftFallback | None = None
    results: list[SearchHit] = []
```

### `SearchHit`

```python
@dataclass(frozen=True)
class SearchHit:
    id: int          # canonical row write_cursor (interim identity carrier)
    kind: str
    body: str
    score: float     # vec_distance_l2 (vector branch) or bm25() (text branch)
    branch: SoftFallbackBranch  # Literal["vector", "text"]
```

`score` is the raw per-branch relevance: `vec_distance_l2` for the vector
branch (lower = closer) and `bm25()` for the text branch (more-negative =
more-relevant). The two are **not** comparable raw — fusing them onto a single
scale is a later (RRF) concern. `branch` tags which retrieval branch produced
the hit.

### `SoftFallback`

```python
@dataclass(frozen=True)
class SoftFallback:
    branch: SoftFallbackBranch  # Literal["vector", "text"]
```

`branch` indicates which non-essential branch could not contribute.
Total request failure is not expressed via this carrier.

### `CounterSnapshot`

```python
@dataclass(frozen=True)
class CounterSnapshot:
    queries: int = 0
    writes: int = 0
    write_rows: int = 0
    admin_ops: int = 0
    cache_hit: int = 0
    cache_miss: int = 0
```

## Errors

`fathomdb.errors` exports `EngineError` (the catch-all base) plus 18
concrete leaf classes. See [errors reference](errors.md) for the full
matrix and recovery-hint codes.

## See also

- [Quickstart](../getting-started/quickstart.md)
- [Config knobs](config.md)
- [Errors](errors.md)
- Locked spec: [`dev/interfaces/python.md`](https://github.com/coreyt/fathomdb/blob/0.6.0-rewrite/dev/interfaces/python.md)
