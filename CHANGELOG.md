# Changelog

All notable changes to FathomDB are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] — 2026-04-17

### Breaking changes

- **Per-kind vec tables**: The global `vec_nodes_active` sqlite-vec virtual table is removed. Each kind with a registered vec profile now gets its own table named `vec_<sanitized_kind>` (e.g. `vec_document`, `vec_note`). No migration is provided — existing databases must re-run `regenerate_vector_embeddings` after upgrading. Direct SQL queries to `vec_nodes_active` will fail.
- **`VectorRegenerationConfig.table_name` removed**: The `table_name` field is replaced by `kind: String`. The engine derives the table name from the kind automatically.

### Added

- **`QueryEmbedder::max_tokens() -> usize`**: New required method on the Rust `QueryEmbedder` trait. Returns the maximum token count the embedder handles per input. All built-in and custom implementations must implement this method.
- **`BatchEmbedder` trait**: New write-time embedding trait (`batch_embed`, `identity`, `max_tokens`) for use with `regenerate_vector_embeddings_in_process`. Allows in-process embedding without a subprocess.
- **`regenerate_vector_embeddings_in_process`**: New `AdminService` method that takes `&dyn BatchEmbedder` directly, replacing the subprocess-based regeneration path for callers that have an in-process embedder.
- **TypeScript embedding adapters** (`fathomdb` npm package): `QueryEmbedder` interface plus four concrete adapters — `OpenAIEmbedder` (with TTL cache), `JinaEmbedder`, `StellaEmbedder`, `SubprocessEmbedder` (binary f32 LE wire protocol, serialized concurrent calls).

### Fixed

- `rebuild_projections(Vec)` was silently no-oping on 0.5.0 databases because it queried the removed `vec_nodes_active` table. Now iterates all kinds registered in `projection_profiles`.
- `check_semantics` fields `stale_vec_rows` and `vec_rows_for_superseded_nodes` always returned 0 on 0.5.0 databases for the same reason. Both are now computed correctly across all per-kind vec tables.

## [0.4.5] — 2026-04-15

### Added

- **Projection profiles** (`FtsProfile`, `VecProfile`, `ImpactReport`): CRUD methods on `AdminService` (`set_fts_profile`, `get_fts_profile`, `set_vec_profile`, `get_vec_profile`, `preview_projection_impact`) backed by the `projection_profiles` table. Five built-in tokenizer presets: `recall-optimized-english`, `precision-optimized`, `global-cjk`, `substring-trigram`, `source-code`.
- **Rust FFI + PyO3 bindings** (`set_fts_profile`, `get_fts_profile`, `set_vec_profile`, `get_vec_profile`, `preview_projection_impact`) on `EngineCore`, releasing the GIL via `py.detach()`.
- **Python profile management** (`fathomdb.FtsProfile`, `VecProfile`, `ImpactReport`, `RebuildMode`, `RebuildImpactError`): `AdminClient.configure_fts`, `configure_vec`, `preview_projection_impact`, `get_fts_profile`, `get_vec_profile`. `RebuildImpactError` raised when rows > 0 and `agree_to_rebuild_impact=False`.
- **Python embedding adapters** (`fathomdb.embedders`): `OpenAIEmbedder` (httpx, 300 s TTL cache), `JinaEmbedder`, `StellaEmbedder` (lazy `sentence-transformers`, L2-norm after Matryoshka truncation), `SubprocessEmbedder` (persistent process, binary f32 LE protocol). Optional deps: `fathomdb[openai]`, `fathomdb[jina]`, `fathomdb[stella]`, `fathomdb[embedders]`.
- **Admin CLI** (`fathomdb admin …`): `configure-fts`, `configure-vec`, `preview-impact`, `get-fts-profile`, `get-vec-profile`. Interactive rebuild-impact prompt; CI-safe abort when `--agree-to-rebuild-impact` is omitted. Optional dep: `fathomdb[cli]`.
- **Vec identity lifecycle guard**: `check_vec_identity_at_open` emits `tracing::warn!` when the configured embedder's `model_identity` or `dimensions` differ from the stored `VecProfile`; never blocks startup.
- **Query-side tokenizer adaptations**: `TokenizerStrategy` loaded from `projection_profiles` at open. `SubstringTrigram` queries shorter than 3 chars return empty (not an error). `SourceCode` strategy uses FTS5 phrase-quoting; post-render escaping removed (phrase-quoting is sufficient and post-render transform corrupted the expression).

### Fixed

- All new profile FFI PyO3 methods use `py.detach()` (PyO3 0.28 API) for GIL release; `py.allow_threads` was removed in 0.4.0 and is not used anywhere.
- `FtsPropertyPathSpec.from_wire` in Python no longer raises `TypeError` when a wire payload contains a `weight` key with a `null` value; `null` is now treated as absent (same as missing key).
- TypeScript package version bumped to `0.4.5` (was behind at `0.4.1`).
- `vitest.config.ts` prebuild resolution now prefers `linux-arm64-gnu.node` (napi CLI output) over the legacy `linux-arm64.node` name so local `napi build` outputs are picked up without manual renaming.

## [0.4.2] — 2026-04-15

### Breaking changes

- **`fts_node_properties` removed**: The global FTS5 table is replaced by per-kind tables `fts_props_<kind>` (migration 23). Direct SQL queries to `fts_node_properties` will fail. No public API change — all existing search calls continue to work.
- **Property FTS requires kind filter**: Kind-less text searches no longer return property FTS hits (only chunk/vector hits). A `KindEq` predicate is required to search property FTS. Add `.filter_kind_eq("MyKind")` or use `db.nodes("MyKind").search(...)` to restore property hits.
- **`FtsPropertyPathSpec` is `#[non_exhaustive]`**: External code constructing `FtsPropertyPathSpec` via struct literal will fail to compile. Use `FtsPropertyPathSpec::scalar(path)` or `FtsPropertyPathSpec::recursive(path)` constructors instead.
- **`SearchHit.snippet` is unstable**: The content and format of the `snippet` field may change between releases without notice. Do not parse, split, or regex-match snippet substrings in application code.

### Added

- Per-kind FTS5 tables (`fts_props_<kind>`) replacing the global `fts_node_properties` table; created at kind-registration time (migration 21) with async rebuilds enqueued automatically.
- `projection_profiles` table for future per-kind tokenizer and embedding configuration (migration 20); empty in 0.4.2.
- `FtsPropertyPathSpec::with_weight(f32)` for per-column BM25 weight configuration; title matches (high weight) outrank body matches (low weight) in search results.
- `matched_paths` attribution populated for property FTS hits in `SearchHit.attribution.matched_paths`.

### Fixed

- `recover_interrupted_rebuilds` no longer marks PENDING rebuild rows as FAILED on engine restart; PENDING rows now survive restarts and are processed by `RebuildActor`.

## [0.4.1] - 2026-04-15

### New

- **Grouped expand on `SearchBuilder`**: `.search(...).expand(slot, direction, label, max_depth).execute_grouped()` chain now works end-to-end in Rust, Python, and TypeScript. Each call to `.expand()` declares a named slot; `execute_grouped()` returns `GroupedQueryRows` with `roots` (base search hits) and per-slot `expansions` (per-root traversal results).

- **Target-side filter on `.expand()`**: `.expand(..., filter=...)` accepts the same predicate grammar as main-path filters, including named fused-JSON filters registered via property-FTS schemas. Filtering runs before the per-originator limit, so the limit counts matching nodes only. Fused filters raise `BuilderValidationError::MissingPropertyFtsSchema` at builder time if the target kind has no registered schema.

- **Async property-FTS rebuild**: `register_fts_property_schema_async` (Python: `admin.register_fts_property_schema_async`, TypeScript: `admin.registerFtsPropertySchemaAsync`) registers the schema and returns immediately; the FTS rebuild runs in a background thread via `RebuildActor`. Poll `get_rebuild_progress` / `getRebuildProgress` to observe state (`PENDING → BUILDING → SWAPPING → COMPLETE`). The existing `register_fts_property_schema` continues to run the rebuild synchronously (eager mode).

### Behavior change

- **Async-default FTS rebuild**: after `register_fts_property_schema_async`, the new schema is **not immediately visible to search**. Search reads from the live FTS table until the rebuild reaches `COMPLETE`. Callers that need synchronous visibility should use `register_fts_property_schema` (eager mode).

- **Interrupted rebuild recovery**: if the engine restarts during a rebuild, the in-progress state is marked `FAILED` on next open. Call `register_fts_property_schema_async` again to retry.

### Sharp edge

- **Same-kind self-expand at `max_depth > 1`**: fathomdb uses per-path visited-node deduplication. Cycles in the edge graph are safe (the root node is pre-seeded as visited, so no walk loops back to the originator). `max_depth = 1` is unaffected and does not involve cycle detection.

### Per-slot result order

- The order of nodes within an expansion slot is **explicitly undefined**. Callers that need ordering must sort client-side. This contract was always true; 0.4.1 documents it explicitly.

## [0.4.0] - 2026-04-14

This is a substantial minor release. The headline items are a PyO3 0.23 →
0.28 upgrade that clears RUSTSEC-2025-0020, a breaking redesign of vector
regeneration that establishes the embedder as the sole source of vector
identity, and a new named fused-JSON-filter surface on `SearchBuilder` that
pushes `json_extract` predicates into the inner search CTE. 0.4.0 also
delivers per-session `TMPDIR` routing for all CI jobs (GH #40), a GitHub
Actions runtime refresh ahead of the September 2026 Node 20 sunset, and a
round of clippy and flake cleanup. Consumers of the Go `fathom-integrity`
vector-regeneration wrapper and of `regenerate_vector_embeddings_with_policy`
on the Python `AdminClient` need to migrate — see **Breaking changes**.

### Security

- **PyO3 0.23 → 0.28** (resolves GH #39 / RUSTSEC-2025-0020). Mechanical
  rename pass across the Python bindings: `Python::with_gil` →
  `Python::attach`, `py.allow_threads` → `py.detach`,
  `PyResult<PyObject>` → `PyResult<Py<PyAny>>`. The `pymodule` is
  explicitly marked `#[pymodule(gil_used = true)]`, preserving the
  single-GIL invariants the bindings rely on; PyO3 0.28's free-threaded
  Python support is deferred to a later release. `pyo3-log` bumped
  0.12 → 0.13 and `maturin` requirement relaxed from `>=1.8` to `>=1.9`
  to match PyO3 0.28. The `cargo-audit` ignore for RUSTSEC-2025-0020
  is removed from the repo — `cargo audit` now runs clean against the
  PyO3 advisory surface.

### Breaking changes

- **Vector regeneration takes an embedder, not an identity-bearing
  config.** `VectorRegenerationConfig` no longer accepts
  `model_identity`, `model_version`, `dimension`, `normalization_policy`,
  or `generator_command`. Existing configs that carry any of these fields
  fail at deserialization with a clear serde error. `Engine::regenerate_vector_embeddings(config)`
  reads the open-time embedder from the coordinator and returns
  `EngineError::EmbedderNotConfigured` when `Engine::open` was called with
  `embedder=None`. `AdminService::regenerate_vector_embeddings(embedder, config)`
  now takes `&dyn QueryEmbedder` explicitly. The subprocess-generator
  pattern is removed from fathomdb proper; clients that need subprocess
  regeneration should implement a `SubprocessEmbedder` adapter behind
  the `QueryEmbedder` trait.
- **`AdminClient.regenerate_vector_embeddings_with_policy` is removed**
  from the Python SDK. Callers regenerate by opening the engine with
  an embedder configured (`Engine.open(..., embedder="builtin")`) and
  invoking the new embedder-tethered surface.
- **Go `fathom-integrity` vector-regeneration wrapper is removed.** The
  bridge protocol cannot pass an embedder reference across the Go ↔
  Rust boundary, so the wrapper has no working shape under the new
  invariant. Future Go integrations either shell out to a Rust harness
  or implement Go-side embedder integration.

### Architectural changes

- **New invariant: vector identity is the embedder's responsibility, not
  the regeneration config's.** Documented at
  `dev/notes/project-vector-identity-invariant.md`. Future PRs that
  reintroduce identity strings onto vector configs will be rejected on
  review. The motivation is to eliminate the class of bugs where a
  regeneration config and the live embedder disagree on model identity
  and silently write mismatched vectors.
- **New `BuilderValidationError`** type in `fathomdb-query::builder` with
  three variants: `MissingPropertyFtsSchema`, `PathNotIndexed`, and
  `KindRequiredForFusion`. This is the canonical fail-loud error for
  fused-filter misuse — callers that try to fuse a filter on an
  unindexed path or an unkinded builder get a typed error at
  filter-add time instead of a silently degraded query.

### New features

- **Named fused JSON filters on `SearchBuilder`.** New methods on the
  five tethered builders (`NodeQueryBuilder`, `TextSearchBuilder`,
  `FallbackSearchBuilder`, `VectorSearchBuilder`, and `SearchBuilder`
  itself):
    - `filter_json_fused_text_eq(path, value)`
    - `filter_json_fused_timestamp_gt(path, value)`
    - `filter_json_fused_timestamp_gte(path, value)`
    - `filter_json_fused_timestamp_lt(path, value)`
    - `filter_json_fused_timestamp_lte(path, value)`
  These push `json_extract` into the inner search CTE's WHERE clause so
  the `limit` applies **after** the predicate, eliminating the
  small-limit-returns-zero trap documented for the post-filter
  `filter_json_*` family. Each method raises `BuilderValidationError`
  at filter-add time if the node kind has no registered property-FTS
  schema or if the requested path is not in the schema's include list
  — there is no silent degrade. The post-filter `filter_json_*` family
  is unchanged and remains available for ad-hoc predicates on
  non-indexed paths. Mirrored into the Python and TypeScript bindings
  with the same validation semantics.

### Improvements

- **Per-session `TMPDIR` routing for all CI jobs (GH #40).** The Rust,
  Python, Go, and TypeScript test jobs across `ci.yml`, `typescript.yml`,
  and `benchmark-and-robustness.yml` now route temporary files through
  `${{ runner.temp }}/fathomdb-{run_id}-{attempt}`. Linux and macOS use
  `TMPDIR`; Windows uses `TMP` + `TEMP`. Cleanup is a single `rm -rf` at
  job end. The TS sdk-harness `tempDbPath()` helper no longer hardcodes
  `/tmp` — it uses `os.tmpdir()` and inherits the session dir. The
  cross-language `orchestrate.sh` script exports `TMPDIR=$TMP` after
  creating its session-scoped temp directory so spawned subprocesses
  inherit the routing.
- **GitHub Actions runtime refresh** ahead of GitHub's September 2026
  sunset of Node 20 on hosted runners. Bumped `actions/setup-node`,
  `PyO3/maturin-action`, and `pypa/gh-action-pypi-publish` to releases
  declaring `runs.using: node24`. (The `pypa/gh-action-pypi-publish`
  bump is hygiene only — it's a composite action with no Node runtime
  and is not actually subject to the deprecation.)
- **Clippy cleanup under `--features node`** in `node.rs` and
  `node_types.rs`: narrow `#[allow]` attributes with napi-contract
  comments, plus cfg-gating `PyVectorRegenerationReport` on
  `feature = "python"` to eliminate the dead-code warning when building
  with `--features node`. Also added the
  `EngineError::EmbedderNotConfigured` arm to the napi error mapper
  that Pack #7 missed on the node feature path.
- **Clippy cleanup under `--features sqlite-vec`** in `projection.rs`,
  `sqlite.rs`, and `writer.rs`: resolved five lints
  (`needless_raw_string_hashes`, two `missing_transmute_annotations`,
  `doc_markdown`, `too_many_lines`).
- **`verify-release-gates.py`** now accepts short SHAs (≥7 chars) as
  well as full 40-char SHAs, matching typical human-copied hashes from
  `git log --oneline`. Includes regression tests for both the green
  path and the too-short-SHA `ValueError`.
- **`scripts/preflight.sh` and `preflight-CI.sh`** check for
  `cargo-audit` availability and surface the canonical
  `cargo install cargo-audit --locked` install hint — `preflight.sh`
  warns, `preflight-CI.sh` hard-fails. `preflight-CI.sh` also resolves
  its git-hooks path via `git rev-parse --git-common-dir` so it works
  from worktrees as well as the main checkout.

### Bug fixes

- **Python feedback slow-heartbeat test flake on macOS.** Widened the
  timing margin in `test_python_feedback_emits_slow_and_heartbeat_for_slow_operation`
  from 50 ms to 200 ms to give 4× headroom on slow CI runners. Eliminates
  the intermittent HEARTBEAT-phase miss observed on macOS.

### Tests / CI

- `dev/notes/release-checklist.md` updated to mark Flake A (bulk-run
  `vec_nodes_active`, fixed in 0.3.1 via commit `5ae82d7`) and the
  macOS heartbeat flake as resolved.

### Removed

- `crates/fathomdb-engine/src/executable_trust.rs` and
  `go/fathom-integrity/internal/commands/vector_regeneration.go` — the
  task #7 and #7b implementers emptied these as part of the vector
  regeneration redesign; the 0.4.0 cleanup pass `git rm`s them
  properly.

### Notes (pre-existing tech debt, disclosed for transparency)

- `cargo audit` reports two pre-existing allowed-warnings advisories
  for unmaintained transitive crates: `paste` (RUSTSEC-2024-0436) and
  `rand` (RUSTSEC-2026-0097). Both are `unmaintained` advisories, not
  vulnerabilities, and do not block the audit gate — they are visible
  in audit output.
- The Go `fathom-integrity` test suite has seven pre-existing
  environmental failures (four in `test/e2e/recover_test.go`, three in
  `internal/commands/repair_test.go`) caused by older `sqlite3` CLI
  binaries missing the `unixepoch()` function. Environmental, not a
  code bug; newer SQLite versions resolve these.
- The TypeScript workspace has no lint tooling (ESLint or Biome)
  configured. Future consideration; out of scope for 0.4.0.

## [0.3.1] - 2026-04-13

This release is a docs-and-hardening fast-follow on top of 0.3.0, plus one
load-bearing bug fix in the recursive property-FTS walker. No API surface,
schema, or wire-format changes.

### Fixed

- Property-FTS recursive walker no longer crashes on payloads that mix
  empty and non-empty string leaves. Previously, writing a node whose
  recursive property-FTS payload contained a zero-length JSON string
  followed by a non-empty string in the same traversal frame would fail
  with a `UNIQUE constraint failed` error against
  `fts_node_property_positions` and roll back the transaction. Affected
  shapes include arrays such as `{"xs": ["", "x"]}`, sibling object keys
  such as `{"a": "", "b": "x"}`, and any nested combination of the two
  (for example `{"inner": {"a": "", "b": "x"}}` or
  `{"a": "", "b": {"c": "x"}}`). Empty string leaves are now skipped at
  extraction time. All-empty payloads (such as `{"xs": ["", ""]}`)
  continue to produce no FTS row, and `null` leaves continue to be
  ignored as before. No schema or API change; existing databases benefit
  immediately on upgrade. No rebuild is required because the bug only
  affected writes that previously failed — there is no corrupt
  persisted state to repair.

### Documentation

- Added a `Reranking SearchRows.hits` recipe to `docs/guides/querying.md`
  showing how callers apply recency decay, pinning, and reputation
  weights on top of the block-ordered output of `search()`. The recipe
  is intentionally docs-only (no shipped `fathomdb.rerank` module) —
  ranking policy remains a caller concern.
- Added `docs/guides/content-refs.md`, a standalone guide to
  externalizing large node payloads via `content_ref` so indexable
  metadata stays on the node and bulky audit payloads live behind the
  ref. Cross-links to the existing `writing-data.md` "External content
  nodes" section for the write-side mechanics.
- Added `docs/guides/operational-queries.md` covering
  `engine.admin.read_operational_collection()` end-to-end, including
  the `OperationalFilterMode` variants (EXACT, PREFIX, RANGE) and
  worked examples for each. Completes query-surface coverage across
  all node/edge/chunk and operational substrates.
- Added a prominent warning in `docs/guides/querying.md` and
  `docs/reference/query.md` explaining that the `filter_json_*`
  methods on `SearchBuilder` run as post-filters over the candidate
  set selected by the search CTE. A small `limit` can silently return
  zero hits when post-filters eliminate every candidate. The callout
  documents the over-fetch idiom for composing `filter_json_*` with
  small-limit searches safely.

### Internal

- Pinned the "unsupported text-query syntax stays literal" grammar
  contract with explicit regression tests in
  `crates/fathomdb-query/src/text_query.rs`. The contract — lowercase
  `or`/`not` → literal, clause-leading `NOT` → literal, and unsupported
  FTS5 syntax → literal — is load-bearing for agent callers that pipe
  raw user chat messages directly into `search()` without a
  sanitization layer. The tests are now tagged so future refactors
  cannot silently erode the property.

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

[Unreleased]: https://github.com/coreyt/fathomdb/compare/v0.4.5...HEAD
[0.4.5]: https://github.com/coreyt/fathomdb/compare/v0.4.2...v0.4.5
[0.4.2]: https://github.com/coreyt/fathomdb/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/coreyt/fathomdb/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/coreyt/fathomdb/compare/v0.3.1...v0.4.0
[0.2.5]: https://github.com/coreyt/fathomdb/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/coreyt/fathomdb/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/coreyt/fathomdb/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/coreyt/fathomdb/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/coreyt/fathomdb/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/coreyt/fathomdb/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/coreyt/fathomdb/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/coreyt/fathomdb/releases/tag/v0.1.0
