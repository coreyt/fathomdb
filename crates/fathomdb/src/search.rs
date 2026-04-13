//! Tethered query builders for the Phase 1 adaptive search surface.
//!
//! These builders wrap the AST-only [`fathomdb_query::QueryBuilder`] and carry
//! a borrow of the [`Engine`] so that a zero-arg `.execute()` terminal can
//! route to the right coordinator entry point by type. Non-search chains
//! return [`QueryRows`]; `.text_search(...).execute()` returns [`SearchRows`].

use fathomdb_engine::{EngineError, QueryRows};
use fathomdb_query::{
    CompileError, CompiledGroupedQuery, CompiledQuery, CompiledRetrievalPlan, CompiledSearchPlan,
    CompiledVectorSearch, QueryAst, QueryBuilder, QueryStep, SearchRows, TextQuery, compile_search,
    compile_search_plan_from_queries, compile_vector_search,
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

    /// Transition this chain into the unified Phase 12 retrieval builder.
    ///
    /// `search()` is the primary client-facing retrieval entry point per
    /// `dev/design-adaptive-text-search-surface-addendum-1-vec.md` §Public
    /// Surface. Subsequent filters accumulate on the returned
    /// [`SearchBuilder`] and `.execute()` returns [`SearchRows`] populated
    /// from the unified retrieval planner: text strict, optional text
    /// relaxed, and (in a future phase) vector retrieval, fused under the
    /// addendum's block precedence rules.
    ///
    /// **v1 scope**: the planner's vector branch slot is wired
    /// architecturally but never fires through `search()` because read-time
    /// embedding of natural-language queries is deferred. Callers who need
    /// vector retrieval today should use the advanced `vector_search()`
    /// override directly with a caller-provided vector literal.
    pub fn search(self, query: impl Into<String>, limit: usize) -> SearchBuilder<'e> {
        SearchBuilder::new(
            self.engine,
            self.inner.ast().root_kind.clone(),
            query,
            limit,
        )
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

    /// Transition this chain into a vector-search builder. Subsequent
    /// filters accumulate on the vector-search builder and `.execute()`
    /// returns [`SearchRows`] populated with the vector retrieval block.
    ///
    /// Phase 11 (HITL-Q5 closure): this method switches to a type-state
    /// terminal returning [`VectorSearchBuilder`], mirroring
    /// [`NodeQueryBuilder::text_search`]. The old self-returning form is
    /// no longer available on the facade surface; advanced callers that
    /// need the flat `vector_search` AST step alongside other pipeline
    /// steps can still reach it via [`QueryBuilder::vector_search`] on
    /// the untethered builder.
    pub fn vector_search(self, query: impl Into<String>, limit: usize) -> VectorSearchBuilder<'e> {
        VectorSearchBuilder::new(
            self.engine,
            self.inner.ast().root_kind.clone(),
            query,
            limit,
        )
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
        //
        // The accumulator is seeded with a no-op `text_search` step so that
        // `partition_search_filters` treats subsequent `.filter_*` calls as
        // post-search filters (the partitioner only fuses predicates that
        // appear after a TextSearch/VectorSearch step). The dummy step's
        // query text and limit are never executed — `compile_plan` pulls
        // the real strict/relaxed text queries and limit from the
        // `FallbackSearchBuilder` fields directly when it assembles the
        // `CompiledSearchPlan`.
        let filter_builder = QueryBuilder::nodes(String::new()).text_search("", 0);
        Self {
            engine,
            strict,
            relaxed,
            limit,
            attribution_requested: false,
            filter_builder,
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
    ///
    /// P6-P2-4: unlike the adaptive `TextSearchBuilder` path (which pins
    /// `root_kind` from `Engine::query(kind)`), the narrow fallback helper
    /// applies the kind check through the fusable filter list only. The
    /// fusion pass pushes the resulting `KindEq` predicate into the
    /// `search_hits` CTE's WHERE clause, which is sufficient to narrow
    /// the result set and keeps the emitted SQL free of the redundant
    /// `src.kind = ?` / `fp.kind = ?` checks inside the inner UNION arms.
    pub fn filter_kind_eq(mut self, kind: impl Into<String>) -> Self {
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
        // `FallbackSearchBuilder` is kind-agnostic at the UNION level:
        // when `root_kind` is empty, the coordinator's `run_search_branch`
        // omits the `src.kind = ?` / `fp.kind = ?` predicates from the
        // inner UNION arms entirely, so the search runs across all node
        // kinds. Callers that want kind filtering chain
        // `.filter_kind_eq(kind)`, which adds a fusable `KindEq`
        // predicate (P6-P2-4: the fusion pass then pushes the check into
        // the outer `search_hits` CTE's WHERE clause — a single kind
        // check, not three). The narrow fallback helper therefore always
        // uses an empty root kind on this path.
        let mut ast = self.filter_builder.clone().into_ast();
        ast.root_kind = String::new();
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

/// Tethered vector-search builder returned from
/// [`NodeQueryBuilder::vector_search`].
///
/// Accumulates filter predicates alongside a caller-provided vector query
/// and dispatches `.execute()` through
/// [`fathomdb_engine::ExecutionCoordinator::execute_compiled_vector_search`],
/// returning [`SearchRows`] whose hits carry
/// `modality = RetrievalModality::Vector`, `source = SearchHitSource::Vector`,
/// `match_mode = None`, and `vector_distance = Some(raw_distance)`. The
/// higher-is-better `score` field is the negated distance.
///
/// See `dev/design-adaptive-text-search-surface-addendum-1-vec.md` §Public
/// Surface for the full surface contract and degradation semantics.
#[must_use]
pub struct VectorSearchBuilder<'e> {
    engine: &'e Engine,
    root_kind: String,
    query: String,
    limit: usize,
    attribution_requested: bool,
    // Reuse a QueryBuilder as a filter accumulator so the fusion helper
    // partitions exactly the same predicates as TextSearchBuilder.
    filter_builder: QueryBuilder,
}

impl<'e> VectorSearchBuilder<'e> {
    pub(crate) fn new(
        engine: &'e Engine,
        root_kind: impl Into<String>,
        query: impl Into<String>,
        limit: usize,
    ) -> Self {
        let root_kind = root_kind.into();
        // Mirror FallbackSearchBuilder: the filter accumulator is seeded
        // with a no-op `vector_search("", 0)` step so that
        // `partition_search_filters` treats subsequent `.filter_*` calls
        // as post-search predicates. The P2-N2 fix tightened the
        // partitioner to only collect filters AFTER a search-step marker;
        // without this seed, `.filter_kind_eq("Goal")` would land in
        // neither bucket and would be silently dropped. The dummy step's
        // query text and limit are never executed — `compile_plan` pulls
        // the real vector query string and limit from the builder's
        // fields directly when it assembles the `CompiledVectorSearch`.
        let filter_builder = QueryBuilder::nodes(root_kind.clone()).vector_search("", 0);
        Self {
            engine,
            root_kind,
            query: query.into(),
            limit,
            attribution_requested: false,
            filter_builder,
        }
    }

    /// Request per-hit match attribution.
    ///
    /// When set, every returned hit carries
    /// `attribution: Some(HitAttribution { matched_paths: vec![] })` per
    /// addendum 1 §Attribution on vector hits. The empty `matched_paths`
    /// list is intentional — vector matches have no per-field provenance
    /// to attribute, but the `Some(...)` sentinel lets downstream code
    /// distinguish "attribution was requested and produced no paths" from
    /// "attribution was not requested at all".
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

    /// Compile the builder into a [`CompiledVectorSearch`] without executing
    /// it. Useful for tests and introspection.
    ///
    /// # Errors
    /// Returns [`CompileError`] if filter partitioning fails.
    pub fn compile_plan(&self) -> Result<CompiledVectorSearch, CompileError> {
        let mut ast = self.filter_builder.clone().into_ast();
        ast.root_kind.clone_from(&self.root_kind);
        let mut compiled = compile_vector_search(&ast)?;
        // The seed `.vector_search("", 0)` step on the filter accumulator
        // is an artifact of the partition workaround; `compile_plan` pulls
        // the caller's real query text and limit from `self` directly.
        compiled.query_text.clone_from(&self.query);
        compiled.limit = self.limit;
        compiled.attribution_requested = self.attribution_requested;
        Ok(compiled)
    }

    /// Execute the vector search and return matching hits.
    ///
    /// # Errors
    /// Returns [`EngineError`] if compilation or execution fails. A
    /// capability miss (sqlite-vec unavailable) is NOT an error: it
    /// returns an empty [`SearchRows`] with `was_degraded = true`.
    pub fn execute(&self) -> Result<SearchRows, EngineError> {
        let plan = self
            .compile_plan()
            .map_err(|e| EngineError::InvalidConfig(format!("search compilation failed: {e}")))?;
        self.engine
            .coordinator()
            .execute_compiled_vector_search(&plan)
    }
}

/// Tethered unified retrieval builder returned from
/// [`NodeQueryBuilder::search`].
///
/// `SearchBuilder` is the Phase 12 primary retrieval entry point per
/// `dev/design-adaptive-text-search-surface-addendum-1-vec.md` §Public
/// Surface. It accumulates filter predicates alongside a caller-provided
/// raw query string, compiles into a [`CompiledRetrievalPlan`] via
/// [`fathomdb_query::compile_retrieval_plan`], and dispatches `.execute()`
/// through [`fathomdb_engine::ExecutionCoordinator::execute_retrieval_plan`]
/// to return [`SearchRows`] with the strict/relaxed/vector blocks fused
/// under the addendum's block precedence rules.
///
/// **v1 scope**: the unified planner's vector branch slot is wired
/// architecturally but never fires through `search()` because read-time
/// embedding of natural-language queries is deferred. Until that future
/// phase lands, every `SearchBuilder::execute()` result has
/// `vector_hit_count == 0` regardless of vector capability availability.
/// Callers who want vector retrieval today must use the advanced
/// `vector_search()` override directly with a caller-provided vector
/// literal.
#[must_use]
pub struct SearchBuilder<'e> {
    engine: &'e Engine,
    root_kind: String,
    query: String,
    limit: usize,
    attribution_requested: bool,
    // Reuse a QueryBuilder as a filter accumulator so the fusion helper
    // partitions exactly the same predicates as TextSearchBuilder /
    // FallbackSearchBuilder / VectorSearchBuilder. The accumulator is
    // seeded with a no-op `text_search("", 0)` step so `partition_search_filters`
    // treats subsequent `.filter_*` calls as post-search predicates;
    // without that seed the predicates would land in neither bucket and
    // would be silently dropped. The dummy step's query text and limit
    // are never executed — `compile_plan` rewrites the AST with the real
    // `Search { query, limit }` step before calling
    // `compile_retrieval_plan`.
    filter_builder: QueryBuilder,
}

impl<'e> SearchBuilder<'e> {
    pub(crate) fn new(
        engine: &'e Engine,
        root_kind: impl Into<String>,
        query: impl Into<String>,
        limit: usize,
    ) -> Self {
        let root_kind = root_kind.into();
        let filter_builder = QueryBuilder::nodes(root_kind.clone()).text_search("", 0);
        Self {
            engine,
            root_kind,
            query: query.into(),
            limit,
            attribution_requested: false,
            filter_builder,
        }
    }

    /// Request per-hit match attribution on the resulting [`SearchRows`].
    /// Mirrors [`TextSearchBuilder::with_match_attribution`] semantics for
    /// text hits and [`VectorSearchBuilder::with_match_attribution`] for
    /// vector hits.
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

    /// Compile the builder into a [`CompiledRetrievalPlan`] without executing
    /// it. Useful for tests and introspection.
    ///
    /// # Errors
    /// Returns [`CompileError`] if filter partitioning or text-query parsing
    /// fails.
    pub fn compile_plan(&self) -> Result<CompiledRetrievalPlan, CompileError> {
        // Take the filter accumulator AST and rewrite the seed
        // `text_search("", 0)` step into the real `Search { query, limit }`
        // step. Rewriting in place (rather than appending) preserves the
        // post-search position so that the filter partitioner classifies
        // every chained `.filter_*` predicate the same way the text/vector
        // builders do.
        let mut ast: QueryAst = self.filter_builder.clone().into_ast();
        ast.root_kind.clone_from(&self.root_kind);
        let mut replaced = false;
        for step in &mut ast.steps {
            if let QueryStep::TextSearch {
                query: TextQuery::Empty,
                limit: 0,
            } = step
            {
                *step = QueryStep::Search {
                    query: self.query.clone(),
                    limit: self.limit,
                };
                replaced = true;
                break;
            }
        }
        debug_assert!(
            replaced,
            "SearchBuilder filter accumulator must contain the seed TextSearch step"
        );
        let mut plan = fathomdb_query::compile_retrieval_plan(&ast)?;
        plan.text.strict.attribution_requested = self.attribution_requested;
        if let Some(relaxed) = plan.text.relaxed.as_mut() {
            relaxed.attribution_requested = self.attribution_requested;
        }
        Ok(plan)
    }

    /// Execute the unified retrieval plan and return matching hits.
    ///
    /// # Errors
    /// Returns [`EngineError`] if compilation or execution fails.
    pub fn execute(&self) -> Result<SearchRows, EngineError> {
        let plan = self
            .compile_plan()
            .map_err(|e| EngineError::InvalidConfig(format!("search compilation failed: {e}")))?;
        self.engine.coordinator().execute_retrieval_plan(&plan)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::FallbackSearchBuilder;
    use crate::{Engine, EngineOptions};
    use fathomdb_query::Predicate;
    use tempfile::NamedTempFile;

    /// P7.5-N1: pin the dummy-step workaround invariant.
    ///
    /// `FallbackSearchBuilder` seeds an inert `text_search("", 0)` step
    /// into its internal filter accumulator so that
    /// `partition_search_filters` treats subsequent `.filter_*` calls as
    /// post-search predicates (the partitioner only classifies filters
    /// that appear AFTER a `TextSearch` or `VectorSearch` step in source
    /// order). Without that seed, `.filter_kind_eq("Goal")` would land in
    /// neither the fusable nor the residual bucket and would be silently
    /// dropped. This test compiles a plan via the public builder API and
    /// verifies the kind predicate ends up in `fusable_filters`.
    #[test]
    fn fallback_builder_filter_kind_eq_fuses_without_explicit_text_search_step() {
        let db = NamedTempFile::new().expect("temporary db");
        let engine =
            Engine::open(EngineOptions::new(db.path())).expect("engine opens for unit test");

        let builder = FallbackSearchBuilder::new(&engine, "budget", Some("budget OR nothing"), 10)
            .filter_kind_eq("Goal");
        let plan = builder.compile_plan().expect("compile plan");

        assert!(
            plan.strict
                .fusable_filters
                .iter()
                .any(|p| matches!(p, Predicate::KindEq(k) if k == "Goal")),
            "KindEq(\"Goal\") must land in strict.fusable_filters (got {:?})",
            plan.strict.fusable_filters
        );
        assert!(
            plan.strict.residual_filters.is_empty(),
            "strict.residual_filters should be empty for a single kind filter (got {:?})",
            plan.strict.residual_filters
        );

        let relaxed = plan
            .relaxed
            .as_ref()
            .expect("relaxed branch present when caller supplied a relaxed query");
        assert!(
            relaxed
                .fusable_filters
                .iter()
                .any(|p| matches!(p, Predicate::KindEq(k) if k == "Goal")),
            "KindEq(\"Goal\") must also land in relaxed.fusable_filters (got {:?})",
            relaxed.fusable_filters
        );
    }
}
