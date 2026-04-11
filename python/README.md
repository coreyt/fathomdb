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
- Full-text search via SQLite FTS5 -- searches both document chunks and
  schema-declared structured node property projections transparently
- Vector similarity search via sqlite-vec
- FTS property schema management -- register JSON property paths per node kind
  to make structured data searchable without synthetic chunks
- Provenance tracking on every write
- Single-writer / multi-reader with WAL

## Documentation

See the [GitHub repository](https://github.com/coreyt/fathomdb) for full documentation.

## License

MIT
