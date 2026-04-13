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

Fusable filters (kind, logical ID, source ref, content ref) push into
the search CTE; the `filter_json_*` family runs as a post-filter.
`with_match_attribution()` opts in to per-hit attribution, and
`execute()` returns the `SearchRows` described under
[SearchRows](./types.md#searchrows) — including the `vector_hit_count`,
`vector_distance`, `modality`, and optional `match_mode` fields that
generalize the payload across text and vector hits.

**v1 scope**: `search()` does not currently run the vector branch on
natural-language queries — read-time query embedding is deferred to a
future phase. Every `SearchBuilder.execute()` result in v1 therefore has
`vector_hit_count == 0`. Callers who need vector retrieval today must
use [`Query.vector_search`](#queryvector_search) directly with a
caller-provided vector literal.

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
opened with `vector_dimension`. In v1 this is also the **only** way to
run a vector search, because `search()` does not yet embed
natural-language queries at read time. The method extends the current
`Query` chain with a `vector_search` step; the resulting query still
terminates in `execute() -> QueryRows`, and the Python SDK does not
currently ship a dedicated `VectorSearchBuilder` type. When a future
phase wires read-time query embedding into `search()`, vector hits will
be surfaced through the shared `SearchRows` / `SearchHit` family with
`modality == Vector` and a populated `vector_distance` field.

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
