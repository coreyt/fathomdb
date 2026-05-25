# Changelog

Release notes for the FathomDB engine, Python SDK, and TypeScript SDK.
Cuts follow the version tagged on `0.6.0-rewrite`. Each released section
MUST list every removed public symbol under a `### Removed` heading;
the removal-detect linter (`scripts/security/check-removal-changelog.sh`,
AC-050c) gates merges against this invariant.

## [Unreleased]

### Removed

(none yet)

## 0.6.1 - 2026-05-25

Promotion of `0.6.1-rc.1` to GA following V-slice fresh-install
verification (GREEN on all three bindings — see
`dev/plans/runs/0.6.1-V-transcript.txt`). Scope identical to RC1
below; no code or interface change between RC1 and GA. Axis E
(`fathomdb-embedder-api`) remains at `0.6.0` per Wake decision
`d-001`.

## 0.6.1-rc.1 - 2026-05-25

Patch release. Closes three 0.6.0 deferred items (Python and TypeScript
`OpenReport` surfacing, plus the axis-E independence demonstration),
resolves three Dependabot advisories, and carries the AC-012 canonical-
runner re-measurement as documented evidence (verdict RED; Pack 7 perf
work escalates to 0.7.0 per HITL 2026-05-24).

Axis-E (`fathomdb-embedder-api`) stays at `0.6.0` per Wake decision
`d-001`: no trait-surface change in this release, so axis-E does not
bump in lockstep with axis-W. This is the first post-GA exercise of
the two-axis discipline.

### Fixed

- `OpenReport` is now surfaced from both bindings via an engine-attached
  accessor (closes 12-TX-OPENREPORT carry-over from 0.6.0 GA):
  - Python: `engine.open_report()` returns the native `OpenReport`
    fields verbatim under snake_case identifiers
    (`schema_version_before`, `schema_version_after`,
    `migration_steps`, `embedder_warmup_ms`, `query_backend`,
    `default_embedder`). Idempotent — repeat calls return identical
    data (snapshot, not live state). Closes **AC-068c**.
  - TypeScript: `engine.openReport()` returns the camelCase mirror
    (`schemaVersionBefore`, `schemaVersionAfter`, `migrationSteps`,
    `embedderWarmupMs`, `queryBackend`, `defaultEmbedder`). Sync
    return — data lives in the napi engine struct after `open`.
    Closes **AC-068d**.
  - `Engine.open(...)` signatures are unchanged from 0.6.0 in both
    bindings (additive accessor; no return-shape regression).
- `scripts/security/check_removal_changelog.py` and its bash wrapper
  point their `--base` default at `v0.6.1` (was `v0.6.0`), advancing
  the "removals since last released API" anchor as 0.6.1 becomes the
  new GA reference. AC-050c regression-sentinel test #4 will be
  transiently RED in the BUMP → RC1 → GA window until the `v0.6.1`
  tag is pushed.

### Security

- **RUSTSEC-2025-0020** — bump `pyo3` `0.22.6` → `0.24.1` across the
  workspace; rename `*_bound` PyO3 APIs (24 callsites) to drop the
  deprecation warnings under `-D warnings`.
- **GHSA-mh29-5h37-fv8m** — bump `js-yaml` `4.1.0` → `4.1.1` via
  `markdownlint-cli2` `0.18` → `0.22.1` (transitive).
- **CVE-2024-3651 (idna)** — confirmed false-positive against
  fathomdb (not in the Python dependency graph after lock-file
  audit); `src/python/uv.lock` checked in to make the audit
  reproducible.

### Changed

- `scripts/set-version.sh --workspace 0.6.1` exercises the axis-E
  independence invariant for the first time post-GA: axis-W
  (`Cargo.toml`, `pyproject.toml`, `package.json`, and the five
  workspace.dependencies pins for `fathomdb`, `fathomdb-embedder`,
  `fathomdb-engine`, `fathomdb-query`, `fathomdb-schema`) advances
  to `0.6.1`; axis-E (`fathomdb-embedder-api` `[package].version`
  and its `workspace.dependencies` pin) stays at `0.6.0`.
  Regression sentinel codified in
  `scripts/tests/test_set_version.sh` test #13.
- B-001 forward-retag — `scripts/security/check_removal_changelog.py`
  and `scripts/security/check-removal-changelog.sh` default `--base`
  advanced from `v0.6.0` to `v0.6.1`.

### Removed

(none — patch release, no public symbol removals.)

### Deferred (carry-over)

- **AC-012** text-query latency on FTS5 (p50 ≤ 20 ms / p99 ≤ 150 ms):
  re-measured 2026-05-23 on canonical x86_64 tier-1 CI (AMD EPYC
  9V74, 4 cores, Ubuntu 24.04.4, rustc 1.95.0, SQLite 3.45.x via
  `libsqlite3-sys` 0.28.0) at N=1,000,000. Verdict **RED**:
  p50 = 140.95 ms (7.05× over budget), p99 = 458 ms (3.05× over
  budget). Pack 7 un-defer trigger fires; AC-012 closure target
  moved to **0.7.0** (perf-only release; budget revision + tuning).
  Evidence: `dev/notes/perf-canonical-runner-2026-MM.md` and
  `dev/plans/runs/0.6.1-AC012-measure-output.json` (workflow run
  26346417896). 0.6.1 carries this measurement as evidence and
  does NOT claim AC-012 closure.
- **AC-013** vector retrieval latency, **AC-019** mixed-retrieval
  stress tail, **AC-020** N=8 concurrent reader scaling: stay
  deferred per Pack 7 trigger evaluation (Pack 7 escalated to
  0.7.0 alongside AC-012).
- **Logical-id verbs** (`purge_logical_id` / `restore_logical_id`)
  stay deferred to **0.8.0** per HITL 2026-05-24 rescope.

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
  deferred to 0.8.0 (originally deferred to 0.7.x at Phase 12-V-VERBS
  2026-05-17; re-targeted to 0.8.0 per HITL 2026-05-24 alongside the
  canonical-identity substrate and Memex knowledge-store work — see
  `dev/roadmap/0.8.0.md`). Canonical-identity substrate design-only
  in 0.6.0. Client workaround: `fathomdb recover --excise-source <id>`.
- **TypeScript SDK Python-parity** — TS milestone 1 shipped
  2026-04-07; full Python-parity did NOT land at 0.6.0 GA and
  carries forward as a post-GA deliverable. Prefer Python SDK
  for production pilots until parity ships.

### Removed

(none — 0.6.0 is a rewrite; no 0.5.x→0.6.0 deprecation shims)
