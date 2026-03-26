#![cfg(feature = "python")]

use std::path::PathBuf;

use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;

use crate::python_types::{
    PyCompiledQuery, PyIntegrityReport, PyProjectionRepairReport, PyQueryAst, PyQueryPlan,
    PyQueryRows, PySafeExportManifest, PySemanticReport, PyTraceReport, PyWriteReceipt,
    PyWriteRequest,
};
use crate::{
    Engine, EngineError, EngineOptions, ProjectionTarget, ProvenanceMode, SafeExportOptions,
    compile_query, new_id, new_row_id,
};
use fathomdb_query::CompileError as RustCompileError;

create_exception!(_fathomdb, FathomError, PyException);
create_exception!(_fathomdb, SqliteError, FathomError);
create_exception!(_fathomdb, SchemaError, FathomError);
create_exception!(_fathomdb, InvalidWriteError, FathomError);
create_exception!(_fathomdb, CapabilityMissingError, FathomError);
create_exception!(_fathomdb, WriterRejectedError, FathomError);
create_exception!(_fathomdb, BridgeError, FathomError);
create_exception!(_fathomdb, IoError, FathomError);
create_exception!(_fathomdb, CompileError, FathomError);

#[pyclass(unsendable)]
pub struct EngineCore {
    engine: Engine,
}

#[pymethods]
impl EngineCore {
    #[staticmethod]
    #[pyo3(signature = (database_path, provenance_mode, vector_dimension=None))]
    pub fn open(
        database_path: &str,
        provenance_mode: &str,
        vector_dimension: Option<usize>,
    ) -> PyResult<Self> {
        let options = EngineOptions {
            database_path: PathBuf::from(database_path),
            provenance_mode: parse_provenance_mode(provenance_mode)?,
            vector_dimension,
        };
        let engine = Engine::open(options).map_err(map_engine_error)?;
        Ok(Self { engine })
    }

    pub fn compile_ast(&self, ast_json: &str) -> PyResult<String> {
        let ast = parse_ast(ast_json)?;
        let compiled = compile_query(&ast).map_err(map_compile_error)?;
        encode_json(PyCompiledQuery::from(compiled))
    }

    pub fn explain_ast(&self, ast_json: &str) -> PyResult<String> {
        let ast = parse_ast(ast_json)?;
        let compiled = compile_query(&ast).map_err(map_compile_error)?;
        let plan = self.engine.coordinator().explain_compiled_read(&compiled);
        encode_json(PyQueryPlan::from(plan))
    }

    pub fn execute_ast(&self, py: Python<'_>, ast_json: &str) -> PyResult<String> {
        let ast = parse_ast(ast_json)?;
        let compiled = compile_query(&ast).map_err(map_compile_error)?;
        let rows = py
            .allow_threads(|| self.engine.coordinator().execute_compiled_read(&compiled))
            .map_err(map_engine_error)?;
        encode_json(PyQueryRows::from(rows))
    }

    pub fn submit_write(&self, py: Python<'_>, request_json: &str) -> PyResult<String> {
        let request = parse_write_request(request_json)?;
        let receipt = py
            .allow_threads(|| self.engine.writer().submit(request))
            .map_err(map_engine_error)?;
        encode_json(PyWriteReceipt::from(receipt))
    }

    pub fn check_integrity(&self, py: Python<'_>) -> PyResult<String> {
        let admin = self.engine.admin().service();
        let report = py
            .allow_threads(|| admin.check_integrity())
            .map_err(map_engine_error)?;
        encode_json(PyIntegrityReport::from(report))
    }

    pub fn check_semantics(&self, py: Python<'_>) -> PyResult<String> {
        let admin = self.engine.admin().service();
        let report = py
            .allow_threads(|| admin.check_semantics())
            .map_err(map_engine_error)?;
        encode_json(PySemanticReport::from(report))
    }

    pub fn rebuild_projections(&self, py: Python<'_>, target: &str) -> PyResult<String> {
        let admin = self.engine.admin().service();
        let target = parse_projection_target(target)?;
        let report = py
            .allow_threads(|| admin.rebuild_projections(target))
            .map_err(map_engine_error)?;
        encode_json(PyProjectionRepairReport::from(report))
    }

    pub fn rebuild_missing_projections(&self, py: Python<'_>) -> PyResult<String> {
        let admin = self.engine.admin().service();
        let report = py
            .allow_threads(|| admin.rebuild_missing_projections())
            .map_err(map_engine_error)?;
        encode_json(PyProjectionRepairReport::from(report))
    }

    pub fn trace_source(&self, py: Python<'_>, source_ref: &str) -> PyResult<String> {
        let admin = self.engine.admin().service();
        let report = py
            .allow_threads(|| admin.trace_source(source_ref))
            .map_err(map_engine_error)?;
        encode_json(PyTraceReport::from(report))
    }

    pub fn excise_source(&self, py: Python<'_>, source_ref: &str) -> PyResult<String> {
        let admin = self.engine.admin().service();
        let report = py
            .allow_threads(|| admin.excise_source(source_ref))
            .map_err(map_engine_error)?;
        encode_json(PyTraceReport::from(report))
    }

    pub fn safe_export(
        &self,
        py: Python<'_>,
        destination_path: &str,
        force_checkpoint: bool,
    ) -> PyResult<String> {
        let admin = self.engine.admin().service();
        let manifest = py
            .allow_threads(|| {
                admin.safe_export(destination_path, SafeExportOptions { force_checkpoint })
            })
            .map_err(map_engine_error)?;
        encode_json(PySafeExportManifest::from(manifest))
    }
}

fn parse_ast(ast_json: &str) -> PyResult<crate::QueryAst> {
    let ast: PyQueryAst = serde_json::from_str(ast_json)
        .map_err(|error| PyValueError::new_err(format!("invalid query AST JSON: {error}")))?;
    Ok(ast.into())
}

fn parse_write_request(request_json: &str) -> PyResult<crate::WriteRequest> {
    let request: PyWriteRequest = serde_json::from_str(request_json)
        .map_err(|error| PyValueError::new_err(format!("invalid write request JSON: {error}")))?;
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
    }
}

#[pymodule(name = "_fathomdb")]
fn _fathomdb(module: &Bound<'_, PyModule>) -> PyResult<()> {
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
