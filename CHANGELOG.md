# Changelog

Release notes for the FathomDB engine, Python SDK, and TypeScript SDK.
Cuts follow the version tagged on `0.6.0-rewrite`. Each released section
MUST list every removed public symbol under a `### Removed` heading;
the removal-detect linter (`scripts/security/check-removal-changelog.sh`,
AC-050c) gates merges against this invariant.

## [Unreleased]

### Removed

(none yet)

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
