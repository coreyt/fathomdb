# Implementation Plan: `TextQuery` in the Query AST

> **See also**: this plan implements the typed strict-grammar layer used by
> the adaptive text-search design in
> `dev/design-adaptive-text-search-surface.md`. Completing this plan is a
> sibling prerequisite alongside the tokenizer switch
> (`dev/notes/improving-with-better-tokenization-2026-04-11.md`, superseded
> by the adaptive design) for the adaptive design's Phase 1. Nothing in this
> plan changes as a result of the adaptive design; the adaptive work builds
> strictly above the `TextQuery` layer this plan introduces.

## Goal

Implement typed text queries in `fathomdb-query` by changing `QueryStep::TextSearch` to store `TextQuery` instead of `String`, with explicit test-driven development, docstrings on new and changed public types/functions, documentation updates under `docs/`, design/process updates under `dev/`, and a final `mkdocs` validation pass.

## Current code anchors

This plan is grounded in the current files:

- [`crates/fathomdb-query/src/ast.rs`](../../crates/fathomdb-query/src/ast.rs)
- [`crates/fathomdb-query/src/builder.rs`](../../crates/fathomdb-query/src/builder.rs)
- [`crates/fathomdb-query/src/compile.rs`](../../crates/fathomdb-query/src/compile.rs)
- [`crates/fathomdb-query/src/lib.rs`](../../crates/fathomdb-query/src/lib.rs)

Current relevant shapes:

```rust
// ast.rs
TextSearch {
    query: String,
    limit: usize,
}
```

```rust
// builder.rs
pub fn text_search(mut self, query: impl Into<String>, limit: usize) -> Self
```

```rust
// compile.rs
fn sanitize_fts5_query(raw: &str) -> String
```

## Exact target types

Add a new module:

- `crates/fathomdb-query/src/text_query.rs`

Recommended exact initial definitions:

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

impl TextQuery {
    pub fn parse(raw: &str) -> Self {
        // forgiving parser
    }
}

pub fn render_text_query_fts5(query: &TextQuery) -> String {
    // FTS5-safe renderer
}
```

Update `QueryStep`:

```rust
TextSearch {
    query: TextQuery,
    limit: usize,
}
```

## Required engineering constraints

These are mandatory for the implementation, not optional cleanup:

1. Use TDD.
   Each checkpoint starts by adding or updating tests that define the intended behavior before changing implementation.

2. Add docstrings.
   New public items and changed public items must have Rust doc comments, especially:
   - `TextQuery`
   - any public `impl TextQuery` parse entrypoint
   - `render_text_query_fts5` if it is public
   - updated `QueryStep::TextSearch` docs in `ast.rs`
   - updated `QueryBuilder::text_search` docs in `builder.rs`

3. Update user-facing docs in `docs/`.
   The querying guide and any relevant API/concept docs must describe the supported text-search subset and make clear that `text_search()` is a safe constrained surface, not raw FTS5 passthrough.

4. Update design/dev notes in `dev/`.
   The implementation should leave behind updated design notes or tracker notes that reflect the shipped behavior, not just pre-implementation planning docs.

5. Ensure MkDocs runs successfully.
   The implementation is not done until the docs site builds successfully via the repo’s MkDocs entrypoint.

## TDD sequence

### Checkpoint 1: Introduce `TextQuery` module without changing the AST

Purpose:

- build and test the parser and renderer in isolation
- avoid coupling parser correctness to broader query compilation

Steps:

1. Add failing parser and renderer tests in `text_query.rs`.
2. Add `text_query.rs`.
3. Export `TextQuery` and `render_text_query_fts5` from `lib.rs`.
4. Add docstrings to the new public API as it is introduced.
5. Keep all existing runtime behavior unchanged.

Tests to add first in `text_query.rs`:

```rust
#[test]
fn parse_empty_query() {
    assert_eq!(TextQuery::parse(""), TextQuery::Empty);
    assert_eq!(TextQuery::parse("   "), TextQuery::Empty);
}

#[test]
fn parse_plain_terms_as_implicit_and() {
    assert_eq!(
        TextQuery::parse("budget meeting"),
        TextQuery::And(vec![
            TextQuery::Term("budget".into()),
            TextQuery::Term("meeting".into()),
        ])
    );
}

#[test]
fn parse_phrase() {
    assert_eq!(
        TextQuery::parse("\"release notes\""),
        TextQuery::Phrase("release notes".into())
    );
}

#[test]
fn parse_or_operator() {
    assert_eq!(
        TextQuery::parse("ship OR docs"),
        TextQuery::Or(vec![
            TextQuery::Term("ship".into()),
            TextQuery::Term("docs".into()),
        ])
    );
}

#[test]
fn parse_not_operator() {
    assert_eq!(
        TextQuery::parse("ship NOT blocked"),
        TextQuery::And(vec![
            TextQuery::Term("ship".into()),
            TextQuery::Not(Box::new(TextQuery::Term("blocked".into()))),
        ])
    );
}

#[test]
fn parse_lowercase_or_as_literal() {
    assert_eq!(
        TextQuery::parse("ship or docs"),
        TextQuery::And(vec![
            TextQuery::Term("ship".into()),
            TextQuery::Term("or".into()),
            TextQuery::Term("docs".into()),
        ])
    );
}

#[test]
fn parse_trailing_or_as_literal() {
    assert_eq!(
        TextQuery::parse("ship OR"),
        TextQuery::And(vec![
            TextQuery::Term("ship".into()),
            TextQuery::Term("OR".into()),
        ])
    );
}
```

Renderer tests to add first:

```rust
#[test]
fn render_term_query() {
    assert_eq!(
        render_text_query_fts5(&TextQuery::Term("budget".into())),
        "\"budget\""
    );
}

#[test]
fn render_phrase_query() {
    assert_eq!(
        render_text_query_fts5(&TextQuery::Phrase("release notes".into())),
        "\"release notes\""
    );
}

#[test]
fn render_or_query() {
    assert_eq!(
        render_text_query_fts5(&TextQuery::Or(vec![
            TextQuery::Term("ship".into()),
            TextQuery::Term("docs".into()),
        ])),
        "\"ship\" OR \"docs\""
    );
}

#[test]
fn render_not_query() {
    assert_eq!(
        render_text_query_fts5(&TextQuery::And(vec![
            TextQuery::Term("ship".into()),
            TextQuery::Not(Box::new(TextQuery::Term("blocked".into()))),
        ])),
        "\"ship\" NOT \"blocked\""
    );
}

#[test]
fn render_escapes_embedded_quotes() {
    assert_eq!(
        render_text_query_fts5(&TextQuery::Term("say \"hello\"".into())),
        "\"say \"\"hello\"\"\""
    );
}
```

Checkpoint criteria:

- `cargo test -p fathomdb-query text_query -- --nocapture` passes
- no existing query compiler code changed yet
- new public items have doc comments

### Checkpoint 2: Replace sanitization tests with parser/renderer contract tests

Purpose:

- move test ownership from ad hoc string sanitization to typed search semantics

Steps:

1. Start by rewriting the existing `sanitize_fts5_*` contract as typed parser/renderer tests.
2. Remove or rename the existing `sanitize_fts5_*` tests in `compile.rs`.
3. Recreate their intent in `text_query.rs`.
4. Keep coverage for apostrophes, embedded quotes, empty input, unsupported syntax, and operator handling.

Tests to preserve semantically from current code:

- apostrophes remain literal
- embedded quotes are escaped correctly
- empty input stays empty
- `col:value` stays literal
- `foo*` stays literal
- `a NEAR b` treats `NEAR` as literal
- `cats AND dogs OR fish` does not expose raw `AND`; since explicit `AND` is not supported, `AND` should be a literal term while `OR` may still be recognized if surrounded by valid operands

Recommended exact tests:

```rust
#[test]
fn parse_apostrophe_as_literal_term() {
    assert_eq!(
        TextQuery::parse("User's name"),
        TextQuery::And(vec![
            TextQuery::Term("User's".into()),
            TextQuery::Term("name".into()),
        ])
    );
}

#[test]
fn parse_unsupported_column_filter_as_literal() {
    assert_eq!(
        TextQuery::parse("col:value"),
        TextQuery::Term("col:value".into())
    );
}

#[test]
fn parse_unsupported_prefix_as_literal() {
    assert_eq!(
        TextQuery::parse("prefix*"),
        TextQuery::Term("prefix*".into())
    );
}

#[test]
fn parse_near_as_literal() {
    assert_eq!(
        TextQuery::parse("a NEAR b"),
        TextQuery::And(vec![
            TextQuery::Term("a".into()),
            TextQuery::Term("NEAR".into()),
            TextQuery::Term("b".into()),
        ])
    );
}
```

Checkpoint criteria:

- old sanitizer-specific tests no longer define the contract
- new typed parser/renderer tests do

### Checkpoint 3: Change the AST to store `TextQuery`

Purpose:

- move FTS fully into the typed AST model

Steps:

1. Update `ast.rs`:

```rust
use crate::TextQuery;
```

and:

```rust
TextSearch {
    query: TextQuery,
    limit: usize,
}
```

2. Update `lib.rs` exports so `TextQuery` is available to users and internal modules.

3. Update the `QueryStep::TextSearch` doc comment to describe `TextQuery` rather than raw FTS text.
4. Fix all compile errors from changed field types.

Tests to add:

```rust
#[test]
fn builder_stores_typed_text_query_in_ast() {
    let query = QueryBuilder::nodes("task").text_search("ship OR docs", 5);
    let ast = query.clone().build_for_test_only();
    assert!(matches!(
        &ast.steps[0],
        QueryStep::TextSearch {
            query: TextQuery::Or(_),
            limit: 5
        }
    ));
}
```

If no AST inspection helper exists, add a small `#[cfg(test)]` accessor in `builder.rs` or test `QueryStep` construction directly.

Checkpoint criteria:

- `QueryAst` now stores typed text search
- no compiler logic still depends on raw FTS query strings in the AST
- updated AST docs compile cleanly

### Checkpoint 4: Parse in the builder

Purpose:

- make `QueryBuilder::text_search()` the normalization boundary

Steps:

1. Add or update builder tests first.
2. Update `builder.rs`:

```rust
pub fn text_search(mut self, query: impl Into<String>, limit: usize) -> Self {
    let parsed = TextQuery::parse(&query.into());
    self.ast.steps.push(QueryStep::TextSearch { query: parsed, limit });
    self
}
```

3. Keep the public builder signature as `impl Into<String>` to avoid API churn.
4. Update the `QueryBuilder::text_search` doc comment to describe the supported subset.

Builder tests to add:

```rust
#[test]
fn text_search_builder_parses_query_before_storage() {
    let builder = QueryBuilder::nodes("goal").text_search("ship NOT blocked", 10);
    let ast = builder.clone().build_for_test_only();
    assert_eq!(
        ast.steps[0],
        QueryStep::TextSearch {
            query: TextQuery::And(vec![
                TextQuery::Term("ship".into()),
                TextQuery::Not(Box::new(TextQuery::Term("blocked".into()))),
            ]),
            limit: 10,
        }
    );
}
```

Checkpoint criteria:

- builder remains ergonomic
- normalization happens once, early
- builder docs match implementation

### Checkpoint 5: Replace `sanitize_fts5_query()` in the compiler

Purpose:

- remove string sanitizer as the source of truth

Steps:

1. Add compile regression tests first.
2. In `compile.rs`, replace this flow:

```rust
let raw_query = ...
let sanitized = sanitize_fts5_query(raw_query);
```

with:

```rust
let text_query = ...
let rendered = render_text_query_fts5(text_query);
```

3. Delete `sanitize_fts5_query()` entirely after the compiler and tests no longer use it.

4. Keep the rest of the SQL and bind structure unchanged unless needed.

Compiler regression tests to add:

```rust
#[test]
fn text_search_or_compiles_to_fts_or_expression() {
    let compiled = QueryBuilder::nodes("task")
        .text_search("ship OR docs", 5)
        .compile()
        .expect("compile");

    assert!(
        compiled.binds.iter().any(|b| matches!(
            b,
            BindValue::Text(s) if s == "\"ship\" OR \"docs\""
        ))
    );
}

#[test]
fn text_search_not_compiles_to_fts_not_expression() {
    let compiled = QueryBuilder::nodes("task")
        .text_search("ship NOT blocked", 5)
        .compile()
        .expect("compile");

    assert!(
        compiled.binds.iter().any(|b| matches!(
            b,
            BindValue::Text(s) if s == "\"ship\" NOT \"blocked\""
        ))
    );
}

#[test]
fn unsupported_syntax_stays_literal_in_compiled_fts() {
    let compiled = QueryBuilder::nodes("task")
        .text_search("col:value", 5)
        .compile()
        .expect("compile");

    assert!(
        compiled.binds.iter().any(|b| matches!(
            b,
            BindValue::Text(s) if s == "\"col:value\""
        ))
    );
}
```

Checkpoint criteria:

- compiler behavior now derives from `TextQuery`
- `sanitize_fts5_query()` removed
- compile-time comments/docstrings no longer describe the old sanitizer model

### Checkpoint 6: Full regression pass

Purpose:

- prove no broader query API regression

Commands:

- `cargo test -p fathomdb-query`
- optionally `cargo test -p fathomdb`

Validation checklist:

- existing non-FTS query tests still pass
- JSON path validation still works unchanged
- filter pushdown tests still pass
- traversal and grouped query compilation still pass

### Checkpoint 7: Update user and design documentation

Purpose:

- make the shipped behavior discoverable and keep docs aligned with the code

Required updates:

1. Update `docs/guides/querying.md`.
   Document:
   - supported text-search primitives
   - supported operators: implicit `AND`, `OR`, `NOT`, phrases
   - unsupported syntax that remains literal
   - examples that map user input to actual search behavior

2. Update any other relevant `docs/` pages that mention `text_search()`.
   At minimum, search for `text_search(` and update pages whose descriptions assume "plain quoted token AND semantics only".

3. Update `dev/` artifacts to record the final shipped decision.
   This can be:
   - updating these notes in place with "implemented" adjustments, or
   - adding a short follow-up note that records deltas from the original plan

   The `spec-supported-query-primitives-and-operators` note should serve as the
   canonical shipped-behavior record for the default `text_search()` surface.

Checkpoint criteria:

- `docs/` reflects the supported subset accurately
- `dev/` records the final implementation shape and any deviations from plan

### Checkpoint 8: Build docs with MkDocs

Purpose:

- ensure the documentation changes are valid and integrated into the site

Required commands:

- `bash docs/build.sh`

If the environment does not have docs dependencies installed, use the repository’s documented docs environment or install path, then rerun the build.

Checkpoint criteria:

- MkDocs completes successfully
- documentation changes render without broken references or strict-build failures

## Parser design notes for implementation

Keep the parser deliberately small.

Suggested tokenizer output:

```rust
enum Token {
    Word(String),
    Phrase(String),
    Or,
    Not,
}
```

Rules:

- `OR` recognized only as exact uppercase token
- `NOT` recognized only as exact uppercase token
- quoted phrases parsed before operator recognition
- unsupported characters remain inside `Word`

Suggested parser strategy:

1. tokenize
2. convert `NOT atom` pairs into unary nodes where valid
3. split on `OR` where valid
4. combine adjacency as `And`
5. degrade malformed operator positions back into `Term("OR")` / `Term("NOT")`

That strategy is enough for the current supported subset and avoids a general parser framework.

## Open implementation decisions

These should be resolved before coding, but they do not block the overall design:

1. `Empty` representation:
   `TextQuery::Empty` is clearer than `And(vec![])` and makes empty-query tests simpler.

2. Phrase fallback behavior:
   if a trailing `"` is unmatched, treat the whole token stream conservatively as literal terms rather than inventing partial phrase syntax.

3. Rendering `Not`:
   prefer normalized AST forms where `Not` appears inside `And` or `Or` operands rather than as arbitrary top-level shape if FTS5 legality depends on position.

## Definition of done

This work is done when:

- `QueryStep::TextSearch` stores `TextQuery`
- `QueryBuilder::text_search()` parses into `TextQuery`
- `compile_query()` renders `TextQuery` to safe FTS5
- sanitizer-string tests are replaced by typed parser/renderer tests
- `OR` and `NOT` work in compiled FTS binds
- unsupported syntax remains literal
- the broader query builder remains unchanged for callers
- new and changed public APIs have docstrings
- relevant `docs/` pages are updated
- relevant `dev/` notes are updated
- `bash docs/build.sh` succeeds
