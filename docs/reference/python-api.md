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
  Defaults to `[]`. A node/edge item may carry an optional
  `logical_id` (`str`): supplying it makes the write a
  transaction-time **supersession** of the prior active version of
  that `(logical_id, kind)` — the prior version is tombstoned and the
  new version becomes active (invalidate-not-delete). Omitting it (the
  default) is a plain insert with a NULL `logical_id` and never
  collides with other NULL rows.
- Returns: `WriteReceipt(cursor: int, row_cursors: tuple[int, ...],
  dangling_edge_endpoints: int)`. `cursor` advances monotonically across
  writes (the batch high-water cursor); `row_cursors` are the per-row
  `write_cursor`s, 1:1 with the input batch order;
  `dangling_edge_endpoints` (G8) counts the edge endpoints in this batch
  pointing at a non-existent or superseded node — see
  [`WriteReceipt`](#writereceipt).

### `engine.search(query, filter=None) -> SearchResult`

Run hybrid retrieval (FTS5 + vector) for `query`, ranked by **G9 RRF fusion**.

- `query` (`str`).
- `filter` ([`SearchFilter`](#searchfilter) | `None`) — optional closed metadata
  filter. `None` (or an all-`None` filter) is the unfiltered path.
- Returns: `SearchResult(projection_cursor: int, soft_fallback:
  SoftFallback | None, results: list[SearchHit])`. Each
  [`SearchHit`](#searchhit) carries the matched record's `id`, `kind`,
  `body`, the **RRF-fused** `score`, and the `branch` that produced it.

> **Ranking is RRF (behavior-compat event).** Results are ordered by Reciprocal
> Rank Fusion (`Σ 1/(60 + rank)`) of the vector and text branches — a body the
> two branches agree on ranks above one only a single branch found. This is the
> deliberate, documented 0.8.0 ranking change; pre-0.8.0 union-dedup ordering is
> not retained. See [hybrid search guide](../guides/hybrid-search-filtering.md).

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
    cursor: int                       # batch high-water write_cursor
    row_cursors: tuple[int, ...]      # G0 — per-row write_cursor, 1:1 with the batch
    dangling_edge_endpoints: int      # G8 — edge endpoints pointing at no active node
```

`row_cursors` is the `write_cursor`-as-row-id identity carrier (G0 /
Slice 15): for an N-row batch it is `(cursor - N + 1, …, cursor)`.

`dangling_edge_endpoints` (G8 / Slice 20) counts how many edge endpoints
in the batch point at a node that has **no active version** — either
never written, or superseded (an active node = `superseded_at IS NULL`
carrying that `logical_id`). `from_id` and `to_id` are probed
independently, so one edge contributes 0, 1, or 2. It is **informational
only**: the batch always commits (flag-and-count; the write never
rejects on a dangling endpoint). Because endpoints match on `logical_id`,
an edge pointing at a legacy / own-identity node (NULL `logical_id`)
counts as dangling — only `logical_id`-keyed nodes are valid endpoints.
`0` when the batch committed no active edges.

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
    score: float     # G9 RRF-fused relevance (Σ 1/(60+rank)); higher = better
    branch: SoftFallbackBranch  # Literal["vector", "text"]
```

`score` is the **G9 RRF-fused** relevance (higher = more relevant), optionally
recency-reweighted. Raw `vec_distance_l2` (vector) and `bm25()` (text) are fused
on **rank**, never compared raw (they are not comparable). `branch` tags which
branch produced the representative hit (vector-first when both surface a body).

### `SearchFilter`

```python
@dataclass(frozen=True)
class SearchFilter:
    source_type: str | None = None
    kind: str | None = None
    created_after: int | None = None   # created_at >= bound (unix seconds)
    status: str | None = None
```

G10 — a **closed** metadata filter (not an open DSL) for `engine.search`. Each
present field constrains the vector branch in a single phase-1 KNN statement and
constrains the text branch by the same metadata; `None`/all-`None` is the
unfiltered path (byte-identical to the pre-filter query). `status` filters the
vec0 `status` column, which ships an **empty-string sentinel only** (no real
population source yet — vec0 TEXT metadata is not NULL-able), so a
`status="open"`-style filter prunes every row until a population slice lands.

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
