use std::fmt::Write as _;
use std::io;
use std::time::SystemTime;

use rusqlite::{OptionalExtension, TransactionBehavior};
use serde::Deserialize;

use super::{
    AdminService, DEFAULT_OPERATIONAL_READ_LIMIT, EngineError, MAX_OPERATIONAL_READ_LIMIT,
    persist_simple_provenance_event, rebuild_operational_current_rows,
};
use crate::ids::new_id;
use crate::operational::{
    OperationalCollectionKind, OperationalCollectionRecord, OperationalCompactionReport,
    OperationalCurrentRow, OperationalFilterClause, OperationalFilterField,
    OperationalFilterFieldType, OperationalFilterMode, OperationalFilterValue,
    OperationalHistoryValidationIssue, OperationalHistoryValidationReport, OperationalMutationRow,
    OperationalPurgeReport, OperationalReadReport, OperationalReadRequest,
    OperationalRegisterRequest, OperationalRepairReport, OperationalRetentionActionKind,
    OperationalRetentionPlanItem, OperationalRetentionPlanReport, OperationalRetentionRunItem,
    OperationalRetentionRunReport, OperationalSecondaryIndexDefinition,
    OperationalSecondaryIndexRebuildReport, OperationalTraceReport,
    extract_secondary_index_entries_for_current, extract_secondary_index_entries_for_mutation,
    parse_operational_secondary_indexes_json, parse_operational_validation_contract,
    validate_operational_payload_against_contract,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum OperationalRetentionPolicy {
    KeepAll,
    PurgeBeforeSeconds { max_age_seconds: i64 },
    KeepLast { max_rows: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CompiledOperationalReadFilter {
    field: String,
    condition: OperationalReadCondition,
}

#[derive(Clone, Debug)]
struct MatchedAppendOnlySecondaryIndexRead<'a> {
    index_name: &'a str,
    value_filter: &'a CompiledOperationalReadFilter,
    time_range: Option<&'a CompiledOperationalReadFilter>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum OperationalReadCondition {
    ExactString(String),
    ExactInteger(i64),
    Prefix(String),
    Range {
        lower: Option<i64>,
        upper: Option<i64>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExtractedOperationalFilterValue {
    field_name: String,
    string_value: Option<String>,
    integer_value: Option<i64>,
}

impl AdminService {
    /// # Errors
    /// Returns [`EngineError`] if the collection metadata is invalid or the insert fails.
    pub fn register_operational_collection(
        &self,
        request: &OperationalRegisterRequest,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        if request.name.trim().is_empty() {
            return Err(EngineError::InvalidWrite(
                "operational collection name must not be empty".to_owned(),
            ));
        }
        if request.schema_json.is_empty() {
            return Err(EngineError::InvalidWrite(
                "operational collection schema_json must not be empty".to_owned(),
            ));
        }
        if request.retention_json.is_empty() {
            return Err(EngineError::InvalidWrite(
                "operational collection retention_json must not be empty".to_owned(),
            ));
        }
        if request.filter_fields_json.is_empty() {
            return Err(EngineError::InvalidWrite(
                "operational collection filter_fields_json must not be empty".to_owned(),
            ));
        }
        parse_operational_validation_contract(&request.validation_json)
            .map_err(EngineError::InvalidWrite)?;
        parse_operational_secondary_indexes_json(&request.secondary_indexes_json, request.kind)
            .map_err(EngineError::InvalidWrite)?;
        if request.format_version <= 0 {
            return Err(EngineError::InvalidWrite(
                "operational collection format_version must be positive".to_owned(),
            ));
        }
        parse_operational_filter_fields(&request.filter_fields_json)
            .map_err(EngineError::InvalidWrite)?;

        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO operational_collections \
             (name, kind, schema_json, retention_json, filter_fields_json, validation_json, secondary_indexes_json, format_version, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, unixepoch())",
            rusqlite::params![
                request.name.as_str(),
                request.kind.as_str(),
                request.schema_json.as_str(),
                request.retention_json.as_str(),
                request.filter_fields_json.as_str(),
                request.validation_json.as_str(),
                request.secondary_indexes_json.as_str(),
                request.format_version,
            ],
        )?;
        persist_simple_provenance_event(
            &tx,
            "operational_collection_registered",
            request.name.as_str(),
            Some(serde_json::json!({
                "kind": request.kind.as_str(),
                "format_version": request.format_version,
            })),
        )?;
        tx.commit()?;

        self.describe_operational_collection(&request.name)?
            .ok_or_else(|| {
                EngineError::Bridge("registered collection missing after commit".to_owned())
            })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn describe_operational_collection(
        &self,
        name: &str,
    ) -> Result<Option<OperationalCollectionRecord>, EngineError> {
        let conn = self.connect()?;
        load_operational_collection_record(&conn, name)
    }

    /// # Errors
    /// Returns [`EngineError`] if the collection is missing, the filter contract is invalid,
    /// or existing mutation backfill fails.
    pub fn update_operational_collection_filters(
        &self,
        name: &str,
        filter_fields_json: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        if filter_fields_json.is_empty() {
            return Err(EngineError::InvalidWrite(
                "operational collection filter_fields_json must not be empty".to_owned(),
            ));
        }
        let declared_fields = parse_operational_filter_fields(filter_fields_json)
            .map_err(EngineError::InvalidWrite)?;

        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        tx.execute(
            "UPDATE operational_collections SET filter_fields_json = ?2 WHERE name = ?1",
            rusqlite::params![name, filter_fields_json],
        )?;
        tx.execute(
            "DELETE FROM operational_filter_values WHERE collection_name = ?1",
            [name],
        )?;

        let mut mutation_stmt = tx.prepare(
            "SELECT id, payload_json FROM operational_mutations \
             WHERE collection_name = ?1 ORDER BY mutation_order",
        )?;
        let mutations = mutation_stmt
            .query_map([name], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(mutation_stmt);

        let mut insert_filter_value = tx.prepare_cached(
            "INSERT INTO operational_filter_values \
             (mutation_id, collection_name, field_name, string_value, integer_value) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut inserted_values = 0usize;
        for (mutation_id, payload_json) in &mutations {
            for filter_value in
                extract_operational_filter_values(&declared_fields, payload_json.as_str())
            {
                insert_filter_value.execute(rusqlite::params![
                    mutation_id,
                    name,
                    filter_value.field_name,
                    filter_value.string_value,
                    filter_value.integer_value,
                ])?;
                inserted_values += 1;
            }
        }
        drop(insert_filter_value);

        persist_simple_provenance_event(
            &tx,
            "operational_collection_filter_fields_updated",
            name,
            Some(serde_json::json!({
                "field_count": declared_fields.len(),
                "mutations_backfilled": mutations.len(),
                "inserted_filter_values": inserted_values,
            })),
        )?;
        let updated = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::Bridge("operational collection missing after filter update".to_owned())
        })?;
        tx.commit()?;
        Ok(updated)
    }

    /// # Errors
    /// Returns [`EngineError`] if the collection is missing or the validation contract is invalid.
    pub fn update_operational_collection_validation(
        &self,
        name: &str,
        validation_json: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        parse_operational_validation_contract(validation_json)
            .map_err(EngineError::InvalidWrite)?;

        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        tx.execute(
            "UPDATE operational_collections SET validation_json = ?2 WHERE name = ?1",
            rusqlite::params![name, validation_json],
        )?;
        persist_simple_provenance_event(
            &tx,
            "operational_collection_validation_updated",
            name,
            Some(serde_json::json!({
                "has_validation": !validation_json.is_empty(),
            })),
        )?;
        let updated = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::Bridge("operational collection missing after validation update".to_owned())
        })?;
        tx.commit()?;
        Ok(updated)
    }

    /// # Errors
    /// Returns [`EngineError`] if the collection is missing, the contract is invalid,
    /// or derived index rebuild fails.
    pub fn update_operational_collection_secondary_indexes(
        &self,
        name: &str,
        secondary_indexes_json: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        let indexes = parse_operational_secondary_indexes_json(secondary_indexes_json, record.kind)
            .map_err(EngineError::InvalidWrite)?;
        tx.execute(
            "UPDATE operational_collections SET secondary_indexes_json = ?2 WHERE name = ?1",
            rusqlite::params![name, secondary_indexes_json],
        )?;
        let (mutation_entries_rebuilt, current_entries_rebuilt) =
            rebuild_operational_secondary_index_entries(&tx, &record.name, record.kind, &indexes)?;
        persist_simple_provenance_event(
            &tx,
            "operational_collection_secondary_indexes_updated",
            name,
            Some(serde_json::json!({
                "index_count": indexes.len(),
                "mutation_entries_rebuilt": mutation_entries_rebuilt,
                "current_entries_rebuilt": current_entries_rebuilt,
            })),
        )?;
        let updated = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::Bridge(
                "operational collection missing after secondary index update".to_owned(),
            )
        })?;
        tx.commit()?;
        Ok(updated)
    }

    /// # Errors
    /// Returns [`EngineError`] if the collection is missing or rebuild fails.
    pub fn rebuild_operational_secondary_indexes(
        &self,
        name: &str,
    ) -> Result<OperationalSecondaryIndexRebuildReport, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        let indexes =
            parse_operational_secondary_indexes_json(&record.secondary_indexes_json, record.kind)
                .map_err(EngineError::InvalidWrite)?;
        let (mutation_entries_rebuilt, current_entries_rebuilt) =
            rebuild_operational_secondary_index_entries(&tx, &record.name, record.kind, &indexes)?;
        persist_simple_provenance_event(
            &tx,
            "operational_secondary_indexes_rebuilt",
            name,
            Some(serde_json::json!({
                "index_count": indexes.len(),
                "mutation_entries_rebuilt": mutation_entries_rebuilt,
                "current_entries_rebuilt": current_entries_rebuilt,
            })),
        )?;
        tx.commit()?;
        Ok(OperationalSecondaryIndexRebuildReport {
            collection_name: name.to_owned(),
            mutation_entries_rebuilt,
            current_entries_rebuilt,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the collection is missing or its validation contract is invalid.
    pub fn validate_operational_collection_history(
        &self,
        name: &str,
    ) -> Result<OperationalHistoryValidationReport, EngineError> {
        let conn = self.connect()?;
        let record = load_operational_collection_record(&conn, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        let Some(contract) = parse_operational_validation_contract(&record.validation_json)
            .map_err(EngineError::InvalidWrite)?
        else {
            return Err(EngineError::InvalidWrite(format!(
                "operational collection '{name}' has no validation_json configured"
            )));
        };

        let mut stmt = conn.prepare(
            "SELECT id, record_key, op_kind, payload_json FROM operational_mutations \
             WHERE collection_name = ?1 ORDER BY mutation_order",
        )?;
        let rows = stmt
            .query_map([name], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut checked_rows = 0usize;
        let mut issues = Vec::new();
        for (mutation_id, record_key, op_kind, payload_json) in rows {
            if op_kind == "delete" {
                continue;
            }
            checked_rows += 1;
            if let Err(message) =
                validate_operational_payload_against_contract(&contract, payload_json.as_str())
            {
                issues.push(OperationalHistoryValidationIssue {
                    mutation_id,
                    record_key,
                    op_kind,
                    message,
                });
            }
        }

        Ok(OperationalHistoryValidationReport {
            collection_name: name.to_owned(),
            checked_rows,
            invalid_row_count: issues.len(),
            issues,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn disable_operational_collection(
        &self,
        name: &str,
    ) -> Result<OperationalCollectionRecord, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        let changed = if record.disabled_at.is_none() {
            tx.execute(
                "UPDATE operational_collections SET disabled_at = unixepoch() WHERE name = ?1",
                [name],
            )?;
            true
        } else {
            false
        };
        let record = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::Bridge("operational collection missing after disable".to_owned())
        })?;
        persist_simple_provenance_event(
            &tx,
            "operational_collection_disabled",
            name,
            Some(serde_json::json!({
                "disabled_at": record.disabled_at,
                "changed": changed,
            })),
        )?;
        tx.commit()?;
        Ok(record)
    }

    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn compact_operational_collection(
        &self,
        name: &str,
        dry_run: bool,
    ) -> Result<OperationalCompactionReport, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let collection = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        validate_append_only_operational_collection(&collection, "compact")?;
        let (mutation_ids, before_timestamp) =
            operational_compaction_candidates(&tx, &collection.retention_json, name)?;
        if dry_run {
            drop(tx);
            return Ok(OperationalCompactionReport {
                collection_name: name.to_owned(),
                deleted_mutations: mutation_ids.len(),
                dry_run: true,
                before_timestamp,
            });
        }
        let mut delete_stmt =
            tx.prepare_cached("DELETE FROM operational_mutations WHERE id = ?1")?;
        for mutation_id in &mutation_ids {
            delete_stmt.execute([mutation_id.as_str()])?;
        }
        drop(delete_stmt);
        persist_simple_provenance_event(
            &tx,
            "operational_collection_compacted",
            name,
            Some(serde_json::json!({
                "deleted_mutations": mutation_ids.len(),
                "before_timestamp": before_timestamp,
            })),
        )?;
        tx.commit()?;
        Ok(OperationalCompactionReport {
            collection_name: name.to_owned(),
            deleted_mutations: mutation_ids.len(),
            dry_run: false,
            before_timestamp,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn purge_operational_collection(
        &self,
        name: &str,
        before_timestamp: i64,
    ) -> Result<OperationalPurgeReport, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let collection = load_operational_collection_record(&tx, name)?.ok_or_else(|| {
            EngineError::InvalidWrite(format!("operational collection '{name}' is not registered"))
        })?;
        validate_append_only_operational_collection(&collection, "purge")?;
        let deleted_mutations = tx.execute(
            "DELETE FROM operational_mutations WHERE collection_name = ?1 AND created_at < ?2",
            rusqlite::params![name, before_timestamp],
        )?;
        persist_simple_provenance_event(
            &tx,
            "operational_collection_purged",
            name,
            Some(serde_json::json!({
                "deleted_mutations": deleted_mutations,
                "before_timestamp": before_timestamp,
            })),
        )?;
        tx.commit()?;
        Ok(OperationalPurgeReport {
            collection_name: name.to_owned(),
            deleted_mutations,
            before_timestamp,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if collection selection or policy parsing fails.
    pub fn plan_operational_retention(
        &self,
        now_timestamp: i64,
        collection_names: Option<&[String]>,
        max_collections: Option<usize>,
    ) -> Result<OperationalRetentionPlanReport, EngineError> {
        let conn = self.connect()?;
        let records = load_operational_retention_records(&conn, collection_names, max_collections)?;
        let mut items = Vec::with_capacity(records.len());
        for record in records {
            items.push(plan_operational_retention_item(
                &conn,
                &record,
                now_timestamp,
            )?);
        }
        Ok(OperationalRetentionPlanReport {
            planned_at: now_timestamp,
            collections_examined: items.len(),
            items,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if collection selection, policy parsing, or execution fails.
    pub fn run_operational_retention(
        &self,
        now_timestamp: i64,
        collection_names: Option<&[String]>,
        max_collections: Option<usize>,
        dry_run: bool,
    ) -> Result<OperationalRetentionRunReport, EngineError> {
        let mut conn = self.connect()?;
        let records = load_operational_retention_records(&conn, collection_names, max_collections)?;
        let mut items = Vec::with_capacity(records.len());
        let mut collections_acted_on = 0usize;

        for record in records {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let item = run_operational_retention_item(&tx, &record, now_timestamp, dry_run)?;
            if item.deleted_mutations > 0 {
                collections_acted_on += 1;
            }
            if dry_run || item.action_kind == OperationalRetentionActionKind::Noop {
                drop(tx);
            } else {
                tx.commit()?;
            }
            items.push(item);
        }

        Ok(OperationalRetentionRunReport {
            executed_at: now_timestamp,
            collections_examined: items.len(),
            collections_acted_on,
            dry_run,
            items,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the database query fails.
    pub fn trace_operational_collection(
        &self,
        collection_name: &str,
        record_key: Option<&str>,
    ) -> Result<OperationalTraceReport, EngineError> {
        let conn = self.connect()?;
        ensure_operational_collection_registered(&conn, collection_name)?;
        let mutations = if let Some(record_key) = record_key {
            let mut stmt = conn.prepare(
                "SELECT id, collection_name, record_key, op_kind, payload_json, source_ref, created_at \
                 FROM operational_mutations \
                 WHERE collection_name = ?1 AND record_key = ?2 \
                 ORDER BY mutation_order",
            )?;
            stmt.query_map([collection_name, record_key], map_operational_mutation_row)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, collection_name, record_key, op_kind, payload_json, source_ref, created_at \
                 FROM operational_mutations \
                 WHERE collection_name = ?1 \
                 ORDER BY mutation_order",
            )?;
            stmt.query_map([collection_name], map_operational_mutation_row)?
                .collect::<Result<Vec<_>, _>>()?
        };
        let current_rows = if let Some(record_key) = record_key {
            let mut stmt = conn.prepare(
                "SELECT collection_name, record_key, payload_json, updated_at, last_mutation_id \
                 FROM operational_current \
                 WHERE collection_name = ?1 AND record_key = ?2 \
                 ORDER BY updated_at, record_key",
            )?;
            stmt.query_map([collection_name, record_key], map_operational_current_row)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            let mut stmt = conn.prepare(
                "SELECT collection_name, record_key, payload_json, updated_at, last_mutation_id \
                 FROM operational_current \
                 WHERE collection_name = ?1 \
                 ORDER BY updated_at, record_key",
            )?;
            stmt.query_map([collection_name], map_operational_current_row)?
                .collect::<Result<Vec<_>, _>>()?
        };

        Ok(OperationalTraceReport {
            collection_name: collection_name.to_owned(),
            record_key: record_key.map(str::to_owned),
            mutation_count: mutations.len(),
            current_count: current_rows.len(),
            mutations,
            current_rows,
        })
    }

    /// # Errors
    /// Returns [`EngineError`] if the collection contract is invalid or the filtered read fails.
    pub fn read_operational_collection(
        &self,
        request: &OperationalReadRequest,
    ) -> Result<OperationalReadReport, EngineError> {
        if request.collection_name.trim().is_empty() {
            return Err(EngineError::InvalidWrite(
                "operational read collection_name must not be empty".to_owned(),
            ));
        }
        if request.filters.is_empty() {
            return Err(EngineError::InvalidWrite(
                "operational read requires at least one filter clause".to_owned(),
            ));
        }

        let conn = self.connect()?;
        let record = load_operational_collection_record(&conn, &request.collection_name)?
            .ok_or_else(|| {
                EngineError::InvalidWrite(format!(
                    "operational collection '{}' is not registered",
                    request.collection_name
                ))
            })?;
        validate_append_only_operational_collection(&record, "read")?;
        let declared_fields = parse_operational_filter_fields(&record.filter_fields_json)
            .map_err(EngineError::InvalidWrite)?;
        let secondary_indexes =
            parse_operational_secondary_indexes_json(&record.secondary_indexes_json, record.kind)
                .map_err(EngineError::InvalidWrite)?;
        let applied_limit = operational_read_limit(request.limit)?;
        let filters = compile_operational_read_filters(&request.filters, &declared_fields)?;
        if let Some(report) = execute_operational_secondary_index_read(
            &conn,
            &request.collection_name,
            &filters,
            &secondary_indexes,
            applied_limit,
        )? {
            return Ok(report);
        }
        execute_operational_filtered_read(&conn, &request.collection_name, &filters, applied_limit)
    }

    /// # Errors
    /// Returns [`EngineError`] if the database query fails or collection validation fails.
    pub fn rebuild_operational_current(
        &self,
        collection_name: Option<&str>,
    ) -> Result<OperationalRepairReport, EngineError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let collections = if let Some(name) = collection_name {
            let maybe_kind: Option<String> = tx
                .query_row(
                    "SELECT kind FROM operational_collections WHERE name = ?1",
                    [name],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(kind) = maybe_kind else {
                return Err(EngineError::InvalidWrite(format!(
                    "operational collection '{name}' is not registered"
                )));
            };
            if kind != OperationalCollectionKind::LatestState.as_str() {
                return Err(EngineError::InvalidWrite(format!(
                    "operational collection '{name}' is not latest_state"
                )));
            }
            vec![name.to_owned()]
        } else {
            let mut stmt = tx.prepare(
                "SELECT name FROM operational_collections WHERE kind = 'latest_state' ORDER BY name",
            )?;
            stmt.query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };

        let rebuilt_rows = rebuild_operational_current_rows(&tx, &collections)?;
        for collection in &collections {
            let record = load_operational_collection_record(&tx, collection)?.ok_or_else(|| {
                EngineError::Bridge(format!(
                    "operational collection '{collection}' missing during current rebuild"
                ))
            })?;
            let indexes = parse_operational_secondary_indexes_json(
                &record.secondary_indexes_json,
                record.kind,
            )
            .map_err(EngineError::InvalidWrite)?;
            if !indexes.is_empty() {
                rebuild_operational_secondary_index_entries(
                    &tx,
                    &record.name,
                    record.kind,
                    &indexes,
                )?;
            }
        }

        persist_simple_provenance_event(
            &tx,
            "operational_current_rebuilt",
            collection_name.unwrap_or("*"),
            Some(serde_json::json!({
                "collections_rebuilt": collections.len(),
                "current_rows_rebuilt": rebuilt_rows,
            })),
        )?;
        tx.commit()?;

        Ok(OperationalRepairReport {
            collections_rebuilt: collections.len(),
            current_rows_rebuilt: rebuilt_rows,
        })
    }
}

pub(super) fn load_operational_collection_record(
    conn: &rusqlite::Connection,
    name: &str,
) -> Result<Option<OperationalCollectionRecord>, EngineError> {
    conn.query_row(
        "SELECT name, kind, schema_json, retention_json, filter_fields_json, validation_json, secondary_indexes_json, format_version, created_at, disabled_at \
         FROM operational_collections WHERE name = ?1",
        [name],
        map_operational_collection_row,
    )
    .optional()
    .map_err(EngineError::Sqlite)
}

pub(super) fn map_operational_mutation_row(
    row: &rusqlite::Row<'_>,
) -> Result<OperationalMutationRow, rusqlite::Error> {
    Ok(OperationalMutationRow {
        id: row.get(0)?,
        collection_name: row.get(1)?,
        record_key: row.get(2)?,
        op_kind: row.get(3)?,
        payload_json: row.get(4)?,
        source_ref: row.get(5)?,
        created_at: row.get(6)?,
    })
}

pub(super) fn map_operational_current_row(
    row: &rusqlite::Row<'_>,
) -> Result<OperationalCurrentRow, rusqlite::Error> {
    Ok(OperationalCurrentRow {
        collection_name: row.get(0)?,
        record_key: row.get(1)?,
        payload_json: row.get(2)?,
        updated_at: row.get(3)?,
        last_mutation_id: row.get(4)?,
    })
}

fn map_operational_collection_row(
    row: &rusqlite::Row<'_>,
) -> Result<OperationalCollectionRecord, rusqlite::Error> {
    let kind_text: String = row.get(1)?;
    let kind = OperationalCollectionKind::try_from(kind_text.as_str()).map_err(|message| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(io::Error::new(io::ErrorKind::InvalidData, message)),
        )
    })?;
    Ok(OperationalCollectionRecord {
        name: row.get(0)?,
        kind,
        schema_json: row.get(2)?,
        retention_json: row.get(3)?,
        filter_fields_json: row.get(4)?,
        validation_json: row.get(5)?,
        secondary_indexes_json: row.get(6)?,
        format_version: row.get(7)?,
        created_at: row.get(8)?,
        disabled_at: row.get(9)?,
    })
}

fn validate_append_only_operational_collection(
    record: &OperationalCollectionRecord,
    operation: &str,
) -> Result<(), EngineError> {
    if record.kind != OperationalCollectionKind::AppendOnlyLog {
        return Err(EngineError::InvalidWrite(format!(
            "operational collection '{}' must be append_only_log to {operation}",
            record.name
        )));
    }
    Ok(())
}

fn ensure_operational_collection_registered(
    conn: &rusqlite::Connection,
    collection_name: &str,
) -> Result<(), EngineError> {
    if load_operational_collection_record(conn, collection_name)?.is_none() {
        return Err(EngineError::InvalidWrite(format!(
            "operational collection '{collection_name}' is not registered"
        )));
    }
    Ok(())
}

fn clear_operational_secondary_index_entries(
    tx: &rusqlite::Transaction<'_>,
    collection_name: &str,
) -> Result<(), EngineError> {
    tx.execute(
        "DELETE FROM operational_secondary_index_entries WHERE collection_name = ?1",
        [collection_name],
    )?;
    Ok(())
}

fn insert_operational_secondary_index_entry(
    tx: &rusqlite::Transaction<'_>,
    collection_name: &str,
    subject_kind: &str,
    mutation_id: &str,
    record_key: &str,
    entry: &crate::operational::OperationalSecondaryIndexEntry,
) -> Result<(), EngineError> {
    tx.execute(
        "INSERT INTO operational_secondary_index_entries \
         (collection_name, index_name, subject_kind, mutation_id, record_key, sort_timestamp, \
          slot1_text, slot1_integer, slot2_text, slot2_integer, slot3_text, slot3_integer) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            collection_name,
            entry.index_name,
            subject_kind,
            mutation_id,
            record_key,
            entry.sort_timestamp,
            entry.slot1_text,
            entry.slot1_integer,
            entry.slot2_text,
            entry.slot2_integer,
            entry.slot3_text,
            entry.slot3_integer,
        ],
    )?;
    Ok(())
}

pub(super) fn rebuild_operational_secondary_index_entries(
    tx: &rusqlite::Transaction<'_>,
    collection_name: &str,
    collection_kind: OperationalCollectionKind,
    indexes: &[OperationalSecondaryIndexDefinition],
) -> Result<(usize, usize), EngineError> {
    clear_operational_secondary_index_entries(tx, collection_name)?;

    let mut mutation_entries_rebuilt = 0usize;
    if collection_kind == OperationalCollectionKind::AppendOnlyLog {
        let mut stmt = tx.prepare(
            "SELECT id, record_key, payload_json FROM operational_mutations \
             WHERE collection_name = ?1 ORDER BY mutation_order",
        )?;
        let rows = stmt
            .query_map([collection_name], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for (mutation_id, record_key, payload_json) in rows {
            for entry in extract_secondary_index_entries_for_mutation(indexes, &payload_json) {
                insert_operational_secondary_index_entry(
                    tx,
                    collection_name,
                    "mutation",
                    &mutation_id,
                    &record_key,
                    &entry,
                )?;
                mutation_entries_rebuilt += 1;
            }
        }
    }

    let mut current_entries_rebuilt = 0usize;
    if collection_kind == OperationalCollectionKind::LatestState {
        let mut stmt = tx.prepare(
            "SELECT record_key, payload_json, updated_at, last_mutation_id FROM operational_current \
             WHERE collection_name = ?1 ORDER BY updated_at DESC, record_key",
        )?;
        let rows = stmt
            .query_map([collection_name], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for (record_key, payload_json, updated_at, last_mutation_id) in rows {
            for entry in
                extract_secondary_index_entries_for_current(indexes, &payload_json, updated_at)
            {
                insert_operational_secondary_index_entry(
                    tx,
                    collection_name,
                    "current",
                    &last_mutation_id,
                    &record_key,
                    &entry,
                )?;
                current_entries_rebuilt += 1;
            }
        }
    }

    Ok((mutation_entries_rebuilt, current_entries_rebuilt))
}

fn operational_read_limit(limit: Option<usize>) -> Result<usize, EngineError> {
    let applied_limit = limit.unwrap_or(DEFAULT_OPERATIONAL_READ_LIMIT);
    if applied_limit == 0 {
        return Err(EngineError::InvalidWrite(
            "operational read limit must be greater than zero".to_owned(),
        ));
    }
    Ok(applied_limit.min(MAX_OPERATIONAL_READ_LIMIT))
}

pub(super) fn parse_operational_filter_fields(
    filter_fields_json: &str,
) -> Result<Vec<OperationalFilterField>, String> {
    let fields: Vec<OperationalFilterField> = serde_json::from_str(filter_fields_json)
        .map_err(|error| format!("invalid filter_fields_json: {error}"))?;
    let mut seen = std::collections::HashSet::new();
    for field in &fields {
        if field.name.trim().is_empty() {
            return Err("filter_fields_json field names must not be empty".to_owned());
        }
        if !seen.insert(field.name.as_str()) {
            return Err(format!(
                "filter_fields_json contains duplicate field '{}'",
                field.name
            ));
        }
        if field.modes.is_empty() {
            return Err(format!(
                "filter_fields_json field '{}' must declare at least one mode",
                field.name
            ));
        }
        if field.modes.contains(&OperationalFilterMode::Prefix)
            && field.field_type != OperationalFilterFieldType::String
        {
            return Err(format!(
                "filter field '{}' only supports prefix for string types",
                field.name
            ));
        }
    }
    Ok(fields)
}

fn compile_operational_read_filters(
    filters: &[OperationalFilterClause],
    declared_fields: &[OperationalFilterField],
) -> Result<Vec<CompiledOperationalReadFilter>, EngineError> {
    let field_map = declared_fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect::<std::collections::HashMap<_, _>>();
    filters
        .iter()
        .map(|filter| match filter {
            OperationalFilterClause::Exact { field, value } => {
                let declared = field_map.get(field.as_str()).ok_or_else(|| {
                    EngineError::InvalidWrite(format!(
                        "operational read filter uses undeclared field '{field}'"
                    ))
                })?;
                if !declared.modes.contains(&OperationalFilterMode::Exact) {
                    return Err(EngineError::InvalidWrite(format!(
                        "operational read field '{field}' does not allow exact filters"
                    )));
                }
                let condition = match (declared.field_type, value) {
                    (OperationalFilterFieldType::String, OperationalFilterValue::String(value)) => {
                        OperationalReadCondition::ExactString(value.clone())
                    }
                    (
                        OperationalFilterFieldType::Integer | OperationalFilterFieldType::Timestamp,
                        OperationalFilterValue::Integer(value),
                    ) => OperationalReadCondition::ExactInteger(*value),
                    _ => {
                        return Err(EngineError::InvalidWrite(format!(
                            "operational read field '{field}' received a value with the wrong type"
                        )));
                    }
                };
                Ok(CompiledOperationalReadFilter {
                    field: field.clone(),
                    condition,
                })
            }
            OperationalFilterClause::Prefix { field, value } => {
                let declared = field_map.get(field.as_str()).ok_or_else(|| {
                    EngineError::InvalidWrite(format!(
                        "operational read filter uses undeclared field '{field}'"
                    ))
                })?;
                if !declared.modes.contains(&OperationalFilterMode::Prefix) {
                    return Err(EngineError::InvalidWrite(format!(
                        "operational read field '{field}' does not allow prefix filters"
                    )));
                }
                if declared.field_type != OperationalFilterFieldType::String {
                    return Err(EngineError::InvalidWrite(format!(
                        "operational read field '{field}' only supports prefix filters for strings"
                    )));
                }
                Ok(CompiledOperationalReadFilter {
                    field: field.clone(),
                    condition: OperationalReadCondition::Prefix(value.clone()),
                })
            }
            OperationalFilterClause::Range {
                field,
                lower,
                upper,
            } => {
                let declared = field_map.get(field.as_str()).ok_or_else(|| {
                    EngineError::InvalidWrite(format!(
                        "operational read filter uses undeclared field '{field}'"
                    ))
                })?;
                if !declared.modes.contains(&OperationalFilterMode::Range) {
                    return Err(EngineError::InvalidWrite(format!(
                        "operational read field '{field}' does not allow range filters"
                    )));
                }
                if !matches!(
                    declared.field_type,
                    OperationalFilterFieldType::Integer | OperationalFilterFieldType::Timestamp
                ) {
                    return Err(EngineError::InvalidWrite(format!(
                        "operational read field '{field}' only supports range filters for integer/timestamp fields"
                    )));
                }
                if lower.is_none() && upper.is_none() {
                    return Err(EngineError::InvalidWrite(format!(
                        "operational read range filter for '{field}' must specify a lower or upper bound"
                    )));
                }
                Ok(CompiledOperationalReadFilter {
                    field: field.clone(),
                    condition: OperationalReadCondition::Range {
                        lower: *lower,
                        upper: *upper,
                    },
                })
            }
        })
        .collect()
}

fn match_append_only_secondary_index_read<'a>(
    filters: &'a [CompiledOperationalReadFilter],
    indexes: &'a [OperationalSecondaryIndexDefinition],
) -> Option<MatchedAppendOnlySecondaryIndexRead<'a>> {
    indexes.iter().find_map(|index| {
        let OperationalSecondaryIndexDefinition::AppendOnlyFieldTime {
            name,
            field,
            value_type,
            time_field,
        } = index
        else {
            return None;
        };
        if !(1..=2).contains(&filters.len()) {
            return None;
        }

        let mut value_filter = None;
        let mut time_range = None;
        for filter in filters {
            if filter.field == *field {
                let supported = matches!(
                    (&filter.condition, value_type),
                    (
                        OperationalReadCondition::ExactString(_)
                            | OperationalReadCondition::Prefix(_),
                        crate::operational::OperationalSecondaryIndexValueType::String
                    ) | (
                        OperationalReadCondition::ExactInteger(_),
                        crate::operational::OperationalSecondaryIndexValueType::Integer
                            | crate::operational::OperationalSecondaryIndexValueType::Timestamp
                    )
                );
                if !supported || value_filter.is_some() {
                    return None;
                }
                value_filter = Some(filter);
                continue;
            }
            if filter.field == *time_field {
                if !matches!(filter.condition, OperationalReadCondition::Range { .. })
                    || time_range.is_some()
                {
                    return None;
                }
                time_range = Some(filter);
                continue;
            }
            return None;
        }

        value_filter.map(|value_filter| MatchedAppendOnlySecondaryIndexRead {
            index_name: name.as_str(),
            value_filter,
            time_range,
        })
    })
}

fn execute_operational_secondary_index_read(
    conn: &rusqlite::Connection,
    collection_name: &str,
    filters: &[CompiledOperationalReadFilter],
    indexes: &[OperationalSecondaryIndexDefinition],
    applied_limit: usize,
) -> Result<Option<OperationalReadReport>, EngineError> {
    use rusqlite::types::Value;

    let Some(matched) = match_append_only_secondary_index_read(filters, indexes) else {
        return Ok(None);
    };

    let mut sql = String::from(
        "SELECT m.id, m.collection_name, m.record_key, m.op_kind, m.payload_json, m.source_ref, m.created_at \
         FROM operational_secondary_index_entries s \
         JOIN operational_mutations m ON m.id = s.mutation_id \
         WHERE s.collection_name = ?1 AND s.index_name = ?2 AND s.subject_kind = 'mutation' ",
    );
    let mut params = vec![
        Value::from(collection_name.to_owned()),
        Value::from(matched.index_name.to_owned()),
    ];

    match &matched.value_filter.condition {
        OperationalReadCondition::ExactString(value) => {
            let _ = write!(sql, "AND s.slot1_text = ?{} ", params.len() + 1);
            params.push(Value::from(value.clone()));
        }
        OperationalReadCondition::Prefix(value) => {
            let _ = write!(sql, "AND s.slot1_text GLOB ?{} ", params.len() + 1);
            params.push(Value::from(glob_prefix_pattern(value)));
        }
        OperationalReadCondition::ExactInteger(value) => {
            let _ = write!(sql, "AND s.slot1_integer = ?{} ", params.len() + 1);
            params.push(Value::from(*value));
        }
        OperationalReadCondition::Range { .. } => return Ok(None),
    }

    if let Some(time_range) = matched.time_range
        && let OperationalReadCondition::Range { lower, upper } = &time_range.condition
    {
        if let Some(lower) = lower {
            let _ = write!(sql, "AND s.sort_timestamp >= ?{} ", params.len() + 1);
            params.push(Value::from(*lower));
        }
        if let Some(upper) = upper {
            let _ = write!(sql, "AND s.sort_timestamp <= ?{} ", params.len() + 1);
            params.push(Value::from(*upper));
        }
    }

    let _ = write!(
        sql,
        "ORDER BY s.sort_timestamp DESC, m.mutation_order DESC LIMIT ?{}",
        params.len() + 1
    );
    params.push(Value::from(i64::try_from(applied_limit + 1).map_err(
        |_| EngineError::Bridge("operational read limit overflow".to_owned()),
    )?));

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt
        .query_map(
            rusqlite::params_from_iter(params),
            map_operational_mutation_row,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let was_limited = rows.len() > applied_limit;
    if was_limited {
        rows.truncate(applied_limit);
    }

    Ok(Some(OperationalReadReport {
        collection_name: collection_name.to_owned(),
        row_count: rows.len(),
        applied_limit,
        was_limited,
        rows,
    }))
}

fn execute_operational_filtered_read(
    conn: &rusqlite::Connection,
    collection_name: &str,
    filters: &[CompiledOperationalReadFilter],
    applied_limit: usize,
) -> Result<OperationalReadReport, EngineError> {
    use rusqlite::types::Value;

    let mut sql = String::from(
        "SELECT m.id, m.collection_name, m.record_key, m.op_kind, m.payload_json, m.source_ref, m.created_at \
         FROM operational_mutations m ",
    );
    let mut params = vec![Value::from(collection_name.to_owned())];
    for (index, filter) in filters.iter().enumerate() {
        let _ = write!(
            sql,
            "JOIN operational_filter_values f{index} \
             ON f{index}.mutation_id = m.id \
            AND f{index}.collection_name = m.collection_name "
        );
        match &filter.condition {
            OperationalReadCondition::ExactString(value) => {
                let _ = write!(
                    sql,
                    "AND f{index}.field_name = ?{} AND f{index}.string_value = ?{} ",
                    params.len() + 1,
                    params.len() + 2
                );
                params.push(Value::from(filter.field.clone()));
                params.push(Value::from(value.clone()));
            }
            OperationalReadCondition::ExactInteger(value) => {
                let _ = write!(
                    sql,
                    "AND f{index}.field_name = ?{} AND f{index}.integer_value = ?{} ",
                    params.len() + 1,
                    params.len() + 2
                );
                params.push(Value::from(filter.field.clone()));
                params.push(Value::from(*value));
            }
            OperationalReadCondition::Prefix(value) => {
                let _ = write!(
                    sql,
                    "AND f{index}.field_name = ?{} AND f{index}.string_value GLOB ?{} ",
                    params.len() + 1,
                    params.len() + 2
                );
                params.push(Value::from(filter.field.clone()));
                params.push(Value::from(glob_prefix_pattern(value)));
            }
            OperationalReadCondition::Range { lower, upper } => {
                let _ = write!(sql, "AND f{index}.field_name = ?{} ", params.len() + 1);
                params.push(Value::from(filter.field.clone()));
                if let Some(lower) = lower {
                    let _ = write!(sql, "AND f{index}.integer_value >= ?{} ", params.len() + 1);
                    params.push(Value::from(*lower));
                }
                if let Some(upper) = upper {
                    let _ = write!(sql, "AND f{index}.integer_value <= ?{} ", params.len() + 1);
                    params.push(Value::from(*upper));
                }
            }
        }
    }
    let _ = write!(
        sql,
        "WHERE m.collection_name = ?1 ORDER BY m.mutation_order DESC LIMIT ?{}",
        params.len() + 1
    );
    params.push(Value::from(i64::try_from(applied_limit + 1).map_err(
        |_| EngineError::Bridge("operational read limit overflow".to_owned()),
    )?));

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt
        .query_map(
            rusqlite::params_from_iter(params),
            map_operational_mutation_row,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let was_limited = rows.len() > applied_limit;
    if was_limited {
        rows.truncate(applied_limit);
    }
    Ok(OperationalReadReport {
        collection_name: collection_name.to_owned(),
        row_count: rows.len(),
        applied_limit,
        was_limited,
        rows,
    })
}

fn glob_prefix_pattern(value: &str) -> String {
    let mut pattern = String::with_capacity(value.len() + 1);
    for ch in value.chars() {
        match ch {
            '*' => pattern.push_str("[*]"),
            '?' => pattern.push_str("[?]"),
            '[' => pattern.push_str("[[]"),
            _ => pattern.push(ch),
        }
    }
    pattern.push('*');
    pattern
}

fn extract_operational_filter_values(
    filter_fields: &[OperationalFilterField],
    payload_json: &str,
) -> Vec<ExtractedOperationalFilterValue> {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(payload_json) else {
        return Vec::new();
    };
    let Some(object) = parsed.as_object() else {
        return Vec::new();
    };

    filter_fields
        .iter()
        .filter_map(|field| {
            let value = object.get(&field.name)?;
            match field.field_type {
                OperationalFilterFieldType::String => {
                    value
                        .as_str()
                        .map(|string_value| ExtractedOperationalFilterValue {
                            field_name: field.name.clone(),
                            string_value: Some(string_value.to_owned()),
                            integer_value: None,
                        })
                }
                OperationalFilterFieldType::Integer | OperationalFilterFieldType::Timestamp => {
                    value
                        .as_i64()
                        .map(|integer_value| ExtractedOperationalFilterValue {
                            field_name: field.name.clone(),
                            string_value: None,
                            integer_value: Some(integer_value),
                        })
                }
            }
        })
        .collect()
}

fn operational_compaction_candidates(
    conn: &rusqlite::Connection,
    retention_json: &str,
    collection_name: &str,
) -> Result<(Vec<String>, Option<i64>), EngineError> {
    operational_compaction_candidates_at(
        conn,
        retention_json,
        collection_name,
        current_unix_timestamp()?,
    )
}

fn operational_compaction_candidates_at(
    conn: &rusqlite::Connection,
    retention_json: &str,
    collection_name: &str,
    now_timestamp: i64,
) -> Result<(Vec<String>, Option<i64>), EngineError> {
    let policy = parse_operational_retention_policy(retention_json)?;
    match policy {
        OperationalRetentionPolicy::KeepAll => Ok((Vec::new(), None)),
        OperationalRetentionPolicy::PurgeBeforeSeconds { max_age_seconds } => {
            let before_timestamp = now_timestamp - max_age_seconds;
            let mut stmt = conn.prepare(
                "SELECT id FROM operational_mutations \
                 WHERE collection_name = ?1 AND created_at < ?2 \
                 ORDER BY mutation_order",
            )?;
            let mutation_ids = stmt
                .query_map(
                    rusqlite::params![collection_name, before_timestamp],
                    |row| row.get::<_, String>(0),
                )?
                .collect::<Result<Vec<_>, _>>()?;
            Ok((mutation_ids, Some(before_timestamp)))
        }
        OperationalRetentionPolicy::KeepLast { max_rows } => {
            let mut stmt = conn.prepare(
                "SELECT id FROM operational_mutations \
                 WHERE collection_name = ?1 \
                 ORDER BY mutation_order DESC",
            )?;
            let ordered_ids = stmt
                .query_map([collection_name], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok((ordered_ids.into_iter().skip(max_rows).collect(), None))
        }
    }
}

fn parse_operational_retention_policy(
    retention_json: &str,
) -> Result<OperationalRetentionPolicy, EngineError> {
    let policy: OperationalRetentionPolicy = serde_json::from_str(retention_json)
        .map_err(|error| EngineError::InvalidWrite(format!("invalid retention_json: {error}")))?;
    match policy {
        OperationalRetentionPolicy::KeepAll => Ok(policy),
        OperationalRetentionPolicy::PurgeBeforeSeconds { max_age_seconds } => {
            if max_age_seconds <= 0 {
                return Err(EngineError::InvalidWrite(
                    "retention_json max_age_seconds must be greater than zero".to_owned(),
                ));
            }
            Ok(policy)
        }
        OperationalRetentionPolicy::KeepLast { max_rows } => {
            if max_rows == 0 {
                return Err(EngineError::InvalidWrite(
                    "retention_json max_rows must be greater than zero".to_owned(),
                ));
            }
            Ok(policy)
        }
    }
}

fn load_operational_retention_records(
    conn: &rusqlite::Connection,
    collection_names: Option<&[String]>,
    max_collections: Option<usize>,
) -> Result<Vec<OperationalCollectionRecord>, EngineError> {
    let limit = max_collections.unwrap_or(usize::MAX);
    if limit == 0 {
        return Err(EngineError::InvalidWrite(
            "max_collections must be greater than zero".to_owned(),
        ));
    }

    let mut records = Vec::new();
    if let Some(collection_names) = collection_names {
        for name in collection_names.iter().take(limit) {
            let record = load_operational_collection_record(conn, name)?.ok_or_else(|| {
                EngineError::InvalidWrite(format!(
                    "operational collection '{name}' is not registered"
                ))
            })?;
            records.push(record);
        }
        return Ok(records);
    }

    let mut stmt = conn.prepare(
        "SELECT name, kind, schema_json, retention_json, filter_fields_json, validation_json, secondary_indexes_json, format_version, created_at, disabled_at \
         FROM operational_collections ORDER BY name",
    )?;
    let rows = stmt
        .query_map([], map_operational_collection_row)?
        .take(limit)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn last_operational_retention_run_at(
    conn: &rusqlite::Connection,
    collection_name: &str,
) -> Result<Option<i64>, EngineError> {
    conn.query_row(
        "SELECT MAX(executed_at) FROM operational_retention_runs WHERE collection_name = ?1",
        [collection_name],
        |row| row.get(0),
    )
    .optional()
    .map_err(EngineError::Sqlite)
    .map(Option::flatten)
}

fn count_operational_mutations_for_collection(
    conn: &rusqlite::Connection,
    collection_name: &str,
) -> Result<usize, EngineError> {
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM operational_mutations WHERE collection_name = ?1",
        [collection_name],
        |row| row.get(0),
    )?;
    usize::try_from(count).map_err(|_| {
        EngineError::Bridge(format!("count overflow for collection {collection_name}"))
    })
}

fn retention_action_kind_and_limit(
    policy: &OperationalRetentionPolicy,
) -> (OperationalRetentionActionKind, Option<usize>) {
    match policy {
        OperationalRetentionPolicy::KeepAll => (OperationalRetentionActionKind::Noop, None),
        OperationalRetentionPolicy::PurgeBeforeSeconds { .. } => {
            (OperationalRetentionActionKind::PurgeBeforeSeconds, None)
        }
        OperationalRetentionPolicy::KeepLast { max_rows } => {
            (OperationalRetentionActionKind::KeepLast, Some(*max_rows))
        }
    }
}

fn plan_operational_retention_item(
    conn: &rusqlite::Connection,
    record: &OperationalCollectionRecord,
    now_timestamp: i64,
) -> Result<OperationalRetentionPlanItem, EngineError> {
    let last_run_at = last_operational_retention_run_at(conn, &record.name)?;
    if record.kind != OperationalCollectionKind::AppendOnlyLog {
        return Ok(OperationalRetentionPlanItem {
            collection_name: record.name.clone(),
            action_kind: OperationalRetentionActionKind::Noop,
            candidate_deletions: 0,
            before_timestamp: None,
            max_rows: None,
            last_run_at,
        });
    }
    let policy = parse_operational_retention_policy(&record.retention_json)?;
    let (action_kind, max_rows) = retention_action_kind_and_limit(&policy);
    let (candidate_ids, before_timestamp) = operational_compaction_candidates_at(
        conn,
        &record.retention_json,
        &record.name,
        now_timestamp,
    )?;
    Ok(OperationalRetentionPlanItem {
        collection_name: record.name.clone(),
        action_kind,
        candidate_deletions: candidate_ids.len(),
        before_timestamp,
        max_rows,
        last_run_at,
    })
}

fn run_operational_retention_item(
    tx: &rusqlite::Transaction<'_>,
    record: &OperationalCollectionRecord,
    now_timestamp: i64,
    dry_run: bool,
) -> Result<OperationalRetentionRunItem, EngineError> {
    let plan = plan_operational_retention_item(tx, record, now_timestamp)?;
    let mut deleted_mutations = 0usize;
    if record.kind == OperationalCollectionKind::AppendOnlyLog
        && plan.action_kind != OperationalRetentionActionKind::Noop
        && plan.candidate_deletions > 0
        && !dry_run
    {
        let (candidate_ids, _) = operational_compaction_candidates_at(
            tx,
            &record.retention_json,
            &record.name,
            now_timestamp,
        )?;
        let mut delete_stmt =
            tx.prepare_cached("DELETE FROM operational_mutations WHERE id = ?1")?;
        for mutation_id in &candidate_ids {
            delete_stmt.execute([mutation_id.as_str()])?;
            deleted_mutations += 1;
        }
        drop(delete_stmt);

        persist_simple_provenance_event(
            tx,
            "operational_retention_run",
            &record.name,
            Some(serde_json::json!({
                "action_kind": plan.action_kind,
                "deleted_mutations": deleted_mutations,
                "before_timestamp": plan.before_timestamp,
                "max_rows": plan.max_rows,
                "executed_at": now_timestamp,
            })),
        )?;
    }

    let live_rows_remaining = count_operational_mutations_for_collection(tx, &record.name)?;
    let effective_deleted_mutations = if dry_run {
        plan.candidate_deletions
    } else {
        deleted_mutations
    };
    let rows_remaining = if dry_run {
        live_rows_remaining.saturating_sub(effective_deleted_mutations)
    } else {
        live_rows_remaining
    };
    if !dry_run && plan.action_kind != OperationalRetentionActionKind::Noop {
        tx.execute(
            "INSERT INTO operational_retention_runs \
             (id, collection_name, executed_at, action_kind, dry_run, deleted_mutations, rows_remaining, metadata_json) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                new_id(),
                record.name,
                now_timestamp,
                serde_json::to_string(&plan.action_kind)
                    .unwrap_or_else(|_| "\"noop\"".to_owned())
                    .trim_matches('"')
                    .to_owned(),
                i32::from(dry_run),
                deleted_mutations,
                rows_remaining,
                serde_json::json!({
                    "before_timestamp": plan.before_timestamp,
                    "max_rows": plan.max_rows,
                })
                .to_string(),
            ],
        )?;
    }

    Ok(OperationalRetentionRunItem {
        collection_name: plan.collection_name,
        action_kind: plan.action_kind,
        deleted_mutations: effective_deleted_mutations,
        before_timestamp: plan.before_timestamp,
        max_rows: plan.max_rows,
        rows_remaining,
    })
}

fn current_unix_timestamp() -> Result<i64, EngineError> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|error| EngineError::Bridge(format!("system clock error: {error}")))?;
    i64::try_from(now.as_secs())
        .map_err(|_| EngineError::Bridge("unix timestamp overflow".to_owned()))
}
