# Getting Started

A concise guide for new developers and users of fathomdb.

## Prerequisites

| Tool | Minimum version | Notes |
|------|----------------|-------|
| Rust | stable (via [rustup](https://rustup.rs)) | Used for the core engine |
| Go | 1.22+ | Used for `fathom-integrity` recovery tooling |
| Python | 3.11+ | Python bindings (PyO3) |
| Node.js | 20+ | TypeScript/JavaScript bindings (napi-rs) |

For users who just need to build and use fathomdb, run `scripts/setup.sh`.
For full developer tooling (testing, linting, Go, project-local SQLite),
run `scripts/setup_dev.sh` instead. Run once after cloning:

```bash
# Users / CI build-only:
./scripts/setup.sh

# Developers:
./scripts/setup_dev.sh
```

## Building

**Rust engine:**

```bash
cargo build --workspace
```

**Python bindings** (editable install, builds the native extension via maturin):

```bash
cd python && pip install -e . --no-build-isolation
```

**TypeScript SDK** (requires the native binding to be built first):

```bash
cargo build -p fathomdb --features node
cd typescript && npm install
```

**Go recovery tool:**

```bash
cd go/fathom-integrity && go build ./cmd/fathom-integrity
```

## Running tests

```bash
# Rust (296+ tests)
cargo test --workspace

# Go
cd go/fathom-integrity && go test ./...

# Python
PYTHONPATH=python pytest python/tests -q

# TypeScript
cd typescript && npm test
```

## First database (Python)

```python
from fathomdb import Engine, WriteRequestBuilder, new_id, new_row_id

# Open (or create) a database
db = Engine.open("/tmp/my-agent.db")

# Build a write request
wrb = WriteRequestBuilder("first-write")
wrb.add_node(
    row_id=new_row_id(),
    logical_id=new_id(),
    kind="Document",
    properties={"title": "Hello"},
    source_ref="setup-guide",
)
request = wrb.build()

# Submit the write
receipt = db.write(request)

# Query nodes back
rows = (
    db.nodes("Document")
    .filter_json_text_eq("$.title", "Hello")
    .limit(10)
    .execute()
)
```

Key points:

- `Engine.open()` creates the database file and bootstraps the schema if it
  does not already exist.
- `WriteRequestBuilder.add_node()` requires explicit `row_id` and
  `logical_id` values; use `new_row_id()` and `new_id()` to generate them.
- `Query` objects are immutable builders -- each filter/limit call returns a
  new `Query`.

## First database (TypeScript)

```typescript
import { Engine, WriteRequestBuilder, newId, newRowId } from "fathomdb";

// Open (or create) a database
const engine = Engine.open("/tmp/my-agent.db");

// Build a write request
const builder = new WriteRequestBuilder("first-write");
builder.addNode({
  rowId: newRowId(),
  logicalId: newId(),
  kind: "Document",
  properties: { title: "Hello" },
  sourceRef: "setup-guide",
});
engine.write(builder.build());

// Query nodes back
const rows = engine.nodes("Document")
  .filterJsonTextEq("$.title", "Hello")
  .limit(10)
  .execute();
console.log(rows.nodes);

engine.close();
```

Key points:

- `Engine.open()` creates the database file and bootstraps the schema if it
  does not already exist.
- `WriteRequestBuilder.addNode()` requires explicit `rowId` and `logicalId`
  values; use `newRowId()` and `newId()` to generate them.
- `Query` objects are immutable builders -- each filter/limit call returns a
  new `Query`.

## First database (Rust)

```rust
use fathomdb::{
    Engine, EngineOptions, WriteRequestBuilder, QueryBuilder,
    new_id, new_row_id, ChunkPolicy, compile_query,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Engine::open(EngineOptions::new("/tmp/my-agent.db"))?;

    // Write a node
    let mut wrb = WriteRequestBuilder::new("first-write");
    wrb.add_node(
        new_row_id(), new_id(), "Document",
        r#"{"title":"Hello"}"#,
        Some("setup-guide".into()), false, ChunkPolicy::Preserve,
    );
    engine.writer().submit(wrb.build()?)?;

    // Query it back
    let query = QueryBuilder::nodes("Document")
        .filter_json_text_eq("$.title", "Hello")
        .limit(10);
    let compiled = query.compile()?;
    let rows = engine.coordinator().execute_compiled_read(&compiled)?;

    Ok(())
}
```

`EngineOptions` also accepts `vector_dimension` (for sqlite-vec support),
`read_pool_size` (defaults to 4), and `telemetry_level` (defaults to
`TelemetryLevel::Counters`).

## Telemetry

fathomdb tracks operation counters and SQLite cache statistics by default:

```python
snap = db.telemetry_snapshot()
print(f"queries: {snap.queries_total}, writes: {snap.writes_total}")
print(f"cache hits: {snap.cache_hits}, misses: {snap.cache_misses}")
```

See [docs/telemetry.md](./operations/telemetry.md) for configuration, rate
computation, and integration patterns.

## Admin operations

Both Python and Rust expose admin checks that verify database health:

```python
db = Engine.open("/tmp/my-agent.db")

# SQLite-level integrity check (PRAGMA integrity_check under the hood)
integrity = db.admin.check_integrity()

# Semantic validation (checks graph invariants, dangling edges, etc.)
semantics = db.admin.check_semantics()
```

In Rust, these are available on the `AdminHandle` returned by
`engine.admin()`.

## Next steps

- [docs/telemetry.md](./operations/telemetry.md) -- resource telemetry and profiling levels.
- [docs/vector-regeneration.md](./operations/vector-regeneration.md) -- rebuilding vector indexes.
- `dev/ARCHITECTURE.md` -- system design and module layout (internal design doc).
