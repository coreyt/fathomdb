# FathomDB Production-Readiness Checklist

_Date:_ 2026-03-28

## Verdict

As of 2026-03-28, `fathomdb` is **not yet production-ready** with the newly implemented Memex-support features.

The planned feature phases are implemented, but the release gate is still open because:

- the full Rust workspace test suite is currently failing
- broader end-to-end verification has not yet been completed for the current tree
- several intentionally deferred items may still matter depending on the production workload

## Release Blockers

### 1. Full Rust workspace must be green

- [ ] Fix the stale `safe_export` schema-version expectations in
  [`crates/fathomdb-engine/src/admin.rs`](/home/coreyt/projects/fathomdb/crates/fathomdb-engine/src/admin.rs)
  Current failing assertions expect schema version `4`, but the live schema is `10`.
- [ ] Fix the stale explain-plan SQL expectation in
  [`crates/fathomdb-engine/src/coordinator.rs`](/home/coreyt/projects/fathomdb/crates/fathomdb-engine/src/coordinator.rs)
  The expected SQL still reflects the pre-Phase-4 query shape and does not account for the `node_access_metadata` join.
- [ ] Run `cargo test --workspace` and require a clean pass.

### 2. Broader verification matrix must pass

- [ ] Run and pass the relevant Rust feature-flag matrix, including `sqlite-vec`.
- [ ] Run and pass the recovery/export/e2e test matrix for the current tree.
- [ ] Run and pass the full Go test suite.
- [ ] Run and pass the full Python test suite against the rebuilt extension module.

## Production Gate Checks

### 3. Verify the implemented feature set as an integrated release

- [ ] Confirm restore/purge lifecycle behavior still passes after the full-matrix rerun.
- [ ] Confirm grouped-query reads remain bounded and stable under the full test matrix.
- [ ] Confirm `last_accessed` metadata remains provenance-compliant, recoverable, and visible through read/admin surfaces.
- [ ] Confirm filtered operational reads work for both newly registered collections and upgraded preexisting collections using the explicit filter-contract update path.

### 4. Confirm recovery and operator tooling readiness

- [ ] Verify `fathom-integrity` still mirrors engine integrity/semantic findings for the new features.
- [ ] Verify safe export, recover, rebuild, trace, excise, restore, purge, and operational lifecycle commands behave correctly on the current schema version.
- [ ] Verify bridge and CLI surfaces preserve all required request semantics, including explicit zero-valued operational range bounds.

## Deferred Items To Review Before Production Sign-Off

These are not automatically blockers for every deployment, but they must be reviewed against the intended production workload.

- [ ] Decide whether optional operational payload schema validation is required for production.
- [ ] Decide whether additional collection-declared secondary indexes are required beyond the current filtered-read support.
- [ ] Decide whether explicit admin-only retention is acceptable, or whether automatic/background retention is required.

If any of the above are required for the intended deployment, promote them from deferred work into implementation before production launch.

## Ready-To-Ship Criteria

`fathomdb` should be treated as production-ready only when all of the following are true:

- [ ] `cargo test --workspace` passes cleanly
- [ ] the broader Rust feature/e2e matrix passes
- [ ] Go and Python suites pass against the current tree
- [ ] no open correctness findings remain for the new features
- [ ] deferred items have been explicitly reviewed and accepted for the target production workload

## Notes

- The planned Memex-support phases are implemented and tracked as complete in
  [`dev/implementation-operational-store-plan.md`](/home/coreyt/projects/fathomdb/dev/implementation-operational-store-plan.md).
- This checklist is a release gate for the current tree, not a statement that the feature roadmap is incomplete.
