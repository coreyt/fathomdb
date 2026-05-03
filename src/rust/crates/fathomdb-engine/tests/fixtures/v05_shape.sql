PRAGMA user_version = 1;

CREATE TABLE fathom_nodes(
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    body TEXT NOT NULL
);

INSERT INTO fathom_nodes(id, kind, body)
VALUES('legacy-node-1', 'doc', '{"body":"legacy 0.5 row"}');
