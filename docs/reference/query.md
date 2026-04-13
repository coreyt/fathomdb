# Query

Fluent, immutable query builder for fetching nodes from a fathomdb database.
Instances are created via `Engine.nodes()`. Each filter or traversal method
returns a new `Query`, leaving the original unchanged. See the
[Querying](../guides/querying.md) guide for usage patterns and examples.

The `text_search()` step is the **adaptive text search** surface. It accepts
a constrained safe subset of familiar search syntax: bare terms, quoted
phrases, implicit `AND`, uppercase `OR`, and uppercase `NOT`. Unsupported
syntax stays literal instead of being passed through as raw FTS5 control
syntax. Unlike the rest of `Query`, `text_search()` steps out of the regular
builder: it returns a [`TextSearchBuilder`](#adaptive-text-search) whose
terminal `execute()` is statically typed to return
[`SearchRows`](./types.md#searchrows) rather than `QueryRows`.

For the strict query grammar, see
[Text Query Syntax](../guides/text-query-syntax.md). For the adaptive
strict-then-relaxed policy, per-branch counts, and the
`fallback_used` / `strict_hit_count` / `relaxed_hit_count` fields on
`SearchRows`, see the
[adaptive text search guide](../guides/querying.md#adaptive-text-search).
The design rationale lives in
`dev/design-adaptive-text-search-surface.md`.

::: fathomdb.Query
    options:
      members_order: source
      heading_level: 2

## Adaptive text search

### TextSearchBuilder

Returned from [`Query.text_search`](../guides/querying.md#adaptive-text-search).
Terminal `execute()` runs the engine's adaptive retrieval policy (strict
branch first; relaxed branch only if the strict branch returns zero hits)
and returns [`SearchRows`](./types.md#searchrows). The builder carries the
same filter surface as `Query` — `filter_kind_eq`, `filter_logical_id_eq`,
`filter_source_ref_eq`, `filter_content_ref_eq`,
`filter_content_ref_not_null`, and the `filter_json_*` family — plus
`with_match_attribution()` to opt in to per-hit attribution.

::: fathomdb.TextSearchBuilder
    options:
      members_order: source
      heading_level: 4

### FallbackSearchBuilder

Returned from [`Engine.fallback_search`](./engine.md). Unlike
`TextSearchBuilder`, neither branch is adaptively rewritten — the engine
runs `strict_query` first, and if it returns zero hits, runs
`relaxed_query` verbatim. Passing `None` for `relaxed_query` degenerates
to a strict-only search (the shape used by the dedup-on-write pattern).
`fallback_search` is a **narrow helper**, not a general query-composition
API; `Query.text_search` remains the right surface for almost all
application queries.

::: fathomdb.FallbackSearchBuilder
    options:
      members_order: source
      heading_level: 4
