# Changelog

All notable changes to FathomDB are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-04-13

This is a significant minor release bringing a unified retrieval surface
(`search()`), a read-time query embedder with an optional built-in
Candle-based default, and a large set of supporting type, SDK, and
infrastructure changes. The rollout spans Phases 10 through 15 plus
Phase 12.5 of the adaptive + unified search design of record at
`dev/design-adaptive-text-search-surface.md` and its vector addendum at
`dev/design-adaptive-text-search-surface-addendum-1-vec.md`.

### Added

- **Unified `search()` query surface**: a single entry point that runs a
  strict text branch, engine-derived relaxed text branch, and (when an
  embedder is configured) vector branch, fusing results under a
  block-precedence policy. `NodeQueryBuilder::search(query, limit)`
  returns a tethered `SearchBuilder` whose `.execute()` is statically
  typed to return `SearchRows` — no union return types. Chainable with
  the full filter surface (kind / logical_id / source_ref / content_ref /
  json_* family) and `.with_match_attribution()`.
- **`SearchBuilder` in all three SDKs**: Rust (`crates/fathomdb/src/search.rs`),
  Python (`python/fathomdb/_query.py`), and TypeScript
  (`typescript/packages/fathomdb/src/query.ts`) each expose a distinct
  builder class with identical filter surfaces and a typed `SearchRows`
  terminal. Python exposes the config as `db.query(kind).search(...)`;
  TypeScript as `engine.nodes(kind).search(...)`.
- **Tethered `VectorSearchBuilder`**: `NodeQueryBuilder::vector_search()`
  returns a distinct builder that carries the full filter surface and
  returns `SearchRows` via `execute_compiled_vector_search`. Filters
  fuse into the vec_nodes CTE (kind / logical_id / source_ref /
  content_ref) with JSON predicates running as outer-WHERE residuals.
  Capability-miss (no sqlite-vec extension) is non-fatal — returns empty
  `SearchRows` with `was_degraded = true`.
- **`RetrievalModality` enum** (`Text | Vector`) on `SearchHit` and
  `SearchRows.vector_hit_count`, `SearchHit.vector_distance: Option<f64>`,
  `SearchHit.match_mode: Option<SearchMatchMode>` for unifying text and
  vector retrieval shapes under a single result type. Score-direction
  contract is higher-is-better across both modalities (text uses
  `-bm25`, vector uses `-distance`). `SearchHitSource::Vector` is a
  first-class source classifier.
- **Read-time query embedder scaffolding**: `QueryEmbedder` trait
  (`embed_query(&str) -> Result<Vec<f32>, EmbedderError>` + `identity()`),
  `QueryEmbedderIdentity`, `EmbedderError`, and `EmbedderChoice` enum
  (`None | Builtin | InProcess(Arc<dyn QueryEmbedder>)`) on
  `EngineOptions`. When an embedder is configured, the coordinator's
  `fill_vector_branch` step injects a `CompiledVectorSearch` into the
  retrieval plan post-compile. When `EmbedderChoice::None` (the default),
  behavior is identical to pre-12.5 — vector branch stays empty, and
  `compile_retrieval_plan` still returns `vector: None`.
- **Built-in Candle-based default embedder** (`BuiltinBgeSmallEmbedder`)
  behind the new `default-embedder` Cargo feature (off by default).
  Uses BAAI/bge-small-en-v1.5 (384-dim, BERT-small) via
  `candle-transformers`, with `[CLS]` token pooling and explicit L2
  normalization (NOT Candle's stock mean pooling, which would silently
  degrade BGE retrieval quality). Model weights lazy-load on first
  `embed_query()` call via `hf-hub` into the standard huggingface cache,
  honoring `HF_HUB_OFFLINE` and `HF_HOME`. Load failures surface as
  `EmbedderError::Unavailable` and degrade the `search()` vector branch
  via `was_degraded = true` — no panic, no engine-open failure.
- **SDK-level embedder config**: Python `Engine.open(embedder="none"|"builtin")`
  kwarg and TypeScript `EngineOpenOptions.embedder?: "none" | "builtin"`.
  `"builtin"` is feature-flag-agnostic on the SDK surface: when fathomdb
  is built without `default-embedder`, it silently falls back to `None`.
- **Recursive property FTS** with a position map sidecar, `[CLS]`+L2
  pooling for BGE small, guardrails (`MAX_RECURSIVE_DEPTH=8`,
  `MAX_EXTRACTED_BYTES=65_536`), eager rebuild on schema registration,
  and opt-in match attribution via `with_match_attribution()`. Attribution
  resolves FTS5 `highlight()` sentinels against the position map to
  populate `SearchHit.attribution.matched_paths` with the JSON paths
  that contributed to the match.
- **Adaptive strict-then-relaxed policy** in `compile_retrieval_plan`
  and `execute_retrieval_plan`: the engine derives a relaxed branch
  from the strict query via `derive_relaxed`, runs the relaxed branch
  only when strict returns fewer than `K=1` hits, and fuses results
  under a block-precedence rule (strict block → relaxed block → vector
  block, with cross-branch dedup by branch precedence). `RELAXED_BRANCH_CAP=4`,
  `FALLBACK_TRIGGER_K=1`.
- **`fallback_search` narrow helper**: `Engine.fallback_search(strict, relaxed, limit)`
  for the dedup-on-write pattern where the caller wants to supply both
  branches verbatim without engine-side relaxation. Shares the same
  `SearchRows` result shape and filter surface as `search()`.
- **Tokenizer migration to `unicode61 + remove_diacritics 2 + porter`**
  for `fts_nodes` and `fts_node_properties` (schema version 16).
  Existing databases upgrade via full rebuild from canonical state.
- **FTS filter fusion**: `partition_search_filters` pushes fusable
  predicates (kind / logical_id / source_ref / content_ref) into the
  FTS/vec CTE so per-branch `limit` applies after filtering. JSON
  predicates remain as outer-WHERE residuals.
- **External content objects**: nodes can reference external content
  (PDFs, web pages, datasets) via `content_ref`, and chunks can track
  content provenance via `content_hash`. Schema migration 14; new
  query filters `filter_content_ref_not_null()` and
  `filter_content_ref_eq(uri)` across all three SDKs; `NodeRow.content_ref`
  in query results; `WriteRequestBuilder.add_node()` and `add_chunk()`
  accept the new fields.
- **Cross-language parity fixtures and SDK harness scenarios** for
  `search()` (`xlang_search_basic`, `xlang_search_strict_miss_relaxed`,
  `xlang_search_with_attribution`, `xlang_search_empty_query_returns_empty`)
  plus matching harness scenarios in `python/examples/harness/` and
  `typescript/apps/sdk-harness/`.
- **Stress tests** covering `search()` under concurrent reads, writer
  contention, and determinism invariants
  (`search_reads_never_block_on_background_writes`,
  `search_deterministic_hit_ordering`,
  `search_fallback_stable_under_concurrent_reads`).
- **Consumer docs rewrite**: `docs/guides/querying.md` promotes
  `search()` as the primary surface with `text_search()` / `vector_search()` /
  `fallback_search()` retained as advanced overrides. New "Read-time
  embedding" section covers `EmbedderChoice` variants, Python/TS
  examples, degradation semantics, cold vs warm latency notes, and
  `[CLS]`+L2 pooling technical detail. Python and TypeScript READMEs
  lead Quick Start with `search()` and `embedder="builtin"`.
- **Release checklist** at `dev/notes/release-checklist.md` covering
  preconditions, code gates, version sync, changelog, documentation
  currency, CI workflow health, commit/tag, release workflow monitoring,
  and rollback plan.

### Changed

- **`SearchHit.match_mode`** is now `Option<SearchMatchMode>` instead of
  `SearchMatchMode`. Vector hits carry `match_mode: None`; text hits
  carry `Some(Strict | Relaxed)`. This is a breaking change for callers
  that previously unwrapped `match_mode` directly.
- **`SearchRows`** gains `vector_hit_count`, `strict_hit_count` and
  `relaxed_hit_count` exposure, and a generalized `fallback_used` flag
  that now covers both text-relaxed fallback and vector-branch firing.
- **`compile_retrieval_plan`** is the unified compile entry point for
  `search()`. `compile_search_plan` and `compile_search_plan_from_queries`
  remain for the explicit text-only and two-shape paths.

### Fixed

- **Filter-chain drops on `fallback_search` FFI**: prior to 13a the FFI
  layer built a filter-only `QueryAst` without a sentinel `TextSearch`
  step, causing `partition_search_filters` to silently drop all caller
  filters. Fixed by seeding the sentinel step before `compile_search_plan_from_queries`.
- **Crash-recovery hole in property FTS rebuild**: the rebuild was
  gated on migration-version check; a crash between migration commit
  and rebuild commit would lose the rebuild. Replaced with always-on
  empty-table check.
- **Sibling-kind FTS duplication**: eager rebuild iterated all kinds
  but only deleted the target kind's rows. Added `insert_property_fts_rows_for_kind`
  scoped helper.
- **`cast_possible_wrap` clippy lint** in `crates/fathomdb-schema/src/bootstrap.rs`
  under `--features sqlite-vec`: replaced `dimension as i64` with
  `i64::try_from(dimension)` routing through `SchemaError::Sqlite`.
- **Python harness baseline**: stale expected scenario counts (5→22)
  and `vec_nodes_active` activation in baseline mode (now gated on
  `context.mode == "vector"`).
- **TypeScript SDK harness runtime scenario**: pre-existing broken
  scenario rewritten to use `engine.admin.traceSource` + `checkSemantics`
  instead of stale `engine.nodes("Document").execute()` runtime-table
  assertions.

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
