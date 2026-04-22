#![cfg(feature = "python")]
// PyO3 #[pymethods] require &self even for methods that delegate to free functions;
// Option<Vec<String>> parameters are required by the PyO3 signature contract.
#![allow(clippy::unused_self, clippy::needless_pass_by_value)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;

use crate::ffi_types::{
    FfiCompiledGroupedQuery, FfiCompiledQuery, FfiGroupedQueryRows, FfiIntegrityReport,
    FfiLastAccessTouchReport, FfiLastAccessTouchRequest, FfiProjectionRepairReport, FfiQueryAst,
    FfiQueryPlan, FfiQueryRows, FfiSafeExportManifest, FfiSemanticReport, FfiTraceReport,
    FfiVectorRegenerationReport, FfiWriteReceipt, FfiWriteRequest,
};
use crate::{
    EmbedderChoice, Engine, EngineError, EngineOptions, OperationalReadRequest,
    OperationalRegisterRequest, ProjectionTarget, ProvenanceMode, SafeExportOptions,
    TelemetryLevel, compile_grouped_query, compile_query, new_id, new_row_id,
};
use fathomdb_engine::VectorRegenerationConfig;
use fathomdb_query::CompileError as RustCompileError;

create_exception!(_fathomdb, FathomError, PyException);
create_exception!(_fathomdb, SqliteError, FathomError);
create_exception!(_fathomdb, SchemaError, FathomError);
create_exception!(_fathomdb, InvalidWriteError, FathomError);
create_exception!(_fathomdb, CapabilityMissingError, FathomError);
create_exception!(_fathomdb, WriterRejectedError, FathomError);
create_exception!(_fathomdb, WriterTimedOutError, FathomError);
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
/// `py.detach()` so the GIL is released during shutdown.
impl Drop for EngineCore {
    fn drop(&mut self) {
        let engine = self.engine.get_mut().ok().and_then(Option::take);
        if let Some(engine) = engine {
            Python::attach(|py| {
                py.detach(move || drop(engine));
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
    #[pyo3(signature = (database_path, provenance_mode, vector_dimension=None, telemetry_level=None, embedder=None))]
    pub fn open(
        py: Python<'_>,
        database_path: &str,
        provenance_mode: &str,
        vector_dimension: Option<usize>,
        telemetry_level: Option<&str>,
        embedder: Option<&str>,
    ) -> PyResult<Self> {
        let options = EngineOptions {
            database_path: PathBuf::from(database_path),
            provenance_mode: parse_provenance_mode(provenance_mode)?,
            vector_dimension,
            read_pool_size: None,
            telemetry_level: parse_telemetry_level(telemetry_level)?,
            embedder: parse_embedder_choice(embedder)?,
        };
        // Release the GIL during engine open — schema bootstrap emits tracing
        // events that pyo3-log forwards to Python logging.  Holding the GIL
        // here while another engine's writer thread also logs causes a deadlock.
        let engine = py
            .detach(|| Engine::open(options))
            .map_err(map_engine_error)?;
        Ok(Self {
            engine: RwLock::new(Some(engine)),
        })
    }

    /// Close the engine, flushing pending writes and releasing all resources.
    ///
    /// Idempotent — calling on an already-closed engine is a no-op.
    pub fn close(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| {
            let mut guard = self
                .engine
                .write()
                .map_err(|_| BridgeError::new_err("engine lock poisoned"))?;
            let _ = guard.take();
            Ok(())
        })
    }

    /// Read all telemetry counters and aggregated `SQLite` cache statistics.
    ///
    /// Returns a dict with keys: `queries_total`, `writes_total`,
    /// `write_rows_total`, `errors_total`, `admin_ops_total`, `cache_hits`,
    /// `cache_misses`, `cache_writes`, `cache_spills`.
    pub fn telemetry_snapshot(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.with_engine(|engine| {
            let snap = engine.telemetry_snapshot();
            let dict = pyo3::types::PyDict::new(py);
            dict.set_item("queries_total", snap.queries_total)?;
            dict.set_item("writes_total", snap.writes_total)?;
            dict.set_item("write_rows_total", snap.write_rows_total)?;
            dict.set_item("errors_total", snap.errors_total)?;
            dict.set_item("admin_ops_total", snap.admin_ops_total)?;
            dict.set_item("cache_hits", snap.sqlite_cache.cache_hits)?;
            dict.set_item("cache_misses", snap.sqlite_cache.cache_misses)?;
            dict.set_item("cache_writes", snap.sqlite_cache.cache_writes)?;
            dict.set_item("cache_spills", snap.sqlite_cache.cache_spills)?;
            Ok(dict.into())
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
        encode_json(FfiCompiledQuery::from(compiled))
    }

    pub fn compile_grouped_ast(&self, ast_json: &str) -> PyResult<String> {
        let ast = parse_ast(ast_json)?;
        let compiled = compile_grouped_query(&ast).map_err(map_compile_error)?;
        encode_json(FfiCompiledGroupedQuery::from(compiled))
    }

    pub fn explain_ast(&self, ast_json: &str) -> PyResult<String> {
        let ast = parse_ast(ast_json)?;
        let compiled = compile_query(&ast).map_err(map_compile_error)?;
        self.with_engine(|engine| {
            let plan = engine.coordinator().explain_compiled_read(&compiled);
            encode_json(FfiQueryPlan::from(plan))
        })
    }

    pub fn execute_ast(&self, py: Python<'_>, ast_json: &str) -> PyResult<String> {
        let ast = parse_ast(ast_json)?;
        let compiled = compile_query(&ast).map_err(map_compile_error)?;
        self.with_engine(|engine| {
            let rows = py
                .detach(|| engine.coordinator().execute_compiled_read(&compiled))
                .map_err(map_engine_error)?;
            encode_json(FfiQueryRows::from(rows))
        })
    }

    /// Execute an adaptive or fallback text search and return the serialized
    /// [`crate::search_ffi::PySearchRows`] as a JSON string. The `request_json`
    /// envelope is a [`crate::search_ffi::PySearchRequest`] (mode, strict
    /// query, optional relaxed query, filters, limit, attribution flag).
    pub fn execute_search(&self, py: Python<'_>, request_json: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            py.detach(|| crate::search_ffi::execute_search_json(engine, request_json))
                .map_err(map_search_ffi_error)
        })
    }

    pub fn execute_grouped_ast(&self, py: Python<'_>, ast_json: &str) -> PyResult<String> {
        let ast = parse_ast(ast_json)?;
        let compiled = compile_grouped_query(&ast).map_err(map_compile_error)?;
        self.with_engine(|engine| {
            let rows = py
                .detach(|| {
                    engine
                        .coordinator()
                        .execute_compiled_grouped_read(&compiled)
                })
                .map_err(map_engine_error)?;
            encode_json(FfiGroupedQueryRows::from(rows))
        })
    }

    pub fn submit_write(&self, py: Python<'_>, request_json: &str) -> PyResult<String> {
        let request = parse_write_request(request_json)?;
        self.with_engine(|engine| {
            let receipt = py
                .detach(|| engine.writer().submit(request))
                .map_err(map_engine_error)?;
            encode_json(FfiWriteReceipt::from(receipt))
        })
    }

    pub fn touch_last_accessed(&self, py: Python<'_>, request_json: &str) -> PyResult<String> {
        let request = parse_last_access_touch_request(request_json)?;
        self.with_engine(|engine| {
            let report = py
                .detach(|| engine.touch_last_accessed(request))
                .map_err(map_engine_error)?;
            encode_json(FfiLastAccessTouchReport::from(report))
        })
    }

    pub fn check_integrity(&self, py: Python<'_>) -> PyResult<String> {
        self.with_engine(|engine| {
            let admin = engine.admin().service();
            let report = py
                .detach(|| admin.check_integrity())
                .map_err(map_engine_error)?;
            encode_json(FfiIntegrityReport::from(report))
        })
    }

    pub fn check_semantics(&self, py: Python<'_>) -> PyResult<String> {
        self.with_engine(|engine| {
            let admin = engine.admin().service();
            let report = py
                .detach(|| admin.check_semantics())
                .map_err(map_engine_error)?;
            encode_json(FfiSemanticReport::from(report))
        })
    }

    pub fn rebuild_projections(&self, py: Python<'_>, target: &str) -> PyResult<String> {
        let target = parse_projection_target(target)?;
        self.with_engine(|engine| {
            let admin = engine.admin().service();
            let report = py
                .detach(|| admin.rebuild_projections(target))
                .map_err(map_engine_error)?;
            encode_json(FfiProjectionRepairReport::from(report))
        })
    }

    pub fn rebuild_missing_projections(&self, py: Python<'_>) -> PyResult<String> {
        self.with_engine(|engine| {
            let admin = engine.admin().service();
            let report = py
                .detach(|| admin.rebuild_missing_projections())
                .map_err(map_engine_error)?;
            encode_json(FfiProjectionRepairReport::from(report))
        })
    }

    pub fn restore_vector_profiles(&self, py: Python<'_>) -> PyResult<String> {
        self.with_engine(|engine| {
            let admin = engine.admin().service();
            let report = py
                .detach(|| admin.restore_vector_profiles())
                .map_err(map_engine_error)?;
            encode_json(FfiProjectionRepairReport::from(report))
        })
    }

    pub fn regenerate_vector_embeddings(
        &self,
        py: Python<'_>,
        config_json: &str,
    ) -> PyResult<String> {
        check_json_size(
            config_json,
            MAX_REQUEST_JSON_BYTES,
            "vector regeneration config",
        )?;
        let config: VectorRegenerationConfig =
            serde_json::from_str(config_json).map_err(|error| {
                PyValueError::new_err(format!("invalid vector regeneration config JSON: {error}"))
            })?;
        self.with_engine(|engine| {
            let report = py
                .detach(|| engine.regenerate_vector_embeddings(&config))
                .map_err(map_engine_error)?;
            encode_json(FfiVectorRegenerationReport::from(report))
        })
    }

    pub fn trace_source(&self, py: Python<'_>, source_ref: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            let admin = engine.admin().service();
            let report = py
                .detach(|| admin.trace_source(source_ref))
                .map_err(map_engine_error)?;
            encode_json(FfiTraceReport::from(report))
        })
    }

    pub fn excise_source(&self, py: Python<'_>, source_ref: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            let admin = engine.admin().service();
            let report = py
                .detach(|| admin.excise_source(source_ref))
                .map_err(map_engine_error)?;
            encode_json(FfiTraceReport::from(report))
        })
    }

    pub fn restore_logical_id(&self, py: Python<'_>, logical_id: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            let report = py
                .detach(|| engine.restore_logical_id(logical_id))
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    pub fn purge_logical_id(&self, py: Python<'_>, logical_id: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            let report = py
                .detach(|| engine.purge_logical_id(logical_id))
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
                .detach(|| {
                    admin.safe_export(destination_path, SafeExportOptions { force_checkpoint })
                })
                .map_err(map_engine_error)?;
            encode_json(FfiSafeExportManifest::from(manifest))
        })
    }

    #[pyo3(signature = (kind, property_paths_json, separator=None))]
    pub fn register_fts_property_schema(
        &self,
        py: Python<'_>,
        kind: &str,
        property_paths_json: &str,
        separator: Option<&str>,
    ) -> PyResult<String> {
        let paths: Vec<String> = serde_json::from_str(property_paths_json).map_err(|error| {
            PyValueError::new_err(format!("invalid property paths JSON: {error}"))
        })?;
        let kind = kind.to_owned();
        let separator = separator.map(ToOwned::to_owned);
        self.with_engine(|engine| {
            let record = py
                .detach(|| engine.register_fts_property_schema(&kind, &paths, separator.as_deref()))
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    /// Register (or update) an FTS property projection schema with
    /// per-path modes (scalar vs recursive) and optional exclude paths.
    /// The `request_json` envelope matches
    /// [`crate::admin_ffi::PyRegisterFtsPropertySchemaRequest`].
    #[pyo3(signature = (request_json))]
    pub fn register_fts_property_schema_with_entries(
        &self,
        py: Python<'_>,
        request_json: &str,
    ) -> PyResult<String> {
        self.with_engine(|engine| {
            py.detach(|| {
                crate::admin_ffi::register_fts_property_schema_with_entries_json(
                    engine,
                    request_json,
                )
            })
            .map_err(map_admin_ffi_error)
        })
    }

    #[pyo3(signature = (kind, property_paths_json, separator=None))]
    pub fn register_fts_property_schema_async(
        &self,
        py: Python<'_>,
        kind: &str,
        property_paths_json: &str,
        separator: Option<&str>,
    ) -> PyResult<String> {
        let paths: Vec<String> = serde_json::from_str(property_paths_json).map_err(|error| {
            PyValueError::new_err(format!("invalid property paths JSON: {error}"))
        })?;
        let kind = kind.to_owned();
        let separator = separator.map(ToOwned::to_owned);
        self.with_engine(|engine| {
            let record = py
                .detach(|| {
                    engine.register_fts_property_schema_async(&kind, &paths, separator.as_deref())
                })
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    pub fn get_property_fts_rebuild_progress(
        &self,
        py: Python<'_>,
        kind: &str,
    ) -> PyResult<String> {
        let kind = kind.to_owned();
        self.with_engine(|engine| {
            let progress = py
                .detach(|| engine.get_property_fts_rebuild_progress(&kind))
                .map_err(map_engine_error)?;
            encode_json(progress)
        })
    }

    pub fn describe_fts_property_schema(&self, py: Python<'_>, kind: &str) -> PyResult<String> {
        let kind = kind.to_owned();
        self.with_engine(|engine| {
            let record = py
                .detach(|| engine.describe_fts_property_schema(&kind))
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    pub fn list_fts_property_schemas(&self, py: Python<'_>) -> PyResult<String> {
        self.with_engine(|engine| {
            let records = py
                .detach(|| engine.list_fts_property_schemas())
                .map_err(map_engine_error)?;
            encode_json(records)
        })
    }

    pub fn remove_fts_property_schema(&self, py: Python<'_>, kind: &str) -> PyResult<String> {
        let kind = kind.to_owned();
        self.with_engine(|engine| {
            py.detach(|| engine.remove_fts_property_schema(&kind))
                .map_err(map_engine_error)?;
            encode_json(serde_json::json!({"removed": true}))
        })
    }

    pub fn register_operational_collection(
        &self,
        py: Python<'_>,
        request_json: &str,
    ) -> PyResult<String> {
        check_json_size(
            request_json,
            MAX_REQUEST_JSON_BYTES,
            "operational collection",
        )?;
        let request: OperationalRegisterRequest =
            serde_json::from_str(request_json).map_err(|error| {
                PyValueError::new_err(format!("invalid operational collection JSON: {error}"))
            })?;
        self.with_engine(|engine| {
            let record = py
                .detach(|| engine.register_operational_collection(&request))
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    pub fn describe_operational_collection(&self, py: Python<'_>, name: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            let record = py
                .detach(|| engine.describe_operational_collection(name))
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
        check_json_size(filter_fields_json, MAX_REQUEST_JSON_BYTES, "filter fields")?;
        self.with_engine(|engine| {
            let record = py
                .detach(|| engine.update_operational_collection_filters(name, filter_fields_json))
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
        check_json_size(validation_json, MAX_REQUEST_JSON_BYTES, "validation")?;
        self.with_engine(|engine| {
            let record = py
                .detach(|| engine.update_operational_collection_validation(name, validation_json))
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
        check_json_size(
            secondary_indexes_json,
            MAX_REQUEST_JSON_BYTES,
            "secondary indexes",
        )?;
        self.with_engine(|engine| {
            let record = py
                .detach(|| {
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
                .detach(|| engine.trace_operational_collection(collection_name, record_key))
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    pub fn read_operational_collection(
        &self,
        py: Python<'_>,
        request_json: &str,
    ) -> PyResult<String> {
        check_json_size(request_json, MAX_REQUEST_JSON_BYTES, "operational read")?;
        let request: OperationalReadRequest =
            serde_json::from_str(request_json).map_err(|error| {
                PyValueError::new_err(format!("invalid operational read JSON: {error}"))
            })?;
        self.with_engine(|engine| {
            let report = py
                .detach(|| engine.read_operational_collection(&request))
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
                .detach(|| engine.rebuild_operational_current(collection_name))
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
                .detach(|| engine.validate_operational_collection_history(collection_name))
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
                .detach(|| engine.rebuild_operational_secondary_indexes(collection_name))
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
                .detach(|| {
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
                .detach(|| {
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
                .detach(|| engine.disable_operational_collection(name))
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
                .detach(|| engine.compact_operational_collection(name, dry_run))
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
                .detach(|| engine.purge_operational_collection(name, before_timestamp))
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
        check_json_size(
            options_json,
            MAX_REQUEST_JSON_BYTES,
            "provenance purge options",
        )?;
        let options: crate::ProvenancePurgeOptions = serde_json::from_str(options_json)
            .map_err(|e| PyValueError::new_err(format!("invalid options JSON: {e}")))?;
        self.with_engine(|engine| {
            let report = py
                .detach(|| engine.purge_provenance_events(before_timestamp, &options))
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    pub fn set_fts_profile(&self, py: Python<'_>, request_json: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            py.detach(|| crate::admin_ffi::set_fts_profile_json(engine, request_json))
                .map_err(map_admin_ffi_error)
        })
    }

    pub fn get_fts_profile(&self, py: Python<'_>, kind: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            py.detach(|| crate::admin_ffi::get_fts_profile_json(engine, kind))
                .map_err(map_admin_ffi_error)
        })
    }

    pub fn set_vec_profile(&self, py: Python<'_>, request_json: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            py.detach(|| crate::admin_ffi::set_vec_profile_json(engine, request_json))
                .map_err(map_admin_ffi_error)
        })
    }

    pub fn configure_embedding(&self, py: Python<'_>, request_json: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            py.detach(|| crate::admin_ffi::configure_embedding_json(engine, request_json))
                .map_err(map_admin_ffi_error)
        })
    }

    pub fn get_vec_profile(&self, py: Python<'_>, kind: &str) -> PyResult<String> {
        self.with_engine(|engine| {
            py.detach(|| crate::admin_ffi::get_vec_profile_json(engine, kind))
                .map_err(map_admin_ffi_error)
        })
    }

    pub fn preview_projection_impact(
        &self,
        py: Python<'_>,
        kind: &str,
        facet: &str,
    ) -> PyResult<String> {
        self.with_engine(|engine| {
            py.detach(|| crate::admin_ffi::preview_projection_impact_json(engine, kind, facet))
                .map_err(map_admin_ffi_error)
        })
    }
}

const MAX_AST_JSON_BYTES: usize = 16 * 1024 * 1024; // 16 MB
const MAX_WRITE_JSON_BYTES: usize = 64 * 1024 * 1024; // 64 MB
const MAX_REQUEST_JSON_BYTES: usize = 1024 * 1024; // 1 MB — operational requests, config, options

fn parse_ast(ast_json: &str) -> PyResult<crate::QueryAst> {
    check_json_size(ast_json, MAX_AST_JSON_BYTES, "AST")?;
    let ast: FfiQueryAst = serde_json::from_str(ast_json)
        .map_err(|error| PyValueError::new_err(format!("invalid query AST JSON: {error}")))?;
    Ok(ast.into())
}

fn parse_write_request(request_json: &str) -> PyResult<crate::WriteRequest> {
    check_json_size(request_json, MAX_WRITE_JSON_BYTES, "write request")?;
    let request: FfiWriteRequest = serde_json::from_str(request_json)
        .map_err(|error| PyValueError::new_err(format!("invalid write request JSON: {error}")))?;
    Ok(request.into())
}

fn check_json_size(json: &str, max_bytes: usize, label: &str) -> PyResult<()> {
    if json.len() > max_bytes {
        return Err(PyValueError::new_err(format!(
            "{label} JSON exceeds maximum size of {max_bytes} bytes"
        )));
    }
    Ok(())
}

fn parse_last_access_touch_request(request_json: &str) -> PyResult<crate::LastAccessTouchRequest> {
    check_json_size(
        request_json,
        MAX_REQUEST_JSON_BYTES,
        "last_access touch request",
    )?;
    let request: FfiLastAccessTouchRequest =
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

fn parse_telemetry_level(level: Option<&str>) -> PyResult<TelemetryLevel> {
    match level {
        None | Some("counters") => Ok(TelemetryLevel::Counters),
        Some("statements") => Ok(TelemetryLevel::Statements),
        Some("profiling") => Ok(TelemetryLevel::Profiling),
        Some(other) => Err(PyValueError::new_err(format!(
            "invalid telemetry_level: {other} (expected counters, statements, or profiling)"
        ))),
    }
}

fn parse_embedder_choice(value: Option<&str>) -> PyResult<EmbedderChoice> {
    match value {
        None | Some("none") => Ok(EmbedderChoice::None),
        Some("builtin") => Ok(EmbedderChoice::Builtin),
        Some(other) => Err(PyValueError::new_err(format!(
            "invalid embedder: {other} (expected none or builtin)"
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

fn map_admin_ffi_error(error: crate::admin_ffi::AdminFfiError) -> PyErr {
    use crate::admin_ffi::AdminFfiError;
    match error {
        AdminFfiError::Parse(err) => PyValueError::new_err(format!("admin request parse: {err}")),
        AdminFfiError::Engine(err) => map_engine_error(err),
        AdminFfiError::Serialize(err) => {
            BridgeError::new_err(format!("admin response serialize: {err}"))
        }
    }
}

fn map_search_ffi_error(error: crate::search_ffi::SearchFfiError) -> PyErr {
    use crate::search_ffi::SearchFfiError;
    match error {
        SearchFfiError::Parse(err) => PyValueError::new_err(format!("search request parse: {err}")),
        SearchFfiError::Compile(err) => CompileError::new_err(format!("{err:?}")),
        SearchFfiError::Engine(err) => map_engine_error(err),
        SearchFfiError::Serialize(err) => {
            BridgeError::new_err(format!("search response serialize: {err}"))
        }
    }
}

fn map_engine_error(error: EngineError) -> PyErr {
    match error {
        EngineError::Sqlite(error) => SqliteError::new_err(error.to_string()),
        EngineError::Schema(error) => SchemaError::new_err(error.to_string()),
        EngineError::Io(error) => IoError::new_err(error.to_string()),
        EngineError::WriterRejected(message) => WriterRejectedError::new_err(message),
        EngineError::WriterTimedOut(message) => WriterTimedOutError::new_err(message),
        EngineError::InvalidWrite(message) => InvalidWriteError::new_err(message),
        EngineError::Bridge(message) => BridgeError::new_err(message),
        EngineError::CapabilityMissing(message) => CapabilityMissingError::new_err(message),
        EngineError::DatabaseLocked(message) => DatabaseLockedError::new_err(message),
        EngineError::InvalidConfig(message) => FathomError::new_err(message),
        EngineError::EmbedderNotConfigured => FathomError::new_err(
            "embedder not configured: open the Engine with a non-None EmbedderChoice to regenerate vector embeddings",
        ),
        EngineError::EmbeddingChangeRequiresAck { affected_kinds } => {
            FathomError::new_err(format!(
                "changing the database-wide embedding identity would invalidate {affected_kinds} enabled vector index kinds; re-invoke with acknowledge_rebuild_impact=True"
            ))
        }
    }
}

// TODO: free-threaded (gil_used = false) support deferred to a follow-up
// release per dev/notes/pyo3-0.28-upgrade-plan.md. EngineCore::Drop has a
// documented GIL deadlock invariant around pyo3-log and the writer thread
// (commit history references D-096) that needs manual review before the
// free-threaded default in pyo3 0.28 can be accepted.
#[pymodule(name = "_fathomdb", gil_used = true)]
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
    module.add(
        "WriterTimedOutError",
        module.py().get_type::<WriterTimedOutError>(),
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
    module.add_function(wrap_pyfunction!(py_list_tokenizer_presets, module)?)?;
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

/// Return the well-known tokenizer presets mapped to their FTS5 tokenizer
/// strings. This is the single source of truth for the Python SDK —
/// `fathomdb._admin.TOKENIZER_PRESETS` is computed from this function at
/// module load time.
#[pyfunction(name = "list_tokenizer_presets")]
fn py_list_tokenizer_presets() -> HashMap<String, String> {
    fathomdb_engine::TOKENIZER_PRESETS
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use pyo3::Python;
    use serde_json::Value;
    use tempfile::NamedTempFile;

    use super::{DatabaseLockedError, EngineCore, FathomError, py_list_tokenizer_presets};

    /// ARCH-006: Rust is the single source of truth for tokenizer presets.
    /// The FFI helper must surface exactly what `TOKENIZER_PRESETS` holds.
    #[test]
    fn list_tokenizer_presets_matches_engine_constant() {
        let presets = py_list_tokenizer_presets();
        let expected: std::collections::HashMap<String, String> =
            fathomdb_engine::TOKENIZER_PRESETS
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect();
        assert_eq!(presets, expected);
        assert_eq!(presets.len(), 5);
        assert_eq!(
            presets.get("recall-optimized-english").map(String::as_str),
            Some("porter unicode61 remove_diacritics 2")
        );
    }

    /// Regression: `EngineOptions` gained `read_pool_size` but the Python binding
    /// constructor was not updated, causing a compile error only visible with
    /// `--features python`.
    #[test]
    fn open_constructs_engine_options_with_all_fields() {
        let db = NamedTempFile::new().expect("temp db");
        Python::attach(|py| {
            let engine = EngineCore::open(
                py,
                db.path().to_str().expect("db path"),
                "warn",
                None,
                None,
                None,
            );
            assert!(engine.is_ok(), "open must succeed: {:?}", engine.err());
        });
    }

    #[test]
    fn close_makes_subsequent_calls_fail() {
        let db = NamedTempFile::new().expect("temp db");
        Python::attach(|py| {
            let engine = EngineCore::open(
                py,
                db.path().to_str().expect("path"),
                "warn",
                None,
                None,
                None,
            )
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
        Python::attach(|py| {
            let engine = EngineCore::open(
                py,
                db.path().to_str().expect("path"),
                "warn",
                None,
                None,
                None,
            )
            .expect("open");
            engine.close(py).expect("first close");
            engine.close(py).expect("second close");
        });
    }

    #[test]
    fn open_locked_database_raises_database_locked_error() {
        let db = NamedTempFile::new().expect("temp db");
        Python::attach(|py| {
            let _first = EngineCore::open(
                py,
                db.path().to_str().expect("path"),
                "warn",
                None,
                None,
                None,
            )
            .expect("open");
            let result = EngineCore::open(
                py,
                db.path().to_str().expect("path"),
                "warn",
                None,
                None,
                None,
            );
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
        Python::attach(|py| {
            let engine = EngineCore::open(
                py,
                db.path().to_str().expect("db path"),
                "warn",
                None,
                None,
                None,
            )
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
        Python::attach(|py| {
            let engine = EngineCore::open(
                py,
                db.path().to_str().expect("db path"),
                "warn",
                None,
                None,
                None,
            )
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
        Python::attach(|py| {
            let engine = EngineCore::open(
                py,
                db.path().to_str().expect("db path"),
                "warn",
                None,
                None,
                None,
            )
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
