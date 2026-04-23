# Design: Typed Vector Regeneration Failures and Audit Completeness

**Status:** Implemented; retained as design rationale
**Last updated:** 2026-04-22

## Purpose

Resolve review finding 1: ensure vector regeneration failures in the
`unsupported vec capability` class reliably produce the new
`vector_regeneration_failed` audit event.

This design also removes the broader fragility that caused the bug: reparsing
formatted `EngineError::Bridge(...)` strings in order to recover failure class
information after the class was already known internally.

## Problem

The current hardening change introduced:

- `VectorRegenerationFailureClass`
- request / failure / apply audit events
- `classify_vector_regeneration_error(...)`

but the implementation still converts some failures into rendered
`EngineError::Bridge` strings before the audit path sees them. In particular,
`map_vector_profile_schema_error(...)` maps `SchemaError::MissingCapability(...)`
into a `Bridge` string, and the later classifier does not recognize that
rendered message as `UnsupportedVecCapability`.

Result:

- the operator still sees an error
- the operation still fails correctly
- but `vector_regeneration_failed` is skipped for that failure class

That makes the new audit lifecycle incomplete and leaves correctness dependent
on formatted message prefixes.

## Goals

1. Preserve failure class and retryability as structured data until the public
   API boundary.
2. Guarantee `vector_regeneration_failed` for every classified regeneration
   failure that occurs after the request event is written, including unsupported
   vec capability.
3. Keep operator-visible bridge / CLI messages unchanged or intentionally
   improved, without making auditing depend on string parsing.
4. Make future failure classes easy to add without duplicating message-prefix
   logic.

## Non-Goals

This design does not:

- redesign the external bridge protocol
- change the public `EngineError` type outside the regeneration path
- add a separate regeneration-history table
- record unbounded diagnostics in provenance

## Current Failure Path

Today the regeneration path mixes two representations:

- structured `VectorRegenerationFailureClass`
- rendered `EngineError::Bridge("vector regeneration ...")`

The audit writer accepts only the structured class, so callers that already
converted to `EngineError` must recover the class by parsing the message text.
That recovery is incomplete and brittle.

## Design

### 1. Introduce a typed internal failure object

Add an internal regeneration-only error type in the admin implementation
(`crates/fathomdb-engine/src/admin/mod.rs`):

```rust
struct VectorRegenerationFailure {
    class: VectorRegenerationFailureClass,
    detail: String,
}
```

Required methods:

- `fn retryable(&self) -> bool`
- `fn to_engine_error(&self) -> EngineError`
- `fn failure_class_label(&self) -> &'static str`

The existing rendered message format stays centralized in
`to_engine_error()`. The rest of the regeneration implementation uses the typed
failure directly.

### 2. Keep helpers typed until the public boundary

Change regeneration-internal helpers to return typed failures instead of
`EngineError` where they are part of classification logic:

- contract validation
- generator executable validation
- bounded generator execution
- generated embedding validation
- snapshot drift
- capability mapping from `SchemaError::MissingCapability`

The public admin entrypoint still returns `Result<..., EngineError>`, but only
after:

- best-effort failure auditing is attempted with the typed class
- the typed failure is rendered once for the outward-facing error

### 3. Remove message-prefix classification from audit decisions

Delete the current `classify_vector_regeneration_error(...)` dependency for the
main regeneration flow. The audit path should receive `&VectorRegenerationFailure`
or `VectorRegenerationFailureClass` directly.

String-prefix parsing may remain only as a compatibility shim if there is some
other existing callsite that still must classify an already-rendered error, but
the regeneration path itself must not depend on that shim.

### 4. Define the audit contract precisely

The lifecycle becomes:

1. validate config and collect chunk snapshot
2. write `vector_regeneration_requested`
3. perform generate / apply flow
4. on any typed classified failure after step 2, write
   `vector_regeneration_failed`
5. on successful apply transaction, write `vector_regeneration_apply`

Explicit rule:

- failures that happen before the request event exists do not require a failure
  event
- failures that happen after the request event exists must attempt a failure
  event with the exact internal class

That means unsupported vec capability during `ensure_vector_profile(...)`
becomes auditable because it happens after the request event.

### 5. Keep metadata bounded and stable

The existing bounded metadata payload remains correct. The only behavioral
change required here is that `failure_class` must always be populated from the
typed class for a failed event, never inferred from message text.

## Implementation Changes

Primary files:

- `crates/fathomdb-engine/src/admin/mod.rs`
- `crates/fathomdb-engine/src/admin/vector.rs`
- `docs/operations/vector-regeneration.md`
- `dev/repair-support-contract.md`

Concrete edits:

- add `VectorRegenerationFailure`
- update regeneration helpers to return typed failures
- change `map_vector_profile_schema_error(...)` to produce typed failure
- change `persist_vector_regeneration_failure_best_effort(...)` to accept typed
  failure or class directly
- remove bridge-message reparsing from the regeneration control flow
- update docs to state that failure events cover unsupported capability after
  the request event is recorded

## Test Plan

Rust tests in `crates/fathomdb-engine/src/admin/mod.rs` and
`crates/fathomdb-engine/src/admin/vector.rs`:

- unsupported vec capability after request audit:
  - simulate `SchemaError::MissingCapability(...)`
  - assert `vector_regeneration_requested` exists
  - assert `vector_regeneration_failed` exists
  - assert failed metadata contains `"failure_class":"unsupported vec capability"`
- snapshot drift still writes failed audit with retryable class
- generator nonzero exit still writes failed audit with the correct class
- invalid contract before request audit:
  - assert no request event
  - assert no failed event
- success path still writes request + apply and no failed event

Regression test:

- no test may depend on parsing the outward-facing `EngineError` string to infer
  the internal failure class

## Acceptance Criteria

- unsupported vec capability produces `vector_regeneration_failed`
- failure auditing no longer depends on formatted bridge-message prefixes
- retryability and failure class come from one typed source of truth
- operator-visible errors remain coherent after the refactor
