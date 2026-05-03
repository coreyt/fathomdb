// Property-test scaffold for fathomdb-schema.
//
// Real targets (per ADR-0.6.0-json-schema-policy and ADR-0.6.0-decision-index):
//   - schema parse/serialize idempotence: parse(serialize(s)) == s
//   - schema version monotonicity: migration registry stays contiguous
//   - field-name canonicalization: round-trips through wire form
//
// Extend this as real schema types land.

use proptest::prelude::*;

proptest! {
    #[test]
    fn schema_version_covers_registered_migrations(_x in any::<u32>()) {
        let max_step = fathomdb_schema::MIGRATIONS.iter().map(|step| step.step_id).max().unwrap();
        prop_assert_eq!(fathomdb_schema::SCHEMA_VERSION, max_step);
    }
}
