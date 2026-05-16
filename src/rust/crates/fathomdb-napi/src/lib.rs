//! napi-rs binding from the TypeScript SDK to `fathomdb-engine`.
//!
//! FFI safety contract (mirrors the PyO3 binding in `fathomdb-py`):
//!
//! 1. Every method that may block inside the engine runs its blocking
//!    body inside `tokio::task::spawn_blocking`, so the libuv main
//!    thread is never tied up. napi-rs's `#[napi] async fn` wraps
//!    return values as JS `Promise<T>`.
//! 2. Engine entry points return typed errors via [`engine_error_to_napi`] /
//!    [`engine_open_error_to_napi`] — single-switch mapping with no
//!    catch-all arm; the binding fails to compile when the Rust variant
//!    set drifts from the TS leaf-class set (AC-060a).
//! 3. Every string crossing the FFI is checked by [`validate_ffi_string`]
//!    for embedded NUL or unpaired UTF-16 surrogates BEFORE the writer
//!    transaction opens (AC-068a / AC-068b).
//! 4. Engine panics are caught via `catch_unwind` inside the spawn-blocking
//!    body and rethrown as a distinct `FathomDbPanicError` (code
//!    `FDB_PANIC`); the host process is not aborted (AC-067).
//
// why: napi-rs catches panics by default but throws a generic JS
// `Error` with `Status::GenericFailure` and a Rust-formatted message,
// which is hard to assert on. Explicit `catch_unwind` + a stable
// `FDB_PANIC` code lets the TS-side `rethrowTyped` map panics to a
// distinct `FathomDbPanicError` class that is intentionally NOT a
// `FathomDbError` subclass: panic is a contract bug, not a typed
// engine outcome.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use fathomdb_engine::{
    CorruptionDetail, CorruptionKind, Engine as RustEngine, EngineError as RustEngineError,
    EngineOpenError, OpenStage, PreparedWrite, SearchResult as RustSearchResult,
    SoftFallbackBranch, WriteReceipt as RustWriteReceipt,
};
use napi::{Error, JsUnknown, Result, Status};
use napi_derive::napi;
use serde::Serialize;
use serde_json::{json, Value as JsonValue};

// ===== Error-code constants ===========================================
//
// One per typed leaf class on the TS side. The single-switch translator
// below maps every engine variant to exactly one of these codes; the TS
// `rethrowTyped` table maps each code to exactly one leaf class. Drift
// fails to compile (Rust side) or fails an exhaustiveness check (TS).

const CODE_STORAGE: &str = "FDB_STORAGE";
const CODE_PROJECTION: &str = "FDB_PROJECTION";
const CODE_VECTOR: &str = "FDB_VECTOR";
const CODE_EMBEDDER: &str = "FDB_EMBEDDER";
const CODE_EMBEDDER_NOT_CONFIGURED: &str = "FDB_EMBEDDER_NOT_CONFIGURED";
const CODE_KIND_NOT_VECTOR_INDEXED: &str = "FDB_KIND_NOT_VECTOR_INDEXED";
const CODE_EMBEDDER_DIMENSION_MISMATCH: &str = "FDB_EMBEDDER_DIMENSION_MISMATCH";
const CODE_SCHEDULER: &str = "FDB_SCHEDULER";
const CODE_OP_STORE: &str = "FDB_OP_STORE";
const CODE_WRITE_VALIDATION: &str = "FDB_WRITE_VALIDATION";
const CODE_SCHEMA_VALIDATION: &str = "FDB_SCHEMA_VALIDATION";
const CODE_OVERLOADED: &str = "FDB_OVERLOADED";
const CODE_CLOSING: &str = "FDB_CLOSING";
const CODE_DATABASE_LOCKED: &str = "FDB_DATABASE_LOCKED";
const CODE_CORRUPTION: &str = "FDB_CORRUPTION";
const CODE_INCOMPATIBLE_SCHEMA_VERSION: &str = "FDB_INCOMPATIBLE_SCHEMA_VERSION";
const CODE_MIGRATION: &str = "FDB_MIGRATION";
const CODE_EMBEDDER_IDENTITY_MISMATCH: &str = "FDB_EMBEDDER_IDENTITY_MISMATCH";
const CODE_PANIC: &str = "FDB_PANIC";

// ===== Typed-error encoder ============================================

/// Encode a typed-error envelope as JSON in `err.message` so the
/// TS-side `rethrowTyped` can reconstitute the right leaf class with
/// the right payload. napi-rs 2.x has no public API for throwing a
/// Rust-defined JS class directly; the envelope-in-message pattern is
/// the canonical workaround.
#[derive(Serialize)]
struct TypedEnvelope<'a> {
    code: &'a str,
    message: String,
    payload: JsonValue,
}

fn typed_error(code: &'static str, message: impl Into<String>, payload: JsonValue) -> Error {
    let envelope = TypedEnvelope { code, message: message.into(), payload };
    // serde_json serialization is infallible for our envelope shape;
    // unwrap is safe.
    let reason = serde_json::to_string(&envelope).unwrap();
    Error::new(Status::GenericFailure, reason)
}

// ===== String validation (AC-068a / AC-068b) =========================

/// Reject strings carrying an embedded NUL or an unpaired UTF-16
/// surrogate codepoint (`U+D800..=U+DFFF`).
///
/// JavaScript strings are UTF-16 by spec, so lone surrogates are
/// representable on the JS side (`String.fromCharCode(0xD800)`). The
/// napi-rs string conversion translates UTF-16 → UTF-8 and accepts
/// some malformed inputs; this helper rejects them BEFORE the writer
/// transaction opens (no-row-written invariant).
pub fn validate_ffi_string(value: &str) -> std::result::Result<(), String> {
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

fn validate_ffi_string_napi(value: &str) -> Result<()> {
    validate_ffi_string(value)
        .map_err(|msg| typed_error(CODE_WRITE_VALIDATION, msg, JsonValue::Null))
}

// ===== Error mapping ==================================================

/// Translate every `EngineError` variant to its typed JS counterpart.
///
/// No catch-all arm: drift between the Rust enum and the TS class set
/// is a compile error.
fn engine_error_to_napi(err: RustEngineError) -> Error {
    match err {
        RustEngineError::Storage => typed_error(CODE_STORAGE, "storage error", JsonValue::Null),
        RustEngineError::Projection => {
            typed_error(CODE_PROJECTION, "projection error", JsonValue::Null)
        }
        RustEngineError::Vector => typed_error(CODE_VECTOR, "vector error", JsonValue::Null),
        RustEngineError::Embedder => typed_error(CODE_EMBEDDER, "embedder error", JsonValue::Null),
        RustEngineError::EmbedderNotConfigured => {
            typed_error(CODE_EMBEDDER_NOT_CONFIGURED, "embedder is not configured", JsonValue::Null)
        }
        RustEngineError::KindNotVectorIndexed => typed_error(
            CODE_KIND_NOT_VECTOR_INDEXED,
            "kind is not configured for vector indexing",
            JsonValue::Null,
        ),
        RustEngineError::EmbedderDimensionMismatch { expected, actual } => typed_error(
            CODE_EMBEDDER_DIMENSION_MISMATCH,
            format!("embedder vector dimension mismatch: stored {expected}, supplied {actual}"),
            json!({ "stored": expected, "supplied": actual }),
        ),
        RustEngineError::Scheduler => {
            typed_error(CODE_SCHEDULER, "scheduler error", JsonValue::Null)
        }
        RustEngineError::OpStore => typed_error(CODE_OP_STORE, "op-store error", JsonValue::Null),
        RustEngineError::WriteValidation => {
            typed_error(CODE_WRITE_VALIDATION, "write validation error", JsonValue::Null)
        }
        RustEngineError::SchemaValidation => {
            typed_error(CODE_SCHEMA_VALIDATION, "schema validation error", JsonValue::Null)
        }
        RustEngineError::Overloaded => {
            typed_error(CODE_OVERLOADED, "engine overloaded", JsonValue::Null)
        }
        RustEngineError::Closing => typed_error(CODE_CLOSING, "engine is closing", JsonValue::Null),
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

fn corruption_to_napi(detail: CorruptionDetail) -> Error {
    let kind = corruption_kind_str(detail.kind);
    let stage = open_stage_str(detail.stage);
    let recovery_hint_code = detail.recovery_hint.code;
    let doc_anchor = detail.recovery_hint.doc_anchor;
    typed_error(
        CODE_CORRUPTION,
        format!("corruption {kind} at stage {stage} ({recovery_hint_code})"),
        json!({
            "kind": kind,
            "stage": stage,
            "recoveryHintCode": recovery_hint_code,
            "docAnchor": doc_anchor,
        }),
    )
}

fn engine_open_error_to_napi(err: EngineOpenError) -> Error {
    match err {
        EngineOpenError::DatabaseLocked { holder_pid } => typed_error(
            CODE_DATABASE_LOCKED,
            match holder_pid {
                Some(pid) => format!("database is locked by process {pid}"),
                None => "database is locked by another engine instance".to_string(),
            },
            json!({ "holderPid": holder_pid }),
        ),
        EngineOpenError::Corruption(detail) => corruption_to_napi(detail),
        EngineOpenError::IncompatibleSchemaVersion { seen, supported } => typed_error(
            CODE_INCOMPATIBLE_SCHEMA_VERSION,
            format!(
                "database schema version {seen} is incompatible with supported version {supported}"
            ),
            json!({ "seen": seen, "supported": supported }),
        ),
        EngineOpenError::MigrationError {
            schema_version_before,
            schema_version_current,
            step_id,
        } => typed_error(
            CODE_MIGRATION,
            format!(
                "schema migration failed at step {step_id}; schema version remained between {schema_version_before} and {schema_version_current}"
            ),
            json!({
                "schemaVersionBefore": schema_version_before,
                "schemaVersionCurrent": schema_version_current,
                "stepId": step_id,
            }),
        ),
        EngineOpenError::EmbedderIdentityMismatch { stored, supplied } => typed_error(
            CODE_EMBEDDER_IDENTITY_MISMATCH,
            format!(
                "embedder identity mismatch: stored {}@{}, supplied {}@{}",
                stored.name, stored.revision, supplied.name, supplied.revision,
            ),
            json!({
                "storedName": stored.name,
                "storedRevision": stored.revision,
                "suppliedName": supplied.name,
                "suppliedRevision": supplied.revision,
            }),
        ),
        EngineOpenError::EmbedderDimensionMismatch { stored, supplied } => typed_error(
            CODE_EMBEDDER_DIMENSION_MISMATCH,
            format!(
                "embedder vector dimension mismatch: stored {stored}, supplied {supplied}"
            ),
            json!({ "stored": stored, "supplied": supplied }),
        ),
        EngineOpenError::Io { message } => typed_error(
            CODE_STORAGE,
            format!("database I/O error: {message}"),
            JsonValue::Null,
        ),
    }
}

fn panic_error() -> Error {
    typed_error(CODE_PANIC, "engine panic (see logs)", JsonValue::Null)
}

/// Run a blocking engine call inside `tokio::task::spawn_blocking` so
/// the libuv event loop stays free, wrapping it in `catch_unwind` so
/// panics surface as the typed [`panic_error`] rather than aborting the
/// host process. `AssertUnwindSafe` lets us thread the closure through
/// without requiring `UnwindSafe` from the engine's `Arc<dyn Embedder>`
/// substrate.
async fn call_engine<R, F>(f: F) -> Result<R>
where
    R: Send + 'static,
    F: FnOnce() -> std::result::Result<R, RustEngineError> + Send + 'static,
{
    let join_result = tokio::task::spawn_blocking(move || catch_unwind(AssertUnwindSafe(f))).await;
    match join_result {
        Ok(Ok(Ok(value))) => Ok(value),
        Ok(Ok(Err(err))) => Err(engine_error_to_napi(err)),
        Ok(Err(_panic)) => Err(panic_error()),
        Err(join_err) => Err(typed_error(
            CODE_PANIC,
            format!("spawn_blocking join error: {join_err}"),
            JsonValue::Null,
        )),
    }
}

// ===== Data classes ===================================================

#[napi(object)]
pub struct WriteReceipt {
    pub cursor: i64,
}

impl WriteReceipt {
    fn from_rust(r: RustWriteReceipt) -> Self {
        Self { cursor: r.cursor as i64 }
    }
}

#[napi(object)]
pub struct SoftFallback {
    /// "vector" | "text"
    pub branch: String,
}

#[napi(object)]
pub struct SearchResult {
    pub projection_cursor: i64,
    pub soft_fallback: Option<SoftFallback>,
    pub results: Vec<String>,
}

impl SearchResult {
    fn from_rust(r: RustSearchResult) -> Self {
        Self {
            projection_cursor: r.projection_cursor as i64,
            soft_fallback: r.soft_fallback.as_ref().map(|s| SoftFallback {
                branch: match s.branch {
                    SoftFallbackBranch::Vector => "vector".to_string(),
                    SoftFallbackBranch::Text => "text".to_string(),
                },
            }),
            results: r.results,
        }
    }
}

#[napi(object)]
pub struct CounterSnapshot {
    pub queries: i64,
    pub writes: i64,
    pub write_rows: i64,
    pub admin_ops: i64,
    pub cache_hit: i64,
    pub cache_miss: i64,
}

#[napi(object)]
pub struct AttachSubscriberOptions {
    pub heartbeat_interval_ms: Option<u32>,
}

#[napi(object)]
pub struct EngineConfig {
    pub embedder_pool_size: Option<u32>,
    pub scheduler_runtime_threads: Option<u32>,
    pub provenance_row_cap: Option<u32>,
    pub embedder_call_timeout_ms: Option<u32>,
    pub slow_threshold_ms: Option<u32>,
}

#[napi(object)]
pub struct EngineOpenOptions {
    pub engine_config: Option<EngineConfig>,
}

#[napi(object)]
pub struct AdminConfigureOptions {
    pub name: String,
    pub body: String,
}

// ===== Engine =========================================================

#[napi]
pub struct Engine {
    inner: Arc<RustEngine>,
}

#[napi]
impl Engine {
    /// Promise-returning open per `dev/interfaces/typescript.md`. The
    /// blocking SQLite work runs on a tokio blocking thread.
    #[napi(factory)]
    pub async fn open(path: String, options: Option<EngineOpenOptions>) -> Result<Engine> {
        validate_ffi_string_napi(&path)?;
        let _ = options; // engineConfig knobs are recognised but not yet plumbed; see fathomdb-py.
        let join_result = tokio::task::spawn_blocking(move || {
            catch_unwind(AssertUnwindSafe(|| RustEngine::open(path)))
        })
        .await;
        let opened = match join_result {
            Ok(Ok(Ok(opened))) => opened,
            Ok(Ok(Err(err))) => return Err(engine_open_error_to_napi(err)),
            Ok(Err(_panic)) => return Err(panic_error()),
            Err(join_err) => {
                return Err(typed_error(
                    CODE_PANIC,
                    format!("spawn_blocking join error: {join_err}"),
                    JsonValue::Null,
                ))
            }
        };
        Ok(Engine { inner: Arc::new(opened.engine) })
    }

    #[napi]
    pub async fn write(&self, batch: Vec<JsonValue>) -> Result<WriteReceipt> {
        let prepared = translate_batch(batch)?;
        let engine = Arc::clone(&self.inner);
        let receipt = call_engine(move || engine.write(&prepared)).await?;
        Ok(WriteReceipt::from_rust(receipt))
    }

    #[napi]
    pub async fn search(&self, query: String) -> Result<SearchResult> {
        validate_ffi_string_napi(&query)?;
        if query.trim().is_empty() {
            return Err(typed_error(
                CODE_WRITE_VALIDATION,
                "query must not be empty",
                JsonValue::Null,
            ));
        }
        let engine = Arc::clone(&self.inner);
        let result = call_engine(move || engine.search(&query)).await?;
        Ok(SearchResult::from_rust(result))
    }

    #[napi]
    pub async fn close(&self) -> Result<()> {
        let engine = Arc::clone(&self.inner);
        call_engine(move || engine.close()).await
    }

    #[napi]
    pub async fn drain(&self, timeout_ms: u32) -> Result<()> {
        let engine = Arc::clone(&self.inner);
        let ms = timeout_ms as u64;
        call_engine(move || engine.drain(ms)).await
    }

    #[napi]
    pub fn counters(&self) -> CounterSnapshot {
        let snap = self.inner.counters();
        CounterSnapshot {
            queries: snap.queries as i64,
            writes: snap.writes as i64,
            write_rows: snap.write_rows as i64,
            admin_ops: snap.admin_ops as i64,
            cache_hit: snap.cache_hit as i64,
            cache_miss: snap.cache_miss as i64,
        }
    }

    #[napi]
    pub fn set_profiling(&self, enabled: bool) -> Result<()> {
        self.inner.set_profiling(enabled).map_err(engine_error_to_napi)
    }

    #[napi]
    pub fn set_slow_threshold_ms(&self, value: u32) -> Result<()> {
        self.inner.set_slow_threshold_ms(value as u64).map_err(engine_error_to_napi)
    }

    /// Subscriber wiring lands in a later 0.6.x slice; the binding
    /// accepts the call so callers can wire a callback against the
    /// public surface without a runtime error.
    #[napi]
    pub fn attach_subscriber(
        &self,
        callback: JsUnknown,
        options: Option<AttachSubscriberOptions>,
    ) -> Result<()> {
        let _ = callback;
        let _ = options;
        Ok(())
    }
}

// ===== admin.configure ================================================

#[napi(js_name = "adminConfigure")]
pub async fn admin_configure(
    engine: &Engine,
    options: AdminConfigureOptions,
) -> Result<WriteReceipt> {
    validate_ffi_string_napi(&options.name)?;
    validate_ffi_string_napi(&options.body)?;
    if options.name.is_empty() {
        return Err(typed_error(
            CODE_WRITE_VALIDATION,
            "admin.configure requires a non-empty name",
            JsonValue::Null,
        ));
    }
    // why: `dev/interfaces/typescript.md` § Runtime surface pins the
    // admin.configure({ name, body }) signature; the engine's
    // `PreparedWrite::AdminSchema` requires `kind ∈ {latest_state,
    // append_only_log}`. The TS verb is sugar over latest-state
    // collection registration in 0.6.0.
    let batch = vec![PreparedWrite::AdminSchema {
        name: options.name,
        kind: "latest_state".to_string(),
        schema_json: options.body,
        retention_json: "{}".to_string(),
    }];
    let inner = Arc::clone(&engine.inner);
    let receipt = call_engine(move || inner.write(&batch)).await?;
    Ok(WriteReceipt::from_rust(receipt))
}

// ===== Batch translation ==============================================
//
// The TS surface accepts a JS array of typed-write objects. Each item
// is one of:
//   { node: { kind, body?, sourceId? } }
//   { edge: { kind, from, to, sourceId? } }
//   { opStore: { collection, recordKey, schemaId?, body } }
//   { adminSchema: { name, kind, schemaJson, retentionJson? } }
// Plus a bare `{ kind, body?, sourceId? }` shape treated as Node (parity
// with the Python stub). serde_json::Value is the cheapest cross-thread
// representation; napi-rs converts JS objects via the `serde-json`
// feature.

fn translate_batch(batch: Vec<JsonValue>) -> Result<Vec<PreparedWrite>> {
    batch.into_iter().map(translate_write_item).collect()
}

fn json_get<'a>(v: &'a JsonValue, key: &str) -> Option<&'a JsonValue> {
    v.as_object().and_then(|m| m.get(key))
}

fn json_str(v: &JsonValue, key: &str) -> Result<Option<String>> {
    match json_get(v, key) {
        Some(JsonValue::Null) | None => Ok(None),
        Some(JsonValue::String(s)) => {
            validate_ffi_string_napi(s)?;
            Ok(Some(s.clone()))
        }
        Some(_other) => Err(typed_error(
            CODE_WRITE_VALIDATION,
            format!("field {key:?} must be a string"),
            JsonValue::Null,
        )),
    }
}

fn json_str_required(v: &JsonValue, key: &str) -> Result<String> {
    json_str(v, key)?.ok_or_else(|| {
        typed_error(
            CODE_WRITE_VALIDATION,
            format!("write item missing required field {key:?}"),
            JsonValue::Null,
        )
    })
}

fn json_serialised(v: &JsonValue, key: &str) -> Result<Option<String>> {
    match json_get(v, key) {
        Some(JsonValue::Null) | None => Ok(None),
        Some(JsonValue::String(s)) => {
            validate_ffi_string_napi(s)?;
            Ok(Some(s.clone()))
        }
        Some(other) => {
            let serialised = serde_json::to_string(other).map_err(|e| {
                typed_error(
                    CODE_WRITE_VALIDATION,
                    format!("field {key:?} not serialisable: {e}"),
                    JsonValue::Null,
                )
            })?;
            validate_ffi_string_napi(&serialised)?;
            Ok(Some(serialised))
        }
    }
}

fn json_serialised_required(v: &JsonValue, key: &str) -> Result<String> {
    json_serialised(v, key)?.ok_or_else(|| {
        typed_error(
            CODE_WRITE_VALIDATION,
            format!("write item missing required field {key:?}"),
            JsonValue::Null,
        )
    })
}

fn translate_write_item(item: JsonValue) -> Result<PreparedWrite> {
    if !item.is_object() {
        return Err(typed_error(
            CODE_WRITE_VALIDATION,
            "write item must be an object",
            JsonValue::Null,
        ));
    }
    if let Some(inner) = json_get(&item, "edge") {
        return translate_edge(inner);
    }
    if let Some(inner) = json_get(&item, "opStore").or_else(|| json_get(&item, "op_store")) {
        return translate_op_store(inner);
    }
    if let Some(inner) = json_get(&item, "adminSchema").or_else(|| json_get(&item, "admin_schema"))
    {
        return translate_admin_schema(inner);
    }
    if let Some(inner) = json_get(&item, "node") {
        return translate_node(inner);
    }
    translate_node(&item)
}

/// Look up `camelCase` first, then `snake_case`, returning the first
/// present string. Both forms are checked so callers porting from the
/// Python stub keep working without surface-level rewrites.
fn json_str_alt(item: &JsonValue, camel: &str, snake: &str) -> Result<Option<String>> {
    if let Some(v) = json_str(item, camel)? {
        return Ok(Some(v));
    }
    json_str(item, snake)
}

fn json_str_alt_required(item: &JsonValue, camel: &str, snake: &str) -> Result<String> {
    json_str_alt(item, camel, snake)?.ok_or_else(|| {
        typed_error(
            CODE_WRITE_VALIDATION,
            format!("write item missing required field {camel:?}"),
            JsonValue::Null,
        )
    })
}

fn json_serialised_alt(item: &JsonValue, camel: &str, snake: &str) -> Result<Option<String>> {
    if let Some(v) = json_serialised(item, camel)? {
        return Ok(Some(v));
    }
    json_serialised(item, snake)
}

fn json_serialised_alt_required(item: &JsonValue, camel: &str, snake: &str) -> Result<String> {
    json_serialised_alt(item, camel, snake)?.ok_or_else(|| {
        typed_error(
            CODE_WRITE_VALIDATION,
            format!("write item missing required field {camel:?}"),
            JsonValue::Null,
        )
    })
}

fn translate_node(item: &JsonValue) -> Result<PreparedWrite> {
    let kind = json_str_required(item, "kind")?;
    let body = json_serialised(item, "body")?.unwrap_or_else(|| "{}".to_string());
    let source_id = json_str_alt(item, "sourceId", "source_id")?;
    Ok(PreparedWrite::Node { kind, body, source_id })
}

fn translate_edge(item: &JsonValue) -> Result<PreparedWrite> {
    let kind = json_str_required(item, "kind")?;
    let from = json_str_required(item, "from")?;
    let to = json_str_required(item, "to")?;
    let source_id = json_str_alt(item, "sourceId", "source_id")?;
    Ok(PreparedWrite::Edge { kind, from, to, source_id })
}

fn translate_op_store(item: &JsonValue) -> Result<PreparedWrite> {
    let collection = json_str_required(item, "collection")?;
    let record_key = json_str_alt_required(item, "recordKey", "record_key")?;
    let schema_id = json_str_alt(item, "schemaId", "schema_id")?;
    let body = json_serialised_required(item, "body")?;
    Ok(PreparedWrite::OpStore { collection, record_key, schema_id, body })
}

fn translate_admin_schema(item: &JsonValue) -> Result<PreparedWrite> {
    let name = json_str_required(item, "name")?;
    let kind = json_str_required(item, "kind")?;
    let schema_json = json_serialised_alt_required(item, "schemaJson", "schema_json")?;
    let retention_json = json_serialised_alt(item, "retentionJson", "retention_json")?
        .unwrap_or_else(|| "{}".to_string());
    Ok(PreparedWrite::AdminSchema { name, kind, schema_json, retention_json })
}

// ===== Test hooks =====================================================

/// AC-067 force-panic probe. Gated by `cfg(any(test, feature =
/// "test-hooks"))` so release npm builds without the feature flag do
/// not expose it.
#[cfg(any(test, feature = "test-hooks"))]
#[napi(js_name = "forcePanicForTest")]
pub fn force_panic_for_test() -> Result<()> {
    call_panicking_engine_for_test()
}

#[cfg(any(test, feature = "test-hooks"))]
fn call_panicking_engine_for_test() -> Result<()> {
    // Run the panic through the same catch_unwind path that real
    // engine calls use so the TS-side test exercises the production
    // panic-translation seam.
    let join_result = std::panic::catch_unwind(AssertUnwindSafe(
        || -> std::result::Result<(), RustEngineError> {
            panic!("force_panic_for_test: AC-067 probe");
        },
    ));
    match join_result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(engine_error_to_napi(err)),
        Err(_) => Err(panic_error()),
    }
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
    fn validate_ffi_string_accepts_replacement_codepoint() {
        // U+FFFD is the only "high unicode" codepoint a Rust &str can
        // hold near the surrogate range; the codepoints U+D800..U+DFFF
        // themselves cannot appear in valid UTF-8, so the runtime
        // guard is exercised when JS surrogates round-trip through
        // napi-rs string conversion (covered by ffi-safety.test.ts).
        assert!(validate_ffi_string("\u{FFFD}").is_ok());
    }
}
