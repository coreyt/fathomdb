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
| Recovery path correctness | `done` | Recovery sanitization is now statement-aware, multiline chunk text containing `sql error:` is preserved by tests, recovery bridge restore no longer imposes a fixed timeout ceiling, and recovered vector-capable databases can regain embeddings through the documented regeneration workflow exercised end to end | Keep end-to-end recovery fixtures covering canonical rows, FTS usability, and the documented vector recovery contract |
| Automated repair coverage for corruption cases | `done` | `fathom-integrity repair` now supports `duplicate-active`, `runtime-fk`, and `orphaned-chunks`, `check` points operators to those repair paths, and `dev/repair-support-contract.md` documents the deterministic policy for each class | Keep diagnostics, docs, and operator playbooks aligned when repair support expands |
| Safe export / snapshot semantics | `done` | `crates/fathomdb-engine/src/admin.rs` now exports through SQLite backup API and includes a regression test proving WAL-backed committed rows survive export without relying on file copy semantics | Keep WAL-backed export coverage in place and block regressions in CI |
| Release/versioning discipline | `done` | `dev/release-policy.md`, `scripts/check-version-consistency.py`, and `.github/workflows/release.yml` define a unified version/tag contract and release gate | Keep version alignment enforced and publish only from version tags |
| Artifact/release automation | `done` | `.github/workflows/release.yml` now verifies versions, builds Python distributions, publishes to PyPI and crates.io, and creates a GitHub Release | Keep the workflow aligned with the active public artifact contract |
| Documentation accuracy | `done` | `README.md`, `dev/0.1_IMPLEMENTATION_PLAN.md`, and the new release/acceptance docs now describe the shipped Python/vector/telemetry surface and remaining production gates accurately | Treat doc updates as part of every feature completion |
| Operational feedback for long-running work | `done` | Response-cycle feedback now exists across Rust, Python, and Go/CLI | Keep contract coverage in place and extend it when new public operations are added |
| Performance validation / benchmarks | `done` | `crates/fathomdb/benches/production_paths.rs`, `scripts/run-benchmarks.sh`, and `.github/workflows/benchmark-and-robustness.yml` add repeatable benchmark coverage for public engine paths | Keep the benchmark suite representative and review thresholds current |
| Fuzz / adversarial robustness testing | `done` | Go fuzz targets now cover recovered SQL sanitization and bridge response decoding, and the scheduled robustness workflow runs fuzz smoke coverage | Expand fuzz targets as new parser/admin surfaces are added |
| Production acceptance criteria / SLO bar | `done` | `dev/production-acceptance-bar.md` now defines the minimum functional, safety, robustness, and performance gates for a public production claim | Revisit thresholds when workload scale or supported platforms change |
| Process hygiene between code, docs, and trackers | `done` | `dev/doc-governance.md` defines normative docs and active trackers, `scripts/check-doc-hygiene.py` enforces tracker/checklist alignment, and stale tracker/checklist state has been reconciled | Keep the hygiene check in CI and update tracked docs in the same slice as behavior changes |

## Mandatory Blockers Before A Production Claim

None.

## Strongly Recommended Before Wider Production Use

None.

## Current Overall Assessment

Current assessment: **production-ready within the documented support contract**.

Reason:

- the functional surface is implemented and validated across Rust, Go, and
  Python
- release, CI, benchmark, and robustness gates exist
- the repair and recovery contract is now explicit about what is and is not
  automated in v0.1
- doc and tracker hygiene now have a CI-backed enforcement path

## Suggested Next Closure Order

No remaining `risky` or `missing` areas are open in the readiness matrix.
