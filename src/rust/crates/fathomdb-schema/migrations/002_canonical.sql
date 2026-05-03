-- MIGRATION-ACCRETION-EXEMPTION: 0.6 bootstrap creates canonical storage tables
CREATE TABLE IF NOT EXISTS _fathomdb_migrations(
    step_id INTEGER PRIMARY KEY,
    applied_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS canonical_nodes(
    write_cursor INTEGER NOT NULL,
    kind TEXT NOT NULL,
    body TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS canonical_edges(
    write_cursor INTEGER NOT NULL,
    kind TEXT NOT NULL,
    from_id TEXT NOT NULL,
    to_id TEXT NOT NULL
);
