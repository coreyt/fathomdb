# Query

Fluent, immutable query builder for fetching nodes from a fathomdb database.
Instances are created via `Engine.nodes()`. Each filter or traversal method
returns a new `Query`, leaving the original unchanged. See the
[Querying](../guides/querying.md) guide for usage patterns and examples.

The primary retrieval entry point is `Query.search()`. It returns a
[`SearchBuilder`](#searchbuilder) whose terminal `execute()` is statically
typed to return [`SearchRows`](./types.md#searchrows) rather than
`QueryRows`. The `text_search()`, `vector_search()`, and
`fallback_search()` methods remain available as advanced
modality-specific overrides — they share the `SearchRows` / `SearchHit`
result family so calling code has the same shape on any surface.

All text-valued search surfaces — `search()`, `text_search()`, and the
`strict_query` argument of `fallback_search()` — accept the same
constrained safe subset of familiar search syntax: bare terms, quoted
phrases, implicit `AND`, uppercase `OR`, and uppercase `NOT`. Unsupported
syntax stays literal instead of being passed through as raw FTS5 control
syntax.

For the strict query grammar, see
[Text Query Syntax](../guides/text-query-syntax.md). For the unified
retrieval pipeline, block precedence, per-branch counts, and the
`fallback_used` / `strict_hit_count` / `relaxed_hit_count` /
`vector_hit_count` fields on `SearchRows`, see the
[querying guide](../guides/querying.md#unified-search-recommended).
The design rationale lives in
`dev/design-adaptive-text-search-surface.md` and its addendum
`dev/design-adaptive-text-search-surface-addendum-1-vec.md`.

::: fathomdb.Query
    options:
      members_order: source
      heading_level: 2

## Unified search

### SearchBuilder

Returned from
[`Query.search`](../guides/querying.md#unified-search-recommended). This
is the **primary retrieval entry point** and the one most applications
should use. Terminal `execute()` runs the engine's unified retrieval
pipeline (text strict then text relaxed then vector, with deterministic
block-precedence fusion) and returns [`SearchRows`](./types.md#searchrows).

The builder carries the full filter surface:

- `filter_logical_id_eq` / `filter_kind_eq`
- `filter_source_ref_eq`
- `filter_content_ref_eq` / `filter_content_ref_not_null`
- `filter_json_text_eq` / `filter_json_bool_eq`
- `filter_json_integer_gt` / `filter_json_integer_gte` /
  `filter_json_integer_lt` / `filter_json_integer_lte`
- `filter_json_timestamp_gt` / `filter_json_timestamp_gte` /
  `filter_json_timestamp_lt` / `filter_json_timestamp_lte`
- `filter_json_fused_text_eq` —
  fused JSON-text equality predicate pushed into the search CTE
- `filter_json_fused_timestamp_gt` / `filter_json_fused_timestamp_gte` /
  `filter_json_fused_timestamp_lt` / `filter_json_fused_timestamp_lte` —
  fused JSON-timestamp comparisons, also pushed into the search CTE

Fusable filters (kind, logical ID, source ref, content ref) push into
the search CTE; the `filter_json_*` family runs as a post-filter. The
`filter_json_fused_*` family (shipped in 0.4.0) pushes the predicate
into the search CTE so that `search()`'s `limit` applies *after*
narrowing, but it requires a registered property FTS schema covering
the referenced JSON path — calling a fused method without one raises
[`BuilderValidationError`](./types.md#errors) immediately and never
silently degrades to a post-filter. The auto-generated entries under
[SearchBuilder](#searchbuilder) below carry the full method
signatures and docstrings for each fused variant.
`with_match_attribution()` opts in to per-hit attribution, and
`execute()` returns the `SearchRows` described under
[SearchRows](./types.md#searchrows) — including the `vector_hit_count`,
`vector_distance`, `modality`, and optional `match_mode` fields that
generalize the payload across text and vector hits.

!!! warning "`filter_json_*` post-filter footgun"

    Because `filter_json_*` runs *after* the search CTE, the `limit`
    passed to `search()` / `text_search()` bounds the **candidate set**,
    not the final hit count. A call like
    `.search("x", 10).filter_json_text_eq("$.status", "active")` can
    return 0 hits even when thousands of matching rows exist — the 10
    candidates were chosen before the status filter ran. Either
    over-fetch (raise the search `limit` well above the desired final
    count and slice after filtering), or promote the narrowed field to
    a [property FTS projection](../guides/property-fts.md) so it
    participates in retrieval. See
    [`filter_json_*` vs property FTS](../guides/querying.md#filter_json_-vs-property-fts)
    in the querying guide for worked examples.

**Read-time embedding (Phase 12.5)**: the vector branch fires on
natural-language queries when the engine was opened with a read-time
query embedder — see [Read-time embedder](#read-time-embedder) below,
and [Read-time embedding](../guides/querying.md#read-time-embedding) in
the querying guide for worked Python and TypeScript examples. When
`EngineOptions.embedder` is left at its default
[`EmbedderChoice::None`](#read-time-embedder), the vector branch stays
dormant and every `SearchBuilder.execute()` result has
`vector_hit_count == 0`, matching the original Phase 12 v1 behaviour.
Callers who want to bypass the planner entirely can still use
[`Query.vector_search`](#queryvector_search) with a caller-provided
vector literal.

::: fathomdb.SearchBuilder
    options:
      members_order: source
      heading_level: 4

## Advanced: mechanism-specific overrides

The following builders are **advanced overrides** for callers with a
hard reason to pin the retrieval modality or to supply both query
shapes verbatim. Prefer [`SearchBuilder`](#searchbuilder) above for
general application queries.

### TextSearchBuilder

Returned from `Query.text_search`. Pins retrieval to the text modality:
the engine runs the strict branch first, and if it returns zero hits,
derives and runs an engine-owned relaxed branch. `execute()` returns
[`SearchRows`](./types.md#searchrows) with `vector_hit_count == 0`. The
builder carries the same filter surface as `SearchBuilder`, plus
`with_match_attribution()`.

::: fathomdb.TextSearchBuilder
    options:
      members_order: source
      heading_level: 4

### Query.vector_search

Pins retrieval to the vector modality. Requires the engine to have been
opened with `vector_dimension`. It is the caller-supplied-literal
override: when the engine is opened with
[`EmbedderChoice::None`](#read-time-embedder), this is the only way to
run a vector search because `search()` has no embedder to turn raw text
into a query vector. When a read-time embedder is attached (Phase 12.5
`Builtin` or `InProcess`), `search()` fires its own vector branch on
natural-language queries and most callers should prefer it; the
`vector_search` override remains available for callers that want to
bypass the unified planner and supply a vector literal directly. The
method extends the current `Query` chain with a `vector_search` step;
the resulting query still terminates in `execute() -> QueryRows`, and
the Python SDK does not currently ship a dedicated `VectorSearchBuilder`
type. Vector hits emitted by `search()` are surfaced through the shared
`SearchRows` / `SearchHit` family with `modality == Vector` and a
populated `vector_distance` field.

### FallbackSearchBuilder

Returned from [`Engine.fallback_search`](./engine.md). Neither branch is
adaptively rewritten — the engine runs `strict_query` first, and if it
returns zero hits, runs `relaxed_query` verbatim. Passing `None` for
`relaxed_query` degenerates to a strict-only search (the shape used by
the dedup-on-write pattern). `fallback_search` is a **narrow helper**,
not a general query-composition API; `Query.search` remains the right
surface for almost all application queries.

::: fathomdb.FallbackSearchBuilder
    options:
      members_order: source
      heading_level: 4

## Read-time embedder

Phase 12.5 ships a read-time query embedder that lets
[`SearchBuilder`](#searchbuilder) fire its vector branch on raw
natural-language queries. The embedder is selected once at
`Engine.open(...)` time and never reconfigured for the life of the
engine. The Rust surface lives on the `fathomdb` and `fathomdb-engine`
crates; the Python and TypeScript SDKs expose a narrow string alias
over the same choices.

### `EmbedderChoice` (Rust)

Caller-facing enum stored on `EngineOptions::embedder`. Three variants:
`None` (default — no embedder is attached and the vector branch stays
dormant), `Builtin` (the Candle + `BAAI/bge-small-en-v1.5` default
implementation, resolved only when `fathomdb-engine` is built with the
`default-embedder` feature — when the feature is off the engine logs a
warning and falls back to `None`), and `InProcess(Arc<dyn
QueryEmbedder>)` (a caller-supplied in-process embedder, the most
flexible shape). Construct via `EngineOptions::new(path)
.with_embedder(EmbedderChoice::Builtin)` or the struct literal
`EngineOptions { embedder: EmbedderChoice::Builtin, .. }`. Subprocess /
external-service variants are intentionally deferred — write-time
regeneration continues to flow through `VectorRegenerationConfig`.

### `QueryEmbedder` (Rust trait)

`Send + Sync + Debug` trait implemented by every read-time embedder.
Defines `embed_query(&self, text: &str) -> Result<Vec<f32>,
EmbedderError>` and `identity(&self) -> QueryEmbedderIdentity`. Methods
take `&self` — implementations must be internally immutable or manage
their own interior mutability. The execution coordinator holds a
single `Arc<dyn QueryEmbedder>` shared across reader threads.

### `QueryEmbedderIdentity` (Rust struct)

Identity metadata describing the active embedder: `model_identity`
(e.g. `"bge-small-en-v1.5"`), `model_version`, `dimension` (must match
the active vector profile or the vector branch never fires), and
`normalization_policy` (e.g. `"l2"`, `"none"`). Reported by
`QueryEmbedder::identity()` and used by Phase 12.5b to gate the vector
branch on profile compatibility.

### `EmbedderError` (Rust enum)

Errors reported by a `QueryEmbedder`. Two variants:
`Unavailable(String)` (the embedder cannot produce a vector right now
— the usual cause is the `default-embedder` feature being disabled or
model weights failing to load) and `Failed(String)` (the embedding
pipeline itself errored on this particular query). The coordinator
treats both as graceful degradation: the vector branch is skipped,
`SearchRows.was_degraded` is set, and the rest of the search pipeline
runs normally — neither variant is a hard query failure.

### `BuiltinBgeSmallEmbedder` (Rust, feature-gated)

Concrete `QueryEmbedder` implementation shipped behind the
`default-embedder` Cargo feature on `fathomdb-engine`. Wraps Candle +
`BAAI/bge-small-en-v1.5`, pools the `[CLS]` token, applies L2
normalization, and returns a 384-dim vector. Model weights are
lazy-loaded on first use (~300–500 ms cold start); warm per-query
embedding is ~20 ms on CPU fp32. Selected implicitly via
`EmbedderChoice::Builtin`; direct construction is not a stable public
API.

### SDK string surface

The Python and TypeScript SDKs expose the same choice as a string
keyword on `Engine.open(...)` instead of the Rust enum, so embedder
selection is reachable without crossing the FFI type boundary.

- **Python.** `Engine.open(database_path, *, embedder="builtin", ...)`
  on `fathomdb.Engine`. Accepted values are `None` / `"none"` (no
  embedder) and `"builtin"` (the feature-gated Candle default). Any
  other value raises at open time.
- **TypeScript.** `Engine.open(path, { embedder: "builtin", ... })`.
  The `EngineOpenOptions.embedder` field is typed as `"none" |
  "builtin" | undefined` in `typescript/packages/fathomdb/src/types.ts`;
  `undefined` is the same as `"none"`.

The `InProcess` variant is Rust-only — supplying an
`Arc<dyn QueryEmbedder>` requires the Rust API directly and is not
reachable from the SDK strings.
