use std::fmt::Write;

use crate::{Predicate, QueryAst, QueryStep, TraverseDirection};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrivingTable {
    Nodes,
    FtsNodes,
    VecNodes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionHints {
    pub recursion_limit: usize,
    pub hard_limit: usize,
}

pub fn choose_driving_table(ast: &QueryAst) -> DrivingTable {
    let has_deterministic_id_filter = ast.steps.iter().any(|step| {
        matches!(
            step,
            QueryStep::Filter(Predicate::LogicalIdEq(_) | Predicate::SourceRefEq(_))
        )
    });

    if has_deterministic_id_filter {
        DrivingTable::Nodes
    } else if ast
        .steps
        .iter()
        .any(|step| matches!(step, QueryStep::VectorSearch { .. }))
    {
        DrivingTable::VecNodes
    } else if ast
        .steps
        .iter()
        .any(|step| matches!(step, QueryStep::TextSearch { .. }))
    {
        DrivingTable::FtsNodes
    } else {
        DrivingTable::Nodes
    }
}

pub fn execution_hints(ast: &QueryAst) -> ExecutionHints {
    let step_limit = ast
        .steps
        .iter()
        .find_map(|step| {
            if let QueryStep::Traverse { max_depth, .. } = step {
                Some(*max_depth)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let expansion_limit = ast
        .expansions
        .iter()
        .map(|expansion| expansion.max_depth)
        .max()
        .unwrap_or(0);
    let recursion_limit = step_limit.max(expansion_limit);

    ExecutionHints {
        recursion_limit,
        // FIX(review): was .max(1000) — always produced >= 1000, ignoring user's final_limit.
        // Options considered: (A) use final_limit directly with default, (B) .min(MAX) ceiling,
        // (C) decouple from final_limit. Chose (A): the CTE LIMIT should honor the user's
        // requested limit; the depth bound (compile.rs:177) already constrains recursion.
        hard_limit: ast.final_limit.unwrap_or(1000),
    }
}

pub fn shape_signature(ast: &QueryAst) -> String {
    let mut signature = String::new();
    let _ = write!(&mut signature, "Root({})", ast.root_kind);

    for step in &ast.steps {
        match step {
            QueryStep::VectorSearch { limit, .. } => {
                let _ = write!(&mut signature, "-Vector(limit={limit})");
            }
            QueryStep::TextSearch { limit, .. } => {
                let _ = write!(&mut signature, "-Text(limit={limit})");
            }
            QueryStep::Traverse {
                direction,
                label,
                max_depth,
            } => {
                let dir = match direction {
                    TraverseDirection::In => "in",
                    TraverseDirection::Out => "out",
                };
                let _ = write!(
                    &mut signature,
                    "-Traverse(direction={dir},label={label},depth={max_depth})"
                );
            }
            QueryStep::Filter(predicate) => match predicate {
                Predicate::LogicalIdEq(_) => signature.push_str("-Filter(logical_id_eq)"),
                Predicate::KindEq(_) => signature.push_str("-Filter(kind_eq)"),
                Predicate::JsonPathEq { path, .. } => {
                    let _ = write!(&mut signature, "-Filter(json_eq:{path})");
                }
                Predicate::JsonPathCompare { path, op, .. } => {
                    let op = match op {
                        crate::ComparisonOp::Gt => "gt",
                        crate::ComparisonOp::Gte => "gte",
                        crate::ComparisonOp::Lt => "lt",
                        crate::ComparisonOp::Lte => "lte",
                    };
                    let _ = write!(&mut signature, "-Filter(json_cmp:{path}:{op})");
                }
                Predicate::SourceRefEq(_) => signature.push_str("-Filter(source_ref_eq)"),
                Predicate::ContentRefNotNull => {
                    signature.push_str("-Filter(content_ref_not_null)");
                }
                Predicate::ContentRefEq(_) => signature.push_str("-Filter(content_ref_eq)"),
            },
        }
    }

    for expansion in &ast.expansions {
        let dir = match expansion.direction {
            TraverseDirection::In => "in",
            TraverseDirection::Out => "out",
        };
        let _ = write!(
            &mut signature,
            "-Expand(slot={},direction={dir},label={},depth={})",
            expansion.slot, expansion.label, expansion.max_depth
        );
    }

    if let Some(limit) = ast.final_limit {
        let _ = write!(&mut signature, "-Limit({limit})");
    }

    signature
}

#[cfg(test)]
mod tests {
    use crate::{DrivingTable, QueryBuilder, TraverseDirection};

    use super::{choose_driving_table, execution_hints};

    #[test]
    fn deterministic_filter_overrides_vector_driver() {
        let ast = QueryBuilder::nodes("Meeting")
            .vector_search("budget", 5)
            .filter_logical_id_eq("meeting-123")
            .into_ast();

        assert_eq!(choose_driving_table(&ast), DrivingTable::Nodes);
    }

    #[test]
    fn hard_limit_honors_user_specified_limit_below_default() {
        let ast = QueryBuilder::nodes("Meeting")
            .traverse(TraverseDirection::Out, "HAS_TASK", 3)
            .limit(10)
            .into_ast();

        let hints = execution_hints(&ast);
        assert_eq!(
            hints.hard_limit, 10,
            "hard_limit must honor user's final_limit"
        );
    }

    #[test]
    fn hard_limit_defaults_to_1000_when_no_limit_set() {
        let ast = QueryBuilder::nodes("Meeting")
            .traverse(TraverseDirection::Out, "HAS_TASK", 3)
            .into_ast();

        let hints = execution_hints(&ast);
        assert_eq!(hints.hard_limit, 1000, "hard_limit defaults to 1000");
    }
}
