-- 0.7.0 Pack 1: vector binary-quant data encoding.
-- Per dev/design/0.7.0-vector-quant-pack1.md D4 (fix-3).
-- This migration both creates (fresh DBs) and drops/recreates
-- (established DBs) vector_default, so the accretion guard is
-- satisfied by the in-step DROP TABLE without an exemption.

CREATE VIRTUAL TABLE IF NOT EXISTS vector_default USING vec0(
    embedding float[768]
);

CREATE TEMP TABLE _vec0_migration_assertion(
    check_passes INTEGER NOT NULL CHECK(check_passes = 1)
);
INSERT INTO _vec0_migration_assertion(check_passes)
    SELECT CASE WHEN EXISTS (
        SELECT 1 FROM _fathomdb_vector_rows
        WHERE kind NOT IN ('email','article','paper','meeting','note','todo','doc')
    ) THEN 0 ELSE 1 END;

CREATE TABLE _fathomdb_vector_migration_v0_7_0 (
    rowid     INTEGER PRIMARY KEY,
    embedding BLOB NOT NULL,
    kind      TEXT NOT NULL
);
INSERT INTO _fathomdb_vector_migration_v0_7_0(rowid, embedding, kind)
    SELECT v.rowid, v.embedding, r.kind
    FROM vector_default v
    JOIN _fathomdb_vector_rows r ON r.rowid = v.rowid;

DROP TABLE vector_default;
CREATE VIRTUAL TABLE vector_default USING vec0(
    embedding float[768],
    embedding_bin bit[768],
    source_type TEXT partition key,
    kind TEXT,
    created_at INTEGER
);

INSERT INTO vector_default(
    rowid, embedding, embedding_bin, source_type, kind, created_at
)
SELECT
    s.rowid,
    s.embedding,
    vec_quantize_binary(s.embedding),
    CASE s.kind
        WHEN 'email'   THEN 'email'
        WHEN 'article' THEN 'article'
        WHEN 'paper'   THEN 'paper'
        WHEN 'meeting' THEN 'meeting'
        WHEN 'note'    THEN 'note'
        WHEN 'todo'    THEN 'todo'
        WHEN 'doc'     THEN 'article'
        ELSE 'article'
    END,
    s.kind,
    strftime('%s', 'now')
FROM _fathomdb_vector_migration_v0_7_0 s;

DROP TABLE _fathomdb_vector_migration_v0_7_0;
DROP TABLE _vec0_migration_assertion;
