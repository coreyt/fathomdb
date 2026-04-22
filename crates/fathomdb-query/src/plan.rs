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

#[allow(clippy::too_many_lines)]
pub fn shape_signature(ast: &QueryAst) -> String {
    let mut signature = String::new();
    let _ = write!(&mut signature, "Root({})", ast.root_kind);

    for step in &ast.steps {
        match step {
            QueryStep::Search { limit, .. } => {
                let _ = write!(&mut signature, "-Search(limit={limit})");
            }
            QueryStep::VectorSearch { limit, .. } => {
                let _ = write!(&mut signature, "-Vector(limit={limit})");
            }
            QueryStep::SemanticSearch { limit, .. } => {
                let _ = write!(&mut signature, "-Semantic(limit={limit})");
            }
            QueryStep::RawVectorSearch { vec, limit } => {
                let _ = write!(&mut signature, "-RawVec(dim={},limit={limit})", vec.len());
            }
            QueryStep::TextSearch { limit, .. } => {
                let _ = write!(&mut signature, "-Text(limit={limit})");
            }
            QueryStep::Traverse {
                direction,
                label,
                max_depth,
                filter: _,
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
                Predicate::JsonPathFusedEq { path, .. } => {
                    let _ = write!(&mut signature, "-Filter(json_fused_eq:{path})");
                }
                Predicate::JsonPathFusedTimestampCmp { path, op, .. } => {
                    let op = match op {
                        crate::ComparisonOp::Gt => "gt",
                        crate::ComparisonOp::Gte => "gte",
                        crate::ComparisonOp::Lt => "lt",
                        crate::ComparisonOp::Lte => "lte",
                    };
                    let _ = write!(&mut signature, "-Filter(json_fused_ts_cmp:{path}:{op})");
                }
                Predicate::JsonPathFusedBoolEq { path, .. } => {
                    let _ = write!(&mut signature, "-Filter(json_fused_bool_eq:{path})");
                }
                Predicate::EdgePropertyEq { path, .. } => {
                    let _ = write!(&mut signature, "-Filter(edge_eq:{path})");
                }
                Predicate::EdgePropertyCompare { path, op, .. } => {
                    let op = match op {
                        crate::ComparisonOp::Gt => "gt",
                        crate::ComparisonOp::Gte => "gte",
                        crate::ComparisonOp::Lt => "lt",
                        crate::ComparisonOp::Lte => "lte",
                    };
                    let _ = write!(&mut signature, "-Filter(edge_cmp:{path}:{op})");
                }
                Predicate::JsonPathFusedIn { path, values } => {
                    let _ = write!(
                        &mut signature,
                        "-Filter(json_fused_in:{path}:n={})",
                        values.len()
                    );
                }
                Predicate::JsonPathIn { path, values } => {
                    let _ = write!(&mut signature, "-Filter(json_in:{path}:n={})", values.len());
                }
            },
        }
    }

    for expansion in &ast.expansions {
        let dir = match expansion.direction {
            TraverseDirection::In => "in",
            TraverseDirection::Out => "out",
        };
        let edge_filter_str = match &expansion.edge_filter {
            None => String::new(),
            Some(Predicate::EdgePropertyEq { path, .. }) => {
                format!(",edge_eq:{path}")
            }
            Some(Predicate::EdgePropertyCompare { path, op, .. }) => {
                let op_str = match op {
                    crate::ComparisonOp::Gt => "gt",
                    crate::ComparisonOp::Gte => "gte",
                    crate::ComparisonOp::Lt => "lt",
                    crate::ComparisonOp::Lte => "lte",
                };
                format!(",edge_cmp:{path}:{op_str}")
            }
            Some(p) => unreachable!("edge_filter predicate {p:?} not handled in shape_signature"),
        };
        let _ = write!(
            &mut signature,
            "-Expand(slot={},direction={dir},label={},depth={}{})",
            expansion.slot, expansion.label, expansion.max_depth, edge_filter_str
        );
    }

    for edge_expansion in &ast.edge_expansions {
        let dir = match edge_expansion.direction {
            TraverseDirection::In => "in",
            TraverseDirection::Out => "out",
        };
        let _ = write!(
            &mut signature,
            "-EdgeExpand(slot={},direction={dir},label={},depth={},edge_filter={},endpoint_filter={})",
            edge_expansion.slot,
            edge_expansion.label,
            edge_expansion.max_depth,
            edge_expansion.edge_filter.is_some(),
            edge_expansion.endpoint_filter.is_some(),
        );
    }

    if let Some(limit) = ast.final_limit {
        let _ = write!(&mut signature, "-Limit({limit})");
    }

    signature
}

#[cfg(test)]
#[allow(deprecated)]
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

    #[test]
    fn shape_signature_differs_for_different_edge_filters() {
        use crate::{ComparisonOp, ExpansionSlot, Predicate, QueryAst, ScalarValue};

        let base_expansion = ExpansionSlot {
            slot: "tasks".to_owned(),
            direction: TraverseDirection::Out,
            label: "HAS_TASK".to_owned(),
            max_depth: 1,
            filter: None,
            edge_filter: None,
        };

        let ast_no_filter = QueryAst {
            root_kind: "Meeting".to_owned(),
            steps: vec![],
            expansions: vec![base_expansion.clone()],
            edge_expansions: vec![],
            final_limit: None,
        };

        let ast_with_eq_filter = QueryAst {
            root_kind: "Meeting".to_owned(),
            steps: vec![],
            expansions: vec![ExpansionSlot {
                edge_filter: Some(Predicate::EdgePropertyEq {
                    path: "$.rel".to_owned(),
                    value: ScalarValue::Text("cites".to_owned()),
                }),
                ..base_expansion.clone()
            }],
            edge_expansions: vec![],
            final_limit: None,
        };

        let ast_with_cmp_filter = QueryAst {
            root_kind: "Meeting".to_owned(),
            steps: vec![],
            expansions: vec![ExpansionSlot {
                edge_filter: Some(Predicate::EdgePropertyCompare {
                    path: "$.weight".to_owned(),
                    op: ComparisonOp::Gt,
                    value: ScalarValue::Integer(5),
                }),
                ..base_expansion
            }],
            edge_expansions: vec![],
            final_limit: None,
        };

        let sig_no_filter = super::shape_signature(&ast_no_filter);
        let sig_eq_filter = super::shape_signature(&ast_with_eq_filter);
        let sig_cmp_filter = super::shape_signature(&ast_with_cmp_filter);

        assert_ne!(
            sig_no_filter, sig_eq_filter,
            "no edge_filter and EdgePropertyEq must produce different signatures"
        );
        assert_ne!(
            sig_no_filter, sig_cmp_filter,
            "no edge_filter and EdgePropertyCompare must produce different signatures"
        );
        assert_ne!(
            sig_eq_filter, sig_cmp_filter,
            "EdgePropertyEq and EdgePropertyCompare must produce different signatures"
        );
    }

    #[test]
    fn shape_signature_differs_for_different_edge_expansions() {
        use crate::{EdgeExpansionSlot, Predicate, QueryAst, ScalarValue};

        let base = EdgeExpansionSlot {
            slot: "cites".to_owned(),
            direction: TraverseDirection::Out,
            label: "CITES".to_owned(),
            max_depth: 1,
            endpoint_filter: None,
            edge_filter: None,
        };

        let ast_no_edge_expansions = QueryAst {
            root_kind: "Paper".to_owned(),
            steps: vec![],
            expansions: vec![],
            edge_expansions: vec![],
            final_limit: None,
        };

        let ast_with_edge_expansion = QueryAst {
            root_kind: "Paper".to_owned(),
            steps: vec![],
            expansions: vec![],
            edge_expansions: vec![base.clone()],
            final_limit: None,
        };

        let ast_with_different_slot_name = QueryAst {
            root_kind: "Paper".to_owned(),
            steps: vec![],
            expansions: vec![],
            edge_expansions: vec![EdgeExpansionSlot {
                slot: "references".to_owned(),
                ..base.clone()
            }],
            final_limit: None,
        };

        let ast_with_edge_filter = QueryAst {
            root_kind: "Paper".to_owned(),
            steps: vec![],
            expansions: vec![],
            edge_expansions: vec![EdgeExpansionSlot {
                edge_filter: Some(Predicate::EdgePropertyEq {
                    path: "$.rel".to_owned(),
                    value: ScalarValue::Text("primary".to_owned()),
                }),
                ..base.clone()
            }],
            final_limit: None,
        };

        let ast_with_endpoint_filter = QueryAst {
            root_kind: "Paper".to_owned(),
            steps: vec![],
            expansions: vec![],
            edge_expansions: vec![EdgeExpansionSlot {
                endpoint_filter: Some(Predicate::KindEq("Paper".to_owned())),
                ..base
            }],
            final_limit: None,
        };

        let sig_none = super::shape_signature(&ast_no_edge_expansions);
        let sig_basic = super::shape_signature(&ast_with_edge_expansion);
        let sig_diff_slot = super::shape_signature(&ast_with_different_slot_name);
        let sig_edge = super::shape_signature(&ast_with_edge_filter);
        let sig_endpoint = super::shape_signature(&ast_with_endpoint_filter);

        assert_ne!(
            sig_none, sig_basic,
            "presence of edge_expansions must change signature"
        );
        assert_ne!(
            sig_basic, sig_diff_slot,
            "different edge_expansion slot names must change signature"
        );
        assert_ne!(
            sig_basic, sig_edge,
            "edge_filter presence must change signature"
        );
        assert_ne!(
            sig_basic, sig_endpoint,
            "endpoint_filter presence must change signature"
        );
    }
}
