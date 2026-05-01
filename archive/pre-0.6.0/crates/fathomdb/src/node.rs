#![cfg(feature = "node")]
#![allow(clippy::needless_pass_by_value)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use napi::Result;
use napi_derive::napi;

use crate::ffi_types::{
    FfiCompiledGroupedQuery, FfiCompiledQuery, FfiGroupedQueryRows, FfiIntegrityReport,
    FfiLastAccessTouchReport, FfiLastAccessTouchRequest, FfiProjectionRepairReport, FfiQueryAst,
    FfiQueryPlan, FfiQueryRows, FfiSafeExportManifest, FfiSemanticReport, FfiTraceReport,
    FfiWriteReceipt, FfiWriteRequest,
};
use crate::node_types::{
    MAX_AST_JSON_BYTES, MAX_REQUEST_JSON_BYTES, MAX_WRITE_JSON_BYTES, check_json_size, encode_json,
    invalid_argument, map_admin_ffi_error, map_compile_error, map_engine_error,
    map_search_ffi_error, parse_embedder_choice, parse_projection_target, parse_provenance_mode,
    parse_telemetry_level,
};
use crate::{
    Engine, EngineOptions, OperationalReadRequest, OperationalRegisterRequest, ProjectionTarget,
    ProvenancePurgeOptions, SafeExportOptions, compile_grouped_query, compile_query, new_id,
    new_row_id,
};
use fathomdb_engine::VectorRegenerationConfig;

#[napi(js_name = "EngineCore")]
pub struct NodeEngineCore {
    engine: RwLock<Option<Engine>>,
}

impl NodeEngineCore {
    fn with_engine<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&Engine) -> Result<R>,
    {
        let guard = self
            .engine
            .read()
            .map_err(|_| invalid_argument("engine lock poisoned"))?;
        match guard.as_ref() {
            Some(engine) => f(engine),
            None => Err(invalid_argument("engine is closed")),
        }
    }
}

#[napi]
impl NodeEngineCore {
    #[napi(factory)]
    pub fn open(
        database_path: String,
        provenance_mode: String,
        vector_dimension: Option<u32>,
        telemetry_level: Option<String>,
        embedder: Option<String>,
        auto_drain_vector: Option<bool>,
    ) -> Result<Self> {
        let options = EngineOptions {
            database_path: PathBuf::from(database_path),
            provenance_mode: parse_provenance_mode(&provenance_mode)?,
            vector_dimension: vector_dimension.map(|value| value as usize),
            read_pool_size: None,
            telemetry_level: parse_telemetry_level(telemetry_level.as_deref())?,
            embedder: parse_embedder_choice(embedder.as_deref())?,
            auto_drain_vector: auto_drain_vector.unwrap_or(false),
        };
        let engine = Engine::open(options).map_err(map_engine_error)?;
        Ok(Self {
            engine: RwLock::new(Some(engine)),
        })
    }

    #[napi]
    pub fn close(&self) -> Result<()> {
        let mut guard = self
            .engine
            .write()
            .map_err(|_| invalid_argument("engine lock poisoned"))?;
        let _ = guard.take();
        Ok(())
    }

    #[napi]
    pub fn telemetry_snapshot(&self) -> Result<String> {
        self.with_engine(|engine| {
            let snap = engine.telemetry_snapshot();
            encode_json(serde_json::json!({
                "queries_total": snap.queries_total,
                "writes_total": snap.writes_total,
                "write_rows_total": snap.write_rows_total,
                "errors_total": snap.errors_total,
                "admin_ops_total": snap.admin_ops_total,
                "cache_hits": snap.sqlite_cache.cache_hits,
                "cache_misses": snap.sqlite_cache.cache_misses,
                "cache_writes": snap.sqlite_cache.cache_writes,
                "cache_spills": snap.sqlite_cache.cache_spills,
            }))
        })
    }

    // Exposed to JS as `engine.compileAst(...)`; napi-derive requires
    // `&self` to bind the method on the class instance even though
    // compilation is pure and does not touch engine state.
    #[allow(clippy::unused_self)]
    #[napi]
    pub fn compile_ast(&self, ast_json: String) -> Result<String> {
        let ast = parse_ast(&ast_json)?;
        let compiled = compile_query(&ast).map_err(map_compile_error)?;
        encode_json(FfiCompiledQuery::from(compiled))
    }

    // Exposed to JS as `engine.compileGroupedAst(...)`; see note on
    // `compile_ast` above for why `&self` is required.
    #[allow(clippy::unused_self)]
    #[napi]
    pub fn compile_grouped_ast(&self, ast_json: String) -> Result<String> {
        let ast = parse_ast(&ast_json)?;
        let compiled = compile_grouped_query(&ast).map_err(map_compile_error)?;
        encode_json(FfiCompiledGroupedQuery::from(compiled))
    }

    #[napi]
    pub fn explain_ast(&self, ast_json: String) -> Result<String> {
        let ast = parse_ast(&ast_json)?;
        let compiled = compile_query(&ast).map_err(map_compile_error)?;
        self.with_engine(|engine| {
            let plan = engine.coordinator().explain_compiled_read(&compiled);
            encode_json(FfiQueryPlan::from(plan))
        })
    }

    #[napi]
    pub fn execute_ast(&self, ast_json: String) -> Result<String> {
        let ast = parse_ast(&ast_json)?;
        let compiled = compile_query(&ast).map_err(map_compile_error)?;
        self.with_engine(|engine| {
            let rows = engine
                .coordinator()
                .execute_compiled_read(&compiled)
                .map_err(map_engine_error)?;
            encode_json(FfiQueryRows::from(rows))
        })
    }

    /// Execute an adaptive or fallback text search and return the serialized
    /// `PySearchRows` JSON. The `request_json` envelope is a
    /// [`crate::search_ffi::PySearchRequest`].
    #[napi]
    pub fn execute_search(&self, request_json: String) -> Result<String> {
        self.with_engine(|engine| {
            crate::search_ffi::execute_search_json(engine, &request_json)
                .map_err(map_search_ffi_error)
        })
    }

    #[napi]
    pub fn execute_grouped_ast(&self, ast_json: String) -> Result<String> {
        let ast = parse_ast(&ast_json)?;
        let compiled = compile_grouped_query(&ast).map_err(map_compile_error)?;
        self.with_engine(|engine| {
            let rows = engine
                .coordinator()
                .execute_compiled_grouped_read(&compiled)
                .map_err(map_engine_error)?;
            encode_json(FfiGroupedQueryRows::from(rows))
        })
    }

    #[napi]
    pub fn submit_write(&self, request_json: String) -> Result<String> {
        let request = parse_write_request(&request_json)?;
        self.with_engine(|engine| {
            let receipt = engine.writer().submit(request).map_err(map_engine_error)?;
            encode_json(FfiWriteReceipt::from(receipt))
        })
    }

    #[napi]
    pub fn touch_last_accessed(&self, request_json: String) -> Result<String> {
        let request = parse_last_access_touch_request(&request_json)?;
        self.with_engine(|engine| {
            let report = engine
                .touch_last_accessed(request)
                .map_err(map_engine_error)?;
            encode_json(FfiLastAccessTouchReport::from(report))
        })
    }

    #[napi]
    pub fn check_integrity(&self) -> Result<String> {
        self.with_engine(|engine| {
            let report = engine
                .admin()
                .service()
                .check_integrity()
                .map_err(map_engine_error)?;
            encode_json(FfiIntegrityReport::from(report))
        })
    }

    #[napi]
    pub fn check_semantics(&self) -> Result<String> {
        self.with_engine(|engine| {
            let report = engine
                .admin()
                .service()
                .check_semantics()
                .map_err(map_engine_error)?;
            encode_json(FfiSemanticReport::from(report))
        })
    }

    #[napi]
    pub fn rebuild_projections(&self, target: String) -> Result<String> {
        let target: ProjectionTarget = parse_projection_target(&target)?;
        self.with_engine(|engine| {
            let report = engine
                .admin()
                .service()
                .rebuild_projections(target)
                .map_err(map_engine_error)?;
            encode_json(FfiProjectionRepairReport::from(report))
        })
    }

    #[napi]
    pub fn rebuild_missing_projections(&self) -> Result<String> {
        self.with_engine(|engine| {
            let report = engine
                .admin()
                .service()
                .rebuild_missing_projections()
                .map_err(map_engine_error)?;
            encode_json(FfiProjectionRepairReport::from(report))
        })
    }

    #[napi]
    pub fn trace_source(&self, source_ref: String) -> Result<String> {
        self.with_engine(|engine| {
            let report = engine
                .admin()
                .service()
                .trace_source(&source_ref)
                .map_err(map_engine_error)?;
            encode_json(FfiTraceReport::from(report))
        })
    }

    #[napi]
    pub fn excise_source(&self, source_ref: String) -> Result<String> {
        self.with_engine(|engine| {
            let report = engine
                .admin()
                .service()
                .excise_source(&source_ref)
                .map_err(map_engine_error)?;
            encode_json(FfiTraceReport::from(report))
        })
    }

    #[napi]
    pub fn restore_logical_id(&self, logical_id: String) -> Result<String> {
        self.with_engine(|engine| {
            let report = engine
                .restore_logical_id(&logical_id)
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    #[napi]
    pub fn purge_logical_id(&self, logical_id: String) -> Result<String> {
        self.with_engine(|engine| {
            let report = engine
                .purge_logical_id(&logical_id)
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    #[napi]
    pub fn safe_export(&self, destination_path: String, force_checkpoint: bool) -> Result<String> {
        self.with_engine(|engine| {
            let manifest = engine
                .admin()
                .service()
                .safe_export(&destination_path, SafeExportOptions { force_checkpoint })
                .map_err(map_engine_error)?;
            encode_json(FfiSafeExportManifest::from(manifest))
        })
    }

    // ── FTS property schema methods ───────────────────────────────────

    #[napi]
    pub fn register_fts_property_schema(
        &self,
        kind: String,
        property_paths_json: String,
        separator: Option<String>,
    ) -> Result<String> {
        let paths: Vec<String> = serde_json::from_str(&property_paths_json)
            .map_err(|error| invalid_argument(format!("invalid property paths JSON: {error}")))?;
        self.with_engine(|engine| {
            let record = engine
                .register_fts_property_schema(&kind, &paths, separator.as_deref())
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    /// Register (or update) an FTS property projection schema with
    /// per-path modes (scalar vs recursive) and optional exclude paths.
    /// The `request_json` envelope matches
    /// `crate::admin_ffi::PyRegisterFtsPropertySchemaRequest`.
    #[napi]
    pub fn register_fts_property_schema_with_entries(
        &self,
        request_json: String,
    ) -> Result<String> {
        self.with_engine(|engine| {
            crate::admin_ffi::register_fts_property_schema_with_entries_json(engine, &request_json)
                .map_err(map_admin_ffi_error)
        })
    }

    #[napi]
    pub fn register_fts_property_schema_async(
        &self,
        kind: String,
        property_paths_json: String,
        separator: Option<String>,
    ) -> Result<String> {
        let paths: Vec<String> = serde_json::from_str(&property_paths_json)
            .map_err(|error| invalid_argument(format!("invalid property paths JSON: {error}")))?;
        self.with_engine(|engine| {
            let record = engine
                .register_fts_property_schema_async(&kind, &paths, separator.as_deref())
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    #[napi]
    pub fn get_property_fts_rebuild_progress(&self, kind: String) -> Result<String> {
        self.with_engine(|engine| {
            let progress = engine
                .get_property_fts_rebuild_progress(&kind)
                .map_err(map_engine_error)?;
            encode_json(progress)
        })
    }

    #[napi]
    pub fn describe_fts_property_schema(&self, kind: String) -> Result<String> {
        self.with_engine(|engine| {
            let record = engine
                .describe_fts_property_schema(&kind)
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    #[napi]
    pub fn list_fts_property_schemas(&self) -> Result<String> {
        self.with_engine(|engine| {
            let records = engine
                .list_fts_property_schemas()
                .map_err(map_engine_error)?;
            encode_json(records)
        })
    }

    #[napi]
    pub fn remove_fts_property_schema(&self, kind: String) -> Result<String> {
        self.with_engine(|engine| {
            engine
                .remove_fts_property_schema(&kind)
                .map_err(map_engine_error)?;
            encode_json(serde_json::json!({"removed": true}))
        })
    }

    // ── Operational collection methods ──────────────────────────────────

    #[napi]
    pub fn register_operational_collection(&self, request_json: String) -> Result<String> {
        check_json_size(
            &request_json,
            MAX_REQUEST_JSON_BYTES,
            "operational collection",
        )?;
        let request: OperationalRegisterRequest =
            serde_json::from_str(&request_json).map_err(|error| {
                invalid_argument(format!("invalid operational collection JSON: {error}"))
            })?;
        self.with_engine(|engine| {
            let record = engine
                .register_operational_collection(&request)
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    #[napi]
    pub fn describe_operational_collection(&self, name: String) -> Result<String> {
        self.with_engine(|engine| {
            let record = engine
                .describe_operational_collection(&name)
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    #[napi]
    pub fn update_operational_collection_filters(
        &self,
        name: String,
        filter_fields_json: String,
    ) -> Result<String> {
        check_json_size(&filter_fields_json, MAX_REQUEST_JSON_BYTES, "filter fields")?;
        self.with_engine(|engine| {
            let record = engine
                .update_operational_collection_filters(&name, &filter_fields_json)
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    #[napi]
    pub fn update_operational_collection_validation(
        &self,
        name: String,
        validation_json: String,
    ) -> Result<String> {
        check_json_size(&validation_json, MAX_REQUEST_JSON_BYTES, "validation")?;
        self.with_engine(|engine| {
            let record = engine
                .update_operational_collection_validation(&name, &validation_json)
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    #[napi]
    pub fn update_operational_collection_secondary_indexes(
        &self,
        name: String,
        secondary_indexes_json: String,
    ) -> Result<String> {
        check_json_size(
            &secondary_indexes_json,
            MAX_REQUEST_JSON_BYTES,
            "secondary indexes",
        )?;
        self.with_engine(|engine| {
            let record = engine
                .update_operational_collection_secondary_indexes(&name, &secondary_indexes_json)
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    #[napi]
    pub fn trace_operational_collection(
        &self,
        collection_name: String,
        record_key: Option<String>,
    ) -> Result<String> {
        self.with_engine(|engine| {
            let report = engine
                .trace_operational_collection(&collection_name, record_key.as_deref())
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    #[napi]
    pub fn read_operational_collection(&self, request_json: String) -> Result<String> {
        check_json_size(&request_json, MAX_REQUEST_JSON_BYTES, "operational read")?;
        let request: OperationalReadRequest = serde_json::from_str(&request_json)
            .map_err(|error| invalid_argument(format!("invalid operational read JSON: {error}")))?;
        self.with_engine(|engine| {
            let report = engine
                .read_operational_collection(&request)
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    #[napi]
    pub fn rebuild_operational_current(&self, collection_name: Option<String>) -> Result<String> {
        self.with_engine(|engine| {
            let report = engine
                .rebuild_operational_current(collection_name.as_deref())
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    #[napi]
    pub fn validate_operational_collection_history(
        &self,
        collection_name: String,
    ) -> Result<String> {
        self.with_engine(|engine| {
            let report = engine
                .validate_operational_collection_history(&collection_name)
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    #[napi]
    pub fn rebuild_operational_secondary_indexes(&self, collection_name: String) -> Result<String> {
        self.with_engine(|engine| {
            let report = engine
                .rebuild_operational_secondary_indexes(&collection_name)
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    #[napi]
    pub fn plan_operational_retention(
        &self,
        now_timestamp: i64,
        collection_names_json: Option<String>,
        max_collections: Option<u32>,
    ) -> Result<String> {
        let collection_names: Option<Vec<String>> = collection_names_json
            .map(|json| {
                serde_json::from_str(&json).map_err(|error| {
                    invalid_argument(format!("invalid collection_names JSON: {error}"))
                })
            })
            .transpose()?;
        self.with_engine(|engine| {
            let report = engine
                .plan_operational_retention(
                    now_timestamp,
                    collection_names.as_deref(),
                    max_collections.map(|v| v as usize),
                )
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    #[napi]
    pub fn run_operational_retention(
        &self,
        now_timestamp: i64,
        collection_names_json: Option<String>,
        max_collections: Option<u32>,
        dry_run: bool,
    ) -> Result<String> {
        let collection_names: Option<Vec<String>> = collection_names_json
            .map(|json| {
                serde_json::from_str(&json).map_err(|error| {
                    invalid_argument(format!("invalid collection_names JSON: {error}"))
                })
            })
            .transpose()?;
        self.with_engine(|engine| {
            let report = engine
                .run_operational_retention(
                    now_timestamp,
                    collection_names.as_deref(),
                    max_collections.map(|v| v as usize),
                    dry_run,
                )
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    #[napi]
    pub fn disable_operational_collection(&self, name: String) -> Result<String> {
        self.with_engine(|engine| {
            let record = engine
                .disable_operational_collection(&name)
                .map_err(map_engine_error)?;
            encode_json(record)
        })
    }

    #[napi]
    pub fn compact_operational_collection(&self, name: String, dry_run: bool) -> Result<String> {
        self.with_engine(|engine| {
            let report = engine
                .compact_operational_collection(&name, dry_run)
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    #[napi]
    pub fn purge_operational_collection(
        &self,
        name: String,
        before_timestamp: i64,
    ) -> Result<String> {
        self.with_engine(|engine| {
            let report = engine
                .purge_operational_collection(&name, before_timestamp)
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    // ── Provenance ──────────────────────────────────────────────────────

    #[napi]
    pub fn purge_provenance_events(
        &self,
        before_timestamp: i64,
        options_json: String,
    ) -> Result<String> {
        check_json_size(
            &options_json,
            MAX_REQUEST_JSON_BYTES,
            "provenance purge options",
        )?;
        let options: ProvenancePurgeOptions = serde_json::from_str(&options_json)
            .map_err(|error| invalid_argument(format!("invalid options JSON: {error}")))?;
        self.with_engine(|engine| {
            let report = engine
                .purge_provenance_events(before_timestamp, &options)
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }

    // ── Projection profile methods ──────────────────────────────────────

    #[napi]
    pub fn set_fts_profile(&self, request_json: String) -> Result<String> {
        self.with_engine(|engine| {
            crate::admin_ffi::set_fts_profile_json(engine, &request_json)
                .map_err(map_admin_ffi_error)
        })
    }

    #[napi]
    pub fn get_fts_profile(&self, kind: String) -> Result<String> {
        self.with_engine(|engine| {
            crate::admin_ffi::get_fts_profile_json(engine, &kind).map_err(map_admin_ffi_error)
        })
    }

    #[napi]
    pub fn set_vec_profile(&self, request_json: String) -> Result<String> {
        self.with_engine(|engine| {
            crate::admin_ffi::set_vec_profile_json(engine, &request_json)
                .map_err(map_admin_ffi_error)
        })
    }

    #[napi]
    pub fn get_vec_profile(&self, kind: String) -> Result<String> {
        self.with_engine(|engine| {
            crate::admin_ffi::get_vec_profile_json(engine, &kind).map_err(map_admin_ffi_error)
        })
    }

    #[napi]
    pub fn preview_projection_impact(&self, kind: String, facet: String) -> Result<String> {
        self.with_engine(|engine| {
            crate::admin_ffi::preview_projection_impact_json(engine, &kind, &facet)
                .map_err(map_admin_ffi_error)
        })
    }

    #[napi]
    pub fn drain_vector_projection(&self, request_json: String) -> Result<String> {
        self.with_engine(|engine| {
            crate::admin_ffi::drain_vector_projection_json(engine, &request_json)
                .map_err(map_admin_ffi_error)
        })
    }

    #[napi]
    #[allow(clippy::unused_self)]
    pub fn capabilities(&self) -> Result<String> {
        crate::admin_ffi::capabilities_json().map_err(map_admin_ffi_error)
    }

    #[napi]
    pub fn current_config(&self) -> Result<String> {
        self.with_engine(|engine| {
            crate::admin_ffi::current_config_json(engine).map_err(map_admin_ffi_error)
        })
    }

    #[napi]
    pub fn describe_kind(&self, kind: String) -> Result<String> {
        self.with_engine(|engine| {
            crate::admin_ffi::describe_kind_json(engine, &kind).map_err(map_admin_ffi_error)
        })
    }

    #[napi]
    pub fn configure_vec_kinds(&self, request_json: String) -> Result<String> {
        self.with_engine(|engine| {
            crate::admin_ffi::configure_vec_kinds_json(engine, &request_json)
                .map_err(map_admin_ffi_error)
        })
    }

    #[napi]
    pub fn configure_embedding(&self, request_json: String) -> Result<String> {
        self.with_engine(|engine| {
            crate::admin_ffi::configure_embedding_json(engine, &request_json)
                .map_err(map_admin_ffi_error)
        })
    }

    #[napi]
    pub fn configure_vec_kind(&self, request_json: String) -> Result<String> {
        self.with_engine(|engine| {
            crate::admin_ffi::configure_vec_kind_json(engine, &request_json)
                .map_err(map_admin_ffi_error)
        })
    }

    #[napi]
    pub fn restore_vector_profiles(&self) -> Result<String> {
        self.with_engine(|engine| {
            let report = engine
                .admin()
                .service()
                .restore_vector_profiles()
                .map_err(map_engine_error)?;
            encode_json(FfiProjectionRepairReport::from(report))
        })
    }

    #[napi]
    pub fn regenerate_vector_embeddings(&self, config_json: String) -> Result<String> {
        check_json_size(
            &config_json,
            MAX_REQUEST_JSON_BYTES,
            "vector regeneration config",
        )?;
        let config: VectorRegenerationConfig = serde_json::from_str(&config_json)
            .map_err(|error| invalid_argument(format!("invalid regen config: {error}")))?;
        self.with_engine(|engine| {
            let report = engine
                .regenerate_vector_embeddings(&config)
                .map_err(map_engine_error)?;
            encode_json(report)
        })
    }
}

fn parse_ast(ast_json: &str) -> Result<crate::QueryAst> {
    check_json_size(ast_json, MAX_AST_JSON_BYTES, "AST")?;
    let ast: FfiQueryAst = serde_json::from_str(ast_json)
        .map_err(|error| invalid_argument(format!("invalid query AST JSON: {error}")))?;
    Ok(ast.into())
}

fn parse_write_request(request_json: &str) -> Result<crate::WriteRequest> {
    check_json_size(request_json, MAX_WRITE_JSON_BYTES, "write request")?;
    let request: FfiWriteRequest = serde_json::from_str(request_json)
        .map_err(|error| invalid_argument(format!("invalid write request JSON: {error}")))?;
    Ok(request.into())
}

fn parse_last_access_touch_request(request_json: &str) -> Result<crate::LastAccessTouchRequest> {
    check_json_size(
        request_json,
        MAX_REQUEST_JSON_BYTES,
        "lastAccess touch request",
    )?;
    let request: FfiLastAccessTouchRequest =
        serde_json::from_str(request_json).map_err(|error| {
            invalid_argument(format!("invalid lastAccess touch request JSON: {error}"))
        })?;
    Ok(request.into())
}

#[allow(dead_code)]
#[napi(js_name = "newId")]
pub fn js_new_id() -> String {
    new_id()
}

#[allow(dead_code)]
#[napi(js_name = "newRowId")]
pub fn js_new_row_id() -> String {
    new_row_id()
}

#[allow(dead_code)]
#[napi(js_name = "version")]
pub fn js_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Return the well-known tokenizer presets mapped to their FTS5 tokenizer
/// strings. This is the single source of truth for the TypeScript SDK —
/// `admin.TOKENIZER_PRESETS` is computed from this function at module load
/// time.
#[allow(dead_code)]
#[napi(js_name = "listTokenizerPresets")]
pub fn js_list_tokenizer_presets() -> HashMap<String, String> {
    fathomdb_engine::TOKENIZER_PRESETS
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use tempfile::NamedTempFile;

    use super::{NodeEngineCore, js_list_tokenizer_presets};

    /// ARCH-006: Rust is the single source of truth for tokenizer presets.
    /// The FFI helper must surface exactly what `TOKENIZER_PRESETS` holds.
    #[test]
    fn list_tokenizer_presets_matches_engine_constant() {
        let presets = js_list_tokenizer_presets();
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

    #[test]
    fn open_constructs_engine_options_with_all_fields() {
        let db = NamedTempFile::new().expect("temp db");
        let engine = NodeEngineCore::open(
            db.path().to_str().expect("db path").to_owned(),
            "warn".to_owned(),
            None,
            None,
            None,
            None,
        );
        assert!(engine.is_ok(), "open must succeed: {:?}", engine.err());
    }

    #[test]
    fn close_is_idempotent() {
        let db = NamedTempFile::new().expect("temp db");
        let engine = NodeEngineCore::open(
            db.path().to_str().expect("db path").to_owned(),
            "warn".to_owned(),
            None,
            None,
            None,
            None,
        )
        .expect("open");
        engine.close().expect("first close");
        engine.close().expect("second close");
    }

    #[test]
    fn close_makes_subsequent_calls_fail() {
        let db = NamedTempFile::new().expect("temp db");
        let engine = NodeEngineCore::open(
            db.path().to_str().expect("db path").to_owned(),
            "warn".to_owned(),
            None,
            None,
            None,
            None,
        )
        .expect("open");
        engine.close().expect("close");
        let result = engine.check_integrity();
        assert!(result.is_err(), "call after close must fail");
    }

    #[test]
    fn get_fts_profile_returns_null_when_unset() {
        let db = NamedTempFile::new().expect("temp db");
        let engine = NodeEngineCore::open(
            db.path().to_str().expect("db path").to_owned(),
            "warn".to_owned(),
            None,
            None,
            None,
            None,
        )
        .expect("open");
        let result = engine
            .get_fts_profile("Article".to_owned())
            .expect("get_fts_profile");
        assert_eq!(result, "null", "unset FTS profile must serialize as null");
    }

    #[test]
    fn set_and_get_fts_profile_round_trip() {
        let db = NamedTempFile::new().expect("temp db");
        let engine = NodeEngineCore::open(
            db.path().to_str().expect("db path").to_owned(),
            "warn".to_owned(),
            None,
            None,
            None,
            None,
        )
        .expect("open");
        let set_result = engine
            .set_fts_profile(r#"{"kind":"Article","tokenizer":"unicode61"}"#.to_owned())
            .expect("set_fts_profile");
        let parsed: serde_json::Value =
            serde_json::from_str(&set_result).expect("set result is valid JSON");
        assert_eq!(parsed["kind"], "Article");
        assert_eq!(parsed["tokenizer"], "unicode61");

        let get_result = engine
            .get_fts_profile("Article".to_owned())
            .expect("get_fts_profile");
        let parsed_get: serde_json::Value =
            serde_json::from_str(&get_result).expect("get result is valid JSON");
        assert_eq!(parsed_get["kind"], "Article");
        assert_eq!(parsed_get["tokenizer"], "unicode61");
    }

    #[test]
    fn get_vec_profile_returns_null_when_unset() {
        let db = NamedTempFile::new().expect("temp db");
        let engine = NodeEngineCore::open(
            db.path().to_str().expect("db path").to_owned(),
            "warn".to_owned(),
            None,
            None,
            None,
            None,
        )
        .expect("open");
        let result = engine
            .get_vec_profile("Document".to_owned())
            .expect("get_vec_profile");
        assert_eq!(result, "null", "unset vec profile must serialize as null");
    }

    #[test]
    fn preview_projection_impact_returns_valid_json() {
        let db = NamedTempFile::new().expect("temp db");
        let engine = NodeEngineCore::open(
            db.path().to_str().expect("db path").to_owned(),
            "warn".to_owned(),
            None,
            None,
            None,
            None,
        )
        .expect("open");
        let result = engine
            .preview_projection_impact("Article".to_owned(), "fts".to_owned())
            .expect("preview_projection_impact");
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("impact result is valid JSON");
        assert!(
            parsed.get("rows_to_rebuild").is_some(),
            "must have rows_to_rebuild field"
        );
    }

    #[test]
    fn restore_vector_profiles_returns_repair_report_json() {
        let db = NamedTempFile::new().expect("temp db");
        let engine = NodeEngineCore::open(
            db.path().to_str().expect("db path").to_owned(),
            "warn".to_owned(),
            None,
            None,
            None,
            None,
        )
        .expect("open");
        let result = engine
            .restore_vector_profiles()
            .expect("restore_vector_profiles");
        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("repair report is valid JSON");
        assert!(
            parsed.get("rebuilt_rows").is_some(),
            "must have rebuilt_rows field"
        );
    }
}
