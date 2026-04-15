# AdminClient

Administrative operations for a fathomdb database -- integrity checks,
projection rebuilds, source tracing, safe exports, FTS property schema
management, and operational collection management. Access via `engine.admin`.
See the [Admin Operations](../operations/admin-operations.md) guide for
detailed walkthroughs and
[Property FTS Projections](../guides/property-fts.md) for the schema
registration workflow.

::: fathomdb.AdminClient
    options:
      members_order: source
      heading_level: 2

## Async FTS property schema registration

### `register_fts_property_schema_async`

Registers a property-FTS schema for a node kind and returns immediately.
The FTS rebuild runs in the background via `RebuildActor`.

=== "Python"

    ```python
    record = db.admin.register_fts_property_schema_async(
        kind,
        property_paths,
        separator=None,
    )
    # Returns FtsPropertySchemaRecord
    ```

=== "TypeScript"

    ```typescript
    const record = engine.admin.registerFtsPropertySchemaAsync(
        kind,
        propertyPaths,
        separator?,
    );
    // Returns FtsPropertySchemaRecord
    ```

**Behavior change vs eager registration:** after `register_fts_property_schema_async`,
the new schema is **not immediately visible to search**. Search reads from the
live FTS table (the previous schema) until the rebuild reaches `COMPLETE`.
Callers that need synchronous visibility should use `register_fts_property_schema`
or `register_fts_property_schema_with_entries` (eager mode).

### `get_rebuild_progress`

Polls the DB-backed rebuild state machine for a given kind.

=== "Python"

    ```python
    progress = db.admin.get_rebuild_progress(kind)
    # Returns RebuildProgress | None
    ```

=== "TypeScript"

    ```typescript
    const progress = engine.admin.getRebuildProgress(kind);
    // Returns RebuildProgress | undefined
    ```

Returns `None` / `undefined` if no rebuild has been registered for the kind.
Otherwise returns a `RebuildProgress` with these fields:

| Field | Type | Description |
|---|---|---|
| `state` | `str` | One of `PENDING`, `BUILDING`, `SWAPPING`, `COMPLETE`, or `FAILED`. |
| `rows_total` | `int \| None` | Total rows to rebuild (available after `BUILDING` starts). |
| `rows_done` | `int \| None` | Rows rebuilt so far. |
| `started_at` | `datetime \| None` | Timestamp when the rebuild started. |
| `last_progress_at` | `datetime \| None` | Timestamp of the most recent progress update. |
| `error_message` | `str \| None` | Populated only when `state == "FAILED"`. |

The state machine progresses: `PENDING → BUILDING → SWAPPING → COMPLETE`.
On failure: `FAILED`. If the engine restarts during a rebuild, the in-progress
state is marked `FAILED` on the next engine open; call
`register_fts_property_schema_async` again to retry.

### `RebuildMode`

Controls whether the FTS rebuild runs synchronously or in the background.

| Mode | API | Behavior |
|---|---|---|
| `Eager` | `register_fts_property_schema`, `register_fts_property_schema_with_entries` | Rebuild runs synchronously inside the schema-upsert transaction. Schema is live on return. |
| `Async` | `register_fts_property_schema_async` | Schema is registered immediately; rebuild runs in the background via `RebuildActor`. New schema is not visible to search until `COMPLETE`. |

The Python and TypeScript bindings use `Async` mode by default when calling
`register_fts_property_schema_async`. The `Eager` mode is available via the
existing `register_fts_property_schema` / `registerFtsPropertySchema` surface.
