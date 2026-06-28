# Slice 35 design memo — G4 filter grammar (`read.list` + `Predicate`)

**Status:** authoritative (written first, before any code). Slice 35 of 0.8.1.
**ADR gate:** `ADR-0.8.0-filter-grammar.md` (HITL-signed 2026-06-06) is the binding spec.
**Context:** Slice 15 is CLOSED on `main`; `canonical_nodes(kind)` index (`canonical_nodes_kind_idx`,
step-12 delta) is already present. No migration needed.

---

## 1. Compiled SQL for `read_list`

```sql
SELECT logical_id, kind, body, write_cursor
FROM   canonical_nodes
WHERE  kind = ?1
  AND  superseded_at IS NULL
  [AND json_extract(body, '$.field') <op> ?N ...]
LIMIT  ?L
```

- `kind` is always `?1` (bound parameter).
- `superseded_at IS NULL` is the active-row predicate (G0 substrate).
- Each predicate in `&[Predicate]` adds one `AND json_extract(body, '$.path') <op> ?N` clause.
- `json_extract(body, '$.path')` uses the **allowlist constant string**, never the caller-supplied
  string. Path validation at constructor time ensures only allowlist members reach the SQL.
- `<op>` is a literal from the closed `ComparisonOp` enum (`=`, `>`, `>=`, `<`, `<=`).
  Eq maps to `=` via `json_extract` equality. Not a parameter — a server-side constant from
  a closed enum.
- Each value is a bound parameter `?N` (rusqlite `ToSql`). Never interpolated.
- `LIMIT ?L` is the `limit` argument.

**Parameter numbering:** rusqlite uses 1-based positional `?N`. `?1` = kind. Each predicate
adds one more bound parameter. `?L` = limit is appended last as the LIMIT value. In practice
the limit is expressed in the SQL as a literal at build-time for simplicity since we control
the SQL string construction. The value bindings array uses `kind` first then each predicate
value in order.

**EXPLAIN gate:** `EXPLAIN QUERY PLAN` on the unfiltered `read_list(kind, [], limit)` must
show `SEARCH canonical_nodes USING INDEX canonical_nodes_kind_idx (kind=?)` (no full SCAN).
This is confirmed by the existing G0 substrate (step-12 folded the kind index).

---

## 2. Path allowlist — initial set and validation

```rust
const PREDICATE_PATH_ALLOWLIST: &[&str] = &[
    "$.status",
    "$.priority",
    "$.tags",
    "$.kind",
    "$.created_at",
];
```

**Validation logic:** The `Predicate::json_path_eq` and `Predicate::json_path_compare`
constructors check whether the caller-supplied `path` exactly matches one of the allowlist
entries (`PREDICATE_PATH_ALLOWLIST.contains(&path.as_str())`). If no match, return
`Err(EngineError::InvalidFilter { reason: "path not in allowlist" })`. The SQL then uses
**the allowlist constant** (the entry itself, not the caller string) in
`json_extract(body, '<allowlist-entry>')`.

**Extending the allowlist:** Add a new entry to `PREDICATE_PATH_ALLOWLIST`. No API change
needed — the enum shape is fixed; the allowlist is engine-internal state. Callers upgrading
to a version with a wider allowlist simply pass the new path string, and the constructor
accepts it. Document the allowlist in the API reference.

---

## 3. AND-composition of predicates

`read_list(kind, predicates, limit)` with `predicates = [p1, p2, ..., pN]` generates:

```sql
WHERE kind = ?1
  AND superseded_at IS NULL
  AND json_extract(body, '$.f1') = ?2
  AND json_extract(body, '$.f2') > ?3
  ...
  AND json_extract(body, '$.fN') <op> ?N+1
LIMIT <limit>
```

Each predicate appends one clause with a fresh `?` parameter. The values are bound in
predicate order. This is implicit AND: all predicates must hold simultaneously.

**Empty predicate slice** (`predicates = &[]`): no additional WHERE clauses — returns all
active nodes of the given kind (up to limit). This is the unfiltered list path.

---

## 4. ScalarValue / ComparisonOp as shared vocabulary

`ScalarValue` and `ComparisonOp` are defined ONCE in `fathomdb-engine::lib.rs` and are
`pub` exports. They are NOT duplicated in the PyO3 or NAPI bindings — the bindings marshal
to/from the Rust types. The name exports from `fathomdb-engine` at the crate root, so a
future G10 adoption (reserved-gap 37) can import them without requiring a path change:

```rust
// future G10 unification (reserved-gap 37) would import:
use fathomdb_engine::{ScalarValue, ComparisonOp};
```

The `SearchFilter` struct is **NOT modified** (D-F3 forbids it in this slice). A test
asserts `SearchFilter` has exactly the four existing fields (`source_type`, `kind`,
`created_after`, `status`).

---

## 5. Test strategy

### Rust (fathomdb-engine tests/slice35_filter_grammar.rs)

| Test | What it pins |
|------|-------------|
| `predicate_enum_is_exactly_jsoneq_and_jsoncompare` | Exhaustiveness via match — fails to compile if a variant is added |
| `allowlisted_path_accepted` | `$.status` → `Ok(Predicate::JsonPathEq {...})` |
| `non_allowlisted_path_rejected` | `$.private_field` → `Err(EngineError::InvalidFilter)` |
| `injection_safe_value_is_bound_not_interpolated` | SQL-injection-shaped value → `?` in SQL (EXPLAIN query plan reveals no literal; query runs without error) |
| `fused_and_unchecked_absent_from_surface` | Structural assertion: no `Fused*` / `*_unchecked` in the public surface (compile-time via match exhaustion + naming) |
| `read_list_returns_active_nodes_by_kind` | Kind-filter: kind-A nodes only; kind-B excluded |
| `read_list_filter_eq_matches` | `$.status = "open"` returns matching nodes; excludes "closed" |
| `read_list_filter_gt_matches` | `$.priority > 3` returns nodes with priority 4,5 etc |
| `read_list_and_composition` | Two predicates AND: `$.status = "open"` AND `$.priority > 3` |
| `read_list_empty_filter_returns_all` | No predicates → all active nodes of kind |
| `searchfilter_struct_shape_unchanged` | `SearchFilter` fields = exactly `{source_type, kind, created_after, status}` |
| `scalar_value_and_comparison_op_are_shared_types` | `ScalarValue`/`ComparisonOp` accessible at `fathomdb_engine::ScalarValue` |

### Python (src/python/tests/test_read_list.py)

- `test_read_list_filter_py` — seed nodes, call `read.list`, assert results
- `test_read_list_non_allowlisted_path_raises` — verify typed error on bad path

### TypeScript (src/ts/tests/functional-read-list.test.ts)

- Same fixtures as Python; `read.list` returns equivalent results

### Cross-binding equivalence

Same DB + same predicates → `read.list` (Python) ≡ `read.list` (TS) — confirmed by seeding
identical data and comparing results.

### Injection-safety test modeling

Modeled on `fts5_injection_safety.rs` + `dev/design/agent-memory-impl-strategy.md :414`:

- Construct `Predicate::json_path_eq("$.status", ScalarValue::Text("'; DROP TABLE canonical_nodes;--"))`.
- Call `engine.read_list("test", &[pred], 100)`.
- Assert no error → the injection string was bound as a parameter, never executed as SQL.
- The `canonical_nodes` table still exists (no drop happened).

---

## 6. Python / TS API shapes

### Python

```python
# read.py addition
def list(
    engine: "Engine",
    kind: str,
    predicates: list[Predicate] | None = None,
    *,
    limit: int = 100,
) -> list[NodeRecord]: ...

# new type in types.py / _fathomdb.pyi
@dataclass
class Predicate:
    type: Literal["eq", "gt", "gte", "lt", "lte"]
    path: str
    value: str | int | bool
```

The Python-side `Predicate` is a simple dataclass converted to the Rust `RustPredicate` in
the PyO3 binding. Path validation happens in Rust (at `Predicate` construction time); Python
receives an `InvalidFilterError` (a new leaf of `EngineError`) on a non-allowlisted path.

### TypeScript

```typescript
export interface Predicate {
  type: 'eq' | 'gt' | 'gte' | 'lt' | 'lte'
  path: string
  value: string | number | boolean
}
export declare function readList(
  engine: Engine,
  kind: string,
  predicates?: Array<Predicate>,
  limit?: number
): Promise<Array<NodeRecord>>
```

`read.list(engine, kind, predicates?, limit?)` in the TS SDK calls `native.readList(...)`.

---

## 7. Why the deferred ADRs are framing-only

`ADR-0.8.1-deferred-f9-confidence-importance.md` and
`ADR-0.8.1-deferred-f5-fielded-fts-bm25f.md` are written in this slice because:

- The design decision points for F9 and F5 require **consumer signal and eval signal** that
  does not yet exist (F9 needs confidence data from Slice 15 BYO-LLM ingest at scale; F5
  needs the R0/R2 CDF/eval results from Slices 5 and 25).
- Writing the framing ADR now captures the decision model — what we know, what we're waiting
  for, and what the implementation would look like once the signal arrives — so the 0.8.2+
  slice can proceed without re-litigating the design.
- Neither ADR requires or implies any code change in Slice 35. Status is `DEFERRED — 0.8.2+`.
- No HITL sign-off is needed in this slice (the ADRs are framing, not an actionable contract).

---

## 8. EngineError extension

Add `InvalidFilter { reason: String }` to the `EngineError` enum for non-allowlisted path
rejections and other filter construction errors. This is a **new leaf** — it does not conflict
with existing error handling. The binding-facing code maps it to a new
`InvalidFilterError(EngineError)` Python class and a new TypeScript `FathomDbInvalidFilterError`.
