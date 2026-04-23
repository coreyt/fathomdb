# Addendum 1: Unifying Text and Vector Into One Adaptive Retrieval Surface

## Status

Proposed addendum to `design-adaptive-text-search-surface.md`.

No backwards compatibility is required. The primary retrieval entry point
defined in the main body (`text_search`) may be renamed or subsumed by the
surface defined here.

## Purpose

The main body of the adaptive text-search design moves `fathomdb` from a thin
text-lowering API toward an adaptive, engine-owned text search surface with
first-class hit metadata, strict/relaxed fallback, and a `SearchHit` /
`SearchRows` result shape. That work is correct and stands unchanged.

What the main body leaves unresolved is how vector retrieval fits the same
client contract. `SearchHitSource::Vector` is reserved (Decision Summary, main
body), but the client-facing entry point is still text-named, the result shape
is still text-centered, and no planner exists that could decide between
lexical and semantic retrieval on the client's behalf.

This addendum resolves that gap by generalizing the main body's adaptive
**text** search surface into an adaptive **retrieval** surface, without
changing the main body's direction on result shapes, ranking, fallback policy,
recursive property extraction, match attribution, or filter composition.

The core principle is:

> The client should ask for relevant information. The engine should decide
> whether to use text retrieval, vector retrieval, or both.

## Problem

Four gaps remain after the main body:

1. **Mechanism choice leaks to the client.** Applications must still choose
   between `text_search()` and a future `vector_search()`. That is a
   retrieval-mechanism decision, not an information-retrieval decision, and it
   forces application code to reason about engine internals.
2. **`SearchHit` is implicitly text-first.** `match_mode` (Strict/Relaxed) and
   `matched_path` are natural for lexical hits but have no equivalent for
   vector hits, which need distance/similarity instead. Adding vector
   retrieval underneath the current shape would either leave fields
   nonsensically empty or pressure the type into ad-hoc extensions.
3. **Fusion is incomplete.** The main body defines block-based merging for
   strict vs. relaxed **text** branches. It does not say how vector hits
   participate in the same ordered result vector.
4. **Degradation is siloed.** Vector retrieval already has capability-miss
   semantics today. The main body's `was_degraded` flag should cover vector
   degradation too, under a single unified retrieval result family, not as a
   parallel concept.

## Decision Summary

- Introduce a single primary client-facing retrieval entry point:
  `search(query, limit)`. It replaces `text_search(query, limit)` as the
  default recommendation.
- Keep `text_search(...)` and a future `vector_search(...)` as **advanced
  mechanism-specific overrides**, not the main integration path.
- Promote `SearchHit` / `SearchRows` to the public result surface for **all
  ranked retrieval**, lexical or semantic.
- Introduce a bounded, engine-owned **retrieval planner** that selects, runs,
  and fuses retrieval branches across modalities.
- Keep strict/relaxed text fallback as a text-modality policy inside the
  broader planner, unchanged from the main body.
- Vector participation in v1 is conservative: vector runs only when text
  retrieval produces nothing, on the same zero-hits-only trigger the main body
  uses for relaxed fallback.
- Retain block-based precedence fusion across modalities. Do not introduce
  cross-modality score normalization or weighted hybrid ranking in this
  tranche.
- Preserve all other resolved decisions in the main body verbatim: tokenizer,
  terminal shape (type-state builder with `execute()`), `SearchHit` payload
  fields, recursive extraction guardrails, match attribution, filter
  composition, FTS filter fusion, schema migration.

## Design Goals

1. Give applications one obvious default retrieval entry point that is not
   mechanism-named.
2. Keep the engine responsible for deciding how to retrieve effectively.
3. Unify ranked retrieval results across text and vector modalities under one
   type family.
4. Preserve text-specific behavior (strict/relaxed fallback, recursive
   extraction, attribution) without forcing its vocabulary onto vector hits.
5. Preserve vector-specific behavior (capability degradation, distance
   semantics) without forcing its vocabulary onto text hits.
6. Keep the planner bounded — a small fixed set of engine-owned branches, not
   a general query algebra.
7. Preserve cross-language parity across Rust, Python, and TypeScript,
   identical to the parity requirements of the main body.

## Non-Goals

1. Do not introduce a general hybrid query language or `union()` branch
   composition.
2. Do not expose caller-tunable modality weights in v1.
3. Do not force all applications into hybrid retrieval. Mechanism-specific
   overrides remain available.
4. Do not move application-specific ranking or semantic post-filtering into
   the engine.
5. Do not attempt cross-modality score normalization in v1.
6. Do not change the main body's resolved decisions on result payload fields,
   filter composition, ranking within a block, fallback trigger semantics, or
   schema migration.

## Public Surface

### Primary retrieval entry point

The primary client-facing retrieval method is `search`. It is **not**
mechanism-named.

Rust:

```rust
engine.nodes("Goal").search("ship quarterly docs", 10).execute()?
```

Python:

```python
engine.nodes("Goal").search("ship quarterly docs", limit=10).execute()
```

TypeScript:

```ts
engine.nodes("Goal").search("ship quarterly docs", 10).execute();
```

`search(query, limit)` returns a dedicated `SearchBuilder` whose `.execute()`
is statically typed to return `SearchRows`. This is the same type-state
pattern the main body adopts for `text_search(...)` — one terminal verb name,
distinct builder types, static return narrowing. The rationale from the main
body's "Add search-specific execution/result surface" section applies
identically.

### Advanced mechanism-specific overrides

Callers with a hard reason to pin the modality may use explicit overrides:

- `text_search(query, limit)` — lexical only, as defined by the main body
- `vector_search(query, limit)` — semantic only, future

Both are modeled as dedicated builders returning `SearchRows`, parallel to
`SearchBuilder`. They are documented as advanced controls, not as the main
product story.

### Relationship to `text_search()` in the main body

`search(query, limit)` becomes the primary API. `text_search(query, limit)` is
retained as a documented mechanism-specific override with identical semantics
to the main body's adaptive text search. Because no backwards compatibility is
required, the product-story default in docs is `search`; `text_search` is
moved into an "advanced retrieval controls" section.

The main body's `TextSearchBuilder`, its filter composition rules, its
`with_match_attribution()` flag, its strict/relaxed fallback, its recursive
extraction behavior, and its `SearchHit` payload fields all remain — they
become the text-modality behavior executed by the planner underneath
`search()` and directly exposed when the caller invokes `text_search()`
explicitly.

## Retrieval Planner Model

The engine owns a bounded retrieval planner. It is not a query composition
language.

### Conceptual stages

1. **Text strict.** Parse the query into the safe `TextQuery` subset (main
   body, Strict interpretation). Run strict text retrieval against the
   chunk/property union.
2. **Text relaxed.** If the strict branch satisfies the main body's fallback
   trigger (`K = 1` in v1: strict returned zero hits), derive and run the
   relaxed interpretation under the main body's relaxed-branch cap and policy.
3. **Vector.** If text retrieval returned zero hits total after stages 1–2,
   and vector capability is available, run vector retrieval against the same
   kind filter and limit.
4. **Fusion.** Collect all candidates, dedupe by `logical_id`, assemble into
   block-ordered `SearchRows`.

Stages 1 and 2 are governed entirely by the main body's existing policy —
this addendum does not alter them. Stages 3 and 4 are the new work.

### Why this is the right abstraction

- The branch set is small, fixed, and engine-defined.
- Clients do not compose branches explicitly.
- Each branch is independently testable.
- The response model is unified across all branches.
- It is the same spirit as the main body's rejection of full generic
  query-branch composition.

## Retrieval Policy

### v1 default policy

1. Run text-strict.
2. If text-strict returned zero hits, run text-relaxed.
3. If text retrieval (strict + relaxed combined) returned zero hits, and
   vector capability is available, run vector retrieval.
4. Fuse candidates under the block precedence rules below.

This trigger intentionally mirrors the main body's `K = 1` fallback decision:
vector, like relaxed, is zero-hits-only in v1. It is the simplest public
contract to describe, document, and test. Raising the vector trigger is a
future policy change, not an API change, and is gated on the same block-based
ranking that makes raising `K` safe for the relaxed branch.

### Future policy room

Later versions may add:

- underfilled-result triggers instead of zero-hit-only triggers
- query-shape heuristics to run vector eagerly for semantically descriptive
  prompts with few lexical anchors
- caller-provided retrieval-policy preferences on `SearchBuilder`

None of these are required for the first coherent design. They are additive.

### Query classification

The planner may use basic heuristics to decide whether vector should run
eagerly in later versions. The exact heuristic is engine policy and is not
public API. In v1 no such heuristic runs; vector is zero-hits-only.

## Unified Result Surface

### Decision

`SearchHit` and `SearchRows` are the public result type for all ranked
retrieval. The main body's payload fields are preserved. Two additions
generalize the type for vector participation.

### Recommended types

```rust
pub enum RetrievalModality {
    Text,
    Vector,
}

pub enum SearchHitSource {
    Chunk,
    Property,
    Vector,
}

pub enum SearchMatchMode {
    Strict,
    Relaxed,
}

pub struct SearchHit {
    pub node: NodeRow,
    pub score: f64,
    pub modality: RetrievalModality,
    pub source: SearchHitSource,
    /// Populated for text hits only. `None` for vector hits.
    pub match_mode: Option<SearchMatchMode>,
    /// FTS5 snippet for chunk hits, trimmed property-blob window
    /// for property hits, `None` for vector hits and future sources
    /// that cannot produce a snippet.
    pub snippet: Option<String>,
    pub written_at: Timestamp,
    pub projection_row_id: Option<String>,
    /// Vector distance/similarity for vector hits. `None` for text hits.
    /// Modality-specific diagnostic metadata; not comparable across
    /// modalities.
    pub vector_distance: Option<f64>,
}

pub struct SearchRows {
    pub hits: Vec<SearchHit>,
    pub was_degraded: bool,
    pub fallback_used: bool,
    pub strict_hit_count: usize,
    pub relaxed_hit_count: usize,
    pub vector_hit_count: usize,
}
```

Changes from the main body's `SearchHit`:

- `match_mode` becomes `Option<SearchMatchMode>`. Vector hits set it to
  `None`. Text hits continue to set `Strict` or `Relaxed`. This is the only
  backwards-visible change in the payload, and the main body has no released
  contract, so it costs nothing.
- `modality` is added as a coarse top-level classifier that every hit carries
  unambiguously. `source` continues to carry the finer classification
  (`Chunk`, `Property`, `Vector`).
- `vector_distance` is added as modality-specific optional diagnostic data,
  parallel to how `snippet` is source-specific optional data.

Changes from the main body's `SearchRows`:

- `vector_hit_count` is added alongside `strict_hit_count` and
  `relaxed_hit_count`, preserving the main body's pattern of per-block counts.
  `was_degraded` and `fallback_used` keep their existing meanings;
  `was_degraded` additionally covers vector capability misses (see
  Vector-Specific Behavior below).
- The `was_degraded: bool` field retains its existing meaning and is kept
  for backward compatibility. Phase 12 additively introduces a
  `degradation_reasons: Vec<DegradationReason>` field alongside it, with
  variants `RelaxedBranchCapped` and `VectorCapabilityMissing`, so callers
  that want to distinguish degradation root causes can. The boolean is
  `true` iff the vec is non-empty. Callers that only test
  `if rows.was_degraded` continue to work unchanged.

Python and TypeScript mirror these types exactly.

### Attribution on vector hits

When a caller invokes `search(query, limit).with_match_attribution().execute()`
and the resulting `SearchRows` contains vector hits, each vector hit carries
`attribution = Some(HitAttribution { matched_paths: vec![] })`. This matches
the Phase 5 contract for chunk hits: `Some(empty)` means "attribution was
requested and this hit doesn't qualify" (no leaf structure), whereas `None`
means "attribution was not requested." The same `Some(empty)` shape applies
uniformly to every hit source that has no leaf structure (chunk, vector).
Only property hits with recursive-mode schemas produce populated
`matched_paths`.

### Why this shape

- It stays retrieval-generic without erasing legitimate modality differences.
- It preserves every resolved decision from the main body about the text
  result payload.
- It lets vector metadata (distance) live on the hit without reshaping the
  type.
- Fusion attribution — which branch produced which hit — stays readable from
  `(modality, source, match_mode)`.

## Fusion Semantics

Fusion is deterministic, bounded, and engine-owned. It is not a rank-learning
system.

### Block-based precedence

The main body's ranking semantics already define block-based ordering for
strict and relaxed text branches. Vector extends that as an additional block
under the same rules. Final ordering:

1. Text strict block
2. Text relaxed block
3. Vector block

Within each block, hits are ordered by raw engine score descending, with
`logical_id` lexicographic tiebreak — identical to the main body's
within-block rule. Scores are **ordering-only within a block**. Scores from
different blocks — and in particular text scores vs. vector distances — are
not on a shared scale. The engine does not normalize across blocks, and
callers must not compare or arithmetically combine scores across blocks. This
is the same contract the main body states for strict vs. relaxed; vector
simply inherits it.

Clients that want to render or rerank one modality at a time slice `hits` by
`strict_hit_count`, `relaxed_hit_count`, and `vector_hit_count`. Blocks appear
in the `hits` vector contiguously and in precedence order.

An empty block has count 0; the corresponding slice of `hits` is zero-length
and the next block immediately follows. A caller who finds
`strict_hit_count == 0 && relaxed_hit_count == 0` can read all `hits` as the
vector block. Blocks appear in precedence order regardless of emptiness.

### Dedup

Dedup by `logical_id` across the full candidate set before assembling blocks.
For duplicate logical IDs:

1. Prefer the highest-priority branch by the precedence order above (text
   strict > text relaxed > vector).
2. If the same branch produced the logical ID more than once, prefer the
   higher score.
3. Fall back to the main body's existing tiebreak chain within a single
   branch (higher score, then fixed source priority chunk > property >
   vector, then branch declaration order). **This within-branch tiebreak
   chain applies only to duplicates inside a single branch. Cross-branch
   dedup is always resolved by branch precedence order (text-strict >
   text-relaxed > vector) and never falls through to source priority.**

This is intentionally simple and biased toward explainability. A logical ID
that matched both lexically and semantically is returned exactly once,
attributed to its highest-priority originating branch.

### Why no cross-modality score normalization in v1

Cross-modality normalization is complex, load-bearing on the ranking story,
and easy to get wrong. A bounded precedence model is easier to reason about,
easier to test, easier to document, and less likely to create surprising
ranking behavior. If future product needs justify it, fusion can evolve
toward weighted hybrid ranking or reciprocal-rank fusion as an opt-in flag,
as an additive change. The v1 contract is bounded and stable without it.

## Text-Specific Behavior Inside the Unified Planner

Text retrieval retains behavior that vector retrieval does not have. The
unified surface does not erase those differences; it localizes them to the
text modality inside the planner. Unchanged from the main body:

- Safe `TextQuery` strict interpretation and relaxed derivation
- `K = 1` fallback trigger
- Relaxed-branch cap at 4 per-term alternatives
- Recursive property extraction and guardrails
- Position map and opt-in `with_match_attribution()`
- Filter composition on the text builder
- FTS filter fusion pass

These all continue to run inside the text branch of the planner. When the
caller uses `search(...)`, these behaviors run transparently. When the caller
uses `text_search(...)` explicitly, they run identically and the planner
simply skips the vector stage.

## Vector-Specific Behavior Inside the Unified Planner

### Degradation

Vector retrieval has capability-miss semantics today. Under the unified
planner:

- If vector capability is unavailable when stage 3 would run, the planner
  skips the vector stage and sets `was_degraded = true` on the resulting
  `SearchRows`.
- A missing vector capability is never fatal for the default retrieval path.
  The caller sees text results, possibly empty, with the degradation flag
  set.
- `vector_search(...)` explicit calls are allowed to surface a harder error
  if that is more useful for that API; the default `search(...)` path is
  always nonfatal.

This reuses the `was_degraded` flag the main body already exposes, extending
its meaning to cover "a retrieval branch the planner would have run was
skipped due to capability miss" in addition to its existing text-degradation
meanings.

### Score and distance

For vector hits, `score` is a negated distance or a direct similarity, such
that higher always means a better match — consistent with the text branch's
`-bm25(...)` convention. For distance metrics (cosine, L2, dot-product-as-
distance), `score = -vector_distance`. For similarity metrics,
`score = similarity` and `vector_distance` carries a canonically-derived
distance (e.g., `1 - similarity` for cosine similarity). The
`dedup_branch_hits` sort is `score descending`; the negation is load-bearing
for intra-block ranking correctness. Callers may read `vector_distance` for
display or internal reranking but must not compare it against text-hit
`score` values. `vector_distance` is a stable optional field on `SearchHit`,
documented as modality-specific and non-cross-comparable.

## Builder, AST, and Engine Changes

### Builder surface

Add:

- `search(query, limit) -> SearchBuilder` on the tethered node builder
- `SearchBuilder::execute() -> SearchRows`
- The existing filter composition methods on `SearchBuilder`, mirroring
  `TextSearchBuilder`

Keep as advanced overrides:

- `text_search(query, limit) -> TextSearchBuilder` (main body, unchanged)
- `vector_search(query, limit) -> VectorSearchBuilder` (future)

The current `NodeQueryBuilder::vector_search()` in the facade crate returns
`Self` (legacy self-returning pattern). Phase 11 refactors this to return
`VectorSearchBuilder<'e>`, mirroring the `TextSearchBuilder<'e>` and
`FallbackSearchBuilder<'e>` type-state pattern established by Phase 1-6.
The old untethered `QueryBuilder::vector_search()` in
`crates/fathomdb-query/src/builder.rs` stays as-is — it is used by
`compile_query` internally and is not part of the public facade surface.

### AST

The query AST gains a retrieval step rather than continuing to model text and
vector as unrelated top-level mechanisms:

```rust
pub enum QueryStep {
    Search {
        query: String,
        limit: usize,
    },
    TextSearch {
        query: TextQuery,
        limit: usize,
    },
    VectorSearch {
        query: String,
        limit: usize,
    },
    Traverse { /* ... */ },
    Filter(/* ... */),
}
```

The exact representation is secondary. The principle is: one primary
retrieval step, plus advanced mechanism-specific steps, plus the existing
non-retrieval steps.

### `fathomdb-query`

- Add the retrieval planning layer above the existing text-query parsing and
  property FTS compilation.
- Preserve `TextQuery` as the strict lexical representation.
- Preserve vector retrieval planning as a supported modality.
- Produce a bounded retrieval plan rather than a general query composition
  tree.

The unified-search planner lives in `fathomdb-query`, not `fathomdb-engine`.
It produces a `CompiledSearchPlan`-equivalent carrier that contains the text
branches (strict, optional relaxed) and the optional vector branch. The
coordinator executes the plan via a sibling of Phase 6's
`execute_compiled_search_plan`. Unit tests for the planner run against pure
`TextQuery` inputs without an engine instance, matching the existing
`relax::derive_relaxed` test style.

### `fathomdb-engine`

- Execute retrieval plans spanning one or more modalities.
- Collect candidate sets across text and vector branches.
- Dedupe and fuse candidates deterministically under the block precedence
  rules above.
- Emit `SearchRows` with per-block counts and the `was_degraded` /
  `fallback_used` flags populated truthfully.

### Rust facade

- Export retrieval-generic `SearchHit`, `SearchRows`, `RetrievalModality`,
  `SearchHitSource`, `SearchMatchMode`.
- Expose `search(...)` as the primary retrieval surface on the tethered node
  builder.
- Keep `text_search(...)` and (future) `vector_search(...)` as advanced
  overrides.

## Python and TypeScript SDKs

Both SDKs mirror Rust semantics exactly. Neither SDK invents its own
hybrid-retrieval policy; both surface what the Rust engine defines.

### Python

```python
rows = engine.nodes("Goal").search("ship quarterly docs", limit=10).execute()
for hit in rows.hits:
    hit.node
    hit.score
    hit.modality
    hit.source
    hit.match_mode        # None for vector hits
    hit.vector_distance   # None for text hits
```

`search()` returns a `SearchBuilder` whose `execute()` is statically annotated
to return `SearchRows`. No `Union` return types are introduced on either
builder.

### TypeScript

```ts
const rows = engine.nodes("Goal").search("ship quarterly docs", 10).execute();
for (const hit of rows.hits) {
  hit.node;
  hit.score;
  hit.modality;
  hit.source;
  hit.matchMode;        // null for vector hits
  hit.vectorDistance;   // null for text hits
}
```

`search()` return type statically narrows to `SearchRows` the moment
`.search(...)` is called in a chain.

## Consumer Documentation

The product story changes from "text search with a separate vector mechanism"
to:

- `search()` is the primary retrieval API.
- The engine may use text retrieval, vector retrieval, or both, under
  engine-owned policy.
- `SearchHit` is the retrieval result unit, uniform across modalities.
- Strict/relaxed text fallback is part of engine retrieval policy, not a
  caller concern.
- Vector capability degradation surfaces through `was_degraded` on the same
  `SearchRows` family.
- `text_search()` and `vector_search()` are advanced overrides for callers
  with a reason to pin the modality.

The main body's documentation changes for `text_search()` remain correct and
become documentation for the text-only advanced override.

## Backup / Restore / Rebuild / Integrity

This addendum requires no backup-format change.

Stable invariants, unchanged from the main body:

- Vector indexes remain derived state.
- FTS indexes remain derived state.
- Property schema declarations remain canonical.
- Restore and rebuild flows remain projection-centric.

Implications:

- Backup/export still preserves canonical state.
- Import/rebuild still regenerates derived indexes, including vector indexes
  on kinds that declare vector capability.
- Integrity checks still validate projection presence and drift, not
  retrieval quality or fusion ranking semantics.

Note: vector index regeneration requires the generator binary referenced in
the `generator_command` contract field to be present in the restore
environment. The backup format preserves contract metadata (profile, model
identity, generator command) but not the binary itself. Restore runbooks
must document that vector regeneration is a post-restore step that requires
model availability. This is an operator-managed dependency in v1;
strengthening the guarantee (e.g., bundling the generator into the backup)
is a future product decision.

## Testing Strategy

Tests become retrieval-oriented rather than purely text-oriented. All
text-specific tests defined by the main body remain required; the additions
below are what the unified surface adds.

### Core unit tests

- Retrieval-plan selection under each policy path (text strict hits, text
  strict miss + relaxed hits, text miss + vector hits, total miss).
- Vector inclusion/exclusion under the zero-hits-only trigger.
- Vector degradation behavior when capability is unavailable.
- Block precedence across text strict, text relaxed, and vector.
- Dedup-by-`logical_id` across modalities, including the tiebreak chain.
- `modality`, `match_mode`, and `vector_distance` population rules per hit
  source.

### Engine integration tests

- `search()` returning text-only hits when text retrieval is sufficient.
- `search()` returning vector-only hits when text retrieval is empty and
  vector is available.
- `search()` returning empty text results with `was_degraded = true` when
  vector capability is absent and text retrieval is empty.
- Deterministic ordering under repeated runs, including repeat runs with
  dedupable logical IDs across modalities.
- Explicit `text_search(...)` and `vector_search(...)` advanced overrides
  producing single-modality results with correct per-block counts.

### Cross-language parity

Cross-language scenarios validate:

- Hit ordering across blocks
- `modality`, `source`, and `match_mode` values per hit
- `vector_distance` population for vector hits
- `was_degraded` visibility under capability miss
- Deterministic fused results for identical inputs

### Harness and stress

- Lexical-only corpora.
- Semantic-only corpora where vector is the winning branch.
- Mixed corpora with overlapping lexical and semantic coverage.
- Vector capability absent.
- Repeated concurrent retrieval under the main body's concurrency contract
  (background writes never block foreground reads, and that invariant now
  covers vector retrieval paths too).

## Resolved Decisions (previously Open Questions)

1. **Documentation surface**: `search()` is the primary documentation entry
   point; `text_search()` and `vector_search()` are demoted to an "advanced
   retrieval controls" section. Phase 15 consumer docs commit to this
   framing.

2. **Future vector trigger shape**: deferred post-v1. v1 is zero-hits-only
   per §Retrieval Policy. Raising the vector trigger is a future policy
   change, not an API change, and is gated on the same block-based ranking
   safety argument that makes raising text `K` safe.

3. **Multi-modality provenance on fused hits**: deferred post-v1. v1
   attributes each final hit to its highest-priority originating branch
   only. Future additive extensions (e.g., a `Mixed` source variant or a
   provenance vector on the hit) are not blocked by the v1 contract.

4. **`vector_distance` API stability**: stable. `vector_distance: Option<f64>`
   is a public field on `SearchHit`, shipped in Phase 10, documented as
   modality-specific diagnostic data that is **not** comparable across
   modalities. Callers must not arithmetically combine `vector_distance`
   with text `score` values.

5. **Day-one filter composition parity**: yes. The main body's Resolved
   Decision on FTS filter fusion explicitly commits to `VecNodes` inheriting
   fusion behavior generically at wire-in time. `SearchBuilder` in Phase 12
   uses the same `partition_search_filters` helper that `TextSearchBuilder`
   and `FallbackSearchBuilder` use; the filter vocabulary is modality-neutral
   from day one.

## Done When

### Caller-visible acceptance

A single `search(query, limit).execute()` call returns deterministically
ranked `SearchRows` in Rust, Python, and TypeScript, with each hit carrying
`modality`, `source`, `score`, `snippet`, `written_at`, `projection_row_id`,
`match_mode` (for text hits), and `vector_distance` (for vector hits), and
with `was_degraded`, `fallback_used`, `strict_hit_count`, `relaxed_hit_count`,
and `vector_hit_count` populated truthfully on the result.

### Implementation done-when

- `search()` is the primary retrieval entry point in all three languages.
- `text_search()` and (future) `vector_search()` remain available as advanced
  overrides with identical result shapes.
- The engine runs a bounded retrieval planner with text-strict, text-relaxed,
  and vector branches.
- v1 policy is zero-hits-only for both the relaxed branch and the vector
  branch.
- Fusion is block-based with deterministic precedence, no cross-modality
  score normalization.
- Vector capability miss is nonfatal under `search()` and surfaces as
  `was_degraded = true`.
- Backup, restore, rebuild, and integrity checks remain correct and
  projection-centric.
- Cross-language, harness, integration, and stress tests assert the unified
  retrieval semantics explicitly.
- All resolved decisions in the main body are preserved verbatim for the text
  modality.

## Design decisions

### Decision: `indexed_json()` declined

During Phase 12/15 planning, a client sketch proposed a four-surface
retrieval API: `search()`, `text_search()`, `indexed_json()`, and
`vector_search()`. We accepted `search()` as the unified primary entry
point (Phase 12) and kept `text_search()` / `vector_search()` as advanced
mechanism-specific overrides, as specified in the Decision Summary
above. We **declined** `indexed_json()`.

The rejection is a category-error argument, not a capability one.
`indexed_json()` would have elevated a filter primitive — the
`filter_json_*` family — to a retrieval mechanism parallel to text and
vector. That framing conflates two orthogonal surfaces:

- `filter_json_*` is an **exact-value post-filter** applied to a
  candidate set at query time (e.g.
  `filter_json_text_eq("$.status", "published")`). It builds no index
  and does not tokenize; it narrows results by equality and
  comparison.
- Property FTS projections are a **retrieval projection** maintained
  on registered schemas. Declared JSON paths — optionally walked
  recursively — are tokenized and written to an FTS5 table at write
  time, and `search()` transparently matches tokens inside those
  extracted values. Property FTS is already the answer to "match text
  inside JSON".

An `indexed_json()` surface would duplicate the existing `filter_json_*`
surface without adding expressive power: it could not match tokens
inside a JSON subtree (that is recursive-mode property FTS, reached via
`search()` plus a schema registration), and it could not narrow by
exact value any better than the existing `filter_*` builder methods
already do. Accepting it would have split one coherent JSON story into
two overlapping surfaces — one retrieval-shaped, one filter-shaped —
that callers would have to pick between without a principled rule.

The current surface handles both use cases cleanly by composition:
register property FTS on the kinds whose structured text should be
searchable, then chain `filter_json_*` on the resulting
`SearchBuilder` to narrow by exact predicates. Applications that need
both features get both, on one builder, via existing primitives. The
Phase 15 consumer docs document this composition explicitly under
"`filter_json_*` vs property FTS" in the querying guide.

## v1.5 update: Phase 12.5 wires read-time embedding

Phase 12.5 lands the read-time query embedder the original addendum
deferred. The invariant "`search()` does not currently run the vector
branch on natural-language queries" now holds only when **no embedder
is configured** — i.e. when `EngineOptions.embedder` is left at its
default `EmbedderChoice::None` (equivalently, the Python
`embedder=None` / `"none"` and the TypeScript `embedder: undefined` /
`"none"`). In that shape nothing in the original dormancy contract
changes: every `SearchBuilder.execute()` result has
`vector_hit_count == 0`, and `vector_search()` remains the only way to
run a vector query.

The opt-in shape is additive. `EmbedderChoice::Builtin` (feature-gated
behind `default-embedder` on `fathomdb-engine`, backed by Candle +
`BAAI/bge-small-en-v1.5`, `[CLS]`-pooled and L2-normalized to 384
dimensions) and `EmbedderChoice::InProcess(Arc<dyn QueryEmbedder>)`
attach an embedder to the execution coordinator; the planner then
fills the vector branch by embedding the caller's raw query text
before CTE construction, and the existing block-precedence fusion
shape carries the resulting hits through `SearchRows` unchanged. When
the builtin feature is off at build time, or the model fails to load
at runtime, or the embedder returns `EmbedderError`, the coordinator
treats it as a graceful capability miss: the vector branch is skipped,
`SearchRows.was_degraded` is set, and the text branches run normally.

Write-time vector regeneration has since moved to the database-wide
`QueryEmbedder` identity: `VectorRegenerationConfig` carries kind and
preprocessing metadata, while the open-time embedder supplies model identity,
dimensions, and normalization policy. The old subprocess generator path is
historical. The remaining deferred work is managed async/incremental vector
projection for per-kind vector-indexed data.
