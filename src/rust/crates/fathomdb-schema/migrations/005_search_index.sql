CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
    body,
    kind UNINDEXED,
    write_cursor UNINDEXED
);
