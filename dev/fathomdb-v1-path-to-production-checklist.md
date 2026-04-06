# FathomDB v0.1 Path to Production Checklist

_Last updated:_ 2026-03-30

## Purpose

Single production-readiness gate for fathomdb v0.1. Replaces the previous
split across `production-readiness-checklist.md`,
`fathomdb-production-readiness-2026-03-28.md`, and
`plan-close-remaining-production-risks.md`.

Status meanings:

- `done` — implemented, verified, strong enough for a production claim
- `risky` — present but incomplete, weakly validated, or operationally fragile
- `missing` — not yet implemented or operationalized

## Current Readiness Matrix

| Area | Status | Evidence | Required To Close |
|---|---|---|---|
| Core engine functionality | `done` | Real reads, writes, provenance, FTS, vector support, admin flows, and Python harness round-trips across `crates/fathomdb`, `crates/fathomdb-engine`, and `python/examples/harness/` | Keep passing functional test suites across Rust, Go, and Python |
| Rust test coverage | `done` | 380 Rust tests; CI runs lint/build/test including tracing and Windows targets in `.github/workflows/ci.yml` | Maintain coverage; keep regressions blocked in CI |
| Go integrity/admin tooling | `done` | 144 Go tests; `go/fathom-integrity` covers check, recover, rebuild, trace, excise, export with e2e and Windows CI | Maintain coverage as engine/admin features expand |
| Python bindings | `done` | 48 Python tests; exclusive file lock, GC-safe Drop, context manager, idempotent close(); harness exercises all storage/retrieval paths | Keep package behavior verified in CI |
| Python CI coverage | `done` | `.github/workflows/python.yml` builds, runs tests, and executes the example harness in baseline and vector modes | Keep Python CI green when the Python surface expands |
| Vector support in Python | `done` | Python build includes sqlite-vec; Python tests and harness cover vector storage/retrieval | Keep vector build/test coverage enforced in CI |
| Recovery path correctness | `done` | Statement-aware sanitization, multiline chunk preservation, no fixed timeout ceiling, documented vector regeneration workflow with e2e coverage | Keep recovery fixtures covering canonical rows, FTS usability, and vector contract |
| Automated repair | `done` | `fathom-integrity repair` supports duplicate-active, runtime-fk, orphaned-chunks; `check` points to repair paths; `dev/repair-support-contract.md` documents the policy | Keep diagnostics and docs aligned when repair support expands |
| Safe export / snapshot | `done` | SQLite backup API with WAL-backed regression test proving committed rows survive export | Keep WAL-backed export coverage; block regressions in CI |
| Release versioning | `done` | `dev/release-policy.md`, `scripts/check-version-consistency.py`, and `scripts/verify-release-gates.py` define unified version/tag contract | Keep version alignment enforced |
| Release pipeline | `risky` | `.github/workflows/release.yml` exists and verify-release + build-python pass. **Publish-rust fails** (empty `CRATES_IO_TOKEN` secret). **Publish-pypi fails** (PyPI trusted publisher not configured for `coreyt/fathomdb`). No release artifact has ever been published. | Configure crates.io token and PyPI trusted publisher; re-run or re-tag to produce a successful release end to end |
| Documentation accuracy | `done` | README, implementation plan, and acceptance bar describe the shipped surface | Treat doc updates as part of every feature completion |
| Operational feedback | `done` | Response-cycle feedback across Rust, Python, and Go/CLI | Keep contract coverage; extend when new public operations are added |
| Performance benchmarks | `done` | `crates/fathomdb/benches/production_paths.rs`, `scripts/run-benchmarks.sh`, `.github/workflows/benchmark-and-robustness.yml` | Keep suite representative; review thresholds when workload changes |
| Fuzz / robustness testing | `done` | Go fuzz targets cover SQL sanitization and bridge decoding; scheduled robustness workflow runs fuzz smoke | Expand fuzz targets as new parser/admin surfaces are added |
| Acceptance criteria / SLO bar | `done` | `dev/production-acceptance-bar.md` defines functional, safety, robustness, and performance gates | Revisit thresholds when scale or platforms change |
| Structured logging and tracing | `done` | Feature-gated tracing crate, SQLITE_CONFIG_LOG bridge, tiered instrumentation, bridge JSON stderr subscriber, pyo3-log Python bridge, CI for tracing-enabled lint and test | Keep instrumentation aligned with new engine seams |
| Process hygiene | `done` | `dev/doc-governance.md` defines normative docs; `scripts/check-doc-hygiene.py` enforces tracker/checklist alignment in CI | Keep hygiene check in CI; update tracked docs in the same slice as behavior changes |

## Mandatory Blockers Before A Production Claim

- **Release pipeline must complete end to end at least once.** The v0.1.0 tag
  triggered the release workflow on 2026-03-30 but both publish steps failed due
  to missing credentials. Until artifacts are actually published, the release
  automation is unproven. (GitHub issue #12)

## Strongly Recommended Before Wider Production Use

- **Enforce benchmark thresholds in CI** so performance regressions are caught
  automatically rather than by manual review. (GitHub issue #14)
- **Resolve the Windows vector platform parity gap** or explicitly document
  Linux/macOS-only support for vector workflows. (GitHub issue #13)

## Current Overall Assessment

Current assessment: **not yet production-ready**.

The codebase quality, test coverage (572 tests across Rust/Go/Python), safety
practices, and recovery tooling are all solid. All critical and high findings
from the 2026-03-29 deep audit (`dev/path-to-production-2026-03-29.md`) are
resolved. CI is green across all 13 jobs.

The gap is operational: the release pipeline has never successfully published an
artifact. Once the publishing credentials are configured and a release completes
end to end, the release pipeline row moves to `done` and the overall assessment
becomes production-ready.

## Open GitHub Issues Relevant to Production

| # | Title | Impact |
|---|-------|--------|
| 12 | Exercise the release workflow end to end with a real tag | Blocks production claim |
| 14 | Enforce benchmark thresholds in CI | Performance regression risk |
| 13 | Windows vector platform parity gap | Platform support clarity |
| 9 | Harden vector regeneration recovery path | Recovery depth |
| 6 | Track missing FTS fallback for degraded vector reads | Graceful degradation |
| 10 | Revisit long-term vector recovery architecture | Architectural clarity |
| 11 | Publish operator binaries as release artifacts | Operator experience |
| 7 | Add recover unit test for migration-history reset | Test coverage |
| 8 | Add bridge test for unsupported-command response | Test coverage |
| 15 | Expand automated repair and recovery coverage | Recovery depth |
| 16 | Track post-v0.1 public-surface gaps | API completeness |

## Deep Audit Status

The 2026-03-29 deep audit (`dev/path-to-production-2026-03-29.md`) examined
writer/transaction safety, admin/recovery paths, and scalability edge cases.

| Severity | Total | Fixed | By Design | Acknowledged | Open |
|----------|-------|-------|-----------|--------------|------|
| Critical | 5 | 5 | 0 | 0 | 0 |
| High | 6 | 6 | 0 | 0 | 0 |
| Medium | 7 | 4 | 1 | 0 | 1 |
| Low | 4 | 2 | 0 | 1 | 0 |

Open findings:
- **M-6**: safe_export manifest page_count is advisory (checkpoint-to-backup
  race)

## Known Design Decisions

- **Operational retention scheduling is external by design.** The engine exposes
  `plan_operational_retention` and `run_operational_retention` primitives; the
  operator or an external scheduler invokes them.
- **PID in lock file is best-effort diagnostic.** On Windows, exclusive file
  locks prevent reading the PID from other handles; the lock still prevents
  concurrent access correctly.

## Suggested Closure Order

1. Configure crates.io token + PyPI trusted publisher; re-run release workflow
2. Close issue #14 (enforce benchmark thresholds)
3. Close issue #13 (Windows vector parity) or document the platform boundary
