//! Filter-fusion helpers for search-driven query pipelines.
//!
//! Phase 2 filter fusion classifies `Filter(Predicate)` steps following a
//! search step into **fusable** predicates — those that can be pushed into
//! the driving-search CTE's `WHERE` clause so the CTE `LIMIT` applies *after*
//! filtering — and **residual** predicates that remain in the outer `WHERE`.
//!
//! A predicate is fusable when it can be evaluated against columns available
//! on the `nodes` table joined inside the search CTE (`kind`, `logical_id`,
//! `source_ref`, `content_ref`). JSON-property predicates are residual: they
//! require `json_extract` against the `n.properties` column projected by the
//! outer SELECT.

use crate::{Predicate, QueryStep};

/// Partition `Filter` predicates **following a search step** into fusable
/// and residual sets, preserving source order within each set.
///
/// # Returns
///
/// A `(fusable, residual)` pair where:
///
/// * `fusable` contains predicates that can be injected into the driving
///   search CTE's `WHERE` clause (currently
///   [`Predicate::KindEq`], [`Predicate::LogicalIdEq`],
///   [`Predicate::SourceRefEq`], [`Predicate::ContentRefEq`], and
///   [`Predicate::ContentRefNotNull`]).
/// * `residual` contains predicates that remain in the outer `WHERE`
///   (currently [`Predicate::JsonPathEq`] and
///   [`Predicate::JsonPathCompare`]).
///
/// Non-`Filter` steps (search steps, traversals) are ignored.
///
/// Only `Filter` steps that appear **after** the first `TextSearch` or
/// `VectorSearch` step contribute to the partition; predicates placed
/// before a search step do not belong to the search-driven path and are
/// skipped. When no search step is present, both returned vectors are
/// empty.
#[must_use]
pub fn partition_search_filters(steps: &[QueryStep]) -> (Vec<Predicate>, Vec<Predicate>) {
    let mut fusable = Vec::new();
    let mut residual = Vec::new();
    let mut seen_search = false;
    for step in steps {
        match step {
            QueryStep::Search { .. }
            | QueryStep::TextSearch { .. }
            | QueryStep::VectorSearch { .. } => {
                seen_search = true;
            }
            QueryStep::Filter(predicate) if seen_search => {
                if is_fusable(predicate) {
                    fusable.push(predicate.clone());
                } else {
                    residual.push(predicate.clone());
                }
            }
            _ => {}
        }
    }
    (fusable, residual)
}

/// Whether a predicate can be fused into a search CTE's `WHERE` clause.
#[must_use]
pub fn is_fusable(predicate: &Predicate) -> bool {
    matches!(
        predicate,
        Predicate::KindEq(_)
            | Predicate::LogicalIdEq(_)
            | Predicate::SourceRefEq(_)
            | Predicate::ContentRefEq(_)
            | Predicate::ContentRefNotNull
            | Predicate::JsonPathFusedEq { .. }
            | Predicate::JsonPathFusedTimestampCmp { .. }
    )
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{ComparisonOp, ScalarValue};

    #[test]
    fn partition_search_filters_separates_fusable_from_residual() {
        use crate::TextQuery;
        let steps = vec![
            QueryStep::TextSearch {
                query: TextQuery::Empty,
                limit: 10,
            },
            QueryStep::Filter(Predicate::KindEq("Goal".to_owned())),
            QueryStep::Filter(Predicate::LogicalIdEq("g-1".to_owned())),
            QueryStep::Filter(Predicate::SourceRefEq("src".to_owned())),
            QueryStep::Filter(Predicate::ContentRefEq("uri".to_owned())),
            QueryStep::Filter(Predicate::ContentRefNotNull),
            QueryStep::Filter(Predicate::JsonPathEq {
                path: "$.status".to_owned(),
                value: ScalarValue::Text("active".to_owned()),
            }),
            QueryStep::Filter(Predicate::JsonPathCompare {
                path: "$.priority".to_owned(),
                op: ComparisonOp::Gte,
                value: ScalarValue::Integer(5),
            }),
        ];

        let (fusable, residual) = partition_search_filters(&steps);
        assert_eq!(fusable.len(), 5, "all five fusable variants must fuse");
        assert_eq!(residual.len(), 2, "both JSON predicates must stay residual");
        assert!(matches!(fusable[0], Predicate::KindEq(_)));
        assert!(matches!(fusable[1], Predicate::LogicalIdEq(_)));
        assert!(matches!(fusable[2], Predicate::SourceRefEq(_)));
        assert!(matches!(fusable[3], Predicate::ContentRefEq(_)));
        assert!(matches!(fusable[4], Predicate::ContentRefNotNull));
        assert!(matches!(residual[0], Predicate::JsonPathEq { .. }));
        assert!(matches!(residual[1], Predicate::JsonPathCompare { .. }));
    }

    #[test]
    fn partition_ignores_non_filter_steps() {
        use crate::TextQuery;
        let steps = vec![
            QueryStep::TextSearch {
                query: TextQuery::Empty,
                limit: 5,
            },
            QueryStep::Filter(Predicate::KindEq("Goal".to_owned())),
        ];
        let (fusable, residual) = partition_search_filters(&steps);
        assert_eq!(fusable.len(), 1);
        assert_eq!(residual.len(), 0);
    }

    #[test]
    fn partition_search_filters_ignores_filters_before_search_step() {
        use crate::TextQuery;
        let steps = vec![
            // This filter appears BEFORE the search step and must be ignored.
            QueryStep::Filter(Predicate::KindEq("A".to_owned())),
            QueryStep::TextSearch {
                query: TextQuery::Empty,
                limit: 10,
            },
            // This filter appears AFTER the search step and must be fusable.
            QueryStep::Filter(Predicate::KindEq("B".to_owned())),
        ];
        let (fusable, residual) = partition_search_filters(&steps);
        assert_eq!(fusable.len(), 1);
        assert_eq!(fusable[0], Predicate::KindEq("B".to_owned()));
        assert!(residual.is_empty());
    }

    #[test]
    fn fused_json_variants_are_fusable() {
        assert!(is_fusable(&Predicate::JsonPathFusedEq {
            path: "$.status".to_owned(),
            value: "active".to_owned(),
        }));
        assert!(is_fusable(&Predicate::JsonPathFusedTimestampCmp {
            path: "$.written_at".to_owned(),
            op: ComparisonOp::Gt,
            value: 1234,
        }));
    }

    #[test]
    fn non_fused_json_variants_stay_residual() {
        assert!(!is_fusable(&Predicate::JsonPathEq {
            path: "$.status".to_owned(),
            value: ScalarValue::Text("active".to_owned()),
        }));
        assert!(!is_fusable(&Predicate::JsonPathCompare {
            path: "$.priority".to_owned(),
            op: ComparisonOp::Gte,
            value: ScalarValue::Integer(5),
        }));
    }

    #[test]
    fn partition_search_filters_returns_empty_without_search_step() {
        let steps = vec![QueryStep::Filter(Predicate::KindEq("A".to_owned()))];
        let (fusable, residual) = partition_search_filters(&steps);
        assert!(fusable.is_empty());
        assert!(residual.is_empty());
    }
}
