# FathomDB Production-Readiness Checklist

_Date:_ 2026-03-28

## Verdict

As of 2026-03-28, the engineering release gate for the newly implemented
Memex-support features is **green**.

The planned feature phases are implemented, the test and recovery matrix is
currently green, and no open correctness findings remain from the validation
pass in this tree.

The formerly deferred operational payload validation, secondary-index, and
retention items are now implemented in the current tree. There are no open
engineering release blockers recorded in this checklist.

## Verification Completed

The following release-gate verification has been run successfully on the
current tree:

- [x] `cargo test --workspace`
- [x] `cargo test --workspace --features sqlite-vec`
- [x] Go full test suite via `go test ./...`
- [x] Go e2e/recovery/export flows within the full Go suite
- [x] Python editable-install build plus `python -m pytest python/tests -q`

## Release Blockers

### 1. Full Rust workspace must be green

- [x] Fix the stale `safe_export` schema-version expectations in
  [`crates/fathomdb-engine/src/admin.rs`](/home/coreyt/projects/fathomdb/crates/fathomdb-engine/src/admin.rs)
  The tests now assert against the live schema version instead of a stale
  hard-coded value.
- [x] Fix the stale explain-plan SQL expectation in
  [`crates/fathomdb-engine/src/coordinator.rs`](/home/coreyt/projects/fathomdb/crates/fathomdb-engine/src/coordinator.rs)
  The test now matches the wrapped row-projection SQL returned by
  `explain_compiled_read()`.
- [x] Make migration-10 bootstrap idempotent when migration history is rebuilt
  against a database that already contains filtered-read schema objects.
- [x] Run `cargo test --workspace` and require a clean pass.

### 2. Broader verification matrix must pass

- [x] Run and pass the relevant Rust feature-flag matrix, including `sqlite-vec`.
- [x] Run and pass the recovery/export/e2e test matrix for the current tree.
- [x] Run and pass the full Go test suite.
- [x] Run and pass the full Python test suite against the rebuilt extension module.

## Production Gate Checks

### 3. Verify the implemented feature set as an integrated release

- [x] Confirm restore/purge lifecycle behavior still passes after the full-matrix rerun.
- [x] Confirm grouped-query reads remain bounded and stable under the full test matrix.
- [x] Confirm `last_accessed` metadata remains provenance-compliant, recoverable, and visible through read/admin surfaces.
- [x] Confirm filtered operational reads work for both newly registered collections and upgraded preexisting collections using the explicit filter-contract update path.

### 4. Confirm recovery and operator tooling readiness

- [x] Verify `fathom-integrity` still mirrors engine integrity/semantic findings for the new features.
- [x] Verify safe export, recover, rebuild, trace, excise, restore, purge, and operational lifecycle commands behave correctly on the current schema version.
- [x] Verify bridge and CLI surfaces preserve all required request semantics, including explicit zero-valued operational range bounds.

## Formerly Deferred Items Now Implemented

- [x] Operational payload schema validation now supports `disabled`,
  `report_only`, and `enforce`, with generic warnings exposed through
  `WriteReceipt.warnings`.
- [x] Additional collection-declared secondary indexes now ship through
  `secondary_indexes_json`, transactional derived-entry maintenance, and
  explicit rebuild support.
- [x] Automatic/background retention now ships through explicit
  `plan_operational_retention` and `run_operational_retention` surfaces plus
  Go operator commands; recurring scheduling remains intentionally external to
  the engine.

## Ready-To-Ship Criteria

`fathomdb` should be treated as production-ready only when all of the following are true:

- [x] `cargo test --workspace` passes cleanly
- [x] the broader Rust feature/e2e matrix passes
- [x] Go and Python suites pass against the current tree
- [x] no open correctness findings remain for the new features
- [x] the formerly deferred operational feature gaps have been implemented in
  the current tree

## Notes

- The planned Memex-support phases are implemented and tracked as complete in
  [`dev/implementation-operational-store-plan.md`](/home/coreyt/projects/fathomdb/dev/implementation-operational-store-plan.md).
- This checklist is a release gate for the current tree, not a statement that
  the feature roadmap is complete in every future direction.
- Automatic retention remains externally scheduled by design; the engine ships
  plan/run primitives rather than an internal scheduler queue.
