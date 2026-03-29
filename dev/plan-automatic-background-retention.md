# Plan: Implement Automatic And Background Retention

## Purpose

Implement the retention design in
[`dev/design-automatic-background-retention.md`](/home/coreyt/projects/fathomdb/dev/design-automatic-background-retention.md)
as bounded engine planning/execution primitives with external scheduling.

The implementation goal is operationally reliable retention without adding an
engine-owned scheduler queue or background-thread contract.

## Required Development Approach

TDD is required.

Rules:

- write failing requirement-level tests before each feature slice
- verify retention behavior through observable mutations and reports, not
  helper internals
- keep scheduling out of the engine
- preserve provenance and recoverability in every execution slice
- update operator/docs/tracker surfaces in the same slice that changes the
  contract

## Implementation Scope

This plan covers:

- retention planning via `plan_operational_retention`
- retention execution via `run_operational_retention`
- existing policy modes:
  - `keep_all`
  - `purge_before_seconds`
  - `keep_last`
- dry-run support
- bounded multi-collection execution
- retention reporting metadata and operator tooling

This plan does not cover:

- an in-engine scheduler
- cron parsing or interval management in the engine
- distributed coordination
- retention policies outside documented operational collection semantics

## Slice 1: Policy Parsing And Planning

### Work items

- Parse the current `retention_json` policy modes into a stable internal model.
- Add `plan_operational_retention(now, collections?, max_collections)`.
- Report action kind, candidate deletions, effective cutoff/limit, and last run
  metadata per collection.

### Required tests

- failing test proving `keep_all` plans as no-op
- failing test proving `purge_before_seconds` computes the expected cutoff and
  candidate count
- failing test proving `keep_last` computes the expected candidate set and row
  limit
- failing test proving collection filtering and `max_collections` bounds are
  honored

### Acceptance criteria

- plan output is deterministic for a given `now`
- plan results are bounded and operator-readable

## Slice 2: Execution And Dry-Run Semantics

### Work items

- Add `run_operational_retention(now, collections?, max_collections, dry_run)`.
- Reuse planning semantics rather than inventing a separate execution path.
- Map `purge_before_seconds` and `keep_last` onto existing canonical retention
  behavior.
- Ensure one transaction per collection.

### Required tests

- failing test proving dry-run reports the intended action without mutation
- failing test proving `keep_last` deletes only the oldest excess rows
- failing test proving `purge_before_seconds` deletes only rows older than the
  computed cutoff
- failing test proving `keep_all` remains a no-op
- failing atomicity test proving failure in one collection does not corrupt
  another collection’s retention work

### Acceptance criteria

- dry-run is side-effect free
- non-dry-run execution is per-collection atomic
- repeated runs are idempotent with respect to current row counts and time

## Slice 3: Provenance And Retention Reporting

### Work items

- Emit provenance-visible retention events for non-dry-run execution.
- Add a bounded metadata table for retention run reporting.
- Surface last-run status through admin reporting.

### Required tests

- failing test proving non-dry-run retention writes a provenance event
- failing test proving retention run metadata is stored with the expected
  action kind, deleted count, and rows remaining
- failing test proving dry-run does not create retention-run records

### Acceptance criteria

- retention execution is auditable
- reporting metadata exists without becoming a scheduler queue

## Slice 4: Operator And Cross-Surface Parity

### Work items

- Expose plan/run retention through the Rust facade, Python bindings, bridge,
  and Go operator commands.
- Keep the CLI surface one-shot and explicit.
- Preserve request semantics for optional collection filters and `max_collections`.

### Required tests

- failing Rust facade test for plan/run parity
- failing Python binding test for plan/run retention reports
- failing bridge request-shape tests for plan/run commands
- failing CLI tests proving operator commands forward `now`, `dry_run`,
  collection filters, and limits correctly

### Acceptance criteria

- plan/run retention is available across supported admin surfaces
- operator tooling can invoke retention without manual SQL

## Slice 5: Lifecycle And Recoverability Proof

### Work items

- Ensure retention execution keeps `operational_current`,
  `operational_filter_values`, and secondary-index derived state consistent.
- Prove export/recover/bootstrap compatibility for retention metadata.
- Verify retention does not regress other operational lifecycle flows.

### Required tests

- failing lifecycle test proving retention leaves operational rebuildable state
  consistent
- failing export/recover/bootstrap test proving retention metadata is preserved
  correctly
- failing regression test proving retention does not break broader operational
  admin flows

### Acceptance criteria

- retention remains recoverable and consistent with operational lifecycle
- metadata survives export/recovery without becoming canonical state

## Verification Matrix

The implementation is complete only when all of the following pass:

- targeted Rust admin tests for planning, execution, dry-run, and reporting
- lifecycle tests covering operational consistency after retention
- Rust facade tests
- Python binding tests
- bridge/Go request-shape and CLI tests
- `cargo test --workspace`
- relevant feature matrix such as `cargo test --workspace --features sqlite-vec`
- `go test ./...`
- `python -m pytest python/tests -q`

## Execution Order And Dependencies

1. Land policy parsing and planning first so execution can reuse one contract.
2. Land dry-run and actual execution before adding reporting metadata.
3. Add provenance/reporting before operator tooling so CLI users receive
   meaningful output.
4. Finish cross-language parity and lifecycle proof before calling the slice
   complete.

## Definition Of Done

This plan is complete only when:

- planning and execution surfaces exist and match the documented policy modes
- dry-run and non-dry-run behavior are both proven
- retention execution is auditable and bounded
- Rust, Python, bridge, and Go operator surfaces expose plan/run behavior
- recurring scheduling remains outside the engine by design
- full verification remains green
