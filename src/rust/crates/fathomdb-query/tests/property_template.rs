// Property-test scaffold for fathomdb-query.
//
// Real targets (per ADRs):
//   - retrieval latency gates: results within p99 budget for synthetic loads
//     (ADR-0.6.0-retrieval-latency-gates, ADR-0.6.0-text-query-latency-gates)
//   - retrieval pipeline shape invariants: stage ordering preserved
//     (ADR-0.6.0-retrieval-pipeline-shape)
//   - query parse/normalize idempotence
//
// Replace this trivial property when query types land.

use proptest::prelude::*;

proptest! {
    #[test]
    fn placeholder_identity(s in "[a-z]{0,10}") {
        prop_assert_eq!(s.clone(), s);
    }
}
