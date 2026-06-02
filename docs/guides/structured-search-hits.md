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
| `score`  | float             | Raw per-branch relevance (see below).                                   |
| `branch` | `"vector"`/`"text"` | Which retrieval branch surfaced the hit.                              |

`score` is the **raw per-branch** relevance: the vector branch reports
`vec_distance_l2` (lower = closer) and the text branch reports `bm25()`
(more-negative = more-relevant). The two scales are **not** comparable raw;
treat `score` as a within-branch ordering signal. Results are returned
vector-branch-first, deduplicated on body.

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
