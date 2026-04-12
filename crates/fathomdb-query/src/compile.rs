use std::fmt::Write;

use crate::fusion::partition_search_filters;
use crate::plan::{choose_driving_table, execution_hints, shape_signature};
use crate::search::{CompiledSearch, CompiledSearchPlan};
use crate::{
    ComparisonOp, DrivingTable, ExpansionSlot, Predicate, QueryAst, QueryStep, ScalarValue,
    TextQuery, TraverseDirection, derive_relaxed, render_text_query_fts5,
};

/// A typed bind value for a compiled SQL query parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindValue {
    /// A UTF-8 text parameter.
    Text(String),
    /// A 64-bit signed integer parameter.
    Integer(i64),
    /// A boolean parameter.
    Bool(bool),
}

/// A deterministic hash of a query's structural shape, independent of bind values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShapeHash(pub u64);

/// A fully compiled query ready for execution against `SQLite`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledQuery {
    /// The generated SQL text.
    pub sql: String,
    /// Positional bind parameters for the SQL.
    pub binds: Vec<BindValue>,
    /// Structural shape hash for caching.
    pub shape_hash: ShapeHash,
    /// The driving table chosen by the query planner.
    pub driving_table: DrivingTable,
    /// Execution hints derived from the query shape.
    pub hints: crate::ExecutionHints,
}

/// A compiled grouped query containing a root query and expansion slots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledGroupedQuery {
    /// The root flat query.
    pub root: CompiledQuery,
    /// Expansion slots to evaluate per root result.
    pub expansions: Vec<ExpansionSlot>,
    /// Structural shape hash covering the root query and all expansion slots.
    pub shape_hash: ShapeHash,
    /// Execution hints derived from the grouped query shape.
    pub hints: crate::ExecutionHints,
}

/// Errors that can occur during query compilation.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CompileError {
    #[error("multiple traversal steps are not supported in v1")]
    TooManyTraversals,
    #[error("flat query compilation does not support expansions; use compile_grouped")]
    FlatCompileDoesNotSupportExpansions,
    #[error("duplicate expansion slot name: {0}")]
    DuplicateExpansionSlot(String),
    #[error("expansion slot name must be non-empty")]
    EmptyExpansionSlotName,
    #[error("too many expansion slots: max {MAX_EXPANSION_SLOTS}, got {0}")]
    TooManyExpansionSlots(usize),
    #[error("too many bind parameters: max 15, got {0}")]
    TooManyBindParameters(usize),
    #[error("traversal depth {0} exceeds maximum of {MAX_TRAVERSAL_DEPTH}")]
    TraversalTooDeep(usize),
    #[error("invalid JSON path: must match $(.key)+ pattern, got {0:?}")]
    InvalidJsonPath(String),
    #[error("compile_search requires exactly one TextSearch step in the AST")]
    MissingTextSearchStep,
}

/// Security fix H-1: Validate JSON path against a strict allowlist pattern to
/// prevent SQL injection. Retained as defense-in-depth even though the path is
/// now parameterized (see `FIX(review)` in `compile_query`). Only paths like
/// `$.foo`, `$.foo.bar_baz` are allowed.
fn validate_json_path(path: &str) -> Result<(), CompileError> {
    let valid = path.starts_with('$')
        && path.len() > 1
        && path[1..].split('.').all(|segment| {
            segment.is_empty()
                || segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && !segment.is_empty()
        })
        && path.contains('.');
    if !valid {
        return Err(CompileError::InvalidJsonPath(path.to_owned()));
    }
    Ok(())
}

/// Append a fusable predicate as an `AND` clause referencing `alias`.
///
/// Only the fusable variants (those that can be evaluated against columns on
/// the `nodes` table join inside a search CTE) are supported — callers must
/// pre-partition predicates via
/// [`crate::fusion::partition_search_filters`]. Residual predicates panic via
/// `unreachable!`.
fn append_fusable_clause(
    sql: &mut String,
    binds: &mut Vec<BindValue>,
    alias: &str,
    predicate: &Predicate,
) {
    match predicate {
        Predicate::KindEq(kind) => {
            binds.push(BindValue::Text(kind.clone()));
            let idx = binds.len();
            let _ = write!(sql, "\n                          AND {alias}.kind = ?{idx}");
        }
        Predicate::LogicalIdEq(logical_id) => {
            binds.push(BindValue::Text(logical_id.clone()));
            let idx = binds.len();
            let _ = write!(
                sql,
                "\n                          AND {alias}.logical_id = ?{idx}"
            );
        }
        Predicate::SourceRefEq(source_ref) => {
            binds.push(BindValue::Text(source_ref.clone()));
            let idx = binds.len();
            let _ = write!(
                sql,
                "\n                          AND {alias}.source_ref = ?{idx}"
            );
        }
        Predicate::ContentRefEq(uri) => {
            binds.push(BindValue::Text(uri.clone()));
            let idx = binds.len();
            let _ = write!(
                sql,
                "\n                          AND {alias}.content_ref = ?{idx}"
            );
        }
        Predicate::ContentRefNotNull => {
            let _ = write!(
                sql,
                "\n                          AND {alias}.content_ref IS NOT NULL"
            );
        }
        Predicate::JsonPathEq { .. } | Predicate::JsonPathCompare { .. } => {
            unreachable!("append_fusable_clause received a residual predicate");
        }
    }
}

const MAX_BIND_PARAMETERS: usize = 15;
const MAX_EXPANSION_SLOTS: usize = 8;

// FIX(review): max_depth was unbounded — usize::MAX produces an effectively infinite CTE.
// Options: (A) silent clamp at compile, (B) reject with CompileError, (C) validate in builder.
// Chose (B): consistent with existing TooManyTraversals/TooManyBindParameters pattern.
// The compiler is the validation boundary; silent clamping would surprise callers.
const MAX_TRAVERSAL_DEPTH: usize = 50;

/// Compile a [`QueryAst`] into a [`CompiledQuery`] ready for execution.
///
/// # Compilation strategy
///
/// The compiled SQL is structured as a `WITH RECURSIVE` CTE named
/// `base_candidates` followed by a final `SELECT ... JOIN nodes` projection.
///
/// For the **Nodes** driving table (no FTS/vector search), all filter
/// predicates (`LogicalIdEq`, `JsonPathEq`, `JsonPathCompare`,
/// `SourceRefEq`) are pushed into the `base_candidates` CTE so that the
/// CTE's `LIMIT` applies *after* filtering. Without this pushdown the LIMIT
/// would truncate the candidate set before property filters run, silently
/// excluding nodes whose properties satisfy the filter but whose insertion
/// order falls outside the limit window.
///
/// For **FTS** and **vector** driving tables, filters remain in the outer
/// `WHERE` clause because the CTE is already narrowed by the search itself.
///
/// # Errors
///
/// Returns [`CompileError::TooManyTraversals`] if more than one traversal step
/// is present, or [`CompileError::TooManyBindParameters`] if the resulting SQL
/// would require more than 15 bind parameters.
///
/// # Panics
///
/// Panics (via `unreachable!`) if the AST is internally inconsistent — for
/// example, if `choose_driving_table` selects `VecNodes` but no
/// `VectorSearch` step is present in the AST. This cannot happen through the
/// public [`QueryBuilder`] API.
#[allow(clippy::too_many_lines)]
pub fn compile_query(ast: &QueryAst) -> Result<CompiledQuery, CompileError> {
    if !ast.expansions.is_empty() {
        return Err(CompileError::FlatCompileDoesNotSupportExpansions);
    }

    let traversals = ast
        .steps
        .iter()
        .filter(|step| matches!(step, QueryStep::Traverse { .. }))
        .count();
    if traversals > 1 {
        return Err(CompileError::TooManyTraversals);
    }

    let excessive_depth = ast.steps.iter().find_map(|step| {
        if let QueryStep::Traverse { max_depth, .. } = step
            && *max_depth > MAX_TRAVERSAL_DEPTH
        {
            return Some(*max_depth);
        }
        None
    });
    if let Some(depth) = excessive_depth {
        return Err(CompileError::TraversalTooDeep(depth));
    }

    let driving_table = choose_driving_table(ast);
    let hints = execution_hints(ast);
    let shape_hash = ShapeHash(hash_signature(&shape_signature(ast)));

    let base_limit = ast
        .steps
        .iter()
        .find_map(|step| match step {
            QueryStep::VectorSearch { limit, .. } | QueryStep::TextSearch { limit, .. } => {
                Some(*limit)
            }
            _ => None,
        })
        .or(ast.final_limit)
        .unwrap_or(25);

    let final_limit = ast.final_limit.unwrap_or(base_limit);
    let traversal = ast.steps.iter().find_map(|step| {
        if let QueryStep::Traverse {
            direction,
            label,
            max_depth,
        } = step
        {
            Some((*direction, label.as_str(), *max_depth))
        } else {
            None
        }
    });

    // Partition Filter predicates for the search-driven paths into fusable
    // (injected into the search CTE's WHERE) and residual (left in the outer
    // WHERE) sets. The Nodes path pushes *every* predicate into the CTE
    // directly and ignores this partition.
    let (fusable_filters, _residual_filters) = partition_search_filters(&ast.steps);

    let mut binds = Vec::new();
    let base_candidates = match driving_table {
        DrivingTable::VecNodes => {
            let query = ast
                .steps
                .iter()
                .find_map(|step| {
                    if let QueryStep::VectorSearch { query, .. } = step {
                        Some(query.as_str())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| unreachable!("VecNodes chosen but no VectorSearch step in AST"));
            binds.push(BindValue::Text(query.to_owned()));
            binds.push(BindValue::Text(ast.root_kind.clone()));
            // sqlite-vec requires the LIMIT/k constraint to be visible directly on the
            // vec0 KNN scan. Using a sub-select isolates the vec0 LIMIT so the join
            // with chunks/nodes does not prevent the query planner from recognising it.
            let mut sql = format!(
                "base_candidates AS (
                    SELECT DISTINCT src.logical_id
                    FROM (
                        SELECT chunk_id FROM vec_nodes_active
                        WHERE embedding MATCH ?1
                        LIMIT {base_limit}
                    ) vc
                    JOIN chunks c ON c.id = vc.chunk_id
                    JOIN nodes src ON src.logical_id = c.node_logical_id AND src.superseded_at IS NULL
                    WHERE src.kind = ?2",
            );
            for predicate in &fusable_filters {
                append_fusable_clause(&mut sql, &mut binds, "src", predicate);
            }
            sql.push_str("\n                )");
            sql
        }
        DrivingTable::FtsNodes => {
            let text_query = ast
                .steps
                .iter()
                .find_map(|step| {
                    if let QueryStep::TextSearch { query, .. } = step {
                        Some(query)
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| unreachable!("FtsNodes chosen but no TextSearch step in AST"));
            // Render the typed text-query subset into safe FTS5 syntax. Only
            // supported operators are emitted as control syntax; all literal
            // terms and phrases remain quoted and escaped.
            let rendered = render_text_query_fts5(text_query);
            // Each FTS5 virtual table requires its own MATCH bind parameter;
            // reusing indices across the UNION is not supported by SQLite.
            binds.push(BindValue::Text(rendered.clone()));
            binds.push(BindValue::Text(ast.root_kind.clone()));
            binds.push(BindValue::Text(rendered));
            binds.push(BindValue::Text(ast.root_kind.clone()));
            // Wrap the chunk/property UNION in an outer SELECT that joins
            // `nodes` once so fusable filters (kind/logical_id/source_ref/
            // content_ref) can reference node columns directly, bringing them
            // inside the CTE's LIMIT window.
            let mut sql = String::from(
                "base_candidates AS (
                    SELECT DISTINCT n.logical_id
                    FROM (
                        SELECT src.logical_id
                        FROM fts_nodes f
                        JOIN chunks c ON c.id = f.chunk_id
                        JOIN nodes src ON src.logical_id = c.node_logical_id AND src.superseded_at IS NULL
                        WHERE fts_nodes MATCH ?1
                          AND src.kind = ?2
                        UNION
                        SELECT fp.node_logical_id AS logical_id
                        FROM fts_node_properties fp
                        JOIN nodes src ON src.logical_id = fp.node_logical_id AND src.superseded_at IS NULL
                        WHERE fts_node_properties MATCH ?3
                          AND fp.kind = ?4
                    ) u
                    JOIN nodes n ON n.logical_id = u.logical_id AND n.superseded_at IS NULL
                    WHERE 1 = 1",
            );
            for predicate in &fusable_filters {
                append_fusable_clause(&mut sql, &mut binds, "n", predicate);
            }
            let _ = write!(
                &mut sql,
                "\n                    LIMIT {base_limit}\n                )"
            );
            sql
        }
        DrivingTable::Nodes => {
            binds.push(BindValue::Text(ast.root_kind.clone()));
            let mut sql = "base_candidates AS (
                    SELECT DISTINCT src.logical_id
                    FROM nodes src
                    WHERE src.superseded_at IS NULL
                      AND src.kind = ?1"
                .to_owned();
            // Push filter predicates into base_candidates so the LIMIT applies
            // after filtering, not before. Without this, the CTE may truncate
            // the candidate set before property/source_ref filters run, causing
            // nodes that satisfy the filter to be excluded from results.
            for step in &ast.steps {
                if let QueryStep::Filter(predicate) = step {
                    match predicate {
                        Predicate::LogicalIdEq(logical_id) => {
                            binds.push(BindValue::Text(logical_id.clone()));
                            let bind_index = binds.len();
                            let _ = write!(
                                &mut sql,
                                "\n                      AND src.logical_id = ?{bind_index}"
                            );
                        }
                        Predicate::JsonPathEq { path, value } => {
                            validate_json_path(path)?;
                            binds.push(BindValue::Text(path.clone()));
                            let path_index = binds.len();
                            binds.push(match value {
                                ScalarValue::Text(text) => BindValue::Text(text.clone()),
                                ScalarValue::Integer(integer) => BindValue::Integer(*integer),
                                ScalarValue::Bool(boolean) => BindValue::Bool(*boolean),
                            });
                            let value_index = binds.len();
                            let _ = write!(
                                &mut sql,
                                "\n                      AND json_extract(src.properties, ?{path_index}) = ?{value_index}"
                            );
                        }
                        Predicate::JsonPathCompare { path, op, value } => {
                            validate_json_path(path)?;
                            binds.push(BindValue::Text(path.clone()));
                            let path_index = binds.len();
                            binds.push(match value {
                                ScalarValue::Text(text) => BindValue::Text(text.clone()),
                                ScalarValue::Integer(integer) => BindValue::Integer(*integer),
                                ScalarValue::Bool(boolean) => BindValue::Bool(*boolean),
                            });
                            let value_index = binds.len();
                            let operator = match op {
                                ComparisonOp::Gt => ">",
                                ComparisonOp::Gte => ">=",
                                ComparisonOp::Lt => "<",
                                ComparisonOp::Lte => "<=",
                            };
                            let _ = write!(
                                &mut sql,
                                "\n                      AND json_extract(src.properties, ?{path_index}) {operator} ?{value_index}"
                            );
                        }
                        Predicate::SourceRefEq(source_ref) => {
                            binds.push(BindValue::Text(source_ref.clone()));
                            let bind_index = binds.len();
                            let _ = write!(
                                &mut sql,
                                "\n                      AND src.source_ref = ?{bind_index}"
                            );
                        }
                        Predicate::ContentRefNotNull => {
                            let _ = write!(
                                &mut sql,
                                "\n                      AND src.content_ref IS NOT NULL"
                            );
                        }
                        Predicate::ContentRefEq(uri) => {
                            binds.push(BindValue::Text(uri.clone()));
                            let bind_index = binds.len();
                            let _ = write!(
                                &mut sql,
                                "\n                      AND src.content_ref = ?{bind_index}"
                            );
                        }
                        Predicate::KindEq(_) => {
                            // Already filtered by ast.root_kind above.
                        }
                    }
                }
            }
            let _ = write!(
                &mut sql,
                "\n                    LIMIT {base_limit}\n                )"
            );
            sql
        }
    };

    let mut sql = format!("WITH RECURSIVE\n{base_candidates}");
    let source_alias = if traversal.is_some() { "t" } else { "bc" };

    if let Some((direction, label, max_depth)) = traversal {
        binds.push(BindValue::Text(label.to_owned()));
        let label_index = binds.len();
        let (join_condition, next_logical_id) = match direction {
            TraverseDirection::Out => ("e.source_logical_id = t.logical_id", "e.target_logical_id"),
            TraverseDirection::In => ("e.target_logical_id = t.logical_id", "e.source_logical_id"),
        };

        let _ = write!(
            &mut sql,
            ",
traversed(logical_id, depth, visited) AS (
    SELECT bc.logical_id, 0, printf(',%s,', bc.logical_id)
    FROM base_candidates bc
    UNION ALL
    SELECT {next_logical_id}, t.depth + 1, t.visited || {next_logical_id} || ','
    FROM traversed t
    JOIN edges e ON {join_condition}
        AND e.kind = ?{label_index}
        AND e.superseded_at IS NULL
    WHERE t.depth < {max_depth}
      AND instr(t.visited, printf(',%s,', {next_logical_id})) = 0
    LIMIT {}
)",
            hints.hard_limit
        );
    }

    let _ = write!(
        &mut sql,
        "
SELECT DISTINCT n.row_id, n.logical_id, n.kind, n.properties, n.content_ref
FROM {} {source_alias}
JOIN nodes n ON n.logical_id = {source_alias}.logical_id
    AND n.superseded_at IS NULL
WHERE 1 = 1",
        if traversal.is_some() {
            "traversed"
        } else {
            "base_candidates"
        }
    );

    for step in &ast.steps {
        if let QueryStep::Filter(predicate) = step {
            // For the Nodes driving table, filter predicates were already pushed
            // into base_candidates so the CTE LIMIT applies after filtering.
            // Skip them here to avoid duplicate bind values and redundant clauses.
            if driving_table == DrivingTable::Nodes {
                // KindEq is the only predicate NOT pushed into base_candidates
                // (root_kind is handled separately there).
                if let Predicate::KindEq(kind) = predicate {
                    binds.push(BindValue::Text(kind.clone()));
                    let bind_index = binds.len();
                    let _ = write!(&mut sql, "\n  AND n.kind = ?{bind_index}");
                }
                continue;
            }
            // For the search-driven paths (FtsNodes, VecNodes), fusable filter
            // predicates were injected into base_candidates. Skip them here so
            // bind values are not duplicated and the outer WHERE only contains
            // residual predicates (JSON path filters).
            if crate::fusion::is_fusable(predicate) {
                continue;
            }
            match predicate {
                Predicate::LogicalIdEq(logical_id) => {
                    binds.push(BindValue::Text(logical_id.clone()));
                    let bind_index = binds.len();
                    let _ = write!(&mut sql, "\n  AND n.logical_id = ?{bind_index}");
                }
                Predicate::KindEq(kind) => {
                    binds.push(BindValue::Text(kind.clone()));
                    let bind_index = binds.len();
                    let _ = write!(&mut sql, "\n  AND n.kind = ?{bind_index}");
                }
                Predicate::JsonPathEq { path, value } => {
                    validate_json_path(path)?;
                    binds.push(BindValue::Text(path.clone()));
                    let path_index = binds.len();
                    binds.push(match value {
                        ScalarValue::Text(text) => BindValue::Text(text.clone()),
                        ScalarValue::Integer(integer) => BindValue::Integer(*integer),
                        ScalarValue::Bool(boolean) => BindValue::Bool(*boolean),
                    });
                    let value_index = binds.len();
                    let _ = write!(
                        &mut sql,
                        "\n  AND json_extract(n.properties, ?{path_index}) = ?{value_index}",
                    );
                }
                Predicate::JsonPathCompare { path, op, value } => {
                    validate_json_path(path)?;
                    binds.push(BindValue::Text(path.clone()));
                    let path_index = binds.len();
                    binds.push(match value {
                        ScalarValue::Text(text) => BindValue::Text(text.clone()),
                        ScalarValue::Integer(integer) => BindValue::Integer(*integer),
                        ScalarValue::Bool(boolean) => BindValue::Bool(*boolean),
                    });
                    let value_index = binds.len();
                    let operator = match op {
                        ComparisonOp::Gt => ">",
                        ComparisonOp::Gte => ">=",
                        ComparisonOp::Lt => "<",
                        ComparisonOp::Lte => "<=",
                    };
                    let _ = write!(
                        &mut sql,
                        "\n  AND json_extract(n.properties, ?{path_index}) {operator} ?{value_index}",
                    );
                }
                Predicate::SourceRefEq(source_ref) => {
                    binds.push(BindValue::Text(source_ref.clone()));
                    let bind_index = binds.len();
                    let _ = write!(&mut sql, "\n  AND n.source_ref = ?{bind_index}");
                }
                Predicate::ContentRefNotNull => {
                    let _ = write!(&mut sql, "\n  AND n.content_ref IS NOT NULL");
                }
                Predicate::ContentRefEq(uri) => {
                    binds.push(BindValue::Text(uri.clone()));
                    let bind_index = binds.len();
                    let _ = write!(&mut sql, "\n  AND n.content_ref = ?{bind_index}");
                }
            }
        }
    }

    let _ = write!(&mut sql, "\nLIMIT {final_limit}");

    if binds.len() > MAX_BIND_PARAMETERS {
        return Err(CompileError::TooManyBindParameters(binds.len()));
    }

    Ok(CompiledQuery {
        sql,
        binds,
        shape_hash,
        driving_table,
        hints,
    })
}

/// Compile a [`QueryAst`] into a [`CompiledGroupedQuery`] for grouped execution.
///
/// # Errors
///
/// Returns a [`CompileError`] if the AST exceeds expansion-slot limits,
/// contains empty slot names, or specifies a traversal depth beyond the
/// configured maximum.
pub fn compile_grouped_query(ast: &QueryAst) -> Result<CompiledGroupedQuery, CompileError> {
    if ast.expansions.len() > MAX_EXPANSION_SLOTS {
        return Err(CompileError::TooManyExpansionSlots(ast.expansions.len()));
    }

    let mut seen = std::collections::BTreeSet::new();
    for expansion in &ast.expansions {
        if expansion.slot.trim().is_empty() {
            return Err(CompileError::EmptyExpansionSlotName);
        }
        if expansion.max_depth > MAX_TRAVERSAL_DEPTH {
            return Err(CompileError::TraversalTooDeep(expansion.max_depth));
        }
        if !seen.insert(expansion.slot.clone()) {
            return Err(CompileError::DuplicateExpansionSlot(expansion.slot.clone()));
        }
    }

    let mut root_ast = ast.clone();
    root_ast.expansions.clear();
    let root = compile_query(&root_ast)?;
    let hints = execution_hints(ast);
    let shape_hash = ShapeHash(hash_signature(&shape_signature(ast)));

    Ok(CompiledGroupedQuery {
        root,
        expansions: ast.expansions.clone(),
        shape_hash,
        hints,
    })
}

/// Compile a [`QueryAst`] into a [`CompiledSearch`] describing an adaptive
/// text-search execution.
///
/// Unlike [`compile_query`], this path does not emit SQL directly: the
/// coordinator owns the search SELECT so it can project the richer row shape
/// (score, source, snippet, projection id) that flat queries do not need.
///
/// # Errors
///
/// Returns [`CompileError::MissingTextSearchStep`] if the AST contains no
/// [`QueryStep::TextSearch`] step.
pub fn compile_search(ast: &QueryAst) -> Result<CompiledSearch, CompileError> {
    let mut text_query = None;
    let mut limit = None;
    for step in &ast.steps {
        match step {
            QueryStep::TextSearch {
                query,
                limit: step_limit,
            } => {
                text_query = Some(query.clone());
                limit = Some(*step_limit);
            }
            QueryStep::Filter(_) | QueryStep::VectorSearch { .. } | QueryStep::Traverse { .. } => {
                // Filter steps are partitioned below; Vector/Traverse steps
                // are not composable with text search in the adaptive surface
                // yet.
            }
        }
    }
    let text_query = text_query.ok_or(CompileError::MissingTextSearchStep)?;
    let limit = limit.unwrap_or(25);
    let (fusable_filters, residual_filters) = partition_search_filters(&ast.steps);
    Ok(CompiledSearch {
        root_kind: ast.root_kind.clone(),
        text_query,
        limit,
        fusable_filters,
        residual_filters,
        attribution_requested: false,
    })
}

/// Compile a [`QueryAst`] into a [`CompiledSearchPlan`] whose strict branch
/// is the user's [`TextQuery`] and whose relaxed branch is derived via
/// [`derive_relaxed`].
///
/// Phase 6 uses this as the one-query entry point into the shared
/// strict+relaxed coordinator routine. The two-query `fallback_search` path
/// builds its plan via [`compile_search_plan_from_queries`] instead.
///
/// # Errors
/// Returns [`CompileError::MissingTextSearchStep`] if the AST contains no
/// [`QueryStep::TextSearch`] step.
pub fn compile_search_plan(ast: &QueryAst) -> Result<CompiledSearchPlan, CompileError> {
    let strict = compile_search(ast)?;
    let (relaxed_query, was_degraded_at_plan_time) = derive_relaxed(&strict.text_query);
    let relaxed = relaxed_query.map(|q| CompiledSearch {
        root_kind: strict.root_kind.clone(),
        text_query: q,
        limit: strict.limit,
        fusable_filters: strict.fusable_filters.clone(),
        residual_filters: strict.residual_filters.clone(),
        attribution_requested: strict.attribution_requested,
    });
    Ok(CompiledSearchPlan {
        strict,
        relaxed,
        was_degraded_at_plan_time,
    })
}

/// Compile a caller-provided strict/relaxed [`TextQuery`] pair into a
/// [`CompiledSearchPlan`] against a [`QueryAst`] that supplies the kind
/// root, filters, and limit.
///
/// This is the two-query entry point used by `Engine::fallback_search`. The
/// caller's relaxed [`TextQuery`] is used verbatim — it is NOT passed through
/// [`derive_relaxed`], and the 4-alternative
/// [`crate::RELAXED_BRANCH_CAP`] is NOT applied. As a result
/// [`CompiledSearchPlan::was_degraded_at_plan_time`] is always `false` on
/// this path.
///
/// The AST supplies:
///  - `root_kind` — reused for both branches
///  - filter steps — partitioned once via [`partition_search_filters`] and
///    shared unchanged across both branches
///  - `limit` from the text-search step (or the default used by
///    [`compile_search`]) when present; if the AST has no `TextSearch` step,
///    the caller-supplied `limit` is used
///
/// Any `TextSearch` step already on the AST is IGNORED — `strict` and
/// `relaxed` come from the caller. `Vector`/`Traverse` steps are also
/// ignored for symmetry with [`compile_search`].
///
/// # Errors
/// Returns [`CompileError`] if filter partitioning produces an unsupported
/// shape (currently none; reserved for forward compatibility).
pub fn compile_search_plan_from_queries(
    ast: &QueryAst,
    strict: TextQuery,
    relaxed: Option<TextQuery>,
    limit: usize,
    attribution_requested: bool,
) -> Result<CompiledSearchPlan, CompileError> {
    let (fusable_filters, residual_filters) = partition_search_filters(&ast.steps);
    let strict_compiled = CompiledSearch {
        root_kind: ast.root_kind.clone(),
        text_query: strict,
        limit,
        fusable_filters: fusable_filters.clone(),
        residual_filters: residual_filters.clone(),
        attribution_requested,
    };
    let relaxed_compiled = relaxed.map(|q| CompiledSearch {
        root_kind: ast.root_kind.clone(),
        text_query: q,
        limit,
        fusable_filters,
        residual_filters,
        attribution_requested,
    });
    Ok(CompiledSearchPlan {
        strict: strict_compiled,
        relaxed: relaxed_compiled,
        was_degraded_at_plan_time: false,
    })
}

/// FNV-1a 64-bit hash — deterministic across Rust versions and program
/// invocations, unlike `DefaultHasher`.
fn hash_signature(signature: &str) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for byte in signature.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::items_after_statements)]
mod tests {
    use rstest::rstest;

    use crate::{
        CompileError, DrivingTable, QueryBuilder, TraverseDirection, compile_grouped_query,
        compile_query,
    };

    #[test]
    fn vector_query_compiles_to_chunk_resolution() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .vector_search("budget", 5)
                .limit(5)
                .into_ast(),
        )
        .expect("compiled query");

        assert_eq!(compiled.driving_table, DrivingTable::VecNodes);
        assert!(compiled.sql.contains("JOIN chunks c ON c.id = vc.chunk_id"));
        assert!(
            compiled
                .sql
                .contains("JOIN nodes src ON src.logical_id = c.node_logical_id")
        );
    }

    #[rstest]
    #[case(5, 7)]
    #[case(3, 11)]
    fn structural_limits_change_shape_hash(#[case] left: usize, #[case] right: usize) {
        let left_compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .text_search("budget", left)
                .limit(left)
                .into_ast(),
        )
        .expect("left query");
        let right_compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .text_search("budget", right)
                .limit(right)
                .into_ast(),
        )
        .expect("right query");

        assert_ne!(left_compiled.shape_hash, right_compiled.shape_hash);
    }

    #[test]
    fn traversal_query_is_depth_bounded() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .text_search("budget", 5)
                .traverse(TraverseDirection::Out, "HAS_TASK", 3)
                .limit(10)
                .into_ast(),
        )
        .expect("compiled traversal");

        assert!(compiled.sql.contains("WITH RECURSIVE"));
        assert!(compiled.sql.contains("WHERE t.depth < 3"));
    }

    #[test]
    fn text_search_compiles_to_union_over_chunk_and_property_fts() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .text_search("budget", 25)
                .limit(25)
                .into_ast(),
        )
        .expect("compiled text search");

        assert_eq!(compiled.driving_table, DrivingTable::FtsNodes);
        // Must contain UNION of both FTS tables.
        assert!(
            compiled.sql.contains("fts_nodes MATCH"),
            "must search chunk-backed FTS"
        );
        assert!(
            compiled.sql.contains("fts_node_properties MATCH"),
            "must search property-backed FTS"
        );
        assert!(compiled.sql.contains("UNION"), "must UNION both sources");
        // Must have 4 bind parameters: sanitized query + kind for each table.
        assert_eq!(compiled.binds.len(), 4);
    }

    #[test]
    fn logical_id_filter_is_compiled() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .filter_logical_id_eq("meeting-123")
                .filter_json_text_eq("$.status", "active")
                .limit(1)
                .into_ast(),
        )
        .expect("compiled query");

        // LogicalIdEq is applied in base_candidates (src alias) for the Nodes driver,
        // NOT duplicated in the final WHERE. The JOIN condition still contains
        // "n.logical_id =" which satisfies this check.
        assert!(compiled.sql.contains("n.logical_id ="));
        assert!(compiled.sql.contains("src.logical_id ="));
        assert!(compiled.sql.contains("json_extract"));
        // Only one bind for the logical_id (not two).
        use crate::BindValue;
        assert_eq!(
            compiled
                .binds
                .iter()
                .filter(|b| matches!(b, BindValue::Text(s) if s == "meeting-123"))
                .count(),
            1
        );
    }

    #[test]
    fn compile_rejects_invalid_json_path() {
        use crate::{Predicate, QueryStep, ScalarValue};
        let mut ast = QueryBuilder::nodes("Meeting").into_ast();
        // Attempt SQL injection via JSON path.
        ast.steps.push(QueryStep::Filter(Predicate::JsonPathEq {
            path: "$') OR 1=1 --".to_owned(),
            value: ScalarValue::Text("x".to_owned()),
        }));
        use crate::CompileError;
        let result = compile_query(&ast);
        assert!(
            matches!(result, Err(CompileError::InvalidJsonPath(_))),
            "expected InvalidJsonPath, got {result:?}"
        );
    }

    #[test]
    fn compile_accepts_valid_json_paths() {
        use crate::{Predicate, QueryStep, ScalarValue};
        for valid_path in ["$.status", "$.foo.bar", "$.a_b.c2"] {
            let mut ast = QueryBuilder::nodes("Meeting").into_ast();
            ast.steps.push(QueryStep::Filter(Predicate::JsonPathEq {
                path: valid_path.to_owned(),
                value: ScalarValue::Text("v".to_owned()),
            }));
            assert!(
                compile_query(&ast).is_ok(),
                "expected valid path {valid_path:?} to compile"
            );
        }
    }

    #[test]
    fn compile_rejects_too_many_bind_parameters() {
        use crate::{Predicate, QueryStep, ScalarValue};
        let mut ast = QueryBuilder::nodes("Meeting").into_ast();
        // kind occupies 1 bind; each json filter now occupies 2 binds (path + value).
        // 7 json filters → 1 + 14 = 15 (ok), 8 → 1 + 16 = 17 (exceeds limit of 15).
        for i in 0..8 {
            ast.steps.push(QueryStep::Filter(Predicate::JsonPathEq {
                path: format!("$.f{i}"),
                value: ScalarValue::Text("v".to_owned()),
            }));
        }
        use crate::CompileError;
        let result = compile_query(&ast);
        assert!(
            matches!(result, Err(CompileError::TooManyBindParameters(17))),
            "expected TooManyBindParameters(17), got {result:?}"
        );
    }

    #[test]
    fn compile_rejects_excessive_traversal_depth() {
        let result = compile_query(
            &QueryBuilder::nodes("Meeting")
                .text_search("budget", 5)
                .traverse(TraverseDirection::Out, "HAS_TASK", 51)
                .limit(10)
                .into_ast(),
        );
        assert!(
            matches!(result, Err(CompileError::TraversalTooDeep(51))),
            "expected TraversalTooDeep(51), got {result:?}"
        );
    }

    #[test]
    fn grouped_queries_with_same_structure_share_shape_hash() {
        let left = compile_grouped_query(
            &QueryBuilder::nodes("Meeting")
                .text_search("budget", 5)
                .expand("tasks", TraverseDirection::Out, "HAS_TASK", 1)
                .limit(10)
                .into_ast(),
        )
        .expect("left grouped query");
        let right = compile_grouped_query(
            &QueryBuilder::nodes("Meeting")
                .text_search("planning", 5)
                .expand("tasks", TraverseDirection::Out, "HAS_TASK", 1)
                .limit(10)
                .into_ast(),
        )
        .expect("right grouped query");

        assert_eq!(left.shape_hash, right.shape_hash);
    }

    #[test]
    fn compile_grouped_rejects_duplicate_expansion_slot_names() {
        let result = compile_grouped_query(
            &QueryBuilder::nodes("Meeting")
                .expand("tasks", TraverseDirection::Out, "HAS_TASK", 1)
                .expand("tasks", TraverseDirection::Out, "HAS_DECISION", 1)
                .into_ast(),
        );

        assert!(
            matches!(result, Err(CompileError::DuplicateExpansionSlot(ref slot)) if slot == "tasks"),
            "expected DuplicateExpansionSlot(\"tasks\"), got {result:?}"
        );
    }

    #[test]
    fn flat_compile_rejects_queries_with_expansions() {
        let result = compile_query(
            &QueryBuilder::nodes("Meeting")
                .expand("tasks", TraverseDirection::Out, "HAS_TASK", 1)
                .into_ast(),
        );

        assert!(
            matches!(
                result,
                Err(CompileError::FlatCompileDoesNotSupportExpansions)
            ),
            "expected FlatCompileDoesNotSupportExpansions, got {result:?}"
        );
    }

    #[test]
    fn json_path_compiled_as_bind_parameter() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .filter_json_text_eq("$.status", "active")
                .limit(1)
                .into_ast(),
        )
        .expect("compiled query");

        // Path must be parameterized, not interpolated into the SQL string.
        assert!(
            !compiled.sql.contains("'$.status'"),
            "JSON path must not appear as a SQL string literal"
        );
        assert!(
            compiled.sql.contains("json_extract(src.properties, ?"),
            "JSON path must be a bind parameter (pushed into base_candidates for Nodes driver)"
        );
        // Path and value should both be in the bind list.
        use crate::BindValue;
        assert!(
            compiled
                .binds
                .iter()
                .any(|b| matches!(b, BindValue::Text(s) if s == "$.status"))
        );
        assert!(
            compiled
                .binds
                .iter()
                .any(|b| matches!(b, BindValue::Text(s) if s == "active"))
        );
    }

    // --- Filter pushdown regression tests ---
    //
    // These tests verify that filter predicates are pushed into the
    // base_candidates CTE for the Nodes driving table, so the CTE LIMIT
    // applies after filtering rather than before.  Without pushdown, the
    // LIMIT may truncate the candidate set before the filter runs, causing
    // matching nodes to be silently excluded.

    #[test]
    fn nodes_driver_pushes_json_eq_filter_into_base_candidates() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .filter_json_text_eq("$.status", "active")
                .limit(5)
                .into_ast(),
        )
        .expect("compiled query");

        assert_eq!(compiled.driving_table, DrivingTable::Nodes);
        // Filter must appear inside base_candidates (src alias), not the
        // outer WHERE (n alias).
        assert!(
            compiled.sql.contains("json_extract(src.properties, ?"),
            "json_extract must reference src (base_candidates), got:\n{}",
            compiled.sql,
        );
        assert!(
            !compiled.sql.contains("json_extract(n.properties, ?"),
            "json_extract must NOT appear in outer WHERE for Nodes driver, got:\n{}",
            compiled.sql,
        );
    }

    #[test]
    fn nodes_driver_pushes_json_compare_filter_into_base_candidates() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .filter_json_integer_gte("$.priority", 5)
                .limit(10)
                .into_ast(),
        )
        .expect("compiled query");

        assert_eq!(compiled.driving_table, DrivingTable::Nodes);
        assert!(
            compiled.sql.contains("json_extract(src.properties, ?"),
            "comparison filter must be in base_candidates, got:\n{}",
            compiled.sql,
        );
        assert!(
            !compiled.sql.contains("json_extract(n.properties, ?"),
            "comparison filter must NOT be in outer WHERE for Nodes driver",
        );
        assert!(
            compiled.sql.contains(">= ?"),
            "expected >= operator in SQL, got:\n{}",
            compiled.sql,
        );
    }

    #[test]
    fn nodes_driver_pushes_source_ref_filter_into_base_candidates() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .filter_source_ref_eq("ref-123")
                .limit(5)
                .into_ast(),
        )
        .expect("compiled query");

        assert_eq!(compiled.driving_table, DrivingTable::Nodes);
        assert!(
            compiled.sql.contains("src.source_ref = ?"),
            "source_ref filter must be in base_candidates, got:\n{}",
            compiled.sql,
        );
        assert!(
            !compiled.sql.contains("n.source_ref = ?"),
            "source_ref filter must NOT be in outer WHERE for Nodes driver",
        );
    }

    #[test]
    fn nodes_driver_pushes_multiple_filters_into_base_candidates() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .filter_logical_id_eq("meeting-1")
                .filter_json_text_eq("$.status", "active")
                .filter_json_integer_gte("$.priority", 5)
                .filter_source_ref_eq("ref-abc")
                .limit(1)
                .into_ast(),
        )
        .expect("compiled query");

        assert_eq!(compiled.driving_table, DrivingTable::Nodes);
        // All filters should be in base_candidates, none in outer WHERE
        assert!(
            compiled.sql.contains("src.logical_id = ?"),
            "logical_id filter must be in base_candidates",
        );
        assert!(
            compiled.sql.contains("json_extract(src.properties, ?"),
            "JSON filters must be in base_candidates",
        );
        assert!(
            compiled.sql.contains("src.source_ref = ?"),
            "source_ref filter must be in base_candidates",
        );
        // Each bind value should appear exactly once (not duplicated in outer WHERE)
        use crate::BindValue;
        assert_eq!(
            compiled
                .binds
                .iter()
                .filter(|b| matches!(b, BindValue::Text(s) if s == "meeting-1"))
                .count(),
            1,
            "logical_id bind must not be duplicated"
        );
        assert_eq!(
            compiled
                .binds
                .iter()
                .filter(|b| matches!(b, BindValue::Text(s) if s == "ref-abc"))
                .count(),
            1,
            "source_ref bind must not be duplicated"
        );
    }

    #[test]
    fn fts_driver_keeps_json_filter_residual_but_fuses_kind() {
        // Phase 2: JSON filters are residual (stay in outer WHERE); KindEq is
        // fusable (pushed into base_candidates so the CTE LIMIT applies after
        // filtering).
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .text_search("budget", 5)
                .filter_json_text_eq("$.status", "active")
                .filter_kind_eq("Meeting")
                .limit(5)
                .into_ast(),
        )
        .expect("compiled query");

        assert_eq!(compiled.driving_table, DrivingTable::FtsNodes);
        // Residual: JSON predicate stays in outer WHERE on n.properties.
        assert!(
            compiled.sql.contains("json_extract(n.properties, ?"),
            "JSON filter must stay residual in outer WHERE, got:\n{}",
            compiled.sql,
        );
        // Fusable: the second n.kind bind should live inside base_candidates.
        // The CTE block ends before the final SELECT.
        let (cte, outer) = compiled
            .sql
            .split_once("SELECT DISTINCT n.row_id")
            .expect("query has final SELECT");
        assert!(
            cte.contains("AND n.kind = ?"),
            "KindEq must be fused inside base_candidates CTE, got CTE:\n{cte}"
        );
        // Outer WHERE must not contain a duplicate n.kind filter.
        assert!(
            !outer.contains("AND n.kind = ?"),
            "KindEq must NOT appear in outer WHERE for FTS driver, got outer:\n{outer}"
        );
    }

    #[test]
    fn fts_driver_fuses_kind_filter() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Goal")
                .text_search("budget", 5)
                .filter_kind_eq("Goal")
                .limit(5)
                .into_ast(),
        )
        .expect("compiled query");

        assert_eq!(compiled.driving_table, DrivingTable::FtsNodes);
        let (cte, outer) = compiled
            .sql
            .split_once("SELECT DISTINCT n.row_id")
            .expect("query has final SELECT");
        assert!(
            cte.contains("AND n.kind = ?"),
            "KindEq must be fused inside base_candidates, got:\n{cte}"
        );
        assert!(
            !outer.contains("AND n.kind = ?"),
            "KindEq must NOT be in outer WHERE, got:\n{outer}"
        );
    }

    #[test]
    fn vec_driver_fuses_kind_filter() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Goal")
                .vector_search("budget", 5)
                .filter_kind_eq("Goal")
                .limit(5)
                .into_ast(),
        )
        .expect("compiled query");

        assert_eq!(compiled.driving_table, DrivingTable::VecNodes);
        let (cte, outer) = compiled
            .sql
            .split_once("SELECT DISTINCT n.row_id")
            .expect("query has final SELECT");
        assert!(
            cte.contains("AND src.kind = ?"),
            "KindEq must be fused inside base_candidates, got:\n{cte}"
        );
        assert!(
            !outer.contains("AND n.kind = ?"),
            "KindEq must NOT be in outer WHERE, got:\n{outer}"
        );
    }

    #[test]
    fn fts5_query_bind_uses_rendered_literals() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .text_search("User's name", 5)
                .limit(5)
                .into_ast(),
        )
        .expect("compiled query");

        use crate::BindValue;
        assert!(
            compiled
                .binds
                .iter()
                .any(|b| matches!(b, BindValue::Text(s) if s == "\"User's\" \"name\"")),
            "FTS5 query bind should use rendered literal terms; got {:?}",
            compiled.binds
        );
    }

    #[test]
    fn fts5_query_bind_supports_or_operator() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .text_search("ship OR docs", 5)
                .limit(5)
                .into_ast(),
        )
        .expect("compiled query");

        use crate::BindValue;
        assert!(
            compiled
                .binds
                .iter()
                .any(|b| matches!(b, BindValue::Text(s) if s == "\"ship\" OR \"docs\"")),
            "FTS5 query bind should preserve supported OR; got {:?}",
            compiled.binds
        );
    }

    #[test]
    fn fts5_query_bind_supports_not_operator() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .text_search("ship NOT blocked", 5)
                .limit(5)
                .into_ast(),
        )
        .expect("compiled query");

        use crate::BindValue;
        assert!(
            compiled
                .binds
                .iter()
                .any(|b| matches!(b, BindValue::Text(s) if s == "\"ship\" NOT \"blocked\"")),
            "FTS5 query bind should preserve supported NOT; got {:?}",
            compiled.binds
        );
    }

    #[test]
    fn fts5_query_bind_literalizes_clause_leading_not() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .text_search("NOT blocked", 5)
                .limit(5)
                .into_ast(),
        )
        .expect("compiled query");

        use crate::BindValue;
        assert!(
            compiled
                .binds
                .iter()
                .any(|b| matches!(b, BindValue::Text(s) if s == "\"NOT\" \"blocked\"")),
            "Clause-leading NOT should degrade to literals; got {:?}",
            compiled.binds
        );
    }

    #[test]
    fn fts5_query_bind_literalizes_or_not_sequence() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .text_search("ship OR NOT blocked", 5)
                .limit(5)
                .into_ast(),
        )
        .expect("compiled query");

        use crate::BindValue;
        assert!(
            compiled.binds.iter().any(
                |b| matches!(b, BindValue::Text(s) if s == "\"ship\" \"OR\" \"NOT\" \"blocked\"")
            ),
            "`OR NOT` should degrade to literals rather than emit invalid FTS5; got {:?}",
            compiled.binds
        );
    }

    #[test]
    fn fts5_query_bind_preserves_lowercase_not_as_literal_text() {
        let compiled = compile_query(
            &QueryBuilder::nodes("Meeting")
                .text_search("not a ship", 5)
                .limit(5)
                .into_ast(),
        )
        .expect("compiled query");

        use crate::BindValue;
        assert!(
            compiled
                .binds
                .iter()
                .any(|b| matches!(b, BindValue::Text(s) if s == "\"not\" \"a\" \"ship\"")),
            "Lowercase not should remain a literal term sequence; got {:?}",
            compiled.binds
        );
    }
}
