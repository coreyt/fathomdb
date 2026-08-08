# Python API

Module: `fathomdb`. Authoritative spec:
[`dev/interfaces/python.md`](https://github.com/coreyt/fathomdb/blob/main/dev/interfaces/python.md).

> **Release state.** 0.8.21 is the current published release. This reference
> also documents the local 0.8.22 candidate; its candidate-only APIs are not
> available from a registry until the held release gates complete.

## Top-level

```python
from fathomdb import (
    DenseReadiness,
    Engine,
    EngineConfig,
    IdSpace,
    NodeRecord,
    OpStoreRow,
    ProjectionDelta,
    ProjectionRole,
    ProjectionRuntimeStatus,
    ProjectionRuntimeStatusEntry,
    ProjectionRuntimeUnavailabilityReason,
    ProjectionSpec,
    ProjectionStatusDenseReadiness,
    SearchFilter,
    SearchHit,
    SearchResult,
    SoftFallback,
    SoftFallbackBranch,
    WriteReceipt,
    CounterSnapshot,
    admin,
    errors,
    graph,
    read,
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

The structured open report is available after open via
`engine.open_report()` (below).

### `engine.open_report() -> OpenReport`

The structured open report defined in `dev/design/engine.md`:
`migration_version_reached`, embedder identity confirmation,
open-stage data, `dense_disabled` / `dense_disabled_reason`, and the
embedder telemetry fields (`embedder_download_ms`, `embedder_events`,
`embedder_mean_centering_required`, `embedder_mean_vec_pinned`).

### `engine.write(batch=None) -> WriteReceipt`

Enqueue a batch of canonical rows. Synchronous; blocks until the
writer thread has accepted the batch.

- `batch` (`list[Any] | None`) — caller-shaped canonical rows.
  Defaults to `[]` (a valid, item-less batch).

Every **node** item accepts these keys:

| Key | Type | Required | Meaning |
| --- | ---- | -------- | ------- |
| `kind` | `str` | **yes** | record kind |
| `body` | `str` | **yes** | record body |
| `source_id` | `str` | **yes** | provenance — see below |
| `logical_id` | `str` | no | governed cross-re-ingestion identity |
| `state` | `str` | no | create-time existence state: `"active"` (default) or `"pending"` |
| `reason` | `str` | no | advisory cause for `state`; stored verbatim, never interpreted |
| `valid_from` | `int` | no | world-time window, INCLUSIVE lower bound, epoch **seconds** UTC |
| `valid_until` | `int` | no | world-time window, EXCLUSIVE upper bound, epoch **seconds** UTC |

An **edge** item takes `kind`, `from`, `to`, the same mandatory
`source_id`, an optional `logical_id`, and the temporal pair
`t_valid` / `t_invalid` (`int | None`, epoch **seconds** UTC; `None`
means "still valid").

**`source_id` is MANDATORY (0.8.20).** `erase_source` addresses rows
*by* `source_id`, so a row written without one is reachable by no
erasure call. A missing, empty, whitespace-only or **reserved**
(`_`-prefixed) value raises `WriteValidationError`. Treat it as a
public identifier: use an opaque document or tenant id, never personal
data — see [Erasure](../operations/erasure.md).

**`logical_id`** — supplying it makes the write a transaction-time
**supersession** of the prior active version of that `logical_id`: the
prior version is tombstoned and the new version becomes active
(invalidate-not-delete). Active-row identity is scoped to `logical_id`
alone, so re-ingesting the same `logical_id` with a different `kind`
supersedes (it does not create a second active row). Omitting it is a
plain insert with a NULL `logical_id`, which never collides with other
NULL rows.

**Validity window** — half-open `[valid_from, valid_until)`. Omitting
both binds NULL/NULL (unbounded, the pre-0.8.20 default). Both bounds
present with `valid_from >= valid_until` is unsatisfiable and raises
`WriteValidationError`; validation runs before any insert, so the whole
batch is rejected. A one-sided window is never refused. A non-integer
bound raises `WriteValidationError` and is never coerced (`bool` is
rejected explicitly).

```python
receipt = engine.write([
    {"kind": "note", "body": "hello", "source_id": "doc-42"},
    {"kind": "note", "body": "governed", "source_id": "doc-42",
     "logical_id": "note:hello"},
    {"kind": "mentions", "from": "note:hello", "to": "acme",
     "source_id": "doc-42", "t_valid": 1_546_300_800, "t_invalid": None},
])
```

- Returns: `WriteReceipt(cursor: int, row_cursors: tuple[int, ...],
  dangling_edge_endpoints: int)`. `cursor` advances monotonically across
  writes (the batch high-water cursor); `row_cursors` are the per-row
  `write_cursor`s, 1:1 with the input batch order;
  `dangling_edge_endpoints` (G8) counts the edge endpoints in this batch
  pointing at a non-existent or superseded node — see
  [`WriteReceipt`](#writereceipt).

### `engine.search(query, filter=None, *, rerank_depth=0, use_graph_arm=False, alpha=None, pool_n=None, limit=10) -> SearchResult`

Run hybrid retrieval (FTS5 + vector) for `query`, ranked by **G9 RRF fusion**,
with optional CPU cross-encoder reranking (0.8.1 R1) and optional graph-BFS
third arm (0.8.1 R3).

- `query` (`str`).
- `filter` ([`SearchFilter`](#searchfilter) | `None`) — optional closed metadata
  filter. `None` (or an all-`None` filter) is the unfiltered path.
- `rerank_depth` (`int`, default `0`) — 0.8.1 R1 opt-in. `0` (default) uses the
  identity / soft-fallback path: byte-identical to the pre-0.8.1 fused order.
  `N > 0` applies a CPU cross-encoder (TinyBERT-L-2, ≈4 MB, p50 ≈ 1.5 ms/pair)
  over the top-N fused hits using score-blend (α=0.3 × CE + 0.7 × RRF-norm).
  Must be a non-negative integer; negative values raise `ValueError`. In the
  default build (no `default-reranker` feature), depth > 0 returns the identity
  order (model absent → soft-fallback).
- `use_graph_arm` (`bool`, default `False`) — 0.8.1 R3 opt-in. When `True`,
  seeds a BFS over temporal fact-edges from the top-10 fused hits (depth ≤ 3,
  cap 50). Edges with `t_invalid` in the past are excluded. Newly-reachable
  nodes are fused as a third RRF arm (`RRF_WEIGHT_GRAPH = 1.0`). Default
  `False` produces byte-identical results to the pre-R3 two-arm pipeline.
  Must be a `bool`; non-bool raises `TypeError`.
- `alpha` (`float | None`, default `None`) — 0.8.5 (EXP-0) CE-blend weight,
  clamped to `[0, 1]` in the engine. `None` ⇒ `0.3` — the **C6 factoid-guard**
  default that prevents a high-CE-but-wrong candidate from displacing a
  BM25-correct factoid. **`alpha=1.0` is opt-in for the agentic-answer / memory
  path** (the measured Mem0-parity config); the `0.3` default protects naive
  factoid lookups. Only effective when `rerank_depth > 0` and the CE model is
  loaded.
- `pool_n` (`int | None`, default `None`) — 0.8.5 (EXP-0) reranked-pool size,
  clamped to the hit count. `None` ⇒ `rerank_depth` (preserves the prior
  pool == depth semantics). Note `rerank_depth == 0` is still the identity gate,
  so `rerank_depth=0, pool_n=10` does **not** rerank.
- `limit` (`int`, default `10`) — maximum ranked hits returned. It must be in
  `1..=100`; zero, negative values, and values above 100 raise
  `InvalidArgumentError` and are never silently clamped.
- Returns: `SearchResult(projection_cursor: int, soft_fallback:
  SoftFallback | None, results: list[SearchHit])`. Each
  [`SearchHit`](#searchhit) carries the matched record's `id`, `kind`,
  `body`, the **RRF-fused** `score`, the `branch` that produced it
  (`"graph_arm"` for nodes surfaced only via graph traversal), and `ce_score`
  (the per-candidate CE score for in-pool reranked hits, `None` otherwise).

> **Ranking is RRF (behavior-compat event).** Results are ordered by Reciprocal
> Rank Fusion (`Σ 1/(60 + rank)`) of the vector and text branches — a body the
> two branches agree on ranks above one only a single branch found. This is the
> deliberate, documented 0.8.0 ranking change; pre-0.8.0 union-dedup ordering is
> not retained. See [hybrid search guide](../guides/hybrid-search-filtering.md).

### `engine.search_text_only(query, view=None, *, limit=10) -> SearchResult`

Run direct FTS retrieval without embedding, vector retrieval, CE reranking, or
graph expansion. Matching node- and edge-body candidates are deterministically
body-deduplicated and ranked before `limit` is applied. The node candidate input
is fixed at 100, so for the same immutable selection, query, and effective
validity time, a smaller accepted limit returns the ordered prefix of a larger
one. Compare `view=ReadView(valid_as_of=...)` calls only at the same explicit
instant; an omitted `valid_as_of` resolves per call. This guarantee does not
extend to hybrid `engine.search`.

### `engine.search_projected_text(query, name, filter=None, *, view=None, limit=10) -> SearchResult`

Search exactly one declared `SEARCHABLE` property-FTS projection. `limit` has
the same `1..=100` validation and default of 10; metadata and validity filters
are applied before retained hits consume that result budget.

### `engine.embed(text: str) -> list[float]`

Embed `text` with the engine's pinned default embedder
(`fathomdb-bge-small-en-v1.5`) and return the raw vector. Read-path
primitive for callers that need vectors under the engine's **own**
embedder identity (e.g. coverage-index clustering) rather than a
parallel, possibly-divergent embedder. Raises
`EmbedderNotConfiguredError` if the engine was opened without an
embedder (`use_default_embedder=False`). Mirrored in TS as
`engine.embed(text)`.

### `engine.transition(logical_id, to_state, reason=None) -> None`

**0.8.19.** Move a **governed** node between existence states, per the
engine-enforced legal-transition table:

| From | To | Effect |
| ---- | -- | ------ |
| `pending` | `active` | promote (clears `reason`) |
| `pending` | `deleted` | **rejected** |
| `active` | `deleted` | soft-delete (sets `reason`) |
| `deleted` | `active` | undelete (clears `reason`) |

`reason` is advisory and never interpreted by the engine. Keys on the
bare `logical_id` — the `logical` (`l:`) id space only; a `content`
(`h:`) or `passage` (`p:`) id raises `NotLifecycleAddressableError`. An
illegal move (a `purged`/`pending` target, a self-loop, an absent node)
raises `IllegalTransitionError` carrying `from_state`, `to_state` and
`legal`.

### `engine.purge(logical_id: str) -> None`

**0.8.19.** Irreversibly hard-erase a governed node across every
row-owned target — all versions, its FTS/vector shadows, and its
touching edges (cascade-removed).

**Deleted-first:** legal only from `deleted`, otherwise
`IllegalTransitionError`. **Idempotent:** purging an absent or
already-purged id is a no-op success. A non-`l:` id raises
`NotLifecycleAddressableError`.

`purge` addresses one **governed** node. For anonymous content — rows
written with no `logical_id` — use `erase_source` below.

### `engine.erase_source(source_id: str) -> EraseReport`

**0.8.20.** Erase every canonical row carrying `source_id`, together with its
row-owned projections (FTS5, `vec0`, `search_index_v2`), then finish the erasure
at rest — redact the erased ids from the telemetry sink and truncate the WAL.

The **companion to `purge`, not a duplicate of it.** `purge` addresses a
*governed* node by `logical_id`; `erase_source` addresses *anonymous* content —
rows written with no `logical_id`, which `purge` cannot reach at all. Together
they make every canonical row erasable from the SDK alone, with no CLI on
`PATH`.

Idempotent: erasing an absent or already-erased source is a zero-count success,
so an interrupted erasure obligation can be retried without a pre-check.

Raises `WriteValidationError` for an empty, whitespace-only, or **reserved**
(`_`-prefixed) `source_id`. The engine's reserved namespace (`_engine:*`
substrate and the `_legacy:pre-0.8.20` migration cohort) is reachable only
through `fathomdb recover --excise-source`. Raises `ErasureIncompleteError`
(carrying `stage` and `detail`) rather than reporting success if the erasure
could not be completed at rest.

Returns an `EraseReport` with `source_ref`, `nodes_excised`, `edges_excised`,
and `projections_invalidated`. Mirrored in TS as `engine.eraseSource(sourceId)`.

See [Erasure](../operations/erasure.md) for what this does and does **not**
guarantee, and for the non-PII `source_id` rule.

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

## `read.*` — governed read verbs (including 0.8.22 Slice 22)

```python
from fathomdb import read
```

The retrieval verbs below use the engine's **ReaderWorkerPool DEFERRED-tx
snapshot path**, preserving single-writer isolation. `read.projections` and
`read.projection_status` are different: they are pure introspection queries
through the ordinarily opened engine and may briefly take its connection lock.
They do not configure, write, or schedule work, but do not promise a separately
opened read-only SQLite mode.

### `read.get(engine, logical_id: str) -> NodeRecord | None`

Active-only point lookup by `logical_id` (active = `superseded_at IS NULL`). A
superseded version is never returned. A missing or superseded id returns `None`
— a **normal absence, not an exception** (a typed `NotFound` is a later-slice
concern).

### `read.get_many(engine, logical_ids: list[str]) -> list[NodeRecord | None]`

Batched point lookup. Returns one slot per requested id, **in request order**;
a missing/superseded id is `None` in its slot (partial result, never
all-or-nothing). `read.get` delegates to `read.get_many`.

### `read.collection(engine, collection, *, after_id=None, limit) -> list[OpStoreRow]`

Paginated op-store read-back over `operational_mutations` for `collection`,
**`ORDER BY id`**. `limit` is **mandatory** (the engine clamps it to a ~1M cap,
so no call yields an unbounded read); `after_id` is the exclusive cursor for the
next page.

### `read.mutations(engine, collection, *, after_id=None, limit) -> list[OpStoreRow]`

Mutation-log-oriented alias surface over the **same** op-store read-back as
`read.collection` (identical args + semantics).

### `read.list(engine, kind, predicates=None, *, limit=100) -> list[NodeRecord]`

*(G4 / Slice 35)* List **active** `canonical_nodes` of the given `kind`
(`superseded_at IS NULL`), optionally filtered by a list of closed
`Predicate` dicts (AND-combined), up to `limit` rows (default 100).

Each predicate dict has the shape:

```python
{"type": "eq"|"gt"|"gte"|"lt"|"lte", "path": str, "value": str | int | bool}
```

`path` must be from the engine allowlist: `$.status`, `$.priority`,
`$.tags`, `$.kind`, `$.created_at`. A non-allowlisted path raises
`InvalidFilterError` (never a panic). Values are **always bound as
parameterized SQL** — never interpolated (injection-safe per ADR
D-F4). An empty `predicates` (or `None`) is the unfiltered path.

```python
from fathomdb import Engine, read
from fathomdb.errors import InvalidFilterError

engine = Engine.open("my.db")
# All active task nodes:
tasks = read.list(engine, "task")
# Filtered: open tasks with priority > 5:
open_high = read.list(engine, "task", predicates=[
    {"type": "eq",  "path": "$.status",   "value": "open"},
    {"type": "gt",  "path": "$.priority", "value": 5},
])
```

### Projection registry and derived readiness

`engine.configure_projections(specs, drop=None) -> ProjectionDelta` declares
the durable projection registry. It is idempotent: omitting a live declaration
does not delete it, while an explicit destructive change requires its name in
`drop`. `read.projections(engine) -> list[ProjectionSpec]` returns those
durable declarations in name order.

For an effective vector declaration, the returned
`ProjectionSpec.vector_dense_readiness: DenseReadiness | None` is engine-set
read metadata, never part of configuration. Supplying a valid readiness value
with `vector=True` to `configure_projections` is accepted but inert, so a result
from `read.projections` can be configured again as a no-op; an invalid spelling
or a readiness with `vector=False` is rejected. With no usable dense runtime
(including an equivalence refusal), the engine reports `"unavailable"`.
With a usable runtime it reports `"embedding"` while eligible shared work is
outstanding; after `engine.drain(...)` completes and no further work is issued,
it reports `"ready"`. `DenseReadiness` is exactly `"unavailable" |
"embedding" | "ready"`.

### `read.projection_status(engine) -> ProjectionRuntimeStatus`

Read the current projection-runtime status without configuring projections or
changing the registry, storage, scheduler, or work queue. It is a status facade,
not a decorated `ProjectionSpec` and not a per-projection completion report.
It may briefly take the ordinarily opened engine connection lock; it is not a
ReaderWorkerPool request and does not promise a separate read-only SQLite
connection.

The frozen result has these fields:

```python
ProjectionRuntimeStatus(
    runtime_embedder_available: bool,
    runtime_unavailability_reason: (
        "none" | "no_runtime" | "vector_equivalence_disabled"
    ),
    projections: tuple[ProjectionRuntimeStatusEntry, ...],
    vector_unsupported_kinds: tuple[str, ...],
)
ProjectionRuntimeStatusEntry(
    name: str,
    dense_readiness: "not_declared" | "unavailable" | "embedding" | "ready",
)
```

`"none"` is returned exactly when `runtime_embedder_available` is true.
Entries are sorted by name. `"not_declared"` means the declaration has no
effective vector arm (it needs both `searchable` and a vector sub-object), so a
legacy non-searchable vector sub-object remains `"not_declared"`. The other
readiness values are corpus-wide shared-pipeline facts and can repeat across
effective vector declarations. `vector_unsupported_kinds` is sorted and
deduplicated; it is `()` when no effective vector arm exists.

## `graph.*` — graph traversal (Slice 20 / G5 + G6)

```python
from fathomdb import graph
```

The `graph.*` namespace exposes bounded BFS traversal and hybrid
search-plus-expansion. Its reads ride the same **ReaderWorkerPool DEFERRED-tx
snapshot path** as the retrieval verbs in `read.*`; projection introspection
uses the ordinarily opened engine connection instead.

### `graph.neighbors(engine, logical_id, depth, direction="both") -> list[NodeRecord]`

G5 — bounded BFS from `logical_id` over `canonical_edges`.

- `logical_id` (`str`) — the root node's stable identity.
- `depth` (`int`) — hop limit; **must be 1, 2, or 3**.
  Depth > 3 raises `InvalidArgumentError`.
- `direction` (`str`) — edge direction to follow: `"outgoing"` (from→to),
  `"incoming"` (to→from), or `"both"`.

Returns up to **50** `NodeRecord`s reachable within `depth` hops
(root excluded). Edges with `t_invalid` in the past are silently skipped
(valid-time filter). Returns `[]` when the root has no reachable neighbors.

Raises `InvalidArgumentError` for depth > 3 or an unrecognised direction.

### `graph.search_expand(engine, query, depth, *, source_type=None, kind=None, created_after=None, status=None, search_limit=10) -> SearchExpandResult`

G6 — FTS/vector search (G1) followed by bounded BFS expansion.

- `query` (`str`) — free-text or embedding query (same as `engine.search`).
- `depth` (`int`) — BFS hop limit for expansion; 0 skips expansion.
  Depth > 3 raises `InvalidArgumentError`.
- Optional filter kwargs match `engine.search` semantics.
- `search_limit` (`int`, default `10`) — maximum initial ranked `search_hits`,
  in `1..=100`. It does not change the 50-per-root graph expansion cap.

Returns a `SearchExpandResult`. Nodes that appear in both the search hit set
and the traversal reach appear **only** in `search_hits` (deduplication:
search score takes priority).

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

### `NodeRecord`

```python
@dataclass(frozen=True)
class NodeRecord:
    logical_id: str
    kind: str
    body: str
    write_cursor: int   # engine-internal positional cursor — NOT SearchHit.id
```

Returned by `read.get` / `read.get_many` for an **active** canonical node
(`superseded_at IS NULL`). Mirrors the TypeScript `NodeRecord`.

`logical_id` is the caller-facing identity here; `write_cursor` is the engine's
positional book-keeping value, reassigned on re-projection and **not** the same
carrier as [`SearchHit.id`](#searchhit) (which is a typed `IdSpace`).

### `OpStoreRow`

```python
@dataclass(frozen=True)
class OpStoreRow:
    id: int               # operational_mutations PK + the after_id cursor key
    collection: str
    record_key: str
    op_kind: str          # always "append"
    payload: str          # the stored payload_json
    schema_id: str | None
    write_cursor: int
```

Returned by `read.collection` / `read.mutations`. `id` is the after-id cursor
key. Mirrors the TypeScript `OpStoreRow`.

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
class IdSpace:
    space: str       # "logical" | "content" | "passage"
    value: str       # the BARE id (id-space prefix stripped)

@dataclass(frozen=True)
class SearchHit:
    id: IdSpace      # typed, non-null, id-space-total hit identity
    kind: str
    body: str
    score: float     # G9 RRF-fused relevance (Σ 1/(60+rank)); higher = better
    branch: SoftFallbackBranch  # Literal["vector", "text", "text_edge", "graph_arm"]
    source_id: str | None = None  # provenance (`erase_source` arg); set on EVERY hit (TC-31)
    ce_score: float | None = None  # 0.8.5 CE score (sigmoid logit) for in-pool reranked hits
```

> **BREAKING since 0.8.9.** `SearchHit.id` was an `int` row cursor. It is now
> the typed `IdSpace` above — the **permanent** caller-facing identity, not an
> interim carrier. `space` is `"logical"` for governed rows (prefix `l:`),
> `"content"` for doc-seeded rows (`h:`), `"passage"` for synthetic passages
> (`p:`); `value` is the bare id with that prefix stripped, and
> `f"{prefix}{value}"` reproduces the pre-0.8.19 `stable_id` byte-for-byte. It
> is stable across sessions and re-ingest, and never participates in ranking.
> The engine's positional `write_cursor` is internal book-keeping and is **not
> surfaced by the bindings**. Only `logical`-space ids are lifecycle-addressable
> by `transition` / `purge`.

`score` is the **G9 RRF-fused** relevance (higher = more relevant), optionally
recency-reweighted. Raw `vec_distance_l2` (vector) and `bm25()` (text) are fused
on **rank**, never compared raw (they are not comparable). `branch` tags which
branch produced the representative hit (vector-first when both surface a body).
`ce_score` (0.8.5 / EXP-0) is the per-candidate cross-encoder score
(`sigmoid(ce_logit)`) for hits inside the reranked pool, `None` otherwise.

`source_id` carries the hit's source-document provenance — the identifier
`engine.erase_source` consumes. **TC-31 (0.8.20): it is populated on
every hit path**, not just the graph arm, so a caller can always resolve a hit
back to the document it came from. Node hits (text/vector) carry the node's own
`source_id`; edge hits (edge-FTS, vector edge-fact) carry the edge's own;
graph-arm hits carry the traversed edge's. It is `None` only when the stored row
genuinely has NULL provenance — written before 0.8.20, or a governed row spared
by the step-21 backfill. `source_id` never participates in ranking.

### `SearchFilter`

```python
@dataclass(frozen=True)
class SearchFilter:
    source_type: str | None = None
    kind: str | None = None
    created_after: int | None = None   # created_at >= bound (unix seconds)
    status: str | None = None
    attributes: tuple[tuple[str, str], ...] = ()
```

G10 — a **closed** metadata filter (not an open DSL) for `engine.search`. Each
present field constrains the vector branch in a single phase-1 KNN statement and
constrains the text branch by the same metadata; `None`/all-`None` is the
unfiltered path (byte-identical to the pre-filter query). `status` filters the
vec0 `status` column, which ships an **empty-string sentinel only** (no real
population source yet — vec0 TEXT metadata is not NULL-able), so a
`status="open"`-style filter prunes every row until a population slice lands.
`attributes` is ordered AND equality over declared `filterable` projections;
its values are canonical text, so projected string `"1"` and number `1` both
match `"1"`.

### `SoftFallback`

```python
@dataclass(frozen=True)
class SoftFallback:
    branch: SoftFallbackBranch  # Literal["vector", "text", "text_edge", "graph_arm"]
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

### `ExpandedNode`

```python
@dataclass(frozen=True)
class ExpandedNode:
    node: NodeRecord      # the reachable node
    hop_count: int        # BFS distance from the nearest search-hit root
```

Returned in `SearchExpandResult.expanded`. Only nodes NOT already in
`search_hits` appear here.

### `SearchExpandResult`

```python
@dataclass(frozen=True)
class SearchExpandResult:
    search_hits: list[SearchHit]     # original RRF-scored search results
    expanded: list[ExpandedNode]     # nodes reachable by traversal, not in search_hits
    all_logical_ids: list[str]       # deduplicated union of both sets
```

Returned by `graph.search_expand`. `all_logical_ids` contains the
`logical_id` strings for every node in both `search_hits` and `expanded`.

## Errors

`fathomdb.errors` exports `EngineError` (the catch-all base) plus **27**
concrete classes below it. See [errors reference](errors.md) for the full
matrix and recovery-hint codes.

The lifecycle / erasure verbs raise `IllegalTransitionError`,
`NotLifecycleAddressableError`, `ErasureIncompleteError` and
`WriteValidationError`; `configure_projections` raises
`ProjectionDestructiveError` and `WriteValidationError`.

## Embedder device (GPU)

There is **no Python API** for selecting the embedder device — it is chosen by a
build-time cargo feature (`embed-cuda` / `embed-metal`) plus the
`FATHOMDB_EMBED_DEVICE` environment variable (`cpu` default · `cuda` · `cuda:N` ·
`metal`), resolved when the engine opens. The default (CPU) behavior is
unchanged. See [Default Embedder → GPU acceleration](../embedder.md#gpu-acceleration-opt-in).

## See also

- [Quickstart](../getting-started/quickstart.md)
- [Config knobs](config.md)
- [Errors](errors.md)
- [Erasure](../operations/erasure.md)
- Locked spec: [`dev/interfaces/python.md`](https://github.com/coreyt/fathomdb/blob/main/dev/interfaces/python.md)
