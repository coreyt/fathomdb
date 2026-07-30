# Working with structured search hits

`engine.search(query)` returns a `SearchResult` whose `results` is a list of
**structured hits**. Each hit carries the matched record's identity, content,
a relevance `score`, and the retrieval `branch` that produced it — so callers
can rank, filter, and attribute results without a second lookup.

## Hit shape

| Field       | Type (Py / TS)            | Meaning                                                                 |
| ----------- | ------------------------- | ----------------------------------------------------------------------- |
| `id`        | `IdSpace`                 | Typed, non-null hit identity — `{space, value}`. **Not an integer.**    |
| `kind`      | `str` / `string`          | The record kind supplied at write time.                                 |
| `body`      | `str` / `string`          | The matched record body.                                                |
| `score`     | `float` / `number`        | G9 RRF-fused relevance (see below).                                     |
| `branch`    | `"vector"` \| `"text"` \| `"text_edge"` \| `"graph_arm"` | Which retrieval branch produced the representative hit. |
| `source_id` / `sourceId` | `str \| None` / `string \| null` | Source-document provenance — the id `erase_source` consumes. Populated on **every** hit path. |
| `ce_score` / `ceScore`   | `float \| None` / `number \| null` | Cross-encoder score (`sigmoid(logit)`) for hits inside the reranked pool; `None`/`null` otherwise. |

### `id` is a typed `IdSpace` (breaking since 0.8.9)

`SearchHit.id` used to be an integer row cursor. It is now a two-field
structure and is the **permanent** caller-facing identity, not an interim
carrier:

- `space` — `"logical"` (governed rows, prefix `l:`), `"content"`
  (doc-seeded rows, `h:`) or `"passage"` (synthetic passages, `p:`).
- `value` — the bare id with that prefix stripped. `f"{prefix}{value}"`
  reproduces the pre-0.8.19 `stable_id` byte-for-byte.

It is stable across sessions and re-ingest, and never participates in ranking.
The engine's positional `write_cursor` is internal book-keeping and is **not
surfaced by the bindings**. Only `logical`-space ids are lifecycle-addressable
by `transition` / `purge`.

### Branches

`branch` names which retrieval branch produced the representative hit:
`vector` (ANN vector branch over node bodies), `text` (node-body FTS5),
`text_edge` (an edge-body hit — FTS or vector-projected edge fact,
`kind == "edge_fact"`), and `graph_arm` (a node reached only through the
opt-in graph BFS arm).

`score` is the **G9 RRF-fused** relevance (`Σ 1/(60 + rank)`; higher = more
relevant). The vector (`vec_distance_l2`) and text (`bm25()`) branches are fused
on **rank**, never compared raw. Results are sorted by the fused score
descending (vector-first tiebreak), deduplicated on body. See
[Hybrid search & filtering](hybrid-search-filtering.md) for the full ranking +
filter model.

## Python

```python
from fathomdb import Engine

engine = Engine.open("memory.sqlite")
engine.write([
    {"kind": "note", "body": "structured retrieval hit shape",
     "source_id": "guide-hits"},
])
engine.drain(timeout_s=30)

result = engine.search("structured")
for hit in result.results:
    print(hit.id.space, hit.id.value, hit.kind, hit.branch,
          round(hit.score, 4), hit.source_id, hit.body)
engine.close()
```

## TypeScript

```ts
import { Engine } from "fathomdb";

const engine = await Engine.open("memory.sqlite");
await engine.write([
  { kind: "note", body: "structured retrieval hit shape", sourceId: "guide-hits" },
]);
await engine.drain(30_000);

const result = await engine.search("structured");
for (const hit of result.results) {
  console.log(hit.id.space, hit.id.value, hit.kind, hit.branch,
              hit.score.toFixed(4), hit.sourceId, hit.body);
}
await engine.close();
```

Both bindings return equivalent hits for the same database and query — the
structured-hit shape is part of the cross-binding SDK contract.

> **`source_id` is mandatory on every write.** It is what
> [`erase_source`](../operations/erasure.md) addresses, and it is echoed back
> on every hit so a caller can always resolve a result to the document it came
> from. Omitting it raises `WriteValidationError`.
