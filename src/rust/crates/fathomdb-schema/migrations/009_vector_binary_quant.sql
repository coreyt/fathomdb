-- 0.7.0 Pack 1: vector binary-quant data encoding.
-- Per dev/design/0.7.0-vector-quant-pack1.md D4 (fix-3), with a
-- runtime-deviation noted in dev/plans/runs/0.7.0-PVQ-P1-IMPL-output.json:
-- the vec0 reshape itself is dim-aware and lives in
-- fathomdb-engine's ensure_vector_partition (Rust); SCHEMA_VERSION=9
-- and this SQL-side preflight stay in fathomdb-schema. The schema
-- linter sees a CREATE TABLE + DROP TABLE in this file, so no
-- accretion exemption is needed.

CREATE TEMP TABLE _vec0_migration_assertion(
    check_passes INTEGER NOT NULL CHECK(check_passes = 1)
);
INSERT INTO _vec0_migration_assertion(check_passes)
    SELECT CASE WHEN EXISTS (
        SELECT 1 FROM _fathomdb_vector_rows
        WHERE kind NOT IN ('email','article','paper','meeting','note','todo','doc')
    ) THEN 0 ELSE 1 END;
DROP TABLE _vec0_migration_assertion;
