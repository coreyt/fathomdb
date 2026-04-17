use crate::{
    ComparisonOp, CompileError, CompiledGroupedQuery, CompiledQuery, ExpansionSlot, Predicate,
    QueryAst, QueryStep, ScalarValue, TextQuery, TraverseDirection, compile_grouped_query,
    compile_query,
};

/// Errors raised by tethered search builders when a caller opts into a
/// fused filter variant whose preconditions are not satisfied.
///
/// These errors are surfaced at filter-add time (before any SQL runs)
/// so developers who register a property-FTS schema for the kind see the
/// fused method succeed, while callers who forgot to register a schema
/// get a precise, actionable error instead of silent post-filter
/// degradation. See the Memex near-term roadmap item 7 and
/// `.claude/memory/project_fused_json_filters_contract.md` for the full
/// contract.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BuilderValidationError {
    /// The caller invoked a `filter_json_fused_*` method on a tethered
    /// builder that has no registered property-FTS schema for the kind
    /// it targets.
    #[error(
        "kind {kind:?} has no registered property-FTS schema; register one with admin.register_fts_property_schema(..) before using filter_json_fused_* methods, or use the post-filter filter_json_* family for non-fused semantics"
    )]
    MissingPropertyFtsSchema {
        /// Node kind the builder was targeting.
        kind: String,
    },
    /// The caller invoked a `filter_json_fused_*` method with a path
    /// that is not covered by the registered property-FTS schema for the
    /// kind.
    #[error(
        "kind {kind:?} has a registered property-FTS schema but path {path:?} is not in its include list; add the path to the schema or use the post-filter filter_json_* family"
    )]
    PathNotIndexed {
        /// Node kind the builder was targeting.
        kind: String,
        /// Path the caller attempted to filter on.
        path: String,
    },
    /// The caller invoked a `filter_json_fused_*` method on a tethered
    /// builder that has not been bound to a specific kind (for example,
    /// `FallbackSearchBuilder` without a preceding `filter_kind_eq`).
    /// The fusion gate cannot resolve a schema without a kind.
    #[error(
        "filter_json_fused_* methods require a specific kind; call filter_kind_eq(..) before {method:?} or switch to the post-filter filter_json_* family"
    )]
    KindRequiredForFusion {
        /// Name of the fused filter method that was called.
        method: String,
    },
}

/// Fluent builder for constructing a [`QueryAst`].
///
/// Start with [`QueryBuilder::nodes`] and chain filtering, traversal, and
/// expansion steps before calling [`compile`](QueryBuilder::compile) or
/// [`compile_grouped`](QueryBuilder::compile_grouped).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryBuilder {
    ast: QueryAst,
}

impl QueryBuilder {
    /// Create a builder that queries nodes of the given kind.
    #[must_use]
    pub fn nodes(kind: impl Into<String>) -> Self {
        Self {
            ast: QueryAst {
                root_kind: kind.into(),
                steps: Vec::new(),
                expansions: Vec::new(),
                final_limit: None,
            },
        }
    }

    /// Add a vector similarity search step.
    #[must_use]
    pub fn vector_search(mut self, query: impl Into<String>, limit: usize) -> Self {
        self.ast.steps.push(QueryStep::VectorSearch {
            query: query.into(),
            limit,
        });
        self
    }

    /// Add a full-text search step.
    ///
    /// The input is parsed into `FathomDB`'s safe supported subset: literal
    /// terms, quoted phrases, uppercase `OR`, uppercase `NOT`, and implicit
    /// `AND` by adjacency. Unsupported syntax remains literal rather than being
    /// passed through as raw FTS5 control syntax.
    #[must_use]
    pub fn text_search(mut self, query: impl Into<String>, limit: usize) -> Self {
        let query = TextQuery::parse(&query.into());
        self.ast.steps.push(QueryStep::TextSearch { query, limit });
        self
    }

    /// Add a graph traversal step following edges of the given label.
    #[must_use]
    pub fn traverse(
        mut self,
        direction: TraverseDirection,
        label: impl Into<String>,
        max_depth: usize,
    ) -> Self {
        self.ast.steps.push(QueryStep::Traverse {
            direction,
            label: label.into(),
            max_depth,
            filter: None,
        });
        self
    }

    /// Filter results to a single logical ID.
    #[must_use]
    pub fn filter_logical_id_eq(mut self, logical_id: impl Into<String>) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::LogicalIdEq(logical_id.into())));
        self
    }

    /// Filter results to nodes matching the given kind.
    #[must_use]
    pub fn filter_kind_eq(mut self, kind: impl Into<String>) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::KindEq(kind.into())));
        self
    }

    /// Filter results to nodes matching the given `source_ref`.
    #[must_use]
    pub fn filter_source_ref_eq(mut self, source_ref: impl Into<String>) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::SourceRefEq(source_ref.into())));
        self
    }

    /// Filter results to nodes where `content_ref` is not NULL.
    #[must_use]
    pub fn filter_content_ref_not_null(mut self) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::ContentRefNotNull));
        self
    }

    /// Filter results to nodes matching the given `content_ref` URI.
    #[must_use]
    pub fn filter_content_ref_eq(mut self, content_ref: impl Into<String>) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::ContentRefEq(
                content_ref.into(),
            )));
        self
    }

    /// Filter results where a JSON property at `path` equals the given text value.
    #[must_use]
    pub fn filter_json_text_eq(
        mut self,
        path: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::JsonPathEq {
                path: path.into(),
                value: ScalarValue::Text(value.into()),
            }));
        self
    }

    /// Filter results where a JSON property at `path` equals the given boolean value.
    #[must_use]
    pub fn filter_json_bool_eq(mut self, path: impl Into<String>, value: bool) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::JsonPathEq {
                path: path.into(),
                value: ScalarValue::Bool(value),
            }));
        self
    }

    /// Filter results where a JSON integer at `path` is greater than `value`.
    #[must_use]
    pub fn filter_json_integer_gt(mut self, path: impl Into<String>, value: i64) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::JsonPathCompare {
                path: path.into(),
                op: ComparisonOp::Gt,
                value: ScalarValue::Integer(value),
            }));
        self
    }

    /// Filter results where a JSON integer at `path` is greater than or equal to `value`.
    #[must_use]
    pub fn filter_json_integer_gte(mut self, path: impl Into<String>, value: i64) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::JsonPathCompare {
                path: path.into(),
                op: ComparisonOp::Gte,
                value: ScalarValue::Integer(value),
            }));
        self
    }

    /// Filter results where a JSON integer at `path` is less than `value`.
    #[must_use]
    pub fn filter_json_integer_lt(mut self, path: impl Into<String>, value: i64) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::JsonPathCompare {
                path: path.into(),
                op: ComparisonOp::Lt,
                value: ScalarValue::Integer(value),
            }));
        self
    }

    /// Filter results where a JSON integer at `path` is less than or equal to `value`.
    #[must_use]
    pub fn filter_json_integer_lte(mut self, path: impl Into<String>, value: i64) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::JsonPathCompare {
                path: path.into(),
                op: ComparisonOp::Lte,
                value: ScalarValue::Integer(value),
            }));
        self
    }

    /// Filter results where a JSON timestamp at `path` is after `value` (epoch seconds).
    #[must_use]
    pub fn filter_json_timestamp_gt(self, path: impl Into<String>, value: i64) -> Self {
        self.filter_json_integer_gt(path, value)
    }

    /// Filter results where a JSON timestamp at `path` is at or after `value`.
    #[must_use]
    pub fn filter_json_timestamp_gte(self, path: impl Into<String>, value: i64) -> Self {
        self.filter_json_integer_gte(path, value)
    }

    /// Filter results where a JSON timestamp at `path` is before `value`.
    #[must_use]
    pub fn filter_json_timestamp_lt(self, path: impl Into<String>, value: i64) -> Self {
        self.filter_json_integer_lt(path, value)
    }

    /// Filter results where a JSON timestamp at `path` is at or before `value`.
    #[must_use]
    pub fn filter_json_timestamp_lte(self, path: impl Into<String>, value: i64) -> Self {
        self.filter_json_integer_lte(path, value)
    }

    /// Append a fused JSON text-equality predicate without validating
    /// whether the caller has a property-FTS schema for the kind.
    ///
    /// Callers must have already validated the fusion gate; the
    /// tethered [`crate::QueryBuilder`] has no engine handle to consult
    /// a schema. Mis-use — calling this without prior schema
    /// validation — produces SQL that pushes a `json_extract` predicate
    /// into the search CTE's inner WHERE clause. That is valid SQL but
    /// defeats the "developer opt-in" contract.
    #[must_use]
    pub fn filter_json_fused_text_eq_unchecked(
        mut self,
        path: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::JsonPathFusedEq {
                path: path.into(),
                value: value.into(),
            }));
        self
    }

    /// Append a fused JSON timestamp-greater-than predicate without
    /// validating the fusion gate. See
    /// [`Self::filter_json_fused_text_eq_unchecked`] for the contract.
    #[must_use]
    pub fn filter_json_fused_timestamp_gt_unchecked(
        mut self,
        path: impl Into<String>,
        value: i64,
    ) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::JsonPathFusedTimestampCmp {
                path: path.into(),
                op: ComparisonOp::Gt,
                value,
            }));
        self
    }

    /// Append a fused JSON timestamp-greater-or-equal predicate without
    /// validating the fusion gate. See
    /// [`Self::filter_json_fused_text_eq_unchecked`] for the contract.
    #[must_use]
    pub fn filter_json_fused_timestamp_gte_unchecked(
        mut self,
        path: impl Into<String>,
        value: i64,
    ) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::JsonPathFusedTimestampCmp {
                path: path.into(),
                op: ComparisonOp::Gte,
                value,
            }));
        self
    }

    /// Append a fused JSON timestamp-less-than predicate without
    /// validating the fusion gate. See
    /// [`Self::filter_json_fused_text_eq_unchecked`] for the contract.
    #[must_use]
    pub fn filter_json_fused_timestamp_lt_unchecked(
        mut self,
        path: impl Into<String>,
        value: i64,
    ) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::JsonPathFusedTimestampCmp {
                path: path.into(),
                op: ComparisonOp::Lt,
                value,
            }));
        self
    }

    /// Append a fused JSON timestamp-less-or-equal predicate without
    /// validating the fusion gate. See
    /// [`Self::filter_json_fused_text_eq_unchecked`] for the contract.
    #[must_use]
    pub fn filter_json_fused_timestamp_lte_unchecked(
        mut self,
        path: impl Into<String>,
        value: i64,
    ) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::JsonPathFusedTimestampCmp {
                path: path.into(),
                op: ComparisonOp::Lte,
                value,
            }));
        self
    }

    /// Append a fused JSON boolean-equality predicate without validating
    /// the fusion gate. See
    /// [`Self::filter_json_fused_text_eq_unchecked`] for the contract.
    #[must_use]
    pub fn filter_json_fused_bool_eq_unchecked(
        mut self,
        path: impl Into<String>,
        value: bool,
    ) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::JsonPathFusedBoolEq {
                path: path.into(),
                value,
            }));
        self
    }

    /// Add an expansion slot that traverses edges of the given label for each root result.
    ///
    /// Pass `filter: None` to preserve the existing behavior. `filter: Some(_)` is
    /// accepted by the AST but the compilation is not yet implemented (Pack 3).
    /// Pass `edge_filter: None` to preserve pre-Pack-D behavior (no edge filtering).
    /// `edge_filter: Some(EdgePropertyEq { .. })` filters traversed edges by their
    /// JSON properties; only edges matching the predicate are followed.
    #[must_use]
    pub fn expand(
        mut self,
        slot: impl Into<String>,
        direction: TraverseDirection,
        label: impl Into<String>,
        max_depth: usize,
        filter: Option<Predicate>,
        edge_filter: Option<Predicate>,
    ) -> Self {
        self.ast.expansions.push(ExpansionSlot {
            slot: slot.into(),
            direction,
            label: label.into(),
            max_depth,
            filter,
            edge_filter,
        });
        self
    }

    /// Set the maximum number of result rows.
    #[must_use]
    pub fn limit(mut self, limit: usize) -> Self {
        self.ast.final_limit = Some(limit);
        self
    }

    /// Borrow the underlying [`QueryAst`].
    #[must_use]
    pub fn ast(&self) -> &QueryAst {
        &self.ast
    }

    /// Consume the builder and return the underlying [`QueryAst`].
    #[must_use]
    pub fn into_ast(self) -> QueryAst {
        self.ast
    }

    /// Compile this builder's AST into an executable [`CompiledQuery`].
    ///
    /// # Errors
    ///
    /// Returns [`CompileError`] if the query violates structural constraints
    /// (e.g. too many traversal steps or too many bind parameters).
    pub fn compile(&self) -> Result<CompiledQuery, CompileError> {
        compile_query(&self.ast)
    }

    /// Compile this builder's AST into an executable grouped query.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError`] if the query violates grouped-query structural
    /// constraints such as duplicate slot names or excessive depth.
    pub fn compile_grouped(&self) -> Result<CompiledGroupedQuery, CompileError> {
        compile_grouped_query(&self.ast)
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use crate::{Predicate, QueryBuilder, QueryStep, ScalarValue, TextQuery, TraverseDirection};

    #[test]
    fn builder_accumulates_expected_steps() {
        let query = QueryBuilder::nodes("Meeting")
            .text_search("budget", 5)
            .traverse(TraverseDirection::Out, "HAS_TASK", 2)
            .filter_json_text_eq("$.status", "active")
            .limit(10);

        assert_eq!(query.ast().steps.len(), 3);
        assert_eq!(query.ast().final_limit, Some(10));
    }

    #[test]
    fn builder_filter_json_bool_eq_produces_correct_predicate() {
        let query = QueryBuilder::nodes("Feature").filter_json_bool_eq("$.enabled", true);

        assert_eq!(query.ast().steps.len(), 1);
        match &query.ast().steps[0] {
            QueryStep::Filter(Predicate::JsonPathEq { path, value }) => {
                assert_eq!(path, "$.enabled");
                assert_eq!(*value, ScalarValue::Bool(true));
            }
            other => panic!("expected JsonPathEq/Bool, got {other:?}"),
        }
    }

    #[test]
    fn builder_text_search_parses_into_typed_query() {
        let query = QueryBuilder::nodes("Meeting").text_search("ship NOT blocked", 10);

        match &query.ast().steps[0] {
            QueryStep::TextSearch { query, limit } => {
                assert_eq!(*limit, 10);
                assert_eq!(
                    *query,
                    TextQuery::And(vec![
                        TextQuery::Term("ship".into()),
                        TextQuery::Not(Box::new(TextQuery::Term("blocked".into()))),
                    ])
                );
            }
            other => panic!("expected TextSearch, got {other:?}"),
        }
    }
}
