# Distilled domain summary — `fathomdb-py` binding + STATUS-0.8.9

Faithful, dense technical distillation of three source files:

1. `src/rust/crates/fathomdb-py/Cargo.toml`
2. `src/rust/crates/fathomdb-py/src/lib.rs` (1489 lines)
3. `dev/plans/runs/STATUS-0.8.9.md`

All names/values preserved so questions can be answered without the originals. Terse prose, exhaustive on identifiers.

---

## 1. `fathomdb-py/Cargo.toml`

### Package
- `name = "fathomdb-py"`.
- `version`, `edition`, `license`, `repository`, `rust-version` all `.workspace = true`.
- `publish = false` (this crate is not published).

### `[lib]`
- `name = "fathomdb_py"`.
- `crate-type = ["cdylib", "rlib"]`.

### `[features]`
Five feature entries (plus `default`):

| Feature | Value | Forwards to engine? | cfg-gated in lib.rs? |
|---|---|---|---|
| `default` | `[]` (empty) | — | n/a |
| `test-hooks` | `[]` (empty, no deps) | no | **YES** — `#[cfg(any(test, feature = "test-hooks"))]` gates the test-only functions (see §2 test hooks) |
| `default-embedder` | `["fathomdb-engine/default-embedder"]` | yes | **NO** direct `#[cfg(feature="default-embedder")]` in lib.rs — it is a pure engine-forward; the `use_default_embedder=True` kwarg materializes a real `CandleBgeEmbedder` only when this feature is on, else rejects `True` at open time (per ADR-0.7.1-default-embedder-weight-fetch; no silent network in a no-feature wheel) |
| `default-reranker` | `["fathomdb-engine/default-reranker"]` | yes | **NO** direct cfg in lib.rs — pure engine-forward. A plain `maturin develop` / `pytest` activates the real CE reranker in dev/test; the shipped wheel keeps it OFF (release.yml and ci.yml pin explicit `--features` lists omitting it). Added in 0.8.2 Slice E2 fix-1 [P1]. |
| `embed-cuda` | `["default-embedder", "fathomdb-engine/embed-cuda"]` | yes (and pulls `default-embedder`) | **NO** direct cfg in lib.rs — pure engine-forward. GPU opt-in; default wheel stays CPU. Build: `maturin develop --features pyo3/extension-module,embed-cuda`; activate via `FATHOMDB_EMBED_DEVICE=cuda`. See `dev/design/0.8.1-embedder-gpu-and-portability.md` |
| `embed-metal` | `["default-embedder", "fathomdb-engine/embed-metal"]` | yes (and pulls `default-embedder`) | **NO** direct cfg in lib.rs — pure engine-forward |

**Key cfg distinction:** ONLY `test-hooks` (plus the built-in `test`) is referenced via `#[cfg(...)]` inside lib.rs. The other four features (`default-embedder`, `default-reranker`, `embed-cuda`, `embed-metal`) are NOT cfg-gated in the binding source — they exist purely to forward Cargo features down to `fathomdb-engine`; their runtime effect surfaces only through engine behavior (e.g. `EmbedderChoice::Default` succeeding vs. erroring).

### `[dependencies]`
- `fathomdb-embedder.workspace = true`
- `fathomdb-embedder-api.workspace = true`
- `fathomdb-engine.workspace = true`
- `fathomdb-schema.workspace = true`
- `pyo3 = { version = "0.29", features = ["extension-module", "abi3-py310"] }`  ← **pin = "0.29"; features = `extension-module` + `abi3-py310`**
- `serde_json = "1"`

(Note: lib.rs top comment still references "pyo3 0.22" macro behavior and the module comment references "pyo3 0.28" free-threaded default — stale prose; the actual pin is **0.29**. This pin came from the 0.8.8 pyo3 0.24.1→0.29.0 migration / Dependabot PR #89, RUSTSEC-2026-0176/0177.)

---

## 2. `fathomdb-py/src/lib.rs` (1489 lines)

PyO3 binding from the Python SDK to `fathomdb-engine`. Module is named `_fathomdb`; classes declare `module = "fathomdb._fathomdb"`.

### Crate-level attributes (lines 1-8)
- `#![allow(unexpected_cfgs)]` — pyo3 macros emit `#[cfg(feature = "gil-refs")]` arms referencing an upstream feature this crate does not export; suppresses noise on a clippy `-D warnings` gate.
- `#![allow(clippy::useless_conversion)]` — covers `#[pymethods]`-generated `PyResult` wrappers flagged as redundant `Into<PyErr>`.

### FFI safety contract (module doc)
1. Every method that may block in the engine wraps the call in `py.detach(...)` to release the GIL.
2. Engine entry points return typed errors via `engine_error_to_py` / `engine_open_error_to_py` — single-switch mapping, **no catch-all arm**; binding fails to compile if the Rust variant set drifts from the Python class set (AC-060a).
3. Every string crossing FFI is checked by `validate_ffi_string` for embedded NUL or unpaired UTF-16 surrogate BEFORE the writer transaction opens (AC-068a / AC-068b).
4. Panics inside engine code surface as Python `PanicException` (`pyo3::panic::PanicException`); host process not aborted (AC-067). Engine calls wrapped in `catch_unwind` so the panic is translated Rust-side. **`PanicException` is intentionally NOT an `EngineError` subclass** — panic is a contract bug, not a typed engine outcome.

### Imports
- `std::panic::{catch_unwind, AssertUnwindSafe}`, `std::sync::Arc`.
- `fathomdb_embedder::EmbedderEvent as RustEmbedderEvent`.
- `fathomdb_embedder_api::EmbedderIdentity as RustEmbedderIdentity`.
- From `fathomdb_engine`: `rerank_passages as rust_rerank_passages`, `ComparisonOp as RustComparisonOp`, `CorruptionDetail`, `CorruptionKind`, `EmbedderChoice`, `Engine as RustEngine`, `EngineError as RustEngineError`, `EngineOpenError`, `ExtractDocument as RustExtractDocument`, `IngestWithExtractorReceipt as RustIngestWithExtractorReceipt`, `NodeRecord as RustNodeRecord`, `OpStoreRow as RustOpStoreRow`, `OpenReport as RustOpenReport`, `OpenStage`, `Predicate as RustPredicate`, `PreparedWrite`, `ScalarValue as RustScalarValue`, `SearchExpandResult as RustSearchExpandResult`, `SearchFilter as RustSearchFilter`, `SearchHit as RustSearchHit`, `SearchResult as RustSearchResult`, `SoftFallback as RustSoftFallback`, `SoftFallbackBranch`, `TraversalDirection as RustTraversalDirection`, `WriteReceipt as RustWriteReceipt`.
- `fathomdb_schema::MigrationStepReport as RustMigrationStepReport`.
- pyo3: `create_exception`, `exceptions::{PyException, PyTypeError, PyValueError}`, `panic::PanicException`, `prelude::*`, `types::{PyDict, PyList}`.

### Exception type hierarchy (`create_exception!`, lines 62-86)
Root: **`EngineError`** inherits Python **`PyException`** (i.e. `Exception`). All concrete leaves inherit from `EngineError`, EXCEPT where noted nested. Declared in module `_fathomdb`. Full list with parent:

- `EngineError` ← `PyException`  (root)
- `StorageError` ← `EngineError`
- `ProjectionError` ← `EngineError`
- `VectorError` ← `EngineError`
- `KindNotVectorIndexedError` ← **`VectorError`**  (nested under VectorError, not EngineError directly)
- `EmbedderError` ← `EngineError`
- `EmbedderNotConfiguredError` ← **`EmbedderError`**  (nested under EmbedderError)
- `SchedulerError` ← `EngineError`
- `OpStoreError` ← `EngineError`
- `WriteValidationError` ← `EngineError`
- `SchemaValidationError` ← `EngineError`
- `OverloadedError` ← `EngineError`
- `ClosingError` ← `EngineError`
- `DatabaseLockedError` ← `EngineError`
- `CorruptionError` ← `EngineError`
- `IncompatibleSchemaVersionError` ← `EngineError`
- `MigrationError` ← `EngineError`
- `EmbedderIdentityMismatchError` ← `EngineError`
- `EmbedderDimensionMismatchError` ← `EngineError`
- `ExtractorError` ← `EngineError`  (G11 / Slice 15 — BYO-LLM extraction harness protocol error)
- `InvalidFilterError` ← `EngineError`  (G4 / Slice 35 — filter predicate construction, non-allowlisted path)
- `InvalidArgumentError` ← `EngineError`  (Slice 20 G5/G6 — traversal depth > 3 / out-of-range arg)

So the two two-level leaves are `KindNotVectorIndexedError` (under `VectorError`) and `EmbedderNotConfiguredError` (under `EmbedderError`). 22 `create_exception!` macros total; `PanicException` is separate (NOT in this tree).

### String validation (AC-068a/AC-068b)
- `pub fn validate_ffi_string(value: &str) -> Result<(), String>`: rejects embedded NUL byte (`"embedded NUL byte in FFI string"`) and unpaired UTF-16 surrogate `U+D800..=U+DFFF` (`"unpaired UTF-16 surrogate U+{cp:04X} in FFI string"`).
- `fn validate_ffi_string_py(value: &str) -> PyResult<()>`: maps the err to `WriteValidationError::new_err`.
- `fn extract_validated_str(value: &Bound<PyAny>) -> PyResult<String>`: extracts a Python str into Rust `String` + validates. On extraction failure (lone surrogate → PyO3 `UnicodeEncodeError`) re-raises as `WriteValidationError` ("string contains characters not representable as UTF-8 (lone surrogate)").
- `fn extract_opt_validated_str(value: Option<&Bound<PyAny>>) -> PyResult<Option<String>>`: `None`/None-valued stays `None` (preserves all-`None` byte-identical unfiltered path); present value extracted + validated. Used by `search` for G10 `SearchFilter` string fields.

### Error mapping
- `fn engine_error_to_py(err: RustEngineError) -> PyErr` (no catch-all). Variant → Python class:
  - `Storage` → `StorageError` ("storage error")
  - `Projection` → `ProjectionError` ("projection error")
  - `Vector` → `VectorError` ("vector error")
  - `Embedder` → `EmbedderError` ("embedder error")
  - `EmbedderNotConfigured` → `EmbedderNotConfiguredError` ("embedder is not configured")
  - `KindNotVectorIndexed` → `KindNotVectorIndexedError`
  - `EmbedderDimensionMismatch { expected, actual }` → `EmbedderDimensionMismatchError`; sets attrs `stored = expected`, `supplied = actual`
  - `Scheduler` → `SchedulerError`
  - `OpStore` → `OpStoreError`
  - `WriteValidation` → `WriteValidationError`
  - `SchemaValidation` → `SchemaValidationError`
  - `Overloaded` → `OverloadedError` ("engine overloaded")
  - `Closing` → `ClosingError` ("engine is closing")
  - `Extractor` → `ExtractorError`
  - `InvalidFilter { reason }` → `InvalidFilterError` ("invalid filter: {reason}")
  - `InvalidArgument { msg }` → `InvalidArgumentError` (msg verbatim)
- `fn corruption_kind_str(CorruptionKind) -> &'static str`: `WalReplayFailure`, `HeaderMalformed`, `SchemaInconsistent`, `EmbedderIdentityDrift`.
- `fn open_stage_str(OpenStage) -> &'static str`: `HeaderProbe`, `WalReplay`, `SchemaProbe`, `EmbedderIdentity`.
- `fn engine_open_error_to_py(err: EngineOpenError) -> PyErr`. Variants:
  - `DatabaseLocked { holder_pid }` → `DatabaseLockedError`; sets attr `holder_pid`. Message "database is locked by process {pid}" or "...by another engine instance".
  - `Corruption(detail)` → `corruption_to_py(detail)`
  - `IncompatibleSchemaVersion { seen, supported }` → `IncompatibleSchemaVersionError`
  - `MigrationError { schema_version_before, schema_version_current, step_id }` → `MigrationError`
  - `EmbedderIdentityMismatch { stored, supplied }` → `EmbedderIdentityMismatchError`; sets attrs `stored_name`, `stored_revision`, `supplied_name`, `supplied_revision`
  - `EmbedderDimensionMismatch { stored, supplied }` → `EmbedderDimensionMismatchError`; sets attrs `stored`, `supplied`
  - `Embedder(err)` → `EmbedderError` ("{err:?}")
  - `Io { message }` → `StorageError` ("database I/O error: {message}")
- `fn corruption_to_py(detail: CorruptionDetail) -> PyErr`: builds `CorruptionError` "corruption {kind} at stage {stage} ({recovery_hint_code})"; sets attrs `kind`, `stage`, `recovery_hint_code`, `doc_anchor` (from `detail.recovery_hint.code` / `.doc_anchor`).
- `fn call_engine<R: Send>(py, f: impl FnOnce() -> Result<R, RustEngineError> + Send) -> PyResult<R>`: wraps `f` in `AssertUnwindSafe`, runs `py.detach(|| catch_unwind(wrapped))`. `Ok(Ok)` → value; `Ok(Err)` → `engine_error_to_py`; `Err(_)` (panic) → `PanicException::new_err("engine panic (see logs)")`. Uses `AssertUnwindSafe` because the engine's `Arc<dyn Embedder>` makes the natural `UnwindSafe` bound unsatisfiable.

### Data classes (`#[pyclass]` types)
All declare `module = "fathomdb._fathomdb"` (except the Slice-20 pair which omit module), most are `frozen, get_all, skip_from_py_object` and `#[derive(Clone)]`. Full list of pyclass types and fields:

1. **`PyWriteReceipt`** (name `"WriteReceipt"`): `cursor: u64`, `row_cursors: Vec<u64>` (G0/Slice 15 — per-row write_cursors 1:1 with input batch order), `dangling_edge_endpoints: u64` (G8/Slice 20 — count of edge endpoints pointing at non-existent/superseded canonical node; flag-and-count). `from_rust(RustWriteReceipt)`.
2. **`PyIngestWithExtractorReceipt`** (name `"IngestWithExtractorReceipt"`, G11/Slice 15): `nodes_written: u64`, `edges_written: u64`, `docs_processed: u64`. `from_rust(RustIngestWithExtractorReceipt)`.
3. **`PySoftFallback`** (name `"SoftFallback"`): `branch: String`. `from_rust(&RustSoftFallback)` maps `SoftFallbackBranch::{Vector→"vector", Text→"text", TextEdge→"text_edge", GraphArm→"graph_arm"}`.
4. **`PySearchHit`** (name `"SearchHit"`): `id: u64`, `kind: String`, `body: String`, `score: f64`, `branch: String`, `source_id: Option<String>` (G0 Phase-2 — set only for graph-arm hits, None for two-arm hits), `ce_score: Option<f64>` (0.8.5 EXP-0 — per-candidate `ce_norm = sigmoid(ce_logit)`; Some only inside reranked pool). `from_rust(&RustSearchHit)`.
5. **`PySearchResult`** (name `"SearchResult"`): `projection_cursor: u64`, `soft_fallback: Option<PySoftFallback>`, `results: Vec<PySearchHit>`. `from_rust(RustSearchResult)`.
6. **`PyNodeRecord`** (name `"NodeRecord"`): `logical_id: String`, `kind: String`, `body: String`, `write_cursor: u64`. `from_rust(&RustNodeRecord)`.
7. **`PyOpStoreRow`** (name `"OpStoreRow"`): `id: i64`, `collection: String`, `record_key: String`, `op_kind: String`, `payload: String`, `schema_id: Option<String>`, `write_cursor: u64`. `from_rust(&RustOpStoreRow)`.
8. **`PyCounterSnapshot`** (name `"CounterSnapshot"`): `queries: u64`, `writes: u64`, `write_rows: u64`, `admin_ops: u64`, `cache_hit: u64`, `cache_miss: u64`. (No `from_rust`; built inline in `PyEngine::counters`.)
9. **`PyMigrationStepReport`** (name `"MigrationStepReport"`): `step_id: u32`, `duration_ms: Option<u64>`, `failed: bool`. `from_rust(&RustMigrationStepReport)`.
10. **`PyEmbedderIdentity`** (name `"EmbedderIdentity"`): `name: String`, `revision: String`, `dimension: u32`. `from_rust(&RustEmbedderIdentity)`.
11. **`PyOpenReport`** (name `"OpenReport"`, `frozen, get_all` — NOT `skip_from_py_object`, NOT `Clone`): `schema_version_before: u32`, `schema_version_after: u32`, `migration_steps: Vec<PyMigrationStepReport>`, `embedder_warmup_ms: u64`, `query_backend: String`, `default_embedder: PyEmbedderIdentity`, `embedder_download_ms: Option<u64>` (EU-3 loader wall-time fetching default-embedder weights; None on full cache hit/caller-supplied), `embedder_events: Vec<Py<PyAny>>` (structured loader events as dicts keyed by "kind"), `embedder_mean_centering_required: bool` (static — true when default embedder needs mean-centering e.g. bge-small), `embedder_mean_vec_pinned: bool` (dynamic — true iff `_fathomdb_embedder_profiles.mean_vec IS NOT NULL`). `from_rust(py, &RustOpenReport)`.
12. **`PyExpandedNode`** (name `"ExpandedNode"`, Slice 20; `skip_from_py_object`, `Clone`; fields use per-field `#[pyo3(get)]` not `get_all`; NO `module=`): `node: PyNodeRecord`, `hop_count: u32`.
13. **`PySearchExpandResult`** (name `"SearchExpandResult"`, Slice 20 G6; `skip_from_py_object`, `Clone`; per-field `#[pyo3(get)]`; NO `module=`): `search_hits: Vec<PySearchHit>`, `expanded: Vec<PyExpandedNode>`, `all_logical_ids: Vec<String>`. `from_rust(RustSearchExpandResult)` — `expanded` built from `(node, hop_count)` tuples.

### `embedder_event_to_py(py, ev: &RustEmbedderEvent) -> Py<PyAny>`
Serializes one event as a Python dict (so callers pattern-match on `"kind"` without importing leaf classes). Variants and their dict keys:
- `DefaultEmbedderDownload { file, url, bytes, sha256, cache_path, duration_ms }` → kind `"DefaultEmbedderDownload"` + keys `file`, `url`, `bytes`, `sha256`, `cache_path` (display string), `duration_ms`.
- `DefaultEmbedderCacheHit { file, sha256, cache_path }` → kind `"DefaultEmbedderCacheHit"` + `file`, `sha256`, `cache_path`.
- `MeanVecPinned { dim, doc_count }` → kind `"MeanVecPinned"` + `dim`, `doc_count`.
- `MeanVecRecomputed { dim, doc_count, trigger }` → kind `"MeanVecRecomputed"` + `dim`, `doc_count`, `trigger` (`trigger.as_str()`).

### `PyEngine` (`#[pyclass] name = "Engine"`)
Fields: `inner: Arc<RustEngine>`, `open_report: Arc<RustOpenReport>`.

`#[pymethods]` — full method list:
- **`open`** (`#[staticmethod]`, signature `(path, use_default_embedder = false)`): validates path; `py.detach` + `catch_unwind` calls `RustEngine::open_with_choice(path, choice)` where `choice = if use_default_embedder { EmbedderChoice::Default } else { EmbedderChoice::None }` (EU-6: True → engine materializes pinned bge-small via EU-3 loader; False → engine opens but vector writes fail `EmbedderNotConfigured`). Panic during open → `PanicException::new_err("engine panic during open")`; open error → `engine_open_error_to_py`.
- **`open_report(&self, py) -> PyOpenReport`**: `PyOpenReport::from_rust(py, &self.open_report)`.
- **`write(&self, py, batch: Bound<PyList>) -> PyResult<PyWriteReceipt>`**: `translate_batch` → `call_engine(... engine.write(&prepared))`.
- **`search`** (signature `(query, source_type=None, kind=None, created_after=None, status=None, rerank_depth=0, use_graph_arm=false, alpha=None, pool_n=None)`; `#[allow(clippy::too_many_arguments)]`): G10 + 0.8.1 R1 hybrid search. Validates query + filter strings via `extract_opt_validated_str`. Builds `Some(RustSearchFilter { source_type, kind, created_after, status })` only if any of source_type/kind/created_after/status is set, else `None` (all-None = byte-identical unfiltered). Defaults resolved binding-side: `alpha = alpha.unwrap_or(0.3)`, `pool_n = pool_n.unwrap_or(rerank_depth)`. Calls `engine.search_reranked(&query, filter, rerank_depth, use_graph_arm, alpha, pool_n)`. `rerank_depth=0` is no-op identity; `>0` activates CE path (needs `default-reranker` feature + model loaded, else falls back to identity). `use_graph_arm=true` (0.8.1 R3 / Slice 30) seeds BFS over temporal fact-edges from top-10 fused hits as a 3rd RRF arm. 0.8.5 EXP-0: `alpha` default 0.3 clamped [0,1] in engine; `alpha=1.0, pool_n=10` is the measured-parity config.
- **`close(&self, py) -> PyResult<()>`**: `call_engine(... engine.close())`.
- **`drain`** (signature `(timeout_s = 0.0)`): converts finite `timeout_s>0` to ms (`(timeout_s*1000.0) as u64`) else 0; `call_engine(... engine.drain(ms))`.
- **`ingest_with_extractor(&self, py, cmd: Bound<PyList>, documents: Bound<PyList>) -> PyResult<PyIngestWithExtractorReceipt>`** (G11/Slice 15 BYO-LLM): `cmd` = argv (must be strings, else `WriteValidationError` "cmd elements must be strings"); `documents` = list of dicts with required `source_doc_id` + `body` (else "document must be a dict" / missing-field error) → `Vec<RustExtractDocument>`. Calls `engine.ingest_with_extractor(&cmd_refs, &docs)`.
- **`counters(&self) -> PyCounterSnapshot`**: reads `self.inner.counters()`, builds `PyCounterSnapshot` inline.
- **`set_profiling(&self, enabled: bool) -> PyResult<()>`**: `self.inner.set_profiling(enabled)` mapped via `engine_error_to_py`.
- **`set_slow_threshold_ms(&self, value: u64) -> PyResult<()>`**: `self.inner.set_slow_threshold_ms(value)`.
- **`embed(&self, py, text: &str) -> PyResult<Vec<f32>>`**: validates text; `call_engine(... engine.embed_text(&text))`. Uses pinned default embedder `fathomdb-bge-small-en-v1.5`; raises `EmbedderNotConfiguredError` if opened without embedder.
- **`_configure_vector_kind_for_test(&self, py, kind: &str) -> PyResult<()>`** — **TEST-HOOKS-GATED** `#[cfg(any(test, feature = "test-hooks"))]`. EU-6 vector-write seam. `engine.configure_vector_kind_for_test(&kind)`.
- **`_write_vector_for_test(&self, py, kind: &str, text: &str) -> PyResult<()>`** — **TEST-HOOKS-GATED** `#[cfg(any(test, feature = "test-hooks"))]`. `engine.write_vector_for_test(&kind, &text)`.
- **`attach_logging_subscriber`** (signature `(logger, heartbeat_interval_ms = None)`): currently a no-op (`let _ = logger; let _ = heartbeat_interval_ms; Ok(())`); subscriber wiring deferred to a later 0.6.x slice.

### Standalone `#[pyfunction]`s (module-level, NOT engine methods)
1. **`admin_configure`** (signature `(engine, name, body)`): admin.configure verb. Validates name+body; name non-empty (else `PyValueError` "admin.configure requires a non-empty name"). Builds `PreparedWrite::AdminSchema { name, kind: "latest_state", schema_json: body, retention_json: "{}" }`; `call_engine(... inner.write(&batch))` → `PyWriteReceipt`.
2. **`read_get`** (signature `(engine, logical_id)`, G2/G3 Slice 30): active-only point lookup by logical_id; not-found = `None` (NOT an exception — typed NotFound reserved for Slice 31). Returns `Option<PyNodeRecord>` via `inner.read_get(&logical_id)`.
3. **`read_get_many`** (signature `(engine, logical_ids)`): extracts+validates each id; `inner.read_get_many(&ids)` → `Vec<Option<PyNodeRecord>>`.
4. **`read_collection`** (signature `(engine, collection, after_id=None, limit=0)`): paginated op-store read-back via `read_collection_impl` → `inner.read_collection(&collection, after_id, limit)` → `Vec<PyOpStoreRow>`.
5. **`read_mutations`** (signature `(engine, collection, after_id=None, limit=0)`): same impl as `read_collection` (both call `read_collection_impl`). Mandatory limit + after_id cursor; rides engine ReaderWorkerPool DEFERRED-tx path.
6. **`read_list`** (signature `(engine, kind, predicates=None, limit=100)`, G4/Slice 35): lists active canonical nodes of `kind`, optional `Predicate` dict filters. Each predicate dict shape `{ "type": "eq"|"gt"|"gte"|"lt"|"lte", "path": str, "value": str|int|bool }`. `inner.read_list(&kind, &rust_predicates, limit)` → `Vec<PyNodeRecord>`.
7. **`graph_neighbors`** (signature `(engine, logical_id, depth, direction)`, Slice 20 G5): bounded BFS over `canonical_edges`. `depth` 1..=3 (>3 → `InvalidArgumentError`); direction `"outgoing"|"incoming"|"both"`. Returns reachable nodes (excl. root), hard-capped at 50. `inner.graph_neighbors(&logical_id, depth, dir)`.
8. **`search_expand`** (signature `(engine, query, depth, source_type=None, kind=None, created_after=None, status=None)`, Slice 20 G6; `#[allow(clippy::too_many_arguments)]`): hybrid search then bounded BFS expansion. `depth` 0..=3 (>3 → `InvalidArgumentError`). Same filter-build pattern as `search`. `inner.search_expand(&query, filter, depth)` → `PySearchExpandResult`.
9. **`rerank`** (signature `(query, passages, rerank_depth, alpha=None, pool_n=None)`, 0.8.2 Slice E2): standalone CE rerank over a caller-supplied passage list — NOT engine-bound. Marshals `[{"id": int, "body": str, "score": float}]` into `(id, body, score)` tuples; calls pure `rust_rerank_passages(&query, tuples, rerank_depth, alpha, pool_n)` inside `py.detach` + `catch_unwind`. Defaults `alpha = alpha.unwrap_or(0.3)`, `pool_n = pool_n.unwrap_or(rerank_depth)`. Returns `Vec<Py<PyDict>>` of `{"id", "score", "ce_score"}` (ce_score None outside reranked pool). Identity contract: `rerank_depth==0` OR empty list returns input order/scores (no model load, no network). Panic → `PanicException` ("rerank panic (see logs)"); inner Err (non-finite score) → `WriteValidationError`. E2 fix-1 [P2]: helper returns `Result<Vec<…>, String>`.

### Test-hooks-gated functions (the names to remember)
Three items gated by `#[cfg(any(test, feature = "test-hooks"))]`, compiled OUT of release wheels built `--no-default-features`:
1. **`PyEngine::_configure_vector_kind_for_test`** (method)
2. **`PyEngine::_write_vector_for_test`** (method)
3. **`force_panic_for_test`** (standalone `#[pyfunction]`, AC-067 force-panic probe; body `panic!("force_panic_for_test: AC-067 probe")`)

(The module registration of `force_panic_for_test` is itself cfg-gated too.)

### Helper functions (non-public translation layer)
- `validate_ffi_string` (pub), `validate_ffi_string_py`, `extract_validated_str`, `extract_opt_validated_str`.
- `translate_batch(&Bound<PyList>) -> Vec<PreparedWrite>`.
- `dict_get<'py>(&Bound<PyDict>, key) -> Option<Bound<PyAny>>`.
- `dict_str(&Bound<PyDict>, key) -> Option<String>` (None/None-valued → None, else validated).
- `dict_str_required(&Bound<PyDict>, key) -> String` (else `WriteValidationError` "write item missing required field {key:?}").
- `dict_u64_required(&Bound<PyDict>, key) -> u64` (passage id; else "passage missing required field"/"must be a non-negative integer").
- `dict_f64_required(&Bound<PyDict>, key) -> f64` (passage score; "must be a number").
- `translate_write_item(&Bound<PyAny>) -> PreparedWrite`: dispatches on dict keys `edge` → `translate_edge`, `op_store` → `translate_op_store`, `admin_schema` → `translate_admin_schema`, `node` → `translate_node`; bare `{"kind": ...}` shape treated as a Node.
- `translate_node` → `PreparedWrite::Node { kind (required), body (default "{}"), source_id (opt), logical_id (opt) }`.
- `translate_edge` → `PreparedWrite::Edge { kind, from, to (all required), source_id, logical_id, body (opt — relation text projected into `search_index_edges`), t_valid, t_invalid (opt ISO-8601, R3/Slice 30 temporal validity), confidence: None, extractor_model_id: None, temporal_fallback: None }`.
- `translate_op_store` → `PreparedWrite::OpStore { collection, record_key (required), schema_id (opt), body (required) }`.
- `translate_admin_schema` → `PreparedWrite::AdminSchema { name, kind, schema_json (required), retention_json (default "{}") }`; non-dict → `PyTypeError` (note: not WriteValidationError here).
- `py_predicate_to_rust(&Bound<PyAny>) -> RustPredicate`: reads `type`/`path`/`value`. Value coercion order: **bool first** (Python bool is int subclass → must check before int), then `i64`, else `Text` (validated). `RustScalarValue::{Bool, Integer, Text}`. Maps type → `RustPredicate::json_path_eq` (eq) or `json_path_compare(path, RustComparisonOp::{Gt|Gte|Lt|Lte}, scalar)`; unknown type → `PyValueError` ("unknown predicate type '{other}'; expected 'eq', 'gt', 'gte', 'lt', or 'lte'").
- `parse_direction(&str) -> RustTraversalDirection`: `"outgoing"→Outgoing`, `"incoming"→Incoming`, `"both"→Both`, else `InvalidArgumentError`.
- `read_collection_impl(py, engine, collection, after_id, limit)`.

### `#[pymodule]` registration — **`gil_used = true`**
Declared `#[pymodule(gil_used = true)]` on `fn _fathomdb(py, m: Bound<PyModule>) -> PyResult<()>`. **`gil_used = true`** preserves current GIL semantics (pyo3 0.28+ makes `#[pymodule]` free-threaded by default, but this binding is `abi3-py310` and the whole FFI contract assumes GIL held; free-threading `gil_used = false` is a separate larger campaign — see `dev/design/free-threaded-python-value-lift-and-experiments.md`).

Registered classes (in order): `PyEngine`, `PyWriteReceipt`, `PyIngestWithExtractorReceipt`, `PySoftFallback`, `PySearchHit`, `PySearchResult`, `PyCounterSnapshot`, `PyMigrationStepReport`, `PyEmbedderIdentity`, `PyOpenReport`, `PyNodeRecord`, `PyOpStoreRow`, `PyExpandedNode`, `PySearchExpandResult`.

Registered functions: `admin_configure`, `read_get`, `read_get_many`, `read_collection`, `read_mutations`, `read_list`, `graph_neighbors`, `search_expand`, `rerank`, and (cfg-gated) `force_panic_for_test`.

Registered exception type objects via `m.add("<Name>", py.get_type::<Name>())` for all 22 exceptions (EngineError, StorageError, ProjectionError, VectorError, KindNotVectorIndexedError, EmbedderError, EmbedderNotConfiguredError, SchedulerError, OpStoreError, WriteValidationError, SchemaValidationError, OverloadedError, ClosingError, DatabaseLockedError, CorruptionError, IncompatibleSchemaVersionError, MigrationError, EmbedderIdentityMismatchError, EmbedderDimensionMismatchError, ExtractorError, InvalidFilterError, InvalidArgumentError).

### `#[cfg(test)] mod tests`
Four Rust unit tests on `validate_ffi_string`: `validate_ffi_string_accepts_plain_ascii` ("hello"), `validate_ffi_string_accepts_non_ascii_utf8` ("héllo 🦀 文字"), `validate_ffi_string_rejects_embedded_nul` ("a\0b" → err contains "NUL"), `validate_ffi_string_rejects_lone_surrogate` (uses `"\u{FFFD}"` since a real surrogate can't appear in a Rust &str; exhaustive surrogate guard sits in the Python layer).

---

## 3. `STATUS-0.8.9.md` — board

**Title:** STATUS — 0.8.9 (CI integrity micro, OUT-OF-BAND) · live board. Plan `dev/plans/plan-0.8.9.md`. **Footprint $0** (CI/test-harness only; no library query-path change, no priced runs). Opened 2026-06-27 (`/goal complete 0.8.9`, orchestrator session). Verify-from-git discipline.

### §0 Headline — plan was substantially stale; most of 0.8.9 already shipped
Slice 0 audited actual gate reality vs the plan's premises (written off the `perf-recall-gates-masked-and-ac013b-conflation` memory dated 2026-06-06, the day defects were *exposed* at 0.8.0 Slice 40). Git shows Slice 40 + later cleanup also FIXED most of them. Honest deliverable (R-PG-1) is the map, not a fabricated five-slice pass.

Requirement-vs-reality table:
- **R-PG-2** (ac_013b off synthetic floor): **DONE @ Slice 40 (AC-075).** `perf_gates.rs::ac_013b_recall_at_10_floor` is now report-only (prints `RECALL_FIDELITY_INFO`, no hard assert). Asserting verdict moved to `eu7_real_corpus_ac.rs` (real BGE, vector-stage, one-sided CI `ci_hi≥0.90`). Residue: add cheap RED unit-test on catch predicate.
- **R-PG-3** (cheap subset per-push): **DONE.** Devloop tier `perf_gates_devloop.rs` runs on every `cargo test --workspace` (agent-test → CI `verify`). Canonical tier is `AGENT_LONG` (release-only, real-embed = hours). Residue: doc the split.
- **R-037-1** (AC-037 in CI on userns-permissive runner): **DONE @ `8402e59c`.** `ci.yml` `security` job on **ubuntu-22.04** runs `STRICT=1 agent-security.sh` (AC-036/037/038/050a/050c); STRICT=1 makes a toolchain blocker a hard failure.
- **R-037-2** (demonstrate-the-catch): **OPEN.** No egress fixture proves the gate trips. Cannot execute in this sandbox (rootless userns unavailable); runs on ubuntu-22.04 job. Residue: author fixture + RED proof.
- **R-050c-1** (removal-detect baseline cleared): **DONE @ `a8304652` (0.8.0 Slice 27 fix-1).** Cause: removal-detect scoped `tests/` files into public-surface diff + missing CHANGELOG operator-gate note; fix excluded `tests/` + added note. Passes baseline now (base=v0.6.1, exit 0). The 2026-06-06 memory predates this.
- **R-DEP-3** (no mechanical auto-merge): **CONFIRMED.** `allow_auto_merge=false`; no auto-merge workflow in `.github/`.
- **R-DEP-1 npm** (markdown-it + js-yaml): **OPEN + actionable.** Root `package-lock.json`: markdown-it 14.1.1→**14.2.0**, js-yaml 4.1.1→**4.2.0** (both transitive via `markdownlint-cli2`).
- **R-DEP-1 pip** (idna + torch in python/uv.lock): **MOOT/orphaned.** `python/uv.lock` archived out of tree (`39ee2712`; archive removed `df33207a`); idna already bumped 3.11→**3.15** at `e850052d` before removal. `src/python/uv.lock` carries neither idna nor torch. `torch` has no patched version (`<=2.12.0`, low-sev, eval-only).
- **R-DEP-2** (dependabot.yml coverage): **OPEN (npm only).** npm root `/` uncovered today (only `/src/ts`). pip `/python` moot (no lockfile in tree).

**Net residue (6 genuinely-open items, all $0):** (1) R-PG-1 consolidated gate-map in `dev/design/perf-gates.md`; (2) R-PG-2 RED unit-test on `recall_ci_clears_floor`; (3) R-037-2 deliberately-egressing fixture + RED proof; (4) R-DEP-1(npm) bump root `package-lock.json`; (5) R-DEP-2 add npm `/` to `.github/dependabot.yml`; (6) R-DEP-1(pip) dismiss-with-rationale orphaned idna/torch (HITL-gated).

### §1 Slice board (mod-5)
| Slice | Title | State | Notes |
|---|---|---|---|
| 0 | Setup + audit; map gate reality | **CLOSED** | scope reconciled; residue identified |
| 5 | Perf-gate honesty (R-PG-1/2) | **CLOSED** | `perf-gates.md` per-AC map; `recall_gate_predicate.rs` catch test (3/3 green, RED-confirmed) |
| 10 | AC-037 catch + AC-050c (R-037-2/R-050c) | **CLOSED** | shared `lib-egress-allowlist.sh`; `check-netns-deny-egress-catch.sh` (offline catch green + RED-confirmed, live netns CI-only); R-050c cause documented |
| 15 | Dependency hygiene (R-DEP) | **CLOSED** | npm overrides → markdown-it 14.2.0/js-yaml 4.2.0, `npm audit`=0; dependabot.yml npm `/` added; pip idna/torch orphaned (dismiss pending HITL) |
| 40 | Verify + release readiness | **in progress** | cargo test, mkdocs, codex §9, HITL |

### §2 Cross-cutting DoD (X1/X2/X3)
- **X1 SDK parity** — no library API change (CI/test-harness only). N/A by design.
- **X2 `mkdocs build --strict`** — keep green (perf-gates.md edits stay in nav).
- **X3 docs + DOC-INDEX** — reconcile stale gate-map references in closing docs commit.

### §2a Slice 40 verification (all local, $0)
- `cargo test -p fathomdb-engine --test perf_gates_devloop` → **3/3 green** (per-push tier).
- `cargo test -p fathomdb-engine --test recall_gate_predicate` → **3/3 green**; RED-confirmed (tautological allowlist flags nothing → exit 1).
- `check-netns-deny-egress-catch.sh` → **PASS** (offline catch flags 2 egress, clean trace not flagged; live netns skipped — no userns in sandbox); RED-confirmed.
- `agent-security.sh` battery → catch gate PASS (AC-037 live = BLOCKER here, expected; runs on ubuntu-22.04 `security` job).
- `mkdocs build --strict` → **exit 0** (perf-gates.md lives under `dev/`, outside published `docs/`).
- `npm audit` → **0 vulnerabilities** (was 3 moderate). Override lint-behavior-neutral; `npm run lint:md` is NOT a CI gate (`agent-lint.sh` doesn't run it; doc gate is `mkdocs --strict`).
- **codex §9 review (`--uncommitted`)** → **clean PASS, 0 findings.**

### §3 HITL sign-off ledger
- [x] Working-tree changes reviewed (codex §9) — clean PASS, 0 findings.
- [x] Memory reconciliation — `perf-recall-gates-masked-and-ac013b-conflation` updated (RESOLVED header).
- [x] Commit 0.8.9 residue — **HITL: branch + PR.** Branch `0.8.9-ci-integrity-micro`, commit `d5a68d17`, **PR #93** (10 files; unrelated working-tree changes excluded).
- [x] Dismiss orphaned idna/torch alerts — **HITL: leave open** (documented as orphaned).
- [x] Version-bump / tag — **HITL: no version bump** (zero library-surface change).
- [ ] **Merge PR #93** — HITL action, **blocked on pre-existing CI red** (see §5). ← only open ledger item.

### §5 CI status on PR #93 — pre-existing red on main, NOT caused by 0.8.9
Main's last 3 runs are red on the SAME 4 jobs (on docs-only commits):
| Job | Fails at step | Cause | Owner |
|---|---|---|---|
| `verify` | Bootstrap dev tooling | `bootstrap.sh` Python-tooling `.venv` install dies (~4 min → exit 1) — infra | not 0.8.9 |
| `security` | Bootstrap dev tooling | same bootstrap failure — aborts BEFORE `agent-security.sh`, so AC-037 catch + recall test never execute in CI | not 0.8.9 |
| `rust-macos` | `cargo test --workspace` | pyo3 link error (`_PyDict_GetItemWithError`, `_PyExc_*` undefined) | **0.8.8** (pyo3 0.24→0.29) |
| `rust-windows` | `cargo test --workspace` | same pyo3 link error | **0.8.8** |

**0.8.9 adds zero failures.** Every CI job that reaches the 0.8.9 changes is green: `Analyze (rust)` (compiled `recall_gate_predicate.rs`), `docs`, `default-embedder-tests`, `wheel-size-gate (linux-x64)`. AC-037 catch live-run on ubuntu-22.04 unconfirmable in CI (security job dies at bootstrap first) but catch is proven locally + by `Analyze (rust)`. **Full PR green requires 0.8.8 (pyo3 link) + a bootstrap infra fix — both out of 0.8.9 scope.**

### §4 $ ledger
**$0.00** — no priced runs; CI/test-harness + lockfile work only.

---

## Quick-reference answers
- **pyo3 pin:** `version = "0.29"`, features `["extension-module", "abi3-py310"]`.
- **gil_used:** `true` (`#[pymodule(gil_used = true)]`).
- **Cargo features:** `default=[]`, `test-hooks=[]` (only one cfg-gated in lib.rs), `default-embedder`, `default-reranker`, `embed-cuda`, `embed-metal` (latter four = engine-forwards, NOT cfg-gated in lib.rs).
- **Test-hooks-gated fns:** `PyEngine::_configure_vector_kind_for_test`, `PyEngine::_write_vector_for_test`, `force_panic_for_test`.
- **Exception root:** `EngineError ← PyException`; two nested leaves `KindNotVectorIndexedError ← VectorError`, `EmbedderNotConfiguredError ← EmbedderError`; `PanicException` is separate (not an EngineError subclass).
- **14 pyclasses, 9 standalone pyfunctions (+1 cfg-gated), 22 exceptions** registered in `_fathomdb`.
- **0.8.9 state:** Slices 0/5/10/15 CLOSED, Slice 40 in progress; only open ledger item = merge PR #93 (blocked on pre-existing main CI red from 0.8.8 pyo3 link + bootstrap infra, not 0.8.9). $0 footprint.
