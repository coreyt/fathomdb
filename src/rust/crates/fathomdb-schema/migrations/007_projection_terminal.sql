-- MIGRATION-ACCRETION-EXEMPTION: additive terminal projection state table
CREATE TABLE IF NOT EXISTS _fathomdb_projection_terminal(
    write_cursor INTEGER PRIMARY KEY,
    state TEXT NOT NULL CHECK(state IN ('failed', 'up_to_date'))
);
