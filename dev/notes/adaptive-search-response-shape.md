# Search Response Shape

This page describes the shape of a response returned by `text_search()` under
the design in `dev/design-adaptive-text-search-surface.md`, assuming the
recommendations in `dev/design-adaptive-text-search-surface-review-notes-1.md`
are applied and vector search is wired in.

It is a reference for what a client can expect in a single call, and —
equally important — what the engine deliberately does *not* do.

## Example call

```python
rows = (engine.nodes("KnowledgeItem")
    .where(tag="meeting", pinned=False, since="2026-03-01")
    .text_search("quarterly docs ship deadline", limit=5)
    .execute())
```

## Top-level result

`rows` is a `SearchRows`:

```python
SearchRows(
    hits=[SearchHit, SearchHit, ...],   # length <= limit
    was_degraded=False,                 # engine fell back to a safer plan?
    fallback_used=True,                 # relaxed branch ran?
    strict_hit_count=2,
    relaxed_hit_count=3,
)
```

## Per-hit shape

Each `SearchHit`:

```python
SearchHit(
    node=NodeRow(logical_id="ki_01HXZ...", kind="KnowledgeItem", payload={...}),
    score=7.42,                         # engine-normalized, ordering only
    source=SearchHitSource.Property,    # Chunk | Property | Vector
    match_mode=SearchMatchMode.Strict,  # Strict | Relaxed
    matched_path="$.payload.title",     # property hits only
    snippet="...ship the <mark>quarterly docs</mark> before the...",
    written_at="2026-04-03T14:22:10Z",
    projection_row_id=8821344,          # for provenance drill-in
)
```

## What the engine guarantees

**Deduped by logical_id.** If the same node matches via chunk FTS *and*
property FTS *and* vector, the client sees it once. See
[Search Winning Branch](adaptive-search-winning-branch.md) for precedence
rules.

**Ranked, deterministically.** Ordering is: strict block first (by score
desc, logical_id tiebreak), then relaxed block (same). With vectors wired in,
vector hits are a third branch fused into whichever block produced them —
lexical strict, lexical relaxed, or vector-only — under the same precedence
rules. Scores are comparable *within a branch*, not across; the ordering
contract lives at the block level, not the raw-score level.

**Filtered before ranking.** `where(...)` clauses are pushed into the FTS /
vector plan, so `limit=5` means 5 hits that already satisfy
tag / pinned / date — not 5 hits post-filtered down to 2.

**Snippets on chunk and property hits.** FTS5 `snippet()` for lexical; for
vector-only hits the snippet is the nearest-chunk text (or `None` if vector
hit a non-chunked node). Snippets are the text the caller should show; they
are not re-scored.

**Provenance without a second query.** `written_at` + `projection_row_id`
let the client answer "where did this come from" directly. For property hits,
`matched_path` points at the highest-scoring leaf (document-order tiebreak).

**Adaptive behavior is visible.** `fallback_used` and per-hit `match_mode`
together let the UI say "2 exact matches, 3 related" without the client
rerunning anything. `was_degraded` is the existing safety signal (e.g. query
had to be simplified to stay under FTS limits).

## What the engine does NOT do

- **No domain reranking.** Recency decay, pinned boost, reputation, per-tag
  weights — all client-side. The engine exposes `written_at` and `score` so
  the client has the inputs; it does not apply them.
- **No semantic post-filtering.** "Is this actually about Q2 planning" is the
  caller's judgment call.
- **No cross-kind fusion.** If a client wants to blend search over
  conversation history with search over knowledge items, it issues two calls.
  The search surface is per-kind-scope, not a global meta-search.
- **No guaranteed recall completeness.** `limit=N` is a hard cap even when
  more matches exist; the relaxed branch is capped separately to prevent
  blowup.
- **No result caching or pagination state.** Each call is stateless.
  Pagination (if added) is a separate design.

## The upshot for a client

A turn that wants "relevant meeting notes about quarterly docs from the last
six weeks" makes **one call**, gets back no more than N already-filtered,
already-ranked, already-deduped hits, each with enough metadata
(snippet, path, time, mode, source) to render *and* to explain *and* to
rerank if the client wants to. No branching on backend. No second query for
provenance. No client-side dedup loop.
