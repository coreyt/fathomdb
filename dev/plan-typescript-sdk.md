# Plan: Implement the TypeScript SDK and Harness with TDD

> **Status: COMPLETED** (2026-04-08). All phases implemented and verified.
> See commits `a8b8f30`, `c3fb834`, `e2ecb10` on `main`.

## Summary

Build a new top-level `typescript/` workspace that contains:

- a publishable SDK package at `typescript/packages/fathomdb`
- a consumer-style harness app at `typescript/apps/sdk-harness`
- shared TypeScript workspace tooling at `typescript/package.json` and `typescript/tsconfig.base.json`

Keep Rust as the single source of truth. The TypeScript layer is a thin ergonomic wrapper over new `napi-rs` bindings inside `crates/fathomdb`, with JSON-oriented native methods that mirror the current Python `EngineCore` surface.

Use TDD throughout: every slice starts with a failing consumer-facing TypeScript test where practical, then the minimal TS wrapper change, then the minimal Rust binding change.

## Implementation Changes

### 1. Workspace and toolchain defaults

- Use `npm` workspaces, not `pnpm`.
- Put workspace metadata in `typescript/package.json`.
- Commit `package-lock.json` under `typescript/`.
- Use `typescript/tsconfig.base.json` plus per-package `tsconfig.json`.
- Use `vitest` for SDK and harness tests.
- Use `tsup` for building the public SDK package to `dist/`.
- Use `@napi-rs/cli` for addon packaging/build integration.
- Keep the public package name `fathomdb`.
- Keep the addon private to the package and load it only through `src/native.ts`.

### 2. Rust binding layer in `crates/fathomdb`

- Add a new `node` feature to `crates/fathomdb/Cargo.toml` alongside `python`, keeping the two bindings independent.
- Implement `crates/fathomdb/src/node.rs` and `crates/fathomdb/src/node_types.rs`.
- Export a sync `EngineCore` class plus top-level `newId()` and `newRowId()` helpers.
- Expose narrow JSON/string methods for engine open/close, query compile/explain/execute, write submit, `touchLastAccessed`, telemetry, and admin work.
- Add structured native error codes and map them in TS to stable SDK error subclasses.

### 3. TypeScript SDK package surface

- `index.ts`: stable top-level exports.
- `native.ts`: private addon loading and raw native interface typing.
- `engine.ts`: `Engine.open`, `close`, `telemetrySnapshot`, `nodes/query`, `write/submit`, `touchLastAccessed`.
- `query.ts`: immutable fluent `Query` builder with Python-equivalent AST payload shape and camelCase methods.
- `admin.ts`: `AdminClient`.
- `types.ts`: exported enums, interfaces, and wire adapters.
- `write-builder.ts`: `WriteRequestBuilder` plus handle types and builder-local validation.
- `errors.ts`: stable error hierarchy and native-code-to-error mapping.

### 4. Harness app and docs/parity assets

- Create `typescript/apps/sdk-harness/` as application code, not test scaffolding.
- Add scenario modules for `canonical`, `graph`, `runtime`, `recovery`, and `vector`.
- Add `dev/typescript-sdk-parity-checklist.md` and `docs/typescript/` as the SDK matures.

### 5. TDD execution sequence

1. Add workspace skeleton, test runner, typecheck, and empty package smoke tests.
2. Add addon loading tests and minimal Rust `node` feature/build plumbing.
3. Add failing tests for `Engine.open`, `close`, `newId`, `newRowId`, and telemetry snapshot.
4. Add failing query AST-builder tests, then implement immutable `Query`.
5. Add failing integration tests for write/query round trips, then implement `write`, `submit`, and query execution methods.
6. Add failing tests for progress callback and feedback config plumbing.
7. Add failing admin API tests by category, then implement admin wrappers and native bindings incrementally.
8. Add failing `WriteRequestBuilder` validation/handle tests, then port the Python builder semantics.
9. Add failing harness scenario tests, then implement the harness app and verification helpers.
10. Add parity checklist assertions and docs updates after the API surface is stable.

## Test Plan

- `typescript/packages/fathomdb/test/` for exports, native loading, AST shape, wire conversion, error mapping, and builder validation.
- `typescript/packages/fathomdb/test/integration/` for engine open/close, write/query round trips, grouped queries, telemetry, admin/report operations, restore/purge, safe export, operational flows, vector degradation, vector search, and lock failures.
- `typescript/apps/sdk-harness/test/` for orchestration and end-to-end consumer-style workflows.
- Add a dedicated TypeScript CI workflow parallel to Python.

## Assumptions

- `npm` is the workspace/package-manager default because the repo has no existing JS toolchain and the desired install target is `npm install fathomdb`.
- v1 remains sync-first across the TypeScript SDK.
- The Python SDK is the parity source for public surface coverage.
