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
  `logicalId` (the prior version is tombstoned and the new one
  becomes active — invalidate-not-delete). Active-row identity is scoped
  to `logicalId` alone, so re-ingesting the same `logicalId` with a
  different `kind` supersedes (it does not create a second active row).
  Omitting it is a plain insert with a NULL `logicalId` that never
  collides with other NULLs.
- Returns: `WriteReceipt { cursor, rowCursors, danglingEdgeEndpoints }` —
  `cursor` is the batch high-water `write_cursor`; `rowCursors` are the
  per-row `write_cursor`s, 1:1 with the input batch order;
  `danglingEdgeEndpoints` (G8) counts the edge endpoints in this batch
  pointing at a non-existent or superseded node — see
  [`WriteReceipt`](#writereceipt).

### `engine.search(query, filter?, rerankDepth?, useGraphArm?, alpha?, poolN?) -> Promise<SearchResult>`

Run hybrid retrieval, ranked by **G9 RRF fusion**, with optional CPU
cross-encoder reranking (0.8.1 R1) and optional graph-BFS third arm (0.8.1 R3).

- `query` (`string`).
- `filter` ([`SearchFilter`](#searchfilter), optional) — closed metadata filter;
  omitted (or all-`undefined`) is the unfiltered path.
- `rerankDepth` (`number`, optional, default `undefined`/`0`) — 0.8.1 R1 opt-in.
  `0` or omitted uses the identity / soft-fallback path: byte-identical to the
  pre-0.8.1 fused order. `N > 0` applies a CPU cross-encoder (TinyBERT-L-2,
  ≈4 MB, p50 ≈ 1.5 ms/pair) over the top-N fused hits with score-blend
  (α=0.3 × CE + 0.7 × RRF-norm). Must be a non-negative integer; negative
  values throw `RangeError`, non-integer values throw `TypeError`. In the
  default build (no `default-reranker` feature), depth > 0 returns the identity
  order (model absent → soft-fallback).
- `useGraphArm` (`boolean`, optional, default `undefined`/`false`) — 0.8.1 R3
  opt-in. When `true`, seeds a BFS over temporal fact-edges from the top-10 fused
  hits (depth ≤ 3, cap 50). Edges with `tInvalid` in the past are excluded.
  Newly-reachable nodes are fused as a third RRF arm (`RRF_WEIGHT_GRAPH = 1.0`).
  Omitted or `false` produces byte-identical results to the pre-R3 two-arm
  pipeline. Non-boolean values throw `TypeError`.
- `alpha` (`number`, optional, default `undefined`/`0.3`) — 0.8.5 (EXP-0)
  CE-blend weight, clamped to `[0, 1]` in the engine. Omitted ⇒ `0.3`, the
  **C6 factoid-guard** default. **`alpha: 1.0` is opt-in for the agentic-answer
  / memory path** (the measured Mem0-parity config); the `0.3` default protects
  naive factoid lookups. Non-finite values throw `RangeError`. Effective only
  when `rerankDepth > 0` and the CE model is loaded.
- `poolN` (`number`, optional, default `undefined`/`rerankDepth`) — 0.8.5 (EXP-0)
  reranked-pool size, clamped to the hit count. Omitted ⇒ `rerankDepth`. Note
  `rerankDepth === 0` is still the identity gate, so `rerankDepth: 0, poolN: 10`
  does **not** rerank. Must be a non-negative integer (`RangeError` otherwise).
- Resolves to a `SearchResult` whose `results` is a `SearchHit[]`; each
  [`SearchHit`](#searchhit) carries the matched record's `id`, `kind`, `body`,
  the **RRF-fused** `score`, the `branch` that produced it (`"graph_arm"`
  for nodes surfaced only via graph traversal), and `ceScore` (the per-candidate
  CE score for in-pool reranked hits, `null` otherwise).

> **Ranking is RRF (behavior-compat event).** Results are ordered by Reciprocal
> Rank Fusion (`Σ 1/(60 + rank)`) of the vector and text branches — the
> deliberate, documented 0.8.0 ranking change; pre-0.8.0 union-dedup ordering is
> not retained. See [hybrid search guide](../guides/hybrid-search-filtering.md).

### `engine.embed(text) -> Promise<number[]>`

Embed `text` with the engine's pinned default embedder
(`fathomdb-bge-small-en-v1.5`) and return the raw vector. Read-path
primitive for callers that need vectors under the engine's **own**
embedder identity (e.g. coverage-index clustering) rather than a
parallel, possibly-divergent embedder. Rejects with
`EmbedderNotConfiguredError` if the engine was opened without an
embedder (`useDefaultEmbedder: false`). Mirror of the Python
`engine.embed(text)` (0.8.6 Slice 10 brought it to Py↔TS parity).

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

## `read.*` — governed read verbs (Slice 30 / G2 + G3)

```ts
import { read } from "fathomdb";
```

The `read.*` namespace exposes the governed retrieval verbs. Every read rides
the engine's **ReaderWorkerPool DEFERRED-tx snapshot path** — never the writer
lock — preserving single-writer isolation. Verb names are camelCase in TS but
the governed allowlist names stay dotted snake_case (`read.get_many`).

### `read.get(engine, logicalId: string): Promise<NodeRecord | null>`

Active-only point lookup by `logicalId` (active = `superseded_at IS NULL`). A
superseded version is never returned. A missing or superseded id resolves to
`null` — a **normal absence, not a thrown error**.

### `read.getMany(engine, logicalIds: string[]): Promise<(NodeRecord | null)[]>`

Batched point lookup. Returns one slot per requested id, **in request order**;
a missing/superseded id is `null` in its slot (partial, never all-or-nothing).
`read.get` delegates to `read.getMany`.

### `read.collection(engine, collection, options): Promise<OpStoreRow[]>`

Paginated op-store read-back over `operational_mutations` for `collection`,
**`ORDER BY id`**, where `options: ReadCollectionOptions = { afterId?: number;
limit: number }`. `limit` is **mandatory** (the engine clamps it to a ~1M cap,
so no call yields an unbounded read); `afterId` is the exclusive cursor.

### `read.mutations(engine, collection, options): Promise<OpStoreRow[]>`

Mutation-log-oriented alias surface over the **same** op-store read-back as
`read.collection` (identical args + semantics).

### `read.list(engine, kind, predicates?, limit?): Promise<NodeRecord[]>`

*(G4 / Slice 35)* List **active** `canonical_nodes` of the given `kind`
(`superseded_at IS NULL`), optionally filtered by a `Predicate[]` array
(AND-combined), up to `limit` rows (default 100).

```ts
interface Predicate {
  type: "eq" | "gt" | "gte" | "lt" | "lte";
  path: string;     // must be from the allowlist: $.status, $.priority, $.tags, $.kind, $.created_at
  value: string | number | boolean;
}
```

`path` must be from the engine allowlist: `$.status`, `$.priority`,
`$.tags`, `$.kind`, `$.created_at`. A non-allowlisted path throws
`InvalidFilterError` (never a panic). Values are **always bound as
parameterized SQL** — never interpolated (injection-safe per ADR D-F4).
An empty or omitted `predicates` is the unfiltered path.

```ts
import { Engine, read } from "fathomdb";
import { InvalidFilterError } from "fathomdb";

const engine = await Engine.open("my.db");
// All active task nodes:
const tasks = await read.list(engine, "task");
// Filtered: open tasks with priority > 5:
const openHigh = await read.list(engine, "task", [
  { type: "eq",  path: "$.status",   value: "open" },
  { type: "gt",  path: "$.priority", value: 5 },
]);
```

## Data shapes

### `WriteReceipt`

```ts
interface WriteReceipt {
  cursor: number; // batch high-water write_cursor
  rowCursors: number[]; // G0 — per-row write_cursor, 1:1 with the batch
  danglingEdgeEndpoints: number; // G8 — edge endpoints pointing at no active node
}
```

`rowCursors` is the `write_cursor`-as-row-id identity carrier (G0 /
Slice 15): for an N-row batch it is `[cursor - N + 1, …, cursor]`.

`danglingEdgeEndpoints` (G8 / Slice 20) counts how many edge endpoints
in the batch point at a node that has **no active version** — either
never written, or superseded (an active node = `superseded_at IS NULL`
carrying that `logicalId`). `from`/`to` are probed independently, so one
edge contributes 0, 1, or 2. It is **informational only**: the batch
always commits (flag-and-count; the write never rejects on a dangling
endpoint). Because endpoints match on `logicalId`, an edge pointing at a
legacy / own-identity node (NULL `logicalId`) counts as dangling — only
`logicalId`-keyed nodes are valid endpoints. `0` when the batch committed
no active edges.

### `NodeRecord`

```ts
interface NodeRecord {
  logicalId: string;
  kind: string;
  body: string;
  writeCursor: number; // interim id carrier (parity with SearchHit.id)
}
```

Returned by `read.get` / `read.getMany` for an **active** canonical node
(`superseded_at IS NULL`). Mirrors the Python `NodeRecord`.

### `OpStoreRow`

```ts
interface OpStoreRow {
  id: number; // operational_mutations PK + the afterId cursor key
  collection: string;
  recordKey: string;
  opKind: string; // always "append"
  payload: string; // the stored payload_json
  schemaId: string | null;
  writeCursor: number;
}
```

Returned by `read.collection` / `read.mutations`. Mirrors the Python `OpStoreRow`.

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
  branch: SoftFallbackBranch; // "vector" | "text" | "text_edge" | "graph_arm"
  sourceId: string | null; // G0 Phase-2 provenance; set only for graph-arm hits
  ceScore: number | null; // 0.8.5 CE score (sigmoid logit) for in-pool reranked hits
}
```

`score` is the **G9 RRF-fused** relevance (higher = more relevant), optionally
recency-reweighted. Raw `vec_distance_l2` (vector) and `bm25()` (text) are fused
on **rank**, never compared raw (they are not comparable). `branch` tags which
branch produced the representative hit (vector-first when both surface a body).
`ceScore` (0.8.5 / EXP-0) is the per-candidate cross-encoder score
(`sigmoid(ce_logit)`) for hits inside the reranked pool, `null` otherwise.

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
  branch: SoftFallbackBranch; // "vector" | "text" | "text_edge" | "graph_arm"
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

## `graph.*` — graph traversal (Slice 20 / G5 + G6)

```ts
import { graph } from "fathomdb";
```

The `graph.*` namespace exposes bounded BFS traversal and hybrid
search-plus-expansion. All reads ride the same **ReaderWorkerPool
DEFERRED-tx snapshot path** as `read.*`.

### `graph.neighbors(engine, logicalId, depth, direction?): Promise<NodeRecord[]>`

G5 — bounded BFS from `logicalId` over `canonical_edges`.

- `logicalId` (`string`) — the root node's stable identity.
- `depth` (`number`) — hop limit; **must be 1, 2, or 3**.
  Depth > 3 raises `InvalidArgumentError`.
- `direction` (`"outgoing" | "incoming" | "both"`, default `"both"`) — edge
  direction to follow.

Returns up to **50** `NodeRecord`s reachable within `depth` hops
(root excluded). Edges with `t_invalid` in the past are silently skipped
(valid-time filter). Returns `[]` when the root has no reachable neighbors.

### `graph.searchExpand(engine, query, depth, filter?): Promise<SearchExpandResult>`

G6 — FTS/vector search (G1) followed by bounded BFS expansion.

- `query` (`string`) — free-text or embedding query.
- `depth` (`number`) — BFS hop limit; 0 skips expansion. Depth > 3 raises
  `InvalidArgumentError`.
- `filter` (`SearchFilter | undefined`) — optional metadata filter (same as
  `engine.search`).

Returns a `SearchExpandResult`. Nodes appearing in both the search hit set and
the traversal reach appear **only** in `searchHits` (deduplication: search score
takes priority).

### `ExpandedNode`

```ts
interface ExpandedNode {
  node: NodeRecord; // the reachable node
  hopCount: number; // BFS distance from the nearest search-hit root
}
```

### `SearchExpandResult`

```ts
interface SearchExpandResult {
  searchHits: SearchHit[];     // original RRF-scored search results
  expanded: ExpandedNode[];    // nodes reachable by traversal, not in searchHits
  allLogicalIds: string[];     // deduplicated union of both sets
}
```

## Errors

`fathomdb` exports `FathomDbError` (the catch-all base) plus 20
concrete leaf classes. See [errors reference](errors.md).

Panics in the Rust runtime surface as `FathomDbPanicError` (not a
`FathomDbError` subclass — panic carriers are deliberately outside
the catch-all root).

## Embedder device (GPU)

There is **no TypeScript API** for selecting the embedder device — it is chosen
by a build-time cargo feature (`embed-cuda` / `embed-metal`) plus the
`FATHOMDB_EMBED_DEVICE` environment variable (`cpu` default · `cuda` · `cuda:N` ·
`metal`), resolved when the engine opens. The default (CPU) behavior is
unchanged. See [Default Embedder → GPU acceleration](../embedder.md#gpu-acceleration-opt-in).

## See also

- [Quickstart](../getting-started/quickstart.md)
- [Config knobs](config.md)
- [Errors](errors.md)
- Locked spec: [`dev/interfaces/typescript.md`](https://github.com/coreyt/fathomdb/blob/0.6.0-rewrite/dev/interfaces/typescript.md)
- TS↔Python parity matrix: [`dev/notes/12-TX-parity-matrix.md`](https://github.com/coreyt/fathomdb/blob/0.6.0-rewrite/dev/notes/12-TX-parity-matrix.md)
