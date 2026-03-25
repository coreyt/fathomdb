# Design: Read Execution

## Purpose

This document scopes the next design pass for the **query compiler to SQLite
execution** layer.

The goal is not to revisit the compiler architecture already described in
[ARCHITECTURE.md](./ARCHITECTURE.md). The goal is to define the missing runtime
shape that turns a compiled query into an executed read against a live SQLite
database.

Companion docs:

- [setup-sqlite-vec-capability.md](./setup-sqlite-vec-capability.md)
- [setup-round-trip-fixtures.md](./setup-round-trip-fixtures.md)

## Layer Boundary

This layer starts after `fathomdb-query` has produced a `CompiledQuery`.

Responsibilities in scope:

- open and manage WAL-friendly reader connections
- cache prepared statements by AST shape hash
- bind runtime values safely
- execute compiled reads
- decode rows into an engine-facing result shape
- preserve planner assumptions around inside-out execution

Responsibilities out of scope:

- AST design and SQL generation
- canonical write preparation
- repair workflows and provenance mutation semantics
- SDK bindings

## Current Repository State

The current scaffold is intentionally thin:

- `fathomdb-query` already produces SQL, binds, shape hashes, and planner hints
- `ExecutionCoordinator` bootstraps schema and caches SQL text by shape hash
- no code executes prepared reads yet
- no shared row-decoding or result model exists yet

This means the current read path proves the boundary shape, but not the runtime
behavior.

## Design Goals

1. Keep `fathomdb-query` pure and DB-agnostic.
2. Keep the engine synchronous in v1.
3. Preserve the current planner contract:
   - shape hash keys the cache
   - structural constants remain in SQL
   - user data remains bound
4. Avoid overcommitting to a wide SDK result model too early.
5. Make it easy to add `sqlite-vec` once capability detection is real.

## Proposed Runtime Shape

### 1. Start With One Executable Read Surface

Add one concrete engine entrypoint for v1:

```rust
pub fn execute_compiled_read(&self, compiled: &CompiledQuery) -> Result<QueryRows, EngineError>
```

`QueryRows` should stay narrow at first. It only needs to support the currently
compiled node-shaped reads:

- `row_id`
- `logical_id`
- `kind`
- `properties`

Do not invent a generic columnar result system yet.

### 2. Separate Cache Keys From Live Statements

The current `HashMap<ShapeHash, String>` is useful scaffolding but it is not yet
the real execution cache.

The next design should decide whether v1 uses:

- a per-connection statement cache keyed by `ShapeHash`
- or a coordinator-level cache of canonical SQL text plus connection-local
  prepare-on-demand

The second option is safer for the current architecture because `rusqlite`
prepared statements are tied to one connection.

### 3. Formalize Bind Translation

`BindValue` already exists, but the runtime still needs one binding adapter that
maps it into `rusqlite` values in a single place.

That adapter should be the only place that knows how to bind:

- text
- integer
- bool
- future vector or blob inputs

### 4. Keep Row Decoding Typed But Narrow

The engine should define one initial decoded read type, for example:

- `NodeRow`

That keeps the first runtime loop simple:

1. compile
2. bind
3. execute
4. decode node rows

If later query shapes need joins against runtime tables, add additional result
types then.

### 5. Make Vector Capability A Runtime Gate

The compiler can still emit vector-driven SQL shapes, but execution needs a
clear rule for what happens when `sqlite-vec` is unavailable.

The preferred model is:

- compile succeeds
- execution fails with an explicit capability error if the selected plan depends
  on vector support that is not enabled

That preserves the compiler/runtime boundary cleanly.

## Key Design Questions

1. Should v1 maintain a small read-connection pool, or a single reusable reader
   connection inside `ExecutionCoordinator`?
2. Should execution return decoded structs only, or also expose raw diagnostic
   metadata such as SQL text, bind count, and driving table?
3. How much statement caching is actually useful before benchmarks exist?
4. Should FTS and vector capability checks happen:
   - during bootstrap
   - during query execution
   - or both?
5. When runtime-table joins arrive, do they share the same row decoder path or
   branch into distinct result decoders?

## First Implementation Slice

1. Add `NodeRow` and `QueryRows` to `fathomdb-engine`.
2. Add bind translation from `BindValue` into `rusqlite`.
3. Replace `dispatch_compiled_read()` with an executable read method while
   preserving shape-hash caching.
4. Add one end-to-end test:
   - write canonical node plus chunk plus FTS row
   - compile text query
   - execute read
   - assert decoded row content
5. Add one explicit failure test for vector execution without `sqlite-vec`.

## Definition Of Done For This Design Pass

This layer is scaffolded enough when:

- a compiled query can be executed against SQLite
- the engine returns decoded node rows, not just SQL text
- the cache contract is clear
- vector capability failure is explicit
- one black-box roundtrip test proves the path

## Implementation Checklist

### Phase 1: First Runtime Slice

- [x] Add a black-box failing test that proves a compiled text query can execute
      and return decoded node rows.
- [x] Add `NodeRow` and `QueryRows` result types in `fathomdb-engine`.
- [x] Add bind translation from `BindValue` into `rusqlite` parameters.
- [x] Add `ExecutionCoordinator::execute_compiled_read(...)`.
- [x] Preserve shape-hash SQL caching while executing against SQLite.
- [x] Re-export the new read result types from the public facade.
- [x] Make the new black-box test pass.

### Phase 2: Read Runtime Hardening

- [x] Decide whether the coordinator owns one reusable reader connection or a
      small read pool in v1. (Decision: one Mutex<Connection> stored at open time.)
- [x] Add targeted unit coverage for bind translation and row decoding.
- [x] Add a cache-behavior test that proves repeated query shapes reuse the same
      cached SQL entry.
- [x] Add an explicit failure test for vector-driven reads when `sqlite-vec` is
      unavailable. (Asserts EngineError::CapabilityMissing.)
- [x] Decide whether read results should include diagnostics beyond decoded
      rows.

### Phase 3: Follow-On Expansion

- [x] Extend decoded result support beyond node-shaped reads when runtime-table
      joins actually arrive.
      `RunRow`, `StepRow`, `ActionRow` added to `QueryRows`; `read_run`, `read_step`,
      `read_action`, `read_active_runs` added to `ExecutionCoordinator`.
- [ ] Revisit statement caching with real benchmarks instead of speculative
      optimization.
- [x] Align vector capability checks with the setup work in
      [setup-sqlite-vec-capability.md](./setup-sqlite-vec-capability.md).
      `vector_enabled()` on `ExecutionCoordinator` reflects whether sqlite-vec was
      loaded and a profile bootstrapped; `capability_gate_reports_false_without_feature`
      and `capability_gate_reports_true_when_feature_enabled` verify this.
