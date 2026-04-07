# Querying Data

This guide covers how to query nodes from a fathomdb database using the
Python SDK. For the full API surface, see [Query API Reference](../reference/query.md).
For background on nodes, edges, and properties, see
[Data Model](../concepts/data-model.md).

## Starting a query

Every query begins with `db.nodes(kind)`, where `kind` is the node type you
want to match. The call returns an immutable `Query` builder -- each method
returns a **new** `Query`, leaving the original unchanged.

```python
from fathomdb import Engine

db = Engine.open("/tmp/my-agent.db")

base = db.nodes("Document")
draft = base.filter_json_text_eq("$.status", "draft")     # new Query
archived = base.filter_json_text_eq("$.status", "archived")  # another new Query
```

## Filtering

### Identity filters

```python
db.nodes("Document").filter_logical_id_eq("01HXYZ...")      # exact logical ID
db.nodes("Entity").filter_kind_eq("Person")                  # exact kind
db.nodes("Document").filter_source_ref_eq("ingest-run-42")   # provenance anchor
```

### JSON property filters

Paths use SQLite JSON path syntax -- `$.field_name` for a top-level key.

```python
# Text equality
db.nodes("Document").filter_json_text_eq("$.status", "published")

# Boolean equality
db.nodes("Task").filter_json_bool_eq("$.is_complete", True)

# Integer comparisons: gt, gte, lt, lte
db.nodes("Task").filter_json_integer_gte("$.priority", 3)
db.nodes("Task").filter_json_integer_lt("$.priority", 10)

# Timestamp comparisons (Unix epoch integers): gt, gte, lt, lte
import time
one_day_ago = int(time.time()) - 86400
db.nodes("Event").filter_json_timestamp_gte("$.occurred_at", one_day_ago)
```

Filters chain with AND semantics:

```python
rows = (
    db.nodes("Task")
    .filter_json_text_eq("$.status", "open")
    .filter_json_integer_gte("$.priority", 5)
    .limit(20)
    .execute()
)
```

## Search

### Vector search (semantic similarity)

`vector_search` finds nodes whose embedded content is closest to the query.
The database must have been opened with `vector_dimension`.

```python
db = Engine.open("/tmp/my-agent.db", vector_dimension=1536)

results = (
    db.nodes("Document")
    .vector_search("quarterly revenue discussion", limit=10)
    .execute()
)
for node in results.nodes:
    print(node.logical_id, node.properties.get("title"))
```

### Full-text search

`text_search` uses SQLite FTS5 for keyword and phrase matching:

```python
results = (
    db.nodes("Document")
    .text_search("project deadline", limit=20)
    .execute()
)
```

### Combining search with filters

Search and filters compose. The engine pushes the search deep into the
query plan, then applies filters over the reduced set:

```python
results = (
    db.nodes("Document")
    .text_search("architecture review", limit=50)
    .filter_json_text_eq("$.status", "published")
    .limit(10)
    .execute()
)
```

## Graph traversal

`traverse` follows edges from matched nodes. Specify the direction, edge
label, and maximum depth:

```python
from fathomdb import TraverseDirection

authors = (
    db.nodes("Document")
    .filter_json_text_eq("$.title", "Q4 Report")
    .traverse(direction=TraverseDirection.OUT, label="authored_by", max_depth=1)
    .execute()
)
for node in authors.nodes:
    print(node.properties.get("name"))
```

- `TraverseDirection.OUT` -- follow outgoing edges from matched nodes.
- `TraverseDirection.IN` -- follow incoming edges.
- `max_depth=1` -- single hop. Higher values use recursive traversal with
  cycle detection.

You can also pass the direction as a plain string (`"in"` or `"out"`).

## Limit

`limit(n)` caps the total number of result rows. Note that `vector_search`
and `text_search` each accept their own `limit` controlling candidate set
size at the search stage. The top-level `limit()` applies after all steps.

```python
q = db.nodes("Document").limit(5)
```

## Terminal methods

### execute()

Runs the query and returns a `QueryRows` object:

```python
rows = db.nodes("Document").limit(10).execute()

rows.nodes         # list[NodeRow] -- matched nodes
rows.runs          # list[RunRow]  -- associated runs
rows.steps         # list[StepRow] -- associated steps
rows.actions       # list[ActionRow] -- associated actions
rows.was_degraded  # bool -- True if the engine fell back to a simpler plan
```

Each `NodeRow` has: `row_id`, `logical_id`, `kind`, `properties` (decoded
dict), and `last_accessed_at`.

### compile()

Returns a `CompiledQuery` without executing -- useful for inspecting
generated SQL or caching query shapes:

```python
compiled = db.nodes("Document").filter_json_text_eq("$.status", "draft").compile()

compiled.sql            # the generated SQL string
compiled.driving_table  # DrivingTable.NODES, .FTS_NODES, or .VEC_NODES
compiled.shape_hash     # int -- cache key for prepared statements
```

### explain()

Returns a `QueryPlan` with execution metadata but no result rows:

```python
plan = db.nodes("Document").vector_search("budget", limit=5).explain()

plan.driving_table  # DrivingTable.VEC_NODES
plan.shape_hash
plan.cache_hit      # True if a cached prepared statement was reused
```

## Grouped queries and expansions

When you need a root set of nodes plus related subgraphs for each root, use
`expand()` with `execute_grouped()`. This avoids N+1 round trips.

`expand()` registers a named expansion slot -- a traversal that runs for
every root node. You can register multiple slots:

```python
results = (
    db.nodes("Project")
    .filter_json_text_eq("$.active", "true")
    .limit(5)
    .expand(slot="members", direction=TraverseDirection.IN, label="member_of", max_depth=1)
    .expand(slot="tasks", direction=TraverseDirection.IN, label="belongs_to", max_depth=1)
    .execute_grouped()
)
```

`execute_grouped()` returns a `GroupedQueryRows` with three fields:

- `roots` -- `list[NodeRow]`, the matched root nodes.
- `expansions` -- `list[ExpansionSlotRows]`, one per `expand()` call.
  Each slot contains a list of `ExpansionRootRows`, pairing a
  `root_logical_id` with the `list[NodeRow]` reached from that root.
- `was_degraded` -- `bool`.

Reading the results:

```python
for project in results.roots:
    print(f"Project: {project.properties.get('name')}")

for slot in results.expansions:
    print(f"--- {slot.slot} ---")
    for root_rows in slot.roots:
        names = [n.properties.get("name") for n in root_rows.nodes]
        print(f"  {root_rows.root_logical_id}: {names}")
```

## Quick reference

| Goal | Method |
|------|--------|
| Match by logical ID | `filter_logical_id_eq(id)` |
| Match by kind | `filter_kind_eq(kind)` |
| Match by source ref | `filter_source_ref_eq(ref)` |
| JSON text equality | `filter_json_text_eq(path, value)` |
| JSON bool equality | `filter_json_bool_eq(path, value)` |
| JSON integer range | `filter_json_integer_gt/gte/lt/lte(path, value)` |
| JSON timestamp range | `filter_json_timestamp_gt/gte/lt/lte(path, value)` |
| Semantic similarity | `vector_search(query, limit)` |
| Keyword search | `text_search(query, limit)` |
| Graph hop | `traverse(direction, label, max_depth)` |
| Cap results | `limit(n)` |
| Fetch results | `execute()` |
| Inspect SQL | `compile()` |
| Inspect plan | `explain()` |
| Subgraph expansion | `expand(slot, direction, label, max_depth)` + `execute_grouped()` |
