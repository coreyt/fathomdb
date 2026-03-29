use serde::{Deserialize, Serialize};

fn default_filter_fields_json() -> String {
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
    pub format_version: i64,
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
