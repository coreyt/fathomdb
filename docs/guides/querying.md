# Querying Data

This guide covers how to query nodes from a fathomdb database using the
Python and TypeScript SDKs. Python examples are shown first; see the
[TypeScript equivalent](#typescript-equivalent) section at the end for
the camelCase API. For the full API surface, see [Query API Reference](../reference/query.md).
For background on nodes, edges, and properties, see
[Data Model](../concepts/data-model.md).

## Starting a query

Every query begins with `db.nodes(kind)`, where `kind` is the node type you
want to match. The call returns an immutable `Query` builder -- each method
returns a **new** `Query`, leaving the original unchanged.

```python
from fathomdb import Engine

db = Engine.open("/tmp/my-agent.db")

base = db.nodes("Document")
draft = base.filter_json_text_eq("$.status", "draft")     # new Query
archived = base.filter_json_text_eq("$.status", "archived")  # another new Query
```

## Filtering

### Identity filters

```python
db.nodes("Document").filter_logical_id_eq("01HXYZ...")      # exact logical ID
db.nodes("Entity").filter_kind_eq("Person")                  # exact kind
db.nodes("Document").filter_source_ref_eq("ingest-run-42")   # provenance anchor
```

### Content reference filters

Filter nodes that reference external content:

```python
# All nodes with external content attached
db.nodes("Document").filter_content_ref_not_null()

# Nodes pointing to a specific external resource
db.nodes("Document").filter_content_ref_eq("s3://docs/q4-report.pdf")
```

### JSON property filters

Paths use SQLite JSON path syntax -- `$.field_name` for a top-level key.

```python
# Text equality
db.nodes("Document").filter_json_text_eq("$.status", "published")

# Boolean equality
db.nodes("Task").filter_json_bool_eq("$.is_complete", True)

# Integer comparisons: gt, gte, lt, lte
db.nodes("Task").filter_json_integer_gte("$.priority", 3)
db.nodes("Task").filter_json_integer_lt("$.priority", 10)

# Timestamp comparisons (Unix epoch integers): gt, gte, lt, lte
import time
one_day_ago = int(time.time()) - 86400
db.nodes("Event").filter_json_timestamp_gte("$.occurred_at", one_day_ago)
```

Filters chain with AND semantics:

```python
rows = (
    db.nodes("Task")
    .filter_json_text_eq("$.status", "open")
    .filter_json_integer_gte("$.priority", 5)
    .limit(20)
    .execute()
)
```

## Search

### Unified search (recommended)

`search(query, limit)` is the **primary retrieval entry point**. It is the
one call most applications should use when they want "find me relevant
nodes for this text". The engine owns the retrieval policy: you hand it a
raw user query and it runs a unified pipeline — strict text → relaxed
text → (future) vector — merging the results into a single ranked
`SearchRows` block under engine-owned block precedence.

```python
rows = (
    db.nodes("Goal")
    .search("ship quarterly docs", 10)
    .execute()
)
for hit in rows.hits:
    print(hit.node.logical_id, hit.score, hit.modality.value,
          hit.source.value, hit.snippet)
print(rows.strict_hit_count, rows.relaxed_hit_count, rows.vector_hit_count)
```

`search()` steps out of the regular `Query` builder. It returns a distinct
`SearchBuilder`, whose `.execute()` is statically typed to return
[`SearchRows`](../reference/types.md#searchrows), not `QueryRows`. Each
result is a [`SearchHit`](../reference/types.md#searchhit) carrying
`node`, `score`, `modality`, `source`, `match_mode`, `snippet`,
`written_at`, `projection_row_id`, `vector_distance`, and (optionally)
`attribution`. Callers read `rows.hits` and do **not** branch on backend.

`SearchBuilder` carries the same filter surface as `Query`:
`filter_kind_eq`, `filter_logical_id_eq`, `filter_source_ref_eq`,
`filter_content_ref_eq`, `filter_content_ref_not_null`, and the
`filter_json_*` family. As with `text_search()`, fusable filters (kind,
logical ID, source ref, content ref) are pushed into the search CTE and
`filter_json_*` runs as a post-filter. Opt in to per-hit match
attribution with `.with_match_attribution()`; see
[Property FTS Projections](./property-fts.md#match-attribution-opt-in)
for how recursive schemas populate the underlying position map.

The TypeScript SDK mirrors this surface with camelCase names
(`engine.nodes("Goal").search("ship quarterly docs", 10).execute()`); see
the [TypeScript equivalent](#typescript-equivalent) section below for the
full mapping.

#### Read-time embedding

`search()` fuses text and vector retrieval into one block-ordered
result, and — starting with Phase 12.5 — callers can opt into a
**read-time query embedder** so the vector branch fires on raw
natural-language queries without the caller having to produce a vector
literal themselves. The choice is made once at `Engine.open(...)` time
and has three shapes:

- **None** (default) — no embedder is attached. `search()`'s vector
  branch stays dormant and every result has `vector_hit_count == 0`.
  This preserves the Phase 12 v1 behaviour for callers who do not opt
  in.
- **Builtin** — the Candle-based `BAAI/bge-small-en-v1.5` embedder
  (384-dim, `[CLS]` token pooling with L2 normalization). Feature-gated
  behind the `default-embedder` Cargo feature; see the caveat below.
- **InProcess** — a caller-supplied in-process Rust implementation of
  the `QueryEmbedder` trait. This is the most flexible shape and is
  only reachable from the Rust API (`EmbedderChoice::InProcess(...)`
  on `EngineOptions`); the Python and TypeScript SDKs expose only the
  string-keyed `"none"` / `"builtin"` choices.

```python
from fathomdb import Engine

db = Engine.open(
    "/tmp/my-agent.db",
    embedder="builtin",
    vector_dimension=384,
)

rows = (
    db.nodes("Document")
    .search("quarterly docs", 10)
    .execute()
)
for hit in rows.hits:
    print(hit.node.logical_id, hit.score, hit.modality.value, hit.snippet)
```

TypeScript mirrors the same surface through the camelCase options bag
on `Engine.open`:

```typescript
import { Engine } from "fathomdb";

const engine = Engine.open("/tmp/my-agent.db", {
  embedder: "builtin",
  vectorDimension: 384,
});

const rows = engine
  .nodes("Document")
  .search("quarterly docs", 10)
  .execute();
for (const hit of rows.hits) {
  console.log(hit.node.logicalId, hit.score, hit.modality, hit.snippet);
}
```

**Build-feature caveat.** `"builtin"` only lights up when the underlying
`fathomdb-engine` crate is compiled with `--features default-embedder`.
When the feature is off, the engine logs a warning and silently falls
back to the `None` behaviour, so existing `embedder="builtin"` code
keeps working — it simply runs text-only with `vector_hit_count == 0`.

**Runtime degradation.** Even when the feature is compiled in, the
model weights still have to load on first use. If loading fails (or any
per-query embedding call errors), the coordinator treats it as a
capability miss: the vector branch is skipped and
`rows.was_degraded == True` signals that `search()` fell back to a
simpler plan. The rest of the search pipeline (strict text, relaxed
text, filters) runs normally.

**Latency notes.** Cold start on the Builtin embedder is roughly
300–500 ms for the first query, covering model-weight load and
tokenizer initialization. Warm per-query cost on CPU fp32 is roughly
20 ms. The embedder is held behind the engine for the lifetime of the
process, so the cold-start cost is paid once.

**Write-time regeneration.** Phase 12.5 wires the Builtin embedder into
the read path only. Write-time vector regeneration continues to flow
through `VectorRegenerationConfig` and is not yet driven by
`EmbedderChoice::Builtin`.

The advanced
[`vector_search()`](#advanced-vector-search-semantic-similarity)
override remains available for callers that want to bypass the unified
planner and supply a vector literal directly.

## Reranking `SearchRows.hits`

`search()` owns retrieval: it fuses strict text, relaxed text, and vector
branches into a block-ordered `SearchRows` under an engine-owned policy.
Ranking — the step that applies caller-specific signals like recency
decay, pinning, reputation, or domain boosts — belongs to the caller.
This section shows the recipe we recommend for that step.

The recipe is **docs-only**. There is no `fathomdb.rerank` module in
0.3.1; if the pattern graduates to a shipped helper, it will be a
separate post-0.3.1 call. Copy this function into your application, tune
the exponents, and own it.

### What the recipe does

1. **Splits `rows.hits` by `modality`** into a text pool and a vector
   pool. The FathomDB documentation is explicit that `SearchHit.score`
   values from different modality blocks are not on a shared scale and
   must not be arithmetically combined across blocks (see
   [`SearchHit`](../reference/types.md#searchhit)). The recipe respects
   that: raw scores are never compared across pools.
2. **Applies a pre-normalization `strict_bonus`** to text hits whose
   `match_mode` is `STRICT`. The engine already orders STRICT hits
   ahead of RELAXED hits under block precedence; the multiplier
   preserves that signal when the two branches get pooled for
   normalization. Vector hits have `match_mode == None` and are not
   affected.
3. **Normalizes each pool to `[0, 1]`** by dividing by the pool's max
   post-bonus score. This is the only step that turns raw engine scores
   into a common-scale relevance signal.
4. **Blends the normalized pools** using caller-supplied `text_weight`
   and `vector_weight` into one `relevance` value keyed on
   `node.logical_id`.
5. **Computes a composite score per hit** using the standard
   `relevance^a · decay^b · reputation^c · pin_boost^pinned` shape,
   with all four exponents tunable.
6. **Returns `Sequence[RankedHit]`** — a named tuple of
   `(score, hit)` — sorted highest-score-first. The score is exposed
   so applications can log, plot, or threshold against it during
   tuning. The one-line drop-in form is
   `hits = [r.hit for r in rerank_search_rows(rows, ...)]`.

### The function

```python
from datetime import datetime, timezone
from typing import Callable, NamedTuple, Optional, Sequence

from fathomdb import (
    NodeRow,
    RetrievalModality,
    SearchHit,
    SearchMatchMode,
    SearchRows,
)


class RankedHit(NamedTuple):
    """A `SearchHit` with its caller-computed composite score attached.

    Exposed so applications can inspect and tune the scoring formula.
    Unpack as `for score, hit in rerank_search_rows(...)` or project
    with `[r.hit for r in rerank_search_rows(...)]` for the drop-in
    shape.
    """

    score: float
    hit: SearchHit


def rerank_search_rows(
    rows: SearchRows,
    *,
    now: datetime,
    half_life_days: float,
    # Caller-supplied signal extractors. Defaults are no-ops so the
    # recipe produces sensible output on a minimal caller.
    is_pinned: Callable[[NodeRow], bool] = lambda _node: False,
    reputation_for: Callable[[NodeRow], float] = lambda _node: 1.0,
    # Scoring exponents. The product form keeps each signal
    # independent; exponents < 1 soften a signal, exponents > 1
    # sharpen it. pin_boost is a flat multiplier applied only to
    # pinned hits.
    relevance_exp: float = 1.0,
    decay_exp: float = 0.5,
    reputation_exp: float = 0.3,
    pin_boost: float = 2.0,
    # Block blending. text_weight and vector_weight combine the two
    # normalized pools; strict_bonus preserves the engine's STRICT-
    # over-RELAXED ordering before normalization.
    text_weight: float = 0.5,
    vector_weight: float = 0.5,
    strict_bonus: float = 1.2,
) -> Sequence[RankedHit]:
    """Rerank `SearchRows.hits` into a single ordering.

    Combines recency decay, pinning, and reputation with the
    engine-supplied relevance signal under a
    ``relevance^a * decay^b * reputation^c * pin_boost^pinned`` shape.
    Returns a sequence of `RankedHit` sorted highest-score-first.
    """

    # 1. Split by modality and apply strict_bonus inside the text pool.
    text_raw: list[tuple[SearchHit, float]] = []
    vector_raw: list[tuple[SearchHit, float]] = []
    for hit in rows.hits:
        if hit.modality == RetrievalModality.VECTOR:
            vector_raw.append((hit, hit.score))
        else:
            bonus = strict_bonus if hit.match_mode == SearchMatchMode.STRICT else 1.0
            text_raw.append((hit, hit.score * bonus))

    # 2. Normalize each pool to [0, 1] independently. Scores across
    # pools are never arithmetically combined in raw form.
    def _normalize(pool: list[tuple[SearchHit, float]]) -> dict[str, float]:
        if not pool:
            return {}
        max_score = max(s for _, s in pool)
        if max_score <= 0:
            return {h.node.logical_id: 0.0 for h, _ in pool}
        return {h.node.logical_id: s / max_score for h, s in pool}

    text_norm = _normalize(text_raw)
    vector_norm = _normalize(vector_raw)

    # 3. Blend the normalized pools, keyed on logical_id. A hit that
    # appears in only one pool contributes only that pool's weight.
    relevance_by_id: dict[str, float] = {}
    for lid, score in text_norm.items():
        relevance_by_id[lid] = text_weight * score
    for lid, score in vector_norm.items():
        relevance_by_id[lid] = relevance_by_id.get(lid, 0.0) + vector_weight * score

    # 4. Compute the composite score per unique hit. Under the engine's
    # block precedence the first occurrence of a logical_id in
    # rows.hits is always the highest-preference modality
    # (STRICT text > RELAXED text > VECTOR), so attaching that
    # SearchHit to the RankedHit preserves the engine's preferred
    # provenance view even though the relevance score fuses both pools.
    seen: set[str] = set()
    scored: list[RankedHit] = []
    for hit in rows.hits:
        lid = hit.node.logical_id
        if lid in seen:
            continue
        seen.add(lid)

        relevance = relevance_by_id.get(lid, 0.0)
        if relevance <= 0:
            continue

        # written_at is Unix epoch seconds per
        # docs/reference/types.md#searchhit.
        written_dt = datetime.fromtimestamp(hit.written_at, tz=timezone.utc)
        age_days = max(0.0, (now - written_dt).total_seconds() / 86_400.0)
        decay = 0.5 ** (age_days / half_life_days) if half_life_days > 0 else 1.0

        pinned = is_pinned(hit.node)
        reputation = reputation_for(hit.node)

        composite = (
            (relevance ** relevance_exp)
            * (decay ** decay_exp)
            * (max(reputation, 0.0) ** reputation_exp)
        )
        if pinned:
            composite *= pin_boost

        scored.append(RankedHit(score=composite, hit=hit))

    # 5. Sort highest-score-first. Stable on input order so ties
    # inherit the engine's block ordering.
    scored.sort(key=lambda r: r.score, reverse=True)
    return scored
```

### Worked example

Assume your application stores documents under a `Document` node kind
with two example properties your ranking cares about: `$.pinned`
(boolean) and `$.reputation_score` (float in `[0, 1]`).

```python
from fathomdb import Engine

engine = Engine.open("/tmp/my-app.db", embedder="builtin", vector_dimension=384)

rows = (
    engine.nodes("Document")
    .search("quarterly revenue", 50)
    .execute()
)

now = datetime.now(tz=timezone.utc)

ranked = rerank_search_rows(
    rows,
    now=now,
    half_life_days=14.0,
    is_pinned=lambda node: node.properties.get("pinned") is True,
    reputation_for=lambda node: reputation_store.get(
        node.properties.get("source"), 1.0
    ),
)

# Drop-in: replace rows.hits with the reranked ordering.
for hit in (r.hit for r in ranked[:10]):
    print(hit.node.logical_id, hit.modality.value, hit.snippet)

# Observability: inspect the composite scores while tuning.
for r in ranked[:10]:
    print(f"{r.score:.3f}  {r.hit.node.logical_id}  "
          f"{r.hit.modality.value}  {r.hit.match_mode}")
```

Both callbacks take a `NodeRow`, which keeps the recipe symmetric and
lets the caller extract whatever field they treat as the relevant
signal. `is_pinned` is a predicate: the inline lambda above reads a
JSON property, but any source — an external pin list, a tag, a
content-store flag — works just as well. `reputation_for` receives
the full `NodeRow` so the caller can extract whatever field they
treat as a source identifier — a JSON property like `$.source`,
`content_ref`, `kind`, or a composition of several — and return a
multiplier in `[0, ∞)`. The default is `1.0`, meaning "no reputation
signal, no penalty".

### Tuning the exponents

The composite score shape is:

```
composite = relevance^a · decay^b · reputation^c · pin_boost^pinned
```

Each exponent controls how sharply a signal bites. Exponent `< 1`
softens a signal; `> 1` sharpens it. The defaults
(`relevance_exp=1.0`, `decay_exp=0.5`, `reputation_exp=0.3`,
`pin_boost=2.0`) produce a "relevance dominates, decay and reputation
adjust" behavior that is a reasonable starting point for most
applications.

Tune with the composite score exposed on `RankedHit.score`:

- **Decay too aggressive?** Lower `decay_exp` toward `0.25`, or raise
  `half_life_days`. The half-life is the dominant knob; the exponent
  controls how nonlinearly age bites.
- **Old-but-trusted sources losing out?** Raise `reputation_exp` toward
  `0.5`. The product form means a trusted source has to be roughly
  `1 / decay` worth of reputation to overcome the same decay penalty.
- **Pins not dominant enough?** `pin_boost` is a flat multiplier; raise
  it to `3.0` or `4.0` for hard-pin behavior.
- **Text/vector balance off?** Adjust `text_weight` and `vector_weight`
  directly. They do not need to sum to `1.0` — the composite is
  re-normalized by the `max` within each pool, so the weights control
  relative influence, not absolute magnitude.
- **STRICT matches not dominating RELAXED inside the text pool?** Raise
  `strict_bonus` toward `1.5`. Values above `2.0` tend to make RELAXED
  hits unreachable when a STRICT hit exists at comparable raw score;
  if that is the behavior you want, prefer `text_search()` with
  explicit strict-only control instead.

### Adding a domain-specific boost

The recipe intentionally does not expose a `custom_boost` kwarg — the
four exponents plus `pin_boost` are the tunable surface. Callers that
need a domain-specific adjustment (type-based priority, route-profile
boost, per-user salience) should multiply the composite by their own
term before the final sort, either by wrapping the function or by
copying its body and inserting the adjustment at the composite-score
line. The recipe is small enough to copy; that is deliberate.

### Why this is docs-only

`rerank_search_rows` is a recipe, not a shipped API. Every application
tunes ranking differently, and the recipe's value is in the pattern,
not the defaults. Shipping it as a library function would force the
FathomDB team to pick defaults for everyone, stabilize a
`RankedHit` type across the FFI boundary, and maintain kwargs over
time. A copy-pasted recipe in your own codebase lets you tune, log,
and version ranking as a caller-owned concern, which is how ranking
policy should work.

If the pattern graduates to a shipped helper, it will be a separate
post-0.3.1 call with its own migration notes.

#### `filter_json_*` vs property FTS

The `filter_json_*` family and
[property FTS projections](./property-fts.md) are **orthogonal**
surfaces on JSON properties, and applications often need both.

- **`filter_json_*`** is an **exact-value post-filter** at query time.
  It does not build an index and does not tokenize — it applies
  `json_extract(...)` predicates to the candidate set and narrows it by
  equality, comparison, or range (e.g.
  `filter_json_text_eq("$.status", "published")`,
  `filter_json_integer_gte("$.priority", 3)`). It answers "restrict
  results to rows whose property equals X".
- **Property FTS** is a **retrieval projection** maintained on
  registered schemas. Declared JSON paths are extracted (optionally
  recursively), tokenized, and written to an FTS5 table at write time.
  `search()` — and the advanced `text_search()` override — match
  tokens inside those extracted values, so structured nodes become
  first-class citizens of the text-search pipeline without requiring
  synthetic chunks. It answers "find nodes where some token appears
  somewhere inside this JSON subtree".

The two compose: register property FTS on the kinds whose structured
text should be searchable, then chain `filter_json_*` on the
`SearchBuilder` to narrow results by exact-value predicates after the
search has produced candidates.

```python
rows = (
    db.nodes("KnowledgeItem")
    .search("quarterly docs", 50)
    .filter_json_text_eq("$.status", "published")
    .filter_json_integer_gte("$.priority", 3)
    .execute()
)
```

!!! warning "Post-filter footgun: `filter_json_*` runs *after* the search CTE"

    Because `filter_json_*` is a post-filter, the `limit` you pass to
    `search()` bounds the **candidate set**, not the final hit count.
    If the post-filter rejects most of those candidates, the query can
    return 0 hits even when thousands of matching rows exist in the
    database.

    **Wrong shape — silently drops to 0 hits:**

    ```python
    # Pulls the top 10 candidates by relevance, *then* filters by
    # $.status. If none of those 10 happen to be "active", this
    # returns 0 hits — even if there are thousands of active rows.
    rows = (
        db.nodes("Task")
        .search("urgent review", 10)
        .filter_json_text_eq("$.status", "active")
        .execute()
    )
    ```

    **Right shape — over-fetch so the post-filter has room to work:**

    ```python
    # Pull a candidate set large enough that the post-filter still
    # leaves enough hits after narrowing. Tune the multiplier to the
    # observed pass-through rate of your filter.
    rows = (
        db.nodes("Task")
        .search("urgent review", 200)   # 20x the desired final count
        .filter_json_text_eq("$.status", "active")
        .execute()
    )
    final = rows.hits[:10]
    ```

    **Better shape — push the filter into the retrieval projection:**

    If `$.status` is something you frequently narrow on, declare it as
    a [property FTS projection](./property-fts.md) so `search()` matches
    inside it at retrieval time, rather than applying it as a
    post-filter. Property FTS participates in the search CTE; `filter_json_*`
    does not.

    **Fused named variants (shipped in 0.4.0):** when a property FTS
    schema is registered for the target kind, you can use the
    `filter_json_fused_*` family to push the predicate into the search
    CTE itself, so the `limit` passed to `search()` applies *after*
    the narrowing runs. The full family is `filter_json_fused_text_eq`,
    `filter_json_fused_timestamp_gt`, `filter_json_fused_timestamp_gte`,
    `filter_json_fused_timestamp_lt`, and `filter_json_fused_timestamp_lte`.
    Each method requires a property FTS schema covering the JSON path
    you reference; calling one without a registered schema raises
    `BuilderValidationError` immediately — there is no silent degrade
    to a post-filter. See the feature summary in
    [Query reference](../reference/query.md#searchbuilder) for the
    full method list, and
    [`BuilderValidationError`](../reference/types.md#errors) for the
    error contract.

### Advanced: explicit text-only control

Most applications should prefer `search()` above. The mechanism-specific
builders below — `text_search()`, `vector_search()`, and
`fallback_search()` — are retained as **advanced overrides** for callers
with a hard reason to pin the retrieval modality or to supply both query
shapes verbatim. They share the `SearchRows` / `SearchHit` result family
with `search()` so the calling code shape is identical.

### Advanced: vector search (semantic similarity)

`vector_search` finds nodes whose embedded content is closest to the query.
The database must have been opened with `vector_dimension`. It is the
modality-specific override for callers who want to bypass the unified
planner and run vector retrieval directly. When no read-time embedder
is configured (`embedder=None`), this remains the only way to run a
vector query; when the Builtin or an in-process embedder is attached,
`search()` can embed natural-language queries at read time and most
callers should prefer it.

```python
db = Engine.open("/tmp/my-agent.db", vector_dimension=1536)

results = (
    db.nodes("Document")
    .vector_search("quarterly revenue discussion", limit=10)
    .execute()
)
for node in results.nodes:
    print(node.logical_id, node.properties.get("title"))
```

### Advanced: adaptive text search (text-only)

`text_search(query, limit)` is the text-only advanced override. It pins
retrieval to the text modality — strict-then-relaxed over chunks and
property FTS, with no vector stage even when the engine has vector
capability or a read-time embedder attached. Prefer `search()` above
unless you have a specific reason to exclude the vector branch.

It is an **adaptive** search: you hand the engine a raw user query and it
owns the retrieval policy. Two things matter for callers:

1. `text_search(...)` steps out of the regular `Query` builder. It returns a
   distinct `TextSearchBuilder`, whose `.execute()` is statically typed to
   return [`SearchRows`](../reference/types.md#searchrows), not `QueryRows`.
   There is no union return type — a chain ending in `text_search(...).execute()`
   always gives you hits, and a chain without it always gives you `QueryRows`.
2. Each result is a [`SearchHit`](../reference/types.md#searchhit) carrying
   `node`, `score`, `source`, `match_mode`, `snippet`, `written_at`,
   `projection_row_id`, and (optionally) `attribution`. Callers read
   `rows.hits` and do **not** branch on backend.

```python
rows = (
    db.nodes("Goal")
    .text_search("ship quarterly docs", 10)
    .execute()
)
for hit in rows.hits:
    print(hit.node.logical_id, hit.score, hit.source.value,
          hit.match_mode.value, hit.snippet)
```

#### Supported query syntax

The accepted query grammar is the strict safe subset: bare terms, quoted
phrases, implicit `AND`, uppercase `OR`, and uppercase `NOT`. Unsupported
syntax stays literal rather than passing through as raw FTS5 control syntax.
The full grammar, downgrade rules, and unsupported forms are documented in
[Text Query Syntax](./text-query-syntax.md).

That grammar describes the **strict** half of the adaptive policy. The
relaxed half is engine-owned — there is no user-facing syntax for it.

#### Strict-then-relaxed policy

Internally, the engine runs the query through two branches:

- **Strict branch.** The caller's query is parsed against the safe subset and
  lowered to FTS5 literally. Quoted phrases stay phrases, `AND`/`OR`/`NOT`
  stay boolean, terms match stem-for-stem under the default tokenizer.
- **Relaxed branch.** The engine derives a relaxed shape from the strict AST
  (term-level alternatives, softened exclusions, per-term fallbacks) and runs
  it. The relaxation rules are a fixed engine policy — they are not
  configurable per call.

The relaxed branch only runs when the strict branch returns **zero** hits
(the v1 trigger is `K=1`). When it does run, the two branches are merged so
that strict hits come first, then relaxed hits. Within each block, results
are ordered by the engine's ranking policy.

The returned `SearchRows` tells you what happened:

| Field | Meaning |
|---|---|
| `hits` | All `SearchHit` rows in final order (strict first, then relaxed). |
| `strict_hit_count` | Number of hits contributed by the strict branch. |
| `relaxed_hit_count` | Number of hits contributed by the relaxed branch. |
| `fallback_used` | `True` if the relaxed branch fired. |
| `was_degraded` | `True` if the engine fell back to a simpler plan shape. |

Each hit also carries `match_mode`: `strict` or `relaxed`. An application
that wants to visually separate confident matches from fuzzier fallbacks can
read `hit.match_mode` directly rather than splitting on index.

#### Source transparency

`text_search` covers both chunk-backed document text and property-backed
structured text for kinds with a registered
[FTS property schema](./property-fts.md). The originating surface shows up
as `hit.source`:

- `SearchHitSource.CHUNK` — the hit came from a document chunk.
- `SearchHitSource.PROPERTY` — the hit came from a property-FTS row.
- `SearchHitSource.VECTOR` is a reserved third slot for future use.

Applications do not need to know the source of a given hit to use it, but
the field is there when you want to e.g. show different snippet UI for
chunks vs structured fields.

#### Combining search with filters

`text_search` builders carry the same filter surface as `Query`:
`filter_kind_eq`, `filter_logical_id_eq`, `filter_source_ref_eq`,
`filter_content_ref_eq`, `filter_content_ref_not_null`, and the
`filter_json_*` family. The "fusable" filters — kind, logical ID, source
ref, and content ref — are pushed into the search CTE so that the per-branch
`limit` applies **after** filtering. The `filter_json_*` family runs as a
post-filter over the candidate set.

```python
rows = (
    db.nodes("Document")
    .text_search("architecture review", 50)
    .filter_json_text_eq("$.status", "published")
    .execute()
)
```

#### Match attribution (opt-in)

If the target kinds have recursive-mode property FTS schemas, you can ask
the engine to tell you *which* registered JSON path matched each hit:

```python
rows = (
    db.nodes("KnowledgeItem")
    .text_search("quarterly docs", 10)
    .with_match_attribution()
    .execute()
)
for hit in rows.hits:
    if hit.attribution:
        print(hit.node.logical_id, hit.attribution.matched_paths)
```

`with_match_attribution()` is **opt-in**: hits you receive without it have
`attribution == None`, and the default path pays no extra cost. See
[Property FTS Projections](./property-fts.md#match-attribution-opt-in) for
how recursive schemas populate the underlying position map.

#### Advanced: explicit two-shape fallback search

`search()` is the right surface for almost all application queries; if
you want to pin the text modality, `text_search()` above is the next
step. For the narrow case where the caller wants to supply both a strict
and a relaxed shape **verbatim** — for example, the dedup-on-write pattern where
you already have a canonical key and want to accept a looser match only if
the exact key misses — use `Engine.fallback_search`:

```python
rows = db.fallback_search(
    "quarterly docs",
    "quarterly OR docs",
    limit=10,
).execute()
```

`fallback_search` returns the same `SearchRows` type and the same
`FallbackSearchBuilder` filter surface as `text_search`. Neither branch is
adaptively rewritten — the engine runs `strict_query` first, and if it
returns zero hits, runs `relaxed_query` verbatim. Passing `None` for
`relaxed_query` degenerates to a strict-only search, which is the shape the
dedup-on-write pattern uses. It is a **bounded helper**, not a general
query-composition API: there is no way to stack arbitrary relaxed branches
or to override the engine's merge policy.

## Graph traversal

`traverse` follows edges from matched nodes. Specify the direction, edge
label, and maximum depth:

```python
from fathomdb import TraverseDirection

authors = (
    db.nodes("Document")
    .filter_json_text_eq("$.title", "Q4 Report")
    .traverse(direction=TraverseDirection.OUT, label="authored_by", max_depth=1)
    .execute()
)
for node in authors.nodes:
    print(node.properties.get("name"))
```

- `TraverseDirection.OUT` -- follow outgoing edges from matched nodes.
- `TraverseDirection.IN` -- follow incoming edges.
- `max_depth=1` -- single hop. Higher values use recursive traversal with
  cycle detection.

You can also pass the direction as a plain string (`"in"` or `"out"`).

## Limit

`limit(n)` caps the total number of result rows. Note that `search`,
`vector_search`, `text_search`, and `fallback_search` each accept their
own `limit` controlling candidate set size at the search stage. The
top-level `limit()` applies after all steps.

```python
q = db.nodes("Document").limit(5)
```

## Terminal methods

### execute()

Runs the query and returns a `QueryRows` object:

```python
rows = db.nodes("Document").limit(10).execute()

rows.nodes         # list[NodeRow] -- matched nodes
rows.runs          # list[RunRow]  -- associated runs
rows.steps         # list[StepRow] -- associated steps
rows.actions       # list[ActionRow] -- associated actions
rows.was_degraded  # bool -- True if the engine fell back to a simpler plan
```

Each `NodeRow` has: `row_id`, `logical_id`, `kind`, `properties` (decoded
dict), `content_ref` (string or `None`), and `last_accessed_at`.

### compile()

Returns a `CompiledQuery` without executing -- useful for inspecting
generated SQL or caching query shapes:

```python
compiled = db.nodes("Document").filter_json_text_eq("$.status", "draft").compile()

compiled.sql            # the generated SQL string
compiled.driving_table  # DrivingTable.NODES, .FTS_NODES, or .VEC_NODES
compiled.shape_hash     # int -- cache key for prepared statements
```

### explain()

Returns a `QueryPlan` with execution metadata but no result rows:

```python
plan = db.nodes("Document").vector_search("budget", limit=5).explain()

plan.driving_table  # DrivingTable.VEC_NODES
plan.shape_hash
plan.cache_hit      # True if a cached prepared statement was reused
```

## Grouped queries and expansions

When you need a root set of nodes plus related subgraphs for each root, use
`expand()` with `execute_grouped()`. This avoids N+1 round trips.

`expand()` registers a named expansion slot -- a traversal that runs for
every root node. You can register multiple slots:

```python
results = (
    db.nodes("Project")
    .filter_json_text_eq("$.active", "true")
    .limit(5)
    .expand(slot="members", direction=TraverseDirection.IN, label="member_of", max_depth=1)
    .expand(slot="tasks", direction=TraverseDirection.IN, label="belongs_to", max_depth=1)
    .execute_grouped()
)
```

`execute_grouped()` returns a `GroupedQueryRows` with three fields:

- `roots` -- `list[NodeRow]`, the matched root nodes.
- `expansions` -- `list[ExpansionSlotRows]`, one per `expand()` call.
  Each slot contains a list of `ExpansionRootRows`, pairing a
  `root_logical_id` with the `list[NodeRow]` reached from that root.
- `was_degraded` -- `bool`.

Reading the results:

```python
for project in results.roots:
    print(f"Project: {project.properties.get('name')}")

for slot in results.expansions:
    print(f"--- {slot.slot} ---")
    for root_rows in slot.roots:
        names = [n.properties.get("name") for n in root_rows.nodes]
        print(f"  {root_rows.root_logical_id}: {names}")
```

### Worked example: Memex use case — goal with commitments, actions, and plan steps

Fetch a `WMGoal` node and expand three edge kinds in one query:

```python
from fathomdb import Engine, TraverseDirection

db = Engine.open("memex.db")

results = (
    db.nodes("WMGoal")
    .filter_logical_id_eq(goal_id)
    .expand(
        slot="commitments",
        direction=TraverseDirection.OUT,
        label="HAS_COMMITMENT",
        max_depth=1,
    )
    .expand(
        slot="provenance_actions",
        direction=TraverseDirection.OUT,
        label="HAS_PROVENANCE_ACTION",
        max_depth=1,
    )
    .expand(
        slot="plan_steps",
        direction=TraverseDirection.OUT,
        label="HAS_PLAN_STEP",
        max_depth=1,
    )
    .execute_grouped()
)

goal = results.roots[0]
commitments = next(
    (s.roots for s in results.expansions if s.slot == "commitments"), []
)
plan_steps = next(
    (s.roots for s in results.expansions if s.slot == "plan_steps"), []
)
```

### Worked example: target-side filter — expand by action kind

Use the `filter` argument on `.expand()` to narrow which expanded nodes are
returned. The filter accepts the same predicate grammar as main-path filters:

```python
from fathomdb import Engine, TraverseDirection, JsonTextEq

db = Engine.open("memex.db")

# Only return HAS_ACTION edges where action_kind == "discussed_in"
discussed = (
    db.nodes("WMGoal")
    .search("quarterly planning", limit=20)
    .expand(
        slot="discussed_actions",
        direction=TraverseDirection.OUT,
        label="HAS_ACTION",
        max_depth=1,
        filter=JsonTextEq("$.action_kind", "discussed_in"),
    )
    .execute_grouped()
)

# Only tasks
tasks = (
    db.nodes("WMGoal")
    .search("quarterly planning", limit=20)
    .expand(
        slot="task_actions",
        direction=TraverseDirection.OUT,
        label="HAS_ACTION",
        max_depth=1,
        filter=JsonTextEq("$.action_kind", "task"),
    )
    .execute_grouped()
)
```

Filter validation is **builder-time**: the error is raised when the filter is
added, before any SQL runs. Fused filters on the expansion side raise
`BuilderValidationError::MissingPropertyFtsSchema` at builder time if the
target kind has no registered property-FTS schema.

### Per-originator limit

!!! note "Per-originator, not global"

    The `.limit(N)` call chained after `.expand()` is applied **per
    originator**, not globally. A search returning 50 hits with a
    `.expand(...).limit(20)` chain returns up to 20 expanded nodes **per
    hit**, for up to 1000 total — not 20 total. This holds even when the
    distribution is heavily skewed: a single originator with 500 candidates
    will not starve other originators' budgets.

For full semantics, sharp edges, and the complete method signature reference,
see [Grouped expand](../reference/query.md#grouped-expand-expand--execute_grouped)
in the query reference.

## Quick reference

| Goal | Method |
|------|--------|
| Match by logical ID | `filter_logical_id_eq(id)` |
| Match by kind | `filter_kind_eq(kind)` |
| Match by source ref | `filter_source_ref_eq(ref)` |
| Has external content | `filter_content_ref_not_null()` |
| Match by content URI | `filter_content_ref_eq(uri)` |
| JSON text equality | `filter_json_text_eq(path, value)` |
| JSON bool equality | `filter_json_bool_eq(path, value)` |
| JSON integer range | `filter_json_integer_gt/gte/lt/lte(path, value)` |
| JSON timestamp range | `filter_json_timestamp_gt/gte/lt/lte(path, value)` |
| Unified search (recommended) | `search(query, limit)` → `SearchRows` |
| Advanced: text-only search | `text_search(query, limit)` → `SearchRows` |
| Advanced: vector search | `vector_search(query, limit)` |
| Advanced: explicit fallback search | `Engine.fallback_search(strict, relaxed, limit)` → `SearchRows` |
| Match attribution | `.with_match_attribution()` on any search builder |
| Graph hop | `traverse(direction, label, max_depth)` |
| Cap results | `limit(n)` |
| Fetch results | `execute()` |
| Inspect SQL | `compile()` |
| Inspect plan | `explain()` |
| Subgraph expansion | `expand(slot, direction, label, max_depth)` + `execute_grouped()` |

## TypeScript equivalent

The TypeScript SDK mirrors the Python API with camelCase naming. All query
methods are identical in semantics.

```typescript
import { Engine } from "fathomdb";

const engine = Engine.open("/tmp/my-agent.db");

// Filters use camelCase method names
const rows = engine.nodes("Document")
  .filterJsonTextEq("$.status", "published")
  .filterJsonIntegerGte("$.priority", 3)
  .limit(20)
  .execute();

// Results use camelCase property names
for (const node of rows.nodes) {
  console.log(node.logicalId, node.properties);
}

// Content reference filters
const extDocs = engine.nodes("Document")
  .filterContentRefNotNull()
  .limit(20)
  .execute();

const specific = engine.nodes("Document")
  .filterContentRefEq("s3://docs/q4-report.pdf")
  .execute();

// Unified search — the recommended retrieval entry point. Returns
// SearchRows, not QueryRows.
const searchRows = engine.nodes("Document")
  .search("architecture review", 50)
  .filterJsonTextEq("$.status", "published")
  .execute();
for (const hit of searchRows.hits) {
  console.log(hit.node.logicalId, hit.score, hit.modality, hit.source,
              hit.matchMode, hit.snippet);
}
console.log(searchRows.strictHitCount, searchRows.relaxedHitCount,
            searchRows.vectorHitCount);

// Advanced: pin to the text modality (no vector branch).
const ftsRows = engine.nodes("Document")
  .textSearch("architecture review", 50)
  .filterJsonTextEq("$.status", "published")
  .execute();

// Opt-in match attribution (for recursive property FTS schemas) — works
// on search() and on the advanced text_search() override alike.
const attributed = engine.nodes("KnowledgeItem")
  .search("quarterly docs", 10)
  .withMatchAttribution()
  .execute();

// Explicit two-shape fallback search
const fb = engine.fallbackSearch("quarterly docs", "quarterly OR docs", 10).execute();

// Graph traversal -- pass an options object instead of keyword args
const authors = engine.nodes("Document")
  .filterJsonTextEq("$.title", "Q4 Report")
  .traverse({ direction: "out", label: "authored_by", maxDepth: 1 })
  .execute();

// Grouped queries with expansions
const grouped = engine.nodes("Project")
  .filterJsonTextEq("$.active", "true")
  .limit(5)
  .expand({ slot: "members", direction: "in", label: "member_of", maxDepth: 1 })
  .expand({ slot: "tasks", direction: "in", label: "belongs_to", maxDepth: 1 })
  .executeGrouped();

engine.close();
```

**Key differences from Python:**

| Python | TypeScript |
|--------|-----------|
| `filter_logical_id_eq(id)` | `filterLogicalIdEq(id)` |
| `filter_content_ref_not_null()` | `filterContentRefNotNull()` |
| `filter_content_ref_eq(uri)` | `filterContentRefEq(uri)` |
| `filter_json_text_eq(path, val)` | `filterJsonTextEq(path, val)` |
| `traverse(direction=..., label=..., max_depth=...)` | `traverse({ direction, label, maxDepth })` |
| `expand(slot=..., direction=..., label=..., max_depth=...)` | `expand({ slot, direction, label, maxDepth })` |
| `rows.was_degraded` | `rows.wasDegraded` |
| `node.logical_id` | `node.logicalId` |
| `node.content_ref` | `node.contentRef` |
| `node.last_accessed_at` | `node.lastAccessedAt` |
