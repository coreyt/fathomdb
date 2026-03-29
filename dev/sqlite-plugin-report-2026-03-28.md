# FathomDB SQLite Plugin Report

Date: 2026-03-28

## Executive summary

FathomDB is **not directly transferable** into a SQLite plugin in the same style as `sqlite-vec`.

The short version:

- **Some internal pieces are transferable**.
- **The current overall architecture is not plugin-shaped**.
- Turning it into a true SQLite loadable extension would require a **substantial redesign**, not a packaging change.

My practical assessment:

- **As-is transfer to a sqlite-vec-style plugin:** low
- **Partial extraction into extension-friendly components:** medium
- **Full SQLite-extension rearchitecture:** possible, but expensive

## Why the current design is not plugin-shaped

The repo is explicit that FathomDB is a **shim in front of SQLite**, not an in-SQLite extension layer.

Evidence:

- The architecture defines four strata: fluent AST builder, query compiler, execution coordinator, and write/projection pipeline ([dev/ARCHITECTURE.md:15-20](dev/ARCHITECTURE.md)).
- It also explicitly positions SDKs as the primary interface surface ([dev/ARCHITECTURE.md:5-13](dev/ARCHITECTURE.md), [dev/ARCHITECTURE.md:22-29](dev/ARCHITECTURE.md)).

That matters because a `sqlite-vec`-style plugin usually centers on SQLite extension entrypoints and SQL-visible modules/functions. FathomDB centers on:

- an embedded Rust runtime
- application-owned connection lifecycle
- SDK-built ASTs
- separate read, write, and admin services

## The strongest blockers

### 1. FathomDB owns a runtime, not just SQL objects

`EngineRuntime::open` constructs three subsystems:

- `ExecutionCoordinator`
- `WriterActor`
- `AdminHandle`

Evidence: [crates/fathomdb-engine/src/runtime.rs:11-41](crates/fathomdb-engine/src/runtime.rs)

That is an embedded-engine shape. A SQLite plugin normally exposes:

- virtual tables
- scalar/aggregate/table-valued functions
- collations
- extension init hooks

FathomDB instead boots a service graph.

### 2. Writes depend on a dedicated background writer thread

The write path is not just SQL helpers. It starts a named thread and communicates through channels:

- `thread::Builder::new()`
- `mpsc::channel`
- `WriterActor::start`
- `WriterActor::submit`

Evidence: [crates/fathomdb-engine/src/writer.rs:191-236](crates/fathomdb-engine/src/writer.rs)

That is a major mismatch with a sqlite-vec-style extension. An extension can technically spawn threads, but doing so as the core write path is a very different operational model from SQLite’s usual “host owns the connection, extension augments it” pattern.

### 3. Admin operations assume filesystem and process orchestration

The admin surface does much more than register SQL behavior:

- opens connections by database path ([crates/fathomdb-engine/src/admin.rs:200-219](crates/fathomdb-engine/src/admin.rs))
- runs WAL checkpoints and SQLite backup API exports ([crates/fathomdb-engine/src/admin.rs:829-900](crates/fathomdb-engine/src/admin.rs))
- writes manifest files beside exports ([crates/fathomdb-engine/src/admin.rs:902-910](crates/fathomdb-engine/src/admin.rs))
- spawns external vector-generator processes with stdin/stdout piping, timeouts, and reader threads ([crates/fathomdb-engine/src/admin.rs:1521-1705](crates/fathomdb-engine/src/admin.rs))

That is application-runtime behavior, not normal loadable-extension behavior.

### 4. The integration boundary is `rusqlite::Connection`, not SQLite extension ABI

The engine and schema manager take `rusqlite::Connection` values and open database files themselves:

- `open_connection(path)` opens the DB with flags ([crates/fathomdb-engine/src/sqlite.rs:17-23](crates/fathomdb-engine/src/sqlite.rs))
- `ExecutionCoordinator::open` opens a path, bootstraps schema, and manages an internal mutex-wrapped connection ([crates/fathomdb-engine/src/coordinator.rs:96-141](crates/fathomdb-engine/src/coordinator.rs))
- `SchemaManager::bootstrap` mutates the schema on a connection and applies migrations ([crates/fathomdb-schema/src/bootstrap.rs:213-260](crates/fathomdb-schema/src/bootstrap.rs))

That is not how a sqlite-vec-style extension is usually entered. In that model, SQLite calls the extension through the extension ABI and the extension registers capabilities against the host connection/database engine.

### 5. There is no current SQLite-extension registration surface

I found no evidence of extension-style registration hooks such as:

- `sqlite3_extension_init`
- SQLite module registration
- scalar/aggregate/window function registration
- loadable extension entrypoints

Repo search found none for:

- `sqlite3_extension_init`
- `create_module`
- `create_scalar_function`
- `load_extension`

That absence is significant because it means the project is not already halfway packaged as an extension. It would need a new ABI-facing layer.

## What *is* transferable

### 1. The query compiler is the most extension-friendly piece

`fathomdb-query` is comparatively pure:

- it defines an AST and builder ([crates/fathomdb-query/src/lib.rs:1-11](crates/fathomdb-query/src/lib.rs))
- it compiles AST to SQL plus bind values ([crates/fathomdb-query/src/compile.rs:16-23](crates/fathomdb-query/src/compile.rs))
- it enforces structural limits without touching a live SQLite connection ([crates/fathomdb-query/src/compile.rs:25-105](crates/fathomdb-query/src/compile.rs))

This part could be repurposed behind:

- a SQL function that compiles JSON AST to SQL text
- an eponymous virtual table/table-valued function that accepts an AST payload
- a helper extension that exposes planner utilities

But that would still only expose a slice of FathomDB, not the whole system.

### 2. Schema bootstrap/migrations could be reused

The schema manager is SQL-centric and declarative:

- canonical tables, indexes, and virtual tables are defined in migration SQL ([crates/fathomdb-schema/src/bootstrap.rs:5-194](crates/fathomdb-schema/src/bootstrap.rs))
- connection initialization applies PRAGMAs ([crates/fathomdb-schema/src/bootstrap.rs:355-371](crates/fathomdb-schema/src/bootstrap.rs))
- vector-profile setup already creates vec virtual tables dynamically ([crates/fathomdb-schema/src/bootstrap.rs:374-405](crates/fathomdb-schema/src/bootstrap.rs))

This is reusable, but it still assumes FathomDB owns schema lifecycle and metadata tables.

### 3. The project already knows how to load another extension

The engine registers `sqlite-vec` as an auto-extension before opening connections ([crates/fathomdb-engine/src/sqlite.rs:26-47](crates/fathomdb-engine/src/sqlite.rs)).

That is useful evidence that the codebase is comfortable living near SQLite’s extension machinery, but it is still using that machinery as a **consumer** of an extension, not as a producer of one.

## What would have to change for a real plugin form

To look and behave like `sqlite-vec`, FathomDB would need a different top-level contract.

### A. New entry surface

It would need to register SQL-visible capabilities through SQLite’s extension ABI, for example:

- scalar functions
- table-valued functions
- virtual tables
- possibly an init function for schema bootstrap

Today, the top-level surfaces are SDK methods and engine objects, not SQL-visible extension registrations.

### B. Rework the write path

The current write path is typed Rust request submission into a writer actor. A plugin form would need to expose writes through one of these models:

- SQL procedures/functions operating on JSON payloads
- virtual tables that translate inserts/updates into canonical writes
- triggers plus extension functions

That is a deep redesign, because the current write interface is not SQL-native.

### C. Reframe admin operations

Several admin features are awkward or inappropriate inside a loadable extension:

- safe export to arbitrary filesystem paths
- manifest file creation
- external vector generator process management
- long-running repair/excision workflows

Those are better left in an external tool or host library, even if some narrow helper functions move into an extension.

### D. Replace SDK-first ergonomics with SQL-first ergonomics

The current product direction is intentionally “agent-friendly SDK first,” not “SQL first” ([dev/ARCHITECTURE.md:17-19](dev/ARCHITECTURE.md), [dev/ARCHITECTURE.md:31-37](dev/ARCHITECTURE.md)).

A plugin model would invert that:

- SQL becomes the public surface
- ASTs would need to be serialized into SQL-callable forms
- extension callers would drive behavior from SQLite, not from Rust/Python engine objects

That is a product-shape change, not just an implementation change.

## Best realistic path if plugin support is desired

The most defensible route is **not** “turn all of FathomDB into sqlite-vec.”

It is:

1. Keep the full engine as an embedded library/runtime.
2. Extract the most SQLite-native pieces into a companion extension.
3. Leave admin orchestration and governed writes in the host layer.

The pieces most worth extracting are:

- schema/bootstrap helpers
- ID helpers
- some integrity/projection maintenance helpers
- possibly a table-valued query surface backed by the existing compiler

That would produce something more like:

- `fathomdb-core` as embedded engine/runtime
- `fathomdb-sqlite-ext` as a narrow SQLite extension

rather than a single monolithic extension.

## Verdict

The way FathomDB is currently written makes it **partially transferable**, but **not naturally transferable**, to becoming a SQLite plugin similar to `sqlite-vec`.

The codebase already contains reusable SQL-centric pieces:

- schema/migrations
- query compilation
- close coupling to SQLite features and virtual tables

But the current architecture fundamentally assumes:

- a host-owned runtime
- path-based connection ownership
- a background writer actor
- admin/file/process orchestration outside SQLite
- SDK-first interaction rather than SQL-first interaction

So the bottom line is:

> FathomDB could evolve to include a SQLite extension layer, but the current implementation is not a near drop-in candidate for becoming a sqlite-vec-style plugin. A companion extension is realistic; a full conversion would require major re-architecture.
