use std::sync::atomic::{AtomicU64, Ordering};

use crate::{
    ActionInsert, ChunkInsert, ChunkPolicy, EdgeInsert, EdgeRetire, EngineError, NodeInsert,
    NodeRetire, OperationalWrite, OptionalProjectionTask, ProjectionTarget, RunInsert, StepInsert,
    VecInsert, WriteRequest,
};

static NEXT_BUILDER_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeHandle {
    builder_id: u64,
    pub row_id: String,
    pub logical_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeHandle {
    builder_id: u64,
    pub logical_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunHandle {
    builder_id: u64,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepHandle {
    builder_id: u64,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionHandle {
    builder_id: u64,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkHandle {
    builder_id: u64,
    pub id: String,
    pub node_logical_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeRef {
    Existing(String),
    Handle(NodeHandle),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EdgeRef {
    Existing(String),
    Handle(EdgeHandle),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunRef {
    Existing(String),
    Handle(RunHandle),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepRef {
    Existing(String),
    Handle(StepHandle),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChunkRef {
    Existing(String),
    Handle(ChunkHandle),
}

impl From<String> for NodeRef {
    fn from(value: String) -> Self {
        Self::Existing(value)
    }
}

impl From<&str> for NodeRef {
    fn from(value: &str) -> Self {
        Self::Existing(value.to_owned())
    }
}

impl From<NodeHandle> for NodeRef {
    fn from(value: NodeHandle) -> Self {
        Self::Handle(value)
    }
}

impl From<&NodeHandle> for NodeRef {
    fn from(value: &NodeHandle) -> Self {
        Self::Handle(value.clone())
    }
}

impl From<String> for EdgeRef {
    fn from(value: String) -> Self {
        Self::Existing(value)
    }
}

impl From<&str> for EdgeRef {
    fn from(value: &str) -> Self {
        Self::Existing(value.to_owned())
    }
}

impl From<EdgeHandle> for EdgeRef {
    fn from(value: EdgeHandle) -> Self {
        Self::Handle(value)
    }
}

impl From<&EdgeHandle> for EdgeRef {
    fn from(value: &EdgeHandle) -> Self {
        Self::Handle(value.clone())
    }
}

impl From<String> for RunRef {
    fn from(value: String) -> Self {
        Self::Existing(value)
    }
}

impl From<&str> for RunRef {
    fn from(value: &str) -> Self {
        Self::Existing(value.to_owned())
    }
}

impl From<RunHandle> for RunRef {
    fn from(value: RunHandle) -> Self {
        Self::Handle(value)
    }
}

impl From<&RunHandle> for RunRef {
    fn from(value: &RunHandle) -> Self {
        Self::Handle(value.clone())
    }
}

impl From<String> for StepRef {
    fn from(value: String) -> Self {
        Self::Existing(value)
    }
}

impl From<&str> for StepRef {
    fn from(value: &str) -> Self {
        Self::Existing(value.to_owned())
    }
}

impl From<StepHandle> for StepRef {
    fn from(value: StepHandle) -> Self {
        Self::Handle(value)
    }
}

impl From<&StepHandle> for StepRef {
    fn from(value: &StepHandle) -> Self {
        Self::Handle(value.clone())
    }
}

impl From<String> for ChunkRef {
    fn from(value: String) -> Self {
        Self::Existing(value)
    }
}

impl From<&str> for ChunkRef {
    fn from(value: &str) -> Self {
        Self::Existing(value.to_owned())
    }
}

impl From<ChunkHandle> for ChunkRef {
    fn from(value: ChunkHandle) -> Self {
        Self::Handle(value)
    }
}

impl From<&ChunkHandle> for ChunkRef {
    fn from(value: &ChunkHandle) -> Self {
        Self::Handle(value.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingEdgeInsert {
    row_id: String,
    logical_id: String,
    source: NodeRef,
    target: NodeRef,
    kind: String,
    properties: String,
    source_ref: Option<String>,
    upsert: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingNodeRetire {
    logical_id: NodeRef,
    source_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingEdgeRetire {
    logical_id: EdgeRef,
    source_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingChunkInsert {
    id: String,
    node: NodeRef,
    text_content: String,
    byte_start: Option<i64>,
    byte_end: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingStepInsert {
    id: String,
    run: RunRef,
    kind: String,
    status: String,
    properties: String,
    source_ref: Option<String>,
    upsert: bool,
    supersedes_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingActionInsert {
    id: String,
    step: StepRef,
    kind: String,
    status: String,
    properties: String,
    source_ref: Option<String>,
    upsert: bool,
    supersedes_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct PendingVecInsert {
    chunk: ChunkRef,
    embedding: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WriteRequestBuilder {
    builder_id: u64,
    label: String,
    nodes: Vec<NodeInsert>,
    node_retires: Vec<PendingNodeRetire>,
    edges: Vec<PendingEdgeInsert>,
    edge_retires: Vec<PendingEdgeRetire>,
    chunks: Vec<PendingChunkInsert>,
    runs: Vec<RunInsert>,
    steps: Vec<PendingStepInsert>,
    actions: Vec<PendingActionInsert>,
    optional_backfills: Vec<OptionalProjectionTask>,
    vec_inserts: Vec<PendingVecInsert>,
    operational_writes: Vec<OperationalWrite>,
}

#[allow(clippy::too_many_arguments, clippy::missing_errors_doc, clippy::too_many_lines)]
impl WriteRequestBuilder {
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            builder_id: NEXT_BUILDER_ID.fetch_add(1, Ordering::Relaxed),
            label: label.into(),
            nodes: Vec::new(),
            node_retires: Vec::new(),
            edges: Vec::new(),
            edge_retires: Vec::new(),
            chunks: Vec::new(),
            runs: Vec::new(),
            steps: Vec::new(),
            actions: Vec::new(),
            optional_backfills: Vec::new(),
            vec_inserts: Vec::new(),
            operational_writes: Vec::new(),
        }
    }

    pub fn add_node(
        &mut self,
        row_id: impl Into<String>,
        logical_id: impl Into<String>,
        kind: impl Into<String>,
        properties: impl Into<String>,
        source_ref: Option<String>,
        upsert: bool,
        chunk_policy: ChunkPolicy,
    ) -> NodeHandle {
        let handle = NodeHandle {
            builder_id: self.builder_id,
            row_id: row_id.into(),
            logical_id: logical_id.into(),
        };
        self.nodes.push(NodeInsert {
            row_id: handle.row_id.clone(),
            logical_id: handle.logical_id.clone(),
            kind: kind.into(),
            properties: properties.into(),
            source_ref,
            upsert,
            chunk_policy,
        });
        handle
    }

    pub fn retire_node(&mut self, logical_id: impl Into<NodeRef>, source_ref: Option<String>) {
        self.node_retires.push(PendingNodeRetire {
            logical_id: logical_id.into(),
            source_ref,
        });
    }

    pub fn add_edge(
        &mut self,
        row_id: impl Into<String>,
        logical_id: impl Into<String>,
        source: impl Into<NodeRef>,
        target: impl Into<NodeRef>,
        kind: impl Into<String>,
        properties: impl Into<String>,
        source_ref: Option<String>,
        upsert: bool,
    ) -> EdgeHandle {
        let handle = EdgeHandle {
            builder_id: self.builder_id,
            logical_id: logical_id.into(),
        };
        self.edges.push(PendingEdgeInsert {
            row_id: row_id.into(),
            logical_id: handle.logical_id.clone(),
            source: source.into(),
            target: target.into(),
            kind: kind.into(),
            properties: properties.into(),
            source_ref,
            upsert,
        });
        handle
    }

    pub fn retire_edge(&mut self, logical_id: impl Into<EdgeRef>, source_ref: Option<String>) {
        self.edge_retires.push(PendingEdgeRetire {
            logical_id: logical_id.into(),
            source_ref,
        });
    }

    pub fn add_chunk(
        &mut self,
        id: impl Into<String>,
        node: impl Into<NodeRef>,
        text_content: impl Into<String>,
        byte_start: Option<i64>,
        byte_end: Option<i64>,
    ) -> ChunkHandle {
        let id = id.into();
        let node = node.into();
        let node_logical_id = match &node {
            NodeRef::Existing(logical_id) => logical_id.clone(),
            NodeRef::Handle(handle) => handle.logical_id.clone(),
        };
        self.chunks.push(PendingChunkInsert {
            id: id.clone(),
            node,
            text_content: text_content.into(),
            byte_start,
            byte_end,
        });
        ChunkHandle {
            builder_id: self.builder_id,
            id,
            node_logical_id,
        }
    }

    pub fn add_run(
        &mut self,
        id: impl Into<String>,
        kind: impl Into<String>,
        status: impl Into<String>,
        properties: impl Into<String>,
        source_ref: Option<String>,
        upsert: bool,
        supersedes_id: Option<String>,
    ) -> RunHandle {
        let handle = RunHandle {
            builder_id: self.builder_id,
            id: id.into(),
        };
        self.runs.push(RunInsert {
            id: handle.id.clone(),
            kind: kind.into(),
            status: status.into(),
            properties: properties.into(),
            source_ref,
            upsert,
            supersedes_id,
        });
        handle
    }

    pub fn add_step(
        &mut self,
        id: impl Into<String>,
        run: impl Into<RunRef>,
        kind: impl Into<String>,
        status: impl Into<String>,
        properties: impl Into<String>,
        source_ref: Option<String>,
        upsert: bool,
        supersedes_id: Option<String>,
    ) -> StepHandle {
        let handle = StepHandle {
            builder_id: self.builder_id,
            id: id.into(),
        };
        self.steps.push(PendingStepInsert {
            id: handle.id.clone(),
            run: run.into(),
            kind: kind.into(),
            status: status.into(),
            properties: properties.into(),
            source_ref,
            upsert,
            supersedes_id,
        });
        handle
    }

    pub fn add_action(
        &mut self,
        id: impl Into<String>,
        step: impl Into<StepRef>,
        kind: impl Into<String>,
        status: impl Into<String>,
        properties: impl Into<String>,
        source_ref: Option<String>,
        upsert: bool,
        supersedes_id: Option<String>,
    ) -> ActionHandle {
        let handle = ActionHandle {
            builder_id: self.builder_id,
            id: id.into(),
        };
        self.actions.push(PendingActionInsert {
            id: handle.id.clone(),
            step: step.into(),
            kind: kind.into(),
            status: status.into(),
            properties: properties.into(),
            source_ref,
            upsert,
            supersedes_id,
        });
        handle
    }

    pub fn add_optional_backfill(&mut self, target: ProjectionTarget, payload: impl Into<String>) {
        self.optional_backfills.push(OptionalProjectionTask {
            target,
            payload: payload.into(),
        });
    }

    pub fn add_vec_insert(&mut self, chunk: impl Into<ChunkRef>, embedding: Vec<f32>) {
        self.vec_inserts.push(PendingVecInsert {
            chunk: chunk.into(),
            embedding,
        });
    }

    pub fn add_operational_append(
        &mut self,
        collection: impl Into<String>,
        record_key: impl Into<String>,
        payload_json: impl Into<String>,
        source_ref: Option<String>,
    ) {
        self.operational_writes.push(OperationalWrite::Append {
            collection: collection.into(),
            record_key: record_key.into(),
            payload_json: payload_json.into(),
            source_ref,
        });
    }

    pub fn add_operational_put(
        &mut self,
        collection: impl Into<String>,
        record_key: impl Into<String>,
        payload_json: impl Into<String>,
        source_ref: Option<String>,
    ) {
        self.operational_writes.push(OperationalWrite::Put {
            collection: collection.into(),
            record_key: record_key.into(),
            payload_json: payload_json.into(),
            source_ref,
        });
    }

    pub fn add_operational_delete(
        &mut self,
        collection: impl Into<String>,
        record_key: impl Into<String>,
        source_ref: Option<String>,
    ) {
        self.operational_writes.push(OperationalWrite::Delete {
            collection: collection.into(),
            record_key: record_key.into(),
            source_ref,
        });
    }

    pub fn build(self) -> Result<WriteRequest, EngineError> {
        let builder_id = self.builder_id;
        let nodes = self.nodes;
        let node_retires = self
            .node_retires
            .into_iter()
            .map(|retire| {
                Ok(NodeRetire {
                    logical_id: resolve_node_ref(builder_id, retire.logical_id)?,
                    source_ref: retire.source_ref,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        let edges = self
            .edges
            .into_iter()
            .map(|edge| {
                Ok(EdgeInsert {
                    row_id: edge.row_id,
                    logical_id: edge.logical_id,
                    source_logical_id: resolve_node_ref(builder_id, edge.source)?,
                    target_logical_id: resolve_node_ref(builder_id, edge.target)?,
                    kind: edge.kind,
                    properties: edge.properties,
                    source_ref: edge.source_ref,
                    upsert: edge.upsert,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        let edge_retires = self
            .edge_retires
            .into_iter()
            .map(|retire| {
                Ok(EdgeRetire {
                    logical_id: resolve_edge_ref(builder_id, retire.logical_id)?,
                    source_ref: retire.source_ref,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        let chunks = self
            .chunks
            .into_iter()
            .map(|chunk| {
                Ok(ChunkInsert {
                    id: chunk.id,
                    node_logical_id: resolve_node_ref(builder_id, chunk.node)?,
                    text_content: chunk.text_content,
                    byte_start: chunk.byte_start,
                    byte_end: chunk.byte_end,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        let runs = self.runs;
        let steps = self
            .steps
            .into_iter()
            .map(|step| {
                Ok(StepInsert {
                    id: step.id,
                    run_id: resolve_run_ref(builder_id, step.run)?,
                    kind: step.kind,
                    status: step.status,
                    properties: step.properties,
                    source_ref: step.source_ref,
                    upsert: step.upsert,
                    supersedes_id: step.supersedes_id,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        let actions = self
            .actions
            .into_iter()
            .map(|action| {
                Ok(ActionInsert {
                    id: action.id,
                    step_id: resolve_step_ref(builder_id, action.step)?,
                    kind: action.kind,
                    status: action.status,
                    properties: action.properties,
                    source_ref: action.source_ref,
                    upsert: action.upsert,
                    supersedes_id: action.supersedes_id,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;
        let vec_inserts = self
            .vec_inserts
            .into_iter()
            .map(|vec_insert| {
                Ok(VecInsert {
                    chunk_id: resolve_chunk_ref(builder_id, vec_insert.chunk)?,
                    embedding: vec_insert.embedding,
                })
            })
            .collect::<Result<Vec<_>, EngineError>>()?;

        Ok(WriteRequest {
            label: self.label,
            nodes,
            node_retires,
            edges,
            edge_retires,
            chunks,
            runs,
            steps,
            actions,
            optional_backfills: self.optional_backfills,
            vec_inserts,
            operational_writes: self.operational_writes,
        })
    }
}

fn resolve_node_ref(builder_id: u64, value: NodeRef) -> Result<String, EngineError> {
    match value {
        NodeRef::Existing(logical_id) => Ok(logical_id),
        NodeRef::Handle(handle) if handle.builder_id == builder_id => Ok(handle.logical_id),
        NodeRef::Handle(_) => Err(EngineError::InvalidWrite(
            "node handle belongs to a different WriteRequestBuilder".to_owned(),
        )),
    }
}

fn resolve_edge_ref(builder_id: u64, value: EdgeRef) -> Result<String, EngineError> {
    match value {
        EdgeRef::Existing(logical_id) => Ok(logical_id),
        EdgeRef::Handle(handle) if handle.builder_id == builder_id => Ok(handle.logical_id),
        EdgeRef::Handle(_) => Err(EngineError::InvalidWrite(
            "edge handle belongs to a different WriteRequestBuilder".to_owned(),
        )),
    }
}

fn resolve_run_ref(builder_id: u64, value: RunRef) -> Result<String, EngineError> {
    match value {
        RunRef::Existing(id) => Ok(id),
        RunRef::Handle(handle) if handle.builder_id == builder_id => Ok(handle.id),
        RunRef::Handle(_) => Err(EngineError::InvalidWrite(
            "run handle belongs to a different WriteRequestBuilder".to_owned(),
        )),
    }
}

fn resolve_step_ref(builder_id: u64, value: StepRef) -> Result<String, EngineError> {
    match value {
        StepRef::Existing(id) => Ok(id),
        StepRef::Handle(handle) if handle.builder_id == builder_id => Ok(handle.id),
        StepRef::Handle(_) => Err(EngineError::InvalidWrite(
            "step handle belongs to a different WriteRequestBuilder".to_owned(),
        )),
    }
}

fn resolve_chunk_ref(builder_id: u64, value: ChunkRef) -> Result<String, EngineError> {
    match value {
        ChunkRef::Existing(id) => Ok(id),
        ChunkRef::Handle(handle) if handle.builder_id == builder_id => Ok(handle.id),
        ChunkRef::Handle(_) => Err(EngineError::InvalidWrite(
            "chunk handle belongs to a different WriteRequestBuilder".to_owned(),
        )),
    }
}
