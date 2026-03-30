#![cfg(feature = "python")]
// PyO3 #[pymethods] require &self even for methods that delegate to free functions;
// Option<Vec<String>> parameters are required by the PyO3 signature contract.
#![allow(clippy::unused_self, clippy::needless_pass_by_value)]

use std::path::PathBuf;
use std::sync::RwLock;

use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;

use crate::python_types::{
    PyCompiledGroupedQuery, PyCompiledQuery, PyGroupedQueryRows, PyIntegrityReport,
    PyLastAccessTouchReport, PyLastAccessTouchRequest, PyProjectionRepairReport, PyQueryAst,
    PyQueryPlan, PyQueryRows, PySafeExportManifest, PySemanticReport, PyTraceReport,
    PyWriteReceipt, PyWriteRequest,
};
use crate::{
    Engine, EngineError, EngineOptions, OperationalReadRequest, OperationalRegisterRequest,
    ProjectionTarget, ProvenanceMode, SafeExportOptions, compile_grouped_query, compile_query,
    new_id, new_row_id,
};
use fathomdb_query::CompileError as RustCompileError;

create_exception!(_fathomdb, FathomError, PyException);
create_exception!(_fathomdb, SqliteError, FathomError);
create_exception!(_fathomdb, SchemaError, FathomError);
create_exception!(_fathomdb, InvalidWriteError, FathomError);
create_exception!(_fathomdb, CapabilityMissingError, FathomError);
create_exception!(_fathomdb, WriterRejectedError, FathomError);
create_exception!(_fathomdb, BridgeError, FathomError);
create_exception!(_fathomdb, DatabaseLockedError, FathomError);
create_exception!(_fathomdb, IoError, FathomError);
create_exception!(_fathomdb, CompileError, FathomError);

#[pyclass(frozen)]
pub struct EngineCore {
    engine: RwLock<Option<Engine>>,
}

/// Safety net: if the user drops the Python object without calling `close()`,
/// Python GC runs Rust's `Drop` with the GIL held.  `WriterActor::Drop` calls
/// `thread.join()`, and the writer thread may need the GIL for pyo3-log calls
/// → deadlock.  This `Drop` impl takes the engine out and drops it inside
/// `allow_threads()` so the GIL is released during shutdown.
impl Drop for EngineCore {
    fn drop(&mut self) {
        let engine = self.engine.get_mut().ok().and_then(Option::take);
        if let Some(engine) = engine {
            Python::with_gil(|py| {
                py.allow_threads(move || drop(engine));
            });
        }
    }
}

impl EngineCore {
    fn with_engine<F, R>(&self, f: F) -> PyResult<R>
    where
        F: FnOnce(&Engine) -> PyResult<R>,
    {
        let guard = self
            .engine
            .read()
            .map_err(|_| BridgeError::new_err("engine lock poisoned"))?;
        match guard.as_ref() {
            Some(engine) => f(engine),
            None => Err(FathomError::new_err("engine is closed")),
        }
    }
}

#[pymethods]
impl EngineCore {
    #[staticmethod]
    #[pyo3(signature = (database_path, provenance_mode, vector_dimension=None))]
    pub fn open(
        py: Python<'_>,
        database_path: &str,
        provenance_mode: &str,
        vector_dimension: Option<usize>,
    ) -> PyResult<Self> {
        let options = EngineOptions {
            database_path: PathBuf::from(database_path),
            provenance_mode: parse_provenance_mode(provenance_mode)?,
            vector_dimension,
            read_pool_size: None,
        };
        // Release the GIL during engine open — schema bootstrap emits tracing
        // events that pyo3-log forwards to Python logging.  Holding the GIL
        // here while another engine's writer thread also logs causes a deadlock.
        let engine = py
            .allow_threads(|| Engine::open(options))
            .map_err(map_engine_error)?;
        Ok(Self {
            engine: RwLock::new(Some(engine)),
        })
    }

    /// Close the engine, flushing pending writes and releasing all resources.
    ///
    /// Idempotent — calling on an already-closed engine is a no-op.
    pub fn close(&self, py: Python<'_>) -> PyResult<()> {
        py.allow_threads(|| {
            let mut guard = self
                .engine
                .write()
                .map_err(|_| BridgeError::new_err("engine lock poisoned"))?;
            let _ = guard.take();
            Ok(())
        })
    }

    pub fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    #[pyo3(signature = (_exc_type=None, _exc_val=None, _exc_tb=None))]
    pub fn __exit__(
        &self,
        py: Python<'_>,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_val: Option<&Bound<'_, PyAny>>,
        _exc_tb: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<bool> {
        self.close(py)?;
        Ok(false)
    }

    pub fn compile_ast(&self, ast_json: &str) -> PyResult<String> {
        let ast = parse_ast(ast_json)?;
        let compiled = compile_query(&ast).map_err(map_compile_error)?;
        encode_json(PyCompiledQuery::from(compiled))
    }

    pub fn compile_grouped_ast(&self, ast_json: &str) -> PyResult<String> {
        let ast = parse_ast(ast_json)?;
        let compiled = compile_grouped_query(&ast).map_err(map_compile_error)?;
        encode_json(PyCompiledGroupedQuery::from(compiled))
    }

    pub fn explain_ast(&self, ast_json: &str) -> PyResult<String> {
        let ast = parse_ast(ast_json)?;
        let compiled = compile_query(&ast).map_err(map_compile_error)?;
        self.with_engine(|engine| {
            let plan = engine.coordinator().explain_compiled_read(&compiled);
            encode_json(PyQueryPlan::from(plan))
        })
    }

    pub fn execute_ast(&self, py: Python<'_>, ast_json: &str) -> PyResult<String> {
        let ast = parse_ast(ast_json)?;
        let compiled = compile_query(&ast).map_err(map_compile_error)?;
        self.with_engine(|engine| {
            let rows = py
                .allow_threads(|| engine.coordinator().execute_compiled_read(&compiled))
                .map_err(map_engine_error)?;
            encode_json(PyQueryRows::from(rows))
        })
    }

    pub fn execute_grouped_ast(&self, py: Python<'_>, ast_json: &str) -> PyResult<String> {
        let ast = parse_ast(ast_json)?;
        let compiled = compile_grouped_query(&ast).map_err(map_compile_error)?;
        self.with_engine(|engine| {
            let rows = py
                .allow_threads(|| {
                    engine
                        .coordinator()
                        .execute_compiled_grouped_read(&compiled)
                })
                .map_err(map_engine_error)?;
            encode_json(PyGroupedQueryRows::from(rows))
        })
    }

    pub fn submit_write(&self, py: Python<'_>, request_json: &str) -> PyResult<String> {
        let request = parse_write_request(request_json)?;
        self.with_engine(|engine| {
            let receipt = py
                .allow_threads(|| engine.writer().submit(request))
                .map_err(map_engine_error)?;
            encode_json(PyWriteReceipt::from(receipt))
        })
    }

    pub fn touch_last_accessed(&self, py: Python<'_>, request_json: &str) -> PyResult<String> {
        let request = parse_last_access_touch_request(request_json)?;
        self.with_engine(|engine| {
            let report = py
                .allow_threads(|| engine.touch_last_accessed(request))
                .map_err(map_engine_error)?;
            encode_json(PyLastAccessTouchReport::from(report))
        })
    }

    pub fn check_integrity(&self, py: Python<'_>) -> PyResult<String> {
        self.with_engine(|engine| {
            let admin = engine.admin().service();
            let report = py
                .allow_threads(|| admin.check_integrity())
                .map_err(map_engine_error)?;
            encode_json(PyIntegrityReport::from(report))
        })
    }

    pub fn check_semantics(&self, py: Python<'_>) -> PyResult<String> {
        self.with_engine(|engine| {
            let admin = engine.admin().service();
            let report = py
                .allow_threads(|| admin.check_semantics())
                .map_err(map_engine_error)?;
            encode_json(PySemanticReport::from(report))
        })
    }

    pub fn rebuild_projections(&self, py: Python<'_>, target: &str) -> PyResult<String> {
        let target = parse_projection_target(target)?;
        self.with_engine(|engine| {
            let admin = engine.admin().service();
            let report = py
                .allow_threads(|| admin.rebuild_projections(target))
                .map_err(map_engine_error)?;
            encode_json(PyProjectionRepairReport::from(report))
        })
    }

    pub fn rebuild_missing_projections(&self, py: Python<'_>) -> PyResult<String> {
        self.with_engine(|engine| {
            let admin = engine.admin().service();
            let report = py
                .allow_threads(|| admin.rebuild_missing_projections())
                .map_err(map_engine_error)?;
            encode_json(PyProjectionRepairReport::from(report))
        })
    }

    pub fn trace_source(&self, py: Python<'_>, source_ref: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            let admin = engine.admin().service();
            let report = py
                .allow_threads(|| admin.trace_source(source_ref))
                .map_err(map_engine_error)?;
            encode_json(PyTraceReport::from(report))
        })
    }

    pub fn excise_source(&self, py: Python<'_>, source_ref: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            let admin = engine.admin().service();
            let report = py
                .allow_threads(|| admin.excise_source(source_ref))
                .map_err(map_engine_error)?;
            encode_json(PyTraceReport::from(report))
        })
    }

    pub fn restore_logical_id(&self, py: Python<'_>, logical_id: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            let report = py
                .allow_threads(|| engine.restore_logical_id(logical_id))
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    pub fn purge_logical_id(&self, py: Python<'_>, logical_id: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            let report = py
                .allow_threads(|| engine.purge_logical_id(logical_id))
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    pub fn safe_export(
        &self,
        py: Python<'_>,
        destination_path: &str,
        force_checkpoint: bool,
    ) -> PyResult<String> {
        self.with_engine(|engine| {
            let admin = engine.admin().service();
            let manifest = py
                .allow_threads(|| {
                    admin.safe_export(destination_path, SafeExportOptions { force_checkpoint })
                })
                .map_err(map_engine_error)?;
            encode_json(PySafeExportManifest::from(manifest))
        })
    }

    pub fn register_operational_collection(
        &self,
        py: Python<'_>,
        request_json: &str,
    ) -> PyResult<String> {
        let request: OperationalRegisterRequest =
            serde_json::from_str(request_json).map_err(|error| {
                PyValueError::new_err(format!("invalid operational collection JSON: {error}"))
            })?;
        self.with_engine(|engine| {
            let record = py
                .allow_threads(|| engine.register_operational_collection(&request))
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    pub fn describe_operational_collection(&self, py: Python<'_>, name: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            let record = py
                .allow_threads(|| engine.describe_operational_collection(name))
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    pub fn update_operational_collection_filters(
        &self,
        py: Python<'_>,
        name: &str,
        filter_fields_json: &str,
    ) -> PyResult<String> {
        self.with_engine(|engine| {
            let record = py
                .allow_threads(|| {
                    engine.update_operational_collection_filters(name, filter_fields_json)
                })
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    pub fn update_operational_collection_validation(
        &self,
        py: Python<'_>,
        name: &str,
        validation_json: &str,
    ) -> PyResult<String> {
        self.with_engine(|engine| {
            let record = py
                .allow_threads(|| {
                    engine.update_operational_collection_validation(name, validation_json)
                })
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    pub fn update_operational_collection_secondary_indexes(
        &self,
        py: Python<'_>,
        name: &str,
        secondary_indexes_json: &str,
    ) -> PyResult<String> {
        self.with_engine(|engine| {
            let record = py
                .allow_threads(|| {
                    engine.update_operational_collection_secondary_indexes(
                        name,
                        secondary_indexes_json,
                    )
                })
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    #[pyo3(signature = (collection_name, record_key=None))]
    pub fn trace_operational_collection(
        &self,
        py: Python<'_>,
        collection_name: &str,
        record_key: Option<&str>,
    ) -> PyResult<String> {
        self.with_engine(|engine| {
            let report = py
                .allow_threads(|| engine.trace_operational_collection(collection_name, record_key))
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    pub fn read_operational_collection(
        &self,
        py: Python<'_>,
        request_json: &str,
    ) -> PyResult<String> {
        let request: OperationalReadRequest =
            serde_json::from_str(request_json).map_err(|error| {
                PyValueError::new_err(format!("invalid operational read JSON: {error}"))
            })?;
        self.with_engine(|engine| {
            let report = py
                .allow_threads(|| engine.read_operational_collection(&request))
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    #[pyo3(signature = (collection_name=None))]
    pub fn rebuild_operational_current(
        &self,
        py: Python<'_>,
        collection_name: Option<&str>,
    ) -> PyResult<String> {
        self.with_engine(|engine| {
            let report = py
                .allow_threads(|| engine.rebuild_operational_current(collection_name))
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    pub fn validate_operational_collection_history(
        &self,
        py: Python<'_>,
        collection_name: &str,
    ) -> PyResult<String> {
        self.with_engine(|engine| {
            let report = py
                .allow_threads(|| engine.validate_operational_collection_history(collection_name))
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    pub fn rebuild_operational_secondary_indexes(
        &self,
        py: Python<'_>,
        collection_name: &str,
    ) -> PyResult<String> {
        self.with_engine(|engine| {
            let report = py
                .allow_threads(|| engine.rebuild_operational_secondary_indexes(collection_name))
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    #[pyo3(signature = (now_timestamp, collection_names=None, max_collections=None))]
    pub fn plan_operational_retention(
        &self,
        py: Python<'_>,
        now_timestamp: i64,
        collection_names: Option<Vec<String>>,
        max_collections: Option<usize>,
    ) -> PyResult<String> {
        self.with_engine(|engine| {
            let report = py
                .allow_threads(|| {
                    engine.plan_operational_retention(
                        now_timestamp,
                        collection_names.as_deref(),
                        max_collections,
                    )
                })
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    #[pyo3(signature = (now_timestamp, collection_names=None, max_collections=None, dry_run=false))]
    pub fn run_operational_retention(
        &self,
        py: Python<'_>,
        now_timestamp: i64,
        collection_names: Option<Vec<String>>,
        max_collections: Option<usize>,
        dry_run: bool,
    ) -> PyResult<String> {
        self.with_engine(|engine| {
            let report = py
                .allow_threads(|| {
                    engine.run_operational_retention(
                        now_timestamp,
                        collection_names.as_deref(),
                        max_collections,
                        dry_run,
                    )
                })
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    pub fn disable_operational_collection(&self, py: Python<'_>, name: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            let record = py
                .allow_threads(|| engine.disable_operational_collection(name))
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    pub fn compact_operational_collection(
        &self,
        py: Python<'_>,
        name: &str,
        dry_run: bool,
    ) -> PyResult<String> {
        self.with_engine(|engine| {
            let report = py
                .allow_threads(|| engine.compact_operational_collection(name, dry_run))
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    pub fn purge_operational_collection(
        &self,
        py: Python<'_>,
        name: &str,
        before_timestamp: i64,
    ) -> PyResult<String> {
        self.with_engine(|engine| {
            let report = py
                .allow_threads(|| engine.purge_operational_collection(name, before_timestamp))
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    pub fn purge_provenance_events(
        &self,
        py: Python<'_>,
        before_timestamp: i64,
        options_json: &str,
    ) -> PyResult<String> {
        let options: crate::ProvenancePurgeOptions = serde_json::from_str(options_json)
            .map_err(|e| PyValueError::new_err(format!("invalid options JSON: {e}")))?;
        self.with_engine(|engine| {
            let report = py
                .allow_threads(|| engine.purge_provenance_events(before_timestamp, &options))
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }
}

const MAX_AST_JSON_BYTES: usize = 16 * 1024 * 1024; // 16 MB
const MAX_WRITE_JSON_BYTES: usize = 64 * 1024 * 1024; // 64 MB

fn parse_ast(ast_json: &str) -> PyResult<crate::QueryAst> {
    if ast_json.len() > MAX_AST_JSON_BYTES {
        return Err(PyValueError::new_err(format!(
            "AST JSON exceeds maximum size of {MAX_AST_JSON_BYTES} bytes"
        )));
    }
    let ast: PyQueryAst = serde_json::from_str(ast_json)
        .map_err(|error| PyValueError::new_err(format!("invalid query AST JSON: {error}")))?;
    Ok(ast.into())
}

fn parse_write_request(request_json: &str) -> PyResult<crate::WriteRequest> {
    if request_json.len() > MAX_WRITE_JSON_BYTES {
        return Err(PyValueError::new_err(format!(
            "write request JSON exceeds maximum size of {MAX_WRITE_JSON_BYTES} bytes"
        )));
    }
    let request: PyWriteRequest = serde_json::from_str(request_json)
        .map_err(|error| PyValueError::new_err(format!("invalid write request JSON: {error}")))?;
    Ok(request.into())
}

fn parse_last_access_touch_request(request_json: &str) -> PyResult<crate::LastAccessTouchRequest> {
    let request: PyLastAccessTouchRequest =
        serde_json::from_str(request_json).map_err(|error| {
            PyValueError::new_err(format!("invalid last_access touch request JSON: {error}"))
        })?;
    Ok(request.into())
}

fn parse_provenance_mode(mode: &str) -> PyResult<ProvenanceMode> {
    match mode {
        "warn" => Ok(ProvenanceMode::Warn),
        "require" => Ok(ProvenanceMode::Require),
        other => Err(PyValueError::new_err(format!(
            "invalid provenance_mode: {other}"
        ))),
    }
}

fn parse_projection_target(target: &str) -> PyResult<ProjectionTarget> {
    match target {
        "fts" => Ok(ProjectionTarget::Fts),
        "vec" => Ok(ProjectionTarget::Vec),
        "all" => Ok(ProjectionTarget::All),
        other => Err(PyValueError::new_err(format!(
            "invalid projection target: {other}"
        ))),
    }
}

fn encode_json<T: serde::Serialize>(value: T) -> PyResult<String> {
    serde_json::to_string(&value)
        .map_err(|error| PyValueError::new_err(format!("failed to serialize payload: {error}")))
}

fn map_compile_error(error: RustCompileError) -> PyErr {
    CompileError::new_err(error.to_string())
}

fn map_engine_error(error: EngineError) -> PyErr {
    match error {
        EngineError::Sqlite(error) => SqliteError::new_err(error.to_string()),
        EngineError::Schema(error) => SchemaError::new_err(error.to_string()),
        EngineError::Io(error) => IoError::new_err(error.to_string()),
        EngineError::WriterRejected(message) => WriterRejectedError::new_err(message),
        EngineError::InvalidWrite(message) => InvalidWriteError::new_err(message),
        EngineError::Bridge(message) => BridgeError::new_err(message),
        EngineError::CapabilityMissing(message) => CapabilityMissingError::new_err(message),
        EngineError::DatabaseLocked(message) => DatabaseLockedError::new_err(message),
    }
}

#[pymodule(name = "_fathomdb")]
fn _fathomdb(module: &Bound<'_, PyModule>) -> PyResult<()> {
    // Bridge Rust tracing/log events to Python's logging module.
    // Idempotent — safe to call on repeated import.
    pyo3_log::init();

    module.add_class::<EngineCore>()?;
    module.add("FathomError", module.py().get_type::<FathomError>())?;
    module.add("SqliteError", module.py().get_type::<SqliteError>())?;
    module.add("SchemaError", module.py().get_type::<SchemaError>())?;
    module.add(
        "InvalidWriteError",
        module.py().get_type::<InvalidWriteError>(),
    )?;
    module.add(
        "CapabilityMissingError",
        module.py().get_type::<CapabilityMissingError>(),
    )?;
    module.add(
        "WriterRejectedError",
        module.py().get_type::<WriterRejectedError>(),
    )?;
    module.add("BridgeError", module.py().get_type::<BridgeError>())?;
    module.add(
        "DatabaseLockedError",
        module.py().get_type::<DatabaseLockedError>(),
    )?;
    module.add("IoError", module.py().get_type::<IoError>())?;
    module.add("CompileError", module.py().get_type::<CompileError>())?;
    module.add_function(wrap_pyfunction!(py_new_id, module)?)?;
    module.add_function(wrap_pyfunction!(py_new_row_id, module)?)?;
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

#[pyfunction(name = "new_id")]
fn py_new_id() -> String {
    new_id()
}

#[pyfunction(name = "new_row_id")]
fn py_new_row_id() -> String {
    new_row_id()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use pyo3::Python;
    use serde_json::Value;
    use tempfile::NamedTempFile;

    use super::{DatabaseLockedError, EngineCore, FathomError};

    /// Regression: `EngineOptions` gained `read_pool_size` but the Python binding
    /// constructor was not updated, causing a compile error only visible with
    /// `--features python`.
    #[test]
    fn open_constructs_engine_options_with_all_fields() {
        let db = NamedTempFile::new().expect("temp db");
        Python::with_gil(|py| {
            let engine = EngineCore::open(py, db.path().to_str().expect("db path"), "warn", None);
            assert!(engine.is_ok(), "open must succeed: {:?}", engine.err());
        });
    }

    #[test]
    fn close_makes_subsequent_calls_fail() {
        let db = NamedTempFile::new().expect("temp db");
        Python::with_gil(|py| {
            let engine = EngineCore::open(py, db.path().to_str().expect("path"), "warn", None)
                .expect("open");
            engine.close(py).expect("close");
            let result = engine.check_integrity(py);
            assert!(result.is_err(), "call after close must fail");
            let err = result.unwrap_err();
            assert!(err.is_instance_of::<FathomError>(py));
        });
    }

    #[test]
    fn close_is_idempotent() {
        let db = NamedTempFile::new().expect("temp db");
        Python::with_gil(|py| {
            let engine = EngineCore::open(py, db.path().to_str().expect("path"), "warn", None)
                .expect("open");
            engine.close(py).expect("first close");
            engine.close(py).expect("second close");
        });
    }

    #[test]
    fn open_locked_database_raises_database_locked_error() {
        let db = NamedTempFile::new().expect("temp db");
        Python::with_gil(|py| {
            let _first = EngineCore::open(py, db.path().to_str().expect("path"), "warn", None)
                .expect("open");
            let result = EngineCore::open(py, db.path().to_str().expect("path"), "warn", None);
            match result {
                Ok(_) => panic!("second open must fail"),
                Err(err) => assert!(
                    err.is_instance_of::<DatabaseLockedError>(py),
                    "expected DatabaseLockedError"
                ),
            }
        });
    }

    /// Regression: `Engine::register_operational_collection` changed to take
    /// `&OperationalRegisterRequest` but the Python binding still passed owned.
    #[test]
    fn register_operational_collection_accepts_deserialized_request() {
        let db = NamedTempFile::new().expect("temp db");
        Python::with_gil(|py| {
            let engine = EngineCore::open(py, db.path().to_str().expect("db path"), "warn", None)
                .expect("open engine");
            let result = engine.register_operational_collection(
                py,
                r#"{
                    "name":"reg_test",
                    "kind":"append_only_log",
                    "schema_json":"{}",
                    "retention_json":"{}",
                    "format_version":1
                }"#,
            );
            assert!(result.is_ok(), "register must succeed: {:?}", result.err());
            let record: Value = serde_json::from_str(&result.unwrap()).expect("decode register");
            assert_eq!(record["name"], "reg_test");
        });
    }

    /// Regression: `Engine::read_operational_collection` changed to take
    /// `&OperationalReadRequest` but the Python binding still passed owned.
    #[test]
    fn read_operational_collection_accepts_deserialized_request() {
        let db = NamedTempFile::new().expect("temp db");
        Python::with_gil(|py| {
            let engine = EngineCore::open(py, db.path().to_str().expect("db path"), "warn", None)
                .expect("open engine");
            // Register first so the collection exists
            engine
                .register_operational_collection(
                    py,
                    r#"{
                        "name":"read_test",
                        "kind":"append_only_log",
                        "schema_json":"{}",
                        "retention_json":"{}",
                        "filter_fields_json":"[{\"name\":\"actor\",\"type\":\"string\",\"modes\":[\"exact\"]}]",
                        "format_version":1
                    }"#,
                )
                .expect("register");
            let result = engine.read_operational_collection(
                py,
                r#"{
                    "collection_name":"read_test",
                    "filters":[{"mode":"exact","field":"actor","value":"alice"}],
                    "limit":10
                }"#,
            );
            assert!(result.is_ok(), "read must succeed: {:?}", result.err());
            let report: Value = serde_json::from_str(&result.unwrap()).expect("decode read");
            assert_eq!(report["collection_name"], "read_test");
        });
    }

    #[test]
    fn engine_core_exposes_operational_admin_methods() {
        let db = NamedTempFile::new().expect("temp db");
        Python::with_gil(|py| {
            let engine = EngineCore::open(py, db.path().to_str().expect("db path"), "warn", None)
                .expect("open engine");

            let record: Value = serde_json::from_str(
                &engine
                    .register_operational_collection(
                        py,
                        r#"{
                            "name":"audit_log",
                            "kind":"append_only_log",
                            "schema_json":"{}",
                            "retention_json":"{\"mode\":\"keep_last\",\"max_rows\":2}",
                            "filter_fields_json":"[{\"name\":\"actor\",\"type\":\"string\",\"modes\":[\"exact\"]}]",
                            "format_version":1
                        }"#,
                    )
                    .expect("register"),
            )
            .expect("decode register");
            assert_eq!(record["name"], "audit_log");

            let described: Value = serde_json::from_str(
                &engine
                    .describe_operational_collection(py, "audit_log")
                    .expect("describe"),
            )
            .expect("decode describe");
            assert_eq!(described["name"], "audit_log");

            let read: Value = serde_json::from_str(
                &engine
                    .read_operational_collection(
                        py,
                        r#"{
                            "collection_name":"audit_log",
                            "filters":[{"mode":"exact","field":"actor","value":"alice"}],
                            "limit":10
                        }"#,
                    )
                    .expect("read"),
            )
            .expect("decode read");
            assert_eq!(read["collection_name"], "audit_log");

            let compacted: Value = serde_json::from_str(
                &engine
                    .compact_operational_collection(py, "audit_log", true)
                    .expect("compact"),
            )
            .expect("decode compact");
            assert_eq!(compacted["collection_name"], "audit_log");
            assert_eq!(compacted["dry_run"], true);

            let purged: Value = serde_json::from_str(
                &engine
                    .purge_operational_collection(py, "audit_log", 250)
                    .expect("purge"),
            )
            .expect("decode purge");
            assert_eq!(purged["collection_name"], "audit_log");
            assert_eq!(purged["before_timestamp"], 250);

            let disabled: Value = serde_json::from_str(
                &engine
                    .disable_operational_collection(py, "audit_log")
                    .expect("disable"),
            )
            .expect("decode disable");
            assert_eq!(disabled["name"], "audit_log");
            assert!(disabled["disabled_at"].as_i64().is_some());
        });
    }
}
