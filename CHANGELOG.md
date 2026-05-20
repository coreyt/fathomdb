# Changelog

Release notes for the FathomDB engine, Python SDK, and TypeScript SDK.
Cuts follow the version tagged on `0.6.0-rewrite`. Each released section
MUST list every removed public symbol under a `### Removed` heading;
the removal-detect linter (`scripts/security/check-removal-changelog.sh`,
AC-050c) gates merges against this invariant.

## [Unreleased]

### Removed

(none yet)

## 0.6.0 - 2026-05-19

First stable release of FathomDB 0.6.0 — local-first retrieval
engine on SQLite (FTS5 + `sqlite-vec`) with Rust, Python, and
TypeScript SDKs.

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

### Changed

- Release workflow: napi build matrix uses the canonical
  `win32-x64-msvc` target label for npm `optionalDependencies`
  resolution.
- Release workflow: `publish-rust` dry-run cascade restored via
  the rc.1 bootstrap publish that seeded sibling-dep versions on
  crates.io.
- Release workflow: npm `publish` passes `--tag next` for
  prerelease versions so the `latest` dist-tag stays pinned to
  the most recent stable.
- Release workflow: post-publish gates (`assert-co-tagging.sh`
  and the three `smoke-{crates,pypi-wheel,npm}.sh` scripts)
  accept `MAJOR.MINOR.PATCH(-PRERELEASE)?` SemVer.
- Release workflow: `smoke-pypi-wheel.sh` normalizes SemVer to
  PEP 440 (e.g. `0.6.0-rc.4` → `0.6.0rc4`) before `pip install`
  so the wheel resolves under pip's normalized version index.
- Release workflow: `assert-co-tagging.sh` sends a `User-Agent`
  header on crates.io API calls (the registry returns HTTP 403
  without one).
- Release workflow: PyPI + npm smoke scripts write a minimal
  valid record (`{"kind":"doc","body":"{}"}`) instead of an
  empty batch that the engine rejects per the 5-verb invariant.
- Release workflow: new `src/ts/tsconfig.build.json` emits
  `dist/index.js` at the path `package.json "main"` points to.
- Release workflow: `github-release` job explicitly sets
  `prerelease: ${{ contains(github.ref_name, '-') }}` so future
  RC tags are flagged as prereleases on GitHub.

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
