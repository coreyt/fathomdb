# Temporal Model and Supersession

fathomdb tracks change over time using an append-oriented temporal model. Rows
are never silently overwritten -- old versions are marked as historical while
new versions are inserted alongside them.

## The temporal pair: `created_at` and `superseded_at`

Every node and edge row carries two timestamp columns:

| Column | Meaning |
|---|---|
| `created_at` | Unix epoch seconds when the row was written |
| `superseded_at` | Unix epoch seconds when the row was replaced or retired; `NULL` while the row is still active |

These two columns are the only time axis the engine manages.

## Active vs. historical rows

A row is **active** when `superseded_at IS NULL` and **historical** when
`superseded_at` has a value. The engine enforces a uniqueness constraint: at
most one active row may exist per `logical_id` at any time.

All default queries return only active rows. The query compiler injects
`superseded_at IS NULL` automatically. Historical rows remain in the database
for audit, provenance tracing, and correction, but they are invisible to
normal reads.

## Supersession (upsert)

Supersession updates a node or edge without destroying its history. When you
call `add_node` with `upsert=True`, the engine performs two operations
atomically inside a single transaction:

1. Sets `superseded_at` on the currently active row for that `logical_id`.
2. Inserts a new row with the same `logical_id` but a fresh `row_id`.

The old row becomes historical; the new row becomes active. Edges that
reference the `logical_id` automatically resolve to the new active version.

```python
from fathomdb import WriteRequestBuilder

req = WriteRequestBuilder(label="update-contact")
req.add_node(
    row_id="contact-alice-v2",
    logical_id="contact-alice",
    kind="Contact",
    properties={"name": "Alice", "email": "alice@newdomain.com"},
    source_ref="run-012",
    upsert=True,
)
engine.write(req.build())
```

If no active row exists for that `logical_id`, the node is inserted normally.
If one does exist, it is superseded first. Either way, exactly one active row
exists afterward.

## Chunk lifecycle on supersession

Nodes can have text chunks used for full-text and vector search. When a node
is superseded, you control what happens to its chunks via `chunk_policy`:

- **`ChunkPolicy.PRESERVE`** (default) -- Keep existing chunks. Use this when
  only node properties changed, not text content.
- **`ChunkPolicy.REPLACE`** -- Delete all existing chunks and FTS entries for
  the node, then insert the new chunks from this request. Use this when text
  content has changed.

```python
from fathomdb import WriteRequestBuilder, ChunkPolicy

req = WriteRequestBuilder(label="update-document")

node = req.add_node(
    row_id="doc-report-v3",
    logical_id="doc-report",
    kind="Document",
    properties={"title": "Q4 Report", "version": 3},
    upsert=True,
    chunk_policy=ChunkPolicy.REPLACE,
)

req.add_chunk(
    id="doc-report-v3-chunk-1",
    node=node,
    text_content="Updated Q4 financial results...",
)

engine.write(req.build())
```

If you use `ChunkPolicy.PRESERVE` but the text has actually changed, the FTS
index will contain stale entries. Choose the policy deliberately.

## Retire (soft delete)

Retiring a row sets `superseded_at` without inserting a replacement. After
retirement, no active row exists for that `logical_id`.

```python
req = WriteRequestBuilder(label="cleanup")

req.retire_node(logical_id="contact-alice", source_ref="run-015")
req.retire_edge(logical_id="edge-alice-proj", source_ref="run-015")

engine.write(req.build())
```

When a node is retired, its chunks and FTS index entries are deleted
automatically. Edges are **not** retired automatically -- if the node's edges
should also end, retire them explicitly in the same request.

## Unitemporal, not bitemporal

fathomdb uses a **unitemporal** model. There is one time axis: when the row
was written to the database (`created_at` / `superseded_at`).

A bitemporal model would add a second axis: when the fact was true in the real
world ("valid time"). fathomdb does not track valid time at the engine level.
If your application needs to record when something was true in the real world,
store that as a property on the node. The engine timestamps tell you when the
*record* was created and replaced, not when the underlying fact was valid.

## Correction and rollback

When bad data enters the store, two admin operations help you recover:

- **`excise_source`** -- Supersedes all rows that share a given `source_ref`.
  Use this to undo an entire bad write or agent run in one operation.
- **`restore_logical_id`** -- Un-retires a previously retired row, restoring it
  to active state.

See [Admin Operations](../operations/admin-operations.md) for details.

## What the engine does not provide

The following are application-layer concerns, not engine features:
- **Time-travel queries** ("show me the state as of last Tuesday") -- The
  engine does not expose an "as of time T" query mode. Query historical rows
  directly with timestamp filters on `created_at` and `superseded_at`.
- **Time-bucketing aggregations** ("count changes per hour") -- Aggregate
  queries over temporal columns are standard SQL and belong in application
  code.
- **Bitemporal valid-time tracking** -- Store real-world timestamps in node
  properties as described above.

For more on writing data, see the [Writing Data](../guides/writing-data.md)
guide.
