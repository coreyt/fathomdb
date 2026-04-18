# Design: matched_paths for Chunk Hits (0.5.1 Item 5)

**Release:** 0.5.1  
**Scope item:** Item 5 from `dev/notes/0.5.1-scope.md`  
**Breaking:** No (additive — changes empty vec to populated vec)

---

## Problem

Chunk hits return `attribution.matched_paths = []` unconditionally.
The correct behavior per spec is `matched_paths == ["text_content"]`.
The data exists; this is a wiring gap with an explicit codebase placeholder.

---

## Current state (anchored to HEAD)

`crates/fathomdb-engine/src/coordinator.rs:4950` (unit test):

```rust
// Current placeholder behavior: chunk hits carry present-but-empty
// matched_paths.  The target behavior (per C-1 spec) is
// matched_paths == ["text_content"].  Blocked on integration test
// update in text_search_surface.rs.
assert!(att.matched_paths.is_empty(), ...);
```

`crates/fathomdb/tests/text_search_surface.rs:1463`:

```rust
assert!(
    att.matched_paths.is_empty(),
    att.matched_paths,
);
```

Search for all `matched_paths.is_empty()` assertions in `text_search_surface.rs` —
each one that fires on a chunk hit must be updated to assert `vec!["text_content"]`.

Property FTS hit path already populates `matched_paths` correctly (lines 3156–3164,
using `property_fts_hit_matched_paths_from_positions`). The chunk path does not.

---

## Design

### Wiring change (`coordinator.rs`)

Find the site that constructs `HitAttribution` for chunk hits and sets
`matched_paths: Vec::new()`. Change to `matched_paths: vec!["text_content".to_owned()]`.

The path is: search result processing → chunk hit branch → attribution construction.
Look for `attribution` struct construction with `matched_paths: Vec::new()` in a chunk
context. The coordinator.rs line ~4950 comment is inside a unit test; the production
wiring is nearby in the `resolve_hit_attribution` call chain (~line 3156).

**Do NOT change:** property FTS hit path (already correct), vector hit path, or any
other hit type. Only chunk hit attribution construction.

### Test updates

1. `coordinator.rs:4950` unit test: change `assert!(att.matched_paths.is_empty())`
   → `assert_eq!(att.matched_paths, vec!["text_content"])`.

2. `text_search_surface.rs`: grep for `matched_paths.is_empty()`. For each assertion
   on a chunk hit result: change to `assert_eq!(att.matched_paths, vec!["text_content"])`.
   For assertions on property FTS or vector hits: leave unchanged.

3. Verify no other test files assert empty `matched_paths` for chunk hits.

---

## Implementation approach

This is a small wiring fix. TDD approach:

1. **Red:** Change the unit test at `coordinator.rs:4950` to assert `vec!["text_content"]`.
   Run test, confirm it fails.
2. **Green:** Find the chunk hit attribution construction and set `matched_paths:
   vec!["text_content".to_owned()]`. Run test, confirm it passes.
3. **Update integration tests:** Update `text_search_surface.rs` assertions from empty
   to `vec!["text_content"]` for chunk hit cases. Run full suite.

---

## Acceptance criteria

1. Unit test at `coordinator.rs:4950` asserts `matched_paths == ["text_content"]`.
2. All chunk hit attribution in integration tests has `matched_paths == ["text_content"]`.
3. Property FTS hit `matched_paths` still returns the correct leaf path (not changed).
4. `cargo nextest run -p fathomdb text_search` passes.
