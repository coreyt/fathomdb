# Changelog

Release notes for the FathomDB engine, Python SDK, and TypeScript SDK.
Cuts follow the version tagged on `0.6.0-rewrite`. Each released section
MUST list every removed public symbol under a `### Removed` heading;
the removal-detect linter (`scripts/security/check-removal-changelog.sh`,
AC-050c) gates merges against this invariant.

## [Unreleased]

### Removed

(none yet)

## 0.6.0-rc.4 - 2026-05-18

Cut after rc.3's post-publish gates (`assert-co-tagging` +
`smoke-pypi-wheel` + `smoke-npm`) failed on three real defects
in the verification scripts themselves. rc.3 artifacts ARE
correct and live on crates.io, PyPI, and npm, but no GitHub
release entry was created for `v0.6.0-rc.3`; rc.4 supersedes
it on the GitHub-Release axis. No functional engine or SDK code
change since rc.3.

### Changed

- Release workflow: `assert-co-tagging.sh` now sends a
  `User-Agent` header on crates.io API calls (the registry
  returns HTTP 403 without one) — fix landed in `26bb7da`.
- Release workflow: PyPI + npm smoke scripts write a minimal
  valid record (`{"kind":"doc","body":"{}"}`) instead of an
  empty batch that the engine rejects per the 5-verb invariant.
- Release workflow: new `src/ts/tsconfig.build.json` emits
  `dist/index.js` at the path `package.json "main"` points to;
  the previous layout emitted `dist/src/index.js` so the
  published npm tarball was broken at
  `import { Engine } from "fathomdb"`.

## 0.6.0-rc.3 - 2026-05-18

Cut after rc.2's post-publish gates (co-tagging-assert + three
`smoke-*` jobs + github-release) failed on a hardcoded
`MAJOR.MINOR.PATCH` regex that rejected the `-rc.N` suffix.
rc.2 artifacts ARE live on crates.io, PyPI, and npm, but no
GitHub release entry was created for `v0.6.0-rc.2`; rc.3
supersedes it. No functional engine or SDK code change since
rc.2.

### Changed

- Release workflow: `assert-co-tagging.sh` and the three
  `smoke-{crates,pypi-wheel,npm}.sh` scripts now accept
  `MAJOR.MINOR.PATCH(-PRERELEASE)?` so pre-release tags pass
  the post-publish gates (fix landed in `70e6487`).
- Release workflow: `smoke-pypi-wheel.sh` normalizes SemVer to
  PEP 440 (`0.6.0-rc.3` → `0.6.0rc3`) before `pip install` so
  the wheel resolves under pip's normalized version index.

## 0.6.0-rc.2 - 2026-05-18

Real release candidate of 0.6.0. The `0.6.0-rc.1` slot was consumed
by the bootstrap publish (`dev/design/release.md` § RC1 bootstrap
publish) that seeded crates.io with the seven Axis-W crates plus
`fathomdb-embedder-api`; rc.2 is the first RC that exercises the
full tag-trigger workflow end-to-end (smoke + co-tagging +
github-release jobs). No functional engine or SDK code change
since rc.1.

### Changed

- Release workflow: napi build matrix uses the canonical
  `win32-x64-msvc` target label (npm `optionalDependencies`
  resolution).
- Release workflow: `publish-rust` dry-run cascade restored via the
  rc.1 bootstrap publish that seeded sibling-dep versions on
  crates.io.
- Release workflow: npm `publish` passes `--tag next` for
  prerelease versions to keep the `latest` dist-tag pointing at the
  most recent stable.

## 0.6.0-rc.1 - 2026-05-17

First release candidate of 0.6.0. Engine + bindings + release-
engineering substrate landed across Phases 5-12. Axis W bumped to
`0.6.0-rc.1`; Axis E (`fathomdb-embedder-api`) joined the lockstep
at `0.6.0-rc.1` solely to seed crates.io for the RC1 bootstrap
publish (`dev/design/release.md` § RC1 bootstrap publish) — the
trait surface is unchanged. Axis-E independence resumes at/after
0.6.0 GA.

### Added

- Local-first retrieval engine on SQLite (FTS5 + `sqlite-vec`):
  canonical writes, vector projections, scheduler, op-store, reader
  pool with thread-affine workers.
- Five-verb runtime SDK surface: `Engine.open`, `engine.write`,
  `engine.search`, `engine.close`, `admin.configure`.
- Typed error hierarchy under `EngineError` (`StorageError`,
  `ProjectionError`, `VectorError`, `EmbedderError`,
  `EmbedderNotConfiguredError`, `KindNotVectorIndexedError`,
  `SchedulerError`, `OpStoreError`, `WriteValidationError`,
  `SchemaValidationError`, `OverloadedError`, `ClosingError`,
  `DatabaseLockedError`, `CorruptionError`,
  `IncompatibleSchemaVersionError`).
- Engine-attached instrumentation: `engine.drain`,
  `engine.counters`, `engine.set_profiling`,
  `engine.set_slow_threshold_ms`, host logging subscriber attach.
- Python SDK (`fathomdb`) — PyO3 binding with type stubs.
- TypeScript SDK (`fathomdb`) — napi-rs binding, Promise API,
  handoff pool, typed exception envelope (TS milestone 1; not yet
  Python-parity — see Deferred).
- Rust facade crate (`fathomdb`) re-exporting runtime verbs from
  `fathomdb-engine`.
- CLI (`fathomdb-cli`) — `doctor` and `recover` verbs (Phase 10a).
- Two-axis versioning (Axis W workspace lockstep + Axis E
  independent embedder-api semver) with `scripts/set-version.sh
--check-files` enforcement and pre-push hook integration.
- 8-tier topological publish workflow `.github/workflows/release.yml`
  with crates.io index-propagation sleeps, post-publish smoke
  against fresh registry installs, co-tagging assert.
- actionlint v1.7.7 wired as canonical workflow validator.
- External user docs: install + quickstart + reference + concepts
  - compatibility (Phase 12-DX).

### Deferred

- **Performance gates AC-012 / AC-013 / AC-019 / AC-020** deferred
  to 0.6.1 + Pack 7 (HITL re-confirmed 2026-05-17, Phase 12-P).
  AC-020 N=8 concurrent reader scaling is an architectural gap
  requiring vendor-SQLite work; AC-012 expected to close on
  canonical-runner re-measurement; AC-013/AC-019 close via Pack 7
  batched-insert vec0 API. See `dev/test-plan.md` § Current Perf
  Attribution.
- **`Engine.open` structured open report** dropped by both Python
  and TypeScript bindings in 0.6.0; populated native-side but not
  surfaced. Closes in 0.6.1 (slice `12-TX-OPENREPORT`). Symmetric,
  not a parity gap.
- **Logical-id verbs** (`purge_logical_id`, `restore_logical_id`)
  deferred to 0.7.x (HITL re-confirmed 2026-05-17, Phase 12-V-VERBS).
  Canonical-identity substrate design-only in 0.6.0. Client
  workaround: `fathomdb recover --excise-source <id>`.
- **TypeScript SDK Python-parity** — TS milestone 1 shipped
  2026-04-07; full parity is a Phase 12 deliverable, lands before
  GA tag. Prefer Python SDK for production pilots until then.

### Removed

(none — 0.6.0 is a rewrite; no 0.5.x→0.6.0 deprecation shims)
