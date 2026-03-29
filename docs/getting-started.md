# Getting Started

A concise guide for new developers and users of fathomdb.

## Prerequisites

| Tool | Minimum version | Notes |
|------|----------------|-------|
| Rust | stable (via [rustup](https://rustup.rs)) | Used for the core engine |
| Go | 1.22+ | Used for `fathom-integrity` recovery tooling |
| Python | 3.11+ | Python bindings (PyO3) |

The `scripts/developer-setup.sh` script automates toolchain installation
(Rust stable, Go 1.24+, and a pinned SQLite build). Run it once after
cloning:

```bash
./scripts/developer-setup.sh
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

`EngineOptions` also accepts `vector_dimension` (for sqlite-vec support)
and `read_pool_size` (defaults to 4).

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

- [docs/vector-regeneration.md](./vector-regeneration.md) -- rebuilding vector indexes.
- [dev/ARCHITECTURE.md](../dev/ARCHITECTURE.md) -- system design and module layout.
