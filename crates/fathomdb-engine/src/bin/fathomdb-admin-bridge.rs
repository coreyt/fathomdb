use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::Arc;

use fathomdb_engine::{
    AdminService, EngineError, ProjectionTarget, SafeExportOptions, VectorGeneratorPolicy,
    load_vector_regeneration_config,
};
use fathomdb_schema::{SchemaError, SchemaManager};
use serde::{Deserialize, Serialize};
use serde_json::json;

const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum BridgeErrorCode {
    BadRequest,
    UnsupportedCommand,
    UnsupportedCapability,
    IntegrityFailure,
    ExecutionFailure,
}

#[derive(Debug, Deserialize)]
struct BridgeRequest {
    #[serde(default)]
    protocol_version: u32,
    database_path: PathBuf,
    command: String,
    target: Option<String>,
    source_ref: Option<String>,
    destination_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
    vector_generator_policy: Option<VectorGeneratorPolicy>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BridgeCommand {
    CheckIntegrity,
    CheckSemantics,
    RebuildProjections,
    RebuildMissingProjections,
    RestoreVectorProfiles,
    RegenerateVectorEmbeddings,
    TraceSource,
    ExciseSource,
    SafeExport,
}

#[derive(Debug, Serialize)]
struct BridgeResponse {
    protocol_version: u32,
    ok: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<BridgeErrorCode>,
    payload: serde_json::Value,
}

#[allow(clippy::too_many_lines, clippy::print_stdout, clippy::expect_used)]
fn main() {
    let mut stdin = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut stdin) {
        emit_error(
            BridgeErrorCode::BadRequest,
            format!("failed to read stdin: {error}"),
        );
        return;
    }

    let response = handle_request_body(&stdin);
    println!(
        "{}",
        serde_json::to_string(&response).expect("bridge response serializes")
    );
}

fn handle_request_body(stdin: &str) -> BridgeResponse {
    let request: BridgeRequest = match serde_json::from_str(stdin) {
        Ok(request) => request,
        Err(error) => {
            return error_response_with_message(
                classify_parse_error(&error),
                format!("invalid request: {error}"),
            );
        }
    };

    if request.protocol_version != PROTOCOL_VERSION {
        return error_response_with_message(
            BridgeErrorCode::BadRequest,
            format!(
                "unsupported protocol version: expected {PROTOCOL_VERSION}, got {}",
                request.protocol_version
            ),
        );
    }

    handle_request(request)
}

fn handle_request(request: BridgeRequest) -> BridgeResponse {
    let service = AdminService::new(&request.database_path, Arc::new(SchemaManager::new()));
    let command = match parse_command(&request.command) {
        Ok(cmd) => cmd,
        Err(code) => {
            return error_response_with_message(
                code,
                format!("unsupported command: {}", request.command),
            );
        }
    };
    match command {
        BridgeCommand::CheckIntegrity => match service.check_integrity() {
            Ok(report) => success_response(
                "integrity check completed".to_owned(),
                serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            ),
            Err(error) => error_response(error, BridgeErrorCode::IntegrityFailure),
        },
        BridgeCommand::CheckSemantics => match service.check_semantics() {
            Ok(report) => success_response(
                "semantics check completed".to_owned(),
                serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            ),
            Err(error) => error_response(error, BridgeErrorCode::IntegrityFailure),
        },
        BridgeCommand::RebuildProjections => match parse_target(request.target.as_deref()) {
            Ok(target) => match service.rebuild_projections(target) {
                Ok(report) => success_response(
                    "projection rebuild completed".to_owned(),
                    serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                ),
                Err(error) => error_response(error, BridgeErrorCode::ExecutionFailure),
            },
            Err(code) => error_response_with_message(
                code,
                "invalid projection target: expected fts, vec, or all".to_owned(),
            ),
        },
        BridgeCommand::RebuildMissingProjections => match service.rebuild_missing_projections() {
            Ok(report) => success_response(
                "missing projection rebuild completed".to_owned(),
                serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            ),
            Err(error) => error_response(error, BridgeErrorCode::ExecutionFailure),
        },
        BridgeCommand::RestoreVectorProfiles => match service.restore_vector_profiles() {
            Ok(report) => success_response(
                "vector profiles restored".to_owned(),
                serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            ),
            Err(error) => error_response(error, BridgeErrorCode::ExecutionFailure),
        },
        BridgeCommand::RegenerateVectorEmbeddings => match request.config_path {
            Some(config_path) => match load_vector_regeneration_config(&config_path) {
                Ok(config) => match service.regenerate_vector_embeddings_with_policy(
                    &config,
                    &request.vector_generator_policy.unwrap_or_default(),
                ) {
                    Ok(report) => success_response(
                        "vector embeddings regenerated".to_owned(),
                        serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                    ),
                    Err(error) => error_response(error, BridgeErrorCode::ExecutionFailure),
                },
                Err(error) => error_response(error, BridgeErrorCode::BadRequest),
            },
            None => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "config_path is required".to_owned(),
            ),
        },
        // Security fix M-10: Require source_ref for TraceSource and ExciseSource
        // instead of silently defaulting to "". An empty source_ref could cause
        // unintended broad operations.
        BridgeCommand::TraceSource => match request.source_ref.as_deref() {
            Some(source_ref) if !source_ref.is_empty() => match service.trace_source(source_ref) {
                Ok(report) => success_response(
                    "trace completed".to_owned(),
                    serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                ),
                Err(error) => error_response(error, BridgeErrorCode::ExecutionFailure),
            },
            _ => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "source_ref is required for trace_source".to_owned(),
            ),
        },
        BridgeCommand::ExciseSource => match request.source_ref.as_deref() {
            Some(source_ref) if !source_ref.is_empty() => match service.excise_source(source_ref) {
                Ok(report) => success_response(
                    "source excised".to_owned(),
                    serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                ),
                Err(error) => error_response(error, BridgeErrorCode::ExecutionFailure),
            },
            _ => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "source_ref is required for excise_source".to_owned(),
            ),
        },
        BridgeCommand::SafeExport => match request.destination_path {
            Some(destination) => {
                match service.safe_export(destination, SafeExportOptions::default()) {
                    Ok(manifest) => success_response(
                        "export created".to_owned(),
                        // SafeExportManifest contains only primitive types; serialization cannot fail.
                        serde_json::to_value(&manifest).unwrap_or_else(|_| {
                            unreachable!("SafeExportManifest serialization is infallible")
                        }),
                    ),
                    Err(error) => error_response(error, BridgeErrorCode::ExecutionFailure),
                }
            }
            None => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "destination_path is required".to_owned(),
            ),
        },
    }
}

fn parse_target(target: Option<&str>) -> Result<ProjectionTarget, BridgeErrorCode> {
    match target.unwrap_or("all") {
        "fts" => Ok(ProjectionTarget::Fts),
        "vec" => Ok(ProjectionTarget::Vec),
        "all" => Ok(ProjectionTarget::All),
        _ => Err(BridgeErrorCode::BadRequest),
    }
}

fn parse_command(command: &str) -> Result<BridgeCommand, BridgeErrorCode> {
    match command {
        "check_integrity" => Ok(BridgeCommand::CheckIntegrity),
        "check_semantics" => Ok(BridgeCommand::CheckSemantics),
        "rebuild_projections" => Ok(BridgeCommand::RebuildProjections),
        "rebuild_missing_projections" => Ok(BridgeCommand::RebuildMissingProjections),
        "restore_vector_profiles" => Ok(BridgeCommand::RestoreVectorProfiles),
        "regenerate_vector_embeddings" => Ok(BridgeCommand::RegenerateVectorEmbeddings),
        "trace_source" => Ok(BridgeCommand::TraceSource),
        "excise_source" => Ok(BridgeCommand::ExciseSource),
        "safe_export" => Ok(BridgeCommand::SafeExport),
        _ => Err(BridgeErrorCode::UnsupportedCommand),
    }
}

fn classify_parse_error(_error: &serde_json::Error) -> BridgeErrorCode {
    BridgeErrorCode::BadRequest
}

fn classify_engine_error(error: &EngineError, default: BridgeErrorCode) -> BridgeErrorCode {
    match error {
        EngineError::CapabilityMissing(_) => BridgeErrorCode::UnsupportedCapability,
        EngineError::Schema(SchemaError::MissingCapability(_)) => {
            BridgeErrorCode::UnsupportedCapability
        }
        EngineError::InvalidWrite(_) => BridgeErrorCode::BadRequest,
        _ => default,
    }
}

/// Security fix M-4: Sanitize error messages to avoid leaking internal paths,
/// schema details, or system configuration in bridge responses. The full error
/// is printed to stderr for operator debugging.
fn error_response(error: EngineError, default_code: BridgeErrorCode) -> BridgeResponse {
    let code = classify_engine_error(&error, default_code);
    #[allow(clippy::print_stderr)]
    {
        eprintln!("[bridge] error: {error}");
    }
    error_response_with_message(
        code,
        "internal error; check bridge stderr for details".to_owned(),
    )
}

fn success_response(message: String, payload: serde_json::Value) -> BridgeResponse {
    BridgeResponse {
        protocol_version: PROTOCOL_VERSION,
        ok: true,
        message,
        error_code: None,
        payload,
    }
}

fn error_response_with_message(code: BridgeErrorCode, message: String) -> BridgeResponse {
    BridgeResponse {
        protocol_version: PROTOCOL_VERSION,
        ok: false,
        message,
        error_code: Some(code),
        payload: json!({}),
    }
}

#[allow(clippy::print_stdout, clippy::expect_used)]
fn emit_error(code: BridgeErrorCode, message: String) {
    let response = error_response_with_message(code, message);
    println!(
        "{}",
        serde_json::to_string(&response).expect("bridge response serializes")
    );
}

#[cfg(test)]
mod tests {
    use super::{
        BridgeErrorCode, classify_engine_error, handle_request_body, parse_command, parse_target,
    };
    use fathomdb_engine::{EngineError, ProjectionTarget};
    use fathomdb_schema::SchemaError;

    #[test]
    fn parse_command_reports_unsupported_command() {
        let result = parse_command("does_not_exist");
        assert_eq!(result.err(), Some(BridgeErrorCode::UnsupportedCommand));
    }

    #[test]
    fn parse_target_defaults_to_all_when_omitted() {
        assert_eq!(parse_target(None), Ok(ProjectionTarget::All));
    }

    #[test]
    fn parse_target_reports_bad_request_for_invalid_value() {
        let result = parse_target(Some("weird"));
        assert_eq!(result.err(), Some(BridgeErrorCode::BadRequest));
    }

    #[test]
    fn classify_engine_error_maps_capability_missing() {
        let code = classify_engine_error(
            &EngineError::CapabilityMissing("sqlite-vec unavailable".to_owned()),
            BridgeErrorCode::ExecutionFailure,
        );
        assert_eq!(code, BridgeErrorCode::UnsupportedCapability);
    }

    #[test]
    fn classify_engine_error_maps_schema_missing_capability() {
        let code = classify_engine_error(
            &EngineError::Schema(SchemaError::MissingCapability("sqlite-vec")),
            BridgeErrorCode::ExecutionFailure,
        );
        assert_eq!(code, BridgeErrorCode::UnsupportedCapability);
    }

    #[test]
    fn classify_engine_error_preserves_default_for_schema_failures() {
        let code = classify_engine_error(
            &EngineError::Schema(SchemaError::Sqlite(rusqlite::Error::InvalidQuery)),
            BridgeErrorCode::IntegrityFailure,
        );
        assert_eq!(code, BridgeErrorCode::IntegrityFailure);
    }

    #[test]
    fn handle_request_body_rejects_malformed_json() {
        let response = handle_request_body("{");
        assert!(!response.ok);
        assert_eq!(response.error_code, Some(BridgeErrorCode::BadRequest));
        assert!(response.message.contains("invalid request"));
    }

    #[test]
    fn handle_request_body_rejects_unsupported_protocol_version() {
        let response = handle_request_body(
            r#"{"protocol_version":99,"database_path":"/tmp/fathom.db","command":"check_integrity"}"#,
        );
        assert!(!response.ok);
        assert_eq!(response.error_code, Some(BridgeErrorCode::BadRequest));
        assert!(response.message.contains("unsupported protocol version"));
    }

    #[test]
    fn handle_request_body_rejects_missing_command_field() {
        let response =
            handle_request_body(r#"{"protocol_version":1,"database_path":"/tmp/fathom.db"}"#);
        assert!(!response.ok);
        assert_eq!(response.error_code, Some(BridgeErrorCode::BadRequest));
        assert!(response.message.contains("invalid request"));
    }

    #[test]
    fn handle_request_body_rejects_invalid_projection_target() {
        let response = handle_request_body(
            r#"{"protocol_version":1,"database_path":"/tmp/fathom.db","command":"rebuild_projections","target":"weird"}"#,
        );
        assert!(!response.ok);
        assert_eq!(response.error_code, Some(BridgeErrorCode::BadRequest));
        assert!(response.message.contains("invalid projection target"));
    }
}
