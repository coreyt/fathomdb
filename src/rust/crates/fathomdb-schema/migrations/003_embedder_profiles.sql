-- MIGRATION-ACCRETION-EXEMPTION: 0.6 bootstrap creates embedder profile compatibility table
CREATE TABLE IF NOT EXISTS _fathomdb_embedder_profiles(
    profile TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    revision TEXT NOT NULL,
    dimension INTEGER NOT NULL
);
