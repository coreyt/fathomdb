# Improving FTS5 with Better Tokenization

_2026-04-11_

> **Superseded by `dev/design-adaptive-text-search-surface.md`.**
> The adaptive text search surface design is now the authoritative source for
> the tokenizer commitment. The chosen tokenizer is
> `unicode61 remove_diacritics 2` + `porter`, specified in the Tokenization
> section of that document. This note is retained as historical context and
> for its FTS5 migration mechanics, which remain accurate.

## Current State

Both `fts_nodes` and `fts_node_properties` use the SQLite default tokenizer (`unicode61`):

```sql
-- bootstrap.rs:64-69
CREATE VIRTUAL TABLE IF NOT EXISTS fts_nodes USING fts5(
    chunk_id UNINDEXED,
    node_logical_id UNINDEXED,
    kind UNINDEXED,
    text_content
);

-- bootstrap.rs:390-394
CREATE VIRTUAL TABLE IF NOT EXISTS fts_node_properties USING fts5(
    node_logical_id UNINDEXED,
    kind UNINDEXED,
    text_content
);
```

No tokenizer, no prefix index, no column weights specified.

## Problem

The default `unicode61` tokenizer does no stemming. Searching for "running" will not match documents containing "run", "runs", or "ran". This reduces recall for natural-language content.

There is also no prefix index, so prefix/autocomplete queries (`run*`) require a full scan of the FTS index rather than a fast prefix lookup.

## Options

### 1. Porter stemming (`tokenize = 'porter unicode61'`)

The simplest upgrade. Porter wraps unicode61 and stems tokens to root forms:

- "running" -> "run"
- "organizations" -> "organ" (known Porter artifact)
- "better" -> "better" (irregular, not stemmed)

Pros:
- Single config change, no code changes needed
- Good general-purpose recall improvement
- Well-tested, part of SQLite core

Cons:
- Over-stems some words (organization -> organ)
- English-only stemming
- Cannot be changed per-kind (applies to all indexed content)

### 2. Trigram tokenizer (`tokenize = 'trigram'`)

Indexes all 3-character subsequences. Enables substring matching without wildcards.

Pros:
- Substring search works natively
- Language-agnostic

Cons:
- Much larger index size (every 3-char window is a token)
- Poor precision for short queries
- Not suitable as a primary tokenizer for relevance ranking

### 3. Porter + prefix index (`tokenize = 'porter unicode61'`, `prefix = '2,3'`)

Adds a prefix index on top of Porter stemming.

Pros:
- Stemming + fast prefix search
- Enables autocomplete-style queries

Cons:
- Slightly larger index
- Prefix index sizes compound with stemming

## Recommendation

Use `tokenize = "porter unicode61 remove_diacritics 2"` for both tables.
Porter gives English stemming, `unicode61` gives word-level Unicode
tokenization, and `remove_diacritics 2` folds diacritics including those on
non-alphabetic codepoints so "café" and "cafe" are interchangeable at
index- and query-time.

Add `prefix = '2,3'` only if prefix/autocomplete queries become a use case.

## Migration Path

FTS5 tokenizer cannot be altered in place. The migration requires:

1. Drop the existing virtual table
2. Recreate with the new tokenizer
3. Re-insert all rows from canonical data

Both tables already have rebuild infrastructure:
- `fts_nodes`: rebuilt from `chunks` table
- `fts_node_properties`: `rebuild_property_fts_in_tx()` in `admin.rs`

This should be a new schema version migration. The rebuild is transactional and the tables are derived state, so no data loss risk.

```sql
-- Migration (SchemaVersion N)
DROP TABLE IF EXISTS fts_nodes;
CREATE VIRTUAL TABLE fts_nodes USING fts5(
    chunk_id UNINDEXED,
    node_logical_id UNINDEXED,
    kind UNINDEXED,
    text_content,
    tokenize = "porter unicode61 remove_diacritics 2"
);

DROP TABLE IF EXISTS fts_node_properties;
CREATE VIRTUAL TABLE fts_node_properties USING fts5(
    node_logical_id UNINDEXED,
    kind UNINDEXED,
    text_content,
    tokenize = "porter unicode61 remove_diacritics 2"
);
-- Then repopulate both tables from canonical sources.
```

## Related Issues

This note covers tokenization only. Other FTS5 improvements identified in the same audit:

- **No `ORDER BY rank`**: LIMIT truncates by insertion order, not relevance. Most impactful FTS5 correctness issue.
- **No `content=` external content table**: `fts_nodes` duplicates text already stored in `chunks.text_content`.
- **No `bm25()` weighting**: No per-column weight tuning.
