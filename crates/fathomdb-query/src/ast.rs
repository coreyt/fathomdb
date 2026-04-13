use crate::TextQuery;

/// Abstract syntax tree representing a graph query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryAst {
    /// Node kind used as the root of the query.
    pub root_kind: String,
    /// Ordered pipeline of search, traversal, and filter steps.
    pub steps: Vec<QueryStep>,
    /// Named expansion slots evaluated per root result in grouped queries.
    pub expansions: Vec<ExpansionSlot>,
    /// Optional hard cap on the number of result rows.
    pub final_limit: Option<usize>,
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
}

/// A single step in the query pipeline.
#[derive(Clone, Debug, PartialEq, Eq)]
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
