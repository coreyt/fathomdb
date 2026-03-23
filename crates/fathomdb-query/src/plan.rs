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
    let recursion_limit = ast
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

    ExecutionHints {
        recursion_limit,
        hard_limit: ast.final_limit.unwrap_or(1000).max(1000),
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
                Predicate::SourceRefEq(_) => signature.push_str("-Filter(source_ref_eq)"),
            },
        }
    }

    if let Some(limit) = ast.final_limit {
        let _ = write!(&mut signature, "-Limit({limit})");
    }

    signature
}

#[cfg(test)]
mod tests {
    use crate::{DrivingTable, QueryBuilder};

    use super::choose_driving_table;

    #[test]
    fn deterministic_filter_overrides_vector_driver() {
        let ast = QueryBuilder::nodes("Meeting")
            .vector_search("budget", 5)
            .filter_logical_id_eq("meeting-123")
            .into_ast();

        assert_eq!(choose_driving_table(&ast), DrivingTable::Nodes);
    }
}
