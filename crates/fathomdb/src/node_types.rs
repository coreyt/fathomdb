#![cfg(feature = "node")]

use napi::{Error, Result, Status};

use crate::{EmbedderChoice, EngineError, ProjectionTarget, ProvenanceMode, TelemetryLevel};
use fathomdb_query::CompileError as RustCompileError;

pub(crate) const MAX_AST_JSON_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_WRITE_JSON_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_REQUEST_JSON_BYTES: usize = 1024 * 1024;

const ERROR_PREFIX: &str = "FATHOMDB_";

#[derive(Clone, Copy)]
pub(crate) enum ErrorCode {
    Sqlite,
    Schema,
    InvalidWrite,
    CapabilityMissing,
    WriterRejected,
    WriterTimedOut,
    Bridge,
    DatabaseLocked,
    InvalidConfig,
    Io,
    Compile,
    InvalidArgument,
}

impl ErrorCode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Sqlite => "SQLITE_ERROR",
            Self::Schema => "SCHEMA_ERROR",
            Self::InvalidWrite => "INVALID_WRITE",
            Self::CapabilityMissing => "CAPABILITY_MISSING",
            Self::WriterRejected => "WRITER_REJECTED",
            Self::WriterTimedOut => "WRITER_TIMED_OUT",
            Self::Bridge => "BRIDGE_ERROR",
            Self::DatabaseLocked => "DATABASE_LOCKED",
            Self::InvalidConfig => "INVALID_CONFIG",
            Self::Io => "IO_ERROR",
            Self::Compile => "COMPILE_ERROR",
            Self::InvalidArgument => "INVALID_ARGUMENT",
        }
    }
}

pub(crate) fn napi_error(code: ErrorCode, message: impl Into<String>) -> Error {
    Error::new(
        Status::GenericFailure,
        format!("{ERROR_PREFIX}{}::{}", code.as_str(), message.into()),
    )
}

pub(crate) fn invalid_argument(message: impl Into<String>) -> Error {
    napi_error(ErrorCode::InvalidArgument, message)
}

pub(crate) fn check_json_size(json: &str, max_bytes: usize, label: &str) -> Result<()> {
    if json.len() > max_bytes {
        return Err(invalid_argument(format!(
            "{label} JSON exceeds maximum size of {max_bytes} bytes"
        )));
    }
    Ok(())
}

pub(crate) fn parse_provenance_mode(mode: &str) -> Result<ProvenanceMode> {
    match mode {
        "warn" => Ok(ProvenanceMode::Warn),
        "require" => Ok(ProvenanceMode::Require),
        other => Err(invalid_argument(format!("invalid provenanceMode: {other}"))),
    }
}

pub(crate) fn parse_telemetry_level(level: Option<&str>) -> Result<TelemetryLevel> {
    match level {
        None | Some("counters") => Ok(TelemetryLevel::Counters),
        Some("statements") => Ok(TelemetryLevel::Statements),
        Some("profiling") => Ok(TelemetryLevel::Profiling),
        Some(other) => Err(invalid_argument(format!(
            "invalid telemetryLevel: {other} (expected counters, statements, or profiling)"
        ))),
    }
}

pub(crate) fn parse_embedder_choice(value: Option<&str>) -> Result<EmbedderChoice> {
    match value {
        None | Some("none") => Ok(EmbedderChoice::None),
        Some("builtin") => Ok(EmbedderChoice::Builtin),
        Some(other) => Err(invalid_argument(format!(
            "invalid embedder: {other} (expected none or builtin)"
        ))),
    }
}

pub(crate) fn parse_projection_target(target: &str) -> Result<ProjectionTarget> {
    match target {
        "fts" => Ok(ProjectionTarget::Fts),
        "vec" => Ok(ProjectionTarget::Vec),
        "all" => Ok(ProjectionTarget::All),
        other => Err(invalid_argument(format!(
            "invalid projection target: {other}"
        ))),
    }
}

pub(crate) fn encode_json<T: serde::Serialize>(value: T) -> Result<String> {
    serde_json::to_string(&value)
        .map_err(|error| invalid_argument(format!("failed to serialize payload: {error}")))
}

// Used as `.map_err(map_compile_error)` at napi call sites, so it must
// take the error by value even though `to_string` only needs `&self`.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn map_compile_error(error: RustCompileError) -> Error {
    napi_error(ErrorCode::Compile, error.to_string())
}

pub(crate) fn map_admin_ffi_error(error: crate::admin_ffi::AdminFfiError) -> Error {
    use crate::admin_ffi::AdminFfiError;
    match error {
        AdminFfiError::Parse(err) => {
            napi_error(ErrorCode::Bridge, format!("admin request parse: {err}"))
        }
        AdminFfiError::Engine(err) => map_engine_error(err),
        AdminFfiError::Serialize(err) => napi_error(
            ErrorCode::Bridge,
            format!("admin response serialize: {err}"),
        ),
    }
}

pub(crate) fn map_search_ffi_error(error: crate::search_ffi::SearchFfiError) -> Error {
    use crate::search_ffi::SearchFfiError;
    match error {
        SearchFfiError::Parse(err) => {
            napi_error(ErrorCode::Bridge, format!("search request parse: {err}"))
        }
        SearchFfiError::Compile(err) => napi_error(ErrorCode::Compile, format!("{err:?}")),
        SearchFfiError::Engine(err) => map_engine_error(err),
        SearchFfiError::Serialize(err) => napi_error(
            ErrorCode::Bridge,
            format!("search response serialize: {err}"),
        ),
    }
}

pub(crate) fn map_engine_error(error: EngineError) -> Error {
    match error {
        EngineError::Sqlite(error) => napi_error(ErrorCode::Sqlite, error.to_string()),
        EngineError::Schema(error) => napi_error(ErrorCode::Schema, error.to_string()),
        EngineError::Io(error) => napi_error(ErrorCode::Io, error.to_string()),
        EngineError::WriterRejected(message) => napi_error(ErrorCode::WriterRejected, message),
        EngineError::WriterTimedOut(message) => napi_error(ErrorCode::WriterTimedOut, message),
        EngineError::InvalidWrite(message) => napi_error(ErrorCode::InvalidWrite, message),
        EngineError::InvalidConfig(message) => napi_error(ErrorCode::InvalidConfig, message),
        EngineError::Bridge(message) => napi_error(ErrorCode::Bridge, message),
        EngineError::CapabilityMissing(message) => {
            napi_error(ErrorCode::CapabilityMissing, message)
        }
        EngineError::DatabaseLocked(message) => napi_error(ErrorCode::DatabaseLocked, message),
    }
}
