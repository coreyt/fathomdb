use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fathomdb_engine::{
    AdminService, EngineError, OperationalReadRequest, OperationalRegisterRequest,
    ProjectionTarget, ProvenancePurgeOptions, SafeExportOptions, VectorGeneratorPolicy,
    load_vector_regeneration_config,
};
use fathomdb_schema::{SchemaError, SchemaManager};
use serde::{Deserialize, Serialize};
use serde_json::json;

const PROTOCOL_VERSION: u32 = 1;
const MAX_BRIDGE_INPUT_BYTES: u64 = 64 * 1024 * 1024; // 64 MB

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
    logical_id: Option<String>,
    target: Option<String>,
    source_ref: Option<String>,
    collection_name: Option<String>,
    collection_names: Option<Vec<String>>,
    record_key: Option<String>,
    filter_fields_json: Option<String>,
    validation_json: Option<String>,
    secondary_indexes_json: Option<String>,
    destination_path: Option<PathBuf>,
    force_checkpoint: Option<bool>,
    config_path: Option<PathBuf>,
    now_timestamp: Option<i64>,
    max_collections: Option<usize>,
    before_timestamp: Option<i64>,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    preserve_event_types: Vec<String>,
    vector_generator_policy: Option<VectorGeneratorPolicy>,
    operational_collection: Option<OperationalRegisterRequest>,
    operational_read: Option<OperationalReadRequest>,
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
    RestoreLogicalId,
    PurgeLogicalId,
    TraceSource,
    ExciseSource,
    SafeExport,
    RegisterOperationalCollection,
    DescribeOperationalCollection,
    UpdateOperationalCollectionFilters,
    UpdateOperationalCollectionValidation,
    UpdateOperationalCollectionSecondaryIndexes,
    DisableOperationalCollection,
    CompactOperationalCollection,
    PurgeOperationalCollection,
    RebuildOperationalCurrent,
    RebuildOperationalSecondaryIndexes,
    TraceOperationalCollection,
    ReadOperationalCollection,
    ValidateOperationalCollectionHistory,
    PlanOperationalRetention,
    RunOperationalRetention,
    PurgeProvenanceEvents,
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
    if let Err(error) = io::stdin()
        .take(MAX_BRIDGE_INPUT_BYTES)
        .read_to_string(&mut stdin)
    {
        emit_error(
            BridgeErrorCode::BadRequest,
            format!("failed to read stdin: {error}"),
        );
        return;
    }

    if stdin.len() as u64 >= MAX_BRIDGE_INPUT_BYTES {
        emit_error(
            BridgeErrorCode::BadRequest,
            "input exceeds maximum size".to_owned(),
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

fn validate_path(path: &Path, label: &str) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(format!(
            "{label} must be an absolute path: {}",
            path.display()
        ));
    }
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(format!(
                "{label} must not contain '..' components: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn handle_request(request: BridgeRequest) -> BridgeResponse {
    if let Err(msg) = validate_path(&request.database_path, "database_path") {
        return error_response_with_message(BridgeErrorCode::BadRequest, msg);
    }
    if let Some(ref dest) = request.destination_path
        && let Err(msg) = validate_path(dest, "destination_path")
    {
        return error_response_with_message(BridgeErrorCode::BadRequest, msg);
    }

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
            Err(error) => error_response(&error, BridgeErrorCode::IntegrityFailure),
        },
        BridgeCommand::CheckSemantics => match service.check_semantics() {
            Ok(report) => success_response(
                "semantics check completed".to_owned(),
                serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            ),
            Err(error) => error_response(&error, BridgeErrorCode::IntegrityFailure),
        },
        BridgeCommand::RebuildProjections => match parse_target(request.target.as_deref()) {
            Ok(target) => match service.rebuild_projections(target) {
                Ok(report) => success_response(
                    "projection rebuild completed".to_owned(),
                    serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                ),
                Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
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
            Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
        },
        BridgeCommand::RestoreVectorProfiles => match service.restore_vector_profiles() {
            Ok(report) => success_response(
                "vector profiles restored".to_owned(),
                serde_json::to_value(report).unwrap_or_else(|_| json!({})),
            ),
            Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
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
                    Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                },
                Err(error) => error_response(&error, BridgeErrorCode::BadRequest),
            },
            None => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "config_path is required".to_owned(),
            ),
        },
        BridgeCommand::RestoreLogicalId => match request.logical_id.as_deref() {
            Some(logical_id) if !logical_id.is_empty() => {
                match service.restore_logical_id(logical_id) {
                    Ok(report) => success_response(
                        "logical_id restored".to_owned(),
                        serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                    ),
                    Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                }
            }
            _ => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "logical_id is required".to_owned(),
            ),
        },
        BridgeCommand::PurgeLogicalId => match request.logical_id.as_deref() {
            Some(logical_id) if !logical_id.is_empty() => {
                match service.purge_logical_id(logical_id) {
                    Ok(report) => success_response(
                        "logical_id purged".to_owned(),
                        serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                    ),
                    Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                }
            }
            _ => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "logical_id is required".to_owned(),
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
                Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
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
                Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
            },
            _ => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "source_ref is required for excise_source".to_owned(),
            ),
        },
        BridgeCommand::SafeExport => match request.destination_path {
            Some(destination) => {
                match service.safe_export(
                    destination,
                    request
                        .force_checkpoint
                        .map(|force_checkpoint| SafeExportOptions { force_checkpoint })
                        .unwrap_or_default(),
                ) {
                    Ok(manifest) => success_response(
                        "export created".to_owned(),
                        // SafeExportManifest contains only primitive types; serialization cannot fail.
                        serde_json::to_value(&manifest).unwrap_or_else(|_| {
                            unreachable!("SafeExportManifest serialization is infallible")
                        }),
                    ),
                    Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                }
            }
            None => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "destination_path is required".to_owned(),
            ),
        },
        BridgeCommand::RegisterOperationalCollection => match request.operational_collection {
            Some(register_request) => {
                match service.register_operational_collection(&register_request) {
                    Ok(record) => success_response(
                        "operational collection registered".to_owned(),
                        serde_json::to_value(record).unwrap_or_else(|_| json!({})),
                    ),
                    Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                }
            }
            None => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "operational_collection is required".to_owned(),
            ),
        },
        BridgeCommand::DescribeOperationalCollection => match request.collection_name.as_deref() {
            Some(collection_name) if !collection_name.is_empty() => {
                match service.describe_operational_collection(collection_name) {
                    Ok(Some(record)) => success_response(
                        "operational collection described".to_owned(),
                        serde_json::to_value(record).unwrap_or_else(|_| json!({})),
                    ),
                    Ok(None) => error_response_with_message(
                        BridgeErrorCode::BadRequest,
                        "operational collection not found".to_owned(),
                    ),
                    Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                }
            }
            _ => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "collection_name is required".to_owned(),
            ),
        },
        BridgeCommand::UpdateOperationalCollectionFilters => {
            match (
                request.collection_name.as_deref(),
                request.filter_fields_json.as_deref(),
            ) {
                (Some(collection_name), Some(filter_fields_json))
                    if !collection_name.is_empty() && !filter_fields_json.is_empty() =>
                {
                    match service
                        .update_operational_collection_filters(collection_name, filter_fields_json)
                    {
                        Ok(record) => success_response(
                            "operational collection filters updated".to_owned(),
                            serde_json::to_value(record).unwrap_or_else(|_| json!({})),
                        ),
                        Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                    }
                }
                (Some(collection_name), _) if !collection_name.is_empty() => {
                    error_response_with_message(
                        BridgeErrorCode::BadRequest,
                        "filter_fields_json is required".to_owned(),
                    )
                }
                _ => error_response_with_message(
                    BridgeErrorCode::BadRequest,
                    "collection_name is required".to_owned(),
                ),
            }
        }
        BridgeCommand::UpdateOperationalCollectionValidation => {
            match (
                request.collection_name.as_deref(),
                request.validation_json.as_deref(),
            ) {
                (Some(collection_name), Some(validation_json)) if !collection_name.is_empty() => {
                    match service
                        .update_operational_collection_validation(collection_name, validation_json)
                    {
                        Ok(record) => success_response(
                            "operational collection validation updated".to_owned(),
                            serde_json::to_value(record).unwrap_or_else(|_| json!({})),
                        ),
                        Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                    }
                }
                (Some(collection_name), None) if !collection_name.is_empty() => {
                    error_response_with_message(
                        BridgeErrorCode::BadRequest,
                        "validation_json is required".to_owned(),
                    )
                }
                _ => error_response_with_message(
                    BridgeErrorCode::BadRequest,
                    "collection_name is required".to_owned(),
                ),
            }
        }
        BridgeCommand::UpdateOperationalCollectionSecondaryIndexes => {
            match (
                request.collection_name.as_deref(),
                request.secondary_indexes_json.as_deref(),
            ) {
                (Some(collection_name), Some(secondary_indexes_json))
                    if !collection_name.is_empty() && !secondary_indexes_json.is_empty() =>
                {
                    match service.update_operational_collection_secondary_indexes(
                        collection_name,
                        secondary_indexes_json,
                    ) {
                        Ok(record) => success_response(
                            "operational collection secondary indexes updated".to_owned(),
                            serde_json::to_value(record).unwrap_or_else(|_| json!({})),
                        ),
                        Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                    }
                }
                (Some(collection_name), _) if !collection_name.is_empty() => {
                    error_response_with_message(
                        BridgeErrorCode::BadRequest,
                        "secondary_indexes_json is required".to_owned(),
                    )
                }
                _ => error_response_with_message(
                    BridgeErrorCode::BadRequest,
                    "collection_name is required".to_owned(),
                ),
            }
        }
        BridgeCommand::DisableOperationalCollection => match request.collection_name.as_deref() {
            Some(collection_name) if !collection_name.is_empty() => {
                match service.disable_operational_collection(collection_name) {
                    Ok(record) => success_response(
                        "operational collection disabled".to_owned(),
                        serde_json::to_value(record).unwrap_or_else(|_| json!({})),
                    ),
                    Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                }
            }
            _ => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "collection_name is required".to_owned(),
            ),
        },
        BridgeCommand::CompactOperationalCollection => match request.collection_name.as_deref() {
            Some(collection_name) if !collection_name.is_empty() => {
                match service.compact_operational_collection(collection_name, request.dry_run) {
                    Ok(report) => success_response(
                        "operational collection compacted".to_owned(),
                        serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                    ),
                    Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                }
            }
            _ => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "collection_name is required".to_owned(),
            ),
        },
        BridgeCommand::PurgeOperationalCollection => {
            match (request.collection_name.as_deref(), request.before_timestamp) {
                (Some(collection_name), Some(before_timestamp)) if !collection_name.is_empty() => {
                    match service.purge_operational_collection(collection_name, before_timestamp) {
                        Ok(report) => success_response(
                            "operational collection purged".to_owned(),
                            serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                        ),
                        Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                    }
                }
                (Some(collection_name), None) if !collection_name.is_empty() => {
                    error_response_with_message(
                        BridgeErrorCode::BadRequest,
                        "before_timestamp is required".to_owned(),
                    )
                }
                _ => error_response_with_message(
                    BridgeErrorCode::BadRequest,
                    "collection_name is required".to_owned(),
                ),
            }
        }
        BridgeCommand::RebuildOperationalCurrent => {
            match service.rebuild_operational_current(request.collection_name.as_deref()) {
                Ok(report) => success_response(
                    "operational current rebuilt".to_owned(),
                    serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                ),
                Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
            }
        }
        BridgeCommand::RebuildOperationalSecondaryIndexes => {
            match request.collection_name.as_deref() {
                Some(collection_name) if !collection_name.is_empty() => {
                    match service.rebuild_operational_secondary_indexes(collection_name) {
                        Ok(report) => success_response(
                            "operational secondary indexes rebuilt".to_owned(),
                            serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                        ),
                        Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                    }
                }
                _ => error_response_with_message(
                    BridgeErrorCode::BadRequest,
                    "collection_name is required".to_owned(),
                ),
            }
        }
        BridgeCommand::TraceOperationalCollection => match request.collection_name.as_deref() {
            Some(collection_name) if !collection_name.is_empty() => {
                match service
                    .trace_operational_collection(collection_name, request.record_key.as_deref())
                {
                    Ok(report) => success_response(
                        "operational collection traced".to_owned(),
                        serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                    ),
                    Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                }
            }
            _ => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "collection_name is required".to_owned(),
            ),
        },
        BridgeCommand::ReadOperationalCollection => match request.operational_read.as_ref() {
            Some(operational_read) => match service.read_operational_collection(operational_read) {
                Ok(report) => success_response(
                    "operational collection read completed".to_owned(),
                    serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                ),
                Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
            },
            None => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "operational_read is required".to_owned(),
            ),
        },
        BridgeCommand::ValidateOperationalCollectionHistory => {
            match request.collection_name.as_deref() {
                Some(collection_name) if !collection_name.is_empty() => {
                    match service.validate_operational_collection_history(collection_name) {
                        Ok(report) => success_response(
                            "operational collection history validation completed".to_owned(),
                            serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                        ),
                        Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                    }
                }
                _ => error_response_with_message(
                    BridgeErrorCode::BadRequest,
                    "collection_name is required".to_owned(),
                ),
            }
        }
        BridgeCommand::PlanOperationalRetention => match request.now_timestamp {
            Some(now_timestamp) => match service.plan_operational_retention(
                now_timestamp,
                request.collection_names.as_deref(),
                request.max_collections,
            ) {
                Ok(report) => success_response(
                    "operational retention plan completed".to_owned(),
                    serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                ),
                Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
            },
            None => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "now_timestamp is required".to_owned(),
            ),
        },
        BridgeCommand::RunOperationalRetention => match request.now_timestamp {
            Some(now_timestamp) => match service.run_operational_retention(
                now_timestamp,
                request.collection_names.as_deref(),
                request.max_collections,
                request.dry_run,
            ) {
                Ok(report) => success_response(
                    "operational retention run completed".to_owned(),
                    serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                ),
                Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
            },
            None => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "now_timestamp is required".to_owned(),
            ),
        },
        BridgeCommand::PurgeProvenanceEvents => match request.before_timestamp {
            Some(before_timestamp) => {
                let options = ProvenancePurgeOptions {
                    dry_run: request.dry_run,
                    preserve_event_types: request.preserve_event_types,
                };
                match service.purge_provenance_events(before_timestamp, &options) {
                    Ok(report) => success_response(
                        "provenance events purged".to_owned(),
                        serde_json::to_value(report).unwrap_or_else(|_| json!({})),
                    ),
                    Err(error) => error_response(&error, BridgeErrorCode::ExecutionFailure),
                }
            }
            None => error_response_with_message(
                BridgeErrorCode::BadRequest,
                "before_timestamp is required".to_owned(),
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
        "restore_logical_id" => Ok(BridgeCommand::RestoreLogicalId),
        "purge_logical_id" => Ok(BridgeCommand::PurgeLogicalId),
        "trace_source" => Ok(BridgeCommand::TraceSource),
        "excise_source" => Ok(BridgeCommand::ExciseSource),
        "safe_export" => Ok(BridgeCommand::SafeExport),
        "register_operational_collection" => Ok(BridgeCommand::RegisterOperationalCollection),
        "describe_operational_collection" => Ok(BridgeCommand::DescribeOperationalCollection),
        "update_operational_collection_filters" => {
            Ok(BridgeCommand::UpdateOperationalCollectionFilters)
        }
        "update_operational_collection_validation" => {
            Ok(BridgeCommand::UpdateOperationalCollectionValidation)
        }
        "update_operational_collection_secondary_indexes" => {
            Ok(BridgeCommand::UpdateOperationalCollectionSecondaryIndexes)
        }
        "disable_operational_collection" => Ok(BridgeCommand::DisableOperationalCollection),
        "compact_operational_collection" => Ok(BridgeCommand::CompactOperationalCollection),
        "purge_operational_collection" => Ok(BridgeCommand::PurgeOperationalCollection),
        "rebuild_operational_current" => Ok(BridgeCommand::RebuildOperationalCurrent),
        "rebuild_operational_secondary_indexes" => {
            Ok(BridgeCommand::RebuildOperationalSecondaryIndexes)
        }
        "trace_operational_collection" => Ok(BridgeCommand::TraceOperationalCollection),
        "read_operational_collection" => Ok(BridgeCommand::ReadOperationalCollection),
        "validate_operational_collection_history" => {
            Ok(BridgeCommand::ValidateOperationalCollectionHistory)
        }
        "plan_operational_retention" => Ok(BridgeCommand::PlanOperationalRetention),
        "run_operational_retention" => Ok(BridgeCommand::RunOperationalRetention),
        "purge_provenance_events" => Ok(BridgeCommand::PurgeProvenanceEvents),
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
fn error_response(error: &EngineError, default_code: BridgeErrorCode) -> BridgeResponse {
    let code = classify_engine_error(error, default_code);
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
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::{
        BridgeErrorCode, BridgeRequest, classify_engine_error, handle_request_body, parse_command,
        parse_target, validate_path,
    };
    use fathomdb_engine::{EngineError, ProjectionTarget};
    use fathomdb_schema::SchemaError;
    use std::path::Path;

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

    #[test]
    fn handle_request_body_rejects_missing_collection_name_for_disable_operational_collection() {
        let response = handle_request_body(
            r#"{"protocol_version":1,"database_path":"/tmp/fathom.db","command":"disable_operational_collection"}"#,
        );
        assert!(!response.ok);
        assert_eq!(response.error_code, Some(BridgeErrorCode::BadRequest));
        assert!(response.message.contains("collection_name is required"));
    }

    #[test]
    fn handle_request_body_rejects_missing_filter_fields_json_for_update_operational_collection_filters()
     {
        let response = handle_request_body(
            r#"{"protocol_version":1,"database_path":"/tmp/fathom.db","command":"update_operational_collection_filters","collection_name":"audit_log"}"#,
        );
        assert!(!response.ok);
        assert_eq!(response.error_code, Some(BridgeErrorCode::BadRequest));
        assert!(response.message.contains("filter_fields_json is required"));
    }

    #[test]
    fn handle_request_body_rejects_missing_before_timestamp_for_purge_operational_collection() {
        let response = handle_request_body(
            r#"{"protocol_version":1,"database_path":"/tmp/fathom.db","command":"purge_operational_collection","collection_name":"audit_log"}"#,
        );
        assert!(!response.ok);
        assert_eq!(response.error_code, Some(BridgeErrorCode::BadRequest));
        assert!(response.message.contains("before_timestamp is required"));
    }

    #[test]
    fn handle_request_body_rejects_missing_operational_read_for_read_operational_collection() {
        let response = handle_request_body(
            r#"{"protocol_version":1,"database_path":"/tmp/fathom.db","command":"read_operational_collection"}"#,
        );
        assert!(!response.ok);
        assert_eq!(response.error_code, Some(BridgeErrorCode::BadRequest));
        assert!(response.message.contains("operational_read is required"));
    }

    #[test]
    fn handle_request_body_rejects_missing_logical_id_for_restore_logical_id() {
        let response = handle_request_body(
            r#"{"protocol_version":1,"database_path":"/tmp/fathom.db","command":"restore_logical_id"}"#,
        );
        assert!(!response.ok);
        assert_eq!(response.error_code, Some(BridgeErrorCode::BadRequest));
        assert!(response.message.contains("logical_id is required"));
    }

    #[test]
    fn handle_request_body_rejects_missing_logical_id_for_purge_logical_id() {
        let response = handle_request_body(
            r#"{"protocol_version":1,"database_path":"/tmp/fathom.db","command":"purge_logical_id"}"#,
        );
        assert!(!response.ok);
        assert_eq!(response.error_code, Some(BridgeErrorCode::BadRequest));
        assert!(response.message.contains("logical_id is required"));
    }

    #[test]
    fn bridge_request_parses_force_checkpoint_for_safe_export() {
        let request: BridgeRequest = serde_json::from_str(
            r#"{"protocol_version":1,"database_path":"/tmp/fathom.db","command":"safe_export","destination_path":"/tmp/export.db","force_checkpoint":true}"#,
        )
        .expect("request parses");

        assert_eq!(request.force_checkpoint, Some(true));
    }

    #[test]
    fn bridge_request_omits_force_checkpoint_when_not_requested() {
        let request: BridgeRequest = serde_json::from_str(
            r#"{"protocol_version":1,"database_path":"/tmp/fathom.db","command":"safe_export","destination_path":"/tmp/export.db"}"#,
        )
        .expect("request parses");

        assert_eq!(request.force_checkpoint, None);
    }

    #[test]
    fn validate_path_rejects_relative_path() {
        let err = validate_path(Path::new("relative/path"), "test").unwrap_err();
        assert!(err.contains("must be an absolute path"));
    }

    #[test]
    fn validate_path_rejects_parent_traversal() {
        let err = validate_path(Path::new("/foo/../bar"), "test").unwrap_err();
        assert!(err.contains("must not contain '..' components"));
    }

    #[test]
    fn validate_path_accepts_absolute_path() {
        let result = validate_path(Path::new("/valid/absolute/path"), "test");
        assert!(result.is_ok());
    }

    #[test]
    fn bridge_rejects_relative_database_path() {
        let response = handle_request_body(
            r#"{"protocol_version":1,"database_path":"relative/path.db","command":"check_integrity"}"#,
        );
        assert!(!response.ok);
        assert_eq!(response.error_code, Some(BridgeErrorCode::BadRequest));
        assert!(response.message.contains("must be an absolute path"));
    }

    #[test]
    fn bridge_rejects_database_path_with_parent_traversal() {
        let response = handle_request_body(
            r#"{"protocol_version":1,"database_path":"/tmp/../etc/passwd","command":"check_integrity"}"#,
        );
        assert!(!response.ok);
        assert_eq!(response.error_code, Some(BridgeErrorCode::BadRequest));
        assert!(
            response
                .message
                .contains("must not contain '..' components")
        );
    }

    #[test]
    fn bridge_rejects_destination_path_with_parent_traversal() {
        let response = handle_request_body(
            r#"{"protocol_version":1,"database_path":"/tmp/fathom.db","command":"safe_export","destination_path":"/tmp/../etc/export"}"#,
        );
        assert!(!response.ok);
        assert_eq!(response.error_code, Some(BridgeErrorCode::BadRequest));
        assert!(response.message.contains("destination_path"));
        assert!(
            response
                .message
                .contains("must not contain '..' components")
        );
    }
}
