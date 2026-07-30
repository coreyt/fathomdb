# FathomDB

Python SDK for **FathomDB** — a local-first, embedded retrieval engine for application and agent
workloads, built on SQLite.

A FathomDB database is a single file on your disk. There is no server, no daemon and no network
call: you open a path and you have hybrid full-text + vector search over your own data, in your own
process.

## Status: pre-1.0, beta

The 0.8.x line is under active development. The public surface can change between minor releases,
and scale/stability guarantees are staged for 0.9.x and later.

## Install

```bash
pip install fathomdb
```

Requires Python 3.10 or newer. Wheels are `abi3`, so one wheel per platform serves every supported
interpreter. The package is a PyO3 binding to the FathomDB Rust engine and has no Python
dependencies of its own.

## Quick start

```python
from fathomdb import Engine

engine = Engine.open("./app.sqlite")

engine.write([
    {
        "kind": "doc",
        "body": "the quick brown fox",
        # Provenance is MANDATORY on every canonical row: source_id is the
        # axis erase_source() erases on, so a row without one is unerasable.
        "source_id": "import-2026-07",
    }
])

result = engine.search("brown fox")
for hit in result.results:
    print(hit.id, hit.kind)

engine.close()
```

## What is on the surface

- **Engine lifecycle** — `Engine.open`, `engine.close`, `engine.drain`, `engine.counters()`,
  `engine.open_report()`, and a logging-subscriber seam.
- **Writes** — `engine.write(batch)` of canonical nodes
  (`{"kind", "body", "source_id", "logical_id"?, "state"?, "reason"?, "valid_from"?,
  "valid_until"?}`) and edges (`{"edge": {"kind", "from", "to", "source_id"}}`).
- **Search** — `engine.search(query, ...)` fuses FTS5 lexical retrieval with dense vector
  retrieval, with optional cross-encoder reranking, JSON filters, and an opt-in explain sidecar. A
  `soft_fallback` field reports when a branch could not contribute, rather than degrading silently.
- **Reads** — the `fathomdb.read` module: `read.get`, `read.get_many`, `read.list`,
  `read.collection`, `read.mutations`, `read.projections`, `read.crossed_boundary_since`. Each
  takes a `ReadView` selecting existence state and world-time validity.
- **Graph** — `fathomdb.graph.neighbors` and `fathomdb.graph.search_expand` for bounded BFS
  expansion from search hits.
- **Record lifecycle** — `engine.transition`, `engine.purge`, `engine.erase_source`.
- **Projections** — `engine.configure_projections`, declaring which fields are filterable,
  searchable or rankable. The engine owns every derived index; you never maintain one by hand.
- **Typed errors** — a single-rooted hierarchy under `fathomdb.errors.EngineError` with typed
  payload attributes, so you branch on `DatabaseLockedError` or `EmbedderNotConfiguredError`
  instead of parsing message strings.

## Embeddings

Vector search needs vectors. `Engine.open(path, use_default_embedder=True)` opts into the engine's
pinned default embedder; on first use the weights are downloaded and cached under
`~/.cache/fathomdb/embedders/`. The default is `False` — a plain `Engine.open` performs no network
access at all, and vector writes then raise `EmbedderNotConfiguredError` rather than quietly doing
nothing.

## Erasing data

Every canonical row carries a `source_id`, and that is deliberate: it is what makes the row
erasable. `engine.erase_source(source_id)` removes every row written under that provenance together
with its full-text, vector and secondary-index shadows, and finishes the erasure at rest.
`engine.purge(logical_id)` does the same for a single governed record. Both are idempotent, so an
interrupted erasure obligation can simply be retried.

## Other SDKs

The same engine is available as a Rust crate ([`fathomdb`](https://crates.io/crates/fathomdb)) and
a Node.js package (`npm install fathomdb`).

## Building from source

From a checkout of the repository:

```bash
pip install -e src/python/
```

## License

MIT. See the `LICENSE` file shipped in this distribution.

Source, issues and full documentation: <https://github.com/coreyt/fathomdb>
