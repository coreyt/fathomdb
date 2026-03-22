use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::Arc;

use fathomdb_engine::{AdminService, ProjectionTarget};
use fathomdb_schema::SchemaManager;
use serde::{Deserialize, Serialize};
use serde_json::json;

const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
struct BridgeRequest {
    #[serde(default)]
    protocol_version: u32,
    database_path: PathBuf,
    command: BridgeCommand,
    target: Option<String>,
    source_ref: Option<String>,
    destination_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BridgeCommand {
    CheckIntegrity,
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
    payload: serde_json::Value,
}

fn main() {
    let mut stdin = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut stdin) {
        emit(false, format!("failed to read stdin: {error}"), json!({}));
        return;
    }

    let request: BridgeRequest = match serde_json::from_str(&stdin) {
        Ok(request) => request,
        Err(error) => {
            emit(false, format!("invalid request: {error}"), json!({}));
            return;
        }
    };

    if request.protocol_version != PROTOCOL_VERSION {
        emit(
            false,
            format!(
                "unsupported protocol version: expected {PROTOCOL_VERSION}, got {}",
                request.protocol_version
            ),
            json!({}),
        );
        return;
    }

    let service = AdminService::new(&request.database_path, Arc::new(SchemaManager::new()));
    let response = match request.command {
        BridgeCommand::CheckIntegrity => match service.check_integrity() {
            Ok(report) => BridgeResponse {
                protocol_version: PROTOCOL_VERSION,
                ok: true,
                message: "integrity check completed".to_owned(),
                payload: serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            },
            Err(error) => error_response(error),
        },
        BridgeCommand::RebuildProjections => {
            match service.rebuild_projections(parse_target(request.target.as_deref())) {
                Ok(report) => BridgeResponse {
                    protocol_version: PROTOCOL_VERSION,
                    ok: true,
                    message: "projection rebuild completed".to_owned(),
                    payload: serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                },
                Err(error) => error_response(error),
            }
        }
        BridgeCommand::RebuildMissingProjections => match service.rebuild_missing_projections() {
            Ok(report) => BridgeResponse {
                protocol_version: PROTOCOL_VERSION,
                ok: true,
                message: "missing projection rebuild completed".to_owned(),
                payload: serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            },
            Err(error) => error_response(error),
        },
        BridgeCommand::TraceSource => {
            match service.trace_source(request.source_ref.as_deref().unwrap_or_default()) {
                Ok(report) => BridgeResponse {
                    protocol_version: PROTOCOL_VERSION,
                    ok: true,
                    message: "trace completed".to_owned(),
                    payload: serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                },
                Err(error) => error_response(error),
            }
        }
        BridgeCommand::ExciseSource => {
            match service.excise_source(request.source_ref.as_deref().unwrap_or_default()) {
                Ok(report) => BridgeResponse {
                    protocol_version: PROTOCOL_VERSION,
                    ok: true,
                    message: "source excised".to_owned(),
                    payload: serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                },
                Err(error) => error_response(error),
            }
        }
        BridgeCommand::SafeExport => match request.destination_path {
            Some(destination) => match service.safe_export(destination) {
                Ok(manifest) => BridgeResponse {
                    protocol_version: PROTOCOL_VERSION,
                    ok: true,
                    message: "export created".to_owned(),
                    payload: serde_json::to_value(&manifest).unwrap_or_else(|_| json!({})),
                },
                Err(error) => error_response(error),
            },
            None => BridgeResponse {
                protocol_version: PROTOCOL_VERSION,
                ok: false,
                message: "destination_path is required".to_owned(),
                payload: json!({}),
            },
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

fn error_response(error: impl std::fmt::Display) -> BridgeResponse {
    BridgeResponse {
        protocol_version: PROTOCOL_VERSION,
        ok: false,
        message: error.to_string(),
        payload: json!({}),
    }
}

fn emit(ok: bool, message: String, payload: serde_json::Value) {
    let response = BridgeResponse {
        protocol_version: PROTOCOL_VERSION,
        ok,
        message,
        payload,
    };
    println!(
        "{}",
        serde_json::to_string(&response).expect("bridge response serializes")
    );
}
