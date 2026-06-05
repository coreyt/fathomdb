-- 0.8.0 Slice 33 (G3 / F4-READ) — op-store paginated read-back hardening.
-- The governed `read.collection` / `read.mutations` SELECT is
-- `WHERE collection_name = ?1 AND id > ?2 ORDER BY id LIMIT ?3`. Without an index
-- on `collection_name`, SQLite rides the `id` PRIMARY KEY and filters
-- `collection_name` row-by-row over the id-ordered log — O(rows-scanned) for a
-- small collection inside a large multi-collection log. The composite
-- `(collection_name, id)` index makes the plan index-driven: the leading
-- equality on `collection_name` fixes the prefix, the trailing `id` serves both
-- the after-id cursor range and `ORDER BY id` with no temp B-tree — O(page).
-- Pure additive `CREATE INDEX` (no table/column add, no DROP, no reshape), so the
-- accretion guard does not flag it and no exemption marker is required (unlike a
-- column-adding step, which requires the marker). Mirror of the inline
-- step-13 Migration in fathomdb-schema/src/lib.rs; see
-- dev/design/slice-33-cursor-hardening-design.md.
CREATE INDEX IF NOT EXISTS operational_mutations_collection_id_idx
    ON operational_mutations(collection_name, id);
