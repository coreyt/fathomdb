// Property-test scaffold for fathomdb-schema.
//
// Real targets (per ADR-0.6.0-json-schema-policy and ADR-0.6.0-decision-index):
//   - schema parse/serialize idempotence: parse(serialize(s)) == s
//   - schema version monotonicity: bootstrap_steps stable across builds
//   - field-name canonicalization: round-trips through wire form
//
// Replace this trivial property when real schema types land.

use proptest::prelude::*;

proptest! {
    #[test]
    fn schema_version_is_constant(_x in any::<u32>()) {
        prop_assert_eq!(fathomdb_schema::SCHEMA_VERSION, 1);
    }
}
