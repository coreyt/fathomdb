# Plan: Implement Operational Secondary Indexes

## Purpose

Implement the secondary-index design in
[`dev/design-operational-secondary-indexes.md`](/home/coreyt/projects/fathomdb/dev/design-operational-secondary-indexes.md)
as a bounded, recoverable, collection-declared access-path system for
operational collections.

The implementation goal is stronger operational read support without exposing
arbitrary SQL indexing to callers.

## Required Development Approach

TDD is required.

Rules:

- write failing requirement-level tests before each feature slice
- test query behavior, rebuildability, and lifecycle consistency rather than
  private helper structure
- keep index definitions explicit and collection-declared
- maintain canonical/derived separation: indexes are rebuildable derived state
- update docs and trackers in the same slice that changes the public contract

## Implementation Scope

This plan covers:

- `secondary_indexes_json` on operational collections
- three bounded index kinds:
  - `append_only_field_time`
  - `latest_state_field`
  - `latest_state_composite`
- engine-managed derived entries
- explicit rebuild support
- deterministic read-path selection where implemented

This plan does not cover:

- arbitrary SQL DDL from callers
- unlimited composite indexes
- cost-based planning
- user-defined expressions over payload JSON

## Slice 1: Contract Persistence And Validation

### Work items

- Add `secondary_indexes_json` to `operational_collections`.
- Extend collection register/read/update models to include the new contract.
- Parse and validate the declared index definitions per collection kind.
- Treat empty string or `[]` deterministically according to the chosen storage
  contract.

### Required tests

- failing test proving collection register/read/update round-trips
  `secondary_indexes_json`
- failing tests rejecting malformed JSON, duplicate index names, unsupported
  kinds, invalid field/value-type combinations, and out-of-bounds composite
  lengths
- failing export/bootstrap/recovery test proving the contract survives intact

### Acceptance criteria

- index contracts are durable collection metadata
- malformed contracts fail before metadata mutation
- export/recovery/bootstrap preserve the contract

## Slice 2: Derived Storage Shape

### Work items

- Add a stable derived table for secondary-index entries.
- Add bounded supporting SQLite indexes over that derived table.
- Define entry shapes for append-only mutation rows and latest-state current
  rows.

### Required tests

- failing schema/bootstrap tests proving the derived table and support indexes
  exist
- failing rebuild/bootstrap tests proving migration/recovery is idempotent

### Acceptance criteria

- the schema surface is stable and bounded
- derived entries are rebuildable from canonical operational state

## Slice 3: Transactional Write Maintenance

### Work items

- Maintain append-only secondary-index entries on `Append`.
- Maintain latest-state secondary-index entries on `Put` and `Delete`.
- Ensure disable/repair/current-row-clear flows keep derived entries in sync.

### Required tests

- failing append-only maintenance test proving entries are created with the
  correct field and sort values
- failing latest-state maintenance test across `Put`
- failing latest-state maintenance test across `Delete`
- failing atomicity test proving derived entries roll back with canonical
  writes

### Acceptance criteria

- write-time derived maintenance is transactional
- latest-state derived entries track current-row changes correctly

## Slice 4: Read-Path Selection

### Work items

- Add deterministic planner rules that choose a declared index only when the
  request shape matches it.
- Fall back to `filter_fields_json` read support when no secondary index
  matches.
- Keep unsupported request shapes explicit and bounded.

### Required tests

- failing test proving append-only reads use `append_only_field_time` when the
  request shape matches
- failing test proving the read path falls back cleanly when no secondary
  index matches
- failing negative test proving unsupported collection kinds or request shapes
  do not silently use arbitrary SQL

### Acceptance criteria

- index selection is deterministic and rule-based
- reads remain bounded
- fallback behavior is explicit and safe

## Slice 5: Rebuild And Lifecycle Consistency

### Work items

- Add `rebuild_operational_secondary_indexes(collection)`.
- Ensure compact/purge/excise/recovery/current rebuild flows keep derived
  entries consistent.
- Surface rebuild results through admin responses.

### Required tests

- failing rebuild test proving entries can be reconstructed from canonical
  state
- failing lifecycle tests covering compact/purge/excise consistency
- failing recovery/bootstrap test proving rebuild remains available after
  restore/recover flows

### Acceptance criteria

- derived drift is repairable without manual SQL
- lifecycle operations do not leave stale secondary-index entries behind

## Slice 6: Cross-Surface Parity

### Work items

- Expose `secondary_indexes_json`, update, and rebuild through the Rust facade,
  Python bindings, bridge, and Go tooling.
- Preserve request semantics for clearing/replacing contracts.

### Required tests

- failing Rust facade test for update/rebuild
- failing Python binding test for contract management and rebuild reporting
- failing bridge/Go request-shape tests, including empty-contract semantics
- failing CLI test proving operator commands forward the contract correctly

### Acceptance criteria

- all supported admin surfaces can manage and rebuild secondary indexes
- no surface becomes one-way or lossy for contract updates

## Verification Matrix

The implementation is complete only when all of the following pass:

- targeted Rust schema/admin/writer tests for contract, maintenance, rebuild,
  and read-path behavior
- lifecycle tests covering compact/purge/excise/recovery consistency
- Rust facade tests
- Python binding tests
- bridge/Go request-shape and CLI tests
- `cargo test --workspace`
- relevant feature matrix such as `cargo test --workspace --features sqlite-vec`
- `go test ./...`
- `python -m pytest python/tests -q`

## Execution Order And Dependencies

1. Persist and validate the contract first.
2. Land the derived storage table before write maintenance.
3. Prove write maintenance before introducing planner usage.
4. Add rebuild support before expanding operator-facing admin surfaces.
5. Finish cross-language parity and full-matrix verification last.

## Definition Of Done

This plan is complete only when:

- `secondary_indexes_json` is durable collection metadata
- supported index kinds are validated and maintained transactionally
- derived entries can be rebuilt deterministically
- read paths use declared indexes where supported and fall back cleanly
- Rust, Python, bridge, and Go surfaces expose update/rebuild behavior
- full verification remains green
