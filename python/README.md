# fathomdb

Local datastore for persistent AI agents — graph, vector, and full-text search on SQLite.

## Installation

```bash
pip install fathomdb
```

## Quick Start

```python
from fathomdb import Engine

engine = Engine.open("my_agent.db")

with engine.session() as session:
    # Write data
    w = session.write_builder()
    node = w.insert_node(kind="memory", properties={"text": "hello world"})
    session.execute_write(w)

    # Query data
    q = session.query_builder()
    q.kind("memory")
    results = session.execute_query(q)

engine.close()
```

## Features

- Graph backbone with nodes, edges, and temporal tracking
- Adaptive text search via SQLite FTS5 -- one `text_search()` call runs a
  strict-then-relaxed pipeline and returns ranked `SearchHit` rows over
  both document chunks and structured property projections
- Vector similarity search via sqlite-vec
- FTS property schema management -- register JSON property paths per node
  kind, including recursive-mode paths that populate a sidecar position
  map and unlock per-hit match attribution
- Provenance tracking on every write
- Single-writer / multi-reader with WAL

## Adaptive Text Search

```python
from fathomdb import Engine, FtsPropertyPathMode, FtsPropertyPathSpec

engine = Engine.open("/tmp/fathom.db")

# Adaptive text search — engine owns strict-then-relaxed retrieval.
rows = engine.query("Goal").text_search("ship quarterly docs", 10).execute()
for hit in rows.hits:
    print(hit.node.logical_id, hit.score, hit.source.value,
          hit.match_mode.value, hit.snippet)
print(rows.strict_hit_count, rows.relaxed_hit_count, rows.fallback_used)

# Recursive property FTS + opt-in match attribution.
engine.admin.register_fts_property_schema_with_entries(
    "KnowledgeItem",
    entries=[
        FtsPropertyPathSpec(path="$.payload", mode=FtsPropertyPathMode.RECURSIVE),
    ],
)
attributed = (
    engine.query("KnowledgeItem")
    .text_search("quarterly docs", 10)
    .with_match_attribution()
    .execute()
)
for hit in attributed.hits:
    if hit.attribution:
        print(hit.attribution.matched_paths)

# Explicit two-shape fallback search (narrow helper, not a general
# query-composition API — see docs/guides/querying.md).
fb = engine.fallback_search("quarterly docs", "quarterly OR docs", 10).execute()
```

For the full adaptive policy, supported query grammar, and property-FTS
schema registration semantics, see the guides under `docs/guides/`.

## Documentation

See the [GitHub repository](https://github.com/coreyt/fathomdb) for full documentation.

## License

MIT
