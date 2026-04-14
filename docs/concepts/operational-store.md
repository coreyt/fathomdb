# Operational Store

The operational store is a purpose-built storage layer for ephemeral,
high-churn state that does not belong in the versioned graph. It lives in
the same SQLite database as the rest of fathomdb, so it benefits from the
same durability, export, integrity, and recovery guarantees.

Typical use cases:

- **Connector health** -- last-seen timestamps, error counts, status flags
- **Sync cursors** -- checkpoint tokens for pollers and incremental imports
- **Rate-limit counters** -- per-key usage tracking
- **Audit logs** -- immutable event streams for compliance or debugging

## What the operational store is NOT

The operational store is not a backdoor for domain data. If your data has
entity identity, relationships, or benefits from search and graph
traversal, it belongs in the graph as nodes and edges. Use the
operational store only for bookkeeping state where full node versioning
would be pure overhead.

## Collection kinds

Every collection is registered with a `kind` that controls mutation semantics:

### APPEND_ONLY_LOG

An immutable event stream. Each write appends a new record that is never
updated or deleted. Use for audit trails, event logs, and status history.

### LATEST_STATE

A key-value store where each key holds the most recent value. Writes use
`put` and `delete`. Every mutation is still appended to a canonical log,
and a derived current-state view reflects the latest value per key. Use
for cursors, health checks, and counters.

## Registering a collection

Before writing, register the collection through the admin client:

```python
from fathomdb import OperationalCollectionKind, OperationalRegisterRequest

request = OperationalRegisterRequest(
    name="connector_health",
    kind=OperationalCollectionKind.LATEST_STATE,
    schema_json='{"fields": ["connector_id", "status", "last_seen"]}',
    retention_json='{"policy": "keep_all"}',
    format_version=1,
)
record = engine.admin.register_operational_collection(request)
```

The `schema_json` field is documentation-only metadata; the engine does
not validate payload shapes against it. Configure filter fields, validation,
and secondary indexes after registration -- see [Admin Operations](../operations/admin-operations.md).

## Writing data

Operational writes go through `WriteRequestBuilder`, the same builder
used for graph mutations. This means operational and graph writes commit
atomically in a single transaction.

```python
from fathomdb import WriteRequestBuilder

# Append to a log collection
wb = WriteRequestBuilder("log-api-call")
wb.add_operational_append(
    collection="audit_log",
    record_key="evt-001",
    payload_json={"action": "api_call", "endpoint": "/sync", "status": 200},
    source_ref="run-abc",
)
engine.write(wb.build())

# Put state into a latest-state collection
wb = WriteRequestBuilder("update-health")
wb.add_operational_put(
    collection="connector_health",
    record_key="gmail",
    payload_json={"status": "healthy", "last_seen": 1712400000},
)
engine.write(wb.build())

# Delete a key
wb = WriteRequestBuilder("remove-cursor")
wb.add_operational_delete(collection="connector_health", record_key="gmail")
engine.write(wb.build())
```

Graph and operational writes can be mixed. Both commit or roll back together:

```python
wb = WriteRequestBuilder("sync-with-cursor")
wb.add_node(
    row_id="row-1", logical_id="email-42", kind="email",
    properties={"subject": "Hello"}, source_ref="run-abc",
)
wb.add_operational_put(
    collection="sync_cursors",
    record_key="gmail-inbox",
    payload_json={"cursor": "token-xyz", "synced_at": 1712400000},
)
engine.write(wb.build())
```

For more write patterns, see [Writing Data](../guides/writing-data.md).

## Reading data

Use `read_operational_collection` to query mutations with filters.
Filter fields must be declared on the collection first -- see
[Admin Operations](../operations/admin-operations.md). For a worked
end-to-end walkthrough of the three filter modes (EXACT, PREFIX,
RANGE) and how to compose clauses, see the
[Operational Queries guide](../guides/operational-queries.md).

```python
from fathomdb import (
    OperationalFilterClause, OperationalFilterMode,
    OperationalFilterValue, OperationalReadRequest,
)

request = OperationalReadRequest(
    collection_name="audit_log",
    filters=[
        OperationalFilterClause(
            mode=OperationalFilterMode.EXACT,
            field="action",
            value=OperationalFilterValue.string("api_call"),
        ),
    ],
    limit=50,
)
report = engine.admin.read_operational_collection(request)
for row in report.rows:
    print(row.record_key, row.payload_json)
```

Filter modes: `EXACT`, `PREFIX`, `RANGE`. Field types: `STRING`,
`INTEGER`, `TIMESTAMP`. Secondary indexes for multi-field lookups can be
declared via `update_operational_collection_secondary_indexes`.

## Lifecycle

1. **Register** -- create the collection with `register_operational_collection`.
2. **Configure** -- optionally add filter fields, validation, or secondary indexes.
3. **Write and read** -- use the builder methods and read API above.
4. **Disable** -- call `disable_operational_collection` to reject new
   writes while preserving existing data.

## Maintenance

Three maintenance primitives are available through the admin client. None
run automatically; your application or orchestration layer schedules them.

**Retention** evaluates per-collection policies (max age, max rows, or
keep-all) and deletes expired mutations. Preview with
`plan_operational_retention`, execute with `run_operational_retention`.

**Compaction** removes superseded mutations from `latest_state`
collections while preserving the current value per key. Preview with
`compact_operational_collection(dry_run=True)`.

**Purge** deletes all mutations older than a timestamp cutoff via
`purge_operational_collection(before_timestamp=...)`.

## When to use the graph vs. the operational store

| Question | Graph (nodes/edges) | Operational store |
|----------|-------------------|-------------------|
| Does the data have entity identity? | Yes | No |
| Does it participate in relationships? | Yes | No |
| Do you need history, search, or provenance? | Yes | Minimal |
| Is it high-churn bookkeeping? | No | Yes |
| Would versioning be pure overhead? | No | Yes |
| Examples | meetings, tasks, contacts | cursors, health, counters, audit logs |

When in doubt, start with the graph. Move to the operational store when
the graph's versioning model creates unnecessary write amplification.
