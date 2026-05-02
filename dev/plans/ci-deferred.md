---
title: Deferred CI Work for 0.6.0
date: 2026-05-02
target_release: 0.6.0
desc: CI workflows from pre-0.6.0 that were intentionally not restored at scaffold time
status: draft
---

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
- Why deferred: 0.6.0 scaffold has no PyO3, no napi-rs, no maturin, no
  multi-package version-sync policy. Building/publishing these without a
  release-process ADR would freeze decisions that the freeze corpus
  (`dev/release/...`) is still expected to record.
- Adaptations required for 0.6.0:
  - Repath `python/` -> `src/python/`, `typescript/` -> `src/ts/`.
  - Drop maturin until PyO3 actually lands in Phase 11.
  - Drop napi prebuild matrix until napi-rs actually lands in Phase 11.
  - Re-decide the tiered crate publish order against the seven 0.6.0 crates
    (`fathomdb`, `fathomdb-cli`, `fathomdb-engine`, `fathomdb-query`,
    `fathomdb-schema`, `fathomdb-embedder`, `fathomdb-embedder-api`) — the
    pre-0.6.0 order assumed four crates.
  - Re-do `scripts/verify-release-gates.py` and
    `scripts/check-version-consistency.py` against the new package paths, or
    drop them if the release-process ADR replaces them.

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
- Why deferred: requires a release-process ADR (single-source-of-truth
  package version, multi-package sync policy) that has not been written.
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
