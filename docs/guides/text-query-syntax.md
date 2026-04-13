# Text Query Syntax

This page defines the supported query-language subset for
`Query.text_search(...)`.

`fathomdb` does not expose raw FTS5 syntax through `text_search()`. Instead,
it accepts a constrained, familiar search-box subset and lowers that subset to
SQLite FTS5 safely.

## How this fits into adaptive search

`text_search()` is an **adaptive** surface: the engine runs two branches
under the hood (strict-then-relaxed) and merges them into a single
`SearchRows` result. The grammar documented on this page defines the
**strict half** of that policy — the interpretation the engine gives to
your query when it lowers it literally to FTS5.

The relaxed half of the policy is engine-owned. It is derived from the
strict AST (term-level alternatives, softened exclusions, per-term
fallbacks) and is **not** a separate user-facing syntax. You do not write
relaxed queries by hand through `text_search()`.

For the shape of the adaptive policy, per-branch hit counts, and the
`fallback_used` / `strict_hit_count` / `relaxed_hit_count` fields on
`SearchRows`, see [Querying Data](./querying.md#adaptive-text-search). For
the narrow case where you want to supply both a strict and a relaxed
shape verbatim — bypassing adaptive derivation — use
[`Engine.fallback_search`](./querying.md#explicit-two-shape-fallback-search).

## Supported forms

### Bare terms

Bare tokens are searched as literal terms.

Examples:

```python
db.nodes("Document").text_search("ship", limit=20)
db.nodes("Document").text_search("project deadline", limit=20)
```

`project deadline` means an implicit `AND` between `project` and `deadline`.

### Quoted phrases

Double-quoted text is searched as a literal phrase.

Example:

```python
db.nodes("Document").text_search('"release notes"', limit=20)
```

### `OR`

Uppercase `OR` is supported as a boolean operator.

Example:

```python
db.nodes("Document").text_search("ship OR docs", limit=20)
```

### `NOT`

Uppercase `NOT` is supported as an exclusion operator only in valid clause
positions.

Example:

```python
db.nodes("Document").text_search("ship NOT blocked", limit=20)
```

## Literal downgrade rules

The parser is intentionally conservative. Unsupported or malformed syntax is
treated as literal text instead of being passed through as FTS5 control
language.

This protects common inputs from turning into invalid or overly-powerful engine
syntax, especially when text is copied from external sources or passed through
agents.

### Lowercase `or` and `not`

Lowercase `or` and `not` are searched as literal words, not operators.

Example:

```python
db.nodes("Document").text_search("not a ship", limit=20)
```

This searches for the literal words `not`, `a`, and `ship`. It is intended to
match stored text such as `the boat is not a ship`.

### Clause-leading `NOT`

If `NOT` appears where the supported subset does not allow it, it degrades to
a literal term.

Examples:

```python
db.nodes("Document").text_search("NOT blocked", limit=20)
db.nodes("Document").text_search("ship OR NOT blocked", limit=20)
```

These do not emit raw FTS5 `NOT` syntax. They are treated as literal-term
queries instead.

## Matching behavior

The text query grammar on this page describes the **syntax** you can author.
The **match** that FathomDB performs against the index applies additional
transformations:

- **English stemming.** `ship`, `ships`, and `shipping` all match via a shared
  stem, so a search for `ship` will find documents containing any of them.
  This is the main recall lever for natural-language queries.
- **Diacritic folding.** `café` and `cafe` are interchangeable at index- and
  query-time. Diacritics on non-alphabetic codepoints are also folded.
- **Case insensitivity.** You do not need to lowercase input.

These transformations are a property of the default tokenizer
(`unicode61 remove_diacritics 2` + `porter`) and apply uniformly to both
chunk-text search and property-text search. They do not change the lowering
rules above — `ship` still lowers to the FTS5 expression `"ship"`; FTS5
itself does the stem match against indexed stems.

## Unsupported syntax

The following are not part of the public `text_search()` contract:

- raw FTS5 syntax passthrough
- parentheses for grouping
- `NEAR`
- prefix wildcard operators such as `ship*`
- column filters such as `title:ship`

If these forms appear in input, they remain literal text unless future
documentation explicitly expands the supported subset.

## Rule of thumb

- Use uppercase `OR` and `NOT` only when you intend boolean search.
- Use lowercase words when you want them searched literally.
- Assume anything not documented on this page is not a supported operator.

See also:

- [Querying Data](querying.md#adaptive-text-search)
- [Property FTS Projections](property-fts.md)
- [Query API Reference](../reference/query.md)
