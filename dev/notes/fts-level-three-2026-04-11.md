# FTS Level Three Research Note

Date: 2026-04-11

## Purpose

Summarize academic and systems research relevant to the next step beyond
FathomDB's current property FTS design, specifically for nested object
key/value search over structured JSON stored in SQLite and indexed with FTS5.

This note is motivated by the known gap tracked in issue `#36`: object-typed
property values are currently skipped, and per-field weighting is not available.

## Current FathomDB Baseline

FathomDB property FTS today is a declared-path projection model:

- schemas declare an ordered list of simple dot-notation JSON paths
- write-time extraction reads those paths from node properties
- extracted values are concatenated into one `text_content` field
- one derived FTS row is stored per active logical node in
  `fts_node_properties`
- object values are skipped
- arrays of scalars are flattened
- `text_search(...)` unions chunk-backed FTS and property-backed FTS

Relevant repo references:

- [docs/guides/property-fts.md](/home/coreyt/projects/fathomdb/docs/guides/property-fts.md:34)
- [docs/guides/property-fts.md](/home/coreyt/projects/fathomdb/docs/guides/property-fts.md:161)
- [dev/design-structured-node-full-text-projections.md](/home/coreyt/projects/fathomdb/dev/design-structured-node-full-text-projections.md:124)
- [dev/schema-declared-full-text-projections-over-structured-node-properties.md](/home/coreyt/projects/fathomdb/dev/schema-declared-full-text-projections-over-structured-node-properties.md:91)

The resulting model is intentionally narrow and works well for a small set of
known scalar fields. It is not well suited to open-ended nested objects or
ranking that depends on field importance.

## SQLite Constraints And Opportunities

SQLite's JSON and FTS features matter directly here:

- SQLite stores JSON as text, and newer versions also allow JSONB blobs for a
  faster internal representation.
- `json_tree()` recursively walks nested JSON and emits one row per element,
  including full path information.
- FTS5 supports multiple indexed columns, `bm25()` column weighting, and column
  filters in queries.
- FTS5 also allows custom auxiliary functions in C if ranking needs to go
  beyond built-in `bm25()`.

These capabilities are important because they mean nested-object indexing does
not require abandoning SQLite. The existing engine can stay on SQLite and still
support deeper semistructured retrieval models.

Primary references:

- SQLite JSON1: https://sqlite.org/json1.html
- SQLite FTS5: https://sqlite.org/fts5.html

## Research Framing

There is less mature academic literature specifically on SQLite plus JSON FTS
than there is on XML and semistructured retrieval. But for this problem, the
transfer is strong:

- FathomDB node properties are nested labeled trees.
- Nested JSON object search is structurally the same class of problem as XML
  keyword search and semistructured document retrieval.
- The important research themes are structure-aware indexing, path-aware
  ranking, elimination of spurious subtree matches, and weighted fields.

This note therefore treats the XML and semistructured retrieval literature as
the closest academic analogue to "nested object KV search in JSON documents".

## Main Findings

### 1. Recursive descendant-value indexing is the next logical step

The safest improvement over the current design is:

- when a declared path resolves to an object, recursively index descendant
  scalar leaves instead of skipping the object

This is the direct analogue of structure-aware semistructured indexing. It
preserves the current declared-schema model while broadening extraction from
"known leaf only" to "known subtree root". It also aligns naturally with
SQLite's `json_tree()` traversal model.

This is the lowest-risk level-three improvement because it does not force a new
query surface and does not require general runtime JSON search.

### 2. Value-only indexing is not enough for true KV search

If the goal is actual nested object key/value search, not just "search all text
under this object", then indexing leaf values alone is insufficient.

Research on semistructured retrieval consistently treats metadata and structure
as ranking signals:

- field names matter
- path labels matter
- subtree boundaries matter

For FathomDB, that means a better design should preserve some combination of:

- object path
- full leaf path
- key tokens
- scalar value text

Instead of only materializing a single concatenated text blob.

### 3. Whole-node flattening increases spurious matches

The XML keyword-search literature repeatedly shows that large flattened result
regions produce noisy matches because terms can match far apart in the tree with
weak semantic cohesion.

Applied to FathomDB, one FTS row per entire node is acceptable for small entity
records, but it becomes weaker as nested payloads get larger or more irregular.

Better result granularity usually comes from indexing the smallest meaningful
object scope:

- one row per nested object
- or one row per declared object scope
- with hits mapped back to the parent node

This reduces accidental co-matching across unrelated nested branches.

### 4. Field-aware weighting is the standard fix for ranking quality

The core academic model here is BM25F: fielded BM25 with per-field weights.

That maps cleanly onto the problem in issue `#36`. If a match in
`$.action_name` should matter more than a match in `$.payload.notes`, then the
index needs distinct searchable fields or scopes. Once everything is collapsed
into one `text_content` column, that weighting signal is gone.

SQLite FTS5 already supports the mechanics needed for this direction:

- multiple text columns
- per-column `bm25()` weights
- column filters in MATCH expressions

So the gap is not the backend engine. The gap is the current Fathom materialized
shape.

## Design Patterns Suggested By The Literature

### Pattern A: Subtree expansion at declared object paths

Definition:

- a schema may declare a path as recursive
- if that path resolves to an object, index all descendant scalar leaves under
  it in document order or stable path order

Benefits:

- minimal change to current admin API
- preserves schema-declared behavior
- directly addresses the current object-skipping limitation

Costs:

- still mostly value-centric
- still weak for field-aware ranking unless paths are preserved separately

### Pattern B: Path-aware normalized side table

Definition:

- derive a normalized table from `json_tree()` output
- store one row per searchable leaf or searchable object scope

Candidate shape:

```sql
CREATE TABLE node_property_terms (
    node_logical_id TEXT NOT NULL,
    object_path TEXT NOT NULL,
    fullkey TEXT NOT NULL,
    key_token TEXT,
    value_text TEXT,
    value_type TEXT NOT NULL
);
```

Then build FTS over selected textual columns.

Benefits:

- supports real key/value search
- preserves structural information for ranking and filtering
- creates a base for object-scoped results

Costs:

- more write amplification
- more derived-state rows
- more care needed for rebuild and diagnostics

### Pattern C: Fielded FTS rows instead of one text blob

Definition:

- replace or supplement `fts_node_properties(node_logical_id, kind, text_content)`
  with multiple FTS columns or multiple rows per scope

Examples:

- `title_text`
- `key_text`
- `body_text`
- `path_text`

Benefits:

- enables BM25F-like weighting through FTS5 `bm25()`
- supports field-scoped query syntax later if desired
- preserves ranking headroom without changing storage engines

Costs:

- schema migration
- query compiler work
- a more opinionated indexing model

### Pattern D: Object-scoped retrieval with parent-node resolution

Definition:

- index nested object scopes separately
- return parent node as the public result
- optionally keep matched object path for diagnostics or future snippets

Benefits:

- reduces spurious whole-node matches
- creates a better basis for future explanations, snippets, or highlighting

Costs:

- more complex result resolution
- may require tie-breaking or score aggregation across multiple object hits in
  one node

## Recommended Direction For FathomDB

### Short term

Add an opt-in recursive extraction mode for declared object paths.

This directly solves the current gap with the smallest design change:

- keep `register_fts_property_schema(...)`
- add a path-level or schema-level flag for recursive subtree extraction
- when a declared path resolves to an object, walk descendants and extract
  scalar leaves

This should be treated as the minimum viable "level three" improvement.

### Medium term

Introduce a normalized derived representation built from `json_tree()` output.

This is the cleanest path to actual nested object KV search and reduces pressure
to overload one concatenated FTS document with multiple semantics.

Recommended invariants:

- canonical node data remains unchanged
- normalized nested-search rows remain derived and rebuildable
- rebuild/repair flows stay consistent with current projection discipline

### Long term

Move to fielded FTS materialization and weighted ranking.

If ranking quality matters, especially for mixed fields such as names, labels,
notes, and arbitrary payload text, BM25F-style weighting is the academically
sound path. SQLite FTS5 is already capable of supporting this once the stored
shape carries the field distinctions forward.

## What Not To Do

### Avoid raw `json.dumps(...)` indexing as the main solution

Serializing whole objects to plain text is operationally easy but loses nearly
all structure:

- keys and values are mixed without semantics
- ranking cannot distinguish field importance
- spurious matches increase
- future path-aware filtering gets harder

This can be acceptable as a temporary stopgap, but the literature does not
support it as a strong long-term retrieval design.

## Concrete Implications For FathomDB

The academic literature does not suggest replacing SQLite. It suggests changing
the shape of the derived search representation.

For FathomDB, that means:

1. Keep canonical nodes as they are.
2. Keep nested search as derived state.
3. Use SQLite JSON traversal to materialize structural search rows.
4. Use FTS5 multi-column scoring where field importance matters.
5. Prefer object-scoped or path-aware indexing over whole-node flattening as
   nested payloads become less regular.

## Suggested Internal Terminology

If this evolves into a design track, useful terms would be:

- "recursive property FTS"
- "path-aware property FTS"
- "object-scoped property FTS"
- "fielded property FTS"

These describe progressively stronger semantics without implying a new user
query surface.

## References

SQLite and implementation substrate:

- SQLite JSON1 documentation: https://sqlite.org/json1.html
- SQLite FTS5 documentation: https://sqlite.org/fts5.html

Field weighting and semistructured retrieval:

- Robertson, Zaragoza, Taylor. "Simple BM25 extension to multiple weighted
  fields." CIKM 2004. DOI: https://doi.org/10.1145/1031171.1031181
- Amer-Yahia et al. "XML search languages, INEX and scoring." SIGMOD Record.
- Theobald, Schenkel, Weikum. "TopX: efficient and versatile top-k query
  processing for semistructured data." VLDB Journal. DOI:
  https://doi.org/10.1007/s00778-007-0072-z

Structure-aware and result-quality work for semistructured keyword search:

- Li et al. "SAIL: structure-aware indexing for effective and progressive top-k
  keyword search over XML documents." Information Sciences. DOI:
  https://doi.org/10.1016/j.ins.2009.06.025
- Liu et al. "Ranking friendly result composition for XML keyword search."
- Lee et al. "Structural consistency: enabling XML keyword search to eliminate
  spurious results consistently." VLDB Journal. DOI:
  https://doi.org/10.1007/s00778-009-0177-7
- Liu et al. "Semantic relevance ranking for XML keyword search." Information
  Sciences. DOI: https://doi.org/10.1016/j.ins.2011.12.011

Metadata and path-aware search:

- Extending keyword search to metadata on relational databases. DOI:
  https://doi.org/10.1109/INGS.2008.14
- Path-aware keyword search approaches over semistructured documents, including
  path-filtered ranking variants. DOI:
  https://doi.org/10.1108/IJWIS-04-2015-0013

JSON-specific recent work:

- Dyreson, Shatnawi, Bhowmick, Sharma. "Temporal JSON Keyword Search." DOI:
  https://doi.org/10.1145/3654980

## Bottom Line

The research-backed answer to nested object KV search in FathomDB is not
"index more text in one blob". It is:

- recursive extraction for immediate coverage
- path-preserving derived state for real KV semantics
- fielded or scope-aware FTS materialization for ranking quality

SQLite plus FTS5 is sufficient for this direction. The key change is the shape
of the derived projection, not the storage engine.
