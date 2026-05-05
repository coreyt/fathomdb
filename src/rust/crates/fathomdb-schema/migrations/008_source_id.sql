-- MIGRATION-ACCRETION-EXEMPTION: Phase 9 Pack B introduces the
-- canonical `source_id` recovery seam (REQ-026, AC-028a/b/c, AC-042).
-- The column is required to support `excise_source` / `trace_source_ref`
-- without scanning every row in the database. There is no offsetting
-- removal candidate in the 0.6.0 surface: every existing canonical column
-- is load-bearing for write replay, projection scheduling, or recovery
-- locators. REQ-045 accretion-guard offset is therefore documented here
-- as inherently impossible for this slice; the next schema-touching pack
-- carries the offset budget for two adds.
ALTER TABLE canonical_nodes ADD COLUMN source_id TEXT;
ALTER TABLE canonical_edges ADD COLUMN source_id TEXT;
CREATE INDEX IF NOT EXISTS canonical_nodes_source_id_idx
    ON canonical_nodes(source_id);
CREATE INDEX IF NOT EXISTS canonical_edges_source_id_idx
    ON canonical_edges(source_id);
