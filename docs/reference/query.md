# Query

Fluent, immutable query builder for fetching nodes from a fathomdb database.
Instances are created via `Engine.nodes()`. Each filter or traversal method
returns a new `Query`, leaving the original unchanged. See the
[Querying](../guides/querying.md) guide for usage patterns and examples.

The `text_search()` step accepts a constrained safe subset of familiar search
syntax: bare terms, quoted phrases, implicit `AND`, uppercase `OR`, and
uppercase `NOT`. Unsupported syntax stays literal instead of being passed
through as raw FTS5 control syntax.

For the full contract, including literal downgrade behavior and unsupported
forms, see [Text Query Syntax](../guides/text-query-syntax.md).

::: fathomdb.Query
    options:
      members_order: source
      heading_level: 2
