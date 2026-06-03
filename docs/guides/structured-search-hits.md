# Working with structured search hits

`engine.search(query)` returns a `SearchResult` whose `results` is a list of
**structured hits**. Each hit carries the matched record's identity, content,
a relevance `score`, and the retrieval `branch` that produced it — so callers
can rank, filter, and attribute results without a second lookup.

## Hit shape

| Field    | Type              | Meaning                                                                 |
| -------- | ----------------- | ----------------------------------------------------------------------- |
| `id`     | int               | The canonical row's write cursor (the stable per-row identity).         |
| `kind`   | str               | The record kind supplied at write time.                                 |
| `body`   | str               | The matched record body.                                                |
| `score`  | float             | G9 RRF-fused relevance (see below).                                     |
| `branch` | `"vector"`/`"text"` | Which retrieval branch produced the representative hit.               |

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
engine.write([{"kind": "note", "body": "structured retrieval hit shape"}])
engine.drain(timeout_s=30)

result = engine.search("structured")
for hit in result.results:
    print(hit.id, hit.kind, hit.branch, round(hit.score, 4), hit.body)
engine.close()
```

## TypeScript

```ts
import { Engine } from "fathomdb";

const engine = await Engine.open("memory.sqlite");
await engine.write([{ kind: "note", body: "structured retrieval hit shape" }]);
await engine.drain(30_000);

const result = await engine.search("structured");
for (const hit of result.results) {
  console.log(hit.id, hit.kind, hit.branch, hit.score.toFixed(4), hit.body);
}
await engine.close();
```

Both bindings return equivalent hits for the same database and query — the
structured-hit shape is part of the cross-binding SDK contract.
