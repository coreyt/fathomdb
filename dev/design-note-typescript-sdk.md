# Design Note: First-Class TypeScript SDK

## Decision

Build a first-class TypeScript SDK for `fathomdb` with parity to the existing
Python SDK.

Chosen constraints:

- runtime target: Node.js only
- binding approach: native Node addon via `napi-rs`
- scope: parity with Python
- packaging: in-repo TypeScript package

The TypeScript SDK should follow the same architectural shape as Python:

1. a private native binding layer over the Rust facade
2. a thin ergonomic TypeScript package that is the public application surface

This is a binding and ergonomics layer over the existing Rust facade. It is
not a second engine, and it must not introduce engine behavior that diverges
from Rust or from the Python SDK.

## Goals

1. Provide a first-class Node.js SDK that feels native in TypeScript.
2. Preserve parity with the Python SDK across engine, query, write, admin, and
   recoverability features.
3. Keep the Rust engine as the single source of truth for query compilation,
   write validation, execution semantics, and administrative operations.
4. Keep the public SDK thin enough that Python and TypeScript can remain
   aligned over time.
5. Ship the TypeScript SDK as a normal package within this repository.

## Non-Goals

1. Do not build a TypeScript-native reimplementation of the engine.
2. Do not expose raw Rust internals directly as the public API.
3. Do not introduce a TypeScript-only query language or ORM-like model layer.
4. Do not broaden the runtime target beyond Node.js in v1.
5. Do not let transport or binding mechanics leak into the user-facing API.

## Binding Technology

Use `napi-rs` for the native Node binding.

### Why `napi-rs`

- It is the strongest Rust-first option for a stable Node addon.
- It provides a better TypeScript integration story than lower-level Node-API
  bindings and is a better fit than Neon for a broad SDK surface.
- It allows the binding layer to stay thin while still exposing Rust-owned
  objects like `EngineCore`.
- It rides on Node-API stability, which is the right compatibility boundary
  for distributing prebuilt Node binaries.

### Rejected alternatives

- Neon
  - Viable, but a weaker fit than `napi-rs` for TypeScript ergonomics and
    broad SDK surface maintenance.
- Direct low-level Node-API bindings
  - Maximum control, but unnecessarily expensive and fragile for this SDK.
- FFI or C ABI style bindings
  - Poorer object model and ergonomics than a native addon.
- WASM
  - Not appropriate for the chosen Node-only native SDK shape.

## Architecture

The SDK should mirror the Python layering:

1. Private Rust native core exposed to Node via `napi-rs`
2. Public TypeScript package that provides ergonomic classes, types, builders,
   and error mapping

The native layer exists to bind the Rust facade into Node. The TypeScript
layer exists to make that surface pleasant and idiomatic without changing the
underlying semantics.

## Proposed Layout

```text
/
  crates/
    fathomdb/
      src/
        lib.rs
        node.rs
        node_types.rs
  typescript/
    package.json
    tsconfig.json
    src/
      index.ts
      engine.ts
      query.ts
      admin.ts
      types.ts
      write-builder.ts
      errors.ts
      native.ts
    test/
```

The published package should be named `fathomdb`.

The native addon should remain private, analogous to Python's
`fathomdb._fathomdb`, and should be loaded internally by `src/native.ts`.

## Public TypeScript API

The public surface should match Python conceptually and closely in coverage.

```ts
import {
  AdminClient,
  Engine,
  ProjectionTarget,
  ProvenanceMode,
  Query,
  WriteRequestBuilder,
  newId,
  newRowId,
} from "fathomdb";
```

Primary exports:

- `Engine`
- `Query`
- `AdminClient`
- public types and enums from `types.ts`
- `WriteRequestBuilder`
- error classes
- ID helpers

## Engine API

`Engine` should remain the main application entry point.

```ts
class Engine {
  static open(
    databasePath: string,
    options?: {
      provenanceMode?: ProvenanceMode | "warn" | "require";
      vectorDimension?: number;
      telemetryLevel?: TelemetryLevel | "counters" | "statements" | "profiling";
      progressCallback?: (event: ResponseCycleEvent) => void;
      feedbackConfig?: FeedbackConfig;
    }
  ): Engine;

  readonly admin: AdminClient;

  close(): void;
  telemetrySnapshot(): TelemetrySnapshot;
  nodes(kind: string): Query;
  query(kind: string): Query;
  write(request: WriteRequest, options?: OperationOptions): WriteReceipt;
  submit(request: WriteRequest, options?: OperationOptions): WriteReceipt;
  touchLastAccessed(
    request: LastAccessTouchRequest,
    options?: OperationOptions
  ): LastAccessTouchReport;
}
```

Rules:

- `query(kind)` is an alias for `nodes(kind)`.
- `submit()` is an alias for `write()`.
- `close()` is idempotent.
- operations after close should raise `FathomError`.
- v1 should remain sync-first, matching the current Python and Rust shape.

## Query API

`Query` should be an immutable fluent builder just like the Python SDK.

```ts
const rows = db
  .nodes("Meeting")
  .textSearch("budget", 5)
  .filterJsonTextEq("$.status", "active")
  .limit(10)
  .execute();
```

Methods:

- `vectorSearch(query, limit)`
- `textSearch(query, limit)`
- `traverse({ direction, label, maxDepth })`
- `filterLogicalIdEq(logicalId)`
- `filterKindEq(kind)`
- `filterSourceRefEq(sourceRef)`
- `filterJsonTextEq(path, value)`
- `filterJsonBoolEq(path, value)`
- `filterJsonIntegerGt(path, value)`
- `filterJsonIntegerGte(path, value)`
- `filterJsonIntegerLt(path, value)`
- `filterJsonIntegerLte(path, value)`
- `filterJsonTimestampGt(path, value)`
- `filterJsonTimestampGte(path, value)`
- `filterJsonTimestampLt(path, value)`
- `filterJsonTimestampLte(path, value)`
- `expand({ slot, direction, label, maxDepth })`
- `limit(limit)`
- `compile(options?)`
- `compileGrouped(options?)`
- `explain(options?)`
- `execute(options?)`
- `executeGrouped(options?)`

The TypeScript builder should construct the same AST payload shape the Python
SDK sends today.

## Admin API

`AdminClient` should expose the same functional categories as Python:

- integrity checks
- semantic checks
- projection rebuilds
- trace and excision by source reference
- restore and purge by logical ID
- safe export
- operational collection lifecycle and maintenance APIs

The admin surface should preserve recoverability as a first-class property of
the SDK rather than treating it as a secondary or optional interface.

## Types

The TypeScript SDK should export the same practical set of types as Python.

These include:

- enums such as `ProvenanceMode`, `ChunkPolicy`, `ProjectionTarget`,
  `TraverseDirection`, `DrivingTable`, and `TelemetryLevel`
- query results such as `CompiledQuery`, `CompiledGroupedQuery`, `QueryPlan`,
  `QueryRows`, and `GroupedQueryRows`
- row types such as `NodeRow`, `RunRow`, `StepRow`, and `ActionRow`
- write payloads such as `NodeInsert`, `EdgeInsert`, `ChunkInsert`,
  `VecInsert`, `RunInsert`, `StepInsert`, `ActionInsert`,
  `OptionalProjectionTask`, and `WriteRequest`
- admin and operational report types such as `IntegrityReport`,
  `SemanticReport`, `TraceReport`, `LogicalRestoreReport`,
  `LogicalPurgeReport`, `ProjectionRepairReport`, `SafeExportManifest`,
  `OperationalCollectionRecord`, `OperationalReadReport`, and related
  operational types
- telemetry and feedback types such as `TelemetrySnapshot`,
  `FeedbackConfig`, and `ResponseCycleEvent`

Recommended representation strategy:

- use interfaces and plain objects where behavior is minimal
- use classes only where helper behavior is important
- keep explicit `toWire` and `fromWire` conversion helpers in the TypeScript
  layer

Types that likely deserve class wrappers:

- `OperationalFilterValue`
- `OperationalFilterClause`
- `WriteRequestBuilder`
- any result wrappers where `fromWire` materially improves ergonomics

## Write Builder

Port the Python `WriteRequestBuilder` semantics directly into TypeScript.

Requirements:

- mutable builder that assembles a final `WriteRequest`
- opaque handles for nodes, edges, runs, steps, actions, and chunks
- builder-local ownership checks for handles
- support for optional backfills and operational writes
- `build()` returns the final write request payload

This builder is part of the SDK's first-class ergonomics surface and should
not be omitted from parity.

## Error Mapping

The TypeScript package should define a stable JS error hierarchy:

- `FathomError`
- `DatabaseLockedError`
- `CompileError`
- `InvalidWriteError`
- `WriterRejectedError`
- `SchemaError`
- `SqliteError`
- `IoError`
- `BridgeError`
- `CapabilityMissingError`
- `BuilderValidationError`

The native layer should expose structured error codes. The TypeScript layer
should map those codes into the corresponding error subclasses so application
code does not need to parse raw native failures.

## Native Binding Boundary

The public SDK should be strongly typed and ergonomic. The native boundary
itself should stay deliberately narrow and mostly JSON-based.

This is the key implementation choice that keeps the binding layer thin while
preserving Rust as the source of truth.

Recommended native `EngineCore` methods:

- `open(path, provenanceMode, vectorDimension, telemetryLevel?) -> EngineCore`
- `close()`
- `compileAst(astJson) -> string`
- `compileGroupedAst(astJson) -> string`
- `explainAst(astJson) -> string`
- `executeAst(astJson) -> string`
- `executeGroupedAst(astJson) -> string`
- `submitWrite(requestJson) -> string`
- `touchLastAccessed(requestJson) -> string`
- admin methods returning JSON strings
- `telemetrySnapshot() -> string | object`
- `newId()`
- `newRowId()`

This means:

- Rust performs query compilation, validation, execution, and admin work
- TypeScript handles API shape, type conversion, naming, and error wrapping
- the SDK avoids over-modeling Rust internals at the N-API layer

## Naming and Parity Rules

Parity with Python should be intentional and testable.

Rules:

- preserve the same conceptual surface and feature coverage
- preserve aliases like `nodes()` and `query()`, and `write()` and `submit()`
- preserve the same wire payload shapes
- preserve the same recoverability and admin-first philosophy

TypeScript-specific adjustments are allowed when they clearly improve
ergonomics:

- prefer `camelCase` method and property names
- use string enums or string literal unions where appropriate
- use interface and helper combinations instead of Python dataclasses

These adjustments must not change engine semantics.

## Progress Feedback

The TypeScript SDK should support the same feedback model as Python:

- optional `progressCallback`
- optional `feedbackConfig`
- structured `ResponseCycleEvent` payloads

The public callback contract should be designed up front even if the initial
implementation is conservative around blocking native work. The important
constraint is that the TypeScript surface remains aligned with Python.

## Sync vs Async

Keep the SDK sync-first in v1.

Core methods should remain synchronous:

- `Engine.open()`
- `write()`
- query execution methods
- admin operations

This matches the current engine model and the Python SDK. Async variants can be
added later if needed, but v1 should not force an async-only API.

## Packaging

Use standard `napi-rs` packaging conventions:

- prebuilt binaries for supported platforms
- internal addon loading from the public package
- generated or maintained TypeScript declaration surface
- versioning aligned with the repository release process

The user-facing install should be simple:

```bash
npm install fathomdb
```

## Documentation

Add TypeScript docs that mirror the Python docs structurally:

- getting started
- querying
- writing data
- admin
- engine reference
- query reference
- write-builder reference
- types reference

Examples should stay as close as possible to Python examples so the two SDKs
remain obviously parallel.

## Testing

Parity testing should be explicit.

Required test categories:

- API smoke tests analogous to `python/tests/test_bindings.py`
- write and query round-trip tests
- admin and recoverability tests
- degraded vector behavior tests
- operational collection lifecycle tests
- error mapping tests
- write builder validation tests

Add and maintain a Python parity checklist so TypeScript surface drift is
visible and correctable.

## Summary

The TypeScript SDK should be a first-class sibling to Python:

- Node-only
- native addon via `napi-rs`
- in-repo package
- parity with the Python SDK
- private Rust binding surface
- public ergonomic TypeScript layer
- sync-first API
- JSON-shaped native boundary

This is the right architecture for a durable TypeScript SDK that stays aligned
with the Rust engine and the existing Python SDK.
