mod ast;
mod builder;
mod compile;
mod plan;
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
pub use plan::{DrivingTable, ExecutionHints};
pub use search::{
    CompiledSearch, HitAttribution, NodeRowLite, SearchHit, SearchHitSource, SearchMatchMode,
    SearchRows,
};
pub use text_query::{TextQuery, render_text_query_fts5};

pub type Query = QueryBuilder;
