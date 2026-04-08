# TypeScript SDK Specification

## Purpose

Define the exact repository shape, package boundaries, public API, test
strategy, and implementation sequence for a first-class TypeScript SDK for
`fathomdb`.

This spec refines the decisions recorded in
[`design-note-typescript-sdk.md`](./design-note-typescript-sdk.md) into a
concrete implementation target.

## Fixed Decisions

These decisions are already made and should be treated as constraints:

- runtime target: Node.js only
- native binding technology: `napi-rs`
- SDK scope: parity with the Python SDK
- repository placement: in-repo TypeScript package
- implementation discipline: TDD

Additional planning constraint:

- the repository must also contain a separate TypeScript application used to
  exercise the SDK end to end, analogous in purpose to the existing Python
  harness

## Repository Placement

The TypeScript work should live in a dedicated top-level `typescript/`
directory rather than being mixed into `python/`, `crates/`, or `scripts/`.

Use this layout:

```text
typescript/
  packages/
    fathomdb/
      package.json
      tsconfig.json
      src/
        index.ts
        native.ts
        engine.ts
        query.ts
        admin.ts
        types.ts
        write-builder.ts
        errors.ts
      test/
  apps/
    sdk-harness/
      package.json
      tsconfig.json
      src/
        app.ts
        engine-factory.ts
        models.ts
        verify.ts
        scenarios/
          canonical.ts
          graph.ts
          runtime.ts
          recovery.ts
          vector.ts
      test/
```

Rationale:

- `typescript/packages/fathomdb` is the actual SDK package
- `typescript/apps/sdk-harness` is a separate application that consumes the SDK
  as a client would
- `packages/` versus `apps/` keeps library code and consumer code cleanly
  separated
- this gives room for future TypeScript tools without scattering files across
  the repo

## Rust Binding Placement

The Rust binding implementation should remain inside the existing
`crates/fathomdb` crate because the TypeScript SDK is a binding over the
existing public Rust facade, not a new engine crate.

Planned Rust-side files:

```text
crates/fathomdb/src/
  node.rs
  node_types.rs
```

Expected roles:

- `node.rs`
  - `napi-rs` exported classes and functions
  - `EngineCore`
  - native entry points for query, write, admin, telemetry, and ID helpers
- `node_types.rs`
  - Node-visible wire adapters and structured error code helpers

`lib.rs` should gate the Node bindings behind a dedicated feature so the Rust
facade remains cleanly separable from the Node build.

## Package Boundary

The native addon is private implementation detail. The public SDK is the
TypeScript package.

Public install target:

```bash
npm install fathomdb
```

Public import target:

```ts
import { Engine } from "fathomdb";
```

The addon itself should be loaded only through `src/native.ts`.

This is the same boundary strategy used by the Python SDK:

- native core stays private
- public language package owns ergonomics

## SDK Public API

The SDK should export:

- `Engine`
- `Query`
- `AdminClient`
- `WriteRequestBuilder`
- all stable public enums and data types needed by callers
- error classes
- `newId`
- `newRowId`

Top-level usage target:

```ts
import {
  ChunkInsert,
  ChunkPolicy,
  Engine,
  NodeInsert,
  WriteRequest,
  newRowId,
} from "fathomdb";

const db = Engine.open("agent.db");

const receipt = db.write(
  new WriteRequest({
    label: "meeting-ingest",
    nodes: [
      new NodeInsert({
        rowId: newRowId(),
        logicalId: "meeting:budget-2026-03-25",
        kind: "Meeting",
        properties: { title: "Budget review", status: "active" },
        sourceRef: "action:meeting-import",
        upsert: true,
        chunkPolicy: ChunkPolicy.REPLACE,
      }),
    ],
    chunks: [
      new ChunkInsert({
        id: "chunk:meeting:budget-2026-03-25:0",
        nodeLogicalId: "meeting:budget-2026-03-25",
        textContent: "Budget discussion and action items",
      }),
    ],
  })
);
```

The precise type representation may use plain object factories rather than
`new` for every type, but the public surface must remain explicit, typed, and
stable.

## Naming Rule

The TypeScript SDK should use idiomatic `camelCase` names for methods and
properties, while preserving Python parity semantically.

Examples:

- Python `new_row_id` becomes TypeScript `newRowId`
- Python `filter_json_text_eq` becomes TypeScript `filterJsonTextEq`
- Python `touch_last_accessed` becomes TypeScript `touchLastAccessed`

Parity requirement:

- method categories, aliases, payload meaning, and result behavior must stay
  aligned with Python even when names are adjusted for TypeScript style

## Engine Spec

`Engine` is the main SDK entry point.

Required API:

```ts
type OperationOptions = {
  progressCallback?: (event: ResponseCycleEvent) => void;
  feedbackConfig?: FeedbackConfig;
};

type EngineOpenOptions = {
  provenanceMode?: ProvenanceMode | "warn" | "require";
  vectorDimension?: number;
  telemetryLevel?: TelemetryLevel | "counters" | "statements" | "profiling";
  progressCallback?: (event: ResponseCycleEvent) => void;
  feedbackConfig?: FeedbackConfig;
};

class Engine {
  static open(databasePath: string, options?: EngineOpenOptions): Engine;

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

Behavioral requirements:

- `query(kind)` is an alias for `nodes(kind)`
- `submit()` is an alias for `write()`
- `close()` is idempotent
- operations after close raise `FathomError`
- one engine per database path remains the rule, with lock failures reported as
  `DatabaseLockedError`

## Query Spec

`Query` must be immutable.

Every mutating method returns a new `Query` instance. The underlying AST shape
should remain aligned with the Python SDK.

Required methods:

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

## Admin Spec

`AdminClient` must expose parity with the Python admin surface.

Required categories:

- `checkIntegrity`
- `checkSemantics`
- projection rebuilds
- trace and excision by source reference
- restore and purge by logical ID
- safe export
- operational collection registration, description, trace, rebuild, compaction,
  purge, disable, read, validation, filter, and index maintenance APIs

The TypeScript SDK is not complete if it only covers query and write paths.
Recoverability and operational/admin APIs are part of the required parity bar.

## Types Spec

`types.ts` should define and export the stable public data model for callers.

Expected type groups:

- enums
- wire conversion helpers
- write payload types
- query result types
- admin report types
- operational collection types
- telemetry and feedback types

Recommended style:

- use plain TypeScript types/interfaces for simple shapes
- use classes only when behavior is needed
- centralize JSON conversion helpers near the type definitions

Required enum coverage includes:

- `ProvenanceMode`
- `ChunkPolicy`
- `ProjectionTarget`
- `OperationalCollectionKind`
- `OperationalFilterMode`
- `OperationalFilterFieldType`
- `TraverseDirection`
- `DrivingTable`
- `TelemetryLevel`
- `ResponseCyclePhase`

Required special helper types include:

- `RawJson`
- `OperationalFilterValue`
- `OperationalFilterClause`
- `FeedbackConfig`
- `ResponseCycleEvent`

## Write Builder Spec

`write-builder.ts` must port the Python `WriteRequestBuilder`.

Required capabilities:

- add node
- retire node
- add edge
- retire edge
- add chunk
- add run
- add step
- add action
- add vec insert
- add optional backfill
- add operational writes
- build final request

Required handle types:

- `NodeHandle`
- `EdgeHandle`
- `RunHandle`
- `StepHandle`
- `ActionHandle`
- `ChunkHandle`

Required validation behavior:

- builder-local handle ownership enforcement
- early validation of invalid references
- `BuilderValidationError` on misuse

## Error Spec

`errors.ts` must export:

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

The native layer must expose structured error codes so the TypeScript layer can
 produce stable JS error subclasses.

Error mapping is part of the SDK contract, not a later cleanup task.

## Native Boundary Spec

Even though this is a native addon, the binding boundary should stay narrow and
mostly JSON-oriented.

Required native methods:

- `open(path, provenanceMode, vectorDimension, telemetryLevel?)`
- `close()`
- `compileAst(astJson)`
- `compileGroupedAst(astJson)`
- `explainAst(astJson)`
- `executeAst(astJson)`
- `executeGroupedAst(astJson)`
- `submitWrite(requestJson)`
- `touchLastAccessed(requestJson)`
- admin methods returning JSON payloads
- `telemetrySnapshot()`
- `newId()`
- `newRowId()`

Rationale:

- avoids re-modeling the full engine type system in N-API
- keeps Rust authoritative for execution behavior
- keeps the TS layer focused on language ergonomics and type conversion

## SDK Harness App Spec

The repository must include a separate TypeScript application that uses the SDK
as a consumer would.

This app belongs in:

```text
typescript/apps/sdk-harness/
```

Purpose:

- exercise the SDK end to end
- prove that the public TypeScript API is sufficient for realistic usage
- mirror the role of `python/examples/harness`
- catch integration gaps that unit tests on the package alone will miss

The harness should be scenario-driven and should organize work similarly to the
Python harness.

Suggested files:

- `app.ts`
- `engine-factory.ts`
- `models.ts`
- `verify.ts`
- `scenarios/canonical.ts`
- `scenarios/graph.ts`
- `scenarios/runtime.ts`
- `scenarios/recovery.ts`
- `scenarios/vector.ts`

Required harness modes:

- `baseline`
- `vector`

Required scenario categories:

- canonical node + chunk + FTS
- node upsert / supersession
- graph edge + traversal
- edge retire
- runtime writes
- provenance warn / require
- trace and excise
- safe export
- projection rebuild
- vector degradation in baseline mode
- vector insert and search in vector mode

The harness is application code, not package-internal test scaffolding. Keep
it separate from SDK internals.

## TDD Rule

All TypeScript SDK work must follow TDD.

Required order of work:

1. write or extend a failing test
2. implement the smallest change that makes it pass
3. refactor while keeping tests green

This applies to:

- SDK unit tests
- SDK integration tests
- harness scenarios where practical
- Rust Node binding additions

When a feature spans Rust binding code and TypeScript wrapper code, the first
test should be written at the highest practical consumer layer, then driven
downward into the binding as needed.

## Test Layout

Use distinct test layers.

### SDK package tests

Location:

```text
typescript/packages/fathomdb/test/
```

Purpose:

- public API smoke tests
- type conversion tests
- query builder AST tests
- write builder validation tests
- error mapping tests

### SDK integration tests

Location:

```text
typescript/packages/fathomdb/test/integration/
```

Purpose:

- open database
- write/query round trips
- admin/report operations
- vector degradation behavior
- operational collection lifecycle

### Harness app tests

Location:

```text
typescript/apps/sdk-harness/test/
```

Purpose:

- verify harness orchestration
- verify scenario coverage
- run end-to-end consumer-style workflows

## Parity Checklist

A parity checklist must be maintained between Python and TypeScript exports.

The checklist should cover:

- engine methods
- query builder methods
- admin methods
- top-level exports
- core enums
- write payload coverage
- admin report coverage
- operational API coverage
- builder capabilities
- documented examples

If Python adds a public SDK feature, TypeScript parity should be evaluated
explicitly rather than left implicit.

## Documentation Rule

As the SDK is implemented, add TypeScript docs in a structure parallel to the
Python docs.

Expected topics:

- getting started
- querying
- writing data
- admin
- engine reference
- query reference
- write builder reference
- types reference

The docs should prefer near-parallel examples between Python and TypeScript.

## Implementation Sequence

The implementation should proceed in this order:

1. create `typescript/` workspace structure
2. add package/test runner/tooling skeleton
3. add failing package smoke tests
4. add native addon loading path
5. add minimal `Engine.open()` and ID helper support through `napi-rs`
6. add failing write/query round-trip tests
7. implement core query and write paths
8. add failing admin tests
9. implement admin and recoverability paths
10. add failing write-builder tests
11. implement `WriteRequestBuilder`
12. add failing harness scenarios
13. implement the TypeScript harness app
14. add parity checklist and docs updates

This sequence keeps TDD visible and prevents the harness from being deferred
until after the SDK shape is already ossified.

## Acceptance Criteria

The TypeScript SDK is ready for initial use when all of the following are true:

- `typescript/packages/fathomdb` exposes a stable public package
- the SDK covers engine, query, write, admin, telemetry, and builder surfaces
- the SDK maps native errors into stable JS error classes
- package integration tests pass
- the separate harness app runs against the SDK and passes its scenarios
- the TypeScript surface has explicit parity coverage against the Python SDK

## Summary

The TypeScript SDK should be built as a proper in-repo language surface with:

- an organized `typescript/` top-level area
- a dedicated SDK package under `typescript/packages/fathomdb`
- a separate consumer application under `typescript/apps/sdk-harness`
- Rust `napi-rs` bindings kept inside `crates/fathomdb`
- strict TDD across package, binding, and harness work

This is the repository shape and implementation discipline the project should
use going forward.
