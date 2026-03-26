#![cfg(feature = "python")]

use serde::{Deserialize, Serialize};

use crate::{
    ActionInsert, ActionRow, BindValue, ChunkInsert, ChunkPolicy, CompiledQuery, DrivingTable,
    EdgeInsert, EdgeRetire, ExecutionHints, NodeInsert, NodeRetire, NodeRow,
    OptionalProjectionTask, Predicate, ProjectionRepairReport, ProjectionTarget, QueryAst,
    QueryPlan, QueryRows, QueryStep, RunInsert, RunRow, SafeExportManifest, ScalarValue,
    StepInsert, StepRow, TraverseDirection, VecInsert, WriteReceipt, WriteRequest,
};
use fathomdb_engine::{IntegrityReport, SemanticReport, TraceReport};

#[derive(Debug, Deserialize)]
pub struct PyQueryAst {
    pub root_kind: String,
    #[serde(default)]
    pub steps: Vec<PyQueryStep>,
    pub final_limit: Option<usize>,
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
    FilterJsonTextEq {
        path: String,
        value: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize)]
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

impl From<PyQueryAst> for QueryAst {
    fn from(value: PyQueryAst) -> Self {
        let steps = value
            .steps
            .into_iter()
            .map(|step| match step {
                PyQueryStep::VectorSearch { query, limit } => {
                    QueryStep::VectorSearch { query, limit }
                }
                PyQueryStep::TextSearch { query, limit } => QueryStep::TextSearch { query, limit },
                PyQueryStep::Traverse {
                    direction,
                    label,
                    max_depth,
                } => QueryStep::Traverse {
                    direction: direction.into(),
                    label,
                    max_depth,
                },
                PyQueryStep::FilterLogicalIdEq { logical_id } => {
                    QueryStep::Filter(Predicate::LogicalIdEq(logical_id))
                }
                PyQueryStep::FilterKindEq { kind } => QueryStep::Filter(Predicate::KindEq(kind)),
                PyQueryStep::FilterSourceRefEq { source_ref } => {
                    QueryStep::Filter(Predicate::SourceRefEq(source_ref))
                }
                PyQueryStep::FilterJsonTextEq { path, value } => {
                    QueryStep::Filter(Predicate::JsonPathEq {
                        path,
                        value: ScalarValue::Text(value),
                    })
                }
            })
            .collect();

        Self {
            root_kind: value.root_kind,
            steps,
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
}

#[derive(Debug, Deserialize)]
pub struct PyVecInsert {
    pub chunk_id: String,
    pub embedding: Vec<f32>,
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
pub struct PyNodeRow {
    pub row_id: String,
    pub logical_id: String,
    pub kind: String,
    pub properties: String,
}

impl From<NodeRow> for PyNodeRow {
    fn from(value: NodeRow) -> Self {
        Self {
            row_id: value.row_id,
            logical_id: value.logical_id,
            kind: value.kind,
            properties: value.properties,
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
    pub provenance_warnings: Vec<String>,
}

impl From<WriteReceipt> for PyWriteReceipt {
    fn from(value: WriteReceipt) -> Self {
        Self {
            label: value.label,
            optional_backfill_count: value.optional_backfill_count,
            provenance_warnings: value.provenance_warnings,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PyIntegrityReport {
    pub physical_ok: bool,
    pub foreign_keys_ok: bool,
    pub missing_fts_rows: usize,
    pub duplicate_active_logical_ids: usize,
    pub warnings: Vec<String>,
}

impl From<IntegrityReport> for PyIntegrityReport {
    fn from(value: IntegrityReport) -> Self {
        Self {
            physical_ok: value.physical_ok,
            foreign_keys_ok: value.foreign_keys_ok,
            missing_fts_rows: value.missing_fts_rows,
            duplicate_active_logical_ids: value.duplicate_active_logical_ids,
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
    pub dangling_edges: usize,
    pub orphaned_supersession_chains: usize,
    pub stale_vec_rows: usize,
    pub vec_rows_for_superseded_nodes: usize,
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
            dangling_edges: value.dangling_edges,
            orphaned_supersession_chains: value.orphaned_supersession_chains,
            stale_vec_rows: value.stale_vec_rows,
            vec_rows_for_superseded_nodes: value.vec_rows_for_superseded_nodes,
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
    pub node_logical_ids: Vec<String>,
    pub action_ids: Vec<String>,
}

impl From<TraceReport> for PyTraceReport {
    fn from(value: TraceReport) -> Self {
        Self {
            source_ref: value.source_ref,
            node_rows: value.node_rows,
            edge_rows: value.edge_rows,
            action_rows: value.action_rows,
            node_logical_ids: value.node_logical_ids,
            action_ids: value.action_ids,
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
