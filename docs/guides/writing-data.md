# Writing Data

This guide covers how to write nodes, edges, chunks, and provenance records to a
fathomdb database using the Python SDK. For background on the underlying data
structures, see [Data Model](../concepts/data-model.md) and
[Temporal Model](../concepts/temporal-model.md). For full API details, see the
[WriteRequestBuilder Reference](../reference/write-builder.md).

## The builder pattern

Every write goes through three steps: create a `WriteRequestBuilder` with a
descriptive label, add items, then call `build()` and submit with
`engine.write()`. All items in a single request are committed atomically.

```python
from fathomdb import Engine, WriteRequestBuilder, new_id, new_row_id

engine = Engine.open("my_agent.db")
builder = WriteRequestBuilder("ingest-meeting-notes")
node = builder.add_node(
    row_id=new_row_id(), logical_id=new_id(),
    kind="meeting", properties={"title": "Sprint Review", "date": "2025-06-01"},
)
receipt = engine.write(builder.build())
```

## ID generation

Two helpers generate time-sortable unique identifiers:

- **`new_id()`** -- generates a logical ID (stable identity across versions).
- **`new_row_id()`** -- generates a row ID (one physical version of an entity).

You may also use your own IDs (UUIDs, document hashes, etc.).

## Writing nodes

`add_node()` accepts the following parameters:

| Parameter | Description |
|---|---|
| `row_id` | Unique identifier for this physical version of the node. |
| `logical_id` | Stable identity for the entity. Persists across updates. |
| `kind` | Type label (e.g. `"meeting"`, `"person"`, `"task"`). |
| `properties` | JSON-serializable dict of entity data. |
| `source_ref` | Optional provenance reference (see [Provenance](#provenance)). |
| `upsert` | If `True`, supersede the active row with the same `logical_id`. |
| `chunk_policy` | How to handle existing chunks on upsert (see [Upsert](#upsert-supersession)). |

`add_node()` returns a `NodeHandle` you can use to reference this node
elsewhere in the same request.

## Writing edges

`add_edge()` connects two nodes with a typed, directed relationship:

```python
edge = builder.add_edge(
    row_id=new_row_id(),
    logical_id=new_id(),
    source=person_handle,   # NodeHandle or logical_id string
    target=meeting_handle,  # NodeHandle or logical_id string
    kind="ATTENDED",
    properties={"role": "presenter"},
)
```

`source` and `target` accept a `NodeHandle` from the same builder or a string
logical ID for an existing node.

## The handle system

Each `add_*` method returns a typed handle (`NodeHandle`, `EdgeHandle`,
`ChunkHandle`, `RunHandle`, `StepHandle`, `ActionHandle`). Handles let you
cross-reference items within the same request without tracking raw IDs.

```python
builder = WriteRequestBuilder("add-person-and-meeting")
person = builder.add_node(
    row_id=new_row_id(), logical_id=new_id(),
    kind="person", properties={"name": "Alice"},
)
meeting = builder.add_node(
    row_id=new_row_id(), logical_id=new_id(),
    kind="meeting", properties={"title": "Standup"},
)
builder.add_edge(
    row_id=new_row_id(), logical_id=new_id(),
    source=person, target=meeting, kind="ATTENDED", properties={},
)
engine.write(builder.build())
```

Handles are scoped to their builder. Passing a handle from one builder to a
different builder raises `BuilderValidationError`.

## Upsert (supersession)

Setting `upsert=True` replaces the currently active row for a given
`logical_id`. The old row is marked with a `superseded_at` timestamp -- history
is preserved, not deleted.

```python
meeting_id = new_id()

# Initial insert
b1 = WriteRequestBuilder("create-meeting")
b1.add_node(
    row_id=new_row_id(), logical_id=meeting_id,
    kind="meeting", properties={"title": "Standup", "status": "scheduled"},
)
engine.write(b1.build())

# Update via upsert
b2 = WriteRequestBuilder("complete-meeting")
b2.add_node(
    row_id=new_row_id(), logical_id=meeting_id,
    kind="meeting", properties={"title": "Standup", "status": "completed"},
    upsert=True,
)
engine.write(b2.build())
```

### Chunk policy on upsert

When upserting a node that has associated chunks, the `chunk_policy` parameter
controls what happens to existing chunks:

- **`ChunkPolicy.PRESERVE`** (default) -- keep existing chunks untouched. Use
  this when only properties changed, not the text content.
- **`ChunkPolicy.REPLACE`** -- delete all existing chunks (and their FTS rows)
  for this `logical_id`, then insert the new chunks from the request. Use this
  when the text content changed.

```python
from fathomdb import ChunkPolicy

builder = WriteRequestBuilder("update-note-text")
node = builder.add_node(
    row_id=new_row_id(), logical_id=note_id,
    kind="note", properties={"title": "Revised"},
    upsert=True, chunk_policy=ChunkPolicy.REPLACE,
)
builder.add_chunk(
    id=new_id(), node=node,
    text_content="Updated content for full-text search.",
)
engine.write(builder.build())
```

## Retiring (soft-delete)

`retire_node()` and `retire_edge()` mark an entity as superseded without
inserting a replacement. Both accept a handle or string logical ID. Retiring a
node also deletes its chunks and FTS rows.

```python
builder = WriteRequestBuilder("clean-up-old-task")
builder.retire_node(logical_id="task-001", source_ref="action/cleanup-42")
builder.retire_edge(logical_id="edge-task-001-project")
engine.write(builder.build())
```

## Chunks

Chunks break a node's text content into pieces for full-text and vector search:

```python
builder = WriteRequestBuilder("ingest-document")
node = builder.add_node(
    row_id=new_row_id(), logical_id=new_id(),
    kind="document", properties={"title": "Q3 Report"},
)
chunk = builder.add_chunk(
    id=new_id(), node=node,
    text_content="Revenue grew 15% quarter over quarter...",
    byte_start=0, byte_end=42,
)
engine.write(builder.build())
```

`byte_start` and `byte_end` are optional byte offsets into the source document.

## Vector embeddings

Attach pre-computed embeddings to chunks with `add_vec_insert()`. The `chunk`
parameter accepts a `ChunkHandle` or a string chunk ID.

```python
chunk = builder.add_chunk(id=new_id(), node=node, text_content="Revenue grew 15%...")
builder.add_vec_insert(chunk=chunk, embedding=[0.12, -0.34, ...])
```

The engine must be opened with `vector_dimension` matching your embedding size.

## Runs, steps, and actions

fathomdb tracks agent execution with a three-level hierarchy: **Run** (session
or job), **Step** (LLM call, planning phase), and **Action** (tool call,
observation).

```python
builder = WriteRequestBuilder("agent-execution")

run = builder.add_run(
    id=new_id(), kind="chat-session", status="active",
    properties={"model": "claude-sonnet"},
)
step = builder.add_step(
    id=new_id(), run=run, kind="llm-turn", status="active",
    properties={"prompt_tokens": 1200},
)
builder.add_action(
    id=new_id(), step=step, kind="tool-call", status="completed",
    properties={"tool": "web_search", "query": "fathomdb docs"},
)

engine.write(builder.build())
```

Runs, steps, and actions support `upsert=True` for status transitions. They do
not have a separate retire operation -- use upsert with a terminal status.

## Operational writes

The operational store supports append-only logs and key-value state outside the
graph model:

- **`add_operational_append()`** -- append to an immutable log.
- **`add_operational_put()`** -- upsert a key-value record.
- **`add_operational_delete()`** -- delete a record by key.

```python
builder.add_operational_put(
    collection="agent_config",
    record_key="model_preference",
    payload_json={"model": "claude-sonnet", "temperature": 0.7},
)
```

For details, see [Operational Store](../concepts/operational-store.md).

## Provenance

The `source_ref` parameter links a write to the run, step, or action that
produced it, enabling tracing any piece of data back to its origin.

```python
builder.add_node(
    row_id=new_row_id(), logical_id=new_id(),
    kind="claim", properties={"text": "Revenue is up"},
    source_ref="action/extract-facts-7",
)
```

Provenance enforcement is configured when opening the engine. `ProvenanceMode.WARN`
(default) allows writes without `source_ref` but populates
`WriteReceipt.provenance_warnings`. `ProvenanceMode.REQUIRE` rejects them.

```python
from fathomdb import Engine, ProvenanceMode

engine = Engine.open("my_agent.db", provenance_mode=ProvenanceMode.REQUIRE)
```

## WriteReceipt

`engine.write()` returns a `WriteReceipt`:

| Field | Description |
|---|---|
| `label` | The label from the original `WriteRequestBuilder`. |
| `warnings` | General warnings about the write (e.g. no-op retires). |
| `provenance_warnings` | Items that were written without a `source_ref`. |
| `optional_backfill_count` | Number of optional projection tasks queued. |

```python
receipt = engine.write(builder.build())
if receipt.provenance_warnings:
    print(f"Missing provenance on {len(receipt.provenance_warnings)} items")
```
