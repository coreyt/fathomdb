# Plan: Close Remaining Production Risks

## Purpose

This plan covers only the three areas in
`dev/production-readiness-checklist.md` that remain `risky`:

1. recovery path correctness
2. automated repair coverage for corruption cases
3. process hygiene between code, docs, and trackers

The goal is to move each area from `risky` to `done` with explicit phases and
acceptance criteria.

## Required Development Approach

TDD is required.

Rules:

- write failing tests before code changes
- close one risk at a time; do not mix unrelated fixes into the same slice
- use feature-completeness tests, not just unit coverage
- update the checklist and any affected tracker/docs in the same slice that
  changes behavior

## Phase 1: Recovery Correctness Hardening

### Why first

Recovery correctness is the highest-impact remaining risk because it can cause
data loss or operator mistrust during repair workflows.

### Work items

- Remove line-based `sql error:` stripping from `go/fathom-integrity/internal/commands/recover.go`.
  Recovery filtering must be statement-aware only.
- Remove the fixed 30s timeout around projection-restore bridge commands in
  recovery, or replace it with a configurable/operator-controlled timeout policy
  that does not fail valid large repairs by default.
- Resolve the vector recovery contract:
  - either preserve recovered vector embeddings directly, or
  - change the storage model so embeddings are truly rebuildable from canonical
    state
- Align `dev/ARCHITECTURE.md` and recovery docs with the chosen vector-recovery
  contract.
- Extend recovery fixtures so they include:
  - multiline chunk text containing lines that begin with `sql error:`
  - vector-enabled databases with real vec rows, not only `vector_profiles`
    metadata
  - large-enough recovered datasets to exercise slow projection restore paths

### Required tests

- failing unit test proving multiline recovered text survives sanitization
- failing e2e test proving recovered vector search works after recovery
- failing e2e test proving large projection restore does not fail because of a
  hardcoded timeout
- failing e2e test proving recovery preserves canonical rows when reserved words
  appear inside text or JSON payloads

### Acceptance criteria

- no line-oriented recovery filtering remains
- recovery either preserves vector search usability or the docs explicitly and
  correctly narrow the supported vector recovery contract
- recovery e2e tests prove:
  - canonical rows preserved
  - FTS usable after recovery
  - vector path usable after recovery when supported
  - slow projection restoration does not fail from a fixed timeout ceiling
- `dev/production-readiness-checklist.md` marks `Recovery path correctness`
  as `done`

## Phase 2: Automated Repair Coverage For Corruption Cases

### Why second

This area needs a semantic contract before implementation. The risk is not only
missing code; it is undefined repair behavior.

### Work items

- Write a short design doc for repair semantics covering:
  - duplicate active logical IDs
  - broken runtime FK chains
  - orphaned chunks
- For each corruption class, decide one of:
  - auto-repair supported
  - dry-run diagnosis only
  - permanently manual-only and excluded from the production claim
- If auto-repair is supported, define:
  - what invariant determines the winning row or repair action
  - whether the operation is reversible or only audit-recorded
  - what provenance/audit event is recorded
  - whether repair is done in Rust, orchestrated by Go, or split between them
- Add operator-facing commands or flags for supported auto-repairs.
- Update diagnostics so `check` output distinguishes:
  - auto-repair available
  - dry-run only
  - manual-only

### Required tests

- fixture-driven detect -> diagnose -> repair -> verify tests for every
  supported auto-repair class
- negative tests proving unsupported/manual-only cases do not mutate data
- audit/provenance tests proving repair operations leave an observable trail

### Acceptance criteria

- every currently manual-only corruption class is either:
  - covered by an implemented repair path, or
  - explicitly documented as out of scope for the production claim
- Go diagnostics no longer imply “production-ready” while silently depending on
  ad hoc manual investigation for in-scope corruption classes
- repair commands have end-to-end tests and operator-visible behavior docs
- `dev/production-readiness-checklist.md` marks `Automated repair coverage for
  corruption cases` as `done`

## Phase 3: Process Hygiene Enforcement

### Why third

This risk is lower than data-loss or repair semantics, but it is what keeps the
same drift from reappearing after the first two phases are complete.

### Work items

- Define document roles in a short repo-process note:
  - normative docs
  - active TODO trackers
  - historical design notes
- Reconcile current stale files, at minimum:
  - `dev/TODO-response-cycle-feedback.md`
  - `dev/production-readiness-checklist.md`
- Add a lightweight CI/process check that blocks completion when:
  - a tracked TODO doc for a completed feature still has all boxes unchecked, or
  - a production-readiness summary section contradicts the matrix above it
- Update feature completion expectations so code changes that close checklist
  items must also update:
  - the checklist entry
  - the relevant tracker
  - any public doc whose behavior claim changed

### Required tests or checks

- CI smoke check for the new hygiene rule
- one regression fixture or script test proving the check fails on obviously
  stale tracker/checklist states

### Acceptance criteria

- the current stale tracker/checklist mismatches are resolved
- a repo-level automated check exists and runs in CI
- future feature slices have an enforceable “docs/tracker update required”
  completion rule
- `dev/production-readiness-checklist.md` marks `Process hygiene between code,
  docs, and trackers` as `done`

## Execution Order And Dependencies

1. Phase 1 must start first because vector recovery semantics affect both
   recovery tests and what “repair” can safely claim.
2. Phase 2 depends on explicit repair semantics; do not implement commands
   before that design is written.
3. Phase 3 should begin with cleanup of already-stale docs, but the CI
   enforcement part should land only after Phases 1 and 2 update their trackers,
   so the new rule is proven by real use.

## Definition Of Done

This plan is complete only when:

- all three remaining `risky` areas are marked `done` in
  `dev/production-readiness-checklist.md`
- their acceptance tests/checks exist and pass
- the vector recovery contract is no longer ambiguous between implementation and
  architecture docs
- no known stale tracker or checklist contradiction remains in `dev/`
