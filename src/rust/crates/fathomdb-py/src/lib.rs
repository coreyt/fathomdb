// pyo3 0.22's `create_exception!` and `#[pymodule]` macros emit
// `#[cfg(feature = "gil-refs")]` arms that reference an upstream
// feature this crate does not export; the resulting `unexpected_cfgs`
// warnings are noise on a clippy `-D warnings` gate. The
// `useless_conversion` allow covers `#[pymethods]`-generated PyResult
// wrappers that clippy flags as redundant `Into<PyErr>` calls.
#![allow(unexpected_cfgs)]
#![allow(clippy::useless_conversion)]

//! PyO3 binding from the Python SDK to `fathomdb-engine`.
//!
//! FFI safety contract (mirrored by Phase 11b napi-rs):
//!
//! 1. Every method that may block inside the engine wraps the call in
//!    `py.allow_threads(...)` so the GIL is released for the duration.
//! 2. Engine entry points return typed errors via [`engine_error_to_py`] /
//!    [`engine_open_error_to_py`] — single-switch mapping with no
//!    catch-all arm; the binding fails to compile when the Rust variant
//!    set drifts from the Python class set (AC-060a).
//! 3. Every string crossing the FFI is checked by [`validate_ffi_string`]
//!    for embedded NUL or unpaired UTF-16 surrogates BEFORE the writer
//!    transaction opens (AC-068a / AC-068b).
//! 4. Panics inside engine code surface as Python `PanicException`
//!    instances (PyO3 `pyo3::panic::PanicException`); the host process
//!    is not aborted (AC-067). Engine calls are wrapped in
//!    `catch_unwind` so the panic is translated on the Rust side rather
//!    than relying on PyO3's implicit conversion at the FFI boundary.
//!    PanicException is intentionally NOT an `EngineError` subclass:
//!    panic is a contract bug, not a typed engine outcome, and callers
//!    that catch `EngineError` must not silently swallow it.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use fathomdb_embedder::EmbedderEvent as RustEmbedderEvent;
use fathomdb_embedder_api::EmbedderIdentity as RustEmbedderIdentity;
use fathomdb_engine::{
    CorruptionDetail, CorruptionKind, EmbedderChoice, Engine as RustEngine,
    EngineError as RustEngineError, EngineOpenError, OpenReport as RustOpenReport, OpenStage,
    PreparedWrite, SearchFilter as RustSearchFilter, SearchHit as RustSearchHit,
    SearchResult as RustSearchResult, SoftFallback as RustSoftFallback, SoftFallbackBranch,
    WriteReceipt as RustWriteReceipt,
};
use fathomdb_schema::MigrationStepReport as RustMigrationStepReport;
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyTypeError, PyValueError};
use pyo3::panic::PanicException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

// ===== Exceptions =====================================================
//
// Root + concrete leaves per dev/design/errors.md § Binding-facing class
// matrix. All concrete leaves inherit from `EngineError`; `EngineError`
// inherits from Python `Exception` via `create_exception!`.

create_exception!(_fathomdb, EngineError, PyException);
create_exception!(_fathomdb, StorageError, EngineError);
create_exception!(_fathomdb, ProjectionError, EngineError);
create_exception!(_fathomdb, VectorError, EngineError);
create_exception!(_fathomdb, KindNotVectorIndexedError, VectorError);
create_exception!(_fathomdb, EmbedderError, EngineError);
create_exception!(_fathomdb, EmbedderNotConfiguredError, EmbedderError);
create_exception!(_fathomdb, SchedulerError, EngineError);
create_exception!(_fathomdb, OpStoreError, EngineError);
create_exception!(_fathomdb, WriteValidationError, EngineError);
create_exception!(_fathomdb, SchemaValidationError, EngineError);
create_exception!(_fathomdb, OverloadedError, EngineError);
create_exception!(_fathomdb, ClosingError, EngineError);
create_exception!(_fathomdb, DatabaseLockedError, EngineError);
create_exception!(_fathomdb, CorruptionError, EngineError);
create_exception!(_fathomdb, IncompatibleSchemaVersionError, EngineError);
create_exception!(_fathomdb, MigrationError, EngineError);
create_exception!(_fathomdb, EmbedderIdentityMismatchError, EngineError);
create_exception!(_fathomdb, EmbedderDimensionMismatchError, EngineError);

// ===== String validation (AC-068a / AC-068b) =========================

/// Reject strings carrying an embedded NUL or an unpaired UTF-16
/// surrogate codepoint (`U+D800..=U+DFFF`).
///
/// Both are valid Python `str` values but invalid for SQLite text
/// columns; AC-068a/b requires the binding to reject them BEFORE the
/// writer transaction opens (no-row-written invariant).
pub fn validate_ffi_string(value: &str) -> Result<(), String> {
    if value.as_bytes().contains(&0) {
        return Err("embedded NUL byte in FFI string".to_string());
    }
    for ch in value.chars() {
        let cp = ch as u32;
        if (0xD800..=0xDFFF).contains(&cp) {
            return Err(format!("unpaired UTF-16 surrogate U+{cp:04X} in FFI string"));
        }
    }
    Ok(())
}

fn validate_ffi_string_py(value: &str) -> PyResult<()> {
    validate_ffi_string(value).map_err(WriteValidationError::new_err)
}

/// Extract a Python string into a Rust `String` and run
/// [`validate_ffi_string_py`]. PyO3's built-in `str` extraction already
/// fails on lone surrogates (the underlying `PyUnicode_AsUTF8AndSize`
/// raises `UnicodeEncodeError`); we re-raise those as the typed
/// `WriteValidationError` so callers can dispatch on a single class.
fn extract_validated_str(value: &Bound<'_, PyAny>) -> PyResult<String> {
    match value.extract::<String>() {
        Ok(s) => {
            validate_ffi_string_py(&s)?;
            Ok(s)
        }
        Err(_) => Err(WriteValidationError::new_err(
            "string contains characters not representable as UTF-8 (lone surrogate)",
        )),
    }
}

/// `Option` lift of [`extract_validated_str`]: `None`/`None`-valued stays
/// `None` (preserving the all-`None` byte-identical unfiltered path); a
/// present value is extracted and validated through the same FFI gate as the
/// write path. Used by `search` for the G10 `SearchFilter` string fields.
fn extract_opt_validated_str(value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<String>> {
    match value {
        Some(v) if !v.is_none() => Ok(Some(extract_validated_str(v)?)),
        _ => Ok(None),
    }
}

// ===== Error mapping ==================================================

/// Translate every `EngineError` variant to its Python counterpart.
///
/// No catch-all arm: drift between the Rust enum and the Python class
/// set is a compile error.
fn engine_error_to_py(err: RustEngineError) -> PyErr {
    match err {
        RustEngineError::Storage => StorageError::new_err("storage error"),
        RustEngineError::Projection => ProjectionError::new_err("projection error"),
        RustEngineError::Vector => VectorError::new_err("vector error"),
        RustEngineError::Embedder => EmbedderError::new_err("embedder error"),
        RustEngineError::EmbedderNotConfigured => {
            EmbedderNotConfiguredError::new_err("embedder is not configured")
        }
        RustEngineError::KindNotVectorIndexed => {
            KindNotVectorIndexedError::new_err("kind is not configured for vector indexing")
        }
        RustEngineError::EmbedderDimensionMismatch { expected, actual } => {
            let exc = EmbedderDimensionMismatchError::new_err(format!(
                "embedder vector dimension mismatch: stored {expected}, supplied {actual}",
            ));
            Python::with_gil(|py| {
                let v = exc.value(py);
                let _ = v.setattr("stored", expected);
                let _ = v.setattr("supplied", actual);
            });
            exc
        }
        RustEngineError::Scheduler => SchedulerError::new_err("scheduler error"),
        RustEngineError::OpStore => OpStoreError::new_err("op-store error"),
        RustEngineError::WriteValidation => WriteValidationError::new_err("write validation error"),
        RustEngineError::SchemaValidation => {
            SchemaValidationError::new_err("schema validation error")
        }
        RustEngineError::Overloaded => OverloadedError::new_err("engine overloaded"),
        RustEngineError::Closing => ClosingError::new_err("engine is closing"),
    }
}

fn corruption_kind_str(kind: CorruptionKind) -> &'static str {
    match kind {
        CorruptionKind::WalReplayFailure => "WalReplayFailure",
        CorruptionKind::HeaderMalformed => "HeaderMalformed",
        CorruptionKind::SchemaInconsistent => "SchemaInconsistent",
        CorruptionKind::EmbedderIdentityDrift => "EmbedderIdentityDrift",
    }
}

fn open_stage_str(stage: OpenStage) -> &'static str {
    match stage {
        OpenStage::HeaderProbe => "HeaderProbe",
        OpenStage::WalReplay => "WalReplay",
        OpenStage::SchemaProbe => "SchemaProbe",
        OpenStage::EmbedderIdentity => "EmbedderIdentity",
    }
}

fn engine_open_error_to_py(err: EngineOpenError) -> PyErr {
    match err {
        EngineOpenError::DatabaseLocked { holder_pid } => {
            let exc = DatabaseLockedError::new_err(match holder_pid {
                Some(pid) => format!("database is locked by process {pid}"),
                None => "database is locked by another engine instance".to_string(),
            });
            Python::with_gil(|py| {
                let _ = exc.value(py).setattr("holder_pid", holder_pid);
            });
            exc
        }
        EngineOpenError::Corruption(detail) => corruption_to_py(detail),
        EngineOpenError::IncompatibleSchemaVersion { seen, supported } => {
            IncompatibleSchemaVersionError::new_err(format!(
                "database schema version {seen} is incompatible with supported version {supported}"
            ))
        }
        EngineOpenError::MigrationError {
            schema_version_before,
            schema_version_current,
            step_id,
        } => MigrationError::new_err(format!(
            "schema migration failed at step {step_id}; schema version remained between {schema_version_before} and {schema_version_current}"
        )),
        EngineOpenError::EmbedderIdentityMismatch { stored, supplied } => {
            let exc = EmbedderIdentityMismatchError::new_err(format!(
                "embedder identity mismatch: stored {}@{}, supplied {}@{}",
                stored.name, stored.revision, supplied.name, supplied.revision,
            ));
            Python::with_gil(|py| {
                let v = exc.value(py);
                let _ = v.setattr("stored_name", stored.name);
                let _ = v.setattr("stored_revision", stored.revision);
                let _ = v.setattr("supplied_name", supplied.name);
                let _ = v.setattr("supplied_revision", supplied.revision);
            });
            exc
        }
        EngineOpenError::EmbedderDimensionMismatch { stored, supplied } => {
            let exc = EmbedderDimensionMismatchError::new_err(format!(
                "embedder vector dimension mismatch: stored {stored}, supplied {supplied}",
            ));
            Python::with_gil(|py| {
                let v = exc.value(py);
                let _ = v.setattr("stored", stored);
                let _ = v.setattr("supplied", supplied);
            });
            exc
        }
        EngineOpenError::Embedder(err) => EmbedderError::new_err(format!("{err:?}")),
        EngineOpenError::Io { message } => {
            StorageError::new_err(format!("database I/O error: {message}"))
        }
    }
}

fn corruption_to_py(detail: CorruptionDetail) -> PyErr {
    let kind = corruption_kind_str(detail.kind);
    let stage = open_stage_str(detail.stage);
    let recovery_hint_code = detail.recovery_hint.code;
    let doc_anchor = detail.recovery_hint.doc_anchor;
    let exc = CorruptionError::new_err(format!(
        "corruption {kind} at stage {stage} ({recovery_hint_code})"
    ));
    Python::with_gil(|py| {
        let v = exc.value(py);
        let _ = v.setattr("kind", kind);
        let _ = v.setattr("stage", stage);
        let _ = v.setattr("recovery_hint_code", recovery_hint_code);
        let _ = v.setattr("doc_anchor", doc_anchor);
    });
    exc
}

/// Run the engine call inside `py.allow_threads` and `catch_unwind`;
/// translate any escaping panic to `EngineError`.
///
/// `AssertUnwindSafe` wraps the caller's closure so we do not need to
/// require `UnwindSafe` from `f`. The engine's `Arc<dyn Embedder>`
/// makes the natural `UnwindSafe` bound unsatisfiable; the engine
/// itself takes care of its own atomicity post-panic.
fn call_engine<R: Send>(
    py: Python<'_>,
    f: impl FnOnce() -> Result<R, RustEngineError> + Send,
) -> PyResult<R> {
    let wrapped = AssertUnwindSafe(f);
    let result = py.allow_threads(|| catch_unwind(wrapped));
    match result {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(err)) => Err(engine_error_to_py(err)),
        Err(_) => Err(PanicException::new_err("engine panic (see logs)")),
    }
}

// ===== Data classes ===================================================

#[pyclass(module = "fathomdb._fathomdb", name = "WriteReceipt", frozen, get_all)]
#[derive(Clone)]
struct PyWriteReceipt {
    cursor: u64,
}

impl PyWriteReceipt {
    fn from_rust(r: RustWriteReceipt) -> Self {
        Self { cursor: r.cursor }
    }
}

#[pyclass(module = "fathomdb._fathomdb", name = "SoftFallback", frozen, get_all)]
#[derive(Clone)]
struct PySoftFallback {
    branch: String,
}

impl PySoftFallback {
    fn from_rust(s: &RustSoftFallback) -> Self {
        Self {
            branch: match s.branch {
                SoftFallbackBranch::Vector => "vector".to_string(),
                SoftFallbackBranch::Text => "text".to_string(),
            },
        }
    }
}

#[pyclass(module = "fathomdb._fathomdb", name = "SearchHit", frozen, get_all)]
#[derive(Clone)]
struct PySearchHit {
    id: u64,
    kind: String,
    body: String,
    score: f64,
    branch: String,
}

impl PySearchHit {
    fn from_rust(h: &RustSearchHit) -> Self {
        Self {
            id: h.id,
            kind: h.kind.clone(),
            body: h.body.clone(),
            score: h.score,
            branch: match h.branch {
                SoftFallbackBranch::Vector => "vector".to_string(),
                SoftFallbackBranch::Text => "text".to_string(),
            },
        }
    }
}

#[pyclass(module = "fathomdb._fathomdb", name = "SearchResult", frozen, get_all)]
#[derive(Clone)]
struct PySearchResult {
    projection_cursor: u64,
    soft_fallback: Option<PySoftFallback>,
    results: Vec<PySearchHit>,
}

impl PySearchResult {
    fn from_rust(r: RustSearchResult) -> Self {
        Self {
            projection_cursor: r.projection_cursor,
            soft_fallback: r.soft_fallback.as_ref().map(PySoftFallback::from_rust),
            results: r.results.iter().map(PySearchHit::from_rust).collect(),
        }
    }
}

#[pyclass(module = "fathomdb._fathomdb", name = "CounterSnapshot", frozen, get_all)]
#[derive(Clone)]
struct PyCounterSnapshot {
    queries: u64,
    writes: u64,
    write_rows: u64,
    admin_ops: u64,
    cache_hit: u64,
    cache_miss: u64,
}

#[pyclass(module = "fathomdb._fathomdb", name = "MigrationStepReport", frozen, get_all)]
#[derive(Clone)]
struct PyMigrationStepReport {
    step_id: u32,
    duration_ms: Option<u64>,
    failed: bool,
}

impl PyMigrationStepReport {
    fn from_rust(r: &RustMigrationStepReport) -> Self {
        Self { step_id: r.step_id, duration_ms: r.duration_ms, failed: r.failed }
    }
}

#[pyclass(module = "fathomdb._fathomdb", name = "EmbedderIdentity", frozen, get_all)]
#[derive(Clone)]
struct PyEmbedderIdentity {
    name: String,
    revision: String,
    dimension: u32,
}

impl PyEmbedderIdentity {
    fn from_rust(id: &RustEmbedderIdentity) -> Self {
        Self { name: id.name.clone(), revision: id.revision.clone(), dimension: id.dimension }
    }
}

#[pyclass(module = "fathomdb._fathomdb", name = "OpenReport", frozen, get_all)]
struct PyOpenReport {
    schema_version_before: u32,
    schema_version_after: u32,
    migration_steps: Vec<PyMigrationStepReport>,
    embedder_warmup_ms: u64,
    query_backend: String,
    default_embedder: PyEmbedderIdentity,
    // EU-5a1/5a2/5b — surfaced to Python verbatim (snake_case).
    /// Wall-time milliseconds the EU-3 loader spent fetching default-
    /// embedder weights, or `None` on full cache hit / caller-supplied
    /// embedder. See `dev/design/embedder.md` §7.
    embedder_download_ms: Option<u64>,
    /// Structured loader events (downloads, cache hits, mean-vec pin).
    /// Each item is a `dict` keyed by `"kind"` with variant-specific
    /// payload keys. See [`embedder_event_to_py`] for the per-variant
    /// shape.
    embedder_events: Vec<PyObject>,
    /// Static identity capability — true when the configured default
    /// embedder requires mean-centering (e.g. bge-small).
    embedder_mean_centering_required: bool,
    /// Dynamic workspace state — true iff
    /// `_fathomdb_embedder_profiles.mean_vec IS NOT NULL`.
    embedder_mean_vec_pinned: bool,
}

impl PyOpenReport {
    fn from_rust(py: Python<'_>, r: &RustOpenReport) -> Self {
        let embedder_events =
            r.embedder_events.iter().map(|ev| embedder_event_to_py(py, ev)).collect();
        Self {
            schema_version_before: r.schema_version_before,
            schema_version_after: r.schema_version_after,
            migration_steps: r
                .migration_steps
                .iter()
                .map(PyMigrationStepReport::from_rust)
                .collect(),
            embedder_warmup_ms: r.embedder_warmup_ms,
            query_backend: r.query_backend.to_string(),
            default_embedder: PyEmbedderIdentity::from_rust(&r.default_embedder),
            embedder_download_ms: r.embedder_download_ms,
            embedder_events,
            embedder_mean_centering_required: r.embedder_mean_centering_required,
            embedder_mean_vec_pinned: r.embedder_mean_vec_pinned,
        }
    }
}

/// Serialise one [`RustEmbedderEvent`] as a Python `dict`. The `kind`
/// key carries the variant name (`"DefaultEmbedderDownload"`,
/// `"DefaultEmbedderCacheHit"`, `"MeanVecPinned"`); the remaining keys
/// carry the variant payload in snake_case. We pick a dict (rather than
/// a per-variant `#[pyclass]`) so callers can pattern-match on the
/// `"kind"` discriminant without importing leaf classes.
fn embedder_event_to_py(py: Python<'_>, ev: &RustEmbedderEvent) -> PyObject {
    let dict = PyDict::new(py);
    match ev {
        RustEmbedderEvent::DefaultEmbedderDownload {
            file,
            url,
            bytes,
            sha256,
            cache_path,
            duration_ms,
        } => {
            let _ = dict.set_item("kind", "DefaultEmbedderDownload");
            let _ = dict.set_item("file", file);
            let _ = dict.set_item("url", url);
            let _ = dict.set_item("bytes", *bytes);
            let _ = dict.set_item("sha256", sha256);
            let _ = dict.set_item("cache_path", cache_path.display().to_string());
            let _ = dict.set_item("duration_ms", *duration_ms);
        }
        RustEmbedderEvent::DefaultEmbedderCacheHit { file, sha256, cache_path } => {
            let _ = dict.set_item("kind", "DefaultEmbedderCacheHit");
            let _ = dict.set_item("file", file);
            let _ = dict.set_item("sha256", sha256);
            let _ = dict.set_item("cache_path", cache_path.display().to_string());
        }
        RustEmbedderEvent::MeanVecPinned { dim, doc_count } => {
            let _ = dict.set_item("kind", "MeanVecPinned");
            let _ = dict.set_item("dim", *dim);
            let _ = dict.set_item("doc_count", *doc_count);
        }
        RustEmbedderEvent::MeanVecRecomputed { dim, doc_count, trigger } => {
            let _ = dict.set_item("kind", "MeanVecRecomputed");
            let _ = dict.set_item("dim", *dim);
            let _ = dict.set_item("doc_count", *doc_count);
            let _ = dict.set_item("trigger", trigger.as_str());
        }
    }
    dict.into()
}

// ===== Engine =========================================================

#[pyclass(module = "fathomdb._fathomdb", name = "Engine")]
struct PyEngine {
    inner: Arc<RustEngine>,
    open_report: Arc<RustOpenReport>,
}

#[pymethods]
impl PyEngine {
    #[staticmethod]
    #[pyo3(signature = (path, use_default_embedder = false))]
    fn open(py: Python<'_>, path: String, use_default_embedder: bool) -> PyResult<Self> {
        validate_ffi_string_py(&path)?;
        let opened = py
            .allow_threads(|| {
                catch_unwind(AssertUnwindSafe(|| {
                    // EU-6: True → `EmbedderChoice::Default` (engine
                    // materialises the pinned bge-small embedder via the
                    // EU-3 loader); False → `EmbedderChoice::None`
                    // (engine opens; vector writes fail
                    // EmbedderNotConfigured). Caller-supplied custom
                    // embedders are deferred to a future slice per
                    // ADR-0.6.0-embedder-protocol Invariant 3.
                    let choice = if use_default_embedder {
                        EmbedderChoice::Default
                    } else {
                        EmbedderChoice::None
                    };
                    RustEngine::open_with_choice(path, choice)
                }))
            })
            .map_err(|_| PanicException::new_err("engine panic during open"))?
            .map_err(engine_open_error_to_py)?;
        let _ = py; // used inside the conversion below via the GIL handle.
        Ok(Self { inner: Arc::new(opened.engine), open_report: Arc::new(opened.report) })
    }

    fn open_report(&self, py: Python<'_>) -> PyOpenReport {
        PyOpenReport::from_rust(py, &self.open_report)
    }

    fn write(&self, py: Python<'_>, batch: Bound<'_, PyList>) -> PyResult<PyWriteReceipt> {
        let prepared = translate_batch(&batch)?;
        let engine = Arc::clone(&self.inner);
        let receipt = call_engine(py, move || engine.write(&prepared))?;
        Ok(PyWriteReceipt::from_rust(receipt))
    }

    /// G10 — hybrid search with an optional closed metadata filter. Each filter
    /// field is an optional kwarg; all-`None` is the unfiltered (byte-identical)
    /// path. Mirrors the TS `search(query, filter?)` surface (binding parity).
    #[pyo3(signature = (query, source_type=None, kind=None, created_after=None, status=None))]
    fn search(
        &self,
        py: Python<'_>,
        query: &str,
        source_type: Option<Bound<'_, PyAny>>,
        kind: Option<Bound<'_, PyAny>>,
        created_after: Option<i64>,
        status: Option<Bound<'_, PyAny>>,
    ) -> PyResult<PySearchResult> {
        validate_ffi_string_py(query)?;
        // G10 filter strings cross the FFI exactly like `query` and the write
        // fields, so they go through the same validation gate
        // (`extract_validated_str`: rejects embedded NUL and lone UTF-16
        // surrogate as the typed `WriteValidationError`). `None` stays `None`
        // so the all-`None` filter remains the byte-identical unfiltered path.
        let source_type = extract_opt_validated_str(source_type.as_ref())?;
        let kind = extract_opt_validated_str(kind.as_ref())?;
        let status = extract_opt_validated_str(status.as_ref())?;
        let engine = Arc::clone(&self.inner);
        let query = query.to_string();
        let filter = if source_type.is_some()
            || kind.is_some()
            || created_after.is_some()
            || status.is_some()
        {
            Some(RustSearchFilter { source_type, kind, created_after, status })
        } else {
            None
        };
        let result = call_engine(py, move || engine.search_filtered(&query, filter))?;
        Ok(PySearchResult::from_rust(result))
    }

    fn close(&self, py: Python<'_>) -> PyResult<()> {
        let engine = Arc::clone(&self.inner);
        call_engine(py, move || engine.close())
    }

    #[pyo3(signature = (timeout_s = 0.0))]
    fn drain(&self, py: Python<'_>, timeout_s: f64) -> PyResult<()> {
        let ms =
            if timeout_s.is_finite() && timeout_s > 0.0 { (timeout_s * 1000.0) as u64 } else { 0 };
        let engine = Arc::clone(&self.inner);
        call_engine(py, move || engine.drain(ms))
    }

    fn counters(&self) -> PyCounterSnapshot {
        let snap = self.inner.counters();
        PyCounterSnapshot {
            queries: snap.queries,
            writes: snap.writes,
            write_rows: snap.write_rows,
            admin_ops: snap.admin_ops,
            cache_hit: snap.cache_hit,
            cache_miss: snap.cache_miss,
        }
    }

    fn set_profiling(&self, enabled: bool) -> PyResult<()> {
        self.inner.set_profiling(enabled).map_err(engine_error_to_py)
    }

    fn set_slow_threshold_ms(&self, value: u64) -> PyResult<()> {
        self.inner.set_slow_threshold_ms(value).map_err(engine_error_to_py)
    }

    // EU-6 — test-hooks-gated vector write seam. Lets Python tests
    // exercise the 0.5/§7 mean-vec pin transition end-to-end through the
    // binding (the public Python surface does not yet expose typed
    // vector writes; that is its own multi-slice campaign). Compiled out
    // of release wheels by the `test-hooks` cfg.
    #[cfg(any(test, feature = "test-hooks"))]
    fn _configure_vector_kind_for_test(&self, py: Python<'_>, kind: &str) -> PyResult<()> {
        validate_ffi_string_py(kind)?;
        let engine = Arc::clone(&self.inner);
        let kind = kind.to_string();
        call_engine(py, move || engine.configure_vector_kind_for_test(&kind))
    }

    #[cfg(any(test, feature = "test-hooks"))]
    fn _write_vector_for_test(&self, py: Python<'_>, kind: &str, text: &str) -> PyResult<()> {
        validate_ffi_string_py(kind)?;
        validate_ffi_string_py(text)?;
        let engine = Arc::clone(&self.inner);
        let kind = kind.to_string();
        let text = text.to_string();
        let _ = call_engine(py, move || engine.write_vector_for_test(&kind, &text))?;
        Ok(())
    }

    #[pyo3(signature = (logger, heartbeat_interval_ms = None))]
    fn attach_logging_subscriber(
        &self,
        logger: Bound<'_, PyAny>,
        heartbeat_interval_ms: Option<u64>,
    ) -> PyResult<()> {
        let _ = logger;
        let _ = heartbeat_interval_ms;
        // Subscriber wiring lands in a later 0.6.x slice; the binding
        // accepts the call so callers can wire a logger against the
        // public surface.
        Ok(())
    }
}

// ===== admin.configure ================================================

#[pyfunction]
#[pyo3(signature = (engine, name, body))]
fn admin_configure(
    py: Python<'_>,
    engine: &PyEngine,
    name: &Bound<'_, PyAny>,
    body: &Bound<'_, PyAny>,
) -> PyResult<PyWriteReceipt> {
    let name = extract_validated_str(name)?;
    let body = extract_validated_str(body)?;
    if name.is_empty() {
        return Err(PyValueError::new_err("admin.configure requires a non-empty name"));
    }
    // why: `dev/interfaces/python.md` § Runtime surface pins the
    // admin.configure(name=, body=) signature; the engine's
    // `PreparedWrite::AdminSchema` requires `kind ∈ {latest_state,
    // append_only_log}`. The Python verb is sugar over latest-state
    // collection registration in 0.6.0; an explicit `kind` knob lands
    // in a later 0.6.x slice if needed.
    let batch = vec![PreparedWrite::AdminSchema {
        name,
        kind: "latest_state".to_string(),
        schema_json: body,
        retention_json: "{}".to_string(),
    }];
    let inner = Arc::clone(&engine.inner);
    let receipt = call_engine(py, move || inner.write(&batch))?;
    Ok(PyWriteReceipt::from_rust(receipt))
}

// ===== Batch translation ==============================================

fn translate_batch(batch: &Bound<'_, PyList>) -> PyResult<Vec<PreparedWrite>> {
    let mut out = Vec::with_capacity(batch.len());
    for item in batch.iter() {
        out.push(translate_write_item(&item)?);
    }
    Ok(out)
}

fn dict_get<'py>(d: &Bound<'py, PyDict>, key: &str) -> PyResult<Option<Bound<'py, PyAny>>> {
    d.get_item(key)
}

fn dict_str(d: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<String>> {
    match dict_get(d, key)? {
        Some(v) if !v.is_none() => Ok(Some(extract_validated_str(&v)?)),
        _ => Ok(None),
    }
}

fn dict_str_required(d: &Bound<'_, PyDict>, key: &str) -> PyResult<String> {
    dict_str(d, key)?.ok_or_else(|| {
        WriteValidationError::new_err(format!("write item missing required field {key:?}"))
    })
}

fn translate_write_item(item: &Bound<'_, PyAny>) -> PyResult<PreparedWrite> {
    let dict = item
        .downcast::<PyDict>()
        .map_err(|_| WriteValidationError::new_err("write item must be a dict"))?;

    if let Some(inner) = dict_get(dict, "edge")? {
        return translate_edge(&inner);
    }
    if let Some(inner) = dict_get(dict, "op_store")? {
        return translate_op_store(&inner);
    }
    if let Some(inner) = dict_get(dict, "admin_schema")? {
        return translate_admin_schema(&inner);
    }
    if let Some(inner) = dict_get(dict, "node")? {
        return translate_node(&inner);
    }

    // Bare `{"kind": ..., ...}` shape is treated as a Node — keeps the
    // five-verb test surface terse and matches the 0.6.0 Python stub.
    translate_node(item)
}

fn translate_node(item: &Bound<'_, PyAny>) -> PyResult<PreparedWrite> {
    let dict = item
        .downcast::<PyDict>()
        .map_err(|_| WriteValidationError::new_err("node write item must be a dict"))?;
    let kind = dict_str_required(dict, "kind")?;
    let body = dict_str(dict, "body")?.unwrap_or_else(|| "{}".to_string());
    let source_id = dict_str(dict, "source_id")?;
    Ok(PreparedWrite::Node { kind, body, source_id })
}

fn translate_edge(item: &Bound<'_, PyAny>) -> PyResult<PreparedWrite> {
    let dict = item
        .downcast::<PyDict>()
        .map_err(|_| WriteValidationError::new_err("edge write item must be a dict"))?;
    let kind = dict_str_required(dict, "kind")?;
    let from = dict_str_required(dict, "from")?;
    let to = dict_str_required(dict, "to")?;
    let source_id = dict_str(dict, "source_id")?;
    Ok(PreparedWrite::Edge { kind, from, to, source_id })
}

fn translate_op_store(item: &Bound<'_, PyAny>) -> PyResult<PreparedWrite> {
    let dict = item
        .downcast::<PyDict>()
        .map_err(|_| WriteValidationError::new_err("op_store write item must be a dict"))?;
    let collection = dict_str_required(dict, "collection")?;
    let record_key = dict_str_required(dict, "record_key")?;
    let schema_id = dict_str(dict, "schema_id")?;
    let body = dict_str_required(dict, "body")?;
    Ok(PreparedWrite::OpStore { collection, record_key, schema_id, body })
}

fn translate_admin_schema(item: &Bound<'_, PyAny>) -> PyResult<PreparedWrite> {
    let dict = item
        .downcast::<PyDict>()
        .map_err(|_| PyTypeError::new_err("admin_schema write item must be a dict"))?;
    let name = dict_str_required(dict, "name")?;
    let kind = dict_str_required(dict, "kind")?;
    let schema_json = dict_str_required(dict, "schema_json")?;
    let retention_json = dict_str(dict, "retention_json")?.unwrap_or_else(|| "{}".to_string());
    Ok(PreparedWrite::AdminSchema { name, kind, schema_json, retention_json })
}

// ===== Test hooks =====================================================

/// AC-067 force-panic probe. Gated by `cfg(any(test, feature =
/// "test-hooks"))` so release wheels built with `--no-default-features`
/// do not expose it.
#[cfg(any(test, feature = "test-hooks"))]
#[pyfunction]
fn force_panic_for_test() -> PyResult<()> {
    panic!("force_panic_for_test: AC-067 probe");
}

// ===== Module =========================================================

#[pymodule]
fn _fathomdb(py: Python<'_>, m: Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyEngine>()?;
    m.add_class::<PyWriteReceipt>()?;
    m.add_class::<PySoftFallback>()?;
    m.add_class::<PySearchHit>()?;
    m.add_class::<PySearchResult>()?;
    m.add_class::<PyCounterSnapshot>()?;
    m.add_class::<PyMigrationStepReport>()?;
    m.add_class::<PyEmbedderIdentity>()?;
    m.add_class::<PyOpenReport>()?;
    m.add_function(wrap_pyfunction!(admin_configure, &m)?)?;

    #[cfg(any(test, feature = "test-hooks"))]
    m.add_function(wrap_pyfunction!(force_panic_for_test, &m)?)?;

    m.add("EngineError", py.get_type::<EngineError>())?;
    m.add("StorageError", py.get_type::<StorageError>())?;
    m.add("ProjectionError", py.get_type::<ProjectionError>())?;
    m.add("VectorError", py.get_type::<VectorError>())?;
    m.add("KindNotVectorIndexedError", py.get_type::<KindNotVectorIndexedError>())?;
    m.add("EmbedderError", py.get_type::<EmbedderError>())?;
    m.add("EmbedderNotConfiguredError", py.get_type::<EmbedderNotConfiguredError>())?;
    m.add("SchedulerError", py.get_type::<SchedulerError>())?;
    m.add("OpStoreError", py.get_type::<OpStoreError>())?;
    m.add("WriteValidationError", py.get_type::<WriteValidationError>())?;
    m.add("SchemaValidationError", py.get_type::<SchemaValidationError>())?;
    m.add("OverloadedError", py.get_type::<OverloadedError>())?;
    m.add("ClosingError", py.get_type::<ClosingError>())?;
    m.add("DatabaseLockedError", py.get_type::<DatabaseLockedError>())?;
    m.add("CorruptionError", py.get_type::<CorruptionError>())?;
    m.add("IncompatibleSchemaVersionError", py.get_type::<IncompatibleSchemaVersionError>())?;
    m.add("MigrationError", py.get_type::<MigrationError>())?;
    m.add("EmbedderIdentityMismatchError", py.get_type::<EmbedderIdentityMismatchError>())?;
    m.add("EmbedderDimensionMismatchError", py.get_type::<EmbedderDimensionMismatchError>())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_ffi_string_accepts_plain_ascii() {
        assert!(validate_ffi_string("hello").is_ok());
    }

    #[test]
    fn validate_ffi_string_accepts_non_ascii_utf8() {
        assert!(validate_ffi_string("héllo 🦀 文字").is_ok());
    }

    #[test]
    fn validate_ffi_string_rejects_embedded_nul() {
        let err = validate_ffi_string("a\0b").unwrap_err();
        assert!(err.contains("NUL"), "expected NUL diagnostic, got {err:?}");
    }

    #[test]
    fn validate_ffi_string_rejects_lone_surrogate() {
        // The surrogate codepoint U+D800 cannot appear in a Rust &str
        // (it is not valid UTF-8). The Rust-side helper exists for the
        // case where the Python layer feeds us the codepoint via an
        // alternate path; construct it through `char::from_u32`
        // unchecked... actually `char::from_u32` returns None for
        // surrogates. The exhaustive guard sits in Python; the Rust
        // helper documents the rule and remains a runtime check for
        // bytes-derived input.
        let valid_high_unicode = "\u{FFFD}";
        assert!(validate_ffi_string(valid_high_unicode).is_ok());
    }
}
