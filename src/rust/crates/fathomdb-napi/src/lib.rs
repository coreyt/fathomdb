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

use fathomdb_embedder::EmbedderEvent as RustEmbedderEvent;
use fathomdb_embedder_api::EmbedderIdentity as RustEmbedderIdentity;
use fathomdb_engine::{
    ComparisonOp as RustComparisonOp, CorruptionDetail, CorruptionKind, EmbedderChoice,
    Engine as RustEngine, EngineError as RustEngineError, EngineOpenError,
    ExtractDocument as RustExtractDocument,
    IngestWithExtractorReceipt as RustIngestWithExtractorReceipt, NodeRecord as RustNodeRecord,
    OpStoreRow as RustOpStoreRow, OpenReport as RustOpenReport, OpenStage,
    Predicate as RustPredicate, PreparedWrite, ScalarValue as RustScalarValue,
    SearchExpandResult as RustSearchExpandResult, SearchFilter as RustSearchFilter,
    SearchHit as RustSearchHit, SearchResult as RustSearchResult, SoftFallbackBranch,
    TraversalDirection as RustTraversalDirection, WriteReceipt as RustWriteReceipt,
};
use fathomdb_schema::MigrationStepReport as RustMigrationStepReport;
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
// G11 (Slice 15) — BYO-LLM extraction harness protocol error.
const CODE_EXTRACTOR: &str = "FDB_EXTRACTOR";
// G4 (Slice 35) — filter predicate construction error (non-allowlisted path).
const CODE_INVALID_FILTER: &str = "FDB_INVALID_FILTER";
// Slice 20 — depth > 3 or invalid argument (G5/G6).
const CODE_INVALID_ARGUMENT: &str = "FDB_INVALID_ARGUMENT";
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
        RustEngineError::Extractor => {
            typed_error(CODE_EXTRACTOR, "extractor error", JsonValue::Null)
        }
        RustEngineError::InvalidFilter { reason } => {
            typed_error(CODE_INVALID_FILTER, format!("invalid filter: {reason}"), JsonValue::Null)
        }
        RustEngineError::InvalidArgument { msg } => {
            typed_error(CODE_INVALID_ARGUMENT, msg, JsonValue::Null)
        }
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
        EngineOpenError::Embedder(err) => typed_error(
            CODE_EMBEDDER,
            format!("embedder error during open: {err:?}"),
            JsonValue::Null,
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

/// Sync sibling of [`call_engine`]: wrap a non-blocking accessor in
/// `catch_unwind` so a panic on the JS thread surfaces as [`panic_error`]
/// instead of unwinding into napi-rs's default `GenericFailure` path.
fn call_engine_sync<R, F>(f: F) -> Result<R>
where
    F: FnOnce() -> Result<R>,
{
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(_panic) => Err(panic_error()),
    }
}

// ===== Data classes ===================================================

#[napi(object)]
pub struct WriteReceipt {
    pub cursor: i64,
    /// G0 (Slice 15) — per-row `write_cursor`s, 1:1 with the input batch order
    /// (surfaced as `rowCursors`). Each `u64` is narrowed to `i64` at the FFI
    /// boundary, matching the existing `cursor` cast.
    pub row_cursors: Vec<i64>,
    /// G8 (Slice 20) — count of edge endpoints in this batch pointing at a
    /// non-existent or superseded canonical node (surfaced as
    /// `danglingEdgeEndpoints`; informational, flag-and-count). Narrowed `u64 →
    /// i64` at the FFI boundary, matching the `cursor`/`rowCursors` precedent.
    pub dangling_edge_endpoints: i64,
}

impl WriteReceipt {
    fn from_rust(r: RustWriteReceipt) -> Self {
        Self {
            cursor: r.cursor as i64,
            row_cursors: r.row_cursors.into_iter().map(|c| c as i64).collect(),
            dangling_edge_endpoints: r.dangling_edge_endpoints as i64,
        }
    }
}

/// G11 (Slice 15) — BYO-LLM ingest receipt.
#[napi(object)]
pub struct IngestWithExtractorReceipt {
    pub nodes_written: i64,
    pub edges_written: i64,
    pub docs_processed: i64,
}

impl IngestWithExtractorReceipt {
    fn from_rust(r: RustIngestWithExtractorReceipt) -> Self {
        Self {
            nodes_written: r.nodes_written as i64,
            edges_written: r.edges_written as i64,
            docs_processed: r.docs_processed as i64,
        }
    }
}

#[napi(object)]
pub struct SoftFallback {
    /// "vector" | "text" | "text_edge"
    pub branch: String,
}

#[napi(object)]
pub struct SearchHit {
    /// Canonical row `write_cursor` (interim identity carrier per
    /// ADR-0.8.0-canonical-identity-substrate).
    pub id: i64,
    pub kind: String,
    pub body: String,
    /// Raw per-branch relevance: `vec_distance_l2` (vector) or `bm25()`
    /// (text). Not comparable across branches raw.
    pub score: f64,
    /// "vector" | "text"
    pub branch: String,
}

impl SearchHit {
    fn from_rust(h: &RustSearchHit) -> Self {
        Self {
            id: h.id as i64,
            kind: h.kind.clone(),
            body: h.body.clone(),
            score: h.score,
            branch: match h.branch {
                SoftFallbackBranch::Vector => "vector".to_string(),
                SoftFallbackBranch::Text => "text".to_string(),
                SoftFallbackBranch::TextEdge => "text_edge".to_string(),
                SoftFallbackBranch::GraphArm => "graph_arm".to_string(),
            },
        }
    }
}

/// Slice 30 (G2) — an active canonical node row from `read.get` /
/// `read.getMany`. napi maps snake_case → camelCase JS (`logicalId`,
/// `writeCursor`).
#[napi(object)]
pub struct NodeRecord {
    pub logical_id: String,
    pub kind: String,
    pub body: String,
    pub write_cursor: i64,
}

impl NodeRecord {
    fn from_rust(r: &RustNodeRecord) -> Self {
        Self {
            logical_id: r.logical_id.clone(),
            kind: r.kind.clone(),
            body: r.body.clone(),
            write_cursor: r.write_cursor as i64,
        }
    }
}

/// Slice 30 (G3) — one `operational_mutations` row from `read.collection` /
/// `read.mutations`. `id` is the after-id cursor key. napi maps snake_case →
/// camelCase JS (`recordKey`, `opKind`, `schemaId`, `writeCursor`).
#[napi(object)]
pub struct OpStoreRow {
    pub id: i64,
    pub collection: String,
    pub record_key: String,
    pub op_kind: String,
    pub payload: String,
    pub schema_id: Option<String>,
    pub write_cursor: i64,
}

impl OpStoreRow {
    fn from_rust(r: &RustOpStoreRow) -> Self {
        Self {
            id: r.id,
            collection: r.collection.clone(),
            record_key: r.record_key.clone(),
            op_kind: r.op_kind.clone(),
            payload: r.payload.clone(),
            schema_id: r.schema_id.clone(),
            write_cursor: r.write_cursor as i64,
        }
    }
}

/// G10 — closed metadata filter input for `search(query, filter?)`. All fields
/// optional; an all-`None` filter (or omitted) is the unfiltered path. Mirrors
/// the Python `SearchFilter` (cross-binding parity). napi maps the snake_case
/// fields to camelCase JS (`sourceType`, `createdAfter`).
#[napi(object)]
pub struct SearchFilterInput {
    pub source_type: Option<String>,
    pub kind: Option<String>,
    pub created_after: Option<i64>,
    pub status: Option<String>,
}

#[napi(object)]
pub struct SearchResult {
    pub projection_cursor: i64,
    pub soft_fallback: Option<SoftFallback>,
    pub results: Vec<SearchHit>,
}

impl SearchResult {
    fn from_rust(r: RustSearchResult) -> Self {
        Self {
            projection_cursor: r.projection_cursor as i64,
            soft_fallback: r.soft_fallback.as_ref().map(|s| SoftFallback {
                branch: match s.branch {
                    SoftFallbackBranch::Vector => "vector".to_string(),
                    SoftFallbackBranch::Text => "text".to_string(),
                    SoftFallbackBranch::TextEdge => "text_edge".to_string(),
                    SoftFallbackBranch::GraphArm => "graph_arm".to_string(),
                },
            }),
            results: r.results.iter().map(SearchHit::from_rust).collect(),
        }
    }
}

#[napi(object)]
pub struct MigrationStepReport {
    pub step_id: u32,
    pub duration_ms: Option<i64>,
    pub failed: bool,
}

impl MigrationStepReport {
    fn from_rust(r: &RustMigrationStepReport) -> Self {
        Self { step_id: r.step_id, duration_ms: r.duration_ms.map(|v| v as i64), failed: r.failed }
    }
}

#[napi(object)]
pub struct EmbedderIdentity {
    pub name: String,
    pub revision: String,
    pub dimension: u32,
}

impl EmbedderIdentity {
    fn from_rust(id: &RustEmbedderIdentity) -> Self {
        Self { name: id.name.clone(), revision: id.revision.clone(), dimension: id.dimension }
    }
}

/// EU-6 — discriminated-union shape for `OpenReport.embedderEvents`.
///
/// `kind` carries the variant name (`"DefaultEmbedderDownload"`,
/// `"DefaultEmbedderCacheHit"`, `"MeanVecPinned"`); the remaining
/// optional fields carry the variant payload in camelCase. We pick a
/// flat object (rather than a per-variant `#[napi]` class) so callers
/// can pattern-match on `event.kind` without importing leaf classes.
#[napi(object)]
pub struct EmbedderEvent {
    pub kind: String,
    pub file: Option<String>,
    pub url: Option<String>,
    pub bytes: Option<i64>,
    pub sha256: Option<String>,
    pub cache_path: Option<String>,
    pub duration_ms: Option<i64>,
    pub dim: Option<u32>,
    pub doc_count: Option<i64>,
    /// 0.7.2 PR-2b — `"manual"` on `MeanVecRecomputed` (the automatic
    /// `"drift_auto"` trigger was carved out / deferred to 0.8.x).
    pub trigger: Option<String>,
    /// Reserved (always `None` as of 0.7.2 PR-2bc — the `MeanRecomputeDeferred`
    /// event that carried this was removed with the automatic drift path).
    pub drift_cos: Option<f64>,
}

impl EmbedderEvent {
    fn from_rust(ev: &RustEmbedderEvent) -> Self {
        match ev {
            RustEmbedderEvent::DefaultEmbedderDownload {
                file,
                url,
                bytes,
                sha256,
                cache_path,
                duration_ms,
            } => Self {
                kind: "DefaultEmbedderDownload".to_string(),
                file: Some(file.clone()),
                url: Some(url.clone()),
                bytes: Some(*bytes as i64),
                sha256: Some(sha256.clone()),
                cache_path: Some(cache_path.display().to_string()),
                duration_ms: Some(*duration_ms as i64),
                dim: None,
                doc_count: None,
                trigger: None,
                drift_cos: None,
            },
            RustEmbedderEvent::DefaultEmbedderCacheHit { file, sha256, cache_path } => Self {
                kind: "DefaultEmbedderCacheHit".to_string(),
                file: Some(file.clone()),
                url: None,
                bytes: None,
                sha256: Some(sha256.clone()),
                cache_path: Some(cache_path.display().to_string()),
                duration_ms: None,
                dim: None,
                doc_count: None,
                trigger: None,
                drift_cos: None,
            },
            RustEmbedderEvent::MeanVecPinned { dim, doc_count } => Self {
                kind: "MeanVecPinned".to_string(),
                file: None,
                url: None,
                bytes: None,
                sha256: None,
                cache_path: None,
                duration_ms: None,
                dim: Some(*dim),
                doc_count: Some(*doc_count as i64),
                trigger: None,
                drift_cos: None,
            },
            RustEmbedderEvent::MeanVecRecomputed { dim, doc_count, trigger } => Self {
                kind: "MeanVecRecomputed".to_string(),
                file: None,
                url: None,
                bytes: None,
                sha256: None,
                cache_path: None,
                duration_ms: None,
                dim: Some(*dim),
                doc_count: Some(*doc_count as i64),
                trigger: Some(trigger.as_str().to_string()),
                drift_cos: None,
            },
        }
    }
}

#[napi(object)]
pub struct OpenReport {
    pub schema_version_before: u32,
    pub schema_version_after: u32,
    pub migration_steps: Vec<MigrationStepReport>,
    pub embedder_warmup_ms: i64,
    pub query_backend: String,
    pub default_embedder: EmbedderIdentity,
    // EU-5a1/5a2/5b — surfaced by EU-6.
    pub embedder_download_ms: Option<i64>,
    pub embedder_events: Vec<EmbedderEvent>,
    pub embedder_mean_centering_required: bool,
    pub embedder_mean_vec_pinned: bool,
}

impl OpenReport {
    fn from_rust(r: &RustOpenReport) -> Self {
        Self {
            schema_version_before: r.schema_version_before,
            schema_version_after: r.schema_version_after,
            migration_steps: r.migration_steps.iter().map(MigrationStepReport::from_rust).collect(),
            embedder_warmup_ms: r.embedder_warmup_ms as i64,
            query_backend: r.query_backend.to_string(),
            default_embedder: EmbedderIdentity::from_rust(&r.default_embedder),
            embedder_download_ms: r.embedder_download_ms.map(|v| v as i64),
            embedder_events: r.embedder_events.iter().map(EmbedderEvent::from_rust).collect(),
            embedder_mean_centering_required: r.embedder_mean_centering_required,
            embedder_mean_vec_pinned: r.embedder_mean_vec_pinned,
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
    /// EU-6: opt-in to the engine's pinned default embedder
    /// (`fathomdb-bge-small-en-v1.5`). On first use, weights are
    /// downloaded from HuggingFace and cached under
    /// `~/.cache/fathomdb/embedders/`. `false` (the default) opens
    /// without an embedder; vector writes then fail with
    /// `EmbedderNotConfigured`.
    pub use_default_embedder: Option<bool>,
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
    open_report: Arc<RustOpenReport>,
}

#[napi]
impl Engine {
    /// Promise-returning open per `dev/interfaces/typescript.md`. The
    /// blocking SQLite work runs on a tokio blocking thread.
    #[napi(factory)]
    pub async fn open(path: String, options: Option<EngineOpenOptions>) -> Result<Engine> {
        validate_ffi_string_napi(&path)?;
        // EU-6: `useDefaultEmbedder: true` → EmbedderChoice::Default
        // (engine materialises the pinned bge-small embedder via the
        // EU-3 loader); `false`/unset → EmbedderChoice::None (engine
        // opens; vector writes fail EmbedderNotConfigured). Caller-
        // supplied custom embedders are deferred per
        // ADR-0.6.0-embedder-protocol Invariant 3.
        let use_default_embedder =
            options.as_ref().and_then(|o| o.use_default_embedder).unwrap_or(false);
        let join_result = tokio::task::spawn_blocking(move || {
            catch_unwind(AssertUnwindSafe(|| {
                let choice = if use_default_embedder {
                    EmbedderChoice::Default
                } else {
                    EmbedderChoice::None
                };
                RustEngine::open_with_choice(path, choice)
            }))
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
        Ok(Engine { inner: Arc::new(opened.engine), open_report: Arc::new(opened.report) })
    }

    /// Structured open report captured at [`Engine::open`] time. Sync
    /// accessor (no Promise — the data lives on the engine struct
    /// after open). Idempotent: each call returns a fresh copy of the
    /// same snapshot.
    #[napi]
    pub fn open_report(&self) -> Result<OpenReport> {
        let report = Arc::clone(&self.open_report);
        call_engine_sync(move || Ok(OpenReport::from_rust(&report)))
    }

    #[napi]
    pub async fn write(&self, batch: Vec<JsonValue>) -> Result<WriteReceipt> {
        let prepared = translate_batch(batch)?;
        let engine = Arc::clone(&self.inner);
        let receipt = call_engine(move || engine.write(&prepared)).await?;
        Ok(WriteReceipt::from_rust(receipt))
    }

    #[napi]
    pub async fn search(
        &self,
        query: String,
        filter: Option<SearchFilterInput>,
        rerank_depth: Option<u32>,
        // 0.8.1 R3 (Slice 30) — when true, seed a BFS over temporal fact-edges
        // from the top-10 fused hits and fuse the reachable nodes as a third RRF arm.
        // Default false → byte-identical to the pre-Slice-30 two-arm pipeline.
        use_graph_arm: Option<bool>,
    ) -> Result<SearchResult> {
        validate_ffi_string_napi(&query)?;
        if query.trim().is_empty() {
            return Err(typed_error(
                CODE_WRITE_VALIDATION,
                "query must not be empty",
                JsonValue::Null,
            ));
        }
        // G10 filter strings cross the FFI exactly like `query` and the write
        // fields, so they go through the same Rust-side guard BEFORE the engine
        // is touched (defense-in-depth for embedded NUL, as on the write path).
        // `created_after` is numeric — no string validation. Lone UTF-16
        // surrogates are napi-rs-lossy here (replaced with U+FFFD before Rust
        // sees them), so — like write/configure — the TS `search` wrapper guards
        // those JS-side (see src/validation.ts). Validating here leaves the
        // all-`None` collapse below (the byte-identical unfiltered path) intact.
        if let Some(f) = filter.as_ref() {
            for s in [f.source_type.as_deref(), f.kind.as_deref(), f.status.as_deref()]
                .into_iter()
                .flatten()
            {
                validate_ffi_string_napi(s)?;
            }
        }
        // G10 — build the closed filter; an all-`None` (or omitted) filter stays
        // the unfiltered, byte-identical path.
        let filter = filter.and_then(|f| {
            let rust = RustSearchFilter {
                source_type: f.source_type,
                kind: f.kind,
                created_after: f.created_after,
                status: f.status,
            };
            if rust.source_type.is_none()
                && rust.kind.is_none()
                && rust.created_after.is_none()
                && rust.status.is_none()
            {
                None
            } else {
                Some(rust)
            }
        });
        // 0.8.1 R1: rerank_depth=None or 0 → soft-fallback (identity).
        let depth = rerank_depth.unwrap_or(0) as usize;
        // 0.8.1 R3: use_graph_arm=None or false → two-arm byte-identical path.
        let graph_arm = use_graph_arm.unwrap_or(false);
        let engine = Arc::clone(&self.inner);
        let result =
            call_engine(move || engine.search_reranked(&query, filter, depth, graph_arm)).await?;
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

    /// G11 (Slice 15) — BYO-LLM ingest. `cmd` is the argv to spawn (first
    /// element = program, rest = args). `documents` is an array of objects
    /// with `sourceDocId` and `body` string properties.
    #[napi]
    pub async fn ingest_with_extractor(
        &self,
        cmd: Vec<String>,
        documents: Vec<JsonValue>,
    ) -> Result<IngestWithExtractorReceipt> {
        let docs: Vec<RustExtractDocument> = documents
            .iter()
            .map(|item| {
                let source_doc_id = item
                    .get("sourceDocId")
                    .or_else(|| item.get("source_doc_id"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        typed_error(
                            CODE_WRITE_VALIDATION,
                            "document must have sourceDocId",
                            JsonValue::Null,
                        )
                    })?
                    .to_string();
                let body = item
                    .get("body")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        typed_error(
                            CODE_WRITE_VALIDATION,
                            "document must have body",
                            JsonValue::Null,
                        )
                    })?
                    .to_string();
                Ok(RustExtractDocument { source_doc_id, body })
            })
            .collect::<Result<_>>()?;

        let engine = Arc::clone(&self.inner);
        let receipt = call_engine(move || {
            let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
            engine.ingest_with_extractor(&cmd_refs, &docs)
        })
        .await?;
        Ok(IngestWithExtractorReceipt::from_rust(receipt))
    }

    #[napi]
    pub fn counters(&self) -> Result<CounterSnapshot> {
        let engine = Arc::clone(&self.inner);
        call_engine_sync(move || {
            let snap = engine.counters();
            Ok(CounterSnapshot {
                queries: snap.queries as i64,
                writes: snap.writes as i64,
                write_rows: snap.write_rows as i64,
                admin_ops: snap.admin_ops as i64,
                cache_hit: snap.cache_hit as i64,
                cache_miss: snap.cache_miss as i64,
            })
        })
    }

    #[napi]
    pub fn set_profiling(&self, enabled: bool) -> Result<()> {
        let engine = Arc::clone(&self.inner);
        call_engine_sync(move || engine.set_profiling(enabled).map_err(engine_error_to_napi))
    }

    #[napi]
    pub fn set_slow_threshold_ms(&self, value: u32) -> Result<()> {
        let engine = Arc::clone(&self.inner);
        call_engine_sync(move || {
            engine.set_slow_threshold_ms(value as u64).map_err(engine_error_to_napi)
        })
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
        call_engine_sync(|| Ok(()))
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

// ===== read.* (G2/G3) =================================================
//
// Slice 30 — the governed `read.*` namespace native fns. `read.get` /
// `read.getMany` are active-only point lookups by `logicalId` (not-found is a
// normal `null`, never a thrown error — a typed NotFound class is reserved-gap
// Slice 31). `read.collection` / `read.mutations` are the paginated op-store
// read-back with a MANDATORY limit + after-id cursor. All four ride the engine's
// ReaderWorkerPool DEFERRED-tx path; the binding only marshals.

/// Slice 30 (G3) — options for `read.collection` / `read.mutations`. `limit` is
/// MANDATORY (no default — the engine clamps it to the ~1M cap); `afterId` is
/// the exclusive cursor.
#[napi(object)]
pub struct ReadCollectionOptions {
    pub after_id: Option<i64>,
    pub limit: i64,
}

#[napi(js_name = "readGet")]
pub async fn read_get(engine: &Engine, logical_id: String) -> Result<Option<NodeRecord>> {
    validate_ffi_string_napi(&logical_id)?;
    let inner = Arc::clone(&engine.inner);
    let record = call_engine(move || inner.read_get(&logical_id)).await?;
    Ok(record.as_ref().map(NodeRecord::from_rust))
}

#[napi(js_name = "readGetMany")]
pub async fn read_get_many(
    engine: &Engine,
    logical_ids: Vec<String>,
) -> Result<Vec<Option<NodeRecord>>> {
    for id in &logical_ids {
        validate_ffi_string_napi(id)?;
    }
    let inner = Arc::clone(&engine.inner);
    let rows = call_engine(move || inner.read_get_many(&logical_ids)).await?;
    Ok(rows.iter().map(|r| r.as_ref().map(NodeRecord::from_rust)).collect())
}

#[napi(js_name = "readCollection")]
pub async fn read_collection(
    engine: &Engine,
    collection: String,
    options: ReadCollectionOptions,
) -> Result<Vec<OpStoreRow>> {
    read_collection_impl(engine, collection, options).await
}

#[napi(js_name = "readMutations")]
pub async fn read_mutations(
    engine: &Engine,
    collection: String,
    options: ReadCollectionOptions,
) -> Result<Vec<OpStoreRow>> {
    read_collection_impl(engine, collection, options).await
}

async fn read_collection_impl(
    engine: &Engine,
    collection: String,
    options: ReadCollectionOptions,
) -> Result<Vec<OpStoreRow>> {
    validate_ffi_string_napi(&collection)?;
    let after_id = options.after_id;
    // A negative limit is meaningless; clamp the floor to 0 (empty read). The
    // engine clamps the ceiling to the ~1M cap.
    let limit = options.limit.max(0) as usize;
    let inner = Arc::clone(&engine.inner);
    let rows = call_engine(move || inner.read_collection(&collection, after_id, limit)).await?;
    Ok(rows.iter().map(OpStoreRow::from_rust).collect())
}

// ===== read.list (G4 / Slice 35) ======================================

/// G4 (Slice 35) — predicate input for `readList`. Shape mirrors the TS
/// `Predicate` interface: `type` ∈ `{"eq","gt","gte","lt","lte"}`, `path`,
/// `value` (JS `string | number | boolean` — carried as `f64` for numbers).
#[napi(object)]
pub struct PredicateInput {
    /// Comparison type: "eq" | "gt" | "gte" | "lt" | "lte".
    pub r#type: String,
    /// JSON path from the allowlist (e.g. "$.status", "$.priority").
    pub path: String,
    /// String value for eq/gt/gte/lt/lte string comparisons.
    pub value_str: Option<String>,
    /// Integer value for numeric comparisons.
    pub value_int: Option<i64>,
    /// Boolean value for bool comparisons.
    pub value_bool: Option<bool>,
}

fn napi_predicate_to_rust(pred: PredicateInput) -> Result<RustPredicate> {
    // Determine the scalar value: bool > int > str (bool is also "truthy" int in JS).
    let scalar = if let Some(b) = pred.value_bool {
        RustScalarValue::Bool(b)
    } else if let Some(i) = pred.value_int {
        RustScalarValue::Integer(i)
    } else if let Some(s) = pred.value_str {
        RustScalarValue::Text(s)
    } else {
        return Err(typed_error(
            CODE_INVALID_FILTER,
            "predicate must have one of value_str, value_int, or value_bool",
            JsonValue::Null,
        ));
    };
    match pred.r#type.as_str() {
        "eq" => RustPredicate::json_path_eq(pred.path, scalar).map_err(engine_error_to_napi),
        "gt" => RustPredicate::json_path_compare(pred.path, RustComparisonOp::Gt, scalar)
            .map_err(engine_error_to_napi),
        "gte" => RustPredicate::json_path_compare(pred.path, RustComparisonOp::Gte, scalar)
            .map_err(engine_error_to_napi),
        "lt" => RustPredicate::json_path_compare(pred.path, RustComparisonOp::Lt, scalar)
            .map_err(engine_error_to_napi),
        "lte" => RustPredicate::json_path_compare(pred.path, RustComparisonOp::Lte, scalar)
            .map_err(engine_error_to_napi),
        other => Err(typed_error(
            CODE_INVALID_FILTER,
            format!("unknown predicate type '{other}'; expected eq/gt/gte/lt/lte"),
            JsonValue::Null,
        )),
    }
}

#[napi(js_name = "readList")]
pub async fn read_list(
    engine: &Engine,
    kind: String,
    predicates: Option<Vec<PredicateInput>>,
    limit: Option<i64>,
) -> Result<Vec<NodeRecord>> {
    validate_ffi_string_napi(&kind)?;
    let mut rust_predicates: Vec<RustPredicate> = Vec::new();
    if let Some(plist) = predicates {
        for pred in plist {
            rust_predicates.push(napi_predicate_to_rust(pred)?);
        }
    }
    let limit = limit.unwrap_or(100).max(0) as usize;
    let inner = Arc::clone(&engine.inner);
    let rows = call_engine(move || inner.read_list(&kind, &rust_predicates, limit)).await?;
    Ok(rows.iter().map(NodeRecord::from_rust).collect())
}

// ===== Slice 20 (G5/G6) — graph traversal ==============================
//
// `graphNeighbors` (G5) — bounded BFS from a root node, returning the
// reachable `NodeRecord`s within `depth` hops. `searchExpand` (G6)
// composes G1 search + G5 expansion with deduplication.

/// Slice 20 — one expanded node entry in `SearchExpandResult.expanded`.
/// `hopCount` is the BFS distance from the nearest search-hit root.
#[napi(object)]
pub struct ExpandedNode {
    pub node: NodeRecord,
    pub hop_count: u32,
}

/// Slice 20 (G6) — result of `searchExpand`.
///
/// `searchHits` — original RRF-scored results from the search step.
/// `expanded`   — nodes reachable from any hit within `depth` hops that are
///                NOT in `searchHits` (deduplication: search score wins).
/// `allLogicalIds` — deduplicated union of both sets.
#[napi(object)]
pub struct SearchExpandResult {
    pub search_hits: Vec<SearchHit>,
    pub expanded: Vec<ExpandedNode>,
    pub all_logical_ids: Vec<String>,
}

impl SearchExpandResult {
    fn from_rust(r: RustSearchExpandResult) -> Self {
        Self {
            search_hits: r.search_hits.iter().map(SearchHit::from_rust).collect(),
            expanded: r
                .expanded
                .into_iter()
                .map(|(node, hop_count)| ExpandedNode {
                    node: NodeRecord::from_rust(&node),
                    hop_count,
                })
                .collect(),
            all_logical_ids: r.all_logical_ids,
        }
    }
}

fn parse_direction_napi(direction: &str) -> Result<RustTraversalDirection> {
    match direction {
        "outgoing" => Ok(RustTraversalDirection::Outgoing),
        "incoming" => Ok(RustTraversalDirection::Incoming),
        "both" => Ok(RustTraversalDirection::Both),
        other => Err(typed_error(
            CODE_INVALID_ARGUMENT,
            format!("direction must be 'outgoing', 'incoming', or 'both'; got '{other}'"),
            JsonValue::Null,
        )),
    }
}

/// Slice 20 (G5) — bounded BFS from `logicalId` over `canonical_edges`.
///
/// `depth` must be 1–3; rejects depth > 3 with `InvalidArgumentError`.
/// `direction` is `"outgoing"`, `"incoming"`, or `"both"`.
/// Returns up to 50 `NodeRecord`s reachable within `depth` hops.
/// Edges with `t_invalid` in the past are not traversed.
#[napi(js_name = "graphNeighbors")]
pub async fn graph_neighbors(
    engine: &Engine,
    logical_id: String,
    depth: u32,
    direction: String,
) -> Result<Vec<NodeRecord>> {
    validate_ffi_string_napi(&logical_id)?;
    let dir = parse_direction_napi(&direction)?;
    let inner = Arc::clone(&engine.inner);
    let nodes = call_engine(move || inner.graph_neighbors(&logical_id, depth, dir)).await?;
    Ok(nodes.iter().map(NodeRecord::from_rust).collect())
}

/// Slice 20 (G6) — FTS/vector search followed by bounded BFS expansion.
///
/// Runs `search(query, filter)` (G1), then expands each hit via
/// `graph_neighbors(depth, both)`. Nodes appearing in both sets appear
/// only in `searchHits` (deduplication: search score takes priority).
#[napi(js_name = "searchExpand")]
pub async fn search_expand(
    engine: &Engine,
    query: String,
    depth: u32,
    source_type: Option<String>,
    kind: Option<String>,
    created_after: Option<i64>,
    status: Option<String>,
) -> Result<SearchExpandResult> {
    validate_ffi_string_napi(&query)?;
    let filter =
        if source_type.is_some() || kind.is_some() || created_after.is_some() || status.is_some() {
            Some(RustSearchFilter { source_type, kind, created_after, status })
        } else {
            None
        };
    let inner = Arc::clone(&engine.inner);
    let result = call_engine(move || inner.search_expand(&query, filter, depth)).await?;
    Ok(SearchExpandResult::from_rust(result))
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
    let logical_id = json_str_alt(item, "logicalId", "logical_id")?;
    Ok(PreparedWrite::Node { kind, body, source_id, logical_id })
}

fn translate_edge(item: &JsonValue) -> Result<PreparedWrite> {
    let kind = json_str_required(item, "kind")?;
    let from = json_str_required(item, "from")?;
    let to = json_str_required(item, "to")?;
    let source_id = json_str_alt(item, "sourceId", "source_id")?;
    let logical_id = json_str_alt(item, "logicalId", "logical_id")?;
    Ok(PreparedWrite::Edge {
        kind,
        from,
        to,
        source_id,
        logical_id,
        body: None,
        t_valid: None,
        t_invalid: None,
        confidence: None,
        extractor_model_id: None,
    })
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

/// EU-6 — test-hooks-gated vector write seam. Lets TS tests exercise
/// the 0.5/§7 mean-vec pin transition end-to-end through the binding
/// (the public TS surface does not yet expose typed vector writes; that
/// is its own multi-slice campaign). Compiled out of release npm builds
/// by the `test-hooks` cfg. Kept in a separate `#[napi] impl` block
/// because napi-derive's per-method `#[cfg]` gating inside the
/// production-surface impl block does not compose with the impl-level
/// `#[napi]` glue table.
#[cfg(any(test, feature = "test-hooks"))]
#[napi]
impl Engine {
    #[napi]
    pub async fn configure_vector_kind_for_test(&self, kind: String) -> Result<()> {
        validate_ffi_string_napi(&kind)?;
        let engine = Arc::clone(&self.inner);
        call_engine(move || engine.configure_vector_kind_for_test(&kind)).await
    }

    #[napi]
    pub async fn write_vector_for_test(&self, kind: String, text: String) -> Result<()> {
        validate_ffi_string_napi(&kind)?;
        validate_ffi_string_napi(&text)?;
        let engine = Arc::clone(&self.inner);
        call_engine(move || engine.write_vector_for_test(&kind, &text).map(|_| ())).await
    }
}

/// AC-067 force-panic probe. Gated by `cfg(any(test, feature =
/// "test-hooks"))` so release npm builds without the feature flag do
/// not expose it.
#[cfg(any(test, feature = "test-hooks"))]
#[napi(js_name = "forcePanicForTest")]
pub fn force_panic_for_test() -> Result<()> {
    call_panicking_engine_for_test()
}

/// AC-067 sync-path probe: exercises [`call_engine_sync`] so the
/// TS-side test asserts that panics on a sync `#[napi]` accessor land
/// as `FathomDbPanicError` (code `FDB_PANIC`) too, not just the async
/// path covered by [`force_panic_for_test`].
#[cfg(any(test, feature = "test-hooks"))]
#[napi(js_name = "forcePanicInAccessorForTest")]
pub fn force_panic_in_accessor_for_test() -> Result<()> {
    call_engine_sync(|| -> Result<()> {
        panic!("force_panic_in_accessor_for_test: AC-067 sync probe");
    })
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
