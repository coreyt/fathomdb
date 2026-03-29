# Plan: Implement Operational Payload Schema Validation

## Purpose

Implement the payload-validation design in
[`dev/design-operational-payload-schema-validation.md`](/home/coreyt/projects/fathomdb/dev/design-operational-payload-schema-validation.md)
as a bounded, opt-in operational-collection contract.

The implementation goal is not a generic schema engine. It is a narrow,
recoverable storage-boundary validation surface for operational payloads.

## Required Development Approach

TDD is required.

Rules:

- write failing requirement-level tests before each code slice
- verify externally visible behavior and storage invariants, not helper
  function structure
- keep `schema_json` documentation-only throughout the rollout
- preserve export, recovery, bootstrap, and cross-language compatibility in
  the same slice that changes behavior
- update the design, tracker, and readiness docs when the shipped behavior
  changes

## Implementation Scope

This plan covers:

- `validation_json` as a dedicated collection metadata field
- validation modes: `disabled`, `report_only`, `enforce`
- top-level field validation only
- history-validation diagnostics for preexisting rows
- Rust, Python, bridge, and Go surface parity where collection metadata or
  write receipts are exposed

This plan does not cover:

- full JSON Schema compatibility
- nested object-schema recursion
- cross-row validation
- query-time schema inference

## Slice 1: Contract Persistence And Shape Validation

### Work items

- Add `validation_json` to `operational_collections`.
- Extend operational collection register/read/update models to include
  `validation_json`.
- Parse and validate the contract shape at registration/update time.
- Treat empty string as “no validation contract configured”.

### Required tests

- failing test proving newly registered collections round-trip
  `validation_json`
- failing test proving update round-trips a valid contract
- failing tests rejecting malformed JSON, unsupported `format_version`,
  duplicate field names, invalid constraint combinations, and unsupported
  field types
- failing export/bootstrap/recovery test proving the contract survives the
  lifecycle intact

### Acceptance criteria

- collection metadata preserves `validation_json` exactly
- malformed contracts fail before any collection metadata is mutated
- export/recover/bootstrap preserve the contract without reinterpretation

## Slice 2: Enforced Write-Time Validation

### Work items

- Validate payload-bearing operational writes before any mutation/current/derived
  rows are written.
- Apply validation to `Append` and `Put`.
- Exempt `Delete`.
- Return deterministic `InvalidWrite` failures for `enforce`.

### Required tests

- failing test proving `disabled` accepts payloads that would fail validation
- failing test proving `enforce` rejects invalid `Append`
- failing test proving `enforce` rejects invalid `Put`
- failing test proving `Delete` bypasses payload validation
- failing atomicity test proving rejected writes leave no mutation row, current
  row, or derived side effect

### Acceptance criteria

- `enforce` failures are collection/field/rule specific
- invalid writes do not partially mutate operational state
- `disabled` preserves current behavior

## Slice 3: Staged Rollout Support Through `report_only`

### Work items

- Add a generic write-warning surface to the write receipt.
- Route validation warnings through that generic warning channel.
- Accept invalid writes in `report_only` while emitting the same message that
  `enforce` would reject on.
- Preserve existing provenance-warning behavior alongside generic warnings.

### Required tests

- failing test proving `report_only` accepts an otherwise invalid write
- failing test proving the write receipt includes exactly the validation
  warning and no spurious provenance warning
- failing Python/bridge/Go surface tests proving warnings round-trip across the
  public bindings

### Acceptance criteria

- `report_only` is transport-stable across Rust, Python, and bridge surfaces
- callers can distinguish generic warnings from `provenance_warnings`
- no write-side schema behavior is hidden behind `schema_json`

## Slice 4: History Diagnostics

### Work items

- Add `validate_operational_collection_history(collection)`.
- Scan existing mutations for contract compatibility without mutating data.
- Return row-level issue reports with mutation ID, record key, op kind, and
  message.

### Required tests

- failing test proving incompatible historical rows are reported
- failing test proving history validation is read-only
- failing test proving collections with no configured contract fail clearly or
  report deterministically, according to the chosen API contract

### Acceptance criteria

- operators can assess preexisting data before switching to `enforce`
- history diagnostics never rewrite or delete operational history

## Slice 5: Cross-Surface Parity And Lifecycle Proof

### Work items

- Expose `validation_json` and history-validation surfaces through the Rust
  facade, Python bindings, bridge, and Go tooling where operational collection
  admin is already surfaced.
- Ensure safe export / recover / bootstrap remain compatible.

### Required tests

- failing Rust facade test for validation update/history validation
- failing Python binding test for validation contract management
- failing bridge/Go request-shape tests for update/read behavior
- failing workspace regression test proving validation does not break existing
  operational lifecycle flows

### Acceptance criteria

- no supported language surface loses access to the validation contract
- lifecycle flows remain green under full verification

## Verification Matrix

The implementation is complete only when all of the following pass:

- targeted Rust engine/admin tests for contract parsing and write behavior
- lifecycle tests proving atomicity and export/recover/bootstrap compatibility
- Rust facade tests
- Python binding tests
- bridge/Go request-shape and CLI tests
- `cargo test --workspace`
- relevant feature-flag matrix such as `cargo test --workspace --features sqlite-vec`
- `go test ./...`
- `python -m pytest python/tests -q`

## Execution Order And Dependencies

1. Ship contract persistence and shape validation first so later write logic
   has a stable metadata source.
2. Ship `enforce` before `report_only`; the warning mode should reuse the same
   validation semantics as hard rejection.
3. Add history diagnostics before recommending staged rollout in docs.
4. Finish cross-language parity and lifecycle proof before calling the slice
   complete.

## Definition Of Done

This plan is complete only when:

- `validation_json` is durable collection metadata
- `disabled`, `report_only`, and `enforce` all behave as documented
- invalid enforced writes are atomic
- history diagnostics are available and read-only
- Rust, Python, bridge, and Go surfaces expose the required contract
- full verification remains green
