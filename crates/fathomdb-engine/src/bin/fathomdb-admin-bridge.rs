use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::Arc;

use fathomdb_engine::{AdminService, ProjectionTarget};
use fathomdb_schema::SchemaManager;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Deserialize)]
struct BridgeRequest {
    database_path: PathBuf,
    command: String,
    target: Option<String>,
    source_ref: Option<String>,
    destination_path: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct BridgeResponse {
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

    let service = AdminService::new(&request.database_path, Arc::new(SchemaManager::new()));
    let response = match request.command.as_str() {
        "check_integrity" => match service.check_integrity() {
            Ok(report) => BridgeResponse {
                ok: true,
                message: "integrity check completed".to_owned(),
                payload: serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            },
            Err(error) => error_response(error),
        },
        "rebuild_projections" => match service.rebuild_projections(parse_target(request.target.as_deref())) {
            Ok(report) => BridgeResponse {
                ok: true,
                message: "projection rebuild completed".to_owned(),
                payload: serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            },
            Err(error) => error_response(error),
        },
        "rebuild_missing_projections" => match service.rebuild_missing_projections() {
            Ok(report) => BridgeResponse {
                ok: true,
                message: "missing projection rebuild completed".to_owned(),
                payload: serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            },
            Err(error) => error_response(error),
        },
        "trace_source" => match service.trace_source(request.source_ref.as_deref().unwrap_or_default()) {
            Ok(report) => BridgeResponse {
                ok: true,
                message: "trace completed".to_owned(),
                payload: serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            },
            Err(error) => error_response(error),
        },
        "excise_source" => match service.excise_source(request.source_ref.as_deref().unwrap_or_default()) {
            Ok(report) => BridgeResponse {
                ok: true,
                message: "source excised".to_owned(),
                payload: serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            },
            Err(error) => error_response(error),
        },
        "safe_export" => match request.destination_path {
            Some(destination) => match service.safe_export(destination) {
                Ok(()) => BridgeResponse {
                    ok: true,
                    message: "export created".to_owned(),
                    payload: json!({}),
                },
                Err(error) => error_response(error),
            },
            None => BridgeResponse {
                ok: false,
                message: "destination_path is required".to_owned(),
                payload: json!({}),
            },
        },
        other => BridgeResponse {
            ok: false,
            message: format!("unknown command: {other}"),
            payload: json!({}),
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
        ok: false,
        message: error.to_string(),
        payload: json!({}),
    }
}

fn emit(ok: bool, message: String, payload: serde_json::Value) {
    let response = BridgeResponse { ok, message, payload };
    println!(
        "{}",
        serde_json::to_string(&response).expect("bridge response serializes")
    );
}
