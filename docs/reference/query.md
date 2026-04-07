# Query

Fluent, immutable query builder for fetching nodes from a fathomdb database.
Instances are created via `Engine.nodes()`. Each filter or traversal method
returns a new `Query`, leaving the original unchanged. See the
[Querying](../guides/querying.md) guide for usage patterns and examples.

::: fathomdb.Query
    options:
      members_order: source
      heading_level: 2
