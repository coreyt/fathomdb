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
  to `[]`.

### `engine.search(query) -> Promise<SearchResult>`

Run hybrid retrieval.

- `query` (`string`).

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
  cursor: number;
}
```

### `SearchResult`

```ts
interface SearchResult {
  projectionCursor: number;
  softFallback: SoftFallback | null;
  results: string[];
}
```

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
