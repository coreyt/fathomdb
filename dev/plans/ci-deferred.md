---
title: Deferred CI Work for 0.6.0
date: 2026-05-12
target_release: 0.6.0
desc: CI workflows from pre-0.6.0 that were intentionally not restored at scaffold time
status: active
---

> 2026-05-12 — Release-process policy resolved. Two version axes
> (workspace lockstep plus `fathomdb-embedder-api` independent) and the
> 8-tier topological publish order are now spec'd in
> `dev/design/release.md`. The `set-version.sh` rewrite and `release.yml`
> restoration in Phase 11 implement that policy; no remaining ADR work
> blocks restoration.

# Deferred CI Work

The 0.6.0 scaffold restored only `.github/workflows/ci.yml` and
`.github/dependabot.yml`. The pre-0.6.0 repo had three more workflows that
intentionally were not ported on day one. They are listed here so that the
phase that actually needs them does not have to re-derive what existed.

Pre-0.6.0 source commit: `39ee271^` (the commit before
`archive: relocate pre-0.6.0 work out of active tree`).

## release.yml — Phase 11

- Source: `git show 39ee271^:.github/workflows/release.yml`
- Pre-0.6.0 shape: `verify-release` -> matrix(`build-python`, `build-napi`,
  `build-rust`) -> `all-builds-passed` cross-ecosystem gate -> tiered
  `publish-rust` (leaf -> engine -> facade with index-propagation sleeps),
  `publish-pypi`. NPM publish was added in `0.5.x` follow-ups.
- Why deferred (resolved 2026-05-12): 0.6.0 scaffold has no PyO3, no
  napi-rs, no maturin. The multi-package version-sync policy is now
  resolved in `dev/design/release.md` (two version axes; 8-tier topological
  publish order).
- Adaptations required for 0.6.0:
  - Repath `python/` -> `src/python/`, `typescript/` -> `src/ts/`.
  - Drop maturin until PyO3 actually lands in Phase 11.
  - Drop napi prebuild matrix until napi-rs actually lands in Phase 11.
  - Implement the 8-tier publish order from `design/release.md § Tiered
publish order` against the seven 0.6.0 crates plus the two binding
    packages.
  - Re-do `scripts/verify-release-gates.py` and
    `scripts/check-version-consistency.py` to enforce both version axes
    (Axis W lockstep across workspace crates + bindings; Axis E
    independent for `fathomdb-embedder-api`).

## benchmark-and-robustness.yml — DEMOTED to Pack 7 prerequisite (2026-05-17)

> **DEMOTED 2026-05-17 from Phase 12 to Pack 7 prerequisite.** Phase
> 12-B implementer surfaced clean blocker: all 5 pre-0.6.0 jobs depend
> on substrate that does not exist in 0.6.0-rewrite. Authoring the
> missing harnesses is out-of-scope per `feedback_reliability_principles`
> net-negative-LoC bias and the Pack 7 perf-evidence guard. Restoration
> waits for Pack 7 (or later) when the substrate lands. See
> `dev/plans/runs/12-B-benchmark-robustness-workflow-output.json` for
> per-job substrate-gap evidence.

- Source: `git show 39ee271^:.github/workflows/benchmark-and-robustness.yml`
- Pre-0.6.0 shape: weekly cron (`0 7 * * 1`); jobs:
  `rust-benchmarks`, `go-fuzz-smoke`, `rust-scale-tests`,
  `rust-tracing-stress`, `python-stress-tests`,
  `typescript-observability-harness`.
- Per-job substrate gaps in 0.6.0 (per 12-B blocker report):
  - `rust-benchmarks` — `scripts/run-benchmarks.sh` absent;
    `fathomdb-engine` has no `benches/` dir, no `[[bench]]` entry,
    no criterion dep. Pre-0.6.0 bench ran against a single `fathomdb`
    crate with `production_paths` bench — that crate is now a re-export
    facade. Resurrection = net-new authorship, not restoration.
  - `rust-scale-tests` — `fathomdb-engine` has no `scale.rs` test
    target. Never existed in 0.6.0-rewrite.
  - `rust-tracing-stress` — `fathomdb-engine` has no `tracing` cargo
    feature (no `[features]` section in Cargo.toml) + no
    `tracing_events` test. Authoring both is out-of-slice production
    work.
  - `python-stress-tests` — `src/python/tests/test_stress.py` absent.
    Pre-0.6.0 stress suite targeted the prior Python binding; semantics
    don't map onto PyO3 surface.
  - `typescript-observability-harness` — `src/ts/` is a single package
    (`fathomdb`), NOT a workspace. No `@fathomdb/sdk-harness` workspace
    exists. Re-introducing the multi-workspace layout is a Phase 11+
    topology decision, not workflow restoration.
- Workflow adaptations (if/when Pack 7 lands the substrate):
  - Drop `go-fuzz-smoke` entirely (no `go/` surface in 0.6.0).
  - Repath `python/` -> `src/python/`, `typescript/` -> `src/ts/`.
  - Use Phase 11b napi-rs build (`cd src/ts && npm run build:native`),
    not pre-0.6.0 `cargo build --features node`.
  - Use Phase 11d Python build pattern (`pip install -e src/python/`
    via maturin), not pre-0.6.0 `pip install -e python --no-build-isolation`.

## scripts/set-version.sh — Phase 11

- Source: `git show 39ee271^:scripts/set-version.sh`
- Why deferred (resolved 2026-05-12): release-process policy is now
  resolved in `dev/design/release.md`. Rewrite `set-version.sh` to enforce
  the two version axes (Axis W lockstep across workspace + bindings; Axis
  E independent for `fathomdb-embedder-api`).
- Pre-push hook (`scripts/hooks/pre-push`) intentionally does not call
  `set-version.sh --check-files` for 0.6.0 — the script does not exist in
  this tree. Restore the pre-push step in the same PR that restores the
  script.

## What stayed minimal on day one

`.github/workflows/ci.yml` collapses three pre-0.6.0 workflows
(`ci.yml`, `python.yml`, `typescript.yml`) into a single
`bootstrap -> agent-verify` job plus a
multi-OS rust matrix and a `docs` job, because `scripts/agent-verify.sh`
already fans out lint -> typecheck -> test across Rust/Python/TS/markdown
(see AGENTS.md §3). Splitting the workflow back out only buys parallelism;
defer that until CI wall time becomes a problem.
