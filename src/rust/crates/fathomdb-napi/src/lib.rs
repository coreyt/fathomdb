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
    BoundaryCrossing as RustBoundaryCrossing, ComparisonOp as RustComparisonOp,
    ConsolidateAxis as RustConsolidateAxis, ConsolidateReceipt as RustConsolidateReceipt,
    CorruptionDetail, CorruptionKind, EmbedderChoice, Engine as RustEngine,
    EngineError as RustEngineError, EngineOpenError, ExciseReport as RustExciseReport,
    Explanation as RustExplanation, ExtractDocument as RustExtractDocument, Filter as RustFilter,
    FilterTerm as RustFilterTerm, IdSpace as RustIdSpace,
    IngestWithExtractorReceipt as RustIngestWithExtractorReceipt, InitialState,
    LifecycleState as RustLifecycleState, NodeRecord as RustNodeRecord,
    OpStoreRow as RustOpStoreRow, OpenReport as RustOpenReport, OpenStage,
    PerHitExplain as RustPerHitExplain, Predicate as RustPredicate, PreparedWrite,
    ProjectionDelta as RustProjectionDelta, ProjectionFts as RustProjectionFts,
    ProjectionRole as RustProjectionRole, ProjectionSpec as RustProjectionSpec,
    ProjectionVector as RustProjectionVector, QueryTrace as RustQueryTrace,
    ReadView as RustReadView, ScalarValue as RustScalarValue,
    SearchExpandResult as RustSearchExpandResult, SearchFilter as RustSearchFilter,
    SearchHit as RustSearchHit, SearchResult as RustSearchResult, SoftFallbackBranch, SourceId,
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
// 0.8.12 Slice 15 (OPP-2) — BYO-LLM consolidation harness protocol error.
const CODE_CONSOLIDATOR: &str = "FDB_CONSOLIDATOR";
// G4 (Slice 35) — filter predicate construction error (non-allowlisted path).
const CODE_INVALID_FILTER: &str = "FDB_INVALID_FILTER";
// Slice 20 — depth > 3 or invalid argument (G5/G6).
const CODE_INVALID_ARGUMENT: &str = "FDB_INVALID_ARGUMENT";
// 0.8.18 Slice 5 (#5 vector-equivalence probe) — query-time dense-refusal code.
const CODE_VECTOR_EQUIVALENCE_MISMATCH: &str = "FDB_VECTOR_EQUIVALENCE_MISMATCH";
// OPP-12 Phase-1 (0.8.19 Slice 10) — lifecycle-verb typed errors.
const CODE_ILLEGAL_TRANSITION: &str = "FDB_ILLEGAL_TRANSITION";
const CODE_NOT_LIFECYCLE_ADDRESSABLE: &str = "FDB_NOT_LIFECYCLE_ADDRESSABLE";
// 0.8.20 Slice 5b (R-20-E5) — an erasure verb deleted its rows but could not
// complete the erasure at rest.
const CODE_ERASURE_INCOMPLETE: &str = "FDB_ERASURE_INCOMPLETE";
// 0.8.20 Slice 15d (R-20-PR) — configure_projections refused a destructive
// change without an explicit drop.
const CODE_PROJECTION_DESTRUCTIVE: &str = "FDB_PROJECTION_DESTRUCTIVE";
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

/// codex §9 [P2] — convert a napi `i64` id list to the engine's `u64`, rejecting
/// any negative id (which `as u64` would silently wrap). Mirrors the TS/Python
/// wrapper `validateIdArray`/`_validate_id_list` guards so a raw-napi caller
/// cannot smuggle a wrapped id past the boundary.
fn checked_ids_napi(name: &str, ids: &[i64]) -> Result<Vec<u64>> {
    ids.iter()
        .map(|&x| {
            u64::try_from(x).map_err(|_| {
                typed_error(
                    CODE_WRITE_VALIDATION,
                    format!("{name} must contain only non-negative integers, got {x}"),
                    JsonValue::Null,
                )
            })
        })
        .collect()
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
        RustEngineError::Consolidator => {
            typed_error(CODE_CONSOLIDATOR, "consolidator error", JsonValue::Null)
        }
        RustEngineError::InvalidFilter { reason } => {
            typed_error(CODE_INVALID_FILTER, format!("invalid filter: {reason}"), JsonValue::Null)
        }
        RustEngineError::InvalidArgument { msg } => {
            typed_error(CODE_INVALID_ARGUMENT, msg, JsonValue::Null)
        }
        RustEngineError::IllegalTransition { from_state, to_state, legal } => {
            let legal_str: Vec<&'static str> = legal.iter().map(|s| s.as_str()).collect();
            typed_error(
                CODE_ILLEGAL_TRANSITION,
                format!(
                    "illegal lifecycle transition {} -> {}; legal targets: {:?}",
                    from_state.as_str(),
                    to_state.as_str(),
                    legal_str,
                ),
                // Parity-safe field names (S7): `fromState`/`toState`, never `from`.
                json!({
                    "fromState": from_state.as_str(),
                    "toState": to_state.as_str(),
                    "legal": legal_str,
                }),
            )
        }
        RustEngineError::NotLifecycleAddressable { id_space } => typed_error(
            CODE_NOT_LIFECYCLE_ADDRESSABLE,
            format!(
                "id space {:?} is not lifecycle-addressable; only the logical (l:) space is",
                id_space.as_str(),
            ),
            json!({ "idSpace": id_space.as_str() }),
        ),
        RustEngineError::VectorEquivalenceMismatch { reason } => typed_error(
            CODE_VECTOR_EQUIVALENCE_MISMATCH,
            format!("vector-equivalence self-check failed; dense retrieval refused: {reason}"),
            json!({ "reason": reason }),
        ),
        RustEngineError::ErasureIncomplete { stage, detail } => typed_error(
            CODE_ERASURE_INCOMPLETE,
            format!("erasure incomplete at stage '{stage}': {detail}"),
            json!({ "stage": stage, "detail": detail }),
        ),
        RustEngineError::ProjectionDestructive { name, delta } => typed_error(
            CODE_PROJECTION_DESTRUCTIVE,
            format!(
                "configure_projections refused a destructive change to '{name}': {delta}; \
                 re-issue with drop: [\"{name}\"]"
            ),
            json!({ "name": name, "delta": delta }),
        ),
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

/// 0.8.20 Slice 5d (R-20-E4) — outcome of the `eraseSource` lifecycle verb.
/// Mirrors the Rust `ExciseReport`. Counts are narrowed `u64 -> i64` at the FFI
/// boundary, matching the `WriteReceipt.cursor` precedent.
#[napi(object)]
pub struct EraseReport {
    pub source_ref: String,
    pub nodes_excised: i64,
    pub edges_excised: i64,
    /// Row-owned projection rows (FTS5 + vec0 + `search_index_v2`) dropped
    /// alongside the canonical rows.
    pub projections_invalidated: i64,
}

impl EraseReport {
    fn from_rust(r: RustExciseReport) -> Self {
        Self {
            source_ref: r.source_ref,
            nodes_excised: r.nodes_excised as i64,
            edges_excised: r.edges_excised as i64,
            projections_invalidated: r.projections_invalidated as i64,
        }
    }
}

/// 0.8.20 Slice 15d (R-20-PR) — a declarative projection declaration. Flat at
/// the FFI boundary; `fts`/`vector` booleans carry the sub-object PRESENCE and
/// the optional tokenizer/embedder carry the value. JS field names are
/// camelCase (`ftsTokenizer` / `vectorEmbedder`).
#[napi(object)]
pub struct ProjectionSpec {
    pub name: String,
    pub roles: Vec<String>,
    pub fts: bool,
    pub fts_tokenizer: Option<String>,
    pub vector: bool,
    pub vector_embedder: Option<String>,
}

impl ProjectionSpec {
    fn from_rust(s: &RustProjectionSpec) -> Self {
        Self {
            name: s.name.clone(),
            roles: s.roles.iter().map(|r| r.as_str().to_string()).collect(),
            fts: s.fts.is_some(),
            fts_tokenizer: s.fts.as_ref().and_then(|f| f.tokenizer.clone()),
            vector: s.vector.is_some(),
            vector_embedder: s.vector.as_ref().and_then(|v| v.embedder.clone()),
        }
    }

    fn to_rust(&self) -> Result<RustProjectionSpec> {
        // AC-068a/b — reject every string crossing the FFI into the spec BEFORE
        // the engine (writer transaction) is reached. Mirrors the per-string
        // gate applied at every other napi call site (e.g. `:1141`).
        validate_ffi_string_napi(&self.name)?;
        if let Some(tokenizer) = &self.fts_tokenizer {
            validate_ffi_string_napi(tokenizer)?;
        }
        if let Some(embedder) = &self.vector_embedder {
            validate_ffi_string_napi(embedder)?;
        }
        let mut roles = std::collections::BTreeSet::new();
        for r in &self.roles {
            validate_ffi_string_napi(r)?;
            let role = RustProjectionRole::from_str_opt(r).ok_or_else(|| {
                typed_error(
                    CODE_INVALID_ARGUMENT,
                    format!(
                        "unknown projection role {r:?}: expected filterable/rankable/searchable"
                    ),
                    JsonValue::Null,
                )
            })?;
            roles.insert(role);
        }
        Ok(RustProjectionSpec {
            name: self.name.clone(),
            roles,
            fts: self.fts.then(|| RustProjectionFts { tokenizer: self.fts_tokenizer.clone() }),
            vector: self
                .vector
                .then(|| RustProjectionVector { embedder: self.vector_embedder.clone() }),
        })
    }
}

/// 0.8.20 Slice 15d (R-20-PR) — the diff `configureProjections` applied.
#[napi(object)]
pub struct ProjectionDelta {
    pub built: Vec<String>,
    pub dropped: Vec<String>,
    pub deferred: Vec<String>,
    pub unchanged: bool,
}

impl ProjectionDelta {
    fn from_rust(d: &RustProjectionDelta) -> Self {
        Self {
            built: d.built.clone(),
            dropped: d.dropped.clone(),
            deferred: d.deferred.clone(),
            unchanged: d.unchanged,
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

/// 0.8.12 Slice 15 (OPP-2) — BYO-LLM consolidation receipt.
#[napi(object)]
pub struct ConsolidateReceipt {
    pub clusters_processed: i64,
    pub edges_examined: i64,
    pub edges_kept: i64,
    pub edges_invalidated: i64,
    pub edges_superseded: i64,
}

impl ConsolidateReceipt {
    fn from_rust(r: RustConsolidateReceipt) -> Self {
        Self {
            clusters_processed: r.clusters_processed as i64,
            edges_examined: r.edges_examined as i64,
            edges_kept: r.edges_kept as i64,
            edges_invalidated: r.edges_invalidated as i64,
            edges_superseded: r.edges_superseded as i64,
        }
    }
}

#[napi(object)]
pub struct SoftFallback {
    /// "vector" | "text" | "text_edge"
    pub branch: String,
}

/// C-2 (0.8.19 / OPP-12 Phase-1, TC-8) — the typed id-space carrier for
/// [`SearchHit::id`], surfaced to JS as `{ space, value }`. `space` is the
/// lowercase discriminant (`"logical"` | `"content"` | `"passage"`), mirroring
/// the engine's `IdSpaceKind` enum (the C-2 binding — a typed carrier, not a
/// magic-prefixed string). `value` is the bare id (id-space prefix stripped).
#[napi(object)]
pub struct IdSpace {
    /// "logical" | "content" | "passage"
    pub space: String,
    /// The bare id value (id-space prefix stripped).
    pub value: String,
}

impl IdSpace {
    fn from_rust(id: &RustIdSpace) -> Self {
        Self { space: id.space.as_str().to_string(), value: id.value.clone() }
    }
}

#[napi(object)]
pub struct SearchHit {
    /// C-2 (0.8.19 / TC-8) — the typed, non-null, id-space-total hit id
    /// (`{ space, value }`). Governed hits are `logical` (`"l:"`), doc-seeded hits
    /// `content` (`"h:"`), synthetic passages `passage` (`"p:"`). Its `value`
    /// equals the pre-0.8.19 `stableId` (which this subsumes) so cross-session
    /// real-gold keying continues on `id`. The pre-C-2 positional `write_cursor`
    /// id is engine-internal and no longer surfaced.
    pub id: IdSpace,
    pub kind: String,
    pub body: String,
    /// Raw per-branch relevance: `vec_distance_l2` (vector) or `bm25()`
    /// (text). Not comparable across branches raw.
    pub score: f64,
    /// "vector" | "text"
    pub branch: String,
    /// Source-document provenance (`sourceId` in JS) — the identifier
    /// `eraseSource` consumes. TC-31 (0.8.20): populated on EVERY hit path, not
    /// just the graph arm. Node hits (text/vector) carry the node's own
    /// `source_id`; edge hits (edge-FTS, vector edge-fact) carry the edge's own;
    /// graph-arm hits carry the traversed edge's (unchanged). `null` only when
    /// the stored row really has NULL provenance: written before 0.8.20, or a
    /// governed row spared by the step-21 backfill under the TC-11 pin.
    pub source_id: Option<String>,
    /// 0.8.5 (EXP-0) — per-candidate CE score `ce_norm = sigmoid(ce_logit)`
    /// (`ceScore` in JS). Set only for hits inside the reranked pool; `null`
    /// otherwise (out-of-pool, identity path, or no CE model loaded).
    pub ce_score: Option<f64>,
}

impl SearchHit {
    fn from_rust(h: &RustSearchHit) -> Self {
        Self {
            id: IdSpace::from_rust(&h.id),
            kind: h.kind.clone(),
            body: h.body.clone(),
            score: h.score,
            branch: match h.branch {
                SoftFallbackBranch::Vector => "vector".to_string(),
                SoftFallbackBranch::Text => "text".to_string(),
                SoftFallbackBranch::TextEdge => "text_edge".to_string(),
                SoftFallbackBranch::GraphArm => "graph_arm".to_string(),
            },
            source_id: h.source_id.clone(),
            ce_score: h.ce_score,
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
    /// 0.8.8 EXP-OBS (Slice 10) — opt-in retrieval explanation sidecar
    /// (`explanation` in JS). Present only when `search(..., explain=true)`; `null`
    /// (default) keeps the payload byte-identical to the pre-0.8.8 shape.
    pub explanation: Option<Explanation>,
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
            explanation: r.explanation.as_ref().map(Explanation::from_rust),
        }
    }
}

/// 0.8.8 EXP-OBS (Slice 10) — query-level retrieval trace (mirror of engine
/// `QueryTrace`). napi maps snake_case → camelCase JS (`queryChars`,
/// `rerankDepth`, `useGraphArm`, `embedderId`, `ceActive`, `vectorHits`, …).
#[napi(object)]
pub struct QueryTrace {
    pub query_chars: u32,
    pub k: u32,
    pub rerank_depth: u32,
    pub pool_n: u32,
    pub alpha: f64,
    pub use_graph_arm: bool,
    pub recency: bool,
    pub embedder_id: String,
    pub ce_active: bool,
    pub vector_hits: u32,
    pub text_hits: u32,
    pub graph_hits: u32,
}

impl QueryTrace {
    fn from_rust(t: &RustQueryTrace) -> Self {
        Self {
            query_chars: t.query_chars,
            k: t.k,
            rerank_depth: t.rerank_depth,
            pool_n: t.pool_n,
            alpha: t.alpha,
            use_graph_arm: t.use_graph_arm,
            recency: t.recency,
            embedder_id: t.embedder_id.clone(),
            ce_active: t.ce_active,
            vector_hits: t.vector_hits,
            text_hits: t.text_hits,
            graph_hits: t.graph_hits,
        }
    }
}

/// 0.8.8 EXP-OBS (Slice 10) — per-hit provenance + score breakdown (mirror of
/// engine `PerHitExplain`). `id` is the hit's engine-internal positional
/// `write_cursor` (the pre-0.8.19 `SearchHit.id`), `as i64` → JS number (NO
/// BigInt/string promotion). Post-C-2 the caller-facing `SearchHit.id` is the
/// typed `{ space, value }` object; correlate a `PerHitExplain` to its `SearchHit`
/// by position (both arrays are 1:1, same order). napi maps snake_case →
/// camelCase (`vectorRank`, `fusedScore`, `ceScore`).
#[napi(object)]
pub struct PerHitExplain {
    pub id: i64,
    pub arm: String,
    pub vector_rank: Option<u32>,
    pub text_rank: Option<u32>,
    pub graph_rank: Option<u32>,
    pub fused_score: f64,
    pub ce_score: Option<f64>,
    pub blended: f64,
    /// 0.8.16 Slice 5 / F9 — node importance / edge confidence applied to this
    /// hit's contribution (`None` = graceful-absent / neutral). Mirrors the
    /// engine `PerHitExplain` additive fields (napi → `importance`, `confidence`).
    pub importance: Option<f64>,
    pub confidence: Option<f64>,
}

impl PerHitExplain {
    fn from_rust(p: &RustPerHitExplain) -> Self {
        Self {
            id: p.id as i64,
            arm: match p.arm {
                SoftFallbackBranch::Vector => "vector".to_string(),
                SoftFallbackBranch::Text => "text".to_string(),
                SoftFallbackBranch::TextEdge => "text_edge".to_string(),
                SoftFallbackBranch::GraphArm => "graph_arm".to_string(),
            },
            vector_rank: p.vector_rank,
            text_rank: p.text_rank,
            graph_rank: p.graph_rank,
            fused_score: p.fused_score,
            ce_score: p.ce_score,
            blended: p.blended,
            importance: p.importance,
            confidence: p.confidence,
        }
    }
}

#[cfg(test)]
mod per_hit_explain_tests {
    use super::*;
    use fathomdb_engine::{Engine as EngEngine, PreparedWrite as EngPreparedWrite};

    // 0.8.16 Slice 5 / F9 (codex §9 fix-1, FINDING 2) — the N-API `PerHitExplain`
    // mirror must copy the new `importance`/`confidence` fields from the engine
    // type, or Node/TS `searchExplained` callers cannot observe the F9 contribution
    // (the 0.8.14 `embed_batch_cls` binding blind-spot). The engine `PerHitExplain`
    // is `#[non_exhaustive]`, so it cannot be built by literal cross-crate — the
    // source value comes from a REAL `search_explained` run (F9 reweight ON, graph
    // arm ON). Runs under `cargo test` (no maturin / node needed).
    #[test]
    fn from_rust_copies_f9_importance_and_confidence() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(format!("napi_f9_explain{}", fathomdb_schema::SQLITE_SUFFIX));
        let opened = EngEngine::open(&path).expect("open");
        let engine = &opened.engine;
        let receipt = engine
            .write(&[
                EngPreparedWrite::Node {
                    kind: "doc".to_string(),
                    body: "zephyr anchor entity".to_string(),
                    source_id: fathomdb_engine::SourceId::new("test:fixture")
                        .expect("test source id"),
                    logical_id: Some("zephyr".to_string()),
                    state: InitialState::Active,
                    reason: None,
                    valid_from: None,
                    valid_until: None,
                },
                EngPreparedWrite::Node {
                    kind: "doc".to_string(),
                    body: "beta reachable payload node".to_string(),
                    source_id: fathomdb_engine::SourceId::new("test:fixture")
                        .expect("test source id"),
                    logical_id: Some("beta".to_string()),
                    state: InitialState::Active,
                    reason: None,
                    valid_from: None,
                    valid_until: None,
                },
                EngPreparedWrite::Edge {
                    kind: "link".to_string(),
                    from: "zephyr".to_string(),
                    to: "beta".to_string(),
                    source_id: fathomdb_engine::SourceId::new("test:fixture")
                        .expect("test source id"),
                    logical_id: Some("e-zb".to_string()),
                    body: Some("collaboration record".to_string()),
                    t_valid: None,
                    t_invalid: None,
                    confidence: Some(0.90),
                    extractor_model_id: None,
                    temporal_fallback: None,
                },
            ])
            .expect("write");
        let beta_cursor = receipt.row_cursors[1];
        engine.write_node_importance(beta_cursor, 0.25).expect("set importance");
        engine.set_importance_reweight_enabled_for_test(true);

        let explained =
            engine.search_explained("zephyr", None, 0, true, 0.3, 0).expect("search_explained");
        let exp = explained.explanation.expect("explanation sidecar present");
        let entry = exp
            .per_hit
            .iter()
            .find(|p| p.id == beta_cursor)
            .expect("per_hit entry for the graph-reached beta node");
        // The engine populated both fields on the source explain entry.
        assert_eq!(entry.importance, Some(0.25), "source explain carries node importance");
        assert_eq!(entry.confidence, Some(0.90), "source explain carries edge confidence");

        // The binding mirror MUST copy them (the fix).
        let mirror = PerHitExplain::from_rust(entry);
        assert_eq!(mirror.importance, Some(0.25), "importance must propagate to the N-API mirror");
        assert_eq!(mirror.confidence, Some(0.90), "confidence must propagate to the N-API mirror");

        opened.engine.close().unwrap();
    }
}

/// 0.8.8 EXP-OBS (Slice 10) — the explanation sidecar (mirror of engine
/// `Explanation`): a query-level [`QueryTrace`] + a per-hit breakdown parallel to
/// (and in the same order as) `SearchResult.results`.
#[napi(object)]
pub struct Explanation {
    pub trace: QueryTrace,
    pub per_hit: Vec<PerHitExplain>,
}

impl Explanation {
    fn from_rust(e: &RustExplanation) -> Self {
        Self {
            trace: QueryTrace::from_rust(&e.trace),
            per_hit: e.per_hit.iter().map(PerHitExplain::from_rust).collect(),
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
    /// 0.8.18 Slice 5 (#5 vector-equivalence probe, R-VEQ-6) — `true` iff the
    /// open-time #5 self-check found a vector-equivalence divergence beyond the
    /// D4 floor and every vector-dependent arm now refuses at query time with a
    /// `FDB_VECTOR_EQUIVALENCE_MISMATCH` error. The text-only/FTS-only path
    /// (`searchTextOnly`) stays serviceable.
    pub dense_disabled: bool,
    /// R-VEQ-6 — human-readable reason for `denseDisabled`, or `null` when healthy.
    pub dense_disabled_reason: Option<String>,
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
            dense_disabled: r.dense_disabled,
            dense_disabled_reason: r.dense_disabled_reason.clone(),
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

    /// OPP-12 Phase-1 (0.8.19 Slice 10) — `transition` lifecycle verb. Thin
    /// pass-through: enforces the legal-transition table + `reason`
    /// clear-on-admit/set-on-exclude semantics (design §2/§3). Keys on the bare
    /// `logicalId` (`l:` only); a non-`l:` id → `NotLifecycleAddressableError`; an
    /// illegal move → `IllegalTransitionError { fromState, toState, legal }`.
    #[napi]
    pub async fn transition(
        &self,
        logical_id: String,
        to_state: String,
        reason: Option<String>,
    ) -> Result<()> {
        validate_ffi_string_napi(&logical_id)?;
        if let Some(r) = reason.as_deref() {
            validate_ffi_string_napi(r)?;
        }
        // Full LifecycleState vocabulary accepted so illegal targets surface a
        // typed IllegalTransitionError from the engine; only an unknown string is
        // rejected at the boundary.
        let to_state = RustLifecycleState::from_str_opt(&to_state).ok_or_else(|| {
            typed_error(
                CODE_INVALID_ARGUMENT,
                format!(
                    "unknown lifecycle state {to_state:?}: expected one of pending/active/deleted/purged"
                ),
                JsonValue::Null,
            )
        })?;
        let engine = Arc::clone(&self.inner);
        call_engine(move || engine.transition(&logical_id, to_state, reason)).await
    }

    /// OPP-12 Phase-1 (0.8.19 Slice 10) — `purge` lifecycle verb. Thin
    /// pass-through: deleted-first, idempotent hard-erase across every row-owned
    /// target (design §3). Keys on the bare `logicalId` (`l:` only); a non-`l:` id
    /// → `NotLifecycleAddressableError`; a non-`deleted` node →
    /// `IllegalTransitionError`.
    #[napi]
    pub async fn purge(&self, logical_id: String) -> Result<()> {
        validate_ffi_string_napi(&logical_id)?;
        let engine = Arc::clone(&self.inner);
        call_engine(move || engine.purge(&logical_id)).await
    }

    /// 0.8.20 Slice 5d (R-20-E4, design §4 item 9b) — `eraseSource` lifecycle
    /// verb. Deletes every canonical row carrying `sourceId`, plus its
    /// row-owned projections, and finishes the erasure at rest.
    ///
    /// The COMPANION to `purge`, not a duplicate: `purge` addresses a governed
    /// node by `logicalId`; `eraseSource` addresses ANONYMOUS content (rows
    /// with no `logicalId`) by its provenance, which `purge` cannot reach.
    /// Together they make every canonical row erasable from the SDK alone,
    /// with no CLI on `PATH`.
    ///
    /// Idempotent (an absent source is a zero-count success). Throws
    /// `WriteValidationError` for an empty, whitespace-only or reserved
    /// (`_`-prefixed) `sourceId`. NOT a recovery-denylist name — AC-041 holds.
    #[napi]
    pub async fn erase_source(&self, source_id: String) -> Result<EraseReport> {
        validate_ffi_string_napi(&source_id)?;
        let engine = Arc::clone(&self.inner);
        let report = call_engine(move || engine.erase_source(&source_id)).await?;
        Ok(EraseReport::from_rust(report))
    }

    /// 0.8.20 Slice 15d (R-20-PR / C-1) — the `configureProjections` governed
    /// verb. Declarative + idempotent: the engine diffs `specs` against the
    /// durable registry and backfills the difference. `drop` is EXPLICIT
    /// (omission never drops); a destructive change to a live projection without
    /// a drop throws a `FDB_PROJECTION_DESTRUCTIVE` error carrying `{name, delta}`.
    #[napi]
    pub async fn configure_projections(
        &self,
        specs: Vec<ProjectionSpec>,
        drop: Option<Vec<String>>,
    ) -> Result<ProjectionDelta> {
        let rust_specs: Vec<RustProjectionSpec> =
            specs.iter().map(ProjectionSpec::to_rust).collect::<Result<_>>()?;
        let drop = drop.unwrap_or_default();
        // AC-068a/b — the `drop` list is a caller-supplied FFI-string vector too;
        // validate each entry before the engine call, like the spec strings.
        for name in &drop {
            validate_ffi_string_napi(name)?;
        }
        let engine = Arc::clone(&self.inner);
        let delta = call_engine(move || engine.configure_projections(&rust_specs, &drop)).await?;
        Ok(ProjectionDelta::from_rust(&delta))
    }

    /// 0.8.20 Slice 15d (R-20-PR) — `read.projections` introspection. Returns
    /// every declared `ProjectionSpec` (sorted by name).
    #[napi]
    pub async fn read_projections(&self) -> Result<Vec<ProjectionSpec>> {
        let engine = Arc::clone(&self.inner);
        let specs = call_engine(move || engine.read_projections()).await?;
        Ok(specs.iter().map(ProjectionSpec::from_rust).collect())
    }

    #[napi]
    #[allow(clippy::too_many_arguments)]
    pub async fn search(
        &self,
        query: String,
        filter: Option<SearchFilterInput>,
        rerank_depth: Option<u32>,
        // 0.8.1 R3 (Slice 30) — when true, seed a BFS over temporal fact-edges
        // from the top-10 fused hits and fuse the reachable nodes as a third RRF arm.
        // Default false → byte-identical to the pre-Slice-30 two-arm pipeline.
        use_graph_arm: Option<bool>,
        // 0.8.5 (EXP-0) — CE-rerank knobs. `alpha` (default 0.3, clamped to [0,1] in
        // the engine) is the CE-blend weight; `poolN` (default = rerankDepth) is the
        // reranked-pool size. Omitting both reproduces the byte-identical default order.
        alpha: Option<f64>,
        pool_n: Option<u32>,
        // 0.8.8 EXP-OBS (Slice 10) — when true, populate `SearchResult.explanation`
        // with per-hit provenance + score breakdown + query trace. Default false
        // returns `explanation=null` and a byte-identical result (R-OBS-2 zero-cost).
        explain: Option<bool>,
        // 0.8.20 Slice 15b fix-2 (R-20-NV / R-20-RV) — optional validity view,
        // the same trailing options object the five read verbs take. Omitted /
        // `undefined` is the strict view: active-only, non-superseded, and valid
        // AT QUERY TIME. `{ includeOutOfWindow: true }` returns hits whatever
        // their window; `{ validAsOf: t }` evaluates validity at the bound
        // instant `t`. The existence flags are REFUSED here (typed
        // `FDB_INVALID_ARGUMENT`), never silently ignored.
        view: Option<ReadViewInput>,
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
        // 0.8.5 (D4): resolve the binding defaults — α=0.3, pool_n=rerankDepth — so an
        // unset call reproduces the pre-slice ranking. α is clamped in the engine.
        let alpha = alpha.unwrap_or(0.3);
        let pool_n = pool_n.map(|p| p as usize).unwrap_or(depth);
        // 0.8.8 EXP-OBS: explain=true routes to search_explained (same retrieval +
        // the sidecar); default stays on search_reranked (byte-identical).
        let explain = explain.unwrap_or(false);
        let view = read_view_or_default(view);
        let engine = Arc::clone(&self.inner);
        // fix-2: ONE call — `explain` is a parameter of the full-arity view entry
        // point, so the two arms can no longer drift on `view`.
        let result = call_engine(move || {
            engine.search_reranked_view(
                &query, filter, depth, graph_arm, alpha, pool_n, explain, &view,
            )
        })
        .await?;
        Ok(SearchResult::from_rust(result))
    }

    /// 0.8.18 Slice 5 (#5 vector-equivalence probe, R-VEQ-4) — the explicit
    /// text-only / FTS-only search path. Does NOT embed the query and NEVER raises
    /// `FDB_VECTOR_EQUIVALENCE_MISMATCH`, so it stays serviceable when the engine
    /// opened in the degraded `denseDisabled` state. Returns node-body FTS hits
    /// only (no vector recall, no CE rerank, no graph arm).
    #[napi]
    ///
    /// 0.8.20 Slice 15b fix-2 — takes the same optional `view` as `search`.
    pub async fn search_text_only(
        &self,
        query: String,
        view: Option<ReadViewInput>,
    ) -> Result<SearchResult> {
        validate_ffi_string_napi(&query)?;
        if query.trim().is_empty() {
            return Err(typed_error(
                CODE_WRITE_VALIDATION,
                "query must not be empty",
                JsonValue::Null,
            ));
        }
        let view = read_view_or_default(view);
        let engine = Arc::clone(&self.inner);
        let result = call_engine(move || engine.search_text_only_view(&query, &view)).await?;
        Ok(SearchResult::from_rust(result))
    }

    /// 0.8.18 Slice 5 (R-VEQ-6) — `true` iff the engine opened degraded (the #5
    /// self-check found a vector-equivalence divergence and every dense arm is
    /// refusing). Mirrors `OpenReport.denseDisabled`.
    #[napi]
    pub fn dense_disabled(&self) -> bool {
        self.inner.dense_disabled()
    }

    /// 0.8.18 Slice 5 (R-VEQ-6) — the human-readable reason for the degraded state,
    /// or `null` when dense is healthy.
    #[napi]
    pub fn dense_disabled_reason(&self) -> Option<String> {
        self.inner.dense_disabled_reason()
    }

    /// 0.8.18 Slice 5 (R-VEQ-6) — telemetry counter: query-time dense-arm refusals
    /// raised because the engine opened degraded.
    #[napi]
    pub fn vector_equivalence_refusal_count(&self) -> i64 {
        self.inner.vector_equivalence_refusal_count() as i64
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

    /// 0.8.8 Slice 15 (OPP-9) — enable opt-in local telemetry capture to a JSONL
    /// `sinkPath`. Off by default; local file only (no egress).
    #[napi]
    pub async fn enable_telemetry(&self, sink_path: String) -> Result<()> {
        validate_ffi_string_napi(&sink_path)?;
        let engine = Arc::clone(&self.inner);
        call_engine(move || engine.enable_telemetry(&sink_path)).await
    }

    /// 0.8.8 Slice 15 — the most-recent captured `queryId` (for `recordFeedback`),
    /// or `null` when telemetry is off / no query captured yet.
    #[napi]
    pub fn last_telemetry_query_id(&self) -> Option<String> {
        self.inner.last_telemetry_query_id()
    }

    /// 0.8.8 Slice 15 — attach agent relevance labels for a captured `queryId`.
    /// Ids are the positional `write_cursor` keys emitted in the telemetry
    /// `result_ids` array (the pre-0.8.19 `SearchHit.id` space), NOT the post-C-2
    /// typed `SearchHit.id`. Errors if telemetry is off.
    #[napi]
    pub async fn record_feedback(
        &self,
        query_id: String,
        relevant_ids: Vec<i64>,
        irrelevant_ids: Vec<i64>,
        label_source: String,
    ) -> Result<()> {
        validate_ffi_string_napi(&query_id)?;
        validate_ffi_string_napi(&label_source)?;
        // codex §9 [P2] (parity): ids are non-negative `u64` (the telemetry
        // `result_ids` / `write_cursor` key space). A direct napi caller bypassing
        // the TS wrapper could pass a negative `i64` which `as u64` would wrap to a
        // huge value; reject it here to match the TS/Python wrapper guards.
        let rel = checked_ids_napi("relevantIds", &relevant_ids)?;
        let irr = checked_ids_napi("irrelevantIds", &irrelevant_ids)?;
        let engine = Arc::clone(&self.inner);
        call_engine(move || engine.record_feedback(&query_id, &rel, &irr, &label_source)).await
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

    /// 0.8.12 Slice 15 (OPP-2) — BYO-LLM consolidation. `cmd` is the argv to
    /// spawn a caller-supplied harness speaking `fathomdb.consolidate.v1` (the
    /// SAME transport as extraction). `axes` is an array of objects with
    /// `subjectLogicalId` and `relation` string properties. FathomDB assembles
    /// each competing fact-edge cluster deterministically and applies the harness
    /// verdicts as supersession/recency metadata (bodies never rewritten).
    #[napi]
    pub async fn consolidate_with_provider(
        &self,
        cmd: Vec<String>,
        axes: Vec<JsonValue>,
    ) -> Result<ConsolidateReceipt> {
        let rust_axes: Vec<RustConsolidateAxis> = axes
            .iter()
            .map(|item| {
                let subject_logical_id = item
                    .get("subjectLogicalId")
                    .or_else(|| item.get("subject_logical_id"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        typed_error(
                            CODE_WRITE_VALIDATION,
                            "axis must have subjectLogicalId",
                            JsonValue::Null,
                        )
                    })?
                    .to_string();
                let relation = item
                    .get("relation")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        typed_error(
                            CODE_WRITE_VALIDATION,
                            "axis must have relation",
                            JsonValue::Null,
                        )
                    })?
                    .to_string();
                Ok(RustConsolidateAxis { subject_logical_id, relation })
            })
            .collect::<Result<_>>()?;

        let engine = Arc::clone(&self.inner);
        let receipt = call_engine(move || {
            let cmd_refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
            engine.consolidate_with_provider(&cmd_refs, &rust_axes)
        })
        .await?;
        Ok(ConsolidateReceipt::from_rust(receipt))
    }

    /// Embed `text` with the engine's pinned default embedder
    /// (`fathomdb-bge-small-en-v1.5`) and return the raw vector.
    ///
    /// Read-path primitive (mirror of the Python `Engine.embed`) for callers
    /// that need vectors under the engine's own embedder identity (e.g.
    /// coverage-index clustering) rather than a parallel, possibly-divergent
    /// embedder. Rejects with `FDB_EMBEDDER_NOT_CONFIGURED` if the engine was
    /// opened without an embedder (`useDefaultEmbedder: false`).
    #[napi]
    pub async fn embed(&self, text: String) -> Result<Vec<f64>> {
        let engine = Arc::clone(&self.inner);
        let vector = call_engine(move || engine.embed_text(&text)).await?;
        Ok(vector.into_iter().map(|x| x as f64).collect())
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
/// 0.8.20 Slice 10b (R-20-RV / R-20-NV) — the TypeScript face of `ReadView`.
///
/// Idiomatic `camelCase`; every field is optional and every one defaults to the
/// STRICT view, so omitting `view` entirely reproduces the shipped read
/// behaviour exactly.
///
/// World-time only — there is deliberately no `historyAsOf`.
#[napi(object)]
pub struct ReadViewInput {
    /// Relax `superseded_at IS NULL` — include historical versions.
    pub include_superseded: Option<bool>,
    /// Relax `state = 'active'` — include non-active lifecycle states.
    pub include_inactive: Option<bool>,
    /// Relax the validity window entirely (ignores `validAsOf`).
    pub include_out_of_window: Option<bool>,
    /// Validity instant, INTEGER epoch SECONDS. Omitted = now.
    pub valid_as_of: Option<i64>,
}

/// An omitted `view` means the strict default view.
fn read_view_or_default(view: Option<ReadViewInput>) -> RustReadView {
    match view {
        None => RustReadView::default(),
        Some(v) => RustReadView {
            include_superseded: v.include_superseded.unwrap_or(false),
            include_inactive: v.include_inactive.unwrap_or(false),
            include_out_of_window: v.include_out_of_window.unwrap_or(false),
            valid_as_of: v.valid_as_of,
        },
    }
}

/// 0.8.20 Slice 10b (R-20-NV) — the TypeScript face of `BoundaryCrossing`.
#[napi(object)]
pub struct BoundaryCrossing {
    /// The node that crossed a validity boundary.
    pub node: NodeRecord,
    /// Set when the node BECAME VALID inside the interrogated interval.
    pub became_valid_at: Option<i64>,
    /// Set when the node BECAME INVALID inside the interrogated interval.
    pub became_invalid_at: Option<i64>,
}

impl BoundaryCrossing {
    fn from_rust(c: &RustBoundaryCrossing) -> Self {
        Self {
            node: NodeRecord::from_rust(&c.node),
            became_valid_at: c.became_valid_at,
            became_invalid_at: c.became_invalid_at,
        }
    }
}

#[napi(object)]
pub struct ReadCollectionOptions {
    pub after_id: Option<i64>,
    pub limit: i64,
}

#[napi(js_name = "readGet")]
pub async fn read_get(
    engine: &Engine,
    logical_id: String,
    view: Option<ReadViewInput>,
) -> Result<Option<NodeRecord>> {
    validate_ffi_string_napi(&logical_id)?;
    let view = read_view_or_default(view);
    let inner = Arc::clone(&engine.inner);
    let record = call_engine(move || inner.read_get(&logical_id, &view)).await?;
    Ok(record.as_ref().map(NodeRecord::from_rust))
}

#[napi(js_name = "readGetMany")]
pub async fn read_get_many(
    engine: &Engine,
    logical_ids: Vec<String>,
    view: Option<ReadViewInput>,
) -> Result<Vec<Option<NodeRecord>>> {
    for id in &logical_ids {
        validate_ffi_string_napi(id)?;
    }
    let view = read_view_or_default(view);
    let inner = Arc::clone(&engine.inner);
    let rows = call_engine(move || inner.read_get_many(&logical_ids, &view)).await?;
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
    view: Option<ReadViewInput>,
) -> Result<Vec<NodeRecord>> {
    validate_ffi_string_napi(&kind)?;
    let mut rust_predicates: Vec<RustPredicate> = Vec::new();
    if let Some(plist) = predicates {
        for pred in plist {
            rust_predicates.push(napi_predicate_to_rust(pred)?);
        }
    }
    let limit = limit.unwrap_or(100).max(0) as usize;
    let view = read_view_or_default(view);
    let inner = Arc::clone(&engine.inner);
    let rows = call_engine(move || inner.read_list(&kind, &rust_predicates, limit, &view)).await?;
    Ok(rows.iter().map(NodeRecord::from_rust).collect())
}

/// 0.8.11 Slice 40 (#17) — one term of the unified `Filter` grammar. `term` ∈
/// `{"source_type","kind","created_after","status","json"}`. For the four
/// shorthand terms set `valueStr`/`valueInt`; for `json` set `predicate`.
#[napi(object)]
pub struct FilterTermInput {
    /// Discriminator: source_type | kind | created_after | status | json.
    pub term: String,
    /// String value for source_type/kind/status terms.
    pub value_str: Option<String>,
    /// Integer value for the created_after term (unix seconds).
    pub value_int: Option<i64>,
    /// The G4 predicate for a `json` term.
    pub predicate: Option<PredicateInput>,
}

fn napi_filter_term_to_rust(term: FilterTermInput) -> Result<RustFilterTerm> {
    match term.term.as_str() {
        "source_type" => term.value_str.map(RustFilterTerm::SourceType).ok_or_else(|| {
            typed_error(CODE_INVALID_FILTER, "source_type term requires valueStr", JsonValue::Null)
        }),
        "kind" => term.value_str.map(RustFilterTerm::Kind).ok_or_else(|| {
            typed_error(CODE_INVALID_FILTER, "kind term requires valueStr", JsonValue::Null)
        }),
        "created_after" => term.value_int.map(RustFilterTerm::CreatedAfter).ok_or_else(|| {
            typed_error(CODE_INVALID_FILTER, "created_after term requires valueInt", JsonValue::Null)
        }),
        "status" => term.value_str.map(RustFilterTerm::Status).ok_or_else(|| {
            typed_error(CODE_INVALID_FILTER, "status term requires valueStr", JsonValue::Null)
        }),
        "json" => {
            let pred = term.predicate.ok_or_else(|| {
                typed_error(CODE_INVALID_FILTER, "json term requires predicate", JsonValue::Null)
            })?;
            Ok(RustFilterTerm::Json(napi_predicate_to_rust(pred)?))
        }
        other => Err(typed_error(
            CODE_INVALID_FILTER,
            format!("unknown filter term '{other}'; expected source_type/kind/created_after/status/json"),
            JsonValue::Null,
        )),
    }
}

/// 0.8.11 Slice 40 (#17) — unified `Filter` → `read.list` backend. The engine
/// performs the authoritative total dispatch (Json `json_extract`;
/// SourceType/Kind constant-fold vs the partition kind).
#[napi(js_name = "readListFilter")]
pub async fn read_list_filter(
    engine: &Engine,
    kind: String,
    terms: Option<Vec<FilterTermInput>>,
    limit: Option<i64>,
    view: Option<ReadViewInput>,
) -> Result<Vec<NodeRecord>> {
    validate_ffi_string_napi(&kind)?;
    let mut rust_terms: Vec<RustFilterTerm> = Vec::new();
    if let Some(tlist) = terms {
        for t in tlist {
            rust_terms.push(napi_filter_term_to_rust(t)?);
        }
    }
    let filter = RustFilter { terms: rust_terms };
    let limit = limit.unwrap_or(100).max(0) as usize;
    let inner = Arc::clone(&engine.inner);
    let view = read_view_or_default(view);
    let rows = call_engine(move || inner.read_list_filter(&kind, &filter, limit, &view)).await?;
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
    view: Option<ReadViewInput>,
) -> Result<Vec<NodeRecord>> {
    validate_ffi_string_napi(&logical_id)?;
    let dir = parse_direction_napi(&direction)?;
    let view = read_view_or_default(view);
    let inner = Arc::clone(&engine.inner);
    let nodes = call_engine(move || inner.graph_neighbors(&logical_id, depth, dir, &view)).await?;
    Ok(nodes.iter().map(NodeRecord::from_rust).collect())
}

/// 0.8.20 Slice 10b (R-20-NV) — nodes that crossed a validity boundary in
/// `(since, view-instant]`. `since` is INTEGER epoch SECONDS.
#[napi(js_name = "crossedBoundarySince")]
pub async fn crossed_boundary_since(
    engine: &Engine,
    since: i64,
    view: Option<ReadViewInput>,
) -> Result<Vec<BoundaryCrossing>> {
    let view = read_view_or_default(view);
    let inner = Arc::clone(&engine.inner);
    let rows = call_engine(move || inner.crossed_boundary_since(since, &view)).await?;
    Ok(rows.iter().map(BoundaryCrossing::from_rust).collect())
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

/// 0.8.20 Slice 5c (R-20-E3) — `sourceId` is now MANDATORY on every canonical
/// write. Rust makes its absence inexpressible via the `SourceId` newtype;
/// TypeScript has no such guarantee at the N-API boundary, so the binding throws
/// a typed write-validation error for a missing, empty or reserved
/// (`_`-prefixed) id. This is the TS arm of "an un-provenanced write does not
/// compile / raises", and it mirrors the Python binding exactly.
///
/// The rationale is not tidiness: `excise_source` addresses rows BY `source_id`,
/// so a row written without one is reachable by no erasure call — un-erasable.
fn json_source_id_required(item: &JsonValue, kind: &str) -> Result<SourceId> {
    let raw = json_str_alt(item, "sourceId", "source_id")?.ok_or_else(|| {
        typed_error(
            CODE_WRITE_VALIDATION,
            format!(
                "{kind} write item missing required field \"sourceId\": provenance is mandatory \
                 since 0.8.20 — a row written without it can never be erased by excise_source"
            ),
            JsonValue::Null,
        )
    })?;
    SourceId::new(raw).map_err(|_| {
        typed_error(
            CODE_WRITE_VALIDATION,
            "\"sourceId\" must be a non-empty identifier outside the engine's reserved \
             \"_\"-prefixed namespace"
                .to_string(),
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
    let source_id = json_source_id_required(item, "node")?;
    let logical_id = json_str_alt(item, "logicalId", "logical_id")?;
    // OPP-12 Phase-1 (0.8.19 Slice 5) — create-time existence state + advisory
    // reason (X1 parity with the pyo3 binding). `state` defaults to `active`; an
    // out-of-subset value (`deleted`/`purged`/unknown) is a TYPED write-validation
    // rejection — you cannot CREATE a deleted/purged node. Thin pass-through.
    let state = match json_str_alt(item, "state", "state")? {
        Some(s) => InitialState::from_create_str(&s).ok_or_else(|| {
            typed_error(
                CODE_WRITE_VALIDATION,
                format!(
                    "cannot create a node with state {s:?}: only \"pending\" or \"active\" are creatable (deleted/purged require transition/purge)"
                ),
                JsonValue::Null,
            )
        })?,
        None => InitialState::Active,
    };
    let reason = json_str_alt(item, "reason", "reason")?;
    // 0.8.20 Slice 15b (TC-34) — world-time validity window (X1 parity with the
    // pyo3 binding). INTEGER epoch seconds; absent or `null` means unbounded on
    // that side, which lands NULL and reproduces pre-slice behaviour exactly.
    // Both spellings are accepted, exactly as `tValid`/`t_valid` are on edges.
    // The half-open pair is validated in the ENGINE (`validate_write`), so Rust,
    // Python and TypeScript share one rule and cannot drift.
    let valid_from = json_i64_alt(item, "validFrom", "valid_from")?;
    let valid_until = json_i64_alt(item, "validUntil", "valid_until")?;
    Ok(PreparedWrite::Node {
        kind,
        body,
        source_id,
        logical_id,
        state,
        reason,
        valid_from,
        valid_until,
    })
}

/// 0.8.20 Slice 15b (TC-34) — read an optional INTEGER epoch-second field.
///
/// JavaScript has ONE number type, so `10.5` and `true` both arrive where an
/// integer was meant. Both are refused with a typed write-validation error
/// rather than truncated or coerced: a silently truncated instant is a wrong
/// answer that only surfaces at the window boundary. `serde_json`'s `as_i64`
/// returns `None` for any non-integral number, which is exactly the test wanted.
fn json_i64(v: &JsonValue, key: &str) -> Result<Option<i64>> {
    match json_get(v, key) {
        Some(JsonValue::Null) | None => Ok(None),
        Some(JsonValue::Number(n)) => n.as_i64().map(Some).ok_or_else(|| {
            typed_error(
                CODE_WRITE_VALIDATION,
                format!("field {key:?} must be an integer (epoch seconds), not {n}"),
                JsonValue::Null,
            )
        }),
        Some(_other) => Err(typed_error(
            CODE_WRITE_VALIDATION,
            format!("field {key:?} must be an integer (epoch seconds) or null"),
            JsonValue::Null,
        )),
    }
}

/// The `json_str_alt` analogue for integers: accept the camelCase spelling
/// first, then the snake_case one, so a caller porting from the Python stub
/// keeps working. See [`json_str_alt`].
fn json_i64_alt(item: &JsonValue, camel: &str, snake: &str) -> Result<Option<i64>> {
    if let Some(v) = json_i64(item, camel)? {
        return Ok(Some(v));
    }
    json_i64(item, snake)
}

fn translate_edge(item: &JsonValue) -> Result<PreparedWrite> {
    let kind = json_str_required(item, "kind")?;
    let from = json_str_required(item, "from")?;
    let to = json_str_required(item, "to")?;
    let source_id = json_source_id_required(item, "edge")?;
    let logical_id = json_str_alt(item, "logicalId", "logical_id")?;
    // Edge body (the relation text) — optional. Projected into `search_index_edges`
    // so the C1 graph arm can seed from edge-fact FTS (`source A`). NULL = not indexed.
    let body = json_serialised(item, "body")?;
    // R3 (Slice 30) — temporal validity fields accepted from user-facing write API.
    //
    // TC-33 (HITL-RATIFIED 2026-07-21) — `tValid`/`t_valid` and
    // `tInvalid`/`t_invalid` are **INTEGER epoch seconds (UTC)**, not ISO-8601
    // strings. This is the GOVERNED SDK WRITE SURFACE, which carries the same
    // representation as storage; ISO-8601 survives ONLY on the BYO-LLM extractor
    // wire, where the engine normalises it with hard rejection. Reuses the same
    // `json_i64_alt` helper as the node `validFrom`/`validUntil` window, so both
    // temporal axes validate identically — and unlike the old `json_str_alt`
    // (which did NO format validation at all) a wrong-typed value is rejected.
    //
    // `None` = "still valid"; that semantic is load-bearing and unchanged.
    let t_valid = json_i64_alt(item, "tValid", "t_valid")?;
    let t_invalid = json_i64_alt(item, "tInvalid", "t_invalid")?;
    Ok(PreparedWrite::Edge {
        kind,
        from,
        to,
        source_id,
        logical_id,
        body,
        t_valid,
        t_invalid,
        confidence: None,
        extractor_model_id: None,
        temporal_fallback: None,
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
