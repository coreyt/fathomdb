use crate::{
    ComparisonOp, CompileError, CompiledGroupedQuery, CompiledQuery, ExpansionSlot, Predicate,
    QueryAst, QueryStep, ScalarValue, TraverseDirection, compile_grouped_query, compile_query,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryBuilder {
    ast: QueryAst,
}

impl QueryBuilder {
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

    #[must_use]
    pub fn vector_search(mut self, query: impl Into<String>, limit: usize) -> Self {
        self.ast.steps.push(QueryStep::VectorSearch {
            query: query.into(),
            limit,
        });
        self
    }

    #[must_use]
    pub fn text_search(mut self, query: impl Into<String>, limit: usize) -> Self {
        self.ast.steps.push(QueryStep::TextSearch {
            query: query.into(),
            limit,
        });
        self
    }

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
        });
        self
    }

    #[must_use]
    pub fn filter_logical_id_eq(mut self, logical_id: impl Into<String>) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::LogicalIdEq(logical_id.into())));
        self
    }

    #[must_use]
    pub fn filter_kind_eq(mut self, kind: impl Into<String>) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::KindEq(kind.into())));
        self
    }

    #[must_use]
    pub fn filter_source_ref_eq(mut self, source_ref: impl Into<String>) -> Self {
        self.ast
            .steps
            .push(QueryStep::Filter(Predicate::SourceRefEq(source_ref.into())));
        self
    }

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

    #[must_use]
    pub fn filter_json_timestamp_gt(self, path: impl Into<String>, value: i64) -> Self {
        self.filter_json_integer_gt(path, value)
    }

    #[must_use]
    pub fn filter_json_timestamp_gte(self, path: impl Into<String>, value: i64) -> Self {
        self.filter_json_integer_gte(path, value)
    }

    #[must_use]
    pub fn filter_json_timestamp_lt(self, path: impl Into<String>, value: i64) -> Self {
        self.filter_json_integer_lt(path, value)
    }

    #[must_use]
    pub fn filter_json_timestamp_lte(self, path: impl Into<String>, value: i64) -> Self {
        self.filter_json_integer_lte(path, value)
    }

    #[must_use]
    pub fn expand(
        mut self,
        slot: impl Into<String>,
        direction: TraverseDirection,
        label: impl Into<String>,
        max_depth: usize,
    ) -> Self {
        self.ast.expansions.push(ExpansionSlot {
            slot: slot.into(),
            direction,
            label: label.into(),
            max_depth,
        });
        self
    }

    #[must_use]
    pub fn limit(mut self, limit: usize) -> Self {
        self.ast.final_limit = Some(limit);
        self
    }

    #[must_use]
    pub fn ast(&self) -> &QueryAst {
        &self.ast
    }

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
mod tests {
    use crate::{QueryBuilder, TraverseDirection};

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
}
