# TypeScript API

Package: `fathomdb`. Authoritative spec:
[`dev/interfaces/typescript.md`](https://github.com/coreyt/fathomdb/blob/main/dev/interfaces/typescript.md).

> **Release state.** 0.8.21 is the current published release. This reference
> also documents the local 0.8.22 candidate; its candidate-only APIs are not
> available from a registry until the held release gates complete.

**TS SDK parity caveat.** The TS surface covers the same governed command set
and the same error taxonomy as Python, but Python remains the more heavily
exercised binding. Prefer Python for production pilots. See
[SDK parity](../positions/sdk-parity.md).

All runtime operations are Promise-returning. The TS↔Python parity
matrix is in [`dev/notes/12-TX-parity-matrix.md`](https://github.com/coreyt/fathomdb/blob/main/dev/notes/12-TX-parity-matrix.md).

## Top-level

```ts
import {
  Engine,
  admin,
  graph,
  read,
  type EngineConfig,
  type EngineOpenOptions,
  type WriteReceipt,
  type EraseReport,
  type IdSpace,
  type SearchHit,
  type SearchResult,
  type SearchFilter,
  type SoftFallback,
  type SoftFallbackBranch,
  type CounterSnapshot,
  type DenseReadiness,
  type ProjectionSpec,
  type ProjectionDelta,
  type ProjectionRole,
  type ProjectionRuntimeStatus,
  type ProjectionRuntimeStatusEntry,
  type ProjectionRuntimeUnavailabilityReason,
  type ProjectionStatusDenseReadiness,
  type SubscriberCallback,
  type AttachSubscriberOptions,
  type AdminConfigureOptions,
  FathomDbError,
  // ...27 concrete classes below the root, see errors reference
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

The structured open report is available after open via
`engine.openReport()` (below).

### `engine.openReport() -> OpenReport`

The structured open report defined in `dev/design/engine.md`:
`migrationVersionReached`, embedder identity confirmation, open-stage
data, `denseDisabled` / `denseDisabledReason`, and the embedder
telemetry fields (`embedderDownloadMs`, `embedderEvents`,
`embedderMeanCenteringRequired`, `embedderMeanVecPinned`).

### `engine.write(batch?) -> Promise<WriteReceipt>`

Enqueue a batch of canonical rows.

- `batch` (`unknown[]`) — caller-shaped canonical rows. Defaults to `[]`
  (a valid, item-less batch).

Every **node** item accepts these keys:

| Key | Type | Required | Meaning |
| --- | ---- | -------- | ------- |
| `kind` | `string` | **yes** | record kind |
| `body` | `string` | **yes** | record body |
| `sourceId` / `source_id` | `string` | **yes** | provenance — see below |
| `logicalId` | `string` | no | governed cross-re-ingestion identity |
| `state` | `string` | no | create-time existence state: `"active"` (default) or `"pending"` |
| `reason` | `string` | no | advisory cause for `state`; stored verbatim, never interpreted |
| `validFrom` / `valid_from` | `number` | no | world-time window, INCLUSIVE lower bound, epoch **seconds** UTC |
| `validUntil` / `valid_until` | `number` | no | world-time window, EXCLUSIVE upper bound, epoch **seconds** UTC |

An **edge** item takes `kind`, `from`, `to`, the same mandatory
`sourceId`, an optional `logicalId`, and the temporal pair
`tValid` / `tInvalid` (`number | null`, epoch **seconds** UTC; `null`
means "still valid"). Both camelCase and snake_case spellings are
accepted for every dual-spelled key (camelCase is consulted first).

**`sourceId` is MANDATORY (0.8.20).** `eraseSource` addresses rows *by*
`sourceId`, so a row written without one is reachable by no erasure
call. A missing, empty, whitespace-only or **reserved** (`_`-prefixed)
value rejects with `WriteValidationError`. Treat it as a public
identifier: use an opaque document or tenant id, never personal data —
see [Erasure](../operations/erasure.md).

**`logicalId`** — supplying it makes the write a transaction-time
**supersession** of the prior active version of that `logicalId` (the
prior version is tombstoned and the new one becomes active —
invalidate-not-delete). Active-row identity is scoped to `logicalId`
alone, so re-ingesting the same `logicalId` with a different `kind`
supersedes (it does not create a second active row). Omitting it is a
plain insert with a NULL `logicalId` that never collides with other
NULLs.

**Validity window** — half-open `[validFrom, validUntil)`. Omitting both
binds NULL/NULL (unbounded, the pre-0.8.20 default). Both bounds present
with `validFrom >= validUntil` is unsatisfiable and rejects with
`WriteValidationError`; validation runs before any insert, so the whole
batch is rejected. A one-sided window is never refused. A non-integral
bound rejects with `WriteValidationError` and is never truncated.

```ts
const receipt = await engine.write([
  { kind: "note", body: "hello", sourceId: "doc-42" },
  { kind: "note", body: "governed", sourceId: "doc-42", logicalId: "note:hello" },
  { kind: "mentions", from: "note:hello", to: "acme", sourceId: "doc-42",
    tValid: 1_546_300_800, tInvalid: null },
]);
```

- Returns: `WriteReceipt { cursor, rowCursors, danglingEdgeEndpoints }` —
  `cursor` is the batch high-water `write_cursor`; `rowCursors` are the
  per-row `write_cursor`s, 1:1 with the input batch order;
  `danglingEdgeEndpoints` (G8) counts the edge endpoints in this batch
  pointing at a non-existent or superseded node — see
  [`WriteReceipt`](#writereceipt).

### `engine.search(query, filter?, rerankDepth?, useGraphArm?, alpha?, poolN?, explain?, options?) -> Promise<SearchResult>`

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
- `options.limit` (`number`, default `10`) — maximum ranked hits returned. It
  must be an integer in `1..=100`; out-of-range values reject with
  `InvalidArgumentError`, never a silent clamp. `options` also accepts the
  existing `ReadView` fields.
- Resolves to a `SearchResult` whose `results` is a `SearchHit[]`; each
  [`SearchHit`](#searchhit) carries the matched record's `id`, `kind`, `body`,
  the **RRF-fused** `score`, the `branch` that produced it (`"graph_arm"`
  for nodes surfaced only via graph traversal), and `ceScore` (the per-candidate
  CE score for in-pool reranked hits, `null` otherwise).

> **Ranking is RRF (behavior-compat event).** Results are ordered by Reciprocal
> Rank Fusion (`Σ 1/(60 + rank)`) of the vector and text branches — the
> deliberate, documented 0.8.0 ranking change; pre-0.8.0 union-dedup ordering is
> not retained. See [hybrid search guide](../guides/hybrid-search-filtering.md).

### `engine.searchTextOnly(query, options?): Promise<SearchResult>`

Run direct FTS retrieval without embedding, vector retrieval, CE reranking, or
graph expansion. Matching node- and edge-body candidates are deterministically
body-deduplicated and ranked before `options.limit` is applied. The node
candidate input is fixed at 100, so for the same immutable selection, query,
and effective validity time, a smaller accepted limit returns the ordered prefix
of a larger one. Compare `validAsOf` calls only at the same explicit instant;
an omitted `validAsOf` resolves per call. This guarantee does not extend to
hybrid `engine.search`.

### `engine.searchProjectedText(query, name, filter?, options?): Promise<SearchResult>`

Search exactly one declared `SEARCHABLE` property-FTS projection. The final
`SearchOptions` accepts `limit` with the same default and validation as
`engine.search`; metadata and validity filters are applied before retained hits
consume that result budget.

### `engine.embed(text) -> Promise<number[]>`

Embed `text` with the engine's pinned default embedder
(`fathomdb-bge-small-en-v1.5`) and return the raw vector. Read-path
primitive for callers that need vectors under the engine's **own**
embedder identity (e.g. coverage-index clustering) rather than a
parallel, possibly-divergent embedder. Rejects with
`EmbedderNotConfiguredError` if the engine was opened without an
embedder (`useDefaultEmbedder: false`). Mirror of the Python
`engine.embed(text)` (0.8.6 Slice 10 brought it to Py↔TS parity).

### `engine.transition(logicalId, toState, reason?) -> Promise<void>`

**0.8.19.** Move a **governed** node between existence states, per the
engine-enforced legal-transition table. `toState` is a
`LifecycleState` (`"pending" | "active" | "deleted" | "purged"`):

| From | To | Effect |
| ---- | -- | ------ |
| `pending` | `active` | promote (clears `reason`) |
| `pending` | `deleted` | **rejected** |
| `active` | `deleted` | soft-delete (sets `reason`) |
| `deleted` | `active` | undelete (clears `reason`) |

`reason` is advisory and never interpreted by the engine. Keys on the
bare `logicalId` — the `logical` (`l:`) id space only; a `content`
(`h:`) or `passage` (`p:`) id throws `NotLifecycleAddressableError`. An
illegal move throws `IllegalTransitionError` carrying `fromState`,
`toState` and `legal`.

### `engine.purge(logicalId: string) -> Promise<void>`

**0.8.19.** Irreversibly hard-erase a governed node across every
row-owned target — all versions, its FTS/vector shadows, and its
touching edges (cascade-removed).

**Deleted-first:** legal only from `"deleted"`, otherwise
`IllegalTransitionError`. **Idempotent:** purging an absent or
already-purged id is a no-op success. A non-`l:` id throws
`NotLifecycleAddressableError`.

`purge` addresses one **governed** node. For anonymous content — rows
written with no `logicalId` — use `eraseSource` below.

### `engine.eraseSource(sourceId: string) -> Promise<EraseReport>`

**0.8.20.** Erase every canonical row carrying `sourceId`, together with its
row-owned projections (FTS5, `vec0`, `search_index_v2`), then finish the erasure
at rest — redact the erased ids from the telemetry sink and truncate the WAL.

The **companion to `purge`, not a duplicate of it.** `purge` addresses a
*governed* node by `logicalId`; `eraseSource` addresses *anonymous* content —
rows written with no `logicalId`, which `purge` cannot reach at all. Together
they make every canonical row erasable from the SDK alone, with no CLI on
`PATH`.

Idempotent: erasing an absent or already-erased source is a zero-count success,
so an interrupted erasure obligation can be retried without a pre-check.

Rejects with `WriteValidationError` for an empty, whitespace-only, or
**reserved** (`_`-prefixed) `sourceId`. The engine's reserved namespace
(`_engine:*` substrate and the `_legacy:pre-0.8.20` migration cohort) is
reachable only through `fathomdb recover --excise-source`. Rejects with
`ErasureIncompleteError` (carrying `stage` and `detail`) rather than reporting
success if the erasure could not be completed at rest.

Resolves to an `EraseReport` with `sourceRef`, `nodesExcised`, `edgesExcised`,
and `projectionsInvalidated`. Mirror of the Python
`engine.erase_source(source_id)` (Py↔TS parity).

See [Erasure](../operations/erasure.md) for what this does and does **not**
guarantee, and for the non-PII `sourceId` rule.

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

## `read.*` — governed read verbs (including 0.8.22 Slice 22)

```ts
import { read } from "fathomdb";
```

The retrieval verbs below use the engine's **ReaderWorkerPool DEFERRED-tx
snapshot path**, preserving single-writer isolation. `read.projections` and
`read.projectionStatus` are different: they are pure introspection queries
through the ordinarily opened engine and may briefly take its connection lock.
They do not configure, write, or schedule work, but do not promise a separately
opened read-only SQLite mode. Verb names are camelCase in TS but the governed
allowlist names stay dotted snake_case (`read.get_many`).

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

### Projection registry and derived readiness

`engine.configureProjections(specs, drop?)` declares the durable projection
registry. It is idempotent: omitting a live declaration does not delete it,
while an explicit destructive change requires its name in `drop`.
`read.projections(engine): Promise<ProjectionSpec[]>` returns those durable
declarations in name order.

For an effective vector declaration, the returned
`ProjectionSpec.vectorDenseReadiness: DenseReadiness | null` is engine-set
read metadata, never part of configuration. Supplying a valid readiness value
with `vector: true` to `configureProjections` is accepted but inert, so a result
from `read.projections` can be configured again as a no-op; an invalid spelling
or a readiness with `vector: false` is rejected. With no usable dense runtime
(including an equivalence refusal), the engine reports `"unavailable"`.
With a usable runtime it reports `"embedding"` while eligible shared work is
outstanding; after `await engine.drain(...)` completes and no further work is
issued, it reports `"ready"`. `DenseReadiness` is exactly `"unavailable" |
"embedding" | "ready"`.

### `read.projectionStatus(engine): Promise<ProjectionRuntimeStatus>`

Read current projection-runtime status without configuring projections or
changing the registry, storage, scheduler, or work queue. It is a status facade,
not a decorated `ProjectionSpec` and not a per-projection completion report.
It may briefly take the ordinarily opened engine connection lock; it is not a
ReaderWorkerPool request and does not promise a separate read-only SQLite
connection.

```ts
interface ProjectionRuntimeStatus {
  runtimeEmbedderAvailable: boolean;
  runtimeUnavailabilityReason:
    | "none"
    | "no_runtime"
    | "vector_equivalence_disabled";
  projections: ProjectionRuntimeStatusEntry[];
  vectorUnsupportedKinds: string[];
}
interface ProjectionRuntimeStatusEntry {
  name: string;
  denseReadiness: "not_declared" | "unavailable" | "embedding" | "ready";
}
```

`"none"` is returned exactly when `runtimeEmbedderAvailable` is true. Entries
are sorted by name. `"not_declared"` means the declaration has no effective
vector arm (it needs both `searchable` and a vector sub-object), so a legacy
non-searchable vector sub-object remains `"not_declared"`. The other readiness
values are corpus-wide shared-pipeline facts and can repeat across effective
vector declarations. `vectorUnsupportedKinds` is sorted and deduplicated; it is
`[]` when no effective vector arm exists.

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
  writeCursor: number; // engine-internal positional cursor — NOT SearchHit.id
}
```

Returned by `read.get` / `read.getMany` for an **active** canonical node
(`superseded_at IS NULL`). Mirrors the Python `NodeRecord`.

`logicalId` is the caller-facing identity here; `writeCursor` is the engine's
positional book-keeping value, reassigned on re-projection and **not** the same
carrier as [`SearchHit.id`](#searchhit) (which is a typed `IdSpace`).

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
interface IdSpace {
  space: string; // "logical" | "content" | "passage"
  value: string; // the BARE id (id-space prefix stripped)
}

interface SearchHit {
  id: IdSpace; // typed, non-null, id-space-total hit identity
  kind: string;
  body: string;
  score: number; // G9 RRF-fused relevance (Σ 1/(60+rank)); higher = better
  branch: SoftFallbackBranch; // "vector" | "text" | "text_edge" | "graph_arm"
  sourceId: string | null; // provenance (`eraseSource` arg); set on EVERY hit (TC-31)
  ceScore: number | null; // 0.8.5 CE score (sigmoid logit) for in-pool reranked hits
}
```

> **BREAKING since 0.8.9.** `SearchHit.id` was a `number` row cursor. It is now
> the typed `IdSpace` above — the **permanent** caller-facing identity, not an
> interim carrier. `space` is `"logical"` for governed rows (prefix `l:`),
> `"content"` for doc-seeded rows (`h:`), `"passage"` for synthetic passages
> (`p:`); `value` is the bare id with that prefix stripped, and
> `` `${prefix}${value}` `` reproduces the pre-0.8.19 `stableId` byte-for-byte.
> It is stable across sessions and re-ingest, and never participates in ranking.
> The engine's positional `write_cursor` is internal book-keeping and is **not
> surfaced by the bindings**. Only `logical`-space ids are lifecycle-addressable
> by `transition` / `purge`.

`score` is the **G9 RRF-fused** relevance (higher = more relevant), optionally
recency-reweighted. Raw `vec_distance_l2` (vector) and `bm25()` (text) are fused
on **rank**, never compared raw (they are not comparable). `branch` tags which
branch produced the representative hit (vector-first when both surface a body).
`ceScore` (0.8.5 / EXP-0) is the per-candidate cross-encoder score
(`sigmoid(ce_logit)`) for hits inside the reranked pool, `null` otherwise.

`sourceId` carries the hit's source-document provenance — the identifier
`engine.eraseSource` consumes. **TC-31 (0.8.20): it is populated on
every hit path**, not just the graph arm, so a caller can always resolve a hit
back to the document it came from. Node hits (text/vector) carry the node's own
`sourceId`; edge hits (edge-FTS, vector edge-fact) carry the edge's own;
graph-arm hits carry the traversed edge's. It is `null` only when the stored row
genuinely has NULL provenance — written before 0.8.20, or a governed row spared
by the step-21 backfill. `sourceId` never participates in ranking.

### `SearchFilter`

```ts
interface SearchFilter {
  sourceType?: string;
  kind?: string;
  createdAfter?: number; // created_at >= bound (unix seconds)
  status?: string;
  attributes?: [string, string][];
}
```

G10 — a **closed** metadata filter (not an open DSL) for `engine.search`. Each
present field constrains the vector branch in a single phase-1 KNN statement and
constrains the text branch by the same metadata; omitted/all-`undefined` is the
unfiltered path (byte-identical to the pre-filter query). `status` filters the
vec0 `status` column, which ships an **empty-string sentinel only** (no real
population source yet — vec0 TEXT metadata is not NULL-able), so a
`status: "open"`-style filter prunes every row until a population slice lands.
`attributes` is ordered AND equality over declared `filterable` projections;
its values are canonical text, so projected string `"1"` and number `1` both
match `"1"`.

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
search-plus-expansion. Its reads ride the same **ReaderWorkerPool DEFERRED-tx
snapshot path** as the retrieval verbs in `read.*`; projection introspection
uses the ordinarily opened engine connection instead.

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

### `graph.searchExpand(engine, query, depth, filter?, options?): Promise<SearchExpandResult>`

G6 — FTS/vector search (G1) followed by bounded BFS expansion.

- `query` (`string`) — free-text or embedding query.
- `depth` (`number`) — BFS hop limit; 0 skips expansion. Depth > 3 raises
  `InvalidArgumentError`.
- `filter` (`SearchFilter | undefined`) — optional metadata filter (same as
  `engine.search`).
- `options.searchLimit` (`number`, default `10`) — maximum initial ranked
  `searchHits`, in `1..=100`; it does not change the 50-per-root expansion cap.

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

`fathomdb` exports `FathomDbError` (the catch-all base) plus **27**
concrete classes below it. See [errors reference](errors.md).

The lifecycle / erasure verbs reject with `IllegalTransitionError`,
`NotLifecycleAddressableError`, `ErasureIncompleteError` and
`WriteValidationError`; `configureProjections` rejects with
`ProjectionDestructiveError` and `WriteValidationError`.

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
- [Erasure](../operations/erasure.md)
- Locked spec: [`dev/interfaces/typescript.md`](https://github.com/coreyt/fathomdb/blob/main/dev/interfaces/typescript.md)
- TS↔Python parity matrix: [`dev/notes/12-TX-parity-matrix.md`](https://github.com/coreyt/fathomdb/blob/main/dev/notes/12-TX-parity-matrix.md)
