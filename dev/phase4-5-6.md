# Phase 4

## Test plan coverage gaps

- Several Layer 1 (pragma checks, WAL behavior) and Layer 2 (field-level write assertions) tests are unstaffed per test-plan.md

---

# Phase 5

## Semantic integrity gaps (memex-gap-map.md)

- ✅ **Dangling edge detection** — `check_semantics` now reports `dangling_edges`
- ✅ **Stale vec row detection** — `check_semantics` reports stale vec rows and vec rows for superseded nodes
- ✅ **Durable audit trail (retire/excise)** — provenance events are persisted and queryable

## Vector lifecycle completeness

- ✅ Vec cleanup on `NodeRetire` / `ChunkPolicy::Replace` (parallel to existing FTS cleanup)
- ✅ `rebuild --target vec` path through the admin surface

## Read surface breadth

- Wider `QueryRows` result families (currently nodes + sparse runtime rows)
- ✅ `execute_compiled_read` returns graph traversal results
- Capability degradation model (return partial results when `sqlite-vec` absent, rather than hard error)

---

# Phase 6

## Admin bridge binary

- Rust JSON-over-stdio binary target in `fathomdb-engine`
- Required before Go's `rebuild`, `rebuild-missing`, and `excise` commands can use engine semantics (currently go direct-SQLite or aren't wired)
