use std::fmt::Write;

use crate::plan::{choose_driving_table, execution_hints, shape_signature};
use crate::{
    ComparisonOp, DrivingTable, ExpansionSlot, Predicate, QueryAst, QueryStep, ScalarValue,
    TraverseDirection,
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
}

/// Sanitize a user-supplied text search query for safe use as an FTS5 MATCH
/// expression. Splits on whitespace, wraps each token in double quotes (doubling
/// any embedded `"` per FTS5 escaping rules), and joins with spaces. This
/// produces an implicit AND of quoted terms that is safe against FTS5 syntax
/// injection (operators like AND/OR/NOT/NEAR, column filters, parentheses, and
/// wildcards are all neutralized inside quoted strings).
///
/// Empty or whitespace-only input returns an empty string, which the caller
/// should handle (FTS5 MATCH with an empty string returns no rows).
fn sanitize_fts5_query(raw: &str) -> String {
    let tokens: Vec<String> = raw
        .split_whitespace()
        .map(|token| {
            let escaped = token.replace('"', "\"\"");
            format!("\"{escaped}\"")
        })
        .collect();
    tokens.join(" ")
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

const MAX_BIND_PARAMETERS: usize = 15;
const MAX_EXPANSION_SLOTS: usize = 8;

// FIX(review): max_depth was unbounded — usize::MAX produces an effectively infinite CTE.
// Options: (A) silent clamp at compile, (B) reject with CompileError, (C) validate in builder.
// Chose (B): consistent with existing TooManyTraversals/TooManyBindParameters pattern.
// The compiler is the validation boundary; silent clamping would surprise callers.
const MAX_TRAVERSAL_DEPTH: usize = 50;

/// Compile a [`QueryAst`] into a [`CompiledQuery`] ready for execution.
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
            format!(
                "base_candidates AS (
                    SELECT DISTINCT src.logical_id
                    FROM (
                        SELECT chunk_id FROM vec_nodes_active
                        WHERE embedding MATCH ?1
                        LIMIT {base_limit}
                    ) vc
                    JOIN chunks c ON c.id = vc.chunk_id
                    JOIN nodes src ON src.logical_id = c.node_logical_id AND src.superseded_at IS NULL
                    WHERE src.kind = ?2
                )"
            )
        }
        DrivingTable::FtsNodes => {
            let raw_query = ast
                .steps
                .iter()
                .find_map(|step| {
                    if let QueryStep::TextSearch { query, .. } = step {
                        Some(query.as_str())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| unreachable!("FtsNodes chosen but no TextSearch step in AST"));
            // Sanitize FTS5 metacharacters to prevent syntax errors and query
            // injection. Each user token is quoted so FTS5 operators (AND, OR,
            // NOT, NEAR, column filters, wildcards) are treated as literals.
            binds.push(BindValue::Text(sanitize_fts5_query(raw_query)));
            binds.push(BindValue::Text(ast.root_kind.clone()));
            format!(
                "base_candidates AS (
                    SELECT DISTINCT src.logical_id
                    FROM fts_nodes f
                    JOIN chunks c ON c.id = f.chunk_id
                    JOIN nodes src ON src.logical_id = c.node_logical_id AND src.superseded_at IS NULL
                    WHERE fts_nodes MATCH ?1
                      AND src.kind = ?2
                    LIMIT {base_limit}
                )"
            )
        }
        DrivingTable::Nodes => {
            binds.push(BindValue::Text(ast.root_kind.clone()));
            let mut sql = "base_candidates AS (
                    SELECT DISTINCT src.logical_id
                    FROM nodes src
                    WHERE src.superseded_at IS NULL
                      AND src.kind = ?1"
                .to_owned();
            if let Some(logical_id) = ast.steps.iter().find_map(|step| {
                if let QueryStep::Filter(Predicate::LogicalIdEq(logical_id)) = step {
                    Some(logical_id.as_str())
                } else {
                    None
                }
            }) {
                binds.push(BindValue::Text(logical_id.to_owned()));
                let bind_index = binds.len();
                let _ = write!(
                    &mut sql,
                    "\n                      AND src.logical_id = ?{bind_index}"
                );
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
SELECT DISTINCT n.row_id, n.logical_id, n.kind, n.properties
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
            match predicate {
                Predicate::LogicalIdEq(logical_id) => {
                    // For the Nodes driving table the logical_id filter was already
                    // applied inside base_candidates; applying it again would push a
                    // duplicate bind value and add a redundant WHERE clause.
                    if driving_table != DrivingTable::Nodes {
                        binds.push(BindValue::Text(logical_id.clone()));
                        let bind_index = binds.len();
                        let _ = write!(&mut sql, "\n  AND n.logical_id = ?{bind_index}");
                    }
                }
                Predicate::KindEq(kind) => {
                    binds.push(BindValue::Text(kind.clone()));
                    let bind_index = binds.len();
                    let _ = write!(&mut sql, "\n  AND n.kind = ?{bind_index}");
                }
                Predicate::JsonPathEq { path, value } => {
                    // Security fix H-1: Validate JSON path as defense-in-depth.
                    validate_json_path(path)?;
                    // FIX(review): path was previously string-interpolated with single-quote
                    // escaping. Options considered: (A) parameterize path as bind variable,
                    // (B) harden escaping, (C) validate path against strict regex. Chose (A):
                    // SQLite json_extract supports parameterized paths since 3.9.0 (our minimum
                    // is 3.41), eliminating the injection surface entirely. validate_json_path
                    // is retained above as defense-in-depth from Security fix H-1.
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
            compiled.sql.contains("json_extract(n.properties, ?"),
            "JSON path must be a bind parameter"
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

    // --- FTS5 sanitization tests ---

    #[test]
    fn sanitize_fts5_plain_tokens() {
        use super::sanitize_fts5_query;
        assert_eq!(
            sanitize_fts5_query("budget meeting"),
            "\"budget\" \"meeting\""
        );
    }

    #[test]
    fn sanitize_fts5_apostrophe() {
        use super::sanitize_fts5_query;
        // The apostrophe that triggered issue #31
        assert_eq!(sanitize_fts5_query("User's name"), "\"User's\" \"name\"");
    }

    #[test]
    fn sanitize_fts5_embedded_double_quotes() {
        use super::sanitize_fts5_query;
        assert_eq!(
            sanitize_fts5_query(r#"say "hello" world"#),
            "\"say\" \"\"\"hello\"\"\" \"world\""
        );
    }

    #[test]
    fn sanitize_fts5_operators_neutralized() {
        use super::sanitize_fts5_query;
        // FTS5 operators should be quoted, not interpreted
        assert_eq!(
            sanitize_fts5_query("cats AND dogs OR fish"),
            "\"cats\" \"AND\" \"dogs\" \"OR\" \"fish\""
        );
    }

    #[test]
    fn sanitize_fts5_special_chars() {
        use super::sanitize_fts5_query;
        // Wildcards, column filters, parentheses, NEAR
        assert_eq!(sanitize_fts5_query("prefix*"), "\"prefix*\"");
        assert_eq!(sanitize_fts5_query("col:value"), "\"col:value\"");
        assert_eq!(sanitize_fts5_query("(a OR b)"), "\"(a\" \"OR\" \"b)\"");
        assert_eq!(sanitize_fts5_query("a NEAR b"), "\"a\" \"NEAR\" \"b\"");
    }

    #[test]
    fn sanitize_fts5_empty_input() {
        use super::sanitize_fts5_query;
        assert_eq!(sanitize_fts5_query(""), "");
        assert_eq!(sanitize_fts5_query("   "), "");
    }

    #[test]
    fn fts5_query_bind_is_sanitized() {
        // Verify the compiled query's bind value is sanitized, not the raw input
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
            "FTS5 query bind should be sanitized; got {:?}",
            compiled.binds
        );
    }
}
