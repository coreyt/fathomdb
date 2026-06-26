---
name: fathomdb-v05-graph-lineage
description: "FathomDB v0.5.x WAS a full document+graph+KV store (real tables nodes/edges, not fathom_*); 0.6.0 stripped it to 5 verbs; 0.8.0 revives a subset. Don't conflate the three artifacts."
metadata: 
  node_type: memory
  type: reference
  originSessionId: 482125fb-352f-45de-a97b-7961c5482408
---

Git-verified 2026-06-01 against the `v0.5.0` tag. Three distinct artifacts — I
conflated them twice before getting it right:

1. **`fathom_nodes`/`fathom_edges`/`fathom_chunks`** — literal names, **never a
   shipped table** (`git log -S "CREATE TABLE fathom_nodes"` empty across all
   tags). Only a rejection tripwire (`reject_legacy_shape`, `lib.rs:4529`).
   `tests/fixtures/v05_shape.sql` `fathom_nodes(id,kind,body)` is a SYNTHETIC stub,
   not the real schema.
2. **`nodes`/`edges`/`chunks`** — the REAL v0.5.x graph layer (`git show
   v0.5.0:crates/fathomdb-schema/src/bootstrap.rs`, ~36 tables): bitemporal
   `superseded_at`, per-fact `confidence`, dual-endpoint indexes,
   `traverse()`/`expand()`/`TraverseDirection` (query builder), dangling-edge
   detection + `restore_validated_edges` (admin.rs), grouped queries, FTS property
   schemas, rich typed JSON-path filter DSL, in-process admin API. **Deleted by the
   0.6.0 rewrite.** v0.5.x `fusion.rs` was FILTER-fusion, NOT RRF/score fusion.
3. **`canonical_nodes`/`canonical_edges`** — rewrite-era append-only tables;
   what 0.8.0 G0/G5/G8/G11 build on.

**Why:** 0.6.0 deliberately stripped a full doc+graph+KV store to a 5-verb
retrieval engine (AC-057a); 0.8.0 revives only a subset, and G9 (RRF) actually
exceeds v0.5.x. **How to apply:** G5/G8/G11 are net-new code on `canonical_edges`
that *conceptually revive* v0.5.x capability — NOT a rename, NOT `fathom_*`. Use
v0.5.x `admin.rs`/`builder.rs` (via git) as a working reference for G8/G5. Full
detail: `dev/profiling/v05-lineage.md`. Relates to [[fathomdb-consumer-agents]].
