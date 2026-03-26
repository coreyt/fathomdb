use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::Arc;

use fathomdb_engine::{AdminService, ProjectionTarget, SafeExportOptions};
use fathomdb_schema::SchemaManager;
use serde::{Deserialize, Serialize};
use serde_json::json;

const PROTOCOL_VERSION: u32 = 1;
const ERROR_BAD_REQUEST: &str = "bad_request";
const ERROR_UNSUPPORTED_COMMAND: &str = "unsupported_command";
const ERROR_INTEGRITY_FAILURE: &str = "integrity_failure";
const ERROR_EXECUTION_FAILURE: &str = "execution_failure";

#[derive(Debug, Deserialize)]
struct BridgeRequest {
    #[serde(default)]
    protocol_version: u32,
    database_path: PathBuf,
    command: String,
    target: Option<String>,
    source_ref: Option<String>,
    destination_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BridgeCommand {
    CheckIntegrity,
    CheckSemantics,
    RebuildProjections,
    RebuildMissingProjections,
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
    error_code: Option<&'static str>,
    payload: serde_json::Value,
}

#[allow(clippy::too_many_lines, clippy::print_stdout, clippy::expect_used)]
fn main() {
    let mut stdin = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut stdin) {
        emit_error(ERROR_BAD_REQUEST, format!("failed to read stdin: {error}"));
        return;
    }

    let request: BridgeRequest = match serde_json::from_str(&stdin) {
        Ok(request) => request,
        Err(error) => {
            emit_error(
                classify_parse_error(&error),
                format!("invalid request: {error}"),
            );
            return;
        }
    };

    if request.protocol_version != PROTOCOL_VERSION {
        emit_error(
            ERROR_BAD_REQUEST,
            format!(
                "unsupported protocol version: expected {PROTOCOL_VERSION}, got {}",
                request.protocol_version
            ),
        );
        return;
    }

    let service = AdminService::new(&request.database_path, Arc::new(SchemaManager::new()));
    let command = match parse_command(&request.command) {
        Ok(cmd) => cmd,
        Err(code) => {
            emit_error(code, format!("unsupported command: {}", request.command));
            return;
        }
    };
    let response = match command {
        BridgeCommand::CheckIntegrity => match service.check_integrity() {
            Ok(report) => success_response(
                "integrity check completed".to_owned(),
                serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            ),
            Err(error) => error_response(error, ERROR_INTEGRITY_FAILURE),
        },
        BridgeCommand::CheckSemantics => match service.check_semantics() {
            Ok(report) => success_response(
                "semantics check completed".to_owned(),
                serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            ),
            Err(error) => error_response(error, ERROR_INTEGRITY_FAILURE),
        },
        BridgeCommand::RebuildProjections => {
            match service.rebuild_projections(parse_target(request.target.as_deref())) {
                Ok(report) => success_response(
                    "projection rebuild completed".to_owned(),
                    serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                ),
                Err(error) => error_response(error, ERROR_EXECUTION_FAILURE),
            }
        }
        BridgeCommand::RebuildMissingProjections => match service.rebuild_missing_projections() {
            Ok(report) => success_response(
                "missing projection rebuild completed".to_owned(),
                serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            ),
            Err(error) => error_response(error, ERROR_EXECUTION_FAILURE),
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
                Err(error) => error_response(error, ERROR_EXECUTION_FAILURE),
            },
            _ => error_response_with_message(
                ERROR_BAD_REQUEST,
                "source_ref is required for trace_source".to_owned(),
            ),
        },
        BridgeCommand::ExciseSource => match request.source_ref.as_deref() {
            Some(source_ref) if !source_ref.is_empty() => match service.excise_source(source_ref) {
                Ok(report) => success_response(
                    "source excised".to_owned(),
                    serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                ),
                Err(error) => error_response(error, ERROR_EXECUTION_FAILURE),
            },
            _ => error_response_with_message(
                ERROR_BAD_REQUEST,
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
                    Err(error) => error_response(error, ERROR_EXECUTION_FAILURE),
                }
            }
            None => error_response_with_message(
                ERROR_BAD_REQUEST,
                "destination_path is required".to_owned(),
            ),
        },
    };

    println!(
        "{}",
        serde_json::to_string(&response).expect("bridge response serializes")
    );
}

fn parse_target(target: Option<&str>) -> ProjectionTarget {
    match target.unwrap_or("all") {
        "fts" => ProjectionTarget::Fts,
        "vec" => ProjectionTarget::Vec,
        _ => ProjectionTarget::All,
    }
}

fn parse_command(command: &str) -> Result<BridgeCommand, &'static str> {
    match command {
        "check_integrity" => Ok(BridgeCommand::CheckIntegrity),
        "check_semantics" => Ok(BridgeCommand::CheckSemantics),
        "rebuild_projections" => Ok(BridgeCommand::RebuildProjections),
        "rebuild_missing_projections" => Ok(BridgeCommand::RebuildMissingProjections),
        "trace_source" => Ok(BridgeCommand::TraceSource),
        "excise_source" => Ok(BridgeCommand::ExciseSource),
        "safe_export" => Ok(BridgeCommand::SafeExport),
        _ => Err(ERROR_UNSUPPORTED_COMMAND),
    }
}

fn classify_parse_error(error: &serde_json::Error) -> &'static str {
    if error.to_string().contains("missing field `command`") {
        ERROR_BAD_REQUEST
    } else if error.to_string().contains("unknown field") {
        ERROR_BAD_REQUEST
    } else {
        ERROR_BAD_REQUEST
    }
}

/// Security fix M-4: Sanitize error messages to avoid leaking internal paths,
/// schema details, or system configuration in bridge responses. The full error
/// is printed to stderr for operator debugging.
fn error_response(error: impl std::fmt::Display, code: &'static str) -> BridgeResponse {
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

fn error_response_with_message(code: &'static str, message: String) -> BridgeResponse {
    BridgeResponse {
        protocol_version: PROTOCOL_VERSION,
        ok: false,
        message,
        error_code: Some(code),
        payload: json!({}),
    }
}

#[allow(clippy::print_stdout, clippy::expect_used)]
fn emit_error(code: &'static str, message: String) {
    let response = error_response_with_message(code, message);
    println!(
        "{}",
        serde_json::to_string(&response).expect("bridge response serializes")
    );
}

#[cfg(test)]
mod tests {
    use super::{ERROR_UNSUPPORTED_COMMAND, parse_command, parse_target};
    use fathomdb_engine::ProjectionTarget;

    #[test]
    fn parse_command_reports_unsupported_command() {
        let result = parse_command("does_not_exist");
        assert_eq!(result.err(), Some(ERROR_UNSUPPORTED_COMMAND));
    }

    #[test]
    fn parse_target_defaults_to_all_for_unknown_value() {
        assert_eq!(parse_target(Some("weird")), ProjectionTarget::All);
    }
}
