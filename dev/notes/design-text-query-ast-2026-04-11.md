# Design: Store `TextQuery` in `QueryStep::TextSearch`

> **See also**: `dev/design-adaptive-text-search-surface.md` builds on the
> typed `TextQuery` introduced here, treating it as the strict-grammar layer
> under an adaptive retrieval policy (strict + relaxed, block-based ranking,
> opt-in match attribution). This note remains authoritative for the typed
> grammar and its lowering; the adaptive design is authoritative for the
> retrieval policy and result shape above it.

## Goal

Change `QueryStep::TextSearch` from storing a raw `String` to storing a typed `TextQuery`, and make the compiler lower that typed representation into safe FTS5 `MATCH` syntax.

This note intentionally starts at the target design rather than an incremental bridge design. The target should be the stable shape of the query surface.

## Why this change

Today `fathomdb-query` is already mostly structured:

- `QueryBuilder` builds a `QueryAst`
- `QueryAst` stores typed steps and typed predicates
- `compile_query()` lowers the AST into SQLite SQL and bind parameters

The main exception is FTS:

- [`QueryStep::TextSearch`](../../crates/fathomdb-query/src/ast.rs) stores `query: String`
- [`compile_query()`](../../crates/fathomdb-query/src/compile.rs) interprets that string late
- [`sanitize_fts5_query()`](../../crates/fathomdb-query/src/compile.rs) currently combines parsing policy, escaping, and lowering into one string-to-string function

That is the main place where the otherwise unified query surface falls back to an embedded mini-language carried around as an untyped string.

## Current dependencies and assumptions

The target design depends on these existing code facts:

1. `fathomdb-query` already has a typed AST boundary.
   `QueryBuilder` in [`builder.rs`](../../crates/fathomdb-query/src/builder.rs) constructs `QueryAst`, and `compile.rs` assumes it is compiling structured intent rather than arbitrary SQL.

2. `QueryStep::TextSearch` is currently the FTS boundary.
   In [`ast.rs`](../../crates/fathomdb-query/src/ast.rs), the current shape is:

```rust
TextSearch {
    query: String,
    limit: usize,
}
```

3. FTS compilation is already centralized.
   In [`compile.rs`](../../crates/fathomdb-query/src/compile.rs), `DrivingTable::FtsNodes` finds the `TextSearch` step, sanitizes the string, then binds the resulting `MATCH` expression twice, once for `fts_nodes` and once for `fts_node_properties`.

4. The codebase already uses validation at the AST-to-engine boundary.
   `validate_json_path()` is an existing example of a constrained interpreted string surface.

5. The current test suite already has dedicated FTS sanitization tests.
   Those tests can be replaced by parser and renderer tests without changing the surrounding test structure too much.

6. `text_search()` currently means "safe, user-facing search", not "raw FTS5".
   The existing sanitizer and the issue discussion both imply that the public contract is a constrained safe surface. This design preserves that contract instead of introducing raw FTS5 into the default API.

## Target public shape

The public builder remains simple:

```rust
QueryBuilder::nodes("goal").text_search("ship OR docs", 10)
```

But the AST should store a typed query:

```rust
pub enum QueryStep {
    VectorSearch {
        query: String,
        limit: usize,
    },
    TextSearch {
        query: TextQuery,
        limit: usize,
    },
    Traverse {
        direction: TraverseDirection,
        label: String,
        max_depth: usize,
    },
    Filter(Predicate),
}
```

`TextQuery` is not a new public query language. It is an internal representation of the supported subset of already-known search behavior: literal terms, phrases, `OR`, `NOT`, and implicit `AND`.

## Recommended `TextQuery` representation

Use an internal AST that matches the supported subset without exposing raw FTS5 syntax:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextQuery {
    Empty,
    Term(String),
    Phrase(String),
    Not(Box<TextQuery>),
    And(Vec<TextQuery>),
    Or(Vec<TextQuery>),
}
```

This shape is preferable to a flatter token list because:

- it keeps boolean structure explicit
- it is easy to lower to FTS5
- it is easy to test
- it leaves room for future explicit grouping support without redesign

Even if the first parser only accepts a tiny grammar, the AST should represent boolean structure directly.

## Parsing model

The parser should accept a small subset of familiar search syntax:

- bare terms
- quoted phrases
- uppercase `OR`
- uppercase `NOT`
- implicit `AND` by adjacency

Everything else is either:

- treated as a literal term or phrase, or
- rejected if we decide the contract should fail closed for impossible states

For the default `text_search()` surface, the parser should be forgiving:

- unsupported syntax does not pass through
- malformed operators degrade to literals where possible
- parse failures should be rare and reserved for actual internal invariants

Examples:

- `ship docs` -> `And([Term("ship"), Term("docs")])`
- `ship OR docs` -> `Or([Term("ship"), Term("docs")])`
- `ship NOT blocked` -> `And([Term("ship"), Not(Term("blocked"))])`
- `ship OR` -> `And([Term("ship"), Term("OR")])`
- `NOT ship` -> `Not(Term("ship"))`
- `col:value` -> `Term("col:value")`
- `foo*` -> `Term("foo*")`

## Lowering model

Compilation to FTS5 should happen from `TextQuery`, not from raw text.

Introduce a renderer such as:

```rust
pub fn render_text_query_fts5(query: &TextQuery) -> String
```

Lowering rules:

- `Term(s)` -> FTS5-escaped quoted token
- `Phrase(s)` -> FTS5-escaped quoted phrase
- `And([a, b, c])` -> `<a> <b> <c>`
- `Or([a, b])` -> `<a> OR <b>`
- `Not(x)` -> `NOT <x>` when legal in position, or emitted as part of surrounding `And`
- `Empty` -> `""` as an internal sentinel, with outer compile logic continuing current empty behavior

Important constraint: the renderer must be the only place that emits FTS5 syntax. Unsupported raw text never reaches SQLite unescaped.

## Placement in the current pipeline

The final steady-state pipeline should be:

1. `QueryBuilder::text_search(raw, limit)`
2. parse `raw` into `TextQuery`
3. store `TextQuery` in `QueryAst`
4. `compile_query()` renders `TextQuery` to a safe FTS5 string
5. bind rendered string into the existing SQL

This keeps the current `QueryBuilder -> QueryAst -> compile_query()` architecture intact. The change is local to the shape of the `TextSearch` step and its compile-time lowering.

## Builder API design

Recommended builder signature:

```rust
pub fn text_search(mut self, query: impl Into<TextQueryInput>, limit: usize) -> Self
```

That is likely too abstract for the current codebase. A more practical first stable API is:

```rust
pub fn text_search(mut self, query: impl Into<String>, limit: usize) -> Self
```

and parse inside the builder:

```rust
let parsed = TextQuery::parse(&query.into());
```

This preserves current ergonomics and lets SDKs continue passing strings. The AST still becomes typed immediately.

## Error policy

The current builder does not return `Result`, and changing that would ripple through the whole query surface. Therefore the default parsing mode should be non-failing and normalizing.

Recommended policy:

- recognize only exact supported operators
- treat malformed operators as literals
- never pass unsupported syntax through as control syntax

This is consistent with the current safe-user-input intent and avoids introducing a new error channel into the builder API.

## Module layout

Add a dedicated module:

- `crates/fathomdb-query/src/text_query.rs`

Recommended contents:

- `pub enum TextQuery`
- internal tokenizer
- parser from `&str` to `TextQuery`
- renderer from `&TextQuery` to FTS5 string
- parser/renderer unit tests

Then export `TextQuery` from [`lib.rs`](../../crates/fathomdb-query/src/lib.rs).

## Impact on existing code

Files that will need coordinated changes:

- [`crates/fathomdb-query/src/ast.rs`](../../crates/fathomdb-query/src/ast.rs)
- [`crates/fathomdb-query/src/builder.rs`](../../crates/fathomdb-query/src/builder.rs)
- [`crates/fathomdb-query/src/compile.rs`](../../crates/fathomdb-query/src/compile.rs)
- [`crates/fathomdb-query/src/lib.rs`](../../crates/fathomdb-query/src/lib.rs)
- new [`crates/fathomdb-query/src/text_query.rs`](../../crates/fathomdb-query/src/text_query.rs)

## Non-goals

This design does not introduce:

- raw FTS5 passthrough in `text_search()`
- support for column filters
- support for `NEAR`
- support for prefix `*`
- support for parentheses or nested precedence authored by callers
- a new user-visible query language distinct from familiar boolean search primitives

## Why this design fits the unified query surface

`fathomdb` already prefers:

- typed predicates over expression strings
- typed traversal over string query fragments
- validation at the AST-to-engine boundary

Changing `TextSearch` to store `TextQuery` aligns FTS with the rest of the query API:

- user text becomes structured intent
- structured intent becomes engine syntax
- engine syntax is never the default authoring surface

That is the right long-term shape for a unified, agent-safe query surface.
