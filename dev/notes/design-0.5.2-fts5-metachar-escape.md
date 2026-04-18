# Design: FTS5 metacharacter escape hardening (0.5.2 Item 3)

**Release:** 0.5.2
**Scope item:** Item 3 from `dev/notes/0.5.2-scope.md` (GH #31)
**Breaking:** No (bug fix; previously-rejected queries now succeed)

---

## Problem

User-reported crash (GH #31): FTS5 syntax error when query contains
unescaped metacharacters.

Reproduction from the issue body:

```python
engine = Engine.open("test.db")
# ... register a collection with text_search support ...
q = engine.nodes("some_kind").text_search("User's name", limit=10)
q.execute()
# -> SqliteError: fts5: syntax error near "'"
```

Hit in production against Memex's retrieval layer when a query contained
`User's name`. The apostrophe reaches SQLite's FTS5 MATCH parser raw and
is rejected as invalid syntax.

---

## Current state (anchored to 0.5.1 HEAD)

`crates/fathomdb-query/src/text_query.rs:122-125` already defines
`quote_fts5_literal`:

```rust
fn quote_fts5_literal(raw: &str) -> String {
    let escaped = raw.replace('"', "\"\"");
    format!("\"{escaped}\"")
}
```

This wraps a term in double quotes and doubles embedded `"` chars —
sufficient to neutralize most FTS5 metacharacters inside a phrase
literal.

`render_text_query_fts5` (line 83) uses `quote_fts5_literal` for both
`TextQuery::Term` and `TextQuery::Phrase`, so the *happy path* from
`TextQuery` through the renderer is safe.

The bug implies one of:

1. A path bypasses `render_text_query_fts5` and concatenates raw user
   input into a MATCH expression. (Primary hypothesis — the production
   stack trace in GH #31 shows the crash at SQLite FTS5 prepare, which
   means the expression reached SQLite unescaped.)
2. `TextQuery::parse` splits `"User's name"` into tokens in a way that
   defeats `quote_fts5_literal`'s protection. (Secondary; unlikely
   because the quote happens around the final token.)
3. `quote_fts5_literal` itself is incomplete — it protects `"` but not
   some other metacharacter that SQLite's FTS5 tokenizer also treats
   as special.

Call sites to audit:

- `crates/fathomdb-engine/src/coordinator.rs:1569` —
  `render_text_query_fts5(&compiled.text_query)` (safe path)
- `crates/fathomdb-engine/src/coordinator.rs:2115` — same call
  (`strict_expr`) (safe path)
- Any *other* use of `... MATCH ?` in engine SQL. Grep target:
  `MATCH ?` / `MATCH ?1` / `MATCH \?` across `crates/fathomdb-engine/`
  and `crates/fathomdb/`.
- Python SDK: `python/fathomdb/_query.py::text_search` passes `query`
  verbatim to `TextSearchBuilder`; that builder's FFI call goes through
  the Rust renderer.
- TypeScript SDK: equivalent path in `typescript/packages/fathomdb/src/`.

---

## Goal

Make every user-input → FTS5 MATCH path escape metacharacters. Concrete
behavior:

- `text_search("User's name")` returns results (no crash).
- `text_search('He said "hi"')` returns results matching the phrase
  with the embedded quotes treated as literal.
- `text_search("(query)")` treats parentheses as literal characters
  inside a term, not as FTS5 grouping.
- `text_search("foo:bar")` treats the colon as literal (FTS5 would
  otherwise interpret `:` as a column specifier).
- `text_search("-stop")` treats the `-` as literal (FTS5 otherwise
  treats `-` as "exclude").

---

## Design

### Step 1: Audit all raw→MATCH paths

Grep (first task in TDD):

```bash
rg -n '" MATCH|MATCH ?' crates/ --type rust | grep -v tests
```

For each result, confirm the MATCH argument comes from
`render_text_query_fts5` or `quote_fts5_literal` — not a direct user
string. Any call that binds a user-supplied `String` directly to
`?<N>` where that `?<N>` is an FTS5 MATCH operand is a bug site.

### Step 2: Ensure `quote_fts5_literal` covers all FTS5-syntactic chars

FTS5's query syntax treats the following as structural when they appear
outside a quoted phrase:

```
" : ( ) { } [ ] - + * ^ AND OR NOT
```

Inside a double-quoted phrase, SQLite FTS5 treats almost everything as
a literal except `"` itself. The existing `quote_fts5_literal` escapes
only `"`. That *is* correct for phrase literals — once we wrap the
term in `"..."`, every other metacharacter inside is inert.

**Check:** confirm the current renderer always wraps terms in `"..."`,
even single-word terms. `quote_fts5_literal("User's")` returns
`"User's"` — that passes FTS5's phrase parser.

### Step 3: Find the bypass

If Step 1 finds a raw binding site, fix it by routing through
`render_text_query_fts5(&TextQuery::Term(user_input.to_owned()))` or a
thinner escape helper exported from `fathomdb-query`.

If Step 1 finds no bypass, the bug reproduces as a regression test,
and the investigation moves to `TextQuery::parse` tokenization on
apostrophe-containing input. Possible cause: the tokenizer trips on
`'` and produces a token that later renderer paths don't re-escape.
Fix by widening the tokenizer's whitespace split or by passing the
entire input as `TextQuery::Phrase` rather than tokenizing.

### Step 4: Public escape helper (optional)

Expose a public escape helper from `fathomdb-query`:

```rust
/// Wrap an arbitrary user string in an FTS5-safe phrase literal.
/// Doubles embedded double-quotes; other metacharacters are inert
/// inside a phrase literal per SQLite FTS5 syntax.
#[must_use]
pub fn escape_fts5_phrase(raw: &str) -> String {
    // Move `quote_fts5_literal` here, re-export.
    // Currently module-private; promote to `pub`.
}
```

Allows downstream users writing custom SQL against FTS5 tables to share
the same escape contract.

### TDD approach

Tests in `crates/fathomdb/tests/text_search_surface.rs` (new module or
append to existing):

1. **Red: apostrophe crash**

   ```rust
   #[test]
   fn text_search_handles_apostrophe_in_query() {
       let db = open_with_fts_kind("Doc", /* ... */);
       // Insert a node with content matching "User's".
       // ...
       let rows = db.nodes("Doc")
           .text_search("User's name", 10)
           .execute()
           .expect("text_search must not crash on apostrophe");
       assert!(!rows.nodes.is_empty());
   }
   ```

   Fails against 0.5.1 with `SqliteError: fts5: syntax error near "'"`.

2. **Red: embedded double-quote**

   ```rust
   #[test]
   fn text_search_handles_double_quote_in_phrase() {
       // Query: He said "hi"
       // Expected: matches nodes containing that exact phrase literal.
   }
   ```

3. **Red: structural char inside term**

   ```rust
   #[test]
   fn text_search_treats_colon_as_literal() {
       // Query: foo:bar
       // Expected: matches nodes containing the literal "foo:bar", not
       // treated as a column specifier.
   }
   ```

4. **Green**: fix the bypass identified in Step 3. All three tests
   pass.

5. **Python + TypeScript mirror tests**: one test per SDK that passes
   an apostrophe-containing query end-to-end.

### Rust-side fuzz harness (optional)

If the issue survives the targeted tests, add a property test:

```rust
#[cfg(test)]
proptest! {
    #[test]
    fn text_search_never_crashes_on_arbitrary_input(s: String) {
        // ... open engine, run text_search(s, 10), assert no error ...
    }
}
```

Kept off the critical path; run manually if CI has not surfaced the
root cause.

---

## Out of scope

- GH #37 — supporting safe FTS5 operators (`AND`, `OR`, `NOT`) as
  user-expressible syntax. That's a feature; this is a bug fix for
  queries that should have worked.
- GH #36 — nested-object property FTS and per-field weighting.
- Changing the default tokenizer or preset behavior.

---

## Acceptance

- GH #31 reproduction no longer crashes.
- Three new Rust integration tests pass (apostrophe, double-quote,
  colon).
- Python and TypeScript mirror tests pass.
- `crates/fathomdb-engine/` grep for raw `MATCH ?` bindings reveals no
  unescaped user-input path.

---

## Cypher enablement note

N/A. `text_search` is a SDK method, not a Cypher construct.
