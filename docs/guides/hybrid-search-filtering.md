# Hybrid search: RRF ranking + metadata filtering

`engine.search` runs **hybrid retrieval** — a vector (semantic) branch and a
text (FTS5) branch — and fuses the two into one ranked list. As of 0.8.0 the
fusion is **Reciprocal Rank Fusion (RRF)**, and `search` accepts an optional
**metadata filter**.

## RRF ranking (a documented behavior-compat event)

Each branch produces its own ranked list. RRF scores every result by the sum of
`1 / (60 + rank)` over the branches that surfaced it (rank is 1-based within a
branch):

```text
score(body) = Σ_branch  1 / (60 + rank_branch(body))
```

A body that **both** branches rank highly accumulates two terms, so agreement
pushes it to the top — which is the whole point of hybrid retrieval. RRF fuses
the *ordinal rank* each branch assigns, never the raw `vec_distance_l2` / `bm25()`
scores (those live on different, non-comparable scales). The fused value lands in
`SearchHit.score` (higher = more relevant); results are sorted by it, with a
vector-first tiebreak.

> **Behavior-compat note.** This RRF ordering is the deliberate, documented
> ranking change shipped in 0.8.0. Earlier releases returned a scoreless
> union-dedup (vector hits, then text hits). That ordering is **not** retained —
> there is no compatibility knob. If you pinned exact result ordering from a
> pre-0.8.0 release, re-baseline against RRF.

## Filtering by metadata

Pass a closed `SearchFilter` to constrain results. Every field is optional; an
omitted (or all-empty) filter is the unfiltered path.

| Field          | Constrains                                            |
| -------------- | ----------------------------------------------------- |
| `source_type`  | the partition `source_type` (derived from `kind`)     |
| `kind`         | the record `kind`                                     |
| `created_after`| `created_at >= bound` (unix seconds)                  |
| `status`       | the `status` metadata column (see the caveat below)   |

The filter prunes the vector branch inside the single phase-1 KNN statement and
constrains the text branch by the same metadata.

> **`status` caveat.** `status` is wired end-to-end but ships an **empty-string
> sentinel only** — there is no real population source in 0.8.0 (vec0 TEXT
> metadata columns cannot be NULL). A `status="open"`-style filter therefore
> prunes every row until a later slice populates it.

### Python

```python
from fathomdb import Engine, SearchFilter

engine = Engine.open("memory.sqlite")
engine.write([{"kind": "note", "body": "alpha retrieval document"}])
engine.write([{"kind": "doc", "body": "delta retrieval and ranking notes"}])
engine.drain(timeout_s=30)

# RRF-ranked, unfiltered.
hits = engine.search("retrieval").results

# Only `note` records.
notes = engine.search("retrieval", SearchFilter(kind="note")).results
```

### TypeScript

```ts
import { Engine } from "fathomdb";

const engine = await Engine.open("memory.sqlite");
await engine.write([{ kind: "note", body: "alpha retrieval document" }]);
await engine.write([{ kind: "doc", body: "delta retrieval and ranking notes" }]);
await engine.drain(30_000);

// RRF-ranked, unfiltered.
const hits = (await engine.search("retrieval")).results;

// Only `note` records.
const notes = (await engine.search("retrieval", { kind: "note" })).results;
```

## Recency reweight (off by default)

A `write_cursor`-derived recency reweight can nudge more-recent records up after
fusion. It is gated behind a dedicated, off-by-default flag and is conservative
(it breaks near-ties, never overriding a clear RRF signal). It is not yet exposed
on the SDK surface.

## See also

- [Working with structured search hits](structured-search-hits.md)
- [Python API](../reference/python-api.md) · [TypeScript API](../reference/typescript-api.md)
