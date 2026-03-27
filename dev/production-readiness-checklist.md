# Production Readiness Checklist

## Purpose

This document is a strict production-readiness gate for `fathomdb`.

It converts the current repo assessment into explicit readiness areas with a
status for each:

- `done`
- `risky`
- `missing`

An area should not be treated as production-ready unless its status is `done`.

## Status Meanings

- `done`: implemented, verified, and strong enough to support a production
  claim
- `risky`: present but incomplete, weakly validated, or operationally fragile
- `missing`: not yet implemented or not operationalized

## Current Readiness Matrix

| Area | Status | Evidence | Required To Close |
|---|---|---|---|
| Core engine functionality | `done` | Real reads, writes, provenance, FTS, vector support, admin flows, and Python harness round-trips exist in `crates/fathomdb`, `crates/fathomdb-engine`, and `python/examples/harness/` | Keep passing functional test suites across Rust, Go, and Python |
| Rust test coverage for core behavior | `done` | Rust CI runs lint/build/test in `.github/workflows/ci.yml` | Maintain current coverage and keep regressions blocked in CI |
| Go integrity/admin tooling | `done` | `go/fathom-integrity` includes `check`, `recover`, `rebuild`, `trace`, `excise`, `export`, and related tests | Maintain coverage as engine/admin features expand |
| Python bindings exist and work | `done` | `python/fathomdb/` is implemented and the harness exercises current storage/retrieval paths | Keep package behavior verified in automated CI, not only local runs |
| Python CI coverage | `done` | `.github/workflows/python.yml` builds the Python package, runs `python/tests`, and executes the example harness in baseline and vector modes | Keep Python CI green and update it whenever the Python surface expands |
| Vector support in the shipped Python path | `done` | Python build path includes `sqlite-vec`; Python tests and harness cover vector storage/retrieval | Keep vector build/test coverage enforced in CI |
| Feature-complete example harness | `done` | `python/examples/harness/` writes and reads all currently exposed storage forms, including vector mode | Keep harness aligned with every newly exposed write/read/admin surface |
| Recovery path correctness | `risky` | Recovery has been hardened recently, but it already produced multiple high-severity review findings before stabilization | Add more end-to-end corruption/recovery fixtures and prove no canonical-data loss or projection loss across supported failure modes |
| Automated repair coverage for corruption cases | `risky` | `go/fathom-integrity/internal/sqlitecheck/check.go` still says duplicate active logical IDs, broken runtime FK chains, and orphaned chunks require manual investigation | Either add automated repair paths or explicitly narrow the production claim and operator playbooks |
| Safe export / snapshot semantics | `done` | `crates/fathomdb-engine/src/admin.rs` now exports through SQLite backup API and includes a regression test proving WAL-backed committed rows survive export without relying on file copy semantics | Keep WAL-backed export coverage in place and block regressions in CI |
| Release/versioning discipline | `done` | `dev/release-policy.md`, `scripts/check-version-consistency.py`, and `.github/workflows/release.yml` define a unified version/tag contract and release gate | Keep version alignment enforced and publish only from version tags |
| Artifact/release automation | `done` | `.github/workflows/release.yml` now verifies versions, builds Python distributions, publishes to PyPI and crates.io, and creates a GitHub Release | Keep the workflow aligned with the active public artifact contract |
| Documentation accuracy | `done` | `README.md`, `dev/0.1_IMPLEMENTATION_PLAN.md`, and the new release/acceptance docs now describe the shipped Python/vector/telemetry surface and remaining production gates accurately | Treat doc updates as part of every feature completion |
| Operational feedback for long-running work | `done` | Response-cycle feedback now exists across Rust, Python, and Go/CLI | Keep contract coverage in place and extend it when new public operations are added |
| Performance validation / benchmarks | `done` | `crates/fathomdb/benches/production_paths.rs`, `scripts/run-benchmarks.sh`, and `.github/workflows/benchmark-and-robustness.yml` add repeatable benchmark coverage for public engine paths | Keep the benchmark suite representative and review thresholds current |
| Fuzz / adversarial robustness testing | `done` | Go fuzz targets now cover recovered SQL sanitization and bridge response decoding, and the scheduled robustness workflow runs fuzz smoke coverage | Expand fuzz targets as new parser/admin surfaces are added |
| Production acceptance criteria / SLO bar | `done` | `dev/production-acceptance-bar.md` now defines the minimum functional, safety, robustness, and performance gates for a public production claim | Revisit thresholds when workload scale or supported platforms change |
| Process hygiene between code, docs, and trackers | `risky` | Some tracker/docs drift remains, including stale plan/checklist state after implementation work | Keep trackers and docs updated as part of completion criteria for feature work |

## Mandatory Blockers Before A Production Claim

These are the minimum items that should move to `done` before `fathomdb` is
described as production-ready.

1. Python CI coverage
2. Release/versioning discipline
3. Artifact/release automation
4. Documentation accuracy
5. Safe export / snapshot semantics
6. Performance validation / benchmarks
7. Production acceptance criteria / SLO bar

## Strongly Recommended Before Wider Production Use

These items may not block a controlled internal deployment, but they should be
closed before a broad production claim or external user rollout.

1. Recovery path correctness
2. Automated repair coverage for corruption cases
3. Fuzz / adversarial robustness testing
4. Process hygiene between code, docs, and trackers

## Current Overall Assessment

Current assessment: **not yet production-ready for a broad public claim**.

Reason:

- the functional surface is real and increasingly strong
- the Python path exists and works
- the recovery/admin story is meaningful
- but release discipline, Python CI, export hardening, and non-functional
  validation are still below the bar expected for production readiness

## Suggested Next Closure Order

1. Add Python CI, including `sqlite-vec` and harness runs.
2. Update `README.md` and `dev/0.1_IMPLEMENTATION_PLAN.md` so docs match the
   current implementation.
3. Introduce release tags, versioning policy, and release automation.
4. Replace export copy flow with SQLite backup API.
5. Add benchmark/stress coverage and define a measurable production bar.
6. Continue recovery hardening and shrink manual-only repair cases.
