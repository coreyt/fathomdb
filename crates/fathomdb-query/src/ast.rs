use crate::TextQuery;

/// Abstract syntax tree representing a graph query.
///
/// `Eq` is intentionally NOT derived because [`QueryStep::RawVectorSearch`]
/// carries a `Vec<f32>` payload; `Vec<f32>` implements `PartialEq` but not
/// `Eq` (IEEE-754 NaN breaks reflexivity). Existing `PartialEq`-only
/// callers are unaffected.
#[derive(Clone, Debug, PartialEq)]
pub struct QueryAst {
    /// Node kind used as the root of the query.
    pub root_kind: String,
    /// Ordered pipeline of search, traversal, and filter steps.
    pub steps: Vec<QueryStep>,
    /// Named expansion slots evaluated per root result in grouped queries.
    pub expansions: Vec<ExpansionSlot>,
    /// Named edge-projecting expansion slots evaluated per root result in
    /// grouped queries. Sibling to `expansions`; vec membership is the
    /// discriminator between node- and edge-expansion slots. Slot names
    /// must be unique across both vecs.
    pub edge_expansions: Vec<EdgeExpansionSlot>,
    /// Optional hard cap on the number of result rows.
    pub final_limit: Option<usize>,
}

/// An edge-projecting expansion slot.
///
/// Emits `(EdgeRow, NodeRow)` tuples per root on execution. The endpoint
/// node is the target on `Out` traversal, source on `In`. For
/// `max_depth > 1`, each emitted tuple reflects the final-hop edge
/// leading to the emitted endpoint node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeExpansionSlot {
    /// Slot name used to key the expansion results. Must be unique across
    /// both node-expansion and edge-expansion slots in the same query.
    pub slot: String,
    /// Direction to traverse edges.
    pub direction: TraverseDirection,
    /// Edge kind (label) to follow.
    pub label: String,
    /// Maximum traversal depth.
    pub max_depth: usize,
    /// Optional predicate filtering the endpoint node (the target side on
    /// `Out`, the source side on `In`). Reuses the `Predicate` enum.
    pub endpoint_filter: Option<Predicate>,
    /// Optional predicate filtering the traversed edges. Only
    /// `EdgePropertyEq` and `EdgePropertyCompare` are valid here.
    pub edge_filter: Option<Predicate>,
}

/// A named expansion slot that traverses edges per root result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpansionSlot {
    /// Slot name used to key the expansion results.
    pub slot: String,
    /// Direction to traverse edges.
    pub direction: TraverseDirection,
    /// Edge kind (label) to follow.
    pub label: String,
    /// Maximum traversal depth.
    pub max_depth: usize,
    /// Optional predicate to filter target nodes in this expansion slot.
    /// `None` is exactly equivalent to pre-Pack-2 behavior.
    /// `Some(_)` is not yet implemented; see Pack 3.
    pub filter: Option<Predicate>,
    /// Optional predicate to filter the traversed edges in this expansion slot.
    /// Only `EdgePropertyEq` and `EdgePropertyCompare` are valid here.
    /// `None` preserves pre-Pack-D behavior (no edge filtering).
    pub edge_filter: Option<Predicate>,
}

/// A single step in the query pipeline.
///
/// `Eq` is intentionally NOT derived — see [`QueryAst`] for the rationale.
#[derive(Clone, Debug, PartialEq)]
pub enum QueryStep {
    /// Unified adaptive retrieval entry step consumed by the Phase 12
    /// retrieval planner.
    ///
    /// Carries the caller's raw query string (not a parsed [`TextQuery`]):
    /// the planner decides how to interpret and route it across the text
    /// strict, text relaxed, and (future) vector branches. See
    /// `crate::compile_retrieval_plan` for the planner entry point.
    Search {
        /// The raw caller-supplied query string.
        query: String,
        /// Maximum number of candidate rows requested by the caller.
        limit: usize,
    },
    /// Nearest-neighbor search over vector embeddings.
    VectorSearch {
        /// The search query text (to be embedded by the caller).
        query: String,
        /// Maximum number of candidate rows from the vector index.
        limit: usize,
    },
    /// Pack F1 semantic-search step: a natural-language query that the
    /// engine embeds at query time using the db-wide active profile
    /// embedder, then runs KNN against the per-kind `vec_<kind>` table.
    ///
    /// Unlike [`QueryStep::VectorSearch`], the `text` is NEVER a JSON
    /// float-array literal — the caller supplies natural language and
    /// the engine handles embedding internally.
    SemanticSearch {
        /// Natural-language query string to embed at query time.
        text: String,
        /// Maximum number of candidate rows from the vector KNN scan.
        limit: usize,
    },
    /// Pack F1 raw-vector-search step: a caller-supplied dense vector
    /// that the engine binds directly to the per-kind `vec_<kind>` KNN
    /// scan with no embedder call. The vector's dimension must match the
    /// active embedding profile's dimension or the coordinator returns
    /// [`super::CompileError`]-free with a hard `DimensionMismatch`
    /// error at plan-time.
    RawVectorSearch {
        /// Caller-supplied dense vector.
        vec: Vec<f32>,
        /// Maximum number of candidate rows from the vector KNN scan.
        limit: usize,
    },
    /// Full-text search over indexed chunk content using `FathomDB`'s supported
    /// safe text-query subset.
    TextSearch {
        /// Parsed text-search intent to be lowered into safe FTS5 syntax.
        query: TextQuery,
        /// Maximum number of candidate rows from the FTS index.
        limit: usize,
    },
    /// Graph traversal following edges of the given label.
    Traverse {
        /// Direction to traverse.
        direction: TraverseDirection,
        /// Edge kind to follow.
        label: String,
        /// Maximum hops from each candidate.
        max_depth: usize,
        /// Optional predicate to filter traversal results.
        /// `None` is exactly equivalent to the pre-Pack-2 behavior.
        /// `Some(_)` is not yet implemented; see Pack 3.
        filter: Option<Predicate>,
    },
    /// Row-level filter predicate.
    Filter(Predicate),
}

/// A filter predicate applied to candidate nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Predicate {
    /// Match nodes with the exact logical ID.
    LogicalIdEq(String),
    /// Match nodes with the exact kind.
    KindEq(String),
    /// Equality check on a JSON property at the given path.
    JsonPathEq {
        /// JSON path expression (e.g. `$.status`).
        path: String,
        /// Value to compare against.
        value: ScalarValue,
    },
    /// Ordered comparison on a JSON property at the given path.
    JsonPathCompare {
        /// JSON path expression.
        path: String,
        /// Comparison operator.
        op: ComparisonOp,
        /// Value to compare against.
        value: ScalarValue,
    },
    /// Match nodes with the exact `source_ref`.
    SourceRefEq(String),
    /// Match nodes where `content_ref` is not NULL (i.e. content proxy nodes).
    ContentRefNotNull,
    /// Match nodes with the exact `content_ref` URI.
    ContentRefEq(String),
    /// Fused equality check on a JSON text property at the given path.
    ///
    /// Unlike [`Predicate::JsonPathEq`], this variant is classified as
    /// **fusable** by [`crate::fusion::is_fusable`] and is pushed into
    /// the search CTE's inner `WHERE` clause so the CTE `LIMIT` applies
    /// after the predicate runs. The caller opts into fusion by
    /// registering an FTS property schema that covers the path; the
    /// tethered builder enforces that gate at filter-add time.
    JsonPathFusedEq {
        /// JSON path expression (e.g. `$.status`).
        path: String,
        /// Text value to compare against.
        value: String,
    },
    /// Fused ordered comparison on a JSON integer/timestamp property at
    /// the given path. See [`Predicate::JsonPathFusedEq`] for the fusion
    /// contract.
    JsonPathFusedTimestampCmp {
        /// JSON path expression.
        path: String,
        /// Comparison operator.
        op: ComparisonOp,
        /// Integer value to compare against (epoch seconds for
        /// timestamp semantics).
        value: i64,
    },
    /// Fused equality check on a JSON boolean property at the given path.
    /// See [`Predicate::JsonPathFusedEq`] for the fusion contract.
    /// The boolean is stored as `SQLite` integer 1/0.
    JsonPathFusedBoolEq {
        /// JSON path expression (e.g. `$.resolved`).
        path: String,
        /// Boolean value to compare against (stored as 1 or 0).
        value: bool,
    },
    /// Equality check on a JSON property of the traversed edge at the given path.
    ///
    /// Structurally identical to [`Predicate::JsonPathEq`] but targets
    /// `e.properties` on the edge row rather than `n.properties` on the
    /// target node. Only valid inside an expansion slot's `edge_filter`.
    EdgePropertyEq {
        /// JSON path expression (e.g. `$.rel`).
        path: String,
        /// Value to compare against.
        value: ScalarValue,
    },
    /// Ordered comparison on a JSON property of the traversed edge at the given path.
    ///
    /// Structurally identical to [`Predicate::JsonPathCompare`] but targets
    /// `e.properties` on the edge row rather than `n.properties` on the
    /// target node. Only valid inside an expansion slot's `edge_filter`.
    EdgePropertyCompare {
        /// JSON path expression.
        path: String,
        /// Comparison operator.
        op: ComparisonOp,
        /// Value to compare against.
        value: ScalarValue,
    },
    /// Fused IN-set check on a JSON text property at the given path.
    ///
    /// Like [`Predicate::JsonPathFusedEq`], this variant is classified as
    /// **fusable** and is pushed into the search CTE's inner `WHERE` clause.
    /// The caller must have a registered FTS property schema for the path.
    JsonPathFusedIn {
        /// JSON path expression (e.g. `$.status`).
        path: String,
        /// Non-empty set of text values; the node must match at least one.
        values: Vec<String>,
    },
    /// IN-set check on a JSON property at the given path.
    ///
    /// Unlike [`Predicate::JsonPathFusedIn`], this variant is **not** fusable
    /// and is applied as a residual WHERE clause on the Nodes driver scan.
    JsonPathIn {
        /// JSON path expression (e.g. `$.category`).
        path: String,
        /// Non-empty set of values; the node must match at least one.
        values: Vec<ScalarValue>,
    },
}

/// Ordered comparison operator for JSON property filters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparisonOp {
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Gte,
    /// Less than.
    Lt,
    /// Less than or equal.
    Lte,
}

/// A typed scalar value used in query predicates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScalarValue {
    /// A UTF-8 text value.
    Text(String),
    /// A 64-bit signed integer.
    Integer(i64),
    /// A boolean value.
    Bool(bool),
}

/// Direction for graph traversal steps and expansion slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraverseDirection {
    /// Follow edges pointing toward the current node.
    In,
    /// Follow edges pointing away from the current node.
    Out,
}
