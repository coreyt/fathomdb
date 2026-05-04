-- MIGRATION-ACCRETION-EXEMPTION: additive Phase 9 vector runtime tables and vec0 partition
CREATE TABLE IF NOT EXISTS _fathomdb_projection_state(
    kind TEXT PRIMARY KEY,
    last_enqueued_cursor INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS _fathomdb_vector_kinds(
    kind TEXT PRIMARY KEY,
    profile TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS _fathomdb_vector_rows(
    rowid INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    write_cursor INTEGER NOT NULL UNIQUE
);
