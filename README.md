# fathomdb

Local datastore for persistent AI agents. Graph, vector, and full-text search
on SQLite — exposed through a single query surface in Rust, Python, and
TypeScript.

## What Is FathomDB

FathomDB is not a new storage engine. It is a **datastore layer** built on
SQLite that gives AI agents a durable world model: nodes and edges with
logical identity and supersession, FTS5 search over chunks and structured
node properties, sqlite-vec similarity search, an operational state store
with append-only mutation logs, and end-to-end provenance.

This means that your agent gets a single local file it can read and write
through one library. You can save things as connected objects, search them
by keyword or by meaning, walk from one thing to another along relationships,
and always know where each piece of data came from. Updates never overwrite
history — older versions of a record are kept and marked superseded, so you
can trace what the agent knew at any point. Nothing is a server, nothing is
a cloud service, and the on-disk format is a plain SQLite database you can
back up with `cp`.

The canonical database is a single SQLite file. FathomDB adds the query
compiler, the write coordinator, the schema manager, the derived search
projections, and the SDK surfaces that turn SQLite into a graph + vector +
FTS store an agent can talk to directly.

## Why Not Just SQLite (or Postgres, or DuckDB…)

| Database | Embedded | Graph model | Vector | FTS | Operational / provenance | Agent-shaped SDK |
|---|---|---|---|---|---|---|
| **SQLite** | ✓ | none (build it yourself in app code) | via `sqlite-vec` extension | FTS5 extension | none | no (C library + language bindings) |
| **DuckDB** | ✓ | none (SQL joins only) | `vss` extension | basic via `fts` | none | analytics-first |
| **Postgres** | ✗ (server) | via `AGE` extension | `pgvector` | `tsvector` | none built-in | server + drivers |
| **LadybugDB** | ✓/✗ | relational only | no first-class vector | basic | none | relational SQL |
| **FathomDB** | ✓ (SQLite file) | **native**: logical ids, supersession, typed edges | **native**: per-kind sqlite-vec tables, admin-owned regeneration | **native**: FTS5 over chunks *and* structured property projections | **native**: operational collections, append-only mutation log, source-ref provenance, trace/excise/purge | **native**: Rust + Python + TypeScript with identical semantics |

What this buys you over raw SQLite + `sqlite-vec` + `FTS5` + custom glue:

- **One query surface** that fuses FTS hits, vector hits, and graph traversal
  (`search(...).expand(...).execute_grouped()`).
- **Supersession-aware writes**. Logical ids survive row-level mutation;
  readers never see partial updates or stale tombstones.
- **Admin-owned derived state**. FTS and vector projections rebuild
  deterministically from canonical state. Corruption is recoverable, not
  catastrophic.
- **Provenance by default**. Every write carries a source ref; `trace`,
  `excise`, and `purge-provenance` are built-in admin commands.
- **Single-writer, many-reader.** WAL mode + exclusive file lock + reader
  pool. No coordination primitives to wire up in application code.
- **Cross-language parity.** Python and TypeScript SDKs share the same FFI
  core and are round-trip tested against each other.

## The Fathom Layer

```
┌──────────────────────────────────────────────────────────┐
│  Python SDK (PyO3)   │  TypeScript SDK (napi-rs)         │
│  fathomdb package    │  typescript/packages/fathomdb     │
├──────────────────────────────────────────────────────────┤
│  Rust engine         │  Go operator CLI (fathom-integrity)│
│  crates/fathomdb*    │  integrity, repair, safe-export    │
├──────────────────────────────────────────────────────────┤
│                    SQLite (single file)                   │
│         + FTS5          + sqlite-vec                      │
└──────────────────────────────────────────────────────────┘
```

- **Rust engine** (`crates/fathomdb`, `fathomdb-engine`, `fathomdb-query`,
  `fathomdb-schema`). All business logic: query compiler, write coordinator,
  schema manager, FTS/vector projections, supersession, provenance,
  operational store. Single-writer actor, WAL reader pool, inside-out query
  compilation (narrowest index first, resolve hits through chunks, join into
  graph state, late filter).
- **Python SDK** (`python/fathomdb/`). PyO3 bindings over the full engine
  surface. `pip install fathomdb`. Optional embedders: `fathomdb[openai]`,
  `fathomdb[jina]`, `fathomdb[stella]`, `fathomdb[embedders]`.
- **TypeScript SDK** (`typescript/packages/fathomdb`). `napi-rs` addon. Same
  semantics as the Python SDK; tested for round-trip read/write parity with
  Python on every change.
- **Go operator CLI** (`go/fathom-integrity`). Integrity checks, recovery,
  projection rebuild, safe export, provenance trace/excise/purge, operational
  collection management. JSON-bridge protocol with the Rust engine.

Only one `Engine` may be open per database file at a time, enforced by an
exclusive file lock on `{database_path}.lock`.

## Quick Start

### Python

```bash
pip install fathomdb
```

```python
from fathomdb import Engine, WriteRequestBuilder, new_id, new_row_id

with Engine.open("agent.db") as db:
    builder = WriteRequestBuilder("ingest")
    builder.add_node(
        row_id=new_row_id(), logical_id=new_id(),
        kind="Document", properties={"title": "Meeting notes"},
        source_ref="ingest",
    )
    db.write(builder.build())

    rows = db.nodes("Document").limit(10).execute()
    for hit in rows.nodes:
        print(hit.logical_id, hit.properties)
```

### TypeScript

```bash
npm install fathomdb
```

```typescript
import { Engine, WriteRequestBuilder, newId, newRowId } from "fathomdb";

const db = Engine.open("agent.db");
const builder = new WriteRequestBuilder("ingest");
builder.addNode({
  rowId: newRowId(), logicalId: newId(), kind: "Document",
  properties: { title: "Meeting notes" },
});
db.write(builder.build());

const rows = db.nodes("Document").limit(10).execute();
console.log(rows.nodes);
db.close();
```

### Rust

```toml
[dependencies]
fathomdb = "0.5"
```

```rust
use fathomdb::{Engine, EngineOptions};

let db = Engine::open(EngineOptions::new("agent.db"))?;
// write + query via the engine API
```

### Operator CLI

```bash
cd go/fathom-integrity && go build ./cmd/fathom-integrity
./fathom-integrity check-integrity --db agent.db
./fathom-integrity safe-export --db agent.db --out backup/
```

## Development Setup

```bash
bash scripts/setup_dev.sh     # toolchain + deps
cargo test --workspace         # Rust
pip install -e python/ && pytest --rootdir python python/tests/
cd typescript/packages/fathomdb && npm test
```

## Repository Structure

```
crates/           Rust workspace (fathomdb, fathomdb-engine, fathomdb-query, fathomdb-schema)
python/           Python SDK (PyO3 bindings) and tests
typescript/       TypeScript SDK workspace and consumer harness
go/               Go operator tooling (fathom-integrity CLI)
docs/             User and operator documentation
dev/              Current developer docs; historical notes live in dev/archive/
scripts/          Developer setup and CI helpers
tests/            Cross-language SDK consistency tests
.github/          CI workflows
```

## Test Coverage

330+ tests across Rust, Python, TypeScript, and Go, organized in a five-layer
plan covering unit tests, integration tests, cross-language round-trips, CLI
smoke tests, and fuzz testing. The cross-language consistency harness at
`tests/cross-language/` proves that Python and TypeScript SDKs produce
identical database state and can read each other's writes.

## Documentation

See [`docs/`](docs/index.md) for concepts, guides, API reference, and operator
docs. Current architecture, support contracts, and active design notes live in
[`dev/`](dev/); superseded material is kept under `dev/archive/`.

## License

Licensed under the [MIT License](LICENSE).
