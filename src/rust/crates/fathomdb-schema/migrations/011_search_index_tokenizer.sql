-- MIGRATION-ACCRETION-EXEMPTION: tokenizer-default upgrade (drop+recreate FTS5 projection; no source-record migration)
-- 0.8.0 Slice 5 (G1) — global FTS5 tokenizer-default upgrade. Migrations are
-- forward-only and immutable and FTS5 has no `ALTER ... tokenize`, so the
-- tokenizer default is upgraded by dropping and recreating the `search_index`
-- virtual table (NOT by editing the step-5 DDL, which would change the
-- tokenizer for new DBs only). The engine re-tokenizes from the canonical
-- source rows immediately after this step lands (projection-only; no
-- source-record migration). Mirror of the inline step-11 Migration in
-- fathomdb-schema/src/lib.rs; see dev/design/0.8.0-slice-5-G1-design.md.
DROP TABLE IF EXISTS search_index;
CREATE VIRTUAL TABLE search_index USING fts5(
    body,
    kind UNINDEXED,
    write_cursor UNINDEXED,
    tokenize = 'porter unicode61 remove_diacritics 2'
);
