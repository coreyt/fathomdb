# Execution Plan: v0.5.1

**Date written:** 2026-04-17  
**Based on:** `dev/notes/0.5.1-scope.md` and per-item design docs  
**Runbook:** `dev/notes/agent-harness-runbook.md`

---

## Pre-flight checklist (run before first launch)

```bash
cd /home/coreyt/projects/fathomdb
./scripts/preflight.sh --baseline
```

Record:
- `BASE_COMMIT` = HEAD hash on main
- Pre-existing clippy warnings (do not count against packs)
- Test count baseline: `cargo nextest run --workspace 2>&1 | tail -5`

---

## Pack summary

| Pack | Scope items | Key files touched | Phase |
|---|---|---|---|
| A | Item 4 (`JsonPathFusedBoolEq`) | `ast.rs`, `coordinator.rs` (fused builders), `builder.rs`, `fusion.rs`, Python+TS | 1 |
| B | Item 5 (`matched_paths` chunk) | `coordinator.rs` (attribution wiring), `text_search_surface.rs` | 1 |
| C | Item 6 (`StellaEmbedder` baseUrl) | `typescript/.../embedders/stella.ts` | 1 |
| D | Item 1+2 (edge property filter + round-trip) | `ast.rs`, `coordinator.rs` (traversal CTE, compile_edge_filter), `builder.rs`, Python+TS | 2 |
| E | Item 3 (`JsonPathFusedIn` + `JsonPathIn`) | `ast.rs`, `coordinator.rs` (fused builders), `builder.rs`, `fusion.rs`, Python+TS | 2 |

Phase 1 packs (A, B, C) are independent and run in parallel.  
Phase 2 packs (D, E) are independent and run in parallel.  
Phase 2 waits for Phase 1 to fully merge before launch.

---

## Phase 1

### Pack A: `filter_json_fused_bool_eq`

**Design doc:** `dev/notes/design-0.5.1-fused-filter-completeness.md` (Item 4 section)  
**Branch:** `feat/0.5.1-fused-bool-eq`

**MODIFY:**
- `crates/fathomdb-query/src/ast.rs` — add `JsonPathFusedBoolEq` variant
- `crates/fathomdb-query/src/fusion.rs` — add `JsonPathFusedBoolEq` to `is_fusable`
- `crates/fathomdb-query/src/builder.rs` — add `filter_json_fused_bool_eq_unchecked`
- `crates/fathomdb-engine/src/coordinator.rs` — add match arm in all 3 fused compile sites
- `crates/fathomdb/tests/text_search_surface.rs` OR new test file — add fused bool eq integration test
- Python binding (`python/fathomdb/`) — add `filter_json_fused_bool_eq`
- TypeScript (`typescript/packages/fathomdb/src/`) — add `filterJsonFusedBoolEq`

**DO NOT TOUCH:** traversal CTE, `compile_expansion_filter` IN logic, `JsonPathIn`.

**Target test:** `cargo nextest run -p fathomdb-query && cargo nextest run -p fathomdb text_search`

**TDD approach:**  
1. Add `JsonPathFusedBoolEq` to `Predicate` enum (fails compilation).  
2. Add `is_fusable` arm (restores compilation).  
3. Write failing integration test: search with `filter_json_fused_bool_eq("$.resolved", false)`.  
4. Add fused clause builder arms at all 3 compile sites.  
5. Add builder method.  
6. Confirm test passes.  
7. Python binding test. TypeScript binding test.

---

### Pack B: `matched_paths` chunk hits

**Design doc:** `dev/notes/design-0.5.1-matched-paths-chunk.md`  
**Branch:** `feat/0.5.1-matched-paths-chunk`

**MODIFY:**
- `crates/fathomdb-engine/src/coordinator.rs` — wiring fix + unit test update (~line 4950)
- `crates/fathomdb/tests/text_search_surface.rs` — update chunk hit assertions

**DO NOT TOUCH:** property FTS attribution path, vector hit path, any non-chunk attribution.

**Target test:** `cargo nextest run -p fathomdb text_search`

**TDD approach:**  
1. Change `coordinator.rs:4950` unit test to assert `vec!["text_content"]` → red.  
2. Find chunk hit attribution construction, set `matched_paths: vec!["text_content".to_owned()]` → green.  
3. Run `text_search_surface.rs`, update any remaining `is_empty()` chunk assertions.

---

### Pack C: `StellaEmbedder` baseUrl

**Design doc:** `dev/notes/design-0.5.1-stella-baseurl.md`  
**Branch:** `feat/0.5.1-stella-baseurl`

**MODIFY:**
- `typescript/packages/fathomdb/src/embedders/stella.ts` — throw if no baseUrl
- TypeScript test file for embedders (or add to existing)

**DO NOT TOUCH:** other embedder classes, engine, query builders.

**Target test:** TypeScript test suite (`cd typescript/packages/fathomdb && npm test`)

---

## Phase 1 merge and gate

After all three Phase 1 packs merge:
```bash
./scripts/preflight.sh
cargo nextest run --workspace 2>&1 | tail -15
```
All tests must pass before Phase 2 launch.

---

## Phase 2

### Pack D: Edge property filter + EdgeRow (Items 1–2)

**Design doc:** `dev/notes/design-0.5.1-edge-property-filter.md`  
**Branch:** `feat/0.5.1-edge-property`  
**This is the most complex pack — brief carefully.**

**MODIFY:**
- `crates/fathomdb-query/src/ast.rs` — add `EdgePropertyEq`, `EdgePropertyCompare` to `Predicate`; add `edge_filter: Option<Predicate>` to `ExpansionSlot`
- `crates/fathomdb-engine/src/coordinator.rs` — add `compile_edge_filter`; update traversal CTE to carry `e.properties`; inject edge filter in JOIN; add column to `numbered` CTE; update row mapper; add `edge_properties: Option<String>` to all existing `NodeRow` construction sites (set `None`)
- `crates/fathomdb-engine/src/lib.rs` — re-export `NodeRow` if `edge_properties` field is visible
- `crates/fathomdb-query/src/builder.rs` — add `edge_filter: Option<Predicate>` param to `expand()`; update all `expand()` call sites in builder tests
- `crates/fathomdb/tests/grouped_query_reads.rs` — update all `expand()` call sites to pass `None` for `edge_filter`; add edge property filter tests
- New test (or in `grouped_query_reads.rs`) — round-trip test (write edge with properties, traverse, assert `edge_properties` on hit)
- Python binding — add `edge_properties` field on hit type, `edge_filter` kwarg on `expand()`
- TypeScript binding — add `edgeProperties` field, `edgeFilter?: Predicate` on `expand()`
- `crates/fathomdb/src/python_types.rs` — update `PyExpansionRootRows` / Python hit type

**DO NOT TOUCH:** fused clause builders in search CTE path, `JsonPathFusedIn`, `JsonPathIn`, fusion classification for non-edge predicates.

**Target test:** `cargo nextest run -p fathomdb grouped_query && cargo nextest run -p fathomdb-engine`

**TDD approach:**  
1. Add `EdgePropertyEq`, `EdgePropertyCompare` variants → fails compilation on exhaustive matches.  
2. Add `edge_filter` to `ExpansionSlot`, add `edge_properties` to `NodeRow` → update all construction sites.  
3. Write failing test: expand with `edge_filter = Some(EdgePropertyEq(...))`, expect 1 result.  
4. Add `compile_edge_filter` function.  
5. Update traversal CTE SQL to select `e.properties`, inject `{edge_filter_sql}` in JOIN.  
6. Update numbered CTE to carry `edge_properties`. Update row mapper.  
7. Confirm test passes.  
8. Write round-trip test (Item 2). Confirm passes.  
9. Python + TS bindings.

**Known risks:**
- Recursive CTE column count must match base case + recursive case. Test multi-hop explicitly.
- Bind parameter indexes shift when edge filter params precede node filter params. Use a single running counter.

---

### Pack E: `filter_json_fused_text_in` + `filter_json_text_in` (Item 3)

**Design doc:** `dev/notes/design-0.5.1-fused-filter-completeness.md` (Item 3 section)  
**Branch:** `feat/0.5.1-fused-text-in`

**MODIFY:**
- `crates/fathomdb-query/src/ast.rs` — add `JsonPathFusedIn`, `JsonPathIn` variants
- `crates/fathomdb-query/src/fusion.rs` — `JsonPathFusedIn` → `is_fusable` true; `JsonPathIn` → false
- `crates/fathomdb-query/src/builder.rs` — add `filter_json_fused_text_in_unchecked`, `filter_json_text_in`
- `crates/fathomdb-engine/src/coordinator.rs` — add match arms in all 3 fused compile sites for `JsonPathFusedIn`; add `JsonPathIn` to residual/node WHERE path
- `crates/fathomdb-query/src/compile.rs` — if `JsonPathIn` is compiled in the query planner, add arm there
- Test: `crates/fathomdb/tests/text_search_surface.rs` or new file — integration tests for both variants
- Python binding — `filter_json_fused_text_in`, `filter_json_text_in`
- TypeScript binding — `filterJsonFusedTextIn`, `filterJsonTextIn`

**DO NOT TOUCH:** traversal CTE, `EdgePropertyEq`/`EdgePropertyCompare`, edge property logic.

**Target test:** `cargo nextest run -p fathomdb text_search && cargo nextest run -p fathomdb-query`

**TDD approach:**  
1. Add `JsonPathFusedIn`, `JsonPathIn` variants.  
2. Add `is_fusable` arms.  
3. Write failing test: search with `filter_json_fused_text_in(["cites","links"])`.  
4. Add `JsonPathFusedIn` arms at all 3 fused compile sites (IN placeholder generation).  
5. Add builder methods with empty-values guard.  
6. Confirm fused test passes.  
7. Write failing test for `filter_json_text_in` (unfused, no FTS schema).  
8. Add `JsonPathIn` to residual/flat WHERE path.  
9. Confirm unfused test passes.  
10. Python + TS parity.

**File overlap with Pack D:** Both touch `ast.rs` and `coordinator.rs`. Conflict is
trivial (adding different match arms). Merge Pack D first if possible; if running
truly parallel, the orchestrator resolves the conflict after both complete.

---

## Phase 2 merge and gate

After both Phase 2 packs merge:
```bash
./scripts/preflight.sh
cargo nextest run --workspace 2>&1 | tail -15
```

All tests pass → run full regression:
```bash
cargo nextest run --workspace --no-fail-fast 2>&1 | tail -30
```

---

## Version bump and release (Phase 3, serial)

After all 5 packs merged and workspace tests pass:

1. Bump version to `0.5.1` in all `Cargo.toml` files and `package.json`.
2. Update `CHANGELOG.md` with 0.5.1 entry.
3. Tag `v0.5.1`.

---

## Cross-pack file overlap table

| File | Packs touching it |
|---|---|
| `crates/fathomdb-query/src/ast.rs` | A (new variant), D (new variants + ExpansionSlot field), E (new variants) |
| `crates/fathomdb-engine/src/coordinator.rs` | A (fused builders), B (attribution), D (traversal CTE, compile_edge_filter), E (fused builders) |
| `crates/fathomdb-query/src/builder.rs` | A (new method), D (expand signature), E (new methods) |
| `crates/fathomdb-query/src/fusion.rs` | A (is_fusable arm), E (is_fusable arm) |
| `crates/fathomdb/tests/text_search_surface.rs` | B (assertion updates), A (new test), E (new test) |
| `crates/fathomdb/tests/grouped_query_reads.rs` | D (expand call sites + new tests) |
| `typescript/.../embedders/stella.ts` | C only |

Phase 1 packs (A, B, C) are independent — no overlap within Phase 1.  
Phase 2 packs (D, E) overlap in `ast.rs` and `coordinator.rs` but at different sections.
