# Externalizing Payloads with `content_ref`

Some nodes carry metadata that is small, structured, and query-relevant
alongside a payload that is large, opaque, and read-mostly — a meeting
node with a 20KB transcript, a document node with a rendered PDF, a
connector event with an attached audit blob. FathomDB's `content_ref`
field is the pattern for keeping those two concerns apart: **indexable
metadata stays on the node, bulky payload lives behind the ref**.

This guide covers when to externalize payload, what the read side looks
like, and when **not** to reach for `content_ref`. The write-side
mechanism — setting `content_ref` on `NodeInsert`, `content_hash`, and
update semantics via `ChunkPolicy.REPLACE` — is already documented in
[Writing Data — External content nodes](./writing-data.md#external-content-nodes).
This guide cross-links to that section rather than duplicating it.

## When to externalize payload

Reach for `content_ref` when one or more of the following holds:

- **The payload is large** relative to the query-relevant metadata.
  A rule of thumb: if a typical node row exceeds 10KB and most of the
  bytes are in a single field, that field is a candidate for
  externalization. A 20KB meeting transcript attached to a node whose
  properties are `{title, attendees, started_at}` is the canonical
  shape.
- **The payload is read infrequently** relative to the metadata.
  Lists, dashboards, filters, and search results all hit the
  metadata; only "show me the full transcript" needs the payload.
  Externalizing means the frequent reads skip the bulky field
  entirely.
- **The payload is audit-only.** Raw webhook bodies, upstream JSON
  envelopes, or pre-normalized source documents that you need for
  provenance but never query against fit here perfectly.
- **The payload lives in a content-addressed store already.** If
  you are already writing the payload to S3, a CAS, or a blob store,
  `content_ref` is just the URI FathomDB records alongside the node.

Don't externalize when:

- **The payload is small.** A few hundred bytes of JSON properties
  belong on the node. The indirection of an external ref is pure
  overhead.
- **You filter on the payload.** `filter_json_*` operates on a node's
  `properties`, not on whatever lives behind `content_ref`. A field
  you want to narrow on with `filter_json_text_eq` must be on the
  node.
- **You search on the payload via property FTS.** Property FTS
  projects over `properties`. If the payload should participate in
  text search, either keep it on the node or — the preferred shape
  for large searchable text — ingest it as one or more **chunks**
  attached to the node. Chunks participate in text search and vector
  retrieval; `content_ref` payloads do not.

The decision tree: **filter on it or search on it → keep it on the
node (or chunks). Store it or display it → `content_ref` is fine.**

## Refresher on the write side

Setting `content_ref` is one argument on `builder.add_node`. The full
mechanics — including `content_hash` for staleness detection and how
to refresh external content with `chunk_policy=ChunkPolicy.REPLACE` —
live in
[Writing Data — External content nodes](./writing-data.md#external-content-nodes).
The one-line summary: pass `content_ref="scheme://location"` on the
`add_node` call, and the engine stores the URI alongside the row
without fetching it.

## The read side

`content_ref` surfaces on `NodeRow` as an `Optional[str]` field. When
a node is returned from any query path — direct fetch, graph
traversal, or `search()` — the caller can inspect the field to decide
whether to dereference the external payload:

```python
from fathomdb import Engine

engine = Engine.open("/tmp/my-app.db")

rows = (
    engine.nodes("Meeting")
    .filter_json_timestamp_gte("$.started_at", one_week_ago)
    .execute()
)

for node in rows:
    print(node.logical_id, node.properties.get("title"))
    if node.content_ref is not None:
        print("  (transcript available at", node.content_ref, ")")
```

The node carries the ref, not the payload. Your application owns the
fetch policy — when to dereference, how to cache, whether to stream —
and that is deliberate: FathomDB intentionally does not block on
external content during query execution, so a dashboard pulling 50
meeting rows incurs no transcript-fetch cost.

When the caller decides it needs the payload, the fetch is whatever
your storage layer requires:

```python
def load_transcript(node):
    ref = node.content_ref
    if ref is None:
        return None
    # Dereference the URI using whatever client your application uses.
    # FathomDB does not prescribe a transport.
    if ref.startswith("s3://"):
        return s3_client.get_object(ref).read()
    if ref.startswith("file://"):
        return open(ref[len("file://"):], "rb").read()
    raise ValueError(f"Unsupported content_ref scheme: {ref}")
```

`NodeRow.content_ref` is the stable surface; any scheme your caller
understands is allowed. FathomDB treats the value as opaque.

## Worked example: a meeting with a 20KB transcript

A concrete shape using a `Meeting` node kind. The indexable metadata
stays on the node — `title`, `attendees`, `started_at`, `duration_ms`,
`status`. The transcript, which runs to 20KB of plain text per
meeting, lives behind `content_ref`.

### Write side

```python
from fathomdb import ChunkPolicy, WriteRequestBuilder, new_id, new_row_id

def ingest_meeting(engine, meeting_id, title, attendees, started_at,
                   duration_ms, status, transcript_text):
    # Step 1: write the transcript to your external store. Any
    # content-addressed scheme works; this example uses a fake CAS
    # that returns a sha256-addressed URI.
    ref = content_store.put(transcript_text.encode("utf-8"))
    # ref == "cas://sha256/9f86d08..."

    # Step 2: write the node with the ref. Note that properties stay
    # small and structured — everything you filter or display from a
    # list lives here.
    builder = WriteRequestBuilder(f"ingest-meeting-{meeting_id}")
    builder.add_node(
        row_id=new_row_id(),
        logical_id=meeting_id,
        kind="Meeting",
        properties={
            "title": title,
            "attendees": attendees,
            "started_at": started_at,
            "duration_ms": duration_ms,
            "status": status,
        },
        content_ref=ref,
    )
    engine.write(builder.build())
```

If you also want the transcript to be text-searchable, chunk it and
attach the chunks to the node — `content_ref` and chunks are
complementary: the ref is your canonical storage handle, the chunks
are the search-indexed representation.

See
[Writing Data — External content nodes](./writing-data.md#external-content-nodes)
for the full write-side API, including `content_hash` for staleness
detection and `ChunkPolicy.REPLACE` for refreshing an existing node's
chunks when the external content changes.

### Read side — list view (no payload fetch)

A dashboard listing recent meetings should never touch the transcript
store:

```python
rows = (
    engine.nodes("Meeting")
    .filter_json_timestamp_gte("$.started_at", one_week_ago)
    .execute()
)

for node in rows:
    props = node.properties
    print(f"{props['started_at']}  {props['title']}  "
          f"({len(props['attendees'])} attendees)")
```

No dereference, no external fetch, no 20KB-per-row overhead. The list
view runs entirely against node metadata.

### Read side — detail view (explicit payload fetch)

When the user opens a single meeting, dereference the ref explicitly:

```python
def meeting_detail(engine, meeting_id):
    rows = (
        engine.nodes("Meeting")
        .filter_logical_id_eq(meeting_id)
        .execute()
    )
    if not rows:
        return None
    node = rows[0]

    detail = {
        "logical_id": node.logical_id,
        "properties": node.properties,
        "transcript": None,
    }
    if node.content_ref is not None:
        detail["transcript"] = content_store.get(node.content_ref)
    return detail
```

The detail path pays the external-fetch cost once, on the request
that actually needs it. No query-time blocking on the bulky payload.

## Content refs and search

`search()` returns `SearchHit` objects, each of which carries a
`node: NodeRow`. That `NodeRow` has the same `content_ref` field as
any other node result, so the pattern composes directly:

```python
rows = engine.nodes("Meeting").search("quarterly planning", 20).execute()
for hit in rows.hits:
    if hit.node.content_ref is not None:
        transcript = content_store.get(hit.node.content_ref)
        # ... pass transcript to the caller only when needed
```

What the ref does **not** do: the payload behind `content_ref` is not
indexed by text search or property FTS. If the transcript text should
participate in retrieval, write it as one or more chunks attached to
the node — the chunk text is what the text-search path matches on.
`content_ref` is storage, not retrieval.

## See also

- [Writing Data — External content nodes](./writing-data.md#external-content-nodes)
  — the write-side API for `content_ref`, `content_hash`, and
  refreshing external content with `ChunkPolicy.REPLACE`.
- [Data Model — External Content](../concepts/data-model.md#external-content)
  — the underlying model.
- [Writing Data](./writing-data.md) — chunk attachment, the
  complementary path when external content should participate in
  text search or vector retrieval.
