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

## benchmark-and-robustness.yml — Phase 12

- Source: `git show 39ee271^:.github/workflows/benchmark-and-robustness.yml`
- Pre-0.6.0 shape: weekly cron (`0 7 * * 1`); jobs:
  `rust-benchmarks`, `go-fuzz-smoke`, `rust-scale-tests`,
  `rust-tracing-stress`, `python-stress-tests`,
  `typescript-observability-harness`.
- Why deferred: scale/stress/fuzz harnesses do not exist yet on 0.6.0. The
  workflow without targets would be a green no-op that misleads reviewers.
- Adaptations required for 0.6.0:
  - Drop `go-fuzz-smoke` entirely (no `go/` surface).
  - Repath `python/` -> `src/python/`, `typescript/` -> `src/ts/`.
  - Drop the `cargo build -p fathomdb --features node` step until napi-rs
    lands.
  - Re-target `python-stress-tests` to whatever `--features python` /
    binding shape Phase 11 chose.

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
