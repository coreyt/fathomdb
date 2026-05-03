-- MIGRATION-ACCRETION-EXEMPTION: 0.6 bootstrap creates operational store tables
CREATE TABLE IF NOT EXISTS operational_collections(
    name TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('append_only_log', 'latest_state')),
    schema_json TEXT NOT NULL,
    retention_json TEXT NOT NULL,
    format_version INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS operational_mutations(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    collection_name TEXT NOT NULL,
    record_key TEXT NOT NULL,
    op_kind TEXT NOT NULL CHECK(op_kind = 'append'),
    payload_json TEXT NOT NULL,
    schema_id TEXT,
    write_cursor INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS operational_state(
    collection_name TEXT NOT NULL,
    record_key TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    schema_id TEXT,
    write_cursor INTEGER NOT NULL,
    PRIMARY KEY(collection_name, record_key)
);

CREATE TABLE IF NOT EXISTS _fathomdb_open_state(
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR IGNORE INTO operational_collections(
    name, kind, schema_json, retention_json, format_version, created_at
) VALUES (
    'projection_failures',
    'append_only_log',
    '{"type":"object"}',
    '{}',
    1,
    0
);
