# Production Acceptance Bar

## Purpose

Define the minimum validation bar required before `fathomdb` is described as
production-ready for broad public use.

## Required Functional Gates

The following must be green on `main`:

- Rust CI
- Go CI
- Python CI
- Python example harness baseline mode
- Python example harness vector mode
- release version-consistency check

## Required Data-Safety Gates

The following must be proven by automated tests:

- `safe_export` includes committed WAL-backed state
- recovered databases preserve canonical rows
- recovered databases restore FTS usability
- recovered vector-enabled databases preserve vector profile metadata and table
  capability according to `dev/repair-support-contract.md`
- `excise_source` preserves auditability and leaves projections consistent

## Required Performance Review Gates

The benchmark suite must run from `scripts/run-benchmarks.sh` and produce stable
results for review on the reference CI runner.

The current review thresholds for the first public production claim are:

- write submit, single node+chunk: p95 under `100 ms`
- text query execution on seeded benchmark dataset: p95 under `150 ms`
- vector query execution on seeded benchmark dataset: p95 under `200 ms`
- `safe_export` on the seeded benchmark dataset: completes under `500 ms`

These thresholds are review gates, not hidden assumptions. If results exceed
them, the release must either:

1. improve performance to meet the bar, or
2. narrow the public production claim and update docs before release

## Required Robustness Gates

The scheduled robustness workflow must run fuzz smoke coverage for:

- recovered SQL sanitization
- bridge response decoding

## Required Documentation Gates

Before a release:

- `README.md` must describe the current shipped surface
- `dev/0.1_IMPLEMENTATION_PLAN.md` must not describe already-shipped features as
  future work
- `dev/production-readiness-checklist.md` must reflect actual status
- `dev/release-policy.md` must match the active automation
- `dev/repair-support-contract.md` must match the current supported repair and
  recovery contract

## Release Blocking Conditions

Do not make a public production-ready claim if any of the following are true:

- a mandatory checklist blocker is still `missing`
- a public release artifact path is manual-only or undefined
- `safe_export` regresses to file-copy semantics
- Python support exists but is not covered in CI
- benchmark or fuzz workflows are absent or red without an approved waiver
