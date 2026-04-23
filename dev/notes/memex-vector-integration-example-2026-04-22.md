# Memex vector integration — canonical usage (2026-04-22)

The managed vector projection replaces the old "write VecInsert + hope"
flow with a declarative setup step and ordinary writes. Full API
contract lives in
`dev/notes/design-db-wide-embedding-per-kind-vector-indexing-2026-04-22.md`.

## The four-step contract

1. **Configure the db-wide embedding profile** once.
2. **Enable vector indexing per kind** once.
3. **Write nodes and chunks** normally — the projection actor
   populates `vec_<kind>` asynchronously.
4. **Query** via `semantic_search(text)` or `raw_vector_search(vec)`.

## Rust

```rust
use fathomdb::{Engine, EngineOptions, EmbedderChoice, VectorSource, WriteRequest,
               NodeInsert, ChunkInsert, ChunkPolicy};
use std::sync::Arc;

let mut opts = EngineOptions::new("./memex.db");
opts.vector_dimension = Some(embedder.identity().dimension);
opts.embedder = EmbedderChoice::InProcess(Arc::new(embedder.clone()));
let engine = Engine::open(opts)?;

// 1. Db-wide profile. Identity comes from the embedder.
engine.admin().service().configure_embedding(&embedder, true)?;

// 2. Per-kind indexing.
engine.admin().service()
    .configure_vec_kind("KnowledgeItem", VectorSource::Chunks)?;

// 3. Ordinary writes. The projection actor auto-enqueues incremental
//    work on canonical chunk writes.
engine.writer().submit(WriteRequest {
    nodes: vec![NodeInsert { /* ... */ chunk_policy: ChunkPolicy::Preserve, .. }],
    chunks: vec![ChunkInsert { text_content: "Acme Corp".into(), .. }],
    // no vec_inserts — the projection actor will produce vec rows.
    ..Default::default()
})?;

// Optional: synchronously flush the queue (tests, batch ingest).
engine.admin().service()
    .drain_vector_projection(&embedder, std::time::Duration::from_secs(5))?;

// 4. Query.
let rows = engine.query("KnowledgeItem")
    .semantic_search("Acme", 5)
    .execute()?;
assert!(!rows.hits.is_empty());
assert!(!rows.was_degraded);
```

## Python

```python
from fathomdb import Engine, VectorSource

engine = Engine.open("./memex.db", embedder=embedder)
admin = engine.admin

# 1 + 2
admin.configure_embedding(embedder, acknowledge_rebuild_impact=True)
admin.configure_vec("KnowledgeItem", source="chunks")

# 3
with engine.writer() as w:
    w.add_node(logical_id="ki-acme", kind="KnowledgeItem", properties={})
    w.add_chunk(node_logical_id="ki-acme", text_content="Acme Corp")

admin.drain_vector_projection(timeout_ms=5000)

# 4
rows = engine.nodes("KnowledgeItem").semantic_search("Acme", limit=5).execute()
assert rows.hits
assert not rows.was_degraded
```

## TypeScript

```ts
import { Engine } from "fathomdb";

const engine = await Engine.open({ path: "./memex.db", embedder });
// 1 + 2
engine.admin.configureEmbedding(embedder, { acknowledgeRebuildImpact: true });
engine.admin.configureVec("KnowledgeItem", { source: "chunks" });

// 3
const w = engine.newWriteRequest();
const node = w.addNode({ kind: "KnowledgeItem", properties: {} });
w.addChunk({ node, textContent: "Acme Corp" });
engine.writer.submit(w.build());

const report = engine.admin.drainVectorProjection(5000);
// report: DrainReport { incremental_processed, backfill_processed, ... }

// 4
const rows = engine.nodes("KnowledgeItem")
  .semanticSearch("Acme", 5)
  .execute();
```

## Error contract summary

- `EmbedderNotConfigured` — hard error; no active profile exists.
- `KindNotVectorIndexed { kind }` — hard error; per-kind configure_vec
  never ran.
- `DimensionMismatch { expected, actual }` — hard error (raw vector
  search only).
- `was_degraded=true` + empty hits — soft degradation; the per-kind
  schema is stale or the embedder was temporarily unavailable.

## Migration from raw `VecInsert`

Delete all manual `VecInsert` construction. The flow is now:

- Keep your `NodeInsert` + `ChunkInsert`. The writer's canonical chunk
  write path calls the projection actor for you.
- Drop `vec_inserts: vec![...]` (Rust), `vec_inserts=[...]` (Python),
  and `addVecInsert(...)` (TypeScript).
- `vector_search(text)` callers should switch to `semantic_search(text)`
  — same semantics, explicit contract.
- `vector_search(vec)` or equivalent float-vec callers should switch
  to `raw_vector_search(vec)`.
