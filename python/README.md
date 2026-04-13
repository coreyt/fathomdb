# fathomdb

Local datastore for persistent AI agents — graph, vector, and full-text search on SQLite.

## Installation

```bash
pip install fathomdb
```

## Quick Start

```python
from fathomdb import Engine, WriteRequestBuilder

engine = Engine.open("my_agent.db")

# Write data
builder = WriteRequestBuilder("ingest")
node = builder.add_node(
    kind="memory",
    properties={"text": "hello world"},
    source_ref="quickstart",
)
engine.write(builder.build())

# Unified search — the recommended retrieval entry point.
rows = engine.nodes("memory").search("hello", 10).execute()
for hit in rows.hits:
    print(hit.node.logical_id, hit.score, hit.modality.value, hit.snippet)

engine.close()
```

## Features

- Graph backbone with nodes, edges, and temporal tracking
- Unified `search()` entry point -- one call runs a strict-then-relaxed
  text pipeline (with a reserved vector stage for future phases) and
  returns ranked `SearchHit` rows over both document chunks and
  structured property projections
- Vector similarity search via sqlite-vec (advanced override today;
  will fuse into `search()` once read-time query embedding is wired in)
- FTS property schema management -- register JSON property paths per node
  kind, including recursive-mode paths that populate a sidecar position
  map and unlock per-hit match attribution
- Provenance tracking on every write
- Single-writer / multi-reader with WAL

## Unified search

```python
from fathomdb import Engine, FtsPropertyPathMode, FtsPropertyPathSpec

engine = Engine.open("/tmp/fathom.db")

# search() is the primary retrieval entry point. The engine owns the
# strict-then-relaxed policy and returns SearchRows, not QueryRows.
rows = engine.nodes("Goal").search("ship quarterly docs", 10).execute()
for hit in rows.hits:
    print(hit.node.logical_id, hit.score, hit.modality.value,
          hit.source.value, hit.snippet)
print(rows.strict_hit_count, rows.relaxed_hit_count, rows.vector_hit_count)

# Recursive property FTS + opt-in match attribution.
engine.admin.register_fts_property_schema_with_entries(
    "KnowledgeItem",
    entries=[
        FtsPropertyPathSpec(path="$.payload", mode=FtsPropertyPathMode.RECURSIVE),
    ],
)
attributed = (
    engine.nodes("KnowledgeItem")
    .search("quarterly docs", 10)
    .with_match_attribution()
    .execute()
)
for hit in attributed.hits:
    if hit.attribution:
        print(hit.attribution.matched_paths)

# Advanced overrides (pin modality or supply both shapes verbatim):
#   engine.nodes("Goal").text_search("ship quarterly docs", 10).execute()
#   engine.fallback_search("quarterly docs", "quarterly OR docs", 10).execute()
# See docs/guides/querying.md for when each is the right tool.
```

For the full retrieval pipeline, supported query grammar, and property-FTS
schema registration semantics, see the guides under `docs/guides/`.

## Documentation

See the [GitHub repository](https://github.com/coreyt/fathomdb) for full documentation.

## License

MIT
