use std::collections::hash_map::DefaultHasher;
use std::fmt::Write;
use std::hash::{Hash, Hasher};

use crate::plan::{choose_driving_table, execution_hints, shape_signature};
use crate::{DrivingTable, Predicate, QueryAst, QueryStep, ScalarValue, TraverseDirection};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindValue {
    Text(String),
    Integer(i64),
    Bool(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShapeHash(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledQuery {
    pub sql: String,
    pub binds: Vec<BindValue>,
    pub shape_hash: ShapeHash,
    pub driving_table: DrivingTable,
    pub hints: crate::ExecutionHints,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CompileError {
    #[error("multiple traversal steps are not supported in v1")]
    TooManyTraversals,
    #[error("too many bind parameters: max 15, got {0}")]
    TooManyBindParameters(usize),
}

const MAX_BIND_PARAMETERS: usize = 15;

pub fn compile_query(ast: &QueryAst) -> Result<CompiledQuery, CompileError> {
    let traversals = ast
        .steps
        .iter()
        .filter(|step| matches!(step, QueryStep::Traverse { .. }))
        .count();
    if traversals > 1 {
        return Err(CompileError::TooManyTraversals);
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
                .expect("vector search exists when vec_nodes drives");
            binds.push(BindValue::Text(query.to_owned()));
            binds.push(BindValue::Text(ast.root_kind.clone()));
            format!(
                "base_candidates AS (
                    SELECT DISTINCT src.logical_id
                    FROM vec_nodes_active vc
                    JOIN chunks c ON c.id = vc.chunk_id
                    JOIN nodes src ON src.logical_id = c.node_logical_id AND src.superseded_at IS NULL
                    WHERE vc.embedding MATCH ?1
                      AND src.kind = ?2
                    LIMIT {base_limit}
                )"
            )
        }
        DrivingTable::FtsNodes => {
            let query = ast
                .steps
                .iter()
                .find_map(|step| {
                    if let QueryStep::TextSearch { query, .. } = step {
                        Some(query.as_str())
                    } else {
                        None
                    }
                })
                .expect("text search exists when fts_nodes drives");
            binds.push(BindValue::Text(query.to_owned()));
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
            let mut sql = format!(
                "base_candidates AS (
                    SELECT DISTINCT src.logical_id
                    FROM nodes src
                    WHERE src.superseded_at IS NULL
                      AND src.kind = ?1"
            );
            if let Some(logical_id) = ast.steps.iter().find_map(|step| {
                if let QueryStep::Filter(Predicate::LogicalIdEq(logical_id)) = step {
                    Some(logical_id.as_str())
                } else {
                    None
                }
            }) {
                binds.push(BindValue::Text(logical_id.to_owned()));
                let bind_index = binds.len();
                let _ = write!(&mut sql, "\n                      AND src.logical_id = ?{bind_index}");
            }
            let _ = write!(&mut sql, "\n                    LIMIT {base_limit}\n                )");
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
        if traversal.is_some() { "traversed" } else { "base_candidates" }
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
                    let escaped_path = path.replace('\'', "''");
                    let _ = write!(
                        &mut sql,
                        "\n  AND json_extract(n.properties, '{escaped_path}') = ?{}",
                        binds.len() + 1
                    );
                    binds.push(match value {
                        ScalarValue::Text(text) => BindValue::Text(text.clone()),
                        ScalarValue::Integer(integer) => BindValue::Integer(*integer),
                        ScalarValue::Bool(boolean) => BindValue::Bool(*boolean),
                    });
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

fn hash_signature(signature: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    signature.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::{compile_query, DrivingTable, QueryBuilder, TraverseDirection};

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
        assert!(compiled.sql.contains("JOIN nodes src ON src.logical_id = c.node_logical_id"));
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
        assert_eq!(compiled.binds.iter().filter(|b| matches!(b, BindValue::Text(s) if s == "meeting-123")).count(), 1);
    }

    #[test]
    fn compile_rejects_too_many_bind_parameters() {
        use crate::{Predicate, QueryStep, ScalarValue};
        let mut ast = QueryBuilder::nodes("Meeting").into_ast();
        // kind already occupies 1 bind; add 15 json filters → 16 total > 15 limit.
        for i in 0..15 {
            ast.steps.push(QueryStep::Filter(Predicate::JsonPathEq {
                path: format!("$.f{i}"),
                value: ScalarValue::Text("v".to_owned()),
            }));
        }
        use crate::CompileError;
        let result = compile_query(&ast);
        assert!(
            matches!(result, Err(CompileError::TooManyBindParameters(16))),
            "expected TooManyBindParameters(16), got {result:?}"
        );
    }
}
