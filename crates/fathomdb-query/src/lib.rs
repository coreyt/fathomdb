mod ast;
mod builder;
mod compile;
mod fusion;
mod plan;
mod relax;
mod search;
mod text_query;

pub use ast::{
    ComparisonOp, ExpansionSlot, Predicate, QueryAst, QueryStep, ScalarValue, TraverseDirection,
};
pub use builder::QueryBuilder;
pub use compile::{
    BindValue, CompileError, CompiledGroupedQuery, CompiledQuery, ShapeHash, compile_grouped_query,
    compile_query, compile_search,
};
pub use fusion::{is_fusable, partition_predicates, partition_search_filters};
pub use plan::{DrivingTable, ExecutionHints};
pub use relax::{FALLBACK_TRIGGER_K, RELAXED_BRANCH_CAP, derive_relaxed};
pub use search::{
    CompiledSearch, HitAttribution, NodeRowLite, SearchBranch, SearchHit, SearchHitSource,
    SearchMatchMode, SearchPlan, SearchRows,
};
pub use text_query::{TextQuery, render_text_query_fts5};

pub type Query = QueryBuilder;
