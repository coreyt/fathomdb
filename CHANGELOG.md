# Changelog

All notable changes to FathomDB are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **External content objects**: nodes can now reference external content (PDFs,
  web pages, datasets) via `content_ref`, and chunks can track content
  provenance via `content_hash`. Both fields are optional and nullable.
  - Schema migration 14: adds `content_ref TEXT` to `nodes` (with partial
    index) and `content_hash TEXT` to `chunks`
  - New query filters: `filter_content_ref_not_null()` and
    `filter_content_ref_eq(uri)` across Rust, Python, and TypeScript SDKs
  - `NodeRow` now surfaces `content_ref` in query results
  - `WriteRequestBuilder.add_node()` accepts `content_ref` parameter
  - `WriteRequestBuilder.add_chunk()` accepts `content_hash` parameter

## [0.2.5] - 2026-04-10

### Fixed

- **npm OIDC trusted publishing (final fix)**: use `npx npm@latest publish`
  to run npm >= 11.5.1 for the publish step. Node 22 ships npm 10.x which
  doesn't support OIDC; `npm install -g npm@latest` breaks with
  MODULE_NOT_FOUND; and removing `registry-url` from setup-node causes
  ENEEDAUTH. The `npx` approach avoids all three issues — it downloads
  npm 11.x on-demand without corrupting the global install, while
  setup-node's `.npmrc` + `registry-url` provides the registry config.

### Note

Once trusted publishing is enabled on an npm package, the registry rejects
all non-OIDC publishes (including local `npm publish`). This is by design.
Versions 0.2.1–0.2.4 failed to publish to npm due to the OIDC setup
issues above; 0.2.5 is the first version published to all three
registries via CI.

## [0.2.4] - 2026-04-09

### Fixed

- **npm OIDC trusted publishing (take 2)**: explicitly upgrade npm to
  the latest version (>= 11.5.1) before publishing. Trusted publishing
  requires npm 11.5.1+, but Node 20 LTS ships with npm 10.x and Node 22
  ships with npm 10.x as well. Without the upgrade, `npm publish` either
  errors with `ENEEDAUTH` or falls back to token-based auth and 404s.
- **setup-node configuration**: bumped to Node 22 (away from Node 20
  which is being deprecated by GitHub Actions in 2026).

## [0.2.3] - 2026-04-09

### Fixed

- **npm OIDC trusted publishing**: removed `registry-url` from
  `actions/setup-node` in the publish-npm job. The action was injecting
  a placeholder `NODE_AUTH_TOKEN` env var and writing an `.npmrc` that
  caused `npm publish` to attempt token-based auth and bypass OIDC
  trusted publishing entirely. Without `registry-url`, npm discovers
  the GitHub OIDC token automatically and trusted publishing works.

### Note

0.2.2 was the first version published to crates.io and PyPI via the
automated release pipeline. npm was stuck because of the OIDC bug
above. 0.2.3 is the first version published successfully to all three
registries.

## [0.2.2] - 2026-04-09

### Fixed

- **fathomdb-engine packaging**: vendor `tooling/sqlite.env` into the crate
  as `sqlite.env` so `cargo publish` doesn't strip it. The original
  `include_str!("../../../tooling/sqlite.env")` referenced a file outside
  the crate boundary, which broke crates.io publishing.
- **Python wheel build matrix**: replace `--find-interpreter` with explicit
  `-i python3.10 python3.11 python3.12` so cross-compile Docker containers
  don't try to build against Python 3.14 (unsupported by pyo3 0.23).

### Note

0.2.1 partially published: `fathomdb-query@0.2.1` and `fathomdb-schema@0.2.1`
made it to crates.io before `fathomdb-engine@0.2.1` failed verification.
0.2.2 is the first version with the engine fix; query/schema 0.2.2 are
republished alongside for workspace version consistency.

## [0.2.1] - 2026-04-09

### Added

- **macOS CI** — Rust, Go, and Python tests now run on `macos-latest`
- **Multi-platform Python wheels** — release builds manylinux (x86_64, aarch64),
  macOS (x86_64, arm64), and Windows (x86_64) via `PyO3/maturin-action` matrix
- **napi-rs prebuilt binaries** — release builds native bindings for
  `linux-x64-gnu`, `darwin-x64`, `darwin-arm64`, and `win32-x64`, bundled into
  a single npm package
- **napi prebuild smoke test** — CI matrix validates native binding builds on
  all target platforms for every PR
- **npm provenance** — `npm publish --provenance` via OIDC trusted publisher
  (no `NPM_TOKEN` secret required)
- **Package registry metadata** — `readme`, `keywords`, `categories`,
  `homepage` added to Cargo.toml; `license`, `authors`, `classifiers`,
  `urls` added to pyproject.toml; `author`, `homepage`, `bugs` added to
  package.json
- **Consolidated MIT license** — single `LICENSE` file, dropped dual-license
- **CHANGELOG.md** — this file

### Note

0.2.0 was published to npm only (manual publish during distribution setup);
0.2.1 is the first version published to all three registries
(crates.io, PyPI, npm) via the automated release workflow.

## [0.2.0] - 2026-04-08

### Added

- **TypeScript/Node.js SDK** with full Python parity via napi-rs bindings
- **Cross-language SDK consistency test harness** — validates Python and
  TypeScript SDKs produce identical database state across 6 scenarios
- **Progress callback / feedback support** in TypeScript SDK
- **User-facing documentation site** with MkDocs and auto-generated API reference
- **Configurable timeouts** for Go bridge and recovery operations
- **`WriterTimedOut` error variant** — distinguishes timeout (write may still
  commit) from rejection (write will not commit)
- **`InvalidConfig` error** — `read_pool_size=0` now returns a clean error
  instead of panicking
- **`SQLITE_OPEN_READONLY`** on reader pool connections (defense in depth)
- **`callNative` error wrapper** in TypeScript for better error messages
- 6 missing fields added to Go `bridgeSemanticReport` to match Rust `SemanticReport`
- stderr included in bridge error messages with bounded output buffers

### Changed

- **BREAKING**: TypeScript `toJsonString()` now JSON.stringify's all values
  including strings. Pre-serialized JSON strings must be wrapped in
  `new PreserializedJson(jsonString)`.

### Fixed

- TypeScript SDK package exports and native binding discovery
- `describeOperationalCollection` JSON parsing in Go bridge
- String/JSON conflation in write builder
- Tightened vec0 error matching
- Marked `raw_pragma` as doc-hidden
- Log unknown wire fields in Python instead of silently dropping them

### Current Gaps

These are known limitations in the current release:

- **No published packages** — not yet on crates.io, PyPI, or npm (source-build only)
- **No MSRV policy** — requires Rust edition 2024 (stable 1.94+)
- **No macOS CI** — tested on Linux and Windows only
- **No code coverage reporting** — no tarpaulin, coverage.py, or vitest --coverage
- **No encryption at rest** — design doc exists, implementation deferred
- **Retention not automatic** — operator must schedule `run_operational_retention()`
- **No scale testing** — no documented 10K+ node stress tests
- **`synchronous=NORMAL`** — safe for WAL mode but not power-loss-proof
- **3GB mmap default** — may need tuning on memory-constrained systems

## [0.1.1] - 2026-04-07

### Added

- Windows vector support and CI coverage
- Telemetry: always-on counters, SQLite cache stats, typed Python SDK surface
- Layer 6-9 test plan expansion (concurrency, sanitization, crash recovery, scale)
- Python minimum version lowered from 3.11 to 3.10
- Design note for encryption at rest and in motion
- Hardened telemetry: FFI return code checks, overflow prevention

### Fixed

- `filter_json_text_eq` only searching first node's properties
- Windows CI: sqlite3 install, timer granularity, PID check, EngineCore::open args
- Windows: skip world-writable check, add .bat test doubles, skip shell-script doubles
- FTS5 metacharacter sanitization to prevent syntax errors
- Bounded JSON parsing at Python FFI boundary (security fix H-6)
- Telemetry level parameter name for tracing feature compatibility

## [0.1.0] - 2026-04-06

### Added

- Initial release of FathomDB
- **Rust engine**: graph backbone (nodes, edges, runs, steps, actions),
  FTS5 full-text search, sqlite-vec vector search, JSON property filters,
  operational store (append-only logs, latest-state collections)
- **Python SDK** via PyO3 with full engine API surface
- **Go operator CLI** (`fathom-integrity`): integrity checks, recovery,
  repair, projection rebuild, safe export, provenance trace/excise
- Single-writer / multi-reader architecture with WAL
- Provenance tracking on every write
- 9-layer test plan with 460+ tests
- Schema migration system (13 versioned migrations)
- Supersession model (append-only, no destructive updates)

[Unreleased]: https://github.com/coreyt/fathomdb/compare/v0.2.5...HEAD
[0.2.5]: https://github.com/coreyt/fathomdb/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/coreyt/fathomdb/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/coreyt/fathomdb/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/coreyt/fathomdb/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/coreyt/fathomdb/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/coreyt/fathomdb/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/coreyt/fathomdb/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/coreyt/fathomdb/releases/tag/v0.1.0
