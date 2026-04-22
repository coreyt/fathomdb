# Pathway to Basic Cypher Support

Date: 2026-04-17  
Supersedes: `dev/archive/pathway-to-basic-cypher-2026-03-28.md` (still valid for strategic framing; this
document extends it with a concrete implementation plan, full conformance matrix, and sequencing
relative to 0.5.1 and 0.6.0).

---

## Strategic framing (unchanged from 2026-03-28)

Add Cypher as a **compatibility translation layer** over the existing FathomDB query engine.
The canonical execution model remains `QueryAst → SQL`. Cypher is a frontend that compiles to the
same internal IR — not a second query engine and not Bolt.

The fastest market-validation path is `db.cypher(query, params)` in Python, with a precisely
documented subset. Move the parser and translator into Rust once the subset stabilizes, so all
SDKs share the implementation.

---

## What has changed since 2026-03-28

1. **`fathomdb-query` is now the established IR crate.** `ast.rs` and `compile.rs` are mature.
   The Cypher translator's target is `QueryAst` — no new IR design needed for the v1 subset.
2. **Edge property support shipped in 0.5.1; `EdgeRow` shipped in 0.5.3.** Items 1–4 in
   `dev/notes/0.5.1-scope.md` shipped `WHERE r.prop = $v` / `WHERE n.prop IN [...]` support,
   and 0.5.3 shipped the first-class `EdgeRow` + `.traverse_edges()` surface that backs
   `RETURN r` / `RETURN r.prop`. No Cypher surface needs to stub with
   `UnsupportedCypherFeature` on these constructs anymore; the translator wires them
   directly against the 0.5.1 and 0.5.3 engine surfaces.
3. **The conformance matrix is now concrete.** The 2026-03-28 doc described a single-hop example.
   This document defines the exact v1 subset with per-construct mapping to `QueryAst`.

---

## v1 conformance matrix

### Supported

```cypher
-- Node lookup (no traversal)
MATCH (n:Kind) RETURN n LIMIT 10
MATCH (n:Kind) WHERE n.logical_id = $id RETURN n
MATCH (n:Kind) WHERE n.prop = $val RETURN n
MATCH (n:Kind) WHERE n.prop > $val RETURN n          -- also >=, <, <=

-- Single-hop traversal
MATCH (a:K1)-[:TYPE]->(b) WHERE a.logical_id = $id RETURN b
MATCH (a:K1)-[:TYPE]->(b:K2) WHERE a.logical_id = $id RETURN b
MATCH (a)-[:TYPE*1..N]->(b) WHERE a.logical_id = $id RETURN b

-- WHERE with AND
MATCH (n:K) WHERE n.p1 = $v1 AND n.p2 > $v2 RETURN n

-- Inline node kind on either end
MATCH (a:K1)-[:TYPE]->(b:K2) WHERE a.logical_id = $id RETURN b

-- LIMIT
MATCH (n:Kind) RETURN n LIMIT 25

-- Parameters ($name substituted from caller-supplied dict)
MATCH (n:Kind) WHERE n.logical_id = $id RETURN n
```

### Supported (unblocked on prior 0.5.x releases)

```cypher
-- Edge property filter (shipped in 0.5.1)
MATCH (a)-[r:TYPE]->(b) WHERE r.weight > $threshold RETURN b
MATCH (a)-[r:TYPE]->(b) WHERE r.active = true RETURN b
MATCH (a)-[r:TYPE]->(b) WHERE r.status = $s RETURN b, r

-- Return edge (unblocked in 0.5.3 — backed by first-class EdgeRow + .traverse_edges())
MATCH (a)-[r:TYPE]->(b) RETURN b, r
MATCH (a)-[r:TYPE]->(b) RETURN r.weight

-- Set membership (shipped in 0.5.1 — JsonPathIn + unfused variant)
MATCH (n:K) WHERE n.status IN ['active', 'pending'] RETURN n

-- Boolean property filter (shipped in 0.5.1 — JsonPathFusedBoolEq)
MATCH (n:K) WHERE n.resolved = false RETURN n
```

### Not in v1 (deferred — raise `UnsupportedCypherFeature`)

| Construct | Reason deferred |
|---|---|
| `OPTIONAL MATCH` | No equivalent in QueryAst; design not settled |
| `WITH` | Intermediate piping requires multi-stage execution |
| Aggregation (`count`, `sum`, `collect`) | No aggregation in QueryAst |
| `UNWIND` | No list expansion in QueryAst |
| Write operations (`CREATE`, `MERGE`, `DELETE`, `SET`) | Separate write path; out of scope |
| Multi-hop different edge types `(a)-[:T1]->(b)-[:T2]->(c)` | Single-traversal AST constraint |
| `OR` predicates in WHERE | No `Predicate::Or` in AST |
| `NOT` predicates | No `Predicate::Not` in AST |
| Undirected edges `(a)-->(b)` / `(a)--(b)` | No "either" direction in TraverseDirection |
| String predicates (`STARTS WITH`, `ENDS WITH`, `CONTAINS`) | No substring predicate in AST |
| `IS NULL` / `IS NOT NULL` on arbitrary properties | Only `ContentRefNotNull` exists; not general |
| Path variables `p = (a)-[*]->(b)` | No path collection in result shape |
| `RETURN n.prop AS alias` projection | NodeRow returns full JSON blob; property extraction deferred |
| `ORDER BY` multi-variable | No ORDER BY in QueryAst |
| Bolt protocol | Expensive; no demand signal yet |
| `CALL fathom.*` procedures | Vector/FTS extensions deferred to v2 |

---

## Implementation plan

### Module location

All Cypher code lives in `crates/fathomdb-query/src/cypher/`. No new crate — the query crate is
the right boundary. The module is gated behind a `cypher` feature flag in `Cargo.toml` so it
compiles out of Python/TS builds that do not need it. (Feature flag also lets us ship without
Cypher support visible in doc builds until the conformance matrix is stable.)

```
crates/fathomdb-query/src/cypher/
  mod.rs         -- public re-exports: parse, translate, CypherError
  ast.rs         -- CypherQuery, MatchPattern, NodePattern, EdgePattern, WhereExpr, ReturnClause
  parser.rs      -- hand-rolled recursive descent, no combinator library
  translate.rs   -- CypherQuery + params → QueryAst
  error.rs       -- ParseError, TranslateError, CypherError (wraps both)
```

### Cypher AST types (`cypher/ast.rs`)

```rust
pub struct CypherQuery {
    pub match_clause: MatchPattern,
    pub where_clause: Option<WhereExpr>,
    pub return_clause: ReturnClause,
    pub limit: Option<usize>,
}

pub struct MatchPattern {
    pub root: NodePattern,
    pub hops: Vec<HopPattern>,  // (edge, node) pairs
}

pub struct NodePattern {
    pub variable: Option<String>,   // 'n' in (n:Kind)
    pub kind: Option<String>,       // ':Kind'
}

pub struct HopPattern {
    pub edge: EdgePattern,
    pub node: NodePattern,
}

pub struct EdgePattern {
    pub variable: Option<String>,
    pub label: Option<String>,
    pub direction: EdgeDirection,
    pub min_hops: usize,            // default 1
    pub max_hops: usize,            // default 1; *N..M sets both
}

pub enum EdgeDirection { Out, In }  // 'Either' deferred

pub enum WhereExpr {
    And(Vec<WhereExpr>),            // top-level AND chain
    Predicate(WherePredicate),
}

pub enum WherePredicate {
    Eq   { var: String, prop: String, value: CypherValue },
    Cmp  { var: String, prop: String, op: CypherCmpOp, value: CypherValue },
    In   { var: String, prop: String, values: Vec<CypherValue> },
}

pub enum CypherCmpOp { Gt, Gte, Lt, Lte, Neq }

pub enum CypherValue {
    Literal(ScalarValue),
    Parameter(String),
}

pub struct ReturnClause {
    pub items: Vec<ReturnItem>,
}

pub enum ReturnItem {
    Variable(String),                                    // RETURN n
    Property { var: String, prop: String, alias: Option<String> },  // RETURN n.name AS x
    EdgeVariable(String),                                // RETURN r (backed by 0.5.3 EdgeRow)
    EdgeProperty { var: String, prop: String, alias: Option<String> }, // RETURN r.weight (backed by 0.5.3 EdgeRow.properties)
}
```

### Parser (`cypher/parser.rs`)

Hand-rolled recursive descent. Security-sensitive: any JSON path derived from Cypher input
reaches `validate_json_path` in `compile.rs`, which already enforces `$(.key)+` allowlist. The
parser only needs to produce the `CypherAst`; the compiler validates paths.

Grammar (simplified — ASCII, case-insensitive keywords):

```
query      ::= MATCH pattern (WHERE expr)? RETURN return_items (LIMIT integer)?
pattern    ::= node (edge node)*
node       ::= '(' var? (':' kind)? ')'
edge       ::= '-' '[' var? (':' label)? depth? ']' '->'
             | '<-' '[' var? (':' label)? depth? ']' '-'
depth      ::= '*' | '*' integer | '*' integer '..' integer
expr       ::= predicate (AND predicate)*
predicate  ::= expr_path '=' value
             | expr_path ('<' | '<=' | '>' | '>=' | '<>') value
             | expr_path IN '[' value (',' value)* ']'
expr_path  ::= ident '.' ident
value      ::= '$' ident | string_literal | integer | 'true' | 'false'
return_items ::= return_item (',' return_item)*
return_item  ::= ident | ident '.' ident (AS ident)?
```

Error model: `ParseError` is a `thiserror` enum with a message and character offset. No recovery
— fail-fast on first syntax error.

### Translator (`cypher/translate.rs`)

```rust
pub fn translate(
    query: &CypherQuery,
    params: &HashMap<String, ScalarValue>,
) -> Result<QueryAst, TranslateError>
```

**Root variable resolution:**
Scan `WHERE` for a predicate of the form `var.logical_id = $param` or `var.logical_id = 'literal'`
where `var` is the `match_clause.root.variable`. That predicate produces `Filter(LogicalIdEq(...))`.
If the root has no logical_id filter, it produces a full kind-scan (only feasible with a small
enough limit — emit a `TranslateWarning` if no filter restricts the root).

**`root_kind` selection:**
If the root `NodePattern` has a `kind`, that is `QueryAst.root_kind`.
If not, and if `WHERE` contains `var.kind = 'K'` on the root variable, use that value.
If neither, `TranslateError::UnboundRootKind`.

**Filter mapping (node predicates):**

| Cypher WHERE | QueryAst step |
|---|---|
| `n.logical_id = $id` | `Filter(LogicalIdEq)` |
| `n.kind = 'K'` | `Filter(KindEq)` |
| `n.prop = $v` | `Filter(JsonPathEq { path: "$.prop", value })` |
| `n.prop > $v` | `Filter(JsonPathCompare { path: "$.prop", op: Gt, value })` |
| `n.prop IN [...]` | `Filter(JsonPathIn { path: "$.prop", values })` (0.5.1+) |

**Traversal mapping:**

| Cypher edge | QueryAst step |
|---|---|
| `-[:TYPE]->` | `Traverse { direction: Out, label: "TYPE", max_depth: 1, filter: None }` |
| `<-[:TYPE]-` | `Traverse { direction: In, label: "TYPE", max_depth: 1, filter: None }` |
| `-[:TYPE*1..N]->` | `Traverse { direction: Out, label: "TYPE", max_depth: N, filter: None }` |
| terminal kind (e.g. `(b:K2)`) | `Traverse { filter: Some(KindEq("K2")) }` |

**Edge property predicates (shipped in 0.5.1):**
`WHERE r.prop = $v` where `r` is the edge variable → `Traverse { filter: Some(EdgePropertyEq { ... }) }`.
Shipped in 0.5.1; no `UnsupportedFeature` stub needed.

**Multi-hop different types:**
`(a)-[:T1]->(b)-[:T2]->(c)` has two hops with different labels. The current `QueryAst` supports
only one traversal. Translator raises `TranslateError::UnsupportedFeature("multi-hop different edge types")`.

**RETURN handling:**
`RETURN b` where `b` is the terminal variable → result is the `QueryRows` from the traversal;
no special mapping needed.
`RETURN r` / `RETURN r.prop` → unblocked in 0.5.3: translator emits a `QueryAst` carrying one
`EdgeExpansionSlot` (the 0.5.3 surface behind `.traverse_edges()`). `RETURN r` projects the
`EdgeRow` side of each `(EdgeRow, NodeRow)` pair; `RETURN r.prop` extracts
`json_extract(edge.properties, '$.prop')` at projection.
`RETURN n.prop AS alias` → `TranslateError::UnsupportedFeature("property projection")` for v1;
callers extract from `node.properties` JSON.

### Engine surface

```rust
// crates/fathomdb-engine/src/lib.rs (Engine impl)
pub fn execute_cypher(
    &self,
    query: &str,
    params: HashMap<String, ScalarValue>,
) -> Result<QueryRows, CypherError>
```

Internally: `parse_cypher(query)? → translate(ast, params)? → compile_query(query_ast)? → execute`.
`CypherError` wraps `ParseError`, `TranslateError`, and `CompileError` under a unified enum with a
`UnsupportedCypherFeature(String)` variant callers can match to detect capability gaps.

### Python surface

```python
# fathomdb/_engine.py
def cypher(
    self,
    query: str,
    params: dict[str, str | int | float | bool] | None = None,
) -> QueryRows: ...
```

Raises `CypherError` (new exception class, subclass of `FathomDBError`) for parse/translate
failures, `UnsupportedCypherFeature` for constructs outside the v1 matrix.

### TypeScript surface

```typescript
// typescript/packages/fathomdb/src/engine.ts
cypher(
    query: string,
    params?: Record<string, string | number | boolean>,
): Promise<QueryRows>
```

Raises `CypherError` for parse/translate failures.

---

## Build order and sequencing

### Can start now (all underlying engine surfaces shipped in 0.5.1 / 0.5.3)

1. `cypher/ast.rs` — Cypher AST types, no engine dependency
2. `cypher/parser.rs` + `cypher/error.rs` — parser tests against plain Cypher strings
3. `cypher/translate.rs` — translator for node patterns, single-hop traversal, AND-only WHERE
4. `Engine::execute_cypher` wired to the translator
5. Python `db.cypher()` binding
6. Conformance matrix tests: one test per supported construct, one per explicitly-deferred construct
   asserting `UnsupportedCypherFeature`

### Wire the 0.5.x surfaces into the translator (no prior-release blocker)

7. Wire `EdgePropertyEq` / `EdgePropertyCompare` predicates in translator (0.5.1 surface)
8. Wire `JsonPathIn` (unfused) in translator (0.5.1 surface)
9. Wire `JsonPathFusedBoolEq` for `WHERE n.active = false` patterns (0.5.1 surface)
10. Wire `EdgeRow` result type in RETURN handling via 0.5.3 `EdgeExpansionSlot`
11. Wire `RETURN r.prop` extraction from `EdgeRow.properties` (0.5.3 surface)
12. TypeScript `db.cypher()` binding

### Deferred (not in 0.6.0)

- Multi-hop different edge types (two-query chain execution in engine)
- `OR` predicates (requires `Predicate::Or` in AST)
- `ORDER BY` (requires `QueryAst.order_by`)
- `RETURN n.prop AS alias` property projection
- `CALL fathom.vector_search(...)` procedure extension
- Bolt protocol

---

## Error contract

```rust
pub enum CypherError {
    Parse(ParseError),
    Translate(TranslateError),
    Compile(CompileError),
}

pub enum TranslateError {
    UnboundRootKind,
    UnresolvedParameter(String),
    UnsupportedFeature(String),    // message names the unsupported construct
    MultipleTraversals,
    EmptyInList,
}
```

`UnsupportedFeature` messages must be actionable. Not: "OR is not supported". Yes:
"OR predicates are not supported in the v1 Cypher subset; use separate queries and merge results".

---

## Testing strategy

Tests live in `crates/fathomdb-query/src/cypher/` as `#[cfg(test)]` modules plus integration
tests in `crates/fathomdb-engine/tests/cypher_surface.rs`.

Coverage required:
- Parser: each supported syntax form, each expected `ParseError` case, injection attempt on prop
  names and values (path validation in compile.rs catches escapes, but parser must not panic)
- Translator: each Cypher → QueryAst mapping in the conformance matrix above, each
  `UnsupportedFeature` case returns the correct error variant and message
- Engine integration: at least one end-to-end test per supported construct writing nodes/edges,
  running `execute_cypher`, asserting correct result rows
- Python bindings: smoke tests via `pytest` matching the integration test patterns

---

## What to publish (conformance statement)

Ship with a doc page `docs/reference/cypher-compatibility.md` containing the conformance matrix
above verbatim. No "Cypher support" marketing language — only the matrix. The matrix is the
contract. Callers who hit `UnsupportedCypherFeature` can check the doc without filing a bug.

The matrix is versioned: "v1 (FathomDB 0.6.0)". Future releases extend it without removing old
rows.
