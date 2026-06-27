# Distilled technical summary — fathomdb-py binding + STATUS-0.8.9

> Written from held memory (the raw `Cargo.toml`, `src/lib.rs`, and `STATUS-0.8.9.md`
> were read in full earlier this session and are NOT re-read for this distillation).
> Dense on names/values, terse on prose. Scope: the PyO3 binding crate
> `fathomdb-py` (the Python SDK FFI to `fathomdb-engine`) plus the 0.8.9 CI-integrity
> status board.

---

## 1. Crate manifest — `src/rust/crates/fathomdb-py/Cargo.toml`

### `[package]`
- `name = "fathomdb-py"`
- `version.workspace = true`
- `edition.workspace = true`
- `license.workspace = true`
- `repository.workspace = true`
- `rust-version.workspace = true`
- `publish = false` (never published to crates.io; the wheel is the deliverable)

### `[lib]`
- `name = "fathomdb_py"`
- `crate-type = ["cdylib", "rlib"]` — cdylib for the Python extension module, rlib so
  the crate can be unit-tested / linked from Rust.

### `[features]` — SIX flags (NOT five)
1. `default = []` — empty default set; release wheels build with explicit `--features`
   lists (CI/release pin their own), so `default` carries nothing.
2. `test-hooks = []` — gates the test-only FFI seams; **this is the single feature
   referenced via a `cfg` attribute inside `lib.rs`** (`#[cfg(any(test, feature =
   "test-hooks"))]`). The other four features only forward to the engine in
   Cargo.toml and are never named in a `lib.rs` cfg.
3. `default-embedder = ["fathomdb-engine/default-embedder"]` — EU-6: forwards the
   engine's default-embedder feature so `use_default_embedder=True` materializes a real
   `CandleBgeEmbedder` instead of failing with a typed Embedder error. No-feature wheels
   still expose the kwarg but reject `True` at open time (no silent network) per
   ADR-0.7.1-default-embedder-weight-fetch.
4. `default-reranker = ["fathomdb-engine/default-reranker"]` — 0.8.2 Slice E2 fix-1
   [P1]: forwards the engine's default-reranker so a plain `maturin develop` / `pytest`
   activates the real CE reranker in dev/test. Shipped wheel keeps `default-reranker`
   OFF (release.yml + ci.yml pin explicit `--features` lists omitting it).
5. `embed-cuda = ["default-embedder", "fathomdb-engine/embed-cuda"]` — opt-in GPU
   (CUDA); forwards to engine; default wheel stays CPU. Build:
   `maturin develop --features pyo3/extension-module,embed-cuda`; activate via
   `FATHOMDB_EMBED_DEVICE=cuda`. See dev/design/0.8.1-embedder-gpu-and-portability.md.
6. `embed-metal = ["default-embedder", "fathomdb-engine/embed-metal"]` — opt-in GPU
   (Apple Metal); same forwarding pattern.

### `[dependencies]`
- `fathomdb-embedder.workspace = true`
- `fathomdb-embedder-api.workspace = true`
- `fathomdb-engine.workspace = true`
- `fathomdb-schema.workspace = true`
- `pyo3 = { version = "0.29", features = ["extension-module", "abi3-py310"] }`
  — **pyo3 version pin = "0.29"**, features `extension-module` + `abi3-py310`
  (stable ABI targeting Python 3.10+).
- `serde_json = "1"`

---

## 2. `src/lib.rs` — overview (1489 lines, held in full)

PyO3 binding from the Python SDK to `fathomdb-engine`. Crate-level attributes:
- `#![allow(unexpected_cfgs)]` — pyo3's `create_exception!`/`#[pymodule]` macros emit
  `#[cfg(feature = "gil-refs")]` arms referencing a feature this crate doesn't export;
  the resulting `unexpected_cfgs` warnings are noise on a clippy `-D warnings` gate.
- `#![allow(clippy::useless_conversion)]` — covers `#[pymethods]`-generated PyResult
  wrappers flagged as redundant `Into<PyErr>` calls.

### FFI safety contract (module doc; mirrored by Phase 11b napi-rs)
1. Every potentially-blocking engine call wraps in `py.detach(...)` to release the GIL.
2. Engine entry points return typed errors via `engine_error_to_py` /
   `engine_open_error_to_py` — single-switch mapping, **no catch-all arm**; the binding
   fails to compile when the Rust variant set drifts from the Python class set (AC-060a).
3. Every string crossing the FFI is checked by `validate_ffi_string` for embedded NUL or
   unpaired UTF-16 surrogates BEFORE the writer transaction opens (AC-068a / AC-068b).
4. Panics inside engine code surface as Python `PanicException`
   (`pyo3::panic::PanicException`); host process is not aborted (AC-067). Engine calls are
   `catch_unwind`-wrapped so the panic is translated Rust-side. PanicException is
   intentionally NOT an `EngineError` subclass: panic = contract bug, not a typed outcome.

### Key imports
- `std::panic::{catch_unwind, AssertUnwindSafe}`, `std::sync::Arc`
- `fathomdb_embedder::EmbedderEvent as RustEmbedderEvent`
- `fathomdb_embedder_api::EmbedderIdentity as RustEmbedderIdentity`
- From `fathomdb_engine`: `rerank_passages as rust_rerank_passages`,
  `ComparisonOp as RustComparisonOp`, `CorruptionDetail`, `CorruptionKind`,
  `EmbedderChoice`, `Engine as RustEngine`, `EngineError as RustEngineError`,
  `EngineOpenError`, `ExtractDocument as RustExtractDocument`,
  `IngestWithExtractorReceipt as RustIngestWithExtractorReceipt`,
  `NodeRecord as RustNodeRecord`, `OpStoreRow as RustOpStoreRow`,
  `OpenReport as RustOpenReport`, `OpenStage`, `Predicate as RustPredicate`,
  `PreparedWrite`, `ScalarValue as RustScalarValue`,
  `SearchExpandResult as RustSearchExpandResult`, `SearchFilter as RustSearchFilter`,
  `SearchHit as RustSearchHit`, `SearchResult as RustSearchResult`,
  `SoftFallback as RustSoftFallback`, `SoftFallbackBranch`,
  `TraversalDirection as RustTraversalDirection`, `WriteReceipt as RustWriteReceipt`.
- `fathomdb_schema::MigrationStepReport as RustMigrationStepReport`
- pyo3: `create_exception`, exceptions `{PyException, PyTypeError, PyValueError}`,
  `panic::PanicException`, `prelude::*`, `types::{PyDict, PyList}`.

---

## 3. Exception hierarchy (the `create_exception!` matrix)

Module name in `create_exception!` = `_fathomdb`. Root inherits from Python `Exception`
via `PyException`. Per dev/design/errors.md § Binding-facing class matrix.

### Root
- **`EngineError`** ← `PyException` (the ROOT typed exception; all concrete leaves
  inherit from it).

### Direct children of `EngineError` (one level deep)
- `StorageError`
- `ProjectionError`
- `VectorError`
- `EmbedderError`
- `SchedulerError`
- `OpStoreError`
- `WriteValidationError`
- `SchemaValidationError`
- `OverloadedError`
- `ClosingError`
- `DatabaseLockedError`
- `CorruptionError`
- `IncompatibleSchemaVersionError`
- `MigrationError`
- `EmbedderIdentityMismatchError`
- `EmbedderDimensionMismatchError`
- `ExtractorError` — G11 (Slice 15) BYO-LLM extraction harness protocol error.
- `InvalidFilterError` — G4 (Slice 35) filter predicate construction error
  (non-allowlisted path).
- `InvalidArgumentError` — Slice 20 (G5/G6) traversal depth > 3 / out-of-range arg.

### Two-level-deep leaves (nested under a non-root leaf)
- **`KindNotVectorIndexedError`** ← `VectorError` ← `EngineError`
- **`EmbedderNotConfiguredError`** ← `EmbedderError` ← `EngineError`

### NOT in the hierarchy
- `PanicException` (pyo3 built-in) — deliberately NOT an `EngineError` subclass.

### Module registration (`m.add(...)`) — every exception added by name
`EngineError`, `StorageError`, `ProjectionError`, `VectorError`,
`KindNotVectorIndexedError`, `EmbedderError`, `EmbedderNotConfiguredError`,
`SchedulerError`, `OpStoreError`, `WriteValidationError`, `SchemaValidationError`,
`OverloadedError`, `ClosingError`, `DatabaseLockedError`, `CorruptionError`,
`IncompatibleSchemaVersionError`, `MigrationError`, `EmbedderIdentityMismatchError`,
`EmbedderDimensionMismatchError`, `ExtractorError`, `InvalidFilterError`,
`InvalidArgumentError`.

---

## 4. String validation (AC-068a / AC-068b)

- `pub fn validate_ffi_string(value: &str) -> Result<(), String>` — rejects embedded NUL
  byte ("embedded NUL byte in FFI string") and any codepoint in `0xD800..=0xDFFF`
  ("unpaired UTF-16 surrogate U+{cp:04X} in FFI string"). Both are valid Python `str` but
  invalid for SQLite text columns; rejected BEFORE the writer tx opens (no-row-written).
- `fn validate_ffi_string_py(value: &str) -> PyResult<()>` — maps the error to
  `WriteValidationError`.
- `fn extract_validated_str(value: &Bound<'_, PyAny>) -> PyResult<String>` — extract
  Python str → Rust String + validate; lone-surrogate extraction failure re-raised as
  `WriteValidationError` ("string contains characters not representable as UTF-8 (lone
  surrogate)").
- `fn extract_opt_validated_str(value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<String>>`
  — Option lift: `None`/None-valued stays `None` (preserves all-`None` byte-identical
  unfiltered path); present value validated through the same gate. Used by `search` for
  the G10 `SearchFilter` string fields.

---

## 5. Error mapping functions (no catch-all)

### `fn engine_error_to_py(err: RustEngineError) -> PyErr`
Exhaustive match — drift = compile error. Arms:
- `Storage` → `StorageError("storage error")`
- `Projection` → `ProjectionError("projection error")`
- `Vector` → `VectorError("vector error")`
- `Embedder` → `EmbedderError("embedder error")`
- `EmbedderNotConfigured` → `EmbedderNotConfiguredError("embedder is not configured")`
- `KindNotVectorIndexed` → `KindNotVectorIndexedError("kind is not configured for vector indexing")`
- `EmbedderDimensionMismatch { expected, actual }` → `EmbedderDimensionMismatchError`,
  sets attrs `stored=expected`, `supplied=actual`.
- `Scheduler` → `SchedulerError`
- `OpStore` → `OpStoreError`
- `WriteValidation` → `WriteValidationError`
- `SchemaValidation` → `SchemaValidationError`
- `Overloaded` → `OverloadedError("engine overloaded")`
- `Closing` → `ClosingError("engine is closing")`
- `Extractor` → `ExtractorError("extractor error")`
- `InvalidFilter { reason }` → `InvalidFilterError("invalid filter: {reason}")`
- `InvalidArgument { msg }` → `InvalidArgumentError(msg)`

### `fn corruption_kind_str(kind: CorruptionKind) -> &'static str`
- `WalReplayFailure` → "WalReplayFailure"
- `HeaderMalformed` → "HeaderMalformed"
- `SchemaInconsistent` → "SchemaInconsistent"
- `EmbedderIdentityDrift` → "EmbedderIdentityDrift"

### `fn open_stage_str(stage: OpenStage) -> &'static str`
- `HeaderProbe` → "HeaderProbe"
- `WalReplay` → "WalReplay"
- `SchemaProbe` → "SchemaProbe"
- `EmbedderIdentity` → "EmbedderIdentity"

### `fn engine_open_error_to_py(err: EngineOpenError) -> PyErr`
- `DatabaseLocked { holder_pid }` → `DatabaseLockedError`; message varies on
  `Some(pid)`/`None`; sets attr `holder_pid`.
- `Corruption(detail)` → `corruption_to_py(detail)`
- `IncompatibleSchemaVersion { seen, supported }` → `IncompatibleSchemaVersionError`
- `MigrationError { schema_version_before, schema_version_current, step_id }` →
  `MigrationError`
- `EmbedderIdentityMismatch { stored, supplied }` → `EmbedderIdentityMismatchError`; sets
  attrs `stored_name`, `stored_revision`, `supplied_name`, `supplied_revision`.
- `EmbedderDimensionMismatch { stored, supplied }` → `EmbedderDimensionMismatchError`;
  sets attrs `stored`, `supplied`.
- `Embedder(err)` → `EmbedderError("{err:?}")`
- `Io { message }` → `StorageError("database I/O error: {message}")`

### `fn corruption_to_py(detail: CorruptionDetail) -> PyErr`
→ `CorruptionError("corruption {kind} at stage {stage} ({recovery_hint_code})")`; sets
attrs `kind`, `stage`, `recovery_hint_code`, `doc_anchor`.

### `fn call_engine<R: Send>(py, f: impl FnOnce() -> Result<R, RustEngineError> + Send) -> PyResult<R>`
Wraps `f` in `AssertUnwindSafe`, runs inside `py.detach(|| catch_unwind(wrapped))`.
- `Ok(Ok(value))` → `Ok(value)`
- `Ok(Err(err))` → `Err(engine_error_to_py(err))`
- `Err(_)` → `Err(PanicException::new_err("engine panic (see logs)"))`
`AssertUnwindSafe` used because the engine's `Arc<dyn Embedder>` makes the natural
`UnwindSafe` bound unsatisfiable.

---

## 6. `#[pyclass]` data types (all `module = "fathomdb._fathomdb"` unless noted)

Most are `frozen, get_all, skip_from_py_object` + `#[derive(Clone)]`, with a
`from_rust` constructor.

1. **`PyWriteReceipt`** (name `WriteReceipt`) — fields: `cursor: u64`,
   `row_cursors: Vec<u64>` (G0 Slice 15, 1:1 with input batch order),
   `dangling_edge_endpoints: u64` (G8 Slice 20, flag-and-count). `from_rust(RustWriteReceipt)`.
2. **`PyIngestWithExtractorReceipt`** (name `IngestWithExtractorReceipt`, G11 Slice 15) —
   `nodes_written: u64`, `edges_written: u64`, `docs_processed: u64`.
3. **`PySoftFallback`** (name `SoftFallback`) — `branch: String`. `from_rust(&RustSoftFallback)`
   maps `SoftFallbackBranch::{Vector→"vector", Text→"text", TextEdge→"text_edge",
   GraphArm→"graph_arm"}`.
4. **`PySearchHit`** (name `SearchHit`) — `id: u64`, `kind: String`, `body: String`,
   `score: f64`, `branch: String`, `source_id: Option<String>` (G0 Phase-2 provenance; Some
   only for graph-arm hits), `ce_score: Option<f64>` (0.8.5 EXP-0;
   `ce_norm = sigmoid(ce_logit)`; Some only inside the reranked pool).
5. **`PySearchResult`** (name `SearchResult`) — `projection_cursor: u64`,
   `soft_fallback: Option<PySoftFallback>`, `results: Vec<PySearchHit>`.
6. **`PyNodeRecord`** (name `NodeRecord`) — `logical_id: String`, `kind: String`,
   `body: String`, `write_cursor: u64`.
7. **`PyOpStoreRow`** (name `OpStoreRow`) — `id: i64`, `collection: String`,
   `record_key: String`, `op_kind: String`, `payload: String`, `schema_id: Option<String>`,
   `write_cursor: u64`.
8. **`PyCounterSnapshot`** (name `CounterSnapshot`) — `queries: u64`, `writes: u64`,
   `write_rows: u64`, `admin_ops: u64`, `cache_hit: u64`, `cache_miss: u64`.
9. **`PyMigrationStepReport`** (name `MigrationStepReport`) — `step_id: u32`,
   `duration_ms: Option<u64>`, `failed: bool`.
10. **`PyEmbedderIdentity`** (name `EmbedderIdentity`) — `name: String`,
    `revision: String`, `dimension: u32`.
11. **`PyOpenReport`** (name `OpenReport`, `frozen, get_all` — NO skip_from_py_object) —
    `schema_version_before: u32`, `schema_version_after: u32`,
    `migration_steps: Vec<PyMigrationStepReport>`, `embedder_warmup_ms: u64`,
    `query_backend: String`, `default_embedder: PyEmbedderIdentity`,
    `embedder_download_ms: Option<u64>` (EU-5a1/5a2/5b),
    `embedder_events: Vec<Py<PyAny>>` (dicts keyed by "kind"),
    `embedder_mean_centering_required: bool` (static capability — true for bge-small),
    `embedder_mean_vec_pinned: bool` (dynamic; true iff
    `_fathomdb_embedder_profiles.mean_vec IS NOT NULL`). `from_rust(py, &RustOpenReport)`.
12. **`PyExpandedNode`** (name `ExpandedNode`, `skip_from_py_object`, Slice 20) — fields
    with `#[pyo3(get)]`: `node: PyNodeRecord`, `hop_count: u32`.
13. **`PySearchExpandResult`** (name `SearchExpandResult`, `skip_from_py_object`,
    Slice 20 G6) — `#[pyo3(get)]`: `search_hits: Vec<PySearchHit>` (original RRF-scored
    hits), `expanded: Vec<PyExpandedNode>` (nodes reachable by traversal not in
    search_hits), `all_logical_ids: Vec<String>` (deduplicated union).
14. **`PyEngine`** (name `Engine`) — fields `inner: Arc<RustEngine>`,
    `open_report: Arc<RustOpenReport>`.

### `embedder_event_to_py(py, ev: &RustEmbedderEvent) -> Py<PyAny>`
Serializes one event as a dict keyed by `"kind"`:
- `DefaultEmbedderDownload { file, url, bytes, sha256, cache_path, duration_ms }` →
  kind "DefaultEmbedderDownload" + those keys (cache_path via `.display().to_string()`).
- `DefaultEmbedderCacheHit { file, sha256, cache_path }` → kind "DefaultEmbedderCacheHit".
- `MeanVecPinned { dim, doc_count }` → kind "MeanVecPinned".
- `MeanVecRecomputed { dim, doc_count, trigger }` → kind "MeanVecRecomputed"
  (`trigger.as_str()`).

---

## 7. `PyEngine` methods (`#[pymethods] impl PyEngine`)

1. **`open`** — `#[staticmethod]`, `#[pyo3(signature = (path, use_default_embedder = false))]`.
   Validates path; `py.detach` + `catch_unwind` around `RustEngine::open_with_choice(path,
   choice)`; choice = `EmbedderChoice::Default` if `use_default_embedder` else
   `EmbedderChoice::None` (EU-6; caller-supplied custom embedders deferred per
   ADR-0.6.0-embedder-protocol Invariant 3). Panic → `PanicException("engine panic during
   open")`; open error → `engine_open_error_to_py`. Returns `Self { inner, open_report }`.
2. **`open_report`** — `(&self, py) -> PyOpenReport` via `PyOpenReport::from_rust`.
3. **`write`** — `(&self, py, batch: Bound<PyList>) -> PyResult<PyWriteReceipt>`.
   `translate_batch` → `engine.write(&prepared)` via `call_engine`.
4. **`search`** — `#[allow(clippy::too_many_arguments)]`,
   signature `(query, source_type=None, kind=None, created_after=None, status=None,
   rerank_depth=0, use_graph_arm=false, alpha=None, pool_n=None)`. G10 + 0.8.1 R1/R3 +
   0.8.5 EXP-0. Validates query + filter strings via `extract_opt_validated_str`. Builds
   `RustSearchFilter { source_type, kind, created_after, status }` only if any present
   (else `None` = byte-identical unfiltered). Binding-side defaults: `alpha =
   alpha.unwrap_or(0.3)`, `pool_n = pool_n.unwrap_or(rerank_depth)`. Calls
   `engine.search_reranked(&query, filter, rerank_depth, use_graph_arm, alpha, pool_n)`.
   `rerank_depth=0` = identity/soft-fallback; `>0` = CE rerank. `use_graph_arm=true` =
   BFS third RRF arm seeded from top-10 fused hits over temporal fact-edges.
   `alpha=1.0, pool_n=10` = measured-parity config.
5. **`close`** — `(&self, py) -> PyResult<()>` → `engine.close()`.
6. **`drain`** — `#[pyo3(signature = (timeout_s = 0.0))]`; ms =
   `(timeout_s*1000) as u64` if finite & >0 else 0; → `engine.drain(ms)`.
7. **`ingest_with_extractor`** — `(&self, py, cmd: Bound<PyList>, documents: Bound<PyList>)
   -> PyResult<PyIngestWithExtractorReceipt>` (G11 Slice 15). cmd → `Vec<String>` (argv;
   non-string → `WriteValidationError("cmd elements must be strings")`); documents → dicts
   with required `source_doc_id` + `body` → `Vec<RustExtractDocument>`; →
   `engine.ingest_with_extractor(&cmd_refs, &docs)`.
8. **`counters`** — `(&self) -> PyCounterSnapshot` from `self.inner.counters()`.
9. **`set_profiling`** — `(&self, enabled: bool) -> PyResult<()>` →
   `self.inner.set_profiling(enabled)` mapped via `engine_error_to_py`.
10. **`set_slow_threshold_ms`** — `(&self, value: u64) -> PyResult<()>` →
    `self.inner.set_slow_threshold_ms(value)`.
11. **`embed`** — `(&self, py, text: &str) -> PyResult<Vec<f32>>`. Embeds with the pinned
    default embedder `fathomdb-bge-small-en-v1.5`; raises `EmbedderNotConfiguredError` if
    opened without an embedder. Validates text; → `engine.embed_text(&text)`.
12. **`_configure_vector_kind_for_test`** — `#[cfg(any(test, feature = "test-hooks"))]`.
    EU-6 test-only vector write seam; → `engine.configure_vector_kind_for_test(&kind)`.
13. **`_write_vector_for_test`** — `#[cfg(any(test, feature = "test-hooks"))]`; →
    `engine.write_vector_for_test(&kind, &text)`.
14. **`attach_logging_subscriber`** — `#[pyo3(signature = (logger,
    heartbeat_interval_ms = None))]`. No-op stub today (subscriber wiring lands in a
    later 0.6.x slice); accepts the call so callers can wire a logger.

---

## 8. Standalone `#[pyfunction]`s

1. **`admin_configure`** — `#[pyo3(signature = (engine, name, body))]`. admin.configure
   sugar: validates name (non-empty) + body; builds `PreparedWrite::AdminSchema { name,
   kind: "latest_state", schema_json: body, retention_json: "{}" }`; → `inner.write(&batch)`
   → `PyWriteReceipt`.
2. **`read_get`** — `(engine, logical_id)` → `Option<PyNodeRecord>` via
   `inner.read_get(&logical_id)` (not-found = `None`, never an exception; typed NotFound is
   reserved-gap Slice 31). G2/G3 Slice 30.
3. **`read_get_many`** — `(engine, logical_ids: Bound<PyList>)` →
   `Vec<Option<PyNodeRecord>>` via `inner.read_get_many(&ids)`.
4. **`read_collection`** — `#[pyo3(signature = (engine, collection, after_id=None,
   limit=0))]` → `Vec<PyOpStoreRow>` via `read_collection_impl` →
   `inner.read_collection(&collection, after_id, limit)`. Mandatory limit + after_id cursor.
5. **`read_mutations`** — same signature; also delegates to `read_collection_impl`.
6. **`read_list`** — `#[pyo3(signature = (engine, kind, predicates=None, limit=100))]`
   (G4 Slice 35). predicates = list of dicts `{ "type": "eq"|"gt"|"gte"|"lt"|"lte",
   "path": str, "value": str|int|bool }`; `py_predicate_to_rust` builds `RustPredicate`
   (bool checked before int since Python bool ⊂ int; path validated in Rust →
   `InvalidFilterError`; unknown type → `PyValueError`); →
   `inner.read_list(&kind, &rust_predicates, limit)` → `Vec<PyNodeRecord>`.
7. **`graph_neighbors`** — `#[pyo3(signature = (engine, logical_id, depth, direction))]`
   (G5 Slice 20). depth must be 1..=3 (else `InvalidArgumentError`); direction via
   `parse_direction` ("outgoing"→Outgoing, "incoming"→Incoming, "both"→Both, else
   `InvalidArgumentError`); → `inner.graph_neighbors(&logical_id, depth, dir)`; reachable
   nodes excluding root within depth hops, hard-capped at 50.
8. **`search_expand`** — `#[allow(clippy::too_many_arguments)]`, signature `(engine, query,
   depth, source_type=None, kind=None, created_after=None, status=None)` (G6 Slice 20).
   depth 0..=3; same filter construction; → `inner.search_expand(&query, filter, depth)` →
   `PySearchExpandResult`.
9. **`rerank`** — `#[pyo3(signature = (query, passages, rerank_depth, alpha=None,
   pool_n=None))]` (0.8.2 Slice E2; standalone, NOT engine-bound). Marshals
   `[{"id": int, "body": str, "score": float}]` → `(u64, String, f64)` tuples
   (`dict_u64_required` for id, `dict_str_required` for body, `dict_f64_required` for
   score); defaults `alpha=0.3`, `pool_n=rerank_depth`; runs
   `rust_rerank_passages(&query, tuples, rerank_depth, alpha, pool_n)` inside
   `py.detach(catch_unwind(AssertUnwindSafe(...)))`; outer panic → `PanicException("rerank
   panic (see logs)")`; inner Err (non-finite score) → `WriteValidationError`. Returns
   `Vec<Py<PyDict>>` of `{"id", "score", "ce_score"}` (ce_score = None outside the reranked
   pool). Identity contract: `rerank_depth == 0` OR empty list returns input order +
   input scores (no model load, no network).
10. **`force_panic_for_test`** — `#[cfg(any(test, feature = "test-hooks"))]`. AC-067
    force-panic probe; `panic!("force_panic_for_test: AC-067 probe")`.

### Helper fns (translation / extraction)
- `translate_batch(batch) -> Vec<PreparedWrite>` (maps `translate_write_item`).
- `dict_get`, `dict_str`, `dict_str_required`, `dict_u64_required`, `dict_f64_required`.
- `translate_write_item` — dispatches on keys `edge`/`op_store`/`admin_schema`/`node`;
  bare `{"kind": ...}` treated as Node.
- `translate_node` → `PreparedWrite::Node { kind, body(default "{}"), source_id, logical_id }`.
- `translate_edge` → `PreparedWrite::Edge { kind, from, to, source_id, logical_id, body,
  t_valid, t_invalid, confidence: None, extractor_model_id: None, temporal_fallback: None }`
  (R3 Slice 30 temporal fields; body projected into `search_index_edges` for the C1 graph
  arm).
- `translate_op_store` → `PreparedWrite::OpStore { collection, record_key, schema_id, body }`.
- `translate_admin_schema` → `PreparedWrite::AdminSchema { name, kind, schema_json,
  retention_json(default "{}") }`.
- `py_predicate_to_rust`, `parse_direction`.
- `read_collection_impl`.

---

## 9. The three test-hooks-gated functions

Gated by `#[cfg(any(test, feature = "test-hooks"))]` — compiled OUT of release wheels:
1. **`_configure_vector_kind_for_test`** (PyEngine method) — EU-6 vector-kind config seam.
2. **`_write_vector_for_test`** (PyEngine method) — vector write seam (mean-vec pin
   transition end-to-end through the binding).
3. **`force_panic_for_test`** (standalone pyfunction) — AC-067 panic probe.

---

## 10. Module registration — `#[pymodule(gil_used = true)]`

**`gil_used = true`** — preserves current GIL semantics. pyo3 0.28+ makes `#[pymodule]`
free-threaded by default, but this binding is `abi3-py310` and the whole FFI contract
assumes the GIL is held. Opting into free-threading (`gil_used = false`) is a separate,
larger correctness campaign (see
dev/design/free-threaded-python-value-lift-and-experiments.md).

`fn _fathomdb(py, m: Bound<PyModule>) -> PyResult<()>`:
- `add_class`: PyEngine, PyWriteReceipt, PyIngestWithExtractorReceipt, PySoftFallback,
  PySearchHit, PySearchResult, PyCounterSnapshot, PyMigrationStepReport, PyEmbedderIdentity,
  PyOpenReport, PyNodeRecord, PyOpStoreRow, PyExpandedNode, PySearchExpandResult.
- `add_function`: admin_configure, read_get, read_get_many, read_collection,
  read_mutations, read_list, graph_neighbors, search_expand, rerank; plus
  `force_panic_for_test` under `#[cfg(any(test, feature = "test-hooks"))]`.
- `m.add(...)` for all 22 exception types (see §3).

### Rust unit tests (`#[cfg(test)] mod tests`)
- `validate_ffi_string_accepts_plain_ascii`
- `validate_ffi_string_accepts_non_ascii_utf8` ("héllo 🦀 文字")
- `validate_ffi_string_rejects_embedded_nul` (asserts diagnostic contains "NUL")
- `validate_ffi_string_rejects_lone_surrogate` (note: U+D800 can't appear in a Rust &str;
  uses U+FFFD as a valid high-unicode sanity check; exhaustive surrogate guard sits in
  Python).

---

## 11. STATUS-0.8.9.md — CI-integrity micro board (gist)

- **Plan:** `dev/plans/plan-0.8.9.md`. Footprint **$0** — CI/test-harness only; no library
  query-path change, no priced runs. Verify-from-git discipline. Opened 2026-06-27
  (`/goal complete 0.8.9`, orchestrator session).

### §0 Headline — plan was substantially stale; most of 0.8.9 already shipped
Slice 0 audited actual gate reality vs plan premises (plan written off the
`perf-recall-gates-masked-and-ac013b-conflation` memory dated 2026-06-06, the day the
defects were exposed at 0.8.0 Slice 40). Verifying from git: Slice 40 (+ later cleanup)
also fixed most of them. Honest deliverable (R-PG-1) = the gate-reality map, not a
fabricated five-slice pass.

Requirement reality table (verified 2026-06-27):
- **R-PG-2** (ac_013b off synthetic floor) — DONE @ Slice 40 (AC-075).
  `perf_gates.rs::ac_013b_recall_at_10_floor` now report-only (prints
  `RECALL_FIDELITY_INFO`, no hard assert); asserting verdict moved to
  `eu7_real_corpus_ac.rs` (real BGE, vector-stage, one-sided CI `ci_hi≥0.90`). Residue:
  cheap RED unit test on the catch predicate.
- **R-PG-3** (cheap subset runs per-push) — DONE. Devloop tier
  (`perf_gates_devloop.rs`) runs on every `cargo test --workspace`. Canonical tier =
  `AGENT_LONG` (release-only, real-embed = hours). Residue: doc the split.
- **R-037-1** (AC-037 in CI on userns-permissive runner) — DONE @ `8402e59c`. `ci.yml`
  `security` job on `ubuntu-22.04` runs `STRICT=1 agent-security.sh`
  (AC-036/037/038/050a/050c); STRICT=1 → toolchain blocker is a hard failure (no vacuous
  pass).
- **R-037-2** (demonstrate-the-catch) — OPEN. No egress fixture proves the gate trips;
  can't execute in sandbox (rootless userns unavailable); runs on `ubuntu-22.04`. Residue:
  author fixture + RED proof.
- **R-050c-1** (removal-detect baseline cleared) — DONE @ `a8304652` (0.8.0 Slice 27
  fix-1). Cause: removal-detect scoped `tests/` files into the public-surface diff +
  missing CHANGELOG operator-gate note. Fix excluded `tests/` + added note; passes on
  baseline (`base=v0.6.1`, exit 0).
- **R-DEP-3** (no mechanical auto-merge) — CONFIRMED. `allow_auto_merge=false`; no
  auto-merge workflow.
- **R-DEP-1 (npm)** — OPEN + actionable. Root `package-lock.json`: markdown-it
  14.1.1→14.2.0, js-yaml 4.1.1→4.2.0 (both transitive via `markdownlint-cli2`).
- **R-DEP-1 (pip)** — MOOT/orphaned. `python/uv.lock` archived out of tree (`39ee2712`;
  archive removed `df33207a`); idna already bumped 3.11→3.15 (`e850052d`) before removal.
  `src/python/uv.lock` carries neither idna nor torch. torch has no patched version
  (`<=2.12.0`, low-sev, eval-only).
- **R-DEP-2** (dependabot.yml coverage) — OPEN (npm only). npm root `/` uncovered (only
  `/src/ts`). pip `/python` moot (no lockfile).

Net residue (all $0): R-PG-1 consolidated gate-map table; R-PG-2 RED predicate test;
R-037-2 egress fixture + RED proof; R-DEP-1 (npm) bump + md lint; R-DEP-2 add npm `/`;
R-DEP-1 (pip) dismiss-with-rationale orphaned idna/torch (HITL-gated).

### §1 Slice board (mod-5)
- **0** Setup + audit; map gate reality — CLOSED.
- **5** Perf-gate honesty (R-PG-1/2) — CLOSED. `perf-gates.md` per-AC map;
  `recall_gate_predicate.rs` catch test (3/3 green, RED-confirmed).
- **10** AC-037 catch + AC-050c — CLOSED. shared `lib-egress-allowlist.sh`;
  `check-netns-deny-egress-catch.sh` (offline catch green + RED-confirmed, live netns
  CI-only).
- **15** Dependency hygiene (R-DEP) — CLOSED. npm overrides → markdown-it 14.2.0/js-yaml
  4.2.0, `npm audit`=0; dependabot.yml npm `/` added; pip idna/torch orphaned (dismiss
  pending HITL).
- **40** Verify + release readiness — in progress. cargo test, mkdocs, codex §9, HITL.

### §2 Cross-cutting DoD
- X1 SDK parity — no library API change (N/A by design).
- X2 `mkdocs build --strict` — keep green.
- X3 docs + DOC-INDEX — reconcile stale gate-map references.

### §2a Slice 40 verification (all local, $0)
- `cargo test -p fathomdb-engine --test perf_gates_devloop` → 3/3 green.
- `cargo test -p fathomdb-engine --test recall_gate_predicate` → 3/3 green; RED-confirmed.
- `check-netns-deny-egress-catch.sh` → PASS (offline catch flags 2 egress; live netns
  skipped — no userns).
- `agent-security.sh` battery → catch gate PASS (AC-037 live = BLOCKER in sandbox,
  expected; runs on `ubuntu-22.04`).
- `mkdocs build --strict` → exit 0.
- `npm audit` → 0 vulnerabilities (was 3 moderate); override lint-behavior-neutral; `npm
  run lint:md` is NOT a CI gate (the doc gate is `mkdocs --strict`).
- codex §9 review (`--uncommitted`) → clean PASS, 0 findings.

### §3 HITL sign-off ledger
- [x] Working-tree changes reviewed (codex §9) — clean PASS, 0 findings.
- [x] Memory reconciliation — `perf-recall-gates-masked-and-ac013b-conflation` updated.
- [x] Commit 0.8.9 residue — branch `0.8.9-ci-integrity-micro`, commit `d5a68d17`, **PR
      #93** (10 files; unrelated working-tree changes excluded).
- [x] Dismiss orphaned idna/torch alerts — HITL: leave open (documented as orphaned).
- [x] Version-bump / tag — HITL: no version bump (zero library-surface change).
- [ ] Merge PR #93 — HITL action (blocked on pre-existing CI red; see §5).

### §5 CI status on PR #93 — pre-existing red on main, NOT caused by 0.8.9
main's last 3 runs red on the SAME 4 jobs (docs-only commits):
- `verify` — fails at "Bootstrap dev tooling"; `bootstrap.sh` Python-tooling `.venv`
  install dies (~4 min → exit 1) — infra; not 0.8.9.
- `security` — same bootstrap failure; aborts BEFORE `agent-security.sh` (so the AC-037
  catch + recall test never execute in CI) — not 0.8.9.
- `rust-macos` — `cargo test --workspace` pyo3 link error (`_PyDict_GetItemWithError`,
  `_PyExc_*` undefined) — **0.8.8** (pyo3 0.24→0.29).
- `rust-windows` — same pyo3 link error — **0.8.8**.

0.8.9 adds zero failures. Every CI job that reaches the 0.8.9 changes is green:
`Analyze (rust)` (compiled `recall_gate_predicate.rs`), `docs`,
`default-embedder-tests`, `wheel-size-gate (linux-x64)`. Full PR green requires 0.8.8
(pyo3 link) + a bootstrap infra fix — both out of 0.8.9 scope.

### §4 $ ledger
$0.00 — no priced runs; CI/test-harness + lockfile work only.

---

## 12. Quick-reference index (names/values)

- pyo3 pin: **"0.29"**, features `["extension-module", "abi3-py310"]`.
- Cargo features (6): `default`, `test-hooks` (only cfg-gated one in lib.rs),
  `default-embedder`, `default-reranker`, `embed-cuda`, `embed-metal`.
- `#[pymodule(gil_used = true)]`, module fn `_fathomdb`.
- test-hooks-gated fns (3): `_configure_vector_kind_for_test`, `_write_vector_for_test`,
  `force_panic_for_test`.
- Root exception: `EngineError`. Two-level-deep: `KindNotVectorIndexedError` (←VectorError),
  `EmbedderNotConfiguredError` (←EmbedderError).
- PyEngine methods: open, open_report, write, search, close, drain,
  ingest_with_extractor, counters, set_profiling, set_slow_threshold_ms, embed,
  _configure_vector_kind_for_test, _write_vector_for_test, attach_logging_subscriber.
- Standalone pyfunctions: admin_configure, read_get, read_get_many, read_collection,
  read_mutations, read_list, graph_neighbors, search_expand, rerank, force_panic_for_test.
- pyclasses: PyEngine, PyWriteReceipt, PyIngestWithExtractorReceipt, PySoftFallback,
  PySearchHit, PySearchResult, PyNodeRecord, PyOpStoreRow, PyCounterSnapshot,
  PyMigrationStepReport, PyEmbedderIdentity, PyOpenReport, PyExpandedNode,
  PySearchExpandResult.
- STATUS-0.8.9: $0 CI-integrity micro; slices 0/5/10/15 CLOSED, 40 in progress; PR #93
  (`d5a68d17`, branch `0.8.9-ci-integrity-micro`) blocked on pre-existing main CI red
  (bootstrap infra + 0.8.8 pyo3 link), not caused by 0.8.9.
