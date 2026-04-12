//! Tethered query builders for the Phase 1 adaptive search surface.
//!
//! These builders wrap the AST-only [`fathomdb_query::QueryBuilder`] and carry
//! a borrow of the [`Engine`] so that a zero-arg `.execute()` terminal can
//! route to the right coordinator entry point by type. Non-search chains
//! return [`QueryRows`]; `.text_search(...).execute()` returns [`SearchRows`].

use fathomdb_engine::{EngineError, QueryRows};
use fathomdb_query::{
    CompileError, CompiledGroupedQuery, CompiledQuery, CompiledSearchPlan, QueryBuilder,
    SearchRows, TextQuery, compile_search, compile_search_plan_from_queries,
};

use crate::Engine;

/// Tethered node query builder.
///
/// Returned by [`Engine::query`]. Carries an `&Engine` so that terminal
/// methods can dispatch directly to the coordinator. The underlying AST is
/// the same [`QueryBuilder`] the query crate has always produced — this is
/// purely an execution tether, not a new AST.
#[must_use]
pub struct NodeQueryBuilder<'e> {
    engine: &'e Engine,
    inner: QueryBuilder,
}

impl<'e> NodeQueryBuilder<'e> {
    pub(crate) fn new(engine: &'e Engine, kind: impl Into<String>) -> Self {
        Self {
            engine,
            inner: QueryBuilder::nodes(kind),
        }
    }

    /// Transition this chain into a text-search builder. Subsequent filters
    /// accumulate on the search builder and `.execute()` returns
    /// [`SearchRows`] rather than [`QueryRows`].
    pub fn text_search(self, query: impl Into<String>, limit: usize) -> TextSearchBuilder<'e> {
        TextSearchBuilder {
            engine: self.engine,
            inner: self.inner.text_search(query, limit),
            attribution_requested: false,
        }
    }

    /// Add a vector similarity search step.
    pub fn vector_search(mut self, query: impl Into<String>, limit: usize) -> Self {
        self.inner = self.inner.vector_search(query, limit);
        self
    }

    /// Add a graph traversal step.
    pub fn traverse(
        mut self,
        direction: fathomdb_query::TraverseDirection,
        label: impl Into<String>,
        max_depth: usize,
    ) -> Self {
        self.inner = self.inner.traverse(direction, label, max_depth);
        self
    }

    /// Filter results to a single logical ID.
    pub fn filter_logical_id_eq(mut self, logical_id: impl Into<String>) -> Self {
        self.inner = self.inner.filter_logical_id_eq(logical_id);
        self
    }

    /// Filter results to nodes matching the given kind.
    pub fn filter_kind_eq(mut self, kind: impl Into<String>) -> Self {
        self.inner = self.inner.filter_kind_eq(kind);
        self
    }

    /// Filter results to nodes matching the given `source_ref`.
    pub fn filter_source_ref_eq(mut self, source_ref: impl Into<String>) -> Self {
        self.inner = self.inner.filter_source_ref_eq(source_ref);
        self
    }

    /// Filter results to nodes where `content_ref` is not NULL.
    pub fn filter_content_ref_not_null(mut self) -> Self {
        self.inner = self.inner.filter_content_ref_not_null();
        self
    }

    /// Filter results to nodes matching the given `content_ref` URI.
    pub fn filter_content_ref_eq(mut self, content_ref: impl Into<String>) -> Self {
        self.inner = self.inner.filter_content_ref_eq(content_ref);
        self
    }

    /// Filter results where a JSON property at `path` equals the given text value.
    pub fn filter_json_text_eq(
        mut self,
        path: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.inner = self.inner.filter_json_text_eq(path, value);
        self
    }

    /// Filter results where a JSON property at `path` equals the given boolean value.
    pub fn filter_json_bool_eq(mut self, path: impl Into<String>, value: bool) -> Self {
        self.inner = self.inner.filter_json_bool_eq(path, value);
        self
    }

    /// Filter results where a JSON integer at `path` is greater than `value`.
    pub fn filter_json_integer_gt(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_integer_gt(path, value);
        self
    }

    /// Filter results where a JSON integer at `path` is greater than or equal to `value`.
    pub fn filter_json_integer_gte(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_integer_gte(path, value);
        self
    }

    /// Filter results where a JSON integer at `path` is less than `value`.
    pub fn filter_json_integer_lt(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_integer_lt(path, value);
        self
    }

    /// Filter results where a JSON integer at `path` is less than or equal to `value`.
    pub fn filter_json_integer_lte(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_integer_lte(path, value);
        self
    }

    /// Filter results where a JSON timestamp at `path` is after `value`.
    pub fn filter_json_timestamp_gt(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_timestamp_gt(path, value);
        self
    }

    /// Filter results where a JSON timestamp at `path` is at or after `value`.
    pub fn filter_json_timestamp_gte(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_timestamp_gte(path, value);
        self
    }

    /// Filter results where a JSON timestamp at `path` is before `value`.
    pub fn filter_json_timestamp_lt(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_timestamp_lt(path, value);
        self
    }

    /// Filter results where a JSON timestamp at `path` is at or before `value`.
    pub fn filter_json_timestamp_lte(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_timestamp_lte(path, value);
        self
    }

    /// Add an expansion slot that traverses edges per root result.
    pub fn expand(
        mut self,
        slot: impl Into<String>,
        direction: fathomdb_query::TraverseDirection,
        label: impl Into<String>,
        max_depth: usize,
    ) -> Self {
        self.inner = self.inner.expand(slot, direction, label, max_depth);
        self
    }

    /// Set the final row limit.
    pub fn limit(mut self, limit: usize) -> Self {
        self.inner = self.inner.limit(limit);
        self
    }

    /// Borrow the underlying [`QueryBuilder`].
    #[must_use]
    pub fn as_builder(&self) -> &QueryBuilder {
        &self.inner
    }

    /// Consume the tether and return the underlying AST-only builder.
    #[must_use]
    pub fn into_builder(self) -> QueryBuilder {
        self.inner
    }

    /// Consume the tether and return the underlying [`QueryAst`].
    #[must_use]
    pub fn into_ast(self) -> fathomdb_query::QueryAst {
        self.inner.into_ast()
    }

    /// Compile this query to a [`CompiledQuery`]. Mirrors
    /// [`QueryBuilder::compile`].
    ///
    /// # Errors
    /// Returns [`CompileError`] if compilation fails.
    pub fn compile(&self) -> Result<CompiledQuery, CompileError> {
        self.inner.compile()
    }

    /// Compile this query into a grouped plan. Mirrors
    /// [`QueryBuilder::compile_grouped`].
    ///
    /// # Errors
    /// Returns [`CompileError`] if grouped compilation fails.
    pub fn compile_grouped(&self) -> Result<CompiledGroupedQuery, CompileError> {
        self.inner.compile_grouped()
    }

    /// Execute the query and return matching node rows.
    ///
    /// # Errors
    /// Returns [`EngineError`] if compilation or execution fails.
    pub fn execute(&self) -> Result<QueryRows, EngineError> {
        let compiled = self
            .inner
            .compile()
            .map_err(|e| EngineError::InvalidConfig(format!("query compilation failed: {e}")))?;
        self.engine.coordinator().execute_compiled_read(&compiled)
    }
}

/// Tethered text-search builder returned from
/// [`NodeQueryBuilder::text_search`].
///
/// Accumulates filter predicates alongside the text-search step and dispatches
/// `.execute()` through [`fathomdb_engine::ExecutionCoordinator::execute_compiled_search`],
/// returning [`SearchRows`] populated with score, source, snippet, and
/// active-version `written_at` values.
#[must_use]
pub struct TextSearchBuilder<'e> {
    engine: &'e Engine,
    inner: QueryBuilder,
    attribution_requested: bool,
}

impl TextSearchBuilder<'_> {
    /// Request per-hit match attribution.
    ///
    /// When set, the coordinator populates
    /// [`SearchHit::attribution`](fathomdb_query::SearchHit::attribution) on
    /// every hit with the set of property paths (or `"text_content"` for
    /// chunk hits) that contributed to the match. Without this flag (the
    /// default), attribution stays `None` and the Phase 4 position map is not
    /// read at all — it is a pay-as-you-go feature.
    pub fn with_match_attribution(mut self) -> Self {
        self.attribution_requested = true;
        self
    }

    /// Filter results to a single logical ID.
    pub fn filter_logical_id_eq(mut self, logical_id: impl Into<String>) -> Self {
        self.inner = self.inner.filter_logical_id_eq(logical_id);
        self
    }

    /// Filter results to nodes matching the given kind.
    pub fn filter_kind_eq(mut self, kind: impl Into<String>) -> Self {
        self.inner = self.inner.filter_kind_eq(kind);
        self
    }

    /// Filter results to nodes matching the given `source_ref`.
    pub fn filter_source_ref_eq(mut self, source_ref: impl Into<String>) -> Self {
        self.inner = self.inner.filter_source_ref_eq(source_ref);
        self
    }

    /// Filter results to nodes where `content_ref` is not NULL.
    pub fn filter_content_ref_not_null(mut self) -> Self {
        self.inner = self.inner.filter_content_ref_not_null();
        self
    }

    /// Filter results to nodes matching the given `content_ref` URI.
    pub fn filter_content_ref_eq(mut self, content_ref: impl Into<String>) -> Self {
        self.inner = self.inner.filter_content_ref_eq(content_ref);
        self
    }

    /// Filter results where a JSON property at `path` equals the given text value.
    pub fn filter_json_text_eq(
        mut self,
        path: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.inner = self.inner.filter_json_text_eq(path, value);
        self
    }

    /// Filter results where a JSON property at `path` equals the given boolean value.
    pub fn filter_json_bool_eq(mut self, path: impl Into<String>, value: bool) -> Self {
        self.inner = self.inner.filter_json_bool_eq(path, value);
        self
    }

    /// Filter results where a JSON integer at `path` is greater than `value`.
    pub fn filter_json_integer_gt(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_integer_gt(path, value);
        self
    }

    /// Filter results where a JSON integer at `path` is greater than or equal to `value`.
    pub fn filter_json_integer_gte(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_integer_gte(path, value);
        self
    }

    /// Filter results where a JSON integer at `path` is less than `value`.
    pub fn filter_json_integer_lt(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_integer_lt(path, value);
        self
    }

    /// Filter results where a JSON integer at `path` is less than or equal to `value`.
    pub fn filter_json_integer_lte(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_integer_lte(path, value);
        self
    }

    /// Filter results where a JSON timestamp at `path` is after `value`.
    pub fn filter_json_timestamp_gt(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_timestamp_gt(path, value);
        self
    }

    /// Filter results where a JSON timestamp at `path` is at or after `value`.
    pub fn filter_json_timestamp_gte(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_timestamp_gte(path, value);
        self
    }

    /// Filter results where a JSON timestamp at `path` is before `value`.
    pub fn filter_json_timestamp_lt(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_timestamp_lt(path, value);
        self
    }

    /// Filter results where a JSON timestamp at `path` is at or before `value`.
    pub fn filter_json_timestamp_lte(mut self, path: impl Into<String>, value: i64) -> Self {
        self.inner = self.inner.filter_json_timestamp_lte(path, value);
        self
    }

    /// Set the final row limit on the underlying AST.
    ///
    /// Phase 1 note: [`CompiledSearch`](fathomdb_query::CompiledSearch) derives
    /// its effective limit from the `text_search` step, not from this field.
    /// `limit` is still delegated to the inner builder so callers that later
    /// fall back to [`TextSearchBuilder::compile_query`] keep the same shape.
    pub fn limit(mut self, limit: usize) -> Self {
        self.inner = self.inner.limit(limit);
        self
    }

    /// Add a graph traversal step. Applied after the text-search step when
    /// the inner AST is compiled via [`TextSearchBuilder::compile_query`].
    /// The Phase 1 [`TextSearchBuilder::execute`] path ignores traversals.
    pub fn traverse(
        mut self,
        direction: fathomdb_query::TraverseDirection,
        label: impl Into<String>,
        max_depth: usize,
    ) -> Self {
        self.inner = self.inner.traverse(direction, label, max_depth);
        self
    }

    /// Add an expansion slot. Applied when compiling via
    /// [`TextSearchBuilder::compile_grouped_query`]; ignored by
    /// [`TextSearchBuilder::execute`] in Phase 1.
    pub fn expand(
        mut self,
        slot: impl Into<String>,
        direction: fathomdb_query::TraverseDirection,
        label: impl Into<String>,
        max_depth: usize,
    ) -> Self {
        self.inner = self.inner.expand(slot, direction, label, max_depth);
        self
    }

    /// Borrow the underlying [`QueryBuilder`].
    #[must_use]
    pub fn as_builder(&self) -> &QueryBuilder {
        &self.inner
    }

    /// Compile the underlying AST as a flat [`CompiledQuery`]. Provided for
    /// call sites that mix search with traversal steps and still need to run
    /// the flat node-row pipeline.
    ///
    /// # Errors
    /// Returns [`CompileError`] if compilation fails.
    pub fn compile(&self) -> Result<CompiledQuery, CompileError> {
        self.inner.compile()
    }

    /// Compile the underlying AST as a [`CompiledGroupedQuery`].
    ///
    /// # Errors
    /// Returns [`CompileError`] if compilation fails.
    pub fn compile_grouped(&self) -> Result<CompiledGroupedQuery, CompileError> {
        self.inner.compile_grouped()
    }

    /// Consume the tether and return the underlying [`QueryAst`].
    #[must_use]
    pub fn into_ast(self) -> fathomdb_query::QueryAst {
        self.inner.into_ast()
    }

    /// Execute the text search and return matching hits.
    ///
    /// # Errors
    /// Returns [`EngineError`] if compilation or execution fails.
    pub fn execute(&self) -> Result<SearchRows, EngineError> {
        let mut compiled = compile_search(self.inner.ast())
            .map_err(|e| EngineError::InvalidConfig(format!("search compilation failed: {e}")))?;
        compiled.attribution_requested = self.attribution_requested;
        self.engine.coordinator().execute_compiled_search(&compiled)
    }
}

/// Tethered two-shape fallback search builder returned from
/// [`Engine::fallback_search`].
///
/// `fallback_search(strict, Some(relaxed))` is the "advanced caller who
/// wants explicit control over the relaxed shape" surface. The strict and
/// relaxed queries are both caller-provided — neither is passed through
/// [`fathomdb_query::derive_relaxed`] — and the resulting
/// [`SearchRows`] flows through the same retrieval, merge, and dedup
/// machinery as the adaptive [`TextSearchBuilder`] path.
///
/// `fallback_search(strict, None)` is the strict-only "dedup-on-write"
/// form: it runs the strict branch through the same plan shape (with no
/// relaxed sibling) so callers share the same retrieval and result surface
/// as adaptive `text_search()` rather than an ad hoc path.
///
/// Filters mirror [`TextSearchBuilder`]. There is intentionally no `.nodes`
/// or `.traverse` entry point — this helper is narrow. Its only job is to
/// run one or two search shapes through the shared policy.
#[must_use]
pub struct FallbackSearchBuilder<'e> {
    engine: &'e Engine,
    root_kind: Option<String>,
    strict: TextQuery,
    relaxed: Option<TextQuery>,
    limit: usize,
    attribution_requested: bool,
    // Reuse a QueryBuilder as a filter accumulator so the fusion helper
    // partitions exactly the same predicates as TextSearchBuilder.
    filter_builder: QueryBuilder,
}

impl<'e> FallbackSearchBuilder<'e> {
    pub(crate) fn new(
        engine: &'e Engine,
        strict: impl Into<String>,
        relaxed: Option<&str>,
        limit: usize,
    ) -> Self {
        let strict = TextQuery::parse(&strict.into());
        let relaxed = relaxed.map(TextQuery::parse);
        // The filter accumulator's root kind is a placeholder — fallback_search
        // is kind-agnostic until the caller adds `.filter_kind_eq(...)`. We
        // pick an empty string so `partition_search_filters` ignores it (it
        // only inspects Filter steps).
        Self {
            engine,
            root_kind: None,
            strict,
            relaxed,
            limit,
            attribution_requested: false,
            filter_builder: QueryBuilder::nodes(String::new()),
        }
    }

    /// Request per-hit match attribution. Mirrors
    /// [`TextSearchBuilder::with_match_attribution`].
    pub fn with_match_attribution(mut self) -> Self {
        self.attribution_requested = true;
        self
    }

    /// Filter results to a single logical ID.
    pub fn filter_logical_id_eq(mut self, logical_id: impl Into<String>) -> Self {
        self.filter_builder = self.filter_builder.filter_logical_id_eq(logical_id);
        self
    }

    /// Filter results to nodes matching the given kind.
    pub fn filter_kind_eq(mut self, kind: impl Into<String>) -> Self {
        let kind = kind.into();
        self.root_kind = Some(kind.clone());
        self.filter_builder = self.filter_builder.filter_kind_eq(kind);
        self
    }

    /// Filter results to nodes matching the given `source_ref`.
    pub fn filter_source_ref_eq(mut self, source_ref: impl Into<String>) -> Self {
        self.filter_builder = self.filter_builder.filter_source_ref_eq(source_ref);
        self
    }

    /// Filter results to nodes where `content_ref` is not NULL.
    pub fn filter_content_ref_not_null(mut self) -> Self {
        self.filter_builder = self.filter_builder.filter_content_ref_not_null();
        self
    }

    /// Filter results to nodes matching the given `content_ref` URI.
    pub fn filter_content_ref_eq(mut self, content_ref: impl Into<String>) -> Self {
        self.filter_builder = self.filter_builder.filter_content_ref_eq(content_ref);
        self
    }

    /// Filter results where a JSON property at `path` equals the given text value.
    pub fn filter_json_text_eq(
        mut self,
        path: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.filter_builder = self.filter_builder.filter_json_text_eq(path, value);
        self
    }

    /// Filter results where a JSON property at `path` equals the given boolean value.
    pub fn filter_json_bool_eq(mut self, path: impl Into<String>, value: bool) -> Self {
        self.filter_builder = self.filter_builder.filter_json_bool_eq(path, value);
        self
    }

    /// Filter results where a JSON integer at `path` is greater than `value`.
    pub fn filter_json_integer_gt(mut self, path: impl Into<String>, value: i64) -> Self {
        self.filter_builder = self.filter_builder.filter_json_integer_gt(path, value);
        self
    }

    /// Filter results where a JSON integer at `path` is greater than or equal to `value`.
    pub fn filter_json_integer_gte(mut self, path: impl Into<String>, value: i64) -> Self {
        self.filter_builder = self.filter_builder.filter_json_integer_gte(path, value);
        self
    }

    /// Filter results where a JSON integer at `path` is less than `value`.
    pub fn filter_json_integer_lt(mut self, path: impl Into<String>, value: i64) -> Self {
        self.filter_builder = self.filter_builder.filter_json_integer_lt(path, value);
        self
    }

    /// Filter results where a JSON integer at `path` is less than or equal to `value`.
    pub fn filter_json_integer_lte(mut self, path: impl Into<String>, value: i64) -> Self {
        self.filter_builder = self.filter_builder.filter_json_integer_lte(path, value);
        self
    }

    /// Filter results where a JSON timestamp at `path` is after `value`.
    pub fn filter_json_timestamp_gt(mut self, path: impl Into<String>, value: i64) -> Self {
        self.filter_builder = self.filter_builder.filter_json_timestamp_gt(path, value);
        self
    }

    /// Filter results where a JSON timestamp at `path` is at or after `value`.
    pub fn filter_json_timestamp_gte(mut self, path: impl Into<String>, value: i64) -> Self {
        self.filter_builder = self.filter_builder.filter_json_timestamp_gte(path, value);
        self
    }

    /// Filter results where a JSON timestamp at `path` is before `value`.
    pub fn filter_json_timestamp_lt(mut self, path: impl Into<String>, value: i64) -> Self {
        self.filter_builder = self.filter_builder.filter_json_timestamp_lt(path, value);
        self
    }

    /// Filter results where a JSON timestamp at `path` is at or before `value`.
    pub fn filter_json_timestamp_lte(mut self, path: impl Into<String>, value: i64) -> Self {
        self.filter_builder = self.filter_builder.filter_json_timestamp_lte(path, value);
        self
    }

    /// Compile the builder into a [`CompiledSearchPlan`] without executing
    /// it. Useful for tests and introspection.
    ///
    /// # Errors
    /// Returns [`CompileError`] if filter partitioning fails.
    pub fn compile_plan(&self) -> Result<CompiledSearchPlan, CompileError> {
        // The kind root used inside the SQL CTE is carried through to the
        // strict/relaxed branches. When no `.filter_kind_eq(...)` call has
        // set one, we pass an empty string so the branch runs unkind-filtered
        // (the `src.kind = ?` clause matches only rows whose kind equals the
        // empty string, which yields no rows). To run unkind-filtered, we
        // instead need the SQL to skip that predicate entirely — but since
        // the current branch SQL hard-codes `src.kind = ?2/?4`, the helper
        // REQUIRES a kind via `.filter_kind_eq(...)` in this phase. If the
        // caller omitted it, we fall back to the first KindEq predicate in
        // the filter builder (which `.filter_kind_eq()` sets). When neither
        // is present, we return an empty root kind — the caller will get
        // zero rows, matching "unfiltered across all kinds returns nothing
        // because the SQL is kind-bound."
        let root_kind = self.root_kind.clone().unwrap_or_default();
        let mut ast = self.filter_builder.clone().into_ast();
        ast.root_kind = root_kind;
        compile_search_plan_from_queries(
            &ast,
            self.strict.clone(),
            self.relaxed.clone(),
            self.limit,
            self.attribution_requested,
        )
    }

    /// Execute the fallback search and return matching hits.
    ///
    /// # Errors
    /// Returns [`EngineError`] if compilation or execution fails.
    pub fn execute(&self) -> Result<SearchRows, EngineError> {
        let plan = self
            .compile_plan()
            .map_err(|e| EngineError::InvalidConfig(format!("search compilation failed: {e}")))?;
        self.engine
            .coordinator()
            .execute_compiled_search_plan(&plan)
    }
}
