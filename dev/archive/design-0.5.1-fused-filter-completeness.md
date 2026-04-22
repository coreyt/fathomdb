# Design: Fused Filter Completeness (0.5.1 Items 3–4)

**Release:** 0.5.1  
**Scope items:** 3 (`filter_json_fused_text_in`) and 4 (`filter_json_fused_bool_eq`)  
**Breaking:** No (additive new predicates, new builder methods)

---

## Problem

**Item 3:** `filter_json_fused_text_eq` accepts a single value. Filtering on a set
(e.g. 10 relationship strings) requires N queries or over-fetching. `filter_json_fused_text_in`
eliminates this. Also needed: unfused `JsonPathIn` for the Nodes driver (no FTS schema,
required for Cypher `WHERE n.prop IN [...]`).

**Item 4:** `filter_json_bool_eq` exists as a post-filter but no fused variant exists.
Needed for `resolved=false` patterns and Cypher `WHERE n.active = false`.

---

## Current state (anchored to HEAD)

| Location | What exists |
|---|---|
| `crates/fathomdb-query/src/ast.rs:118-135` | `JsonPathFusedEq`, `JsonPathFusedTimestampCmp` — text and timestamp fused variants; no bool, no IN |
| `crates/fathomdb-query/src/builder.rs:183` | `filter_json_bool_eq` — unfused bool eq |
| `crates/fathomdb-query/src/builder.rs:279` | `filter_json_fused_text_eq_unchecked` — fused text eq, single value |
| `crates/fathomdb-engine/src/coordinator.rs:919-927` | `JsonPathFusedEq` in vector search fused clause builder |
| `crates/fathomdb-engine/src/coordinator.rs:1614-1622` | `JsonPathFusedEq` in text search fused clause builder |
| `crates/fathomdb-engine/src/coordinator.rs:86-92` | `JsonPathFusedEq` in `compile_expansion_filter` |

All three compile sites need new arms for Items 3 and 4.

---

## Item 4: `filter_json_fused_bool_eq` (simpler — implement first)

### New AST variant (`ast.rs`)

```rust
/// Fused equality check on a JSON boolean property.
/// See [`Predicate::JsonPathFusedEq`] for the fusion contract.
JsonPathFusedBoolEq {
    path: String,
    value: bool,
}
```

### SQL generation (all three compile sites)

```sql
AND json_extract(src.properties, ?{path_idx}) = ?{value_idx}
```

`value` binds as `Value::Integer(i64::from(value))` (1 for true, 0 for false) —
consistent with how `filter_json_bool_eq` already binds in the post-filter path.

### Builder method (`builder.rs`)

```rust
pub fn filter_json_fused_bool_eq_unchecked(
    mut self,
    path: impl Into<String>,
    value: bool,
) -> Self {
    self.ast.steps.push(QueryStep::Filter(Predicate::JsonPathFusedBoolEq {
        path: path.into(),
        value,
    }));
    self
}
```

Same `_unchecked` suffix and fusion-gate contract as `filter_json_fused_text_eq_unchecked`.

### Fusion classification

`fusion::is_fusable` must return `true` for `JsonPathFusedBoolEq` so it gets pushed
into the search CTE's inner WHERE. Check `crates/fathomdb-query/src/fusion.rs` for
the is_fusable function and add the new variant.

### Python binding

`filter_json_fused_bool_eq(path: str, value: bool)` on `SearchBuilder` and
`TextSearchBuilder`. Maps to `_unchecked` variant after schema check on the Python side.

### TypeScript binding

`filterJsonFusedBoolEq(path: string, value: boolean)` — same semantics.

---

## Item 3: `filter_json_fused_text_in`

Two AST variants are required:
- **Fused:** `JsonPathFusedIn` — pushes into search CTE inner WHERE (requires FTS schema gate)
- **Unfused:** `JsonPathIn` — residual WHERE clause (no FTS schema required, used by Nodes driver)

### New AST variants (`ast.rs`)

```rust
/// Fused IN check on a JSON text property at the given path.
/// `values` must be non-empty; `BuilderValidationError` if empty.
/// See [`Predicate::JsonPathFusedEq`] for the fusion contract.
JsonPathFusedIn {
    path: String,
    values: Vec<String>,
}

/// Unfused IN check on a JSON property at the given path.
/// Accepts Text, Integer, or Bool values; mixed-type list is a compile error.
/// Applied as residual WHERE on the Nodes driving table scan.
JsonPathIn {
    path: String,
    values: Vec<ScalarValue>,
}
```

### SQL generation

Both generate `AND json_extract(src.properties, ?{path_idx}) IN (?, ?, ...)`.

Bind order: path param first (1 bind), then each value (N binds).

```rust
// Example for 3 values starting at param P:
// AND json_extract(src.properties, ?P) IN (?P+1, ?P+2, ?P+3)
let path_idx = first_param;
let value_placeholders: String = (1..=values.len())
    .map(|i| format!("?{}", first_param + i))
    .collect::<Vec<_>>()
    .join(", ");
format!("\n                  AND json_extract(src.properties, ?{path_idx}) IN ({value_placeholders})")
```

Bind values: `[Value::Text(path), Value::Text(v1), Value::Text(v2), ...]`

For `JsonPathIn` with `ScalarValue` values: Text → `Value::Text`, Integer → `Value::Integer`,
Bool → `Value::Integer(i64::from(b))`. Mixed-type rejected at `QueryBuilder::filter_json_text_in`
call time (check all values have same variant type).

### Compile sites

**`JsonPathFusedIn`** — all three fused sites:
1. `coordinator.rs` vector search fused clause builder (~line 919)
2. `coordinator.rs` text search fused clause builder (~line 1614)
3. `compile_expansion_filter` (~line 86) — node alias (`n.properties`)

**`JsonPathIn`** — unfused (residual) path only. Does NOT go in the search CTE inner WHERE.
Applied in the same phase as `JsonPathEq` / `JsonPathCompare`. Compile sites:
- Wherever `JsonPathEq` is compiled for flat node scans (Nodes driver base_candidates WHERE)
- `compile_expansion_filter` for expansion target filtering

### Empty `values` guard

At builder method call time: if `values.is_empty()`, return `Err(BuilderValidationError)`.
`SQLite's `IN ()` is a syntax error; an empty list is always a caller bug.

### Builder methods (`builder.rs`)

```rust
// Fused IN (requires FTS schema gate — caller responsibility, _unchecked suffix)
pub fn filter_json_fused_text_in_unchecked(
    mut self,
    path: impl Into<String>,
    values: impl IntoIterator<Item = impl Into<String>>,
) -> Self { ... }

// Unfused IN (no FTS gate)
pub fn filter_json_text_in(
    mut self,
    path: impl Into<String>,
    values: impl IntoIterator<Item = impl Into<String>>,
) -> Self { ... }
```

`filter_json_text_in` uses `ScalarValue::Text` (Text-only variant of `JsonPathIn`).
Accepts `&str` values. No `_unchecked` suffix since there is no fusion gate to bypass.

### Fusion classification

`fusion::is_fusable`: `JsonPathFusedIn` → true, `JsonPathIn` → false.

### Python binding

```python
# SearchBuilder / TextSearchBuilder
def filter_json_fused_text_in(self, path: str, values: list[str]) -> Self: ...
# QueryBuilder
def filter_json_text_in(self, path: str, values: list[str]) -> Self: ...
```

### TypeScript binding

```typescript
filterJsonFusedTextIn(path: string, values: string[]): this
filterJsonTextIn(path: string, values: string[]): this
```

---

## File overlap with Item 1 (edge property filter)

Items 3–4 and Item 1 both modify:
- `crates/fathomdb-query/src/ast.rs` — different new enum variants, trivial merge conflict
- `crates/fathomdb-engine/src/coordinator.rs` — different sections: Items 3-4 touch
  fused clause builders; Item 1 touches `compile_expansion_filter` and traversal CTE

Parallel worktree implementation is safe. Merge conflicts will be trivial match-arm additions.

---

## Acceptance criteria

### Item 4
1. `JsonPathFusedBoolEq` variant exists in `Predicate` enum.
2. `filter_json_fused_bool_eq_unchecked(path, true)` pushes into search CTE inner WHERE.
3. Search with `filter_json_fused_bool_eq("$.resolved", false)` returns only nodes with `resolved=false`.
4. Python `filter_json_fused_bool_eq("$.resolved", False)` → same result.
5. TypeScript `filterJsonFusedBoolEq("$.resolved", false)` → same result.
6. `fusion::is_fusable` returns true for `JsonPathFusedBoolEq`.

### Item 3
1. `JsonPathFusedIn` and `JsonPathIn` variants exist.
2. `filter_json_fused_text_in_unchecked("$.rel", ["cites","links"])` pushes IN into search CTE.
3. Empty values → `BuilderValidationError` at call time.
4. `filter_json_text_in("$.status", ["open","pending"])` applies as residual WHERE; works without FTS schema.
5. IN with 1 value behaves identically to eq for both variants.
6. Python / TypeScript parity tests pass.

---

## Out of scope

- `filter_json_fused_integer_in` (no demand signal yet)
- `filter_json_fused_in` on `QueryBuilder` node listing (low priority — flat scan)
- Mixed-type IN lists (rejected at builder time)
