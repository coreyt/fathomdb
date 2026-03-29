use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

fn default_filter_fields_json() -> String {
    "[]".to_owned()
}

fn default_validation_json() -> String {
    String::new()
}

fn default_secondary_indexes_json() -> String {
    "[]".to_owned()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalCollectionKind {
    AppendOnlyLog,
    LatestState,
}

impl OperationalCollectionKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AppendOnlyLog => "append_only_log",
            Self::LatestState => "latest_state",
        }
    }
}

impl TryFrom<&str> for OperationalCollectionKind {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "append_only_log" => Ok(Self::AppendOnlyLog),
            "latest_state" => Ok(Self::LatestState),
            other => Err(format!("unknown operational collection kind '{other}'")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalCollectionRecord {
    pub name: String,
    pub kind: OperationalCollectionKind,
    pub schema_json: String,
    pub retention_json: String,
    #[serde(default = "default_filter_fields_json")]
    pub filter_fields_json: String,
    #[serde(default = "default_validation_json")]
    pub validation_json: String,
    #[serde(default = "default_secondary_indexes_json")]
    pub secondary_indexes_json: String,
    pub format_version: i64,
    pub created_at: i64,
    pub disabled_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalMutationRow {
    pub id: String,
    pub collection_name: String,
    pub record_key: String,
    pub op_kind: String,
    pub payload_json: String,
    pub source_ref: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalCurrentRow {
    pub collection_name: String,
    pub record_key: String,
    pub payload_json: String,
    pub updated_at: i64,
    pub last_mutation_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalRegisterRequest {
    pub name: String,
    pub kind: OperationalCollectionKind,
    pub schema_json: String,
    pub retention_json: String,
    #[serde(default = "default_filter_fields_json")]
    pub filter_fields_json: String,
    #[serde(default = "default_validation_json")]
    pub validation_json: String,
    #[serde(default = "default_secondary_indexes_json")]
    pub secondary_indexes_json: String,
    pub format_version: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalValidationMode {
    Disabled,
    ReportOnly,
    Enforce,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperationalValidationContract {
    pub format_version: i64,
    pub mode: OperationalValidationMode,
    #[serde(default = "default_true")]
    pub additional_properties: bool,
    #[serde(default)]
    pub fields: Vec<OperationalValidationField>,
}

const fn default_true() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationalValidationFieldType {
    #[serde(rename = "string")]
    String,
    #[serde(rename = "integer")]
    Integer,
    #[serde(rename = "float")]
    Float,
    #[serde(rename = "boolean")]
    Boolean,
    #[serde(rename = "timestamp")]
    Timestamp,
    #[serde(rename = "object")]
    Object,
    #[serde(rename = "array[string]")]
    ArrayString,
    #[serde(rename = "array[integer]")]
    ArrayInteger,
    #[serde(rename = "array[float]")]
    ArrayFloat,
    #[serde(rename = "array[boolean]")]
    ArrayBoolean,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperationalValidationField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: OperationalValidationFieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default, rename = "enum")]
    pub enum_values: Vec<serde_json::Value>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub max_length: Option<usize>,
    pub max_items: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalHistoryValidationIssue {
    pub mutation_id: String,
    pub record_key: String,
    pub op_kind: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalHistoryValidationReport {
    pub collection_name: String,
    pub checked_rows: usize,
    pub invalid_row_count: usize,
    pub issues: Vec<OperationalHistoryValidationIssue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalSecondaryIndexValueType {
    String,
    Integer,
    Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalSecondaryIndexField {
    pub name: String,
    pub value_type: OperationalSecondaryIndexValueType,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OperationalSecondaryIndexDefinition {
    AppendOnlyFieldTime {
        name: String,
        field: String,
        value_type: OperationalSecondaryIndexValueType,
        time_field: String,
    },
    LatestStateField {
        name: String,
        field: String,
        value_type: OperationalSecondaryIndexValueType,
    },
    LatestStateComposite {
        name: String,
        fields: Vec<OperationalSecondaryIndexField>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OperationalSecondaryIndexEntry {
    pub index_name: String,
    pub sort_timestamp: Option<i64>,
    pub slot1_text: Option<String>,
    pub slot1_integer: Option<i64>,
    pub slot2_text: Option<String>,
    pub slot2_integer: Option<i64>,
    pub slot3_text: Option<String>,
    pub slot3_integer: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalSecondaryIndexRebuildReport {
    pub collection_name: String,
    pub mutation_entries_rebuilt: usize,
    pub current_entries_rebuilt: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalRetentionActionKind {
    Noop,
    PurgeBeforeSeconds,
    KeepLast,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalRetentionPlanItem {
    pub collection_name: String,
    pub action_kind: OperationalRetentionActionKind,
    pub candidate_deletions: usize,
    pub before_timestamp: Option<i64>,
    pub max_rows: Option<usize>,
    pub last_run_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalRetentionPlanReport {
    pub planned_at: i64,
    pub collections_examined: usize,
    pub items: Vec<OperationalRetentionPlanItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalRetentionRunItem {
    pub collection_name: String,
    pub action_kind: OperationalRetentionActionKind,
    pub deleted_mutations: usize,
    pub before_timestamp: Option<i64>,
    pub max_rows: Option<usize>,
    pub rows_remaining: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalRetentionRunReport {
    pub executed_at: i64,
    pub collections_examined: usize,
    pub collections_acted_on: usize,
    pub dry_run: bool,
    pub items: Vec<OperationalRetentionRunItem>,
}

pub(crate) fn parse_operational_validation_contract(
    validation_json: &str,
) -> Result<Option<OperationalValidationContract>, String> {
    if validation_json.is_empty() {
        return Ok(None);
    }
    let contract: OperationalValidationContract = serde_json::from_str(validation_json)
        .map_err(|error| format!("invalid validation_json: {error}"))?;
    validate_operational_validation_contract(&contract)?;
    Ok(Some(contract))
}

pub(crate) fn parse_operational_secondary_indexes_json(
    secondary_indexes_json: &str,
    collection_kind: OperationalCollectionKind,
) -> Result<Vec<OperationalSecondaryIndexDefinition>, String> {
    let secondary_indexes_json = if secondary_indexes_json.is_empty() {
        "[]"
    } else {
        secondary_indexes_json
    };
    let indexes: Vec<OperationalSecondaryIndexDefinition> =
        serde_json::from_str(secondary_indexes_json)
            .map_err(|error| format!("invalid secondary_indexes_json: {error}"))?;
    validate_operational_secondary_indexes(&indexes, collection_kind)?;
    Ok(indexes)
}

pub(crate) fn validate_operational_payload_against_contract(
    contract: &OperationalValidationContract,
    payload_json: &str,
) -> Result<(), String> {
    let payload: Value = serde_json::from_str(payload_json)
        .map_err(|error| format!("payload_json is not valid JSON: {error}"))?;
    validate_operational_payload_value(contract, &payload)
}

fn validate_operational_validation_contract(
    contract: &OperationalValidationContract,
) -> Result<(), String> {
    if contract.format_version != 1 {
        return Err("validation_json format_version must be 1".to_owned());
    }

    let mut seen = HashSet::new();
    for field in &contract.fields {
        if field.name.trim().is_empty() {
            return Err("validation_json field names must not be empty".to_owned());
        }
        if !seen.insert(field.name.as_str()) {
            return Err(format!(
                "validation_json contains duplicate field '{}'",
                field.name
            ));
        }
        validate_operational_validation_field(field)?;
    }
    Ok(())
}

fn validate_operational_secondary_indexes(
    indexes: &[OperationalSecondaryIndexDefinition],
    collection_kind: OperationalCollectionKind,
) -> Result<(), String> {
    let mut seen = HashSet::new();
    for index in indexes {
        let name = index.name();
        if name.trim().is_empty() {
            return Err("secondary_indexes_json index names must not be empty".to_owned());
        }
        if !seen.insert(name) {
            return Err(format!(
                "secondary_indexes_json contains duplicate index '{name}'"
            ));
        }
        validate_operational_secondary_index(index, collection_kind)?;
    }
    Ok(())
}

fn validate_operational_secondary_index(
    index: &OperationalSecondaryIndexDefinition,
    collection_kind: OperationalCollectionKind,
) -> Result<(), String> {
    match index {
        OperationalSecondaryIndexDefinition::AppendOnlyFieldTime {
            field, time_field, ..
        } => {
            if collection_kind != OperationalCollectionKind::AppendOnlyLog {
                return Err(format!(
                    "secondary index '{}' only supports append_only_log collections",
                    index.name()
                ));
            }
            if field.trim().is_empty() || time_field.trim().is_empty() {
                return Err(format!(
                    "secondary index '{}' field names must not be empty",
                    index.name()
                ));
            }
        }
        OperationalSecondaryIndexDefinition::LatestStateField { field, .. } => {
            if collection_kind != OperationalCollectionKind::LatestState {
                return Err(format!(
                    "secondary index '{}' only supports latest_state collections",
                    index.name()
                ));
            }
            if field.trim().is_empty() {
                return Err(format!(
                    "secondary index '{}' field names must not be empty",
                    index.name()
                ));
            }
        }
        OperationalSecondaryIndexDefinition::LatestStateComposite { fields, .. } => {
            if collection_kind != OperationalCollectionKind::LatestState {
                return Err(format!(
                    "secondary index '{}' only supports latest_state collections",
                    index.name()
                ));
            }
            if fields.is_empty() || fields.len() > 3 {
                return Err(format!(
                    "secondary index '{}' must declare between 1 and 3 fields",
                    index.name()
                ));
            }
            let mut seen = HashSet::new();
            for field in fields {
                if field.name.trim().is_empty() {
                    return Err(format!(
                        "secondary index '{}' field names must not be empty",
                        index.name()
                    ));
                }
                if !seen.insert(field.name.as_str()) {
                    return Err(format!(
                        "secondary index '{}' contains duplicate field '{}'",
                        index.name(),
                        field.name
                    ));
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn validate_operational_validation_field(field: &OperationalValidationField) -> Result<(), String> {
    if let (Some(minimum), Some(maximum)) = (field.minimum, field.maximum)
        && minimum > maximum
    {
        return Err(format!(
            "validation field '{}' minimum must be less than or equal to maximum",
            field.name
        ));
    }

    match field.field_type {
        OperationalValidationFieldType::String => {
            if field.minimum.is_some() || field.maximum.is_some() {
                return Err(format!(
                    "validation field '{}' only supports minimum/maximum for integer, float, and timestamp types",
                    field.name
                ));
            }
            if field.max_items.is_some() {
                return Err(format!(
                    "validation field '{}' only supports max_items for array types",
                    field.name
                ));
            }
        }
        OperationalValidationFieldType::Integer | OperationalValidationFieldType::Timestamp => {
            if field.max_length.is_some() {
                return Err(format!(
                    "validation field '{}' only supports max_length for string types",
                    field.name
                ));
            }
            if field.max_items.is_some() {
                return Err(format!(
                    "validation field '{}' only supports max_items for array types",
                    field.name
                ));
            }
            if let Some(minimum) = field.minimum
                && minimum.fract() != 0.0
            {
                return Err(format!(
                    "validation field '{}' minimum must be an integer for {}",
                    field.name,
                    field.field_type.as_str()
                ));
            }
            if let Some(maximum) = field.maximum
                && maximum.fract() != 0.0
            {
                return Err(format!(
                    "validation field '{}' maximum must be an integer for {}",
                    field.name,
                    field.field_type.as_str()
                ));
            }
        }
        OperationalValidationFieldType::Float => {
            if field.max_length.is_some() {
                return Err(format!(
                    "validation field '{}' only supports max_length for string types",
                    field.name
                ));
            }
            if field.max_items.is_some() {
                return Err(format!(
                    "validation field '{}' only supports max_items for array types",
                    field.name
                ));
            }
        }
        OperationalValidationFieldType::Boolean | OperationalValidationFieldType::Object => {
            if field.minimum.is_some() || field.maximum.is_some() {
                return Err(format!(
                    "validation field '{}' only supports minimum/maximum for integer, float, and timestamp types",
                    field.name
                ));
            }
            if field.max_length.is_some() {
                return Err(format!(
                    "validation field '{}' only supports max_length for string types",
                    field.name
                ));
            }
            if field.max_items.is_some() {
                return Err(format!(
                    "validation field '{}' only supports max_items for array types",
                    field.name
                ));
            }
        }
        OperationalValidationFieldType::ArrayString
        | OperationalValidationFieldType::ArrayInteger
        | OperationalValidationFieldType::ArrayFloat
        | OperationalValidationFieldType::ArrayBoolean => {
            if field.minimum.is_some() || field.maximum.is_some() {
                return Err(format!(
                    "validation field '{}' only supports minimum/maximum for integer, float, and timestamp types",
                    field.name
                ));
            }
            if field.max_length.is_some() {
                return Err(format!(
                    "validation field '{}' only supports max_length for string types",
                    field.name
                ));
            }
        }
    }

    if !field.enum_values.is_empty() {
        for value in &field.enum_values {
            if !field.field_type.matches_enum_value(value) {
                return Err(format!(
                    "validation field '{}' has an enum value incompatible with type {}",
                    field.name,
                    field.field_type.as_str()
                ));
            }
        }
    }

    Ok(())
}

fn validate_operational_payload_value(
    contract: &OperationalValidationContract,
    payload: &Value,
) -> Result<(), String> {
    let object = payload
        .as_object()
        .ok_or_else(|| "payload must be a JSON object".to_owned())?;
    let field_map = contract
        .fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect::<HashMap<_, _>>();

    if !contract.additional_properties {
        for key in object.keys() {
            if !field_map.contains_key(key.as_str()) {
                return Err(format!("field '{key}' is not allowed"));
            }
        }
    }

    for field in &contract.fields {
        let Some(value) = object.get(&field.name) else {
            if field.required {
                return Err(format!("field '{}' is required", field.name));
            }
            continue;
        };
        validate_operational_field_value(field, value)?;
    }

    Ok(())
}

#[allow(clippy::cast_precision_loss)]
fn validate_operational_field_value(
    field: &OperationalValidationField,
    value: &Value,
) -> Result<(), String> {
    if value.is_null() {
        if field.nullable {
            return Ok(());
        }
        return Err(format!("field '{}' must not be null", field.name));
    }

    match field.field_type {
        OperationalValidationFieldType::String => {
            let string_value = value
                .as_str()
                .ok_or_else(|| format!("field '{}' must be a string", field.name))?;
            if let Some(max_length) = field.max_length
                && string_value.len() > max_length
            {
                return Err(format!(
                    "field '{}' must have length <= {}",
                    field.name, max_length
                ));
            }
            validate_enum_membership(field, value)?;
        }
        OperationalValidationFieldType::Integer => {
            let integer_value = value
                .as_i64()
                .ok_or_else(|| format!("field '{}' must be an integer", field.name))?;
            validate_numeric_bounds(field, integer_value as f64)?;
            validate_enum_membership(field, value)?;
        }
        OperationalValidationFieldType::Timestamp => {
            let timestamp_value = value
                .as_i64()
                .ok_or_else(|| format!("field '{}' must be a timestamp integer", field.name))?;
            validate_numeric_bounds(field, timestamp_value as f64)?;
            validate_enum_membership(field, value)?;
        }
        OperationalValidationFieldType::Float => {
            let float_value = value
                .as_f64()
                .ok_or_else(|| format!("field '{}' must be a float", field.name))?;
            validate_numeric_bounds(field, float_value)?;
            validate_enum_membership(field, value)?;
        }
        OperationalValidationFieldType::Boolean => {
            value
                .as_bool()
                .ok_or_else(|| format!("field '{}' must be a boolean", field.name))?;
            validate_enum_membership(field, value)?;
        }
        OperationalValidationFieldType::Object => {
            value
                .as_object()
                .ok_or_else(|| format!("field '{}' must be an object", field.name))?;
            validate_enum_membership(field, value)?;
        }
        OperationalValidationFieldType::ArrayString => {
            validate_array(field, value, |item| item.as_str().is_some(), "string")?;
        }
        OperationalValidationFieldType::ArrayInteger => {
            validate_array(field, value, |item| item.as_i64().is_some(), "integer")?;
        }
        OperationalValidationFieldType::ArrayFloat => {
            validate_array(field, value, |item| item.as_f64().is_some(), "float")?;
        }
        OperationalValidationFieldType::ArrayBoolean => {
            validate_array(field, value, |item| item.as_bool().is_some(), "boolean")?;
        }
    }
    Ok(())
}

fn validate_array(
    field: &OperationalValidationField,
    value: &Value,
    predicate: impl Fn(&Value) -> bool,
    expected: &str,
) -> Result<(), String> {
    let items = value
        .as_array()
        .ok_or_else(|| format!("field '{}' must be an array", field.name))?;
    if let Some(max_items) = field.max_items
        && items.len() > max_items
    {
        return Err(format!(
            "field '{}' must have at most {} items",
            field.name, max_items
        ));
    }
    for item in items {
        if !predicate(item) {
            return Err(format!(
                "field '{}' must contain only {} values",
                field.name, expected
            ));
        }
    }
    validate_enum_membership(field, value)?;
    Ok(())
}

fn validate_numeric_bounds(
    field: &OperationalValidationField,
    numeric_value: f64,
) -> Result<(), String> {
    if let Some(minimum) = field.minimum
        && numeric_value < minimum
    {
        return Err(format!("field '{}' must be >= {}", field.name, minimum));
    }
    if let Some(maximum) = field.maximum
        && numeric_value > maximum
    {
        return Err(format!("field '{}' must be <= {}", field.name, maximum));
    }
    Ok(())
}

fn validate_enum_membership(
    field: &OperationalValidationField,
    value: &Value,
) -> Result<(), String> {
    if !field.enum_values.is_empty()
        && !field.enum_values.iter().any(|candidate| candidate == value)
    {
        return Err(format!(
            "field '{}' must be one of {}",
            field.name,
            serde_json::to_string(&field.enum_values).unwrap_or_else(|_| "[]".to_owned())
        ));
    }
    Ok(())
}

impl OperationalValidationFieldType {
    fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Integer => "integer",
            Self::Float => "float",
            Self::Boolean => "boolean",
            Self::Timestamp => "timestamp",
            Self::Object => "object",
            Self::ArrayString => "array[string]",
            Self::ArrayInteger => "array[integer]",
            Self::ArrayFloat => "array[float]",
            Self::ArrayBoolean => "array[boolean]",
        }
    }

    fn matches_enum_value(self, value: &Value) -> bool {
        match self {
            Self::String => value.as_str().is_some(),
            Self::Integer | Self::Timestamp => value.as_i64().is_some(),
            Self::Float => value.as_f64().is_some(),
            Self::Boolean => value.as_bool().is_some(),
            Self::Object => value.as_object().is_some(),
            Self::ArrayString => value
                .as_array()
                .is_some_and(|items| items.iter().all(|item| item.as_str().is_some())),
            Self::ArrayInteger => value
                .as_array()
                .is_some_and(|items| items.iter().all(|item| item.as_i64().is_some())),
            Self::ArrayFloat => value
                .as_array()
                .is_some_and(|items| items.iter().all(|item| item.as_f64().is_some())),
            Self::ArrayBoolean => value
                .as_array()
                .is_some_and(|items| items.iter().all(|item| item.as_bool().is_some())),
        }
    }
}

impl OperationalSecondaryIndexDefinition {
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::AppendOnlyFieldTime { name, .. }
            | Self::LatestStateField { name, .. }
            | Self::LatestStateComposite { name, .. } => name,
        }
    }
}

pub(crate) fn extract_secondary_index_entries_for_mutation(
    indexes: &[OperationalSecondaryIndexDefinition],
    payload_json: &str,
) -> Vec<OperationalSecondaryIndexEntry> {
    let Ok(parsed) = serde_json::from_str::<Value>(payload_json) else {
        return Vec::new();
    };
    let Some(object) = parsed.as_object() else {
        return Vec::new();
    };

    indexes
        .iter()
        .filter_map(|index| match index {
            OperationalSecondaryIndexDefinition::AppendOnlyFieldTime {
                name,
                field,
                value_type,
                time_field,
            } => {
                let sort_timestamp = object.get(time_field)?.as_i64()?;
                let slot1 = extract_secondary_index_slot(object, field, *value_type)?;
                Some(OperationalSecondaryIndexEntry {
                    index_name: name.clone(),
                    sort_timestamp: Some(sort_timestamp),
                    slot1_text: slot1.0,
                    slot1_integer: slot1.1,
                    slot2_text: None,
                    slot2_integer: None,
                    slot3_text: None,
                    slot3_integer: None,
                })
            }
            OperationalSecondaryIndexDefinition::LatestStateField { .. }
            | OperationalSecondaryIndexDefinition::LatestStateComposite { .. } => None,
        })
        .collect()
}

pub(crate) fn extract_secondary_index_entries_for_current(
    indexes: &[OperationalSecondaryIndexDefinition],
    payload_json: &str,
    updated_at: i64,
) -> Vec<OperationalSecondaryIndexEntry> {
    let Ok(parsed) = serde_json::from_str::<Value>(payload_json) else {
        return Vec::new();
    };
    let Some(object) = parsed.as_object() else {
        return Vec::new();
    };

    indexes
        .iter()
        .filter_map(|index| match index {
            OperationalSecondaryIndexDefinition::AppendOnlyFieldTime { .. } => None,
            OperationalSecondaryIndexDefinition::LatestStateField {
                name,
                field,
                value_type,
            } => {
                let slot1 = extract_secondary_index_slot(object, field, *value_type)?;
                Some(OperationalSecondaryIndexEntry {
                    index_name: name.clone(),
                    sort_timestamp: Some(updated_at),
                    slot1_text: slot1.0,
                    slot1_integer: slot1.1,
                    slot2_text: None,
                    slot2_integer: None,
                    slot3_text: None,
                    slot3_integer: None,
                })
            }
            OperationalSecondaryIndexDefinition::LatestStateComposite { name, fields } => {
                let slots = fields
                    .iter()
                    .map(|field| {
                        extract_secondary_index_slot(object, &field.name, field.value_type)
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(OperationalSecondaryIndexEntry {
                    index_name: name.clone(),
                    sort_timestamp: Some(updated_at),
                    slot1_text: slots.first().and_then(|slot| slot.0.clone()),
                    slot1_integer: slots.first().and_then(|slot| slot.1),
                    slot2_text: slots.get(1).and_then(|slot| slot.0.clone()),
                    slot2_integer: slots.get(1).and_then(|slot| slot.1),
                    slot3_text: slots.get(2).and_then(|slot| slot.0.clone()),
                    slot3_integer: slots.get(2).and_then(|slot| slot.1),
                })
            }
        })
        .collect()
}

fn extract_secondary_index_slot(
    object: &serde_json::Map<String, Value>,
    field_name: &str,
    value_type: OperationalSecondaryIndexValueType,
) -> Option<(Option<String>, Option<i64>)> {
    let value = object.get(field_name)?;
    match value_type {
        OperationalSecondaryIndexValueType::String => {
            Some((Some(value.as_str()?.to_owned()), None))
        }
        OperationalSecondaryIndexValueType::Integer
        | OperationalSecondaryIndexValueType::Timestamp => Some((None, Some(value.as_i64()?))),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalFilterMode {
    Exact,
    Prefix,
    Range,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalFilterFieldType {
    String,
    Integer,
    Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalFilterField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: OperationalFilterFieldType,
    pub modes: Vec<OperationalFilterMode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OperationalFilterValue {
    String(String),
    Integer(i64),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum OperationalFilterClause {
    Exact {
        field: String,
        value: OperationalFilterValue,
    },
    Prefix {
        field: String,
        value: String,
    },
    Range {
        field: String,
        lower: Option<i64>,
        upper: Option<i64>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalReadRequest {
    pub collection_name: String,
    pub filters: Vec<OperationalFilterClause>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalReadReport {
    pub collection_name: String,
    pub row_count: usize,
    pub applied_limit: usize,
    pub was_limited: bool,
    pub rows: Vec<OperationalMutationRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalTraceReport {
    pub collection_name: String,
    pub record_key: Option<String>,
    pub mutation_count: usize,
    pub current_count: usize,
    pub mutations: Vec<OperationalMutationRow>,
    pub current_rows: Vec<OperationalCurrentRow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalRepairReport {
    pub collections_rebuilt: usize,
    pub current_rows_rebuilt: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalCompactionReport {
    pub collection_name: String,
    pub deleted_mutations: usize,
    pub dry_run: bool,
    pub before_timestamp: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationalPurgeReport {
    pub collection_name: String,
    pub deleted_mutations: usize,
    pub before_timestamp: i64,
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{
        OperationalValidationContract, OperationalValidationMode,
        parse_operational_validation_contract, validate_operational_payload_against_contract,
    };

    #[test]
    fn parse_validation_contract_accepts_array_enum_members() {
        let contract = parse_operational_validation_contract(
            r#"{"format_version":1,"mode":"enforce","fields":[{"name":"tags","type":"array[string]","enum":[["a"],["b","c"]]}]}"#,
        )
        .expect("contract parses")
        .expect("contract present");

        assert_eq!(contract.mode, OperationalValidationMode::Enforce);
        assert_eq!(contract.fields.len(), 1);
    }

    #[test]
    fn report_only_contract_validates_payload_without_rejecting_contract() {
        let contract = parse_operational_validation_contract(
            r#"{"format_version":1,"mode":"report_only","additional_properties":false,"fields":[{"name":"status","type":"string","required":true,"enum":["ok","failed"]}]}"#,
        )
        .expect("contract parses")
        .expect("contract present");

        assert!(matches!(
            contract.mode,
            OperationalValidationMode::ReportOnly
        ));
        assert!(
            validate_operational_payload_against_contract(&contract, r#"{"status":"bogus"}"#)
                .is_err()
        );
    }

    #[test]
    fn report_only_contract_round_trips_via_serde() {
        let contract: OperationalValidationContract = serde_json::from_str(
            r#"{"format_version":1,"mode":"report_only","additional_properties":true,"fields":[]}"#,
        )
        .expect("deserialize");

        assert!(matches!(
            contract.mode,
            OperationalValidationMode::ReportOnly
        ));
        assert_eq!(
            serde_json::to_string(&contract).expect("serialize"),
            r#"{"format_version":1,"mode":"report_only","additional_properties":true,"fields":[]}"#
        );
    }
}
