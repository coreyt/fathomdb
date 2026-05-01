# Spec: FathomDB-Supported Query Primitives and Operators

## Purpose

This spec defines the supported search primitives and operators for FathomDB's default text query surface.

This is not a new query language. It is a constrained subset of already-familiar search conventions and FTS behavior, chosen so that:

- developers and agents can express common search intent
- copied or hostile text remains data by default
- the compiler can lower supported intent into safe FTS5 syntax

## Scope

This spec applies to the default `text_search()` query surface in `fathomdb-query`.

It does not define a raw FTS5 mode.

It does not change the broader typed `QueryBuilder` surface for filters, traversal, or limits.

## Design principles

1. Default to literals.
   Text is treated as data unless it matches a small supported operator set exactly.

2. Support familiar boolean search.
   The default surface should handle the common cases users and agents expect from search boxes.

3. Do not expose engine-specific structure by default.
   Internal FTS5 features that expose schema, parser complexity, or unbounded expressiveness stay out of the default surface.

4. Keep the authoring surface smaller than the engine grammar.
   FTS5 remains the execution engine, not the user-facing grammar.

## Supported primitives

### Term

A term is a bare token matched as a literal search token.

Examples:

- `ship`
- `docs`
- `blocked`
- `col:value`
- `foo*`

Terms are always treated as literal content unless they are recognized as supported operators.

### Phrase

A phrase is text surrounded by double quotes and matched as a literal phrase.

Examples:

- `"ship docs"`
- `"release notes"`

Embedded double quotes inside a phrase must be escaped by the internal compiler according to FTS5 rules. Callers do not author FTS5 escaping directly.

### Implicit `AND`

Adjacent terms and phrases are combined with logical `AND`.

Examples:

- `ship docs`
- `"ship docs" blocked`

Semantics:

- `ship docs` means documents matching both `ship` and `docs`

### `OR`

Uppercase `OR` is a supported boolean operator.

Examples:

- `ship OR docs`
- `"release notes" OR changelog`

Recognition rules:

- only uppercase `OR` is treated as an operator
- lowercase `or` is treated as a literal term

Malformed use degrades to literal behavior where possible.

Example:

- `ship OR` is interpreted as searching for `ship` and the literal term `OR`

### `NOT`

Uppercase `NOT` is a supported boolean exclusion operator.

Examples:

- `ship NOT blocked`
- `NOT archived`

Recognition rules:

- only uppercase `NOT` is treated as an operator
- lowercase `not` is treated as a literal term
- `NOT` must apply to a supported following atom

Malformed use degrades to literal behavior where possible.

## Unsupported primitives and operators

The following are not part of the default supported surface and must not pass through as control syntax.

### Parentheses

Examples:

- `(a OR b)`
- `ship AND (docs OR notes)`

Status:

- unsupported
- parentheses are treated as literal characters within terms or phrases unless a future version explicitly adds grouping

### `NEAR`

Examples:

- `ship NEAR docs`
- `NEAR/5`

Status:

- unsupported
- treated as literal text unless a future version explicitly adds proximity search

### Prefix wildcard `*`

Examples:

- `ship*`
- `doc*`

Status:

- unsupported as an operator
- treated as part of the literal term

### Column filters

Examples:

- `title:ship`
- `body:docs`

Status:

- unsupported
- treated as literal terms

Reason:

- column filters expose engine-level schema and are not appropriate for the default safe surface

### Raw FTS5 syntax passthrough

Any syntax not explicitly listed as supported above is not part of the public contract for `text_search()`.

## Token recognition rules

The parser recognizes syntax conservatively:

- uppercase `OR` and `NOT` only
- quoted phrases using `"` delimiters
- whitespace as separator

Everything else is interpreted as literal content.

This conservative rule is deliberate. It reduces accidental control-language interpretation when input comes from:

- copied text
- scraped web content
- agent-transformed text
- partially malformed or mixed natural language input

## Lowering rules

The compiler lowers supported primitives to FTS5:

- literal terms are always quoted and escaped
- phrases are always quoted and escaped
- only supported boolean operators are emitted as FTS5 operators
- unsupported syntax is never passed through verbatim as FTS5 control syntax

Examples:

- `ship docs` -> `"ship" "docs"`
- `ship OR docs` -> `"ship" OR "docs"`
- `ship NOT blocked` -> `"ship" NOT "blocked"`
- `"release notes"` -> `"release notes"`
- `col:value` -> `"col:value"`
- `foo*` -> `"foo*"`

## Match-time semantics

The rules above define the **grammar** and its **lowering** to FTS5. They do
not describe how FTS5 then matches the lowered expression against the index.
The default tokenizer — `unicode61 remove_diacritics 2` + `porter` — applies
three transformations during matching:

- English stemming via `porter`, so `ship`, `ships`, and `shipping` share a
  stem and match each other
- diacritic folding via `remove_diacritics 2`, so `café` and `cafe` are
  interchangeable
- case insensitivity via `unicode61`

These transformations are uniform across chunk FTS and property FTS. They
do not affect the lowering rules (literals and phrases are still quoted as
specified); they affect what the lowered FTS5 expression matches inside the
index. See `dev/design-adaptive-text-search-surface.md` for the tokenizer
commitment and its rationale.

## Error handling and normalization

The default `text_search()` surface is forgiving, not strict.

Rules:

- malformed operator usage should prefer literal interpretation over failure
- unsupported syntax should be downgraded to literals instead of partially interpreted
- the default builder path should avoid introducing new parse errors for ordinary user or agent input

This mirrors the intended contract of `text_search()` as a safe default search surface.

## Examples

### Supported

- `ship`
- `ship docs`
- `ship OR docs`
- `ship NOT blocked`
- `NOT archived`
- `"release notes"`
- `"release notes" OR changelog`

### Supported with literal downgrade

- `ship OR`
- `or`
- `not`
- `col:value`
- `foo*`
- `(a OR b)`
- `a NEAR b`

## Relationship to the broader query API

This spec is only for the text-search sublanguage embedded in the broader typed query API.

Elsewhere in the query surface, `fathomdb` should continue to prefer typed structure:

- traversal as typed direction, label, depth
- filters as typed predicates
- comparisons as typed scalar operations

The default text search surface follows the same principle by constraining search input to a typed subset before compilation to FTS5.

## Shipped behavior record

When the `TextQuery`-backed default surface ships, these rules are the expected
public contract to preserve:

- bare terms and quoted phrases are supported
- implicit `AND` applies between adjacent supported atoms
- uppercase `OR` and uppercase `NOT` are supported
- unsupported syntax remains literal instead of becoming raw FTS5 control syntax
- the public `text_search()` API is a safe constrained surface, not a raw FTS5 passthrough

Keep this note and the user docs aligned if implementation details change. Any
future expansion should be added here first so the contract stays explicit.
