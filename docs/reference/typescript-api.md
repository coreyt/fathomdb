# TypeScript API

Package: `fathomdb`. Authoritative spec:
[`dev/interfaces/typescript.md`](https://github.com/coreyt/fathomdb/blob/0.6.0-rewrite/dev/interfaces/typescript.md).

> **TS SDK parity caveat.** TS first working slice shipped 2026-04-07.
> The TS surface covers the same five-verb canonical set and the same
> error taxonomy as Python, but TS is the less-mature SDK in 0.6.0.
> Prefer Python for production pilots. See
> [release notes § TypeScript SDK parity](../release-notes/0.6.0.md).

All runtime operations are Promise-returning. The TS↔Python parity
matrix is in [`dev/notes/12-TX-parity-matrix.md`](https://github.com/coreyt/fathomdb/blob/0.6.0-rewrite/dev/notes/12-TX-parity-matrix.md).

## Top-level

```ts
import {
  Engine,
  admin,
  type EngineConfig,
  type EngineOpenOptions,
  type WriteReceipt,
  type SearchHit,
  type SearchResult,
  type SoftFallback,
  type SoftFallbackBranch,
  type CounterSnapshot,
  type SubscriberCallback,
  type AttachSubscriberOptions,
  type AdminConfigureOptions,
  FathomDbError,
  // ...18 concrete leaf classes, see errors reference
} from "fathomdb";
```

## `Engine`

### `Engine.open(path, options?) -> Promise<Engine>`

Open or create a FathomDB database at `path`.

- `path` (`string`).
- `options` (`EngineOpenOptions`):
    - `engineConfig` (`EngineConfig`) — engine knobs in camelCase.
      See [config](config.md).

Rejects with a `FathomDbError` subclass on failure:
`DatabaseLockedError`, `CorruptionError`,
`IncompatibleSchemaVersionError`, `MigrationError`,
`EmbedderIdentityMismatchError`, `EmbedderDimensionMismatchError`.
See [errors](errors.md).

> **0.6.0 caveat.** The napi-rs binding returns only the engine
> handle. The structured open report defined in
> `dev/design/engine.md` is populated on the Rust side but dropped
> at the binding boundary. Surfacing it defers to **0.6.1** (slice
> `12-TX-OPENREPORT`).

### `engine.write(batch?) -> Promise<WriteReceipt>`

Enqueue a batch of canonical rows.

- `batch` (`unknown[]`) — caller-shaped canonical rows. Defaults
  to `[]`. A node/edge item may carry an optional `logicalId`
  (`string`): supplying it makes the write a transaction-time
  **supersession** of the prior active version of that
  `(logicalId, kind)` (the prior version is tombstoned and the new one
  becomes active — invalidate-not-delete). Omitting it is a plain
  insert with a NULL `logicalId` that never collides with other NULLs.
- Returns: `WriteReceipt { cursor, rowCursors }` — `cursor` is the
  batch high-water `write_cursor`; `rowCursors` are the per-row
  `write_cursor`s, 1:1 with the input batch order.

### `engine.search(query, filter?) -> Promise<SearchResult>`

Run hybrid retrieval, ranked by **G9 RRF fusion**.

- `query` (`string`).
- `filter` ([`SearchFilter`](#searchfilter), optional) — closed metadata filter;
  omitted (or all-`undefined`) is the unfiltered path.
- Resolves to a `SearchResult` whose `results` is a `SearchHit[]`; each
  [`SearchHit`](#searchhit) carries the matched record's `id`, `kind`, `body`,
  the **RRF-fused** `score`, and the `branch` that produced it.

> **Ranking is RRF (behavior-compat event).** Results are ordered by Reciprocal
> Rank Fusion (`Σ 1/(60 + rank)`) of the vector and text branches — the
> deliberate, documented 0.8.0 ranking change; pre-0.8.0 union-dedup ordering is
> not retained. See [hybrid search guide](../guides/hybrid-search-filtering.md).

### `engine.close() -> Promise<void>`

Release SQLite handles, join the writer thread, drain the scheduler.
Idempotent.

### `engine.drain(timeoutMs) -> Promise<void>`

Block until in-flight writes drain or `timeoutMs` elapses. Argument
unit is **milliseconds** (Python counterpart uses seconds).

### `engine.counters() -> CounterSnapshot`

Synchronous snapshot. See [`CounterSnapshot`](#countersnapshot).

### `engine.setProfiling(enabled: boolean) -> void`

Toggle per-operation profiling.

### `engine.setSlowThresholdMs(value: number) -> void`

Set the slow-query threshold for profiling event emission.

### `engine.attachSubscriber(callback, options?) -> void`

Bind engine events to a callback. `callback: (event:
SubscriberEvent) => void` receives the stable `fathomdb` payload
described in `dev/design/bindings.md`. `options.heartbeatIntervalMs`
is optional.

### Properties

- `engine.config` (`EngineConfig`) — resolved config.

## `admin.configure`

```ts
import { admin } from "fathomdb";

const receipt = await admin.configure(engine, { name: "my-schema", body: schemaJson });
```

`admin.configure(engine: Engine, options: AdminConfigureOptions):
Promise<WriteReceipt>` where `AdminConfigureOptions = { name:
string; body: string }`.

## Data shapes

### `WriteReceipt`

```ts
interface WriteReceipt {
  cursor: number; // batch high-water write_cursor
  rowCursors: number[]; // G0 — per-row write_cursor, 1:1 with the batch
}
```

`rowCursors` is the `write_cursor`-as-row-id identity carrier (G0 /
Slice 15): for an N-row batch it is `[cursor - N + 1, …, cursor]`.

### `SearchResult`

```ts
interface SearchResult {
  projectionCursor: number;
  softFallback: SoftFallback | null;
  results: SearchHit[];
}
```

### `SearchHit`

```ts
interface SearchHit {
  id: number; // canonical row write_cursor (interim identity carrier)
  kind: string;
  body: string;
  score: number; // G9 RRF-fused relevance (Σ 1/(60+rank)); higher = better
  branch: SoftFallbackBranch; // "vector" | "text"
}
```

`score` is the **G9 RRF-fused** relevance (higher = more relevant), optionally
recency-reweighted. Raw `vec_distance_l2` (vector) and `bm25()` (text) are fused
on **rank**, never compared raw (they are not comparable). `branch` tags which
branch produced the representative hit (vector-first when both surface a body).

### `SearchFilter`

```ts
interface SearchFilter {
  sourceType?: string;
  kind?: string;
  createdAfter?: number; // created_at >= bound (unix seconds)
  status?: string;
}
```

G10 — a **closed** metadata filter (not an open DSL) for `engine.search`. Each
present field constrains the vector branch in a single phase-1 KNN statement and
constrains the text branch by the same metadata; omitted/all-`undefined` is the
unfiltered path (byte-identical to the pre-filter query). `status` filters the
vec0 `status` column, which ships an **empty-string sentinel only** (no real
population source yet — vec0 TEXT metadata is not NULL-able), so a
`status: "open"`-style filter prunes every row until a population slice lands.

### `SoftFallback`

```ts
interface SoftFallback {
  branch: SoftFallbackBranch; // "vector" | "text"
}
```

### `CounterSnapshot`

```ts
interface CounterSnapshot {
  queries: number;
  writes: number;
  writeRows: number;
  adminOps: number;
  cacheHit: number;
  cacheMiss: number;
}
```

## Errors

`fathomdb` exports `FathomDbError` (the catch-all base) plus 18
concrete leaf classes. See [errors reference](errors.md).

Panics in the Rust runtime surface as `FathomDbPanicError` (not a
`FathomDbError` subclass — panic carriers are deliberately outside
the catch-all root).

## See also

- [Quickstart](../getting-started/quickstart.md)
- [Config knobs](config.md)
- [Errors](errors.md)
- Locked spec: [`dev/interfaces/typescript.md`](https://github.com/coreyt/fathomdb/blob/0.6.0-rewrite/dev/interfaces/typescript.md)
- TS↔Python parity matrix: [`dev/notes/12-TX-parity-matrix.md`](https://github.com/coreyt/fathomdb/blob/0.6.0-rewrite/dev/notes/12-TX-parity-matrix.md)
