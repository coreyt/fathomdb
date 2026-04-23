# FathomDB Production Readiness Checklist

**Status:** Current
**Last updated:** 2026-04-22

## Purpose

Single production-readiness gate for the current FathomDB release line. This
file is intentionally retained at its existing path because
`scripts/check-doc-hygiene.py` validates its readiness sections.

Status meanings:

- `done` -- implemented, verified, and strong enough for the documented support
  contract
- `risky` -- present but incomplete, weakly validated, or operationally fragile
- `missing` -- not yet implemented or operationalized

## Current Readiness Matrix

| Area | Status | Evidence | Required To Close |
|---|---|---|---|
| Core engine functionality | `done` | Reads, writes, provenance, FTS, vector support, admin flows, and language harnesses are covered across Rust, Go, Python, and TypeScript surfaces | Keep functional test suites passing as the public API expands |
| Rust test coverage | `done` | Workspace tests cover engine, schema, writer, coordinator, admin, and bindings paths | Maintain coverage and block regressions in CI |
| Go integrity/admin tooling | `done` | `go/fathom-integrity` covers check, recover, rebuild, trace, excise, export, and repair flows | Keep diagnostics and bridge protocol aligned with engine admin changes |
| Python bindings | `done` | PyO3 bindings cover open/close, writes, reads, admin operations, and vector-enabled harness paths | Keep bindings and `ffi_types.rs` parity tests current |
| TypeScript SDK | `done` | TypeScript SDK and harness cover public write/query/admin paths | Keep SDK examples and generated types aligned with the Rust/Python surface |
| Vector support | `done` | Current release supports sqlite-vec, per-kind vector tables, vector profile metadata, and explicit regeneration from canonical chunks | Track managed vector projection as future work, not as a current production blocker |
| Recovery path correctness | `done` | Recovery restores canonical state and rebuildable projection capability; vector regeneration is documented as explicit operational work | Keep fixtures covering canonical rows, FTS rebuild, and vector profile/regeneration contracts |
| Automated repair | `done` | `fathom-integrity repair` supports duplicate-active, runtime-fk, and orphaned-chunks classes; policy is documented in `dev/repair-support-contract.md` | Expand repair classes only with tests and docs |
| Safe export / snapshot | `done` | SQLite backup API and WAL-backed regression coverage preserve committed rows | Keep export coverage as manifest fields evolve |
| Release versioning | `done` | `dev/release-policy.md`, version consistency checks, and release gate scripts define the version/tag contract | Keep version alignment enforced |
| Release pipeline | `risky` | Release workflow exists, but publishing credentials and end-to-end artifact publication must be proven for each distribution target | Configure credentials and complete a real release workflow end to end |
| Documentation accuracy | `done` | README, public docs, architecture, repair contract, test plan, and governance docs describe the current support surface | Treat doc updates as part of every feature completion |
| Operational feedback | `done` | Response-cycle feedback and structured tracing cover public operation lifecycles | Extend feedback when new public operations are added |
| Performance benchmarks | `done` | Benchmark and robustness workflows exercise representative paths | Review thresholds when workload assumptions change |
| Fuzz / robustness testing | `done` | Go fuzz targets and scheduled robustness workflow cover bridge decoding and SQL sanitization smoke paths | Expand fuzz targets for new parser/admin surfaces |
| Acceptance criteria / SLO bar | `done` | `dev/production-acceptance-bar.md` defines functional, safety, robustness, and performance gates | Revisit thresholds when scale or platform support changes |
| Structured logging and tracing | `done` | Feature-gated tracing, SQLite log bridge, response-cycle feedback, and Python/Go logging integration are documented | Keep instrumentation aligned with new engine boundaries |
| Process hygiene | `done` | `dev/doc-governance.md` defines normative docs; `scripts/check-doc-hygiene.py` enforces checklist consistency | Keep hygiene check in CI |

## Mandatory Blockers Before A Production Claim

- **Release pipeline must complete end to end at least once for the current
  release line.** Until artifacts are successfully published from the release
  workflow, release automation remains operationally unproven.

## Strongly Recommended Before Wider Production Use

- **Keep benchmark thresholds enforced in CI** so performance regressions are
  caught automatically rather than by manual review.

## Current Overall Assessment

Current assessment: **not yet production-ready**.

The engine support contract, test coverage, repair model, and public docs are
strong enough for current development and controlled integration use. The
remaining production blocker is operational release proof: the release workflow
must publish artifacts end to end for the current release line.

## Known Design Decisions

- **Operational retention scheduling is external by design.** The engine exposes
  retention planning and execution primitives; an operator or external
  scheduler invokes them.
- **Projection tables are derived state.** FTS and vector table contents are not
  canonical recovery material.
- **Vector projection is in transition.** Current releases expose explicit
  regeneration and low-level vector insert support; the target design is
  FathomDB-managed async/incremental vector projection from canonical chunks.

## Suggested Closure Order

1. Configure publishing credentials for crates.io, PyPI, npm, and release
   artifacts as applicable.
2. Run a release workflow end to end on the current release line.
3. Update this checklist and `dev/production-acceptance-bar.md` with the
   resulting artifact evidence.
