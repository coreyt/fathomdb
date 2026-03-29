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
- **Response-cycle feedback**: operation progress reporting across Rust, Python,
  and Go/CLI surfaces

## Quick Start

**Developer setup:**

```bash
bash scripts/developer-setup.sh
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

**Go operator CLI:**

```bash
cd go/fathom-integrity && go build ./cmd/fathom-integrity
```

**Documentation:** see the `docs/` directory and design documents in `dev/`.

## Repository Structure

```
crates/           Rust workspace (fathomdb, fathomdb-engine, fathomdb-query, fathomdb-schema)
python/           Python SDK (PyO3 bindings) and examples
go/               Go operator tooling (fathom-integrity CLI)
docs/             User and operator documentation
dev/              Design documents and internal notes
scripts/          Developer setup and CI helpers
tooling/          Build-time configuration (SQLite env)
.github/          CI workflows (Rust, Go, Python)
```

## Test Coverage

296+ tests across Rust, Go, and Python, organized in a 5-layer test plan
covering unit tests, integration tests, cross-language round-trips, CLI
smoke tests, and fuzz testing.

## License

No license file is currently included in this repository.
