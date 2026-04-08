# fathomdb

Local datastore for persistent AI agents. Graph, vector, and full-text search
on SQLite.

## What It Does

fathomdb is canonical local storage for AI agent systems that need a durable
world model, not just a pile of documents. It provides a graph backbone with
logical identity and supersession, chunk-based full-text search (FTS5), vector
search (sqlite-vec), an operational state store with append-only logs and
latest-state collections, and provenance tracking with source attribution.
SQLite remains the single durable file; fathomdb adds an agent-friendly query
compiler, derived search projections, and a governed write path. The engine is
designed for recoverability: canonical state is separated from derived
projections so recovery and rebuild are normal admin operations.

## Architecture

Three layers:

- **Rust engine** (`fathomdb` crate) -- all business logic, query compilation,
  write coordination, schema management. Single-writer execution model with
  WAL-backed reader pool. The query compiler works inside-out: start from the
  narrowest indexed candidate set, resolve vector/FTS hits through chunks, join
  into canonical graph state, apply late filtering.

- **Python SDK** (`fathomdb` package) -- PyO3 bindings exposing the full Rust
  API surface. pip-installable with optional sqlite-vec support for
  vector-capable builds.

- **TypeScript SDK** (`typescript/packages/fathomdb`) -- in-repo Node.js SDK
  surface backed by a `napi-rs` addon and consumer harness application.

- **Go operator CLI** (`fathom-integrity`) -- integrity checks, recovery,
  repair, projection rebuild, safe export, provenance trace/excise,
  operational collection management, and vector regeneration. Communicates with
  the Rust engine via a JSON bridge binary.

## Key Capabilities

- **Graph backbone**: nodes, edges, logical identity, supersession (upsert
  without mutation), runs, steps, actions
- **Chunk-based FTS** via SQLite FTS5
- **Vector search** via sqlite-vec with admin-owned regeneration workflow
- **Operational state store**: append-only logs, latest-state collections,
  retention policies, secondary indexes, compaction, validation contracts
- **Provenance tracking**: source attribution on every write, trace by
  source_ref, excise bad lineage, purge provenance events with selective
  preservation
- **Safe export** with WAL checkpoint and manifest
- **Integrity checks**: physical (sqlite3 integrity_check), semantic (FK
  consistency, orphan detection), and engine-level checks via bridge
- **Projection rebuild**: deterministic rebuild of FTS and vector projections
  from canonical state, including rebuild-missing for gap repair
- **Restore/purge lifecycle**: restore retired logical IDs, permanently purge
  retired objects and their edges
- **Repair commands**: duplicate active logical IDs, broken runtime FK chains,
  orphaned chunks (with dry-run support)
- **Crash recovery**: full database recovery from corrupt SQLite files with
  schema bootstrap
- **Resource telemetry**: always-on operation counters and SQLite cache
  statistics, configurable profiling levels for statement-level and deep
  process metrics
- **Response-cycle feedback**: operation progress reporting across Rust, Python,
  and Go/CLI surfaces
- **Structured tracing**: feature-gated `tracing` instrumentation across all
  engine seams, SQLite internal event bridging, per-consumer configuration
  (Rust subscriber, Python logging via pyo3-log, Go bridge JSON stderr)

## Quick Start

**Developer setup:**

```bash
bash scripts/setup_dev.sh
```

**Run tests:**

```bash
cargo test --workspace
```

**Python SDK:**

```bash
pip install fathomdb
# or for development:
cd python && pip install -e . --no-build-isolation
```

```python
from fathomdb import Engine

with Engine.open("agent.db") as db:
    db.write(...)
    rows = db.nodes("Document").limit(10).execute()
```

Only one `Engine` may be open per database file (enforced by exclusive file
lock).  Use the context manager or call `db.close()` explicitly to release
resources.

**TypeScript SDK:**

```bash
npm install fathomdb
```

```typescript
import { Engine, WriteRequestBuilder, newId, newRowId } from "fathomdb";

const engine = Engine.open("agent.db");

const builder = new WriteRequestBuilder("ingest");
builder.addNode({
  rowId: newRowId(), logicalId: newId(), kind: "Document",
  properties: { title: "Meeting notes" },
});
engine.write(builder.build());

const rows = engine.nodes("Document").limit(10).execute();
console.log(rows.nodes);

engine.close();
```

Only one `Engine` may be open per database file (enforced by exclusive file
lock).  Call `engine.close()` explicitly to release resources.

**Go operator CLI:**

```bash
cd go/fathom-integrity && go build ./cmd/fathom-integrity
```

**Documentation:** see [docs/](docs/index.md) for concepts, guides, API reference, and operator docs.

## Repository Structure

```
crates/           Rust workspace (fathomdb, fathomdb-engine, fathomdb-query, fathomdb-schema)
python/           Python SDK (PyO3 bindings) and examples
typescript/       TypeScript SDK workspace and consumer harness
go/               Go operator tooling (fathom-integrity CLI)
docs/             User and operator documentation
dev/              Design documents and internal notes
scripts/          Developer setup and CI helpers
tooling/          Build-time configuration (SQLite env)
tests/            Cross-language SDK consistency tests
.github/          CI workflows (Rust, Go, Python, TypeScript)
```

## Test Coverage

330+ tests across Rust, Go, Python, and TypeScript, organized in a 5-layer test
plan covering unit tests, integration tests, cross-language round-trips, CLI
smoke tests, and fuzz testing.  A cross-language consistency harness
(`tests/cross-language/`) proves that Python and TypeScript SDKs produce
identical database state and can read each other's writes.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
