# fathomdb-query

The text-query compiler used by **FathomDB**, a local-first embedded retrieval engine built on
SQLite.

## Is this the crate you want?

Probably not, unless you are working *on* FathomDB rather than *with* it. Applications should depend
on [`fathomdb`](https://crates.io/crates/fathomdb) and call `Engine::search`, which uses this crate
internally.

It is published separately because it is a **leaf** — no dependencies at all — and because the one
thing it does is independently useful: turning arbitrary user text into an SQLite FTS5 `MATCH`
expression that is safe to interpolate.

## Status: pre-1.0, beta

The 0.8.x line is under active development. The public surface can change between minor releases.

## What it does

`compile_text_query` lowercases the input, splits on non-alphanumeric characters, drops a small
English stopword list and tokens shorter than three characters, de-duplicates the rest in first-seen
order, and OR-joins them as quoted FTS5 literals:

```rust
use fathomdb_query::compile_text_query;

let compiled = compile_text_query("status of the alpha");
assert_eq!(compiled.match_expression, "\"status\" OR \"alpha\"");
```

Two properties are the point:

- **Injection safety.** Splitting on non-alphanumeric characters removes every FTS5 control
  character (`*`, `"`, `:`, `^`, `(`, `)`, `,`) before quoting, so the emitted expression contains
  only literals.
- **OR, not AND.** Requiring every token to be present near-zeroes recall on natural-language
  questions. OR-ing the content tokens lets `bm25()` rank by overlap instead.

A query with no content tokens at all (all stopwords, all symbols, or all sub-three-character) falls
back to an OR over the raw whitespace tokens as quoted phrases, so it still searches rather than
matching nothing.

## Install

```bash
cargo add fathomdb-query
```

## License

MIT. See the `LICENSE` file shipped in this crate.

Source, issues and full documentation: <https://github.com/coreyt/fathomdb>
