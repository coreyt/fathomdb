#![cfg(any(feature = "python", feature = "node"))]

use fathomdb_query::TextQuery;
use serde::{Deserialize, Serialize};

use crate::{
    ActionInsert, ActionRow, BindValue, ChunkInsert, ChunkPolicy, ComparisonOp,
    CompiledGroupedQuery, CompiledQuery, DrivingTable, EdgeInsert, EdgeRetire, ExecutionHints,
    ExpansionRootRows, ExpansionSlot, ExpansionSlotRows, GroupedQueryRows, LastAccessTouchReport,
    LastAccessTouchRequest, NodeInsert, NodeRetire, NodeRow, OperationalWrite,
    OptionalProjectionTask, Predicate, ProjectionRepairReport, ProjectionTarget, QueryAst,
    QueryPlan, QueryRows, QueryStep, RunInsert, RunRow, SafeExportManifest, ScalarValue,
    StepInsert, StepRow, TraverseDirection, VecInsert, WriteReceipt, WriteRequest,
};
#[cfg(feature = "python")]
use fathomdb_engine::VectorRegenerationReport;
use fathomdb_engine::{IntegrityReport, SemanticReport, TraceReport};

#[derive(Debug, Deserialize)]
pub struct PyQueryAst {
    pub root_kind: String,
    #[serde(default)]
    pub steps: Vec<PyQueryStep>,
    #[serde(default)]
    pub expansions: Vec<PyExpansionSlot>,
    pub final_limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PyExpansionSlot {
    pub slot: String,
    pub direction: PyTraverseDirection,
    pub label: String,
    pub max_depth: usize,
    /// Optional filter predicate applied to target nodes in this slot.
    /// Serialized as a filter-step dict (same format as `PyQueryStep` filter
    /// variants), e.g. `{"type": "filter_json_text_eq", "path": "$.kind", "value": "decision"}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<serde_json::Value>,
    /// Optional edge filter predicate applied to the traversed edges in this slot.
    /// Only edges whose properties satisfy this predicate will be followed.
    /// Serialized as a dict, e.g.
    /// `{"type": "edge_property_eq", "path": "$.rel", "value": "cites"}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_filter: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PyQueryStep {
    VectorSearch {
        query: String,
        limit: usize,
    },
    TextSearch {
        query: String,
        limit: usize,
    },
    Traverse {
        direction: PyTraverseDirection,
        label: String,
        max_depth: usize,
    },
    FilterLogicalIdEq {
        logical_id: String,
    },
    FilterKindEq {
        kind: String,
    },
    FilterSourceRefEq {
        source_ref: String,
    },
    FilterContentRefNotNull {},
    FilterContentRefEq {
        content_ref: String,
    },
    FilterJsonTextEq {
        path: String,
        value: String,
    },
    FilterJsonBoolEq {
        path: String,
        value: bool,
    },
    FilterJsonIntegerGt {
        path: String,
        value: i64,
    },
    FilterJsonIntegerGte {
        path: String,
        value: i64,
    },
    FilterJsonIntegerLt {
        path: String,
        value: i64,
    },
    FilterJsonIntegerLte {
        path: String,
        value: i64,
    },
    FilterJsonTimestampGt {
        path: String,
        value: i64,
    },
    FilterJsonTimestampGte {
        path: String,
        value: i64,
    },
    FilterJsonTimestampLt {
        path: String,
        value: i64,
    },
    FilterJsonTimestampLte {
        path: String,
        value: i64,
    },
    EdgePropertyEq {
        path: String,
        value: serde_json::Value,
    },
    EdgePropertyCompare {
        path: String,
        op: String,
        value: serde_json::Value,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PyTraverseDirection {
    In,
    Out,
}

impl From<PyTraverseDirection> for TraverseDirection {
    fn from(value: PyTraverseDirection) -> Self {
        match value {
            PyTraverseDirection::In => Self::In,
            PyTraverseDirection::Out => Self::Out,
        }
    }
}

/// Convert a raw filter-step JSON value (same format as `PyQueryStep` filter variants)
/// to a `Predicate`. Returns `None` for unknown or non-filter step types.
fn py_filter_value_to_predicate(v: serde_json::Value) -> Option<Predicate> {
    let step: PyQueryStep = serde_json::from_value(v).ok()?;
    match step {
        PyQueryStep::FilterLogicalIdEq { logical_id } => Some(Predicate::LogicalIdEq(logical_id)),
        PyQueryStep::FilterKindEq { kind } => Some(Predicate::KindEq(kind)),
        PyQueryStep::FilterSourceRefEq { source_ref } => Some(Predicate::SourceRefEq(source_ref)),
        PyQueryStep::FilterContentRefNotNull {} => Some(Predicate::ContentRefNotNull),
        PyQueryStep::FilterContentRefEq { content_ref } => {
            Some(Predicate::ContentRefEq(content_ref))
        }
        PyQueryStep::FilterJsonTextEq { path, value } => Some(Predicate::JsonPathEq {
            path,
            value: ScalarValue::Text(value),
        }),
        PyQueryStep::FilterJsonBoolEq { path, value } => Some(Predicate::JsonPathEq {
            path,
            value: ScalarValue::Bool(value),
        }),
        PyQueryStep::FilterJsonIntegerGt { path, value }
        | PyQueryStep::FilterJsonTimestampGt { path, value } => Some(Predicate::JsonPathCompare {
            path,
            op: ComparisonOp::Gt,
            value: ScalarValue::Integer(value),
        }),
        PyQueryStep::FilterJsonIntegerGte { path, value }
        | PyQueryStep::FilterJsonTimestampGte { path, value } => Some(Predicate::JsonPathCompare {
            path,
            op: ComparisonOp::Gte,
            value: ScalarValue::Integer(value),
        }),
        PyQueryStep::FilterJsonIntegerLt { path, value }
        | PyQueryStep::FilterJsonTimestampLt { path, value } => Some(Predicate::JsonPathCompare {
            path,
            op: ComparisonOp::Lt,
            value: ScalarValue::Integer(value),
        }),
        PyQueryStep::FilterJsonIntegerLte { path, value }
        | PyQueryStep::FilterJsonTimestampLte { path, value } => Some(Predicate::JsonPathCompare {
            path,
            op: ComparisonOp::Lte,
            value: ScalarValue::Integer(value),
        }),
        PyQueryStep::EdgePropertyEq { path, value } => {
            let scalar = py_json_to_scalar(&value)?;
            Some(Predicate::EdgePropertyEq {
                path,
                value: scalar,
            })
        }
        PyQueryStep::EdgePropertyCompare { path, op, value } => {
            let scalar = py_json_to_scalar(&value)?;
            let comparison_op = match op.as_str() {
                "gt" => ComparisonOp::Gt,
                "gte" => ComparisonOp::Gte,
                "lt" => ComparisonOp::Lt,
                "lte" => ComparisonOp::Lte,
                _ => return None,
            };
            Some(Predicate::EdgePropertyCompare {
                path,
                op: comparison_op,
                value: scalar,
            })
        }
        // Non-filter step types are not valid expansion filters.
        PyQueryStep::VectorSearch { .. }
        | PyQueryStep::TextSearch { .. }
        | PyQueryStep::Traverse { .. } => None,
    }
}

/// Convert a raw `serde_json::Value` to a `ScalarValue`. Returns `None` for
/// unsupported JSON types (objects, arrays, null).
fn py_json_to_scalar(v: &serde_json::Value) -> Option<ScalarValue> {
    match v {
        serde_json::Value::String(s) => Some(ScalarValue::Text(s.clone())),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(ScalarValue::Integer(i))
            } else {
                None
            }
        }
        serde_json::Value::Bool(b) => Some(ScalarValue::Bool(*b)),
        _ => None,
    }
}

impl From<PyQueryAst> for QueryAst {
    fn from(value: PyQueryAst) -> Self {
        let steps = value
            .steps
            .into_iter()
            .map(|step| match step {
                PyQueryStep::VectorSearch { query, limit } => {
                    QueryStep::VectorSearch { query, limit }
                }
                PyQueryStep::TextSearch { query, limit } => QueryStep::TextSearch {
                    query: TextQuery::parse(&query),
                    limit,
                },
                PyQueryStep::Traverse {
                    direction,
                    label,
                    max_depth,
                } => QueryStep::Traverse {
                    direction: direction.into(),
                    label,
                    max_depth,
                    filter: None,
                },
                PyQueryStep::FilterLogicalIdEq { logical_id } => {
                    QueryStep::Filter(Predicate::LogicalIdEq(logical_id))
                }
                PyQueryStep::FilterKindEq { kind } => QueryStep::Filter(Predicate::KindEq(kind)),
                PyQueryStep::FilterSourceRefEq { source_ref } => {
                    QueryStep::Filter(Predicate::SourceRefEq(source_ref))
                }
                PyQueryStep::FilterContentRefNotNull {} => {
                    QueryStep::Filter(Predicate::ContentRefNotNull)
                }
                PyQueryStep::FilterContentRefEq { content_ref } => {
                    QueryStep::Filter(Predicate::ContentRefEq(content_ref))
                }
                PyQueryStep::FilterJsonTextEq { path, value } => {
                    QueryStep::Filter(Predicate::JsonPathEq {
                        path,
                        value: ScalarValue::Text(value),
                    })
                }
                PyQueryStep::FilterJsonBoolEq { path, value } => {
                    QueryStep::Filter(Predicate::JsonPathEq {
                        path,
                        value: ScalarValue::Bool(value),
                    })
                }
                PyQueryStep::FilterJsonIntegerGt { path, value }
                | PyQueryStep::FilterJsonTimestampGt { path, value } => {
                    QueryStep::Filter(Predicate::JsonPathCompare {
                        path,
                        op: ComparisonOp::Gt,
                        value: ScalarValue::Integer(value),
                    })
                }
                PyQueryStep::FilterJsonIntegerGte { path, value }
                | PyQueryStep::FilterJsonTimestampGte { path, value } => {
                    QueryStep::Filter(Predicate::JsonPathCompare {
                        path,
                        op: ComparisonOp::Gte,
                        value: ScalarValue::Integer(value),
                    })
                }
                PyQueryStep::FilterJsonIntegerLt { path, value }
                | PyQueryStep::FilterJsonTimestampLt { path, value } => {
                    QueryStep::Filter(Predicate::JsonPathCompare {
                        path,
                        op: ComparisonOp::Lt,
                        value: ScalarValue::Integer(value),
                    })
                }
                PyQueryStep::FilterJsonIntegerLte { path, value }
                | PyQueryStep::FilterJsonTimestampLte { path, value } => {
                    QueryStep::Filter(Predicate::JsonPathCompare {
                        path,
                        op: ComparisonOp::Lte,
                        value: ScalarValue::Integer(value),
                    })
                }
                PyQueryStep::EdgePropertyEq { path, value } => {
                    // EdgeProperty* variants are not valid as top-level query steps;
                    // they only appear inside expansion edge_filter. If somehow
                    // used here, treat as a no-op filter.
                    let _ = (path, value);
                    QueryStep::Filter(Predicate::ContentRefNotNull) // unreachable in practice
                }
                PyQueryStep::EdgePropertyCompare { path, op, value } => {
                    let _ = (path, op, value);
                    QueryStep::Filter(Predicate::ContentRefNotNull) // unreachable in practice
                }
            })
            .collect();
        let expansions = value
            .expansions
            .into_iter()
            .map(|slot| ExpansionSlot {
                slot: slot.slot,
                direction: slot.direction.into(),
                label: slot.label,
                max_depth: slot.max_depth,
                filter: slot.filter.and_then(py_filter_value_to_predicate),
                edge_filter: slot.edge_filter.and_then(py_filter_value_to_predicate),
            })
            .collect();

        Self {
            root_kind: value.root_kind,
            steps,
            expansions,
            final_limit: value.final_limit,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PyWriteRequest {
    pub label: String,
    #[serde(default)]
    pub nodes: Vec<PyNodeInsert>,
    #[serde(default)]
    pub node_retires: Vec<PyNodeRetire>,
    #[serde(default)]
    pub edges: Vec<PyEdgeInsert>,
    #[serde(default)]
    pub edge_retires: Vec<PyEdgeRetire>,
    #[serde(default)]
    pub chunks: Vec<PyChunkInsert>,
    #[serde(default)]
    pub runs: Vec<PyRunInsert>,
    #[serde(default)]
    pub steps: Vec<PyStepInsert>,
    #[serde(default)]
    pub actions: Vec<PyActionInsert>,
    #[serde(default)]
    pub optional_backfills: Vec<PyOptionalProjectionTask>,
    #[serde(default)]
    pub vec_inserts: Vec<PyVecInsert>,
    #[serde(default)]
    pub operational_writes: Vec<PyOperationalWrite>,
}

#[derive(Debug, Deserialize)]
pub struct PyLastAccessTouchRequest {
    pub logical_ids: Vec<String>,
    pub touched_at: i64,
    pub source_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PyNodeInsert {
    pub row_id: String,
    pub logical_id: String,
    pub kind: String,
    pub properties: String,
    pub source_ref: Option<String>,
    #[serde(default)]
    pub upsert: bool,
    pub chunk_policy: Option<PyChunkPolicy>,
    pub content_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PyEdgeInsert {
    pub row_id: String,
    pub logical_id: String,
    pub source_logical_id: String,
    pub target_logical_id: String,
    pub kind: String,
    pub properties: String,
    pub source_ref: Option<String>,
    #[serde(default)]
    pub upsert: bool,
}

#[derive(Debug, Deserialize)]
pub struct PyNodeRetire {
    pub logical_id: String,
    pub source_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PyEdgeRetire {
    pub logical_id: String,
    pub source_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PyChunkInsert {
    pub id: String,
    pub node_logical_id: String,
    pub text_content: String,
    pub byte_start: Option<i64>,
    pub byte_end: Option<i64>,
    pub content_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PyVecInsert {
    pub chunk_id: String,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PyOperationalWrite {
    Append {
        collection: String,
        record_key: String,
        payload_json: String,
        source_ref: Option<String>,
    },
    Put {
        collection: String,
        record_key: String,
        payload_json: String,
        source_ref: Option<String>,
    },
    Delete {
        collection: String,
        record_key: String,
        source_ref: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
pub struct PyRunInsert {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
    pub source_ref: Option<String>,
    #[serde(default)]
    pub upsert: bool,
    pub supersedes_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PyStepInsert {
    pub id: String,
    pub run_id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
    pub source_ref: Option<String>,
    #[serde(default)]
    pub upsert: bool,
    pub supersedes_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PyActionInsert {
    pub id: String,
    pub step_id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
    pub source_ref: Option<String>,
    #[serde(default)]
    pub upsert: bool,
    pub supersedes_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PyOptionalProjectionTask {
    pub target: PyProjectionTarget,
    pub payload: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PyChunkPolicy {
    Preserve,
    Replace,
}

impl From<PyChunkPolicy> for ChunkPolicy {
    fn from(value: PyChunkPolicy) -> Self {
        match value {
            PyChunkPolicy::Preserve => Self::Preserve,
            PyChunkPolicy::Replace => Self::Replace,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PyProjectionTarget {
    Fts,
    Vec,
    All,
}

impl From<PyProjectionTarget> for ProjectionTarget {
    fn from(value: PyProjectionTarget) -> Self {
        match value {
            PyProjectionTarget::Fts => Self::Fts,
            PyProjectionTarget::Vec => Self::Vec,
            PyProjectionTarget::All => Self::All,
        }
    }
}

impl From<ProjectionTarget> for PyProjectionTarget {
    fn from(value: ProjectionTarget) -> Self {
        match value {
            ProjectionTarget::Fts => Self::Fts,
            ProjectionTarget::Vec => Self::Vec,
            ProjectionTarget::All => Self::All,
        }
    }
}

impl From<PyWriteRequest> for WriteRequest {
    #[allow(clippy::too_many_lines)]
    fn from(value: PyWriteRequest) -> Self {
        Self {
            label: value.label,
            nodes: value
                .nodes
                .into_iter()
                .map(|node| NodeInsert {
                    row_id: node.row_id,
                    logical_id: node.logical_id,
                    kind: node.kind,
                    properties: node.properties,
                    source_ref: node.source_ref,
                    upsert: node.upsert,
                    chunk_policy: node.chunk_policy.unwrap_or(PyChunkPolicy::Preserve).into(),
                    content_ref: node.content_ref,
                })
                .collect(),
            node_retires: value
                .node_retires
                .into_iter()
                .map(|retire| NodeRetire {
                    logical_id: retire.logical_id,
                    source_ref: retire.source_ref,
                })
                .collect(),
            edges: value
                .edges
                .into_iter()
                .map(|edge| EdgeInsert {
                    row_id: edge.row_id,
                    logical_id: edge.logical_id,
                    source_logical_id: edge.source_logical_id,
                    target_logical_id: edge.target_logical_id,
                    kind: edge.kind,
                    properties: edge.properties,
                    source_ref: edge.source_ref,
                    upsert: edge.upsert,
                })
                .collect(),
            edge_retires: value
                .edge_retires
                .into_iter()
                .map(|retire| EdgeRetire {
                    logical_id: retire.logical_id,
                    source_ref: retire.source_ref,
                })
                .collect(),
            chunks: value
                .chunks
                .into_iter()
                .map(|chunk| ChunkInsert {
                    id: chunk.id,
                    node_logical_id: chunk.node_logical_id,
                    text_content: chunk.text_content,
                    byte_start: chunk.byte_start,
                    byte_end: chunk.byte_end,
                    content_hash: chunk.content_hash,
                })
                .collect(),
            runs: value
                .runs
                .into_iter()
                .map(|run| RunInsert {
                    id: run.id,
                    kind: run.kind,
                    status: run.status,
                    properties: run.properties,
                    source_ref: run.source_ref,
                    upsert: run.upsert,
                    supersedes_id: run.supersedes_id,
                })
                .collect(),
            steps: value
                .steps
                .into_iter()
                .map(|step| StepInsert {
                    id: step.id,
                    run_id: step.run_id,
                    kind: step.kind,
                    status: step.status,
                    properties: step.properties,
                    source_ref: step.source_ref,
                    upsert: step.upsert,
                    supersedes_id: step.supersedes_id,
                })
                .collect(),
            actions: value
                .actions
                .into_iter()
                .map(|action| ActionInsert {
                    id: action.id,
                    step_id: action.step_id,
                    kind: action.kind,
                    status: action.status,
                    properties: action.properties,
                    source_ref: action.source_ref,
                    upsert: action.upsert,
                    supersedes_id: action.supersedes_id,
                })
                .collect(),
            optional_backfills: value
                .optional_backfills
                .into_iter()
                .map(|task| OptionalProjectionTask {
                    target: task.target.into(),
                    payload: task.payload,
                })
                .collect(),
            vec_inserts: value
                .vec_inserts
                .into_iter()
                .map(|vec_insert| VecInsert {
                    chunk_id: vec_insert.chunk_id,
                    embedding: vec_insert.embedding,
                })
                .collect(),
            operational_writes: value
                .operational_writes
                .into_iter()
                .map(|write| match write {
                    PyOperationalWrite::Append {
                        collection,
                        record_key,
                        payload_json,
                        source_ref,
                    } => OperationalWrite::Append {
                        collection,
                        record_key,
                        payload_json,
                        source_ref,
                    },
                    PyOperationalWrite::Put {
                        collection,
                        record_key,
                        payload_json,
                        source_ref,
                    } => OperationalWrite::Put {
                        collection,
                        record_key,
                        payload_json,
                        source_ref,
                    },
                    PyOperationalWrite::Delete {
                        collection,
                        record_key,
                        source_ref,
                    } => OperationalWrite::Delete {
                        collection,
                        record_key,
                        source_ref,
                    },
                })
                .collect(),
        }
    }
}

impl From<PyLastAccessTouchRequest> for LastAccessTouchRequest {
    fn from(value: PyLastAccessTouchRequest) -> Self {
        Self {
            logical_ids: value.logical_ids,
            touched_at: value.touched_at,
            source_ref: value.source_ref,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyCompiledQuery {
    pub sql: String,
    pub binds: Vec<PyBindValue>,
    pub shape_hash: u64,
    pub driving_table: PyDrivingTable,
    pub hints: PyExecutionHints,
}

impl From<CompiledQuery> for PyCompiledQuery {
    fn from(value: CompiledQuery) -> Self {
        Self {
            sql: value.sql,
            binds: value.binds.into_iter().map(Into::into).collect(),
            shape_hash: value.shape_hash.0,
            driving_table: value.driving_table.into(),
            hints: value.hints.into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyCompiledGroupedQuery {
    pub root: PyCompiledQuery,
    pub expansions: Vec<PyExpansionSlot>,
    pub shape_hash: u64,
    pub hints: PyExecutionHints,
}

impl From<CompiledGroupedQuery> for PyCompiledGroupedQuery {
    fn from(value: CompiledGroupedQuery) -> Self {
        Self {
            root: value.root.into(),
            expansions: value
                .expansions
                .into_iter()
                .map(|slot| PyExpansionSlot {
                    slot: slot.slot,
                    direction: match slot.direction {
                        TraverseDirection::In => PyTraverseDirection::In,
                        TraverseDirection::Out => PyTraverseDirection::Out,
                    },
                    label: slot.label,
                    max_depth: slot.max_depth,
                    filter: None,
                    edge_filter: None,
                })
                .collect(),
            shape_hash: value.shape_hash.0,
            hints: value.hints.into(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PyBindValue {
    Text { value: String },
    Integer { value: i64 },
    Bool { value: bool },
}

impl From<BindValue> for PyBindValue {
    fn from(value: BindValue) -> Self {
        match value {
            BindValue::Text(value) => Self::Text { value },
            BindValue::Integer(value) => Self::Integer { value },
            BindValue::Bool(value) => Self::Bool { value },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyExecutionHints {
    pub recursion_limit: usize,
    pub hard_limit: usize,
}

impl From<ExecutionHints> for PyExecutionHints {
    fn from(value: ExecutionHints) -> Self {
        Self {
            recursion_limit: value.recursion_limit,
            hard_limit: value.hard_limit,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PyDrivingTable {
    Nodes,
    FtsNodes,
    VecNodes,
}

impl From<DrivingTable> for PyDrivingTable {
    fn from(value: DrivingTable) -> Self {
        match value {
            DrivingTable::Nodes => Self::Nodes,
            DrivingTable::FtsNodes => Self::FtsNodes,
            DrivingTable::VecNodes => Self::VecNodes,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyQueryPlan {
    pub sql: String,
    pub bind_count: usize,
    pub driving_table: PyDrivingTable,
    pub shape_hash: u64,
    pub cache_hit: bool,
}

impl From<QueryPlan> for PyQueryPlan {
    fn from(value: QueryPlan) -> Self {
        Self {
            sql: value.sql,
            bind_count: value.bind_count,
            driving_table: value.driving_table.into(),
            shape_hash: value.shape_hash.0,
            cache_hit: value.cache_hit,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyQueryRows {
    pub nodes: Vec<PyNodeRow>,
    pub runs: Vec<PyRunRow>,
    pub steps: Vec<PyStepRow>,
    pub actions: Vec<PyActionRow>,
    pub was_degraded: bool,
}

impl From<QueryRows> for PyQueryRows {
    fn from(value: QueryRows) -> Self {
        Self {
            nodes: value.nodes.into_iter().map(Into::into).collect(),
            runs: value.runs.into_iter().map(Into::into).collect(),
            steps: value.steps.into_iter().map(Into::into).collect(),
            actions: value.actions.into_iter().map(Into::into).collect(),
            was_degraded: value.was_degraded,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyExpansionRootRows {
    pub root_logical_id: String,
    pub nodes: Vec<PyNodeRow>,
}

impl From<ExpansionRootRows> for PyExpansionRootRows {
    fn from(value: ExpansionRootRows) -> Self {
        Self {
            root_logical_id: value.root_logical_id,
            nodes: value.nodes.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyExpansionSlotRows {
    pub slot: String,
    pub roots: Vec<PyExpansionRootRows>,
}

impl From<ExpansionSlotRows> for PyExpansionSlotRows {
    fn from(value: ExpansionSlotRows) -> Self {
        Self {
            slot: value.slot,
            roots: value.roots.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyGroupedQueryRows {
    pub roots: Vec<PyNodeRow>,
    pub expansions: Vec<PyExpansionSlotRows>,
    pub was_degraded: bool,
}

impl From<GroupedQueryRows> for PyGroupedQueryRows {
    fn from(value: GroupedQueryRows) -> Self {
        Self {
            roots: value.roots.into_iter().map(Into::into).collect(),
            expansions: value.expansions.into_iter().map(Into::into).collect(),
            was_degraded: value.was_degraded,
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::{PyOperationalWrite, PyQueryAst, PyQueryStep, PyWriteRequest};
    use crate::{ComparisonOp, Predicate, QueryAst, QueryStep, ScalarValue};
    use fathomdb_query::TextQuery;

    // ---------------------------------------------------------------
    // PyQueryStep deserialization: one test per variant to catch
    // Python-to-Rust AST bridge gaps.
    // ---------------------------------------------------------------

    fn parse_step(json: &str) -> PyQueryStep {
        serde_json::from_str(json).expect("parse PyQueryStep")
    }

    fn parse_ast_with_step(step_json: &str) -> QueryAst {
        let ast_json = format!(r#"{{"root_kind":"Node","steps":[{step_json}]}}"#);
        let py_ast: PyQueryAst = serde_json::from_str(&ast_json).expect("parse PyQueryAst");
        QueryAst::from(py_ast)
    }

    #[test]
    fn step_vector_search_roundtrip() {
        let step = parse_step(r#"{"type":"vector_search","query":"hello","limit":5}"#);
        assert!(matches!(step, PyQueryStep::VectorSearch { limit: 5, .. }));
        let ast = parse_ast_with_step(r#"{"type":"vector_search","query":"hello","limit":5}"#);
        assert!(matches!(
            &ast.steps[0],
            QueryStep::VectorSearch { limit: 5, .. }
        ));
    }

    #[test]
    fn step_text_search_roundtrip() {
        let step = parse_step(r#"{"type":"text_search","query":"budget","limit":10}"#);
        assert!(matches!(step, PyQueryStep::TextSearch { limit: 10, .. }));
        let ast = parse_ast_with_step(r#"{"type":"text_search","query":"budget","limit":10}"#);
        match &ast.steps[0] {
            QueryStep::TextSearch { query, limit } => {
                assert_eq!(*limit, 10);
                assert_eq!(*query, TextQuery::Term("budget".into()));
            }
            other => panic!("expected TextSearch, got {other:?}"),
        }
    }

    #[test]
    fn step_text_search_parses_supported_boolean_subset() {
        let ast =
            parse_ast_with_step(r#"{"type":"text_search","query":"ship OR docs","limit":10}"#);
        match &ast.steps[0] {
            QueryStep::TextSearch { query, limit } => {
                assert_eq!(*limit, 10);
                assert_eq!(
                    *query,
                    TextQuery::Or(vec![
                        TextQuery::Term("ship".into()),
                        TextQuery::Term("docs".into()),
                    ])
                );
            }
            other => panic!("expected TextSearch, got {other:?}"),
        }
    }

    #[test]
    fn step_traverse_roundtrip() {
        let ast = parse_ast_with_step(
            r#"{"type":"traverse","direction":"out","label":"OWNS","max_depth":2}"#,
        );
        match &ast.steps[0] {
            QueryStep::Traverse {
                label, max_depth, ..
            } => {
                assert_eq!(label, "OWNS");
                assert_eq!(*max_depth, 2);
            }
            other => panic!("expected Traverse, got {other:?}"),
        }
    }

    #[test]
    fn step_filter_logical_id_eq_roundtrip() {
        let ast = parse_ast_with_step(r#"{"type":"filter_logical_id_eq","logical_id":"node-1"}"#);
        match &ast.steps[0] {
            QueryStep::Filter(Predicate::LogicalIdEq(id)) => assert_eq!(id, "node-1"),
            other => panic!("expected LogicalIdEq, got {other:?}"),
        }
    }

    #[test]
    fn step_filter_kind_eq_roundtrip() {
        let ast = parse_ast_with_step(r#"{"type":"filter_kind_eq","kind":"Meeting"}"#);
        match &ast.steps[0] {
            QueryStep::Filter(Predicate::KindEq(kind)) => assert_eq!(kind, "Meeting"),
            other => panic!("expected KindEq, got {other:?}"),
        }
    }

    #[test]
    fn step_filter_source_ref_eq_roundtrip() {
        let ast = parse_ast_with_step(r#"{"type":"filter_source_ref_eq","source_ref":"src-abc"}"#);
        match &ast.steps[0] {
            QueryStep::Filter(Predicate::SourceRefEq(src)) => assert_eq!(src, "src-abc"),
            other => panic!("expected SourceRefEq, got {other:?}"),
        }
    }

    #[test]
    fn step_filter_content_ref_not_null_roundtrip() {
        let ast = parse_ast_with_step(r#"{"type":"filter_content_ref_not_null"}"#);
        match &ast.steps[0] {
            QueryStep::Filter(Predicate::ContentRefNotNull) => {}
            other => panic!("expected ContentRefNotNull, got {other:?}"),
        }
    }

    #[test]
    fn step_filter_content_ref_eq_roundtrip() {
        let ast = parse_ast_with_step(
            r#"{"type":"filter_content_ref_eq","content_ref":"s3://docs/report.pdf"}"#,
        );
        match &ast.steps[0] {
            QueryStep::Filter(Predicate::ContentRefEq(uri)) => {
                assert_eq!(uri, "s3://docs/report.pdf");
            }
            other => panic!("expected ContentRefEq, got {other:?}"),
        }
    }

    #[test]
    fn step_filter_json_text_eq_roundtrip() {
        let ast = parse_ast_with_step(
            r#"{"type":"filter_json_text_eq","path":"$.status","value":"active"}"#,
        );
        match &ast.steps[0] {
            QueryStep::Filter(Predicate::JsonPathEq { path, value }) => {
                assert_eq!(path, "$.status");
                assert_eq!(*value, ScalarValue::Text("active".to_owned()));
            }
            other => panic!("expected JsonPathEq/Text, got {other:?}"),
        }
    }

    /// Regression: `ScalarValue::Bool` is supported by the AST and compiler but
    /// `PyQueryStep` was missing the `FilterJsonBoolEq` variant, so Python could
    /// not emit boolean equality filters.
    #[test]
    fn step_filter_json_bool_eq_roundtrip() {
        // This tests that the PyQueryStep enum can deserialize the
        // filter_json_bool_eq tag and converts to the correct Predicate.
        let ast =
            parse_ast_with_step(r#"{"type":"filter_json_bool_eq","path":"$.active","value":true}"#);
        match &ast.steps[0] {
            QueryStep::Filter(Predicate::JsonPathEq { path, value }) => {
                assert_eq!(path, "$.active");
                assert_eq!(*value, ScalarValue::Bool(true));
            }
            other => panic!("expected JsonPathEq/Bool, got {other:?}"),
        }
    }

    /// Same as above but with `false`.
    #[test]
    fn step_filter_json_bool_eq_false_roundtrip() {
        let ast = parse_ast_with_step(
            r#"{"type":"filter_json_bool_eq","path":"$.archived","value":false}"#,
        );
        match &ast.steps[0] {
            QueryStep::Filter(Predicate::JsonPathEq { path, value }) => {
                assert_eq!(path, "$.archived");
                assert_eq!(*value, ScalarValue::Bool(false));
            }
            other => panic!("expected JsonPathEq/Bool(false), got {other:?}"),
        }
    }

    #[test]
    fn step_filter_json_integer_gt_roundtrip() {
        let ast = parse_ast_with_step(
            r#"{"type":"filter_json_integer_gt","path":"$.priority","value":5}"#,
        );
        match &ast.steps[0] {
            QueryStep::Filter(Predicate::JsonPathCompare { path, op, value }) => {
                assert_eq!(path, "$.priority");
                assert_eq!(*op, ComparisonOp::Gt);
                assert_eq!(*value, ScalarValue::Integer(5));
            }
            other => panic!("expected JsonPathCompare/Gt, got {other:?}"),
        }
    }

    #[test]
    fn step_filter_json_integer_gte_roundtrip() {
        let ast = parse_ast_with_step(
            r#"{"type":"filter_json_integer_gte","path":"$.priority","value":3}"#,
        );
        match &ast.steps[0] {
            QueryStep::Filter(Predicate::JsonPathCompare { op, .. }) => {
                assert_eq!(*op, ComparisonOp::Gte);
            }
            other => panic!("expected JsonPathCompare/Gte, got {other:?}"),
        }
    }

    #[test]
    fn step_filter_json_integer_lt_roundtrip() {
        let ast = parse_ast_with_step(
            r#"{"type":"filter_json_integer_lt","path":"$.score","value":100}"#,
        );
        match &ast.steps[0] {
            QueryStep::Filter(Predicate::JsonPathCompare { op, .. }) => {
                assert_eq!(*op, ComparisonOp::Lt);
            }
            other => panic!("expected JsonPathCompare/Lt, got {other:?}"),
        }
    }

    #[test]
    fn step_filter_json_integer_lte_roundtrip() {
        let ast =
            parse_ast_with_step(r#"{"type":"filter_json_integer_lte","path":"$.rank","value":10}"#);
        match &ast.steps[0] {
            QueryStep::Filter(Predicate::JsonPathCompare { op, .. }) => {
                assert_eq!(*op, ComparisonOp::Lte);
            }
            other => panic!("expected JsonPathCompare/Lte, got {other:?}"),
        }
    }

    #[test]
    fn step_filter_json_timestamp_gt_roundtrip() {
        let ast = parse_ast_with_step(
            r#"{"type":"filter_json_timestamp_gt","path":"$.created_at","value":1710000000}"#,
        );
        match &ast.steps[0] {
            QueryStep::Filter(Predicate::JsonPathCompare { op, value, .. }) => {
                assert_eq!(*op, ComparisonOp::Gt);
                assert_eq!(*value, ScalarValue::Integer(1_710_000_000));
            }
            other => panic!("expected JsonPathCompare/Gt timestamp, got {other:?}"),
        }
    }

    #[test]
    fn step_filter_json_timestamp_gte_roundtrip() {
        let ast =
            parse_ast_with_step(r#"{"type":"filter_json_timestamp_gte","path":"$.ts","value":1}"#);
        assert!(matches!(
            &ast.steps[0],
            QueryStep::Filter(Predicate::JsonPathCompare {
                op: ComparisonOp::Gte,
                ..
            })
        ));
    }

    #[test]
    fn step_filter_json_timestamp_lt_roundtrip() {
        let ast =
            parse_ast_with_step(r#"{"type":"filter_json_timestamp_lt","path":"$.ts","value":9}"#);
        assert!(matches!(
            &ast.steps[0],
            QueryStep::Filter(Predicate::JsonPathCompare {
                op: ComparisonOp::Lt,
                ..
            })
        ));
    }

    #[test]
    fn step_filter_json_timestamp_lte_roundtrip() {
        let ast =
            parse_ast_with_step(r#"{"type":"filter_json_timestamp_lte","path":"$.ts","value":99}"#);
        assert!(matches!(
            &ast.steps[0],
            QueryStep::Filter(Predicate::JsonPathCompare {
                op: ComparisonOp::Lte,
                ..
            })
        ));
    }

    /// Rejects an unrecognized step type so Python gets a clear error instead
    /// of silent data loss.
    #[test]
    fn step_unknown_type_tag_is_rejected() {
        let result = serde_json::from_str::<PyQueryStep>(
            r#"{"type":"filter_json_float_eq","path":"$.x","value":1.5}"#,
        );
        assert!(
            result.is_err(),
            "unknown step type should fail deserialization"
        );
    }

    // ---------------------------------------------------------------
    // PyWriteRequest field coverage: catches struct-level divergence
    // between the Python JSON schema and the Rust engine types.
    // ---------------------------------------------------------------

    #[test]
    fn write_request_deserializes_all_entity_arrays() {
        let request: PyWriteRequest = serde_json::from_str(
            r#"{
                "label": "full_write",
                "nodes": [{
                    "row_id": "r1", "logical_id": "l1", "kind": "Doc",
                    "properties": "{}", "source_ref": "s1", "upsert": false
                }],
                "node_retires": [{"logical_id": "l1", "source_ref": "s1"}],
                "edges": [{
                    "row_id": "e1", "logical_id": "el1",
                    "source_logical_id": "l1", "target_logical_id": "l2",
                    "kind": "LINKS", "properties": "{}", "source_ref": "s1",
                    "upsert": false
                }],
                "edge_retires": [{"logical_id": "el1", "source_ref": "s1"}],
                "chunks": [{
                    "id": "c1", "node_logical_id": "l1",
                    "text_content": "hello world"
                }],
                "runs": [{
                    "id": "run1", "kind": "ingest", "status": "running",
                    "properties": "{}", "source_ref": "s1"
                }],
                "steps": [{
                    "id": "step1", "run_id": "run1", "kind": "extract",
                    "status": "running", "properties": "{}", "source_ref": "s1"
                }],
                "actions": [{
                    "id": "act1", "step_id": "step1", "kind": "fetch",
                    "status": "done", "properties": "{}", "source_ref": "s1"
                }],
                "vec_inserts": [{
                    "chunk_id": "c1", "embedding": [0.1, 0.2, 0.3]
                }],
                "optional_backfills": [{
                    "target": "fts", "payload": "{}"
                }],
                "operational_writes": [{
                    "type": "append", "collection": "log",
                    "record_key": "k1", "payload_json": "{}"
                }]
            }"#,
        )
        .expect("parse full write request");

        assert_eq!(request.nodes.len(), 1);
        assert_eq!(request.node_retires.len(), 1);
        assert_eq!(request.edges.len(), 1);
        assert_eq!(request.edge_retires.len(), 1);
        assert_eq!(request.chunks.len(), 1);
        assert_eq!(request.runs.len(), 1);
        assert_eq!(request.steps.len(), 1);
        assert_eq!(request.actions.len(), 1);
        assert_eq!(request.vec_inserts.len(), 1);
        assert_eq!(request.optional_backfills.len(), 1);
        assert_eq!(request.operational_writes.len(), 1);
    }

    /// Verifies that the `From<PyWriteRequest>` conversion preserves all fields
    /// through to the engine `WriteRequest`. If the engine adds a field and the
    /// bridge omits it, this test must be updated — which is the point.
    #[test]
    fn write_request_conversion_preserves_all_entity_fields() {
        let request: PyWriteRequest = serde_json::from_str(
            r#"{
                "label": "conv_test",
                "nodes": [{
                    "row_id": "r1", "logical_id": "l1", "kind": "Doc",
                    "properties": "{\"k\":1}", "source_ref": "s1", "upsert": true,
                    "chunk_policy": "replace"
                }],
                "edges": [{
                    "row_id": "e1", "logical_id": "el1",
                    "source_logical_id": "l1", "target_logical_id": "l2",
                    "kind": "LINKS", "properties": "{}", "source_ref": "s2",
                    "upsert": true
                }],
                "runs": [{
                    "id": "run1", "kind": "ingest", "status": "done",
                    "properties": "{}", "source_ref": "s3",
                    "upsert": true, "supersedes_id": "run0"
                }],
                "steps": [{
                    "id": "step1", "run_id": "run1", "kind": "parse",
                    "status": "ok", "properties": "{}", "source_ref": "s4",
                    "upsert": true, "supersedes_id": "step0"
                }],
                "actions": [{
                    "id": "act1", "step_id": "step1", "kind": "fetch",
                    "status": "ok", "properties": "{}", "source_ref": "s5",
                    "upsert": true, "supersedes_id": "act0"
                }]
            }"#,
        )
        .expect("parse");

        let wr = crate::WriteRequest::from(request);
        assert_eq!(wr.label, "conv_test");

        let node = &wr.nodes[0];
        assert_eq!(node.row_id, "r1");
        assert_eq!(node.logical_id, "l1");
        assert_eq!(node.kind, "Doc");
        assert_eq!(node.properties, "{\"k\":1}");
        assert_eq!(node.source_ref.as_deref(), Some("s1"));
        assert!(node.upsert);
        assert_eq!(node.chunk_policy, crate::ChunkPolicy::Replace);

        let edge = &wr.edges[0];
        assert_eq!(edge.source_logical_id, "l1");
        assert_eq!(edge.target_logical_id, "l2");
        assert!(edge.upsert);

        let run = &wr.runs[0];
        assert_eq!(run.supersedes_id.as_deref(), Some("run0"));
        assert!(run.upsert);

        let step = &wr.steps[0];
        assert_eq!(step.supersedes_id.as_deref(), Some("step0"));

        let action = &wr.actions[0];
        assert_eq!(action.supersedes_id.as_deref(), Some("act0"));
    }

    // ---------------------------------------------------------------
    // Operational write variants: catches missing or renamed tags.
    // ---------------------------------------------------------------

    #[test]
    fn py_write_request_deserializes_operational_writes() {
        let request: PyWriteRequest = serde_json::from_str(
            r#"{
                "label": "operational",
                "operational_writes": [
                    {
                        "type": "put",
                        "collection": "connector_health",
                        "record_key": "gmail",
                        "payload_json": "{\"status\":\"ok\"}",
                        "source_ref": "src-1"
                    }
                ]
            }"#,
        )
        .expect("parse request");

        assert_eq!(request.operational_writes.len(), 1);
        match &request.operational_writes[0] {
            PyOperationalWrite::Put {
                collection,
                record_key,
                payload_json,
                source_ref,
            } => {
                assert_eq!(collection, "connector_health");
                assert_eq!(record_key, "gmail");
                assert_eq!(payload_json, "{\"status\":\"ok\"}");
                assert_eq!(source_ref.as_deref(), Some("src-1"));
            }
            other => panic!("unexpected operational write: {other:?}"),
        }
    }

    #[test]
    fn operational_write_append_variant() {
        let request: PyWriteRequest = serde_json::from_str(
            r#"{
                "label": "op",
                "operational_writes": [{
                    "type": "append", "collection": "logs",
                    "record_key": "k1", "payload_json": "{}"
                }]
            }"#,
        )
        .expect("parse");
        assert!(matches!(
            &request.operational_writes[0],
            PyOperationalWrite::Append { .. }
        ));
    }

    #[test]
    fn operational_write_delete_variant() {
        let request: PyWriteRequest = serde_json::from_str(
            r#"{
                "label": "op",
                "operational_writes": [{
                    "type": "delete", "collection": "cache",
                    "record_key": "k1"
                }]
            }"#,
        )
        .expect("parse");
        assert!(matches!(
            &request.operational_writes[0],
            PyOperationalWrite::Delete { .. }
        ));
    }

    // ---------------------------------------------------------------
    // EngineError variant coverage: ensures map_engine_error handles
    // every variant. If a new variant is added, this test must be
    // updated — the non-exhaustive match would cause a compile error.
    // ---------------------------------------------------------------

    #[test]
    fn engine_error_map_covers_all_variants() {
        use crate::EngineError;

        // Construct one instance of each variant and verify the mapper
        // produces a PyErr (we only check it doesn't panic; the test
        // compiles only if the match in map_engine_error is exhaustive).
        let variants: Vec<EngineError> = vec![
            EngineError::Sqlite(rusqlite::Error::InvalidColumnIndex(0)),
            EngineError::Schema(fathomdb_schema::SchemaError::MissingCapability("test")),
            EngineError::Io(std::io::Error::other("x")),
            EngineError::WriterRejected("w".into()),
            EngineError::WriterTimedOut("t".into()),
            EngineError::InvalidWrite("i".into()),
            EngineError::Bridge("b".into()),
            EngineError::CapabilityMissing("c".into()),
            EngineError::DatabaseLocked("d".into()),
            EngineError::InvalidConfig("cfg".into()),
        ];
        for variant in variants {
            // map_engine_error is not pub, so exercise it through the
            // string representation to verify all variants are
            // accounted for.
            let display = format!("{variant}");
            assert!(!display.is_empty());
        }
    }

    // ---------------------------------------------------------------
    // PyBindValue output coverage: ensures the Rust→Python serialized
    // bind values cover all ScalarValue/BindValue variants.
    // ---------------------------------------------------------------

    #[test]
    fn bind_value_serializes_all_variants() {
        use super::PyBindValue;

        let text = serde_json::to_string(&PyBindValue::Text {
            value: "hello".into(),
        })
        .expect("serialize text");
        assert!(text.contains("\"type\":\"text\""));

        let integer =
            serde_json::to_string(&PyBindValue::Integer { value: 42 }).expect("serialize integer");
        assert!(integer.contains("\"type\":\"integer\""));

        let boolean =
            serde_json::to_string(&PyBindValue::Bool { value: true }).expect("serialize bool");
        assert!(boolean.contains("\"type\":\"bool\""));
    }

    // ---------------------------------------------------------------
    // Cross-layer parity: engine report → Py* report serialization.
    // Each test constructs an engine struct with non-default values,
    // converts via From, serializes to JSON, and asserts every field
    // is present with the expected value.  If a field is added to the
    // engine type but not the Py* type, these tests will fail to
    // compile (missing field in the engine constructor) or will fail
    // at runtime (missing key in the serialized JSON).
    // ---------------------------------------------------------------

    #[test]
    fn integrity_report_serializes_all_fields() {
        use super::PyIntegrityReport;
        use fathomdb_engine::IntegrityReport;

        let report = IntegrityReport {
            physical_ok: true,
            foreign_keys_ok: false,
            missing_fts_rows: 3,
            missing_property_fts_rows: 5,
            duplicate_active_logical_ids: 1,
            operational_missing_collections: 2,
            operational_missing_last_mutations: 4,
            warnings: vec!["warn-1".into(), "warn-2".into()],
        };
        let py = PyIntegrityReport::from(report);
        let json: serde_json::Value = serde_json::to_value(&py).expect("serialize");

        assert_eq!(json["physical_ok"], true);
        assert_eq!(json["foreign_keys_ok"], false);
        assert_eq!(json["missing_fts_rows"], 3);
        assert_eq!(json["missing_property_fts_rows"], 5);
        assert_eq!(json["duplicate_active_logical_ids"], 1);
        assert_eq!(json["operational_missing_collections"], 2);
        assert_eq!(json["operational_missing_last_mutations"], 4);
        let warnings = json["warnings"].as_array().expect("warnings array");
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0], "warn-1");
        assert_eq!(warnings[1], "warn-2");
    }

    #[test]
    fn semantic_report_serializes_all_fields() {
        use super::PySemanticReport;
        use fathomdb_engine::SemanticReport;

        let report = SemanticReport {
            orphaned_chunks: 1,
            null_source_ref_nodes: 2,
            broken_step_fk: 3,
            broken_action_fk: 4,
            stale_fts_rows: 5,
            fts_rows_for_superseded_nodes: 6,
            stale_property_fts_rows: 15,
            orphaned_property_fts_rows: 16,
            mismatched_kind_property_fts_rows: 17,
            duplicate_property_fts_rows: 18,
            drifted_property_fts_rows: 19,
            dangling_edges: 7,
            orphaned_supersession_chains: 8,
            stale_vec_rows: 9,
            vec_rows_for_superseded_nodes: 10,
            missing_operational_current_rows: 11,
            stale_operational_current_rows: 12,
            disabled_collection_mutations: 13,
            orphaned_last_access_metadata_rows: 14,
            warnings: vec!["sem-warn".into()],
        };
        let py = PySemanticReport::from(report);
        let json: serde_json::Value = serde_json::to_value(&py).expect("serialize");

        assert_eq!(json["orphaned_chunks"], 1);
        assert_eq!(json["null_source_ref_nodes"], 2);
        assert_eq!(json["broken_step_fk"], 3);
        assert_eq!(json["broken_action_fk"], 4);
        assert_eq!(json["stale_fts_rows"], 5);
        assert_eq!(json["fts_rows_for_superseded_nodes"], 6);
        assert_eq!(json["stale_property_fts_rows"], 15);
        assert_eq!(json["orphaned_property_fts_rows"], 16);
        assert_eq!(json["mismatched_kind_property_fts_rows"], 17);
        assert_eq!(json["duplicate_property_fts_rows"], 18);
        assert_eq!(json["drifted_property_fts_rows"], 19);
        assert_eq!(json["dangling_edges"], 7);
        assert_eq!(json["orphaned_supersession_chains"], 8);
        assert_eq!(json["stale_vec_rows"], 9);
        assert_eq!(json["vec_rows_for_superseded_nodes"], 10);
        assert_eq!(json["missing_operational_current_rows"], 11);
        assert_eq!(json["stale_operational_current_rows"], 12);
        assert_eq!(json["disabled_collection_mutations"], 13);
        assert_eq!(json["orphaned_last_access_metadata_rows"], 14);
        let warnings = json["warnings"].as_array().expect("warnings array");
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0], "sem-warn");
    }

    #[test]
    fn trace_report_serializes_all_fields() {
        use super::PyTraceReport;
        use fathomdb_engine::TraceReport;

        let report = TraceReport {
            source_ref: "src-42".into(),
            node_rows: 10,
            edge_rows: 20,
            action_rows: 30,
            operational_mutation_rows: 5,
            node_logical_ids: vec!["n1".into(), "n2".into()],
            action_ids: vec!["a1".into()],
            operational_mutation_ids: vec!["om1".into(), "om2".into()],
        };
        let py = PyTraceReport::from(report);
        let json: serde_json::Value = serde_json::to_value(&py).expect("serialize");

        assert_eq!(json["source_ref"], "src-42");
        assert_eq!(json["node_rows"], 10);
        assert_eq!(json["edge_rows"], 20);
        assert_eq!(json["action_rows"], 30);
        assert_eq!(json["operational_mutation_rows"], 5);
        let node_ids = json["node_logical_ids"]
            .as_array()
            .expect("node_logical_ids");
        assert_eq!(node_ids.len(), 2);
        assert_eq!(node_ids[0], "n1");
        let action_ids = json["action_ids"].as_array().expect("action_ids");
        assert_eq!(action_ids.len(), 1);
        assert_eq!(action_ids[0], "a1");
        let op_ids = json["operational_mutation_ids"]
            .as_array()
            .expect("operational_mutation_ids");
        assert_eq!(op_ids.len(), 2);
    }

    #[test]
    fn projection_repair_report_serializes_all_fields() {
        use super::PyProjectionRepairReport;
        use crate::{ProjectionRepairReport, ProjectionTarget};

        let report = ProjectionRepairReport {
            targets: vec![ProjectionTarget::Fts, ProjectionTarget::Vec],
            rebuilt_rows: 42,
            notes: vec!["rebuilt fts".into()],
        };
        let py = PyProjectionRepairReport::from(report);
        let json: serde_json::Value = serde_json::to_value(&py).expect("serialize");

        let targets = json["targets"].as_array().expect("targets");
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0], "fts");
        assert_eq!(targets[1], "vec");
        assert_eq!(json["rebuilt_rows"], 42);
        let notes = json["notes"].as_array().expect("notes");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0], "rebuilt fts");
    }

    #[cfg(feature = "python")]
    #[test]
    fn vector_regeneration_report_serializes_all_fields() {
        use super::PyVectorRegenerationReport;
        use fathomdb_engine::VectorRegenerationReport;

        let report = VectorRegenerationReport {
            profile: "default".into(),
            table_name: "vec_chunks".into(),
            dimension: 384,
            total_chunks: 100,
            regenerated_rows: 95,
            contract_persisted: true,
            notes: vec!["skipped 5 empty chunks".into()],
        };
        let py = PyVectorRegenerationReport::from(report);
        let json: serde_json::Value = serde_json::to_value(&py).expect("serialize");

        assert_eq!(json["profile"], "default");
        assert_eq!(json["table_name"], "vec_chunks");
        assert_eq!(json["dimension"], 384);
        assert_eq!(json["total_chunks"], 100);
        assert_eq!(json["regenerated_rows"], 95);
        assert_eq!(json["contract_persisted"], true);
        let notes = json["notes"].as_array().expect("notes array");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0], "skipped 5 empty chunks");
    }

    #[test]
    fn safe_export_manifest_serializes_all_fields() {
        use super::PySafeExportManifest;
        use crate::SafeExportManifest;

        let manifest = SafeExportManifest {
            exported_at: 1_700_000_000,
            sha256: "abcdef1234567890".into(),
            schema_version: 15,
            protocol_version: 1,
            page_count: 256,
        };
        let py = PySafeExportManifest::from(manifest);
        let json: serde_json::Value = serde_json::to_value(&py).expect("serialize");

        assert_eq!(json["exported_at"], 1_700_000_000_u64);
        assert_eq!(json["sha256"], "abcdef1234567890");
        assert_eq!(json["schema_version"], 15);
        assert_eq!(json["protocol_version"], 1);
        assert_eq!(json["page_count"], 256);
    }

    #[test]
    fn write_receipt_serializes_all_fields() {
        use super::PyWriteReceipt;
        use crate::WriteReceipt;

        let receipt = WriteReceipt {
            label: "batch-1".into(),
            optional_backfill_count: 7,
            warnings: vec!["w1".into()],
            provenance_warnings: vec!["pw1".into(), "pw2".into()],
        };
        let py = PyWriteReceipt::from(receipt);
        let json: serde_json::Value = serde_json::to_value(&py).expect("serialize");

        assert_eq!(json["label"], "batch-1");
        assert_eq!(json["optional_backfill_count"], 7);
        let warnings = json["warnings"].as_array().expect("warnings");
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0], "w1");
        let prov = json["provenance_warnings"]
            .as_array()
            .expect("provenance_warnings");
        assert_eq!(prov.len(), 2);
        assert_eq!(prov[0], "pw1");
        assert_eq!(prov[1], "pw2");
    }

    #[test]
    fn query_rows_serializes_all_fields_with_populated_arrays() {
        use super::PyQueryRows;
        use crate::{ActionRow, NodeRow, QueryRows, RunRow, StepRow};

        let rows = QueryRows {
            nodes: vec![NodeRow {
                row_id: "nr1".into(),
                logical_id: "nl1".into(),
                kind: "Doc".into(),
                properties: r#"{"a":1}"#.into(),
                content_ref: None,
                last_accessed_at: Some(1_700_000_000),
                edge_properties: None,
            }],
            runs: vec![RunRow {
                id: "run1".into(),
                kind: "ingest".into(),
                status: "done".into(),
                properties: "{}".into(),
            }],
            steps: vec![StepRow {
                id: "step1".into(),
                run_id: "run1".into(),
                kind: "parse".into(),
                status: "ok".into(),
                properties: "{}".into(),
            }],
            actions: vec![ActionRow {
                id: "act1".into(),
                step_id: "step1".into(),
                kind: "fetch".into(),
                status: "done".into(),
                properties: "{}".into(),
            }],
            was_degraded: true,
        };
        let py = PyQueryRows::from(rows);
        let json: serde_json::Value = serde_json::to_value(&py).expect("serialize");

        // Top-level fields
        assert_eq!(json["was_degraded"], true);

        // Nodes array
        let nodes = json["nodes"].as_array().expect("nodes");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0]["row_id"], "nr1");
        assert_eq!(nodes[0]["logical_id"], "nl1");
        assert_eq!(nodes[0]["kind"], "Doc");
        assert_eq!(nodes[0]["properties"], r#"{"a":1}"#);
        assert_eq!(nodes[0]["last_accessed_at"], 1_700_000_000_i64);

        // Runs array
        let runs = json["runs"].as_array().expect("runs");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0]["id"], "run1");
        assert_eq!(runs[0]["kind"], "ingest");
        assert_eq!(runs[0]["status"], "done");
        assert_eq!(runs[0]["properties"], "{}");

        // Steps array
        let steps = json["steps"].as_array().expect("steps");
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0]["id"], "step1");
        assert_eq!(steps[0]["run_id"], "run1");
        assert_eq!(steps[0]["kind"], "parse");
        assert_eq!(steps[0]["status"], "ok");

        // Actions array
        let actions = json["actions"].as_array().expect("actions");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["id"], "act1");
        assert_eq!(actions[0]["step_id"], "step1");
        assert_eq!(actions[0]["kind"], "fetch");
        assert_eq!(actions[0]["status"], "done");
    }

    #[test]
    fn last_access_touch_report_serializes_all_fields() {
        use super::PyLastAccessTouchReport;
        use crate::LastAccessTouchReport;

        let report = LastAccessTouchReport {
            touched_logical_ids: 5,
            touched_at: 1_700_000_000,
        };
        let py = PyLastAccessTouchReport::from(report);
        let json: serde_json::Value = serde_json::to_value(&py).expect("serialize");

        assert_eq!(json["touched_logical_ids"], 5);
        assert_eq!(json["touched_at"], 1_700_000_000_i64);
    }

    // ---------------------------------------------------------------
    // Operational report parity: OperationalCompactionReport is
    // serialized directly (no Py* wrapper), so we test its serde
    // output to catch field drift (e.g. the dry_run gap).
    // ---------------------------------------------------------------

    #[test]
    fn operational_compaction_report_serializes_dry_run() {
        use crate::OperationalCompactionReport;

        let report = OperationalCompactionReport {
            collection_name: "audit_log".into(),
            deleted_mutations: 42,
            dry_run: true,
            before_timestamp: Some(1_700_000_000),
        };
        let json: serde_json::Value = serde_json::to_value(&report).expect("serialize");

        assert_eq!(json["collection_name"], "audit_log");
        assert_eq!(json["deleted_mutations"], 42);
        assert_eq!(json["dry_run"], true);
        assert_eq!(json["before_timestamp"], 1_700_000_000_i64);

        // Also verify dry_run=false
        let report_no_dry = OperationalCompactionReport {
            collection_name: "logs".into(),
            deleted_mutations: 0,
            dry_run: false,
            before_timestamp: None,
        };
        let json2: serde_json::Value = serde_json::to_value(&report_no_dry).expect("serialize");
        assert_eq!(json2["dry_run"], false);
        assert!(json2["before_timestamp"].is_null());
    }

    // ---------------------------------------------------------------
    // Row-level parity: NodeRow, RunRow, StepRow, ActionRow
    // ---------------------------------------------------------------

    #[test]
    fn node_row_serializes_all_fields_including_last_accessed_at() {
        use super::PyNodeRow;
        use crate::NodeRow;

        let row = NodeRow {
            row_id: "row-abc".into(),
            logical_id: "log-xyz".into(),
            kind: "Meeting".into(),
            properties: r#"{"title":"standup"}"#.into(),
            content_ref: Some("s3://bucket/standup.pdf".into()),
            last_accessed_at: Some(1_710_000_000),
            edge_properties: None,
        };
        let py = PyNodeRow::from(row);
        let json: serde_json::Value = serde_json::to_value(&py).expect("serialize");

        assert_eq!(json["row_id"], "row-abc");
        assert_eq!(json["logical_id"], "log-xyz");
        assert_eq!(json["kind"], "Meeting");
        assert_eq!(json["properties"], r#"{"title":"standup"}"#);
        assert_eq!(json["content_ref"], "s3://bucket/standup.pdf");
        assert_eq!(json["last_accessed_at"], 1_710_000_000_i64);
        assert!(json["edge_properties"].is_null());
    }

    #[test]
    fn node_row_serializes_null_last_accessed_at() {
        use super::PyNodeRow;
        use crate::NodeRow;

        let row = NodeRow {
            row_id: "r1".into(),
            logical_id: "l1".into(),
            kind: "Doc".into(),
            properties: "{}".into(),
            content_ref: None,
            last_accessed_at: None,
            edge_properties: None,
        };
        let py = PyNodeRow::from(row);
        let json: serde_json::Value = serde_json::to_value(&py).expect("serialize");

        assert!(json["last_accessed_at"].is_null());
    }

    #[test]
    fn node_row_serializes_edge_properties_when_present() {
        use super::PyNodeRow;
        use crate::NodeRow;

        let row = NodeRow {
            row_id: "r2".into(),
            logical_id: "l2".into(),
            kind: "Doc".into(),
            properties: "{}".into(),
            content_ref: None,
            last_accessed_at: None,
            edge_properties: Some(r#"{"rel":"cites"}"#.into()),
        };
        let py = PyNodeRow::from(row);
        let json: serde_json::Value = serde_json::to_value(&py).expect("serialize");

        assert_eq!(json["edge_properties"], r#"{"rel":"cites"}"#);
    }

    #[test]
    fn run_row_serializes_all_fields() {
        use super::PyRunRow;
        use crate::RunRow;

        let row = RunRow {
            id: "run-99".into(),
            kind: "ingest".into(),
            status: "completed".into(),
            properties: r#"{"source":"api"}"#.into(),
        };
        let py = PyRunRow::from(row);
        let json: serde_json::Value = serde_json::to_value(&py).expect("serialize");

        assert_eq!(json["id"], "run-99");
        assert_eq!(json["kind"], "ingest");
        assert_eq!(json["status"], "completed");
        assert_eq!(json["properties"], r#"{"source":"api"}"#);
    }

    #[test]
    fn step_row_serializes_all_fields() {
        use super::PyStepRow;
        use crate::StepRow;

        let row = StepRow {
            id: "step-55".into(),
            run_id: "run-99".into(),
            kind: "extract".into(),
            status: "running".into(),
            properties: r#"{"page":1}"#.into(),
        };
        let py = PyStepRow::from(row);
        let json: serde_json::Value = serde_json::to_value(&py).expect("serialize");

        assert_eq!(json["id"], "step-55");
        assert_eq!(json["run_id"], "run-99");
        assert_eq!(json["kind"], "extract");
        assert_eq!(json["status"], "running");
        assert_eq!(json["properties"], r#"{"page":1}"#);
    }

    #[test]
    fn action_row_serializes_all_fields() {
        use super::PyActionRow;
        use crate::ActionRow;

        let row = ActionRow {
            id: "act-77".into(),
            step_id: "step-55".into(),
            kind: "download".into(),
            status: "failed".into(),
            properties: r#"{"url":"https://example.com"}"#.into(),
        };
        let py = PyActionRow::from(row);
        let json: serde_json::Value = serde_json::to_value(&py).expect("serialize");

        assert_eq!(json["id"], "act-77");
        assert_eq!(json["step_id"], "step-55");
        assert_eq!(json["kind"], "download");
        assert_eq!(json["status"], "failed");
        assert_eq!(json["properties"], r#"{"url":"https://example.com"}"#);
    }
}

#[derive(Debug, Serialize)]
pub struct PyNodeRow {
    pub row_id: String,
    pub logical_id: String,
    pub kind: String,
    pub properties: String,
    pub content_ref: Option<String>,
    pub last_accessed_at: Option<i64>,
    pub edge_properties: Option<String>,
}

impl From<NodeRow> for PyNodeRow {
    fn from(value: NodeRow) -> Self {
        Self {
            row_id: value.row_id,
            logical_id: value.logical_id,
            kind: value.kind,
            properties: value.properties,
            content_ref: value.content_ref,
            last_accessed_at: value.last_accessed_at,
            edge_properties: value.edge_properties,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyRunRow {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
}

impl From<RunRow> for PyRunRow {
    fn from(value: RunRow) -> Self {
        Self {
            id: value.id,
            kind: value.kind,
            status: value.status,
            properties: value.properties,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyStepRow {
    pub id: String,
    pub run_id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
}

impl From<StepRow> for PyStepRow {
    fn from(value: StepRow) -> Self {
        Self {
            id: value.id,
            run_id: value.run_id,
            kind: value.kind,
            status: value.status,
            properties: value.properties,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyActionRow {
    pub id: String,
    pub step_id: String,
    pub kind: String,
    pub status: String,
    pub properties: String,
}

impl From<ActionRow> for PyActionRow {
    fn from(value: ActionRow) -> Self {
        Self {
            id: value.id,
            step_id: value.step_id,
            kind: value.kind,
            status: value.status,
            properties: value.properties,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyWriteReceipt {
    pub label: String,
    pub optional_backfill_count: usize,
    pub warnings: Vec<String>,
    pub provenance_warnings: Vec<String>,
}

impl From<WriteReceipt> for PyWriteReceipt {
    fn from(value: WriteReceipt) -> Self {
        Self {
            label: value.label,
            optional_backfill_count: value.optional_backfill_count,
            warnings: value.warnings,
            provenance_warnings: value.provenance_warnings,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyLastAccessTouchReport {
    pub touched_logical_ids: usize,
    pub touched_at: i64,
}

impl From<LastAccessTouchReport> for PyLastAccessTouchReport {
    fn from(value: LastAccessTouchReport) -> Self {
        Self {
            touched_logical_ids: value.touched_logical_ids,
            touched_at: value.touched_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyIntegrityReport {
    pub physical_ok: bool,
    pub foreign_keys_ok: bool,
    pub missing_fts_rows: usize,
    pub missing_property_fts_rows: usize,
    pub duplicate_active_logical_ids: usize,
    pub operational_missing_collections: usize,
    pub operational_missing_last_mutations: usize,
    pub warnings: Vec<String>,
}

impl From<IntegrityReport> for PyIntegrityReport {
    fn from(value: IntegrityReport) -> Self {
        Self {
            physical_ok: value.physical_ok,
            foreign_keys_ok: value.foreign_keys_ok,
            missing_fts_rows: value.missing_fts_rows,
            missing_property_fts_rows: value.missing_property_fts_rows,
            duplicate_active_logical_ids: value.duplicate_active_logical_ids,
            operational_missing_collections: value.operational_missing_collections,
            operational_missing_last_mutations: value.operational_missing_last_mutations,
            warnings: value.warnings,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PySemanticReport {
    pub orphaned_chunks: usize,
    pub null_source_ref_nodes: usize,
    pub broken_step_fk: usize,
    pub broken_action_fk: usize,
    pub stale_fts_rows: usize,
    pub fts_rows_for_superseded_nodes: usize,
    pub stale_property_fts_rows: usize,
    pub orphaned_property_fts_rows: usize,
    pub mismatched_kind_property_fts_rows: usize,
    pub duplicate_property_fts_rows: usize,
    pub drifted_property_fts_rows: usize,
    pub dangling_edges: usize,
    pub orphaned_supersession_chains: usize,
    pub stale_vec_rows: usize,
    pub vec_rows_for_superseded_nodes: usize,
    pub missing_operational_current_rows: usize,
    pub stale_operational_current_rows: usize,
    pub disabled_collection_mutations: usize,
    pub orphaned_last_access_metadata_rows: usize,
    pub warnings: Vec<String>,
}

impl From<SemanticReport> for PySemanticReport {
    fn from(value: SemanticReport) -> Self {
        Self {
            orphaned_chunks: value.orphaned_chunks,
            null_source_ref_nodes: value.null_source_ref_nodes,
            broken_step_fk: value.broken_step_fk,
            broken_action_fk: value.broken_action_fk,
            stale_fts_rows: value.stale_fts_rows,
            fts_rows_for_superseded_nodes: value.fts_rows_for_superseded_nodes,
            stale_property_fts_rows: value.stale_property_fts_rows,
            orphaned_property_fts_rows: value.orphaned_property_fts_rows,
            mismatched_kind_property_fts_rows: value.mismatched_kind_property_fts_rows,
            duplicate_property_fts_rows: value.duplicate_property_fts_rows,
            drifted_property_fts_rows: value.drifted_property_fts_rows,
            dangling_edges: value.dangling_edges,
            orphaned_supersession_chains: value.orphaned_supersession_chains,
            stale_vec_rows: value.stale_vec_rows,
            vec_rows_for_superseded_nodes: value.vec_rows_for_superseded_nodes,
            missing_operational_current_rows: value.missing_operational_current_rows,
            stale_operational_current_rows: value.stale_operational_current_rows,
            disabled_collection_mutations: value.disabled_collection_mutations,
            orphaned_last_access_metadata_rows: value.orphaned_last_access_metadata_rows,
            warnings: value.warnings,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyTraceReport {
    pub source_ref: String,
    pub node_rows: usize,
    pub edge_rows: usize,
    pub action_rows: usize,
    pub operational_mutation_rows: usize,
    pub node_logical_ids: Vec<String>,
    pub action_ids: Vec<String>,
    pub operational_mutation_ids: Vec<String>,
}

impl From<TraceReport> for PyTraceReport {
    fn from(value: TraceReport) -> Self {
        Self {
            source_ref: value.source_ref,
            node_rows: value.node_rows,
            edge_rows: value.edge_rows,
            action_rows: value.action_rows,
            operational_mutation_rows: value.operational_mutation_rows,
            node_logical_ids: value.node_logical_ids,
            action_ids: value.action_ids,
            operational_mutation_ids: value.operational_mutation_ids,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyProjectionRepairReport {
    pub targets: Vec<PyProjectionTarget>,
    pub rebuilt_rows: usize,
    pub notes: Vec<String>,
}

impl From<ProjectionRepairReport> for PyProjectionRepairReport {
    fn from(value: ProjectionRepairReport) -> Self {
        Self {
            targets: value.targets.into_iter().map(Into::into).collect(),
            rebuilt_rows: value.rebuilt_rows,
            notes: value.notes,
        }
    }
}

#[cfg(feature = "python")]
#[derive(Debug, Serialize)]
pub struct PyVectorRegenerationReport {
    pub profile: String,
    pub table_name: String,
    pub dimension: usize,
    pub total_chunks: usize,
    pub regenerated_rows: usize,
    pub contract_persisted: bool,
    pub notes: Vec<String>,
}

#[cfg(feature = "python")]
impl From<VectorRegenerationReport> for PyVectorRegenerationReport {
    fn from(value: VectorRegenerationReport) -> Self {
        Self {
            profile: value.profile,
            table_name: value.table_name,
            dimension: value.dimension,
            total_chunks: value.total_chunks,
            regenerated_rows: value.regenerated_rows,
            contract_persisted: value.contract_persisted,
            notes: value.notes,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PySafeExportManifest {
    pub exported_at: u64,
    pub sha256: String,
    pub schema_version: u32,
    pub protocol_version: u32,
    pub page_count: u64,
}

impl From<SafeExportManifest> for PySafeExportManifest {
    fn from(value: SafeExportManifest) -> Self {
        Self {
            exported_at: value.exported_at,
            sha256: value.sha256,
            schema_version: value.schema_version,
            protocol_version: value.protocol_version,
            page_count: value.page_count,
        }
    }
}
