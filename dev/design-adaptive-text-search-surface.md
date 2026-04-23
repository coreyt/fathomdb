# Design: Adaptive Text Search Surface

## Status

Proposed.

No backwards compatibility is required. Existing `text_search()` semantics,
result shapes, and documentation may be changed directly to the new design.

## Purpose

Define the next search-surface design for `fathomdb` around four coherent
changes:

1. a single adaptive `text_search()` entry point
2. a first-class `SearchHit` result surface with score and source metadata
3. recursive property extraction for declared object paths
4. a narrow `fallback_search(strict, relaxed)` helper

This design replaces the earlier idea of splitting the public API into
multiple `text_search_*` methods. The database, not the client, should own the
default retrieval policy for text search.

## Problem

`fathomdb` already has a unified FTS entry point in name: `text_search()`
searches chunk-backed and property-backed text through one query surface.

That is not yet sufficient for real application search workloads.

The current design still leaves too much burden on callers:

- `text_search()` exposes a constrained safe syntax, but not a strong retrieval
  policy
- result rows expose matched nodes, but not enough search semantics to rank,
  debug, or explain results well
- property FTS is too shallow for nested payload-heavy nodes
- common strict-then-relaxed retrieval patterns must be rebuilt in SDK or
  application code

The result is that application code still has to compensate for a search
surface that is nominally unified but operationally incomplete.

## Design Goals

1. Keep one obvious default text-search entry point.
2. Make the engine responsible for safe, useful default search behavior.
3. Expose enough search metadata for applications to make informed decisions
   without reconstructing FTS behavior themselves.
4. Expand property FTS to cover declared nested structures better, while
   preserving the current derived-state and rebuild discipline.
5. Add a bounded best-effort search helper without committing to full general
   query branch composition.
6. Preserve cross-language parity across Rust, Python, and TypeScript.
7. Avoid changing backup/export format unless required by stored derived state.

## Non-Goals

1. Do not add multiple primary `text_search_*` entry points.
2. Do not expose raw FTS5 syntax as the default search surface.
3. Do not add full query `union()` / branch-composition semantics in this
   tranche.
4. Do not add a full staged search/traverse/hydrate language in this tranche.
5. Do not move domain-specific ranking or semantic filtering into the engine.
6. Seed-based relatedness ("find similar to this node") is a future
   specialized surface, not part of `text_search()`.

## Decision Summary

- Keep a single public `text_search(query, limit)` builder/API entry point.
- Redefine `text_search()` as an adaptive engine-owned search surface rather
  than a thin string-to-FTS lowering step.
- Introduce search-specific result types carrying hit metadata instead of
  returning only node rows for FTS-backed reads.
- Extend property FTS schemas to support recursive extraction at declared
  object paths.
- Add a narrow fallback helper for strict-vs-relaxed search behavior and use
  the same retrieval-policy machinery internally from `text_search()`.
- Treat all ranking/explanation surfaces as query-time behavior. Backup,
  export, restore, and integrity remain projection-centric rather than
  search-semantics-centric.

## Core Product Principle

**The engine owns fusion; the caller owns reranking.**

The caller should not have to choose among several search methods to get
reasonable recall.

The default `text_search()` should be the easy, reliable way to find text in
`fathomdb`. The engine should carry the complexity of:

- safe query interpretation
- strict-versus-relaxed retrieval policy
- unified chunk/property search
- result metadata production

Applications should still own domain-specific reranking, semantic post-filter
rules, and presentation logic.

"Adaptive" in this document means engine-owned strict-then-relaxed
retrieval policy — not learned query rewriting, not ML-driven re-ranking.
The engine picks the policy; the policy is fixed code.

**Scope: lexical-only in v1.** This tranche covers chunk FTS and property
FTS. When vector retrieval lands, it extends `SearchRows` as an additional
block under the same ranking model — `SearchHitSource::Vector`, same
`SearchHit` shape, same block precedence. Callers written against v1 see
vector hits appear without an API change.

**Concurrency contract.** Background writes (ingest, rebuild, property-FTS
repopulation) never block foreground `text_search()` reads. A stress-suite
assertion enforces this invariant.

## Current State

Today the search surface is shaped roughly like this:

- the public query builders expose a single `text_search(query, limit)` method
- the builder parses a constrained safe subset into `TextQuery`
- the compiler lowers that typed query into SQLite FTS5-safe `MATCH` syntax
- the FTS branch compiles to a UNION over chunk FTS and property FTS
- execution returns `QueryRows`, which contain nodes plus `was_degraded`, but
  no search-specific metadata
- property FTS concatenates declared values into a single flat text document

This design is safe and narrow, but it is not yet an application-friendly
search surface.

## Proposed Public Surface

### 1. Keep one primary `text_search()`

Rust:

```rust
QueryBuilder::nodes("Goal").text_search("ship quarterly docs", 10)
```

Python:

```python
engine.nodes("Goal").text_search("ship quarterly docs", limit=10)
```

TypeScript:

```ts
engine.nodes("Goal").textSearch("ship quarterly docs", 10)
```

This remains the default text-search API in every surface.

### 2. Add search-specific execution/result surface

Introduce a search-oriented result shape rather than overloading plain
`QueryRows` with ad hoc fields.

Recommended core types:

```rust
pub enum SearchHitSource {
    Chunk,
    Property,
    /// Reserved for future vector retrieval. No v1 code path emits this
    /// variant. It is exported now so vector hits can land as an additive
    /// change rather than a wire-format break across Rust, Python, and
    /// TypeScript.
    Vector,
}

pub enum SearchMatchMode {
    Strict,
    Relaxed,
}

pub struct SearchHit {
    pub node: NodeRow,
    pub score: f64,
    pub source: SearchHitSource,
    pub match_mode: SearchMatchMode,
    /// Highlighted excerpt. For chunk hits, produced from FTS5
    /// `snippet()`. For property hits, produced from the matched leaf
    /// in the property blob, trimmed to a default window (~200 chars).
    /// `None` for hits that cannot produce a snippet (e.g. future
    /// vector hits).
    pub snippet: Option<String>,
    /// Canonical write time of the underlying node, carried on the hit
    /// so callers can sort, filter, or display without a second read.
    pub written_at: Timestamp,
    /// Identifier of the derived row that produced this hit: the
    /// `fts_nodes.chunk_id` for chunk hits, the per-kind `fts_props_<kind>`
    /// row id for property hits. `None` when not applicable.
    pub projection_row_id: Option<String>,
}

pub struct SearchRows {
    pub hits: Vec<SearchHit>,
    pub was_degraded: bool,
    pub fallback_used: bool,
    pub strict_hit_count: usize,
    pub relaxed_hit_count: usize,
}
```

Recommended SDK parity:

- Python: `SearchHit`, `SearchRows`, enums for `SearchHitSource` and
  `SearchMatchMode`
- TypeScript: corresponding exported types

**Terminal shape: distinct builder type, single terminal name.**

`text_search(...)` does not return the same builder type it was called on.
It returns a dedicated `TextSearchBuilder`, whose `.execute()` is
statically typed to return `SearchRows`. The general query builder's
`.execute()` continues to return `QueryRows`. The terminal method name
(`execute` / `execute()` / `execute()`) is the same across builder types;
the type system routes the return.

This is the type-state pattern. It is the only shape that is clean in all
three languages, prevents a "wrong terminal" footgun, and scales to
additional specialized surfaces (vector, graph) without multiplying
terminal method names.

Language bindings:

- **Rust**: `NodeQueryBuilder::text_search(...) -> TextSearchBuilder`;
  `TextSearchBuilder::execute() -> SearchRows`.
- **Python**: `TextSearchBuilder` exposed from `python/fathomdb/_query.py`;
  `execute()` type-annotated to return `SearchRows`. No `Union` return
  types on any builder.
- **TypeScript**: `TextSearchBuilder` exported from
  `typescript/packages/fathomdb/src/query.ts`; `execute()` return type
  statically narrows to `SearchRows` the moment `.textSearch(...)` is
  called in a chain.

Rejected alternatives:

- **Polymorphic `execute()`** — one terminal name, runtime-varying return
  type. Forces `Union[QueryRows, SearchRows]` in Python / TS and a wrapper
  enum in Rust. Loses static guarantees and creates silent refactor
  hazards.
- **Second terminal verb (`execute_search()`)** — preserves static typing
  but multiplies terminal names, opens the door to an `execute_vector()`
  / `execute_graph()` proliferation, and leaves an undefined answer for
  "what does `.execute()` do on a text-search builder?"

### 3. Add narrow explicit helper

A bounded `fallback_search(strict, relaxed)` helper shares retrieval-policy
machinery with adaptive `text_search()`. It is documented in full in the
`fallback_search` section below.

## Adaptive `text_search()` Semantics

`text_search()` remains the default and recommended entry point. It should be
adaptive, inspectable, and engine-owned.

### High-level behavior

1. Parse caller input into the existing safe `TextQuery` subset.
2. Treat that parsed query as the **strict** interpretation.
3. Derive a **relaxed** interpretation when appropriate.
4. Execute strict search first.
5. If strict search is strong enough, return strict hits.
6. If strict search is weak or empty, run relaxed search and merge or replace
   under bounded policy.
7. Return `SearchHit`s that describe what happened.

### Strict interpretation

Strict mode is the existing safe typed query subset:

- bare terms
- quoted phrases
- implicit `AND`
- uppercase `OR`
- uppercase `NOT`

This remains the core safe search grammar. The difference is that it is no
longer the full user-facing retrieval policy by itself.

### Relaxed interpretation

Relaxed mode is a derived query intended to recover useful partial matches when
strict interpretation is too brittle.

Examples of relaxation policy:

- break implicit-AND term sets into per-term alternatives
- preserve quoted phrases when possible
- drop or soften exclusions when they overconstrain recall
- search the same chunk/property union surface

The exact relaxed form is engine-owned and does not become a second public
query language.

### Relaxed-branch cap

Relaxation is capped at **4 per-term alternatives**. A query with more
than 4 supported terms is truncated by token order when the relaxed
branch is derived, and the truncation sets `was_degraded = true` on the
resulting `SearchRows`. This cap is a named internal constant in the
search-policy layer, parallel to the fallback trigger `K`, so it can be
tuned later without a public API change.

The cap exists because relaxation expands an N-term implicit-AND into an
N-way OR over the chunk/property union surface; uncapped, a long query
against a conversation-history table could generate a ranker-dominating
candidate set. Capping keeps the relaxed branch bounded in cost without
changing its qualitative behavior.

### Fallback trigger policy

The trigger is a single integer threshold `K`: the engine runs the relaxed
branch if and only if the strict branch returned fewer than `min(limit, K)`
hits.

**Internally**, `K` is a named constant in the search-policy layer so the
trigger can be raised later without code changes.

**Externally in v1**, `K = 1`. This collapses the rule to "run relaxed only
when strict returned zero hits" — the simplest public contract to describe,
document, and test. Callers do not see `K`; they see a deterministic
zero-hits-only fallback.

Raising `K` later is a policy change, not an API change:

- block-based ranking (see Ranking Semantics) means relaxed hits always
  form a block below strict hits, so raising `K` never risks relaxed hits
  interleaving or outranking strict hits
- `fallback_used` and per-block counts on `SearchRows` already let clients
  tell how many strict and relaxed hits were returned, so UI that wants to
  distinguish "exact" from "related" keeps working without changes
- the test surface expands from two cases (strict-hit, strict-miss) to
  three (strict-fills-above-K, strict-fills-below-K, strict-miss) when the
  constant is raised, but the v1 test surface stays at two

The trigger policy must be visible in docs and test fixtures for v1 as
"relaxed runs only when strict returned zero hits." The internal `K` knob
should be a documented constant in the engine source, not a caller-facing
configuration option.

### Result metadata

Every returned `SearchHit` should make the search behavior inspectable:

- `score`: raw engine score used for ordering
- `source`: whether the hit originated from chunk text or property text
- `match_mode`: whether the hit came from the strict or relaxed branch
- `matched_path`: optional, initially for property hits only when available

This metadata is essential to making adaptive search debuggable and useful.

## Ranking Semantics

The engine exposes a stable ordering of hits, not an application-specific
relevance policy. Ranking is **block-based**: hits from the strict branch
form one ordered block, hits from the relaxed branch form the next. Within
a block, hits are ordered by raw engine score descending, with `logical_id`
lexicographic tiebreak. Future retrieval branches (e.g. vector) extend this
as additional blocks; the block concept does not change.

### Initial ranking requirements

1. FTS-backed search must not behave like unordered candidate selection.
2. Search results must expose score in the public result.
3. Scores are **ordering-only within a block**. Scores from different
   blocks — and, in future, from different retrieval backends such as
   vectors — are not on a shared scale. The engine does not normalize
   across blocks, and callers must not compare or arithmetically combine
   scores across blocks.
4. Merged strict/relaxed results must have deterministic precedence rules.
5. The block a hit belongs to must be visible to the client. In v1 the
   client reads `match_mode` on each `SearchHit` and the per-block counts
   (`strict_hit_count`, `relaxed_hit_count`) on `SearchRows`. Blocks appear
   in the `hits` vector contiguously and in precedence order, so a client
   that wants to render or rerank one block at a time can slice the vector
   by the counts without re-inspecting each hit.

### Merge policy

- strict hits rank ahead of relaxed hits, always, regardless of raw score
- duplicates are deduped by logical ID
- for duplicate logical IDs, the winning branch is selected by:
  1. strict over relaxed
  2. higher score within the same mode
  3. fixed source priority (chunk > property; future: vector)
  4. branch declaration order as the final deterministic tiebreak
- within a block, hits are ordered by score descending, then `logical_id`
  ascending

This remains intentionally simple and does not require score normalization
across blocks. If a future caller needs a globally ranked list (strict and
relaxed interleaved by quality), that is an additive change — e.g.
reciprocal-rank fusion behind an opt-in flag — and does not break the v1
contract.

## Tokenization

The default tokenizer for all text search — chunk FTS and property FTS — is
FTS5 `unicode61` with `remove_diacritics 2`, layered with the FTS5 `porter`
stemmer.

### Rationale

- **`unicode61`**: word-level tokenization with full Unicode normalization.
  Robust across languages, handles mixed scripts, and matches the shape of
  the queries real clients produce.
- **`remove_diacritics 2`**: strips diacritics including those on
  non-alphabetic codepoints. "café" and "cafe" are interchangeable at
  index-time and query-time with no caller involvement.
- **`porter`**: English-language stemming so "ship", "ships", and
  "shipping" collapse to a shared stem. This is the main recall lever for
  vague-cue queries and removes the need for the relaxed branch to handle
  simple morphology.

### Implications

- Case insensitive by construction. Callers do not need to lowercase
  inputs.
- Phrase search still works; quoted phrases match against the stemmed,
  normalized stream.
- Recall is strongly English-biased. Non-English stemming is out of scope
  for v1; index behavior on non-English text remains correct (unicode61 +
  diacritic folding) but does not benefit from stem collapsing.
- Index format is fixed by this choice. Changing the tokenizer later is a
  full FTS rebuild, not a migration.

### Scope

Trigram and other substring-oriented tokenizers were considered and
rejected as defaults: index bloat (roughly 3–5x), noisier BM25 ranking
dominated by short common trigrams, ugly snippet boundaries, and
significant overlap with the relaxed branch already in this design. A
per-property-schema tokenizer override — e.g. trigram for identifier-shaped
fields like URLs, usernames, and file paths — is a deliberate non-goal for
this tranche and may be added later without breaking the default.

## SearchHit Result Design

### Why a dedicated type

FTS-backed results are not just node rows. They carry retrieval semantics that
plain node reads do not.

A dedicated `SearchHit` type keeps that distinction explicit and prevents
search metadata from being awkwardly bolted onto generic row types.

### Source metadata

`source` should initially distinguish:

- chunk-backed hit
- property-backed hit

This matters for:

- application-side ranking
- debugging
- UI display
- operator testing

### Path metadata

Path-level attribution ("which declared property leaf produced this match")
is **not** a field on the default `SearchHit`. It is a specialized-surface
concern exposed via an opt-in builder flag; see the Match Attribution
section.

Rationale: the field is not correctness-critical (dedup, ranking, snippets,
and provenance all work without it), it is not free to produce reliably,
and the callers who need it are specialized (domain rerankers, UI match
labels, operator debugging). Keeping it off the default surface lets the
simple path stay lean and lets the attribution mechanism be as careful as
it needs to be without warping the hot path.

## Recursive Property Extraction

### Current limitation

Property FTS currently works best for shallow scalar fields. Object values are
skipped, and extracted values are flattened into one text blob.

### Decision

Extend schema-declared property FTS to support recursive extraction for
declared object paths.

Recommended contract extension:

```rust
register_fts_property_schema(
    kind,
    [
        PropertyFtsPath::scalar("$.title"),
        PropertyFtsPath::recursive("$.payload"),
    ],
    separator,
)
```

Equivalent Python and TypeScript admin surfaces should exist.

If a narrower schema API is preferred, the same concept can be modeled with a
path-plus-mode record rather than bare strings.

### Extraction behavior

For recursive paths:

- if the path resolves to an object, walk descendant scalar leaves
- preserve stable path order
- include strings directly
- stringify numbers and booleans
- flatten arrays of scalars
- skip nulls and missing values
- emit a **position map** alongside the concatenated blob — a compact list
  of `(start_offset, end_offset, leaf_path)` entries, one per emitted leaf,
  produced during the same walk. The position map is derived state on the
  property-FTS row, rebuilt whenever the blob is rebuilt. It exists to
  support opt-in match attribution (see below) and is otherwise unused at
  query time.

### Extraction guardrails

Recursive extraction runs against caller-declared schemas, not arbitrary
payloads, but nested structures can still be large. Guardrails bound the
worst-case index size and walk time so property-FTS rebuilds remain
predictable:

- **`max_depth = 8`.** Walks terminate at this depth; deeper leaves are
  skipped.
- **`max_extracted_bytes = 65_536` per node.** When the emitted blob
  would exceed this, the walk stops and the excess leaves are dropped.
- **`exclude_paths: Vec<String>`** on the schema registration, optional.
  Matching subtrees are skipped entirely.

When a guardrail fires, the row is still indexed with whatever was
emitted; the node is not skipped, and `check_integrity()` does not flag
it. A per-rebuild stats record tracks how many rows hit each guardrail
so operators can tune schemas and exclude-path lists. Per-query results
do not surface this.

These limits are internal constants, tunable in engine source like the
fallback trigger `K` and the relaxed cap. Schema-level override is a
future extension if the defaults prove too tight.

### Concatenation separator

Leaves are joined by a separator that the `unicode61` tokenizer treats as
a **hard phrase break**, so FTS5 phrase queries cannot silently match
across leaf boundaries. The exact byte sequence is an implementation
detail, but the contract is fixed: no tokenized content may straddle two
leaves.

This makes phrase-match attribution unambiguous — a phrase match's
position range always lies entirely within one leaf — and it prevents the
concatenation from producing matches that would not exist in any
individual leaf.

### Storage-shape choice

This design does **not** require a full fielded/path-aware property-FTS
materialization yet.

The first step should remain compatible with the current derived-state model:

- property FTS stays rebuildable
- schema table remains canonical
- projection rows remain derived

However, recursive extraction should be implemented in a way that does not
block future path-aware materialization.

## Match Attribution (opt-in)

Path-level attribution is an **opt-in specialized surface**, not a field
on the default `SearchHit`. A caller that wants to know which declared
leaves produced a property match activates attribution at the query site
via a builder flag:

```rust
engine.nodes("KnowledgeItem")
    .text_search("quarterly docs", 10)
    .with_match_attribution()
    .execute()?
```

Python and TypeScript expose the equivalent flag on their
`TextSearchBuilder`. The default `text_search(...).execute()` path is
unchanged and pays no attribution cost.

### Result shape

When attribution is requested, each `SearchHit` carries a parallel
attribution record:

```rust
pub struct HitAttribution {
    /// Declared leaf paths that contributed to this hit, ordered by
    /// first-match offset in the property blob. Empty for chunk-only
    /// hits.
    pub matched_paths: Vec<String>,
}
```

`Vec<String>` rather than `Option<String>` is deliberate: a multi-term
query can legitimately match across leaves (e.g. title + body), and the
opt-in record should express that honestly. Richer attribution (per-path
matched terms, positions for debugging) is a future additive extension.

### Mechanism: position map + FTS5 match introspection

Attribution is **deterministic**, not post-hoc re-tokenization. The
mechanism has two halves.

**At index time** (paid unconditionally, during the same walk that
produces the blob): the recursive-extraction walk emits the position map
described under Recursive Property Extraction. One entry per leaf,
recording `(start_offset, end_offset, leaf_path)` in blob coordinates.
Stored on the property-FTS row as derived state, rebuilt whenever the
blob is rebuilt.

**At query time** (paid only when attribution is requested): after FTS5
returns a hit, the engine reads match positions for that hit using
FTS5's match-introspection functions (`matchinfo()` / `offsets()` /
equivalent). For each matched token, a binary search in the position map
resolves the token's offset to exactly one leaf path. Matched paths are
deduped, ordered by first-match offset, and returned on the
`HitAttribution` record.

### Correctness under each failure mode

- **Stemming (`porter`).** FTS5 match positions refer to offsets in the
  original indexed text, not the stemmed token stream. "shipping"
  matching query stem `ship` returns the original-text offset of
  "shipping", which the position map resolves to the correct leaf. No
  approximation.
- **Phrase queries.** FTS5 reports the phrase match as a contiguous
  position range. Because the leaf separator is a hard phrase break
  (see Concatenation separator), the range always lies entirely within
  one leaf. Lookup is unambiguous.
- **`NOT` clauses.** Negative clauses contribute no positive match
  positions; they only filter candidates. Nothing to attribute.
- **Multi-term AND spanning leaves.** Each term's positions resolve
  independently, yielding multiple leaves on `matched_paths`. This is
  the honest answer and is only expressible because the record uses a
  vector.
- **Relaxed branch.** Relaxed is a query rewrite against the same index.
  Position map and FTS5 match semantics are unchanged, so attribution
  is correct in both strict and relaxed modes. `match_mode` on the hit
  tells callers which branch fired; attribution is orthogonal.
- **Chunk hits.** Chunk hits have no leaf structure. `matched_paths` is
  empty for chunk-only hits; attribution callers that want chunk
  information use the chunk identifier on the hit directly.
- **Vector hits (future).** Vector hits carry no term-level positions.
  `matched_paths` is empty; if a nearest-leaf heuristic is ever wanted,
  it is an additive future extension.

### Cost profile

- **Index time**: unconditional. One position-map row per property-FTS
  row, produced during the same extraction walk. Budget: a few hundred
  bytes per 20-leaf node. Rebuilt with the blob; restore and integrity
  checks extend naturally.
- **Query time without attribution**: zero. The position map is not
  read.
- **Query time with attribution**: one match-introspection call per hit
  plus a binary search per matched term. For limit-10 results with a
  few query terms, trivial.

### Ruled out

These approaches were considered and rejected:

- **Re-running the query per leaf at attribution time.** Correct-ish,
  N× query cost per attributed hit, and drifts from blob-level
  tokenization under stemming.
- **Hand-rolled re-tokenization in attribution code.** Approximates
  `unicode61 + porter`; will silently drift and produce wrong
  attribution.
- **Storing only FTS5 columns for each declared root path, with no
  position map.** Resolves to the declared root ("matched in
  `$.payload`") but not to a leaf under recursive extraction.
  Insufficient alone; the position map supersedes it.
- **Regenerating the position map on the fly at attribution time** by
  re-running extraction in memory instead of persisting it. Lower
  index-size overhead, higher query-time cost, and opens a class of
  "regenerated map drifted from the stored blob" bugs. Reject for v1;
  revisit only if persisted-map storage ever becomes a real pressure.

## `fallback_search(strict, relaxed)` Helper

### Purpose

Provide explicit access to the bounded strict-vs-relaxed search pattern
without adding full query branch composition.

### Proposed scope

The helper should:

- accept one or two search shapes (`relaxed` may be absent; when absent,
  the helper is strict-only and shares the same merge, dedup, and result
  shape)
- execute strict first
- execute relaxed only when present and policy says to do so
- dedup and merge with deterministic precedence
- return `SearchRows`

It should not:

- allow arbitrary query-tree branching
- expose generic `union()` semantics
- become a general multi-stage query DSL

The strict-only mode (`relaxed=None`) exists to serve the dedup-on-write
pattern: callers that need "has any node already matched this strict
query?" use the same retrieval, result, and dedup surface as adaptive
`text_search()` rather than an ad hoc path.

### Relationship to adaptive `text_search()`

`text_search()` should use the same retrieval-policy machinery internally.

This keeps behavior consistent:

- default search is easy
- explicit fallback remains available when an advanced caller wants control

## Core Architecture Changes

### `fathomdb-query`

Required changes:

- keep `TextQuery` as the strict safe-grammar representation
- add a search-policy layer above `TextQuery` that can derive relaxed search
  plans
- teach the compiler to produce ranked search candidates and carry search
  metadata forward

Recommended additions:

- `SearchPlan`
- `SearchPolicy`
- `SearchBranch` with exactly two supported branches in v1: strict and relaxed

This should remain narrower than a generic query composition framework.

#### Filter composition

Filters compose with text search via the existing `filter_*` chain on
the builder. `TextSearchBuilder` exposes the same filter vocabulary as
the general query builder — `filter_kind_eq`, `filter_logical_id_eq`,
`filter_source_ref_eq`, `filter_content_ref_*`, `filter_json_*`. There
is no search-specific filter vocabulary, no separate `SearchFilters`
struct, and no positional filter arguments on `text_search(...)`.

This keeps Rust, Python, and TypeScript SDK surfaces symmetric — all
three already compose `text_search` and `filter_*` as peers in a single
step pipeline — and lets clients build filters conditionally from UI
state without dict gymnastics:

```python
q = engine.nodes("Item").text_search(query, 10)
if ui.type: q = q.filter_kind_eq(ui.type)
if ui.after: q = q.filter_json_timestamp_gt("$.captured_at", ui.after)
rows = q.execute()
```

#### FTS filter fusion

Filter composition alone is not enough. Today, when the driving table
is `FtsNodes` or `VecNodes`, `Filter` steps remain in the outer `WHERE`,
applied only *after* the FTS/vector `LIMIT` has already truncated
candidates. The test `fts_driver_keeps_json_filter_in_outer_where` in
`crates/fathomdb-query/src/compile.rs` codifies this behavior. The
consequence is that

```python
engine.nodes("Item").text_search("budget", 5).filter_kind_eq("Goal").execute()
```

fetches 5 raw `budget` matches and *then* filters for `kind = "Goal"`,
so the returned count can be anything from 0 to 5 even when the index
contains many more `budget`-and-`Goal` matches. Under an adaptive
search contract this is wrong; callers cannot trust `limit`.

The compiler gains an **FTS filter-fusion pass** that runs over `Filter`
steps following a search step and partitions them into:

- **fusable** predicates are injected into the `base_candidates` CTE so
  the FTS (or vector) candidate set is already narrowed before `LIMIT`
  applies
- **residual** predicates remain in the outer `WHERE`, unchanged from
  today

v1 eligibility:

- **fusable**: `filter_kind_eq`, `filter_logical_id_eq`,
  `filter_source_ref_eq`, `filter_content_ref_eq`,
  `filter_content_ref_not_null` — each of these maps to a column
  already present on the FTS/vector candidate rows
- **residual**: `filter_json_*` (text, bool, integer comparisons,
  timestamp comparisons) — these need `json_extract` on the node
  properties blob, which is not carried on the FTS or vector candidate
  rows

The partition is a single match statement in `compile.rs`; new fusable
filters are added by extending match arms.

**Vector-search alignment.** The fusion pass is written generically
over search-driven driving tables, not FTS-specifically. `VecNodes` has
exactly the same `LIMIT`-before-filter problem today, and the same v1
eligibility list applies: kind, logical_id, source_ref, and content_ref
filters fuse into vector `base_candidates` just as they do into FTS
`base_candidates`. Vector retrieval inherits filter fusion the moment
it wires in; no separate vec-path fusion work is required when vectors
ship. The `SearchHitSource::Vector` reservation and filter fusion
together mean a future vector wiring is an additive change.

**Tests.** Flip `fts_driver_keeps_json_filter_in_outer_where` to assert
the partition explicitly: json filters stay residual, kind filters
fuse. Add `fts_driver_fuses_kind_filter` proving the kind constraint
appears inside `base_candidates`, not the outer `WHERE`. When vector
retrieval lands, mirror both tests for `VecNodes`.

Promoting currently-residual filters (tags, captured timestamps,
pinned) to fusable is a later decision gated on promoting those fields
to columns on the FTS/vec candidate rows. It is not blocking v1 and
can be done additively — one new match arm per promoted field.

**Why a compile-layer pass instead of a search-builder filter surface.**
An earlier framing of this problem asked whether `text_search(...)`
should accept structured filter arguments (`kind`, `tags`, `after`,
`pinned`) or a separate `SearchFilters` struct. Research into the
current codebase and the primary client (Memex) showed that framing was
wrong. Filters already chain onto `text_search` today via the general
`filter_*` methods — the builder surface is not the problem. The
problem is entirely in `compile.rs`: when the driving table is
`FtsNodes` or `VecNodes`, filter predicates stay in the outer `WHERE`
and are applied *after* `LIMIT` has already truncated candidates.
Memex's client-side filter loops (`fathom_facade.py` line 339: "client-
side filtering for fields fathomdb doesn't filter natively") exist
because of that truncation, not because the builder API is unergonomic.
Moving filters into a separate search-specific vocabulary would have
added a second `Predicate` path, forced cross-SDK mirroring of the new
filter struct, and still left the `base_candidates` truncation bug
unfixed. Fixing `base_candidates` is the actual work; the existing
builder chain is the correct surface.

### `fathomdb-engine`

Required changes:

- add search-specific execution path(s) returning `SearchRows`
- stop treating FTS execution as “find logical IDs, then project nodes only”
- compute and propagate score and source metadata
- support deterministic strict/relaxed merging

`QueryPlan` / `explain()` should also be expanded or paralleled with a
search-specific explanation surface so operator tooling can inspect:

- strict branch SQL or plan
- whether fallback ran
- how many hits came from each source

The read-result diagnostics design should remain conceptually separate from
payload results, but the search result itself must expose enough metadata to be
useful.

### Rust Public Facade

Required changes:

- export `SearchHit`, `SearchRows`, and supporting enums
- add terminal methods or adaptive terminal return types in the Rust facade
- rework serde/wire payloads used by Python and TypeScript bindings

No backwards compatibility is required, so existing FTS result contracts may be
replaced rather than layered.

### Python Binding Layer

The Python bindings should remain thin and Rust-owned in semantics.

Required changes:

- update Rust `ffi_types.rs` to serialize/deserialize `SearchRows`
- add Python dataclasses/enums in `python/fathomdb/_types.py`
- update `python/fathomdb/_query.py` so `text_search()` produces the new
  result contract
- update package exports in `python/fathomdb/__init__.py`

Python API goal:

```python
rows = engine.nodes("Goal").text_search("ship blocked", limit=10).execute()
for hit in rows.hits:
    hit.node.logical_id
    hit.score
    hit.source
    hit.match_mode
```

`text_search()` returns a `TextSearchBuilder` whose `execute()` is
statically annotated to return `SearchRows`. The general query builder's
`execute()` continues to return `QueryRows`. No `Union` return types are
introduced on either builder.

### Python SDK Documentation

Required documentation changes:

- rewrite `text_search()` docs around adaptive behavior rather than a narrow
  syntax-only contract
- document `SearchHit` and `SearchRows`
- document fallback behavior, match modes, and source metadata
- document recursive property extraction in admin/property FTS guides

Existing docs that describe `text_search()` as a transparent UNION over chunk
and property FTS remain true but incomplete; they must be updated to describe
the new result and retrieval behavior.

### TypeScript SDK

The TypeScript SDK should mirror Python conceptually and closely in coverage.

Required changes:

- add `SearchHit`, `SearchRows`, and enums to `typescript/packages/fathomdb/src/types.ts`
- update `query.ts` terminal behavior for text-search queries
- update package exports in `index.ts`
- keep AST construction thin and Rust-aligned

TypeScript API goal:

```ts
const rows = engine.nodes("Goal").textSearch("ship blocked", 10).execute();
for (const hit of rows.hits) {
  hit.node.logicalId;
  hit.score;
  hit.source;
  hit.matchMode;
}
```

The SDK must not invent different adaptive-search semantics from Rust. It
should only surface what Rust defines.

## Consumer Documentation

The following documentation areas must be rewritten, not patched lightly:

1. query guide
2. text-query syntax guide
3. property FTS guide
4. query API reference
5. Python README/API examples
6. TypeScript README/API examples

Key documentation changes:

- `text_search()` is now described as adaptive safe search, not just a typed
  boolean subset
- `TextQuery` remains the strict interpretation grammar
- `SearchHit` is the public unit of search results
- `recursive` property extraction is a declared projection behavior
- `fallback_search()` is documented as a bounded helper, not a general query
  composition API

## Backup / Restore / Rebuild / Integrity

### Export / backup

No backup format change is required unless recursive path definitions require a
schema-table migration.

The critical principle remains:

- search rows are derived state
- schema/contract rows are canonical

`safe_export()` should continue to preserve canonical schema declarations and
not rely on exporting live derived FTS rows as authoritative state.

### Restore and rebuild

Restore, rebuild, and rebuild-missing should continue to treat FTS as derived
state. If recursive extraction is added to property schemas:

- rebuild must use the new extraction rules
- restore must regenerate property FTS using the same rules
- semantic checks should continue to validate projection presence/drift, not
  search ranking semantics

### Schema migration for recursive flag

Adding or removing a recursive path on an existing property FTS schema is
a schema registration operation that triggers an **eager, transactional
rebuild** of that kind's property-FTS rows in the same transaction. The
rebuild walks the affected nodes, regenerates the blob and position map
under the new extraction rules, and commits atomically.

Consequences:

- consistency is immediate; no caller ever sees a row mixing old and new
  extraction rules
- registration can take minutes on large kinds (bounded by kind size,
  not total DB size)
- registration exposes a progress signal so operators can monitor long
  rebuilds

Lazy mark-stale-and-rebuild-later and versioned co-existence were both
considered and rejected. Lazy staleness violates the derived-state
invariant that "derived rows reflect the current schema." Versioned
co-existence is over-engineered for the first users and multiplies the
test matrix. If long rebuild windows become a real pressure, incremental
background rebuild can be added additively without breaking the
eager-rebuild contract.

### Integrity semantics

`check_integrity()` and `check_semantics()` should remain focused on
projection-health questions:

- missing rows
- stale rows
- drifted extraction content

They should not attempt to validate adaptive search result quality or fallback
policy.

## Testing Strategy

The current test matrix is not enough. Search tests must stop asserting mostly
row counts and start asserting search semantics.

### Core unit tests

Add tests for:

- strict query parsing
- relaxed-query derivation
- fallback trigger policy
- score ordering
- strict-versus-relaxed dedup precedence
- chunk-versus-property source metadata
- recursive property extraction over nested objects and arrays
- position-map emission during extraction (one entry per leaf, correct
  offsets, stable order)
- the leaf separator is a hard phrase break under `unicode61 + porter`
  (phrase queries cannot straddle leaf boundaries)
- match attribution under stemming, phrase queries, `NOT` clauses,
  multi-term AND spanning leaves, and relaxed-branch rewrites
- default `text_search()` pays no attribution cost (position map not
  read when `with_match_attribution()` is not set)

### Engine integration tests

Add tests for:

- `text_search()` returns `SearchRows`
- fallback runs only when policy requires it
- duplicate logical IDs across strict/relaxed and chunk/property sources are
  merged deterministically
- restore/rebuild preserve searchability under recursive extraction

### Cross-language parity tests

Cross-language scenarios must expand beyond `count` / `expect_min_count`.

Required new assertions:

- ordered hit logical IDs
- hit source
- match mode
- whether fallback was used, and per-block counts on `SearchRows`
- attribution results (`matched_paths`) when the query was run with
  `with_match_attribution()`, covering stemming, phrase, multi-term AND
  across leaves, and relaxed-branch cases

Python and TypeScript driver fixtures must both validate the richer wire
payload shape.

### Harness scenarios

The Python and TypeScript harnesses should gain explicit search semantics
scenarios:

- strict hit only
- strict miss, relaxed recovery
- mixed chunk/property hits
- recursive nested payload search
- rebuild / restore after recursive-property schema use

### Stress testing

Stress tests should continue to assert correctness under concurrency and load,
but add search-specific invariants:

- no panics or deadlocks under adaptive search workloads
- deterministic hit ordering under repeated runs
- fallback behavior remains stable under repeated concurrent reads
- property FTS rebuild plus search remains correct after heavy writes

## Migration / Rollout

No backwards compatibility is required, so the rollout should favor coherence
over bridging.

Recommended sequence:

1. redesign core result types and query execution for FTS-backed reads
2. add adaptive search policy and narrow fallback helper
3. add recursive property extraction and rebuild support
4. update Python and TypeScript bindings/SDKs
5. rewrite consumer docs
6. replace cross-language and stress fixtures with the new search assertions

## Resolved Decisions

- **Tokenizer**: `unicode61` with `remove_diacritics 2` plus the `porter`
  stemmer. See the Tokenization section.
- **Score contract**: raw engine score, ordering-only within a block, no
  cross-block comparability. See the Ranking Semantics section.
- **Fallback visibility**: `fallback_used` is an explicit field on
  `SearchRows`, alongside per-block counts, so clients do not have to infer
  it from per-hit `match_mode`.
- **Fallback trigger**: threshold-based (`min(limit, K)`) internally, with
  `K = 1` in v1 so the public contract is zero-hits-only. Raising `K` later
  is a policy change, not an API change. See the Fallback trigger policy
  section.
- **Terminal shape**: `text_search(...)` returns a dedicated
  `TextSearchBuilder` whose `execute()` is statically typed to return
  `SearchRows`. The general query builder's `execute()` continues to
  return `QueryRows`. One terminal name, distinct builder types. See the
  "Add search-specific execution/result surface" section.
- **Path attribution**: dropped from the default `SearchHit`. Exposed as
  an opt-in specialized surface via `with_match_attribution()` on
  `TextSearchBuilder`, producing a `HitAttribution` record with
  `matched_paths: Vec<String>`. The mechanism is a position map emitted
  at index time (during the same recursive-extraction walk as the blob)
  plus FTS5 match introspection at query time. Deterministic under
  stemming, phrases, `NOT`, multi-term AND across leaves, and the
  relaxed branch. See the Match Attribution section.
- **`SearchHit` payload**: carries `snippet`, `written_at`, and
  `projection_row_id` in addition to `node`, `score`, `source`, and
  `match_mode`, so callers can sort, display, or trace without a second
  read.
- **`SearchHitSource::Vector` reserved**: the enum exports a `Vector`
  variant now, even though no v1 code path emits it, so that vector
  retrieval can land as an additive change rather than a wire-format
  break across Rust, Python, and TypeScript.
- **Relaxed-branch cap**: 4 per-term alternatives, internal constant,
  excess truncated by token order and marked `was_degraded`. See
  Relaxed-branch cap.
- **Recursive extraction guardrails**: `max_depth = 8`,
  `max_extracted_bytes = 64 KiB per node`, optional `exclude_paths` on
  the schema. Rows that hit a guardrail are still indexed with what was
  emitted; per-rebuild stats track the counts. See Extraction
  guardrails.
- **`fallback_search(relaxed=None)`**: strict-only mode is supported and
  serves the dedup-on-write pattern, with the same merge, dedup, and
  result shape as the two-shape case.
- **Concurrency contract**: background writes never block foreground
  `text_search()` reads; one stress-suite assertion enforces it. See
  Core Product Principle.
- **Recursive-flag schema migration**: registering a schema with a new
  recursive path triggers an eager, transactional rebuild of that
  kind's property-FTS rows. Lazy mark-stale and versioned co-existence
  were rejected. See Schema migration for recursive flag.
- **Filter composition**: filters compose with text search via the
  existing `filter_*` chain on `TextSearchBuilder`. No search-specific
  filter vocabulary, no positional filter arguments on
  `text_search(...)`, no separate `SearchFilters` struct. Keeps SDK
  surfaces symmetric and supports conditional filter construction from
  UI state. See Filter composition.
- **FTS filter fusion**: the compiler gains a fusion pass that
  partitions `Filter` predicates following a search step into fusable
  (`kind_eq`, `logical_id_eq`, `source_ref_eq`, `content_ref_*`) and
  residual (`json_*`) sets, and injects fusable predicates into the
  `base_candidates` CTE so `LIMIT` applies after filtering. The pass
  is generic over search-driven driving tables, so `VecNodes`
  inherits the same fusion behavior when vector retrieval wires in —
  no separate vec-path work required. Promoting currently-residual
  filters (tags, timestamps, pinned) to fusable is an additive future
  decision gated on column promotion. See FTS filter fusion.

## Open Questions

_None currently open._ All design decisions for this tranche have been
resolved above; see the Resolved Decisions section.

## Done When

### Caller-visible acceptance

A single `text_search(query, limit).execute()` call returns deterministically
ranked `SearchRows` with `score`, `source`, `match_mode`, `snippet`,
`written_at`, and `projection_row_id` on every hit, with no caller-side
branch on backend, in Rust, Python, and TypeScript.

### Implementation done-when

- `text_search()` remains the single primary text-search entry point
- FTS-backed reads return a search-specific result surface with score and
  source metadata
- property FTS supports recursive extraction for declared object paths
- a bounded strict-vs-relaxed fallback helper exists and shares machinery with
  adaptive `text_search()`
- Rust, Python, and TypeScript all expose the same semantics
- docs describe adaptive search rather than only strict syntax lowering
- backup/restore/rebuild remain correct under the new projection rules
- cross-language, harness, integration, and stress tests assert the new search
  semantics explicitly
