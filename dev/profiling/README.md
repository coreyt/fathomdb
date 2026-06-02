# FathomDB stack profiling — component guide

One file per component of the FathomDB storage/retrieval stack, each capturing
the **important** aspects of profiling that component on the **ingest/write
path** and the **search/retrieval path**. These are the durable "what to
measure and what will bite you" notes; the runnable harnesses are specified by
the slice prompts:

- Ingest profiler: `dev/plans/prompts/0.8.0-PROF-ingest-stack-profiling.md`
- Retrieval profiler: `dev/plans/prompts/0.8.0-PROF-retrieval-stack-profiling.md`

Context for *why* we profile this stack: `dev/design/0.8.0-agent-memory-fit.md`
(gap ladder G0–G12) and `dev/design/agent-memory-impl-strategy.md` (build plan).
All file:line references are against head of `main` (0.7.2); verify before
relying on an exact line.

## Components

| File | Component | Ingest role | Retrieval role |
|---|---|---|---|
| [`01-writer-thread.md`](01-writer-thread.md) | Single writer thread + `commit_batch` | the whole write path | n/a (reads bypass it) |
| [`02-canonical-store.md`](02-canonical-store.md) | `canonical_nodes` / `canonical_edges` | row INSERT | body fetch by cursor |
| [`03-fts5.md`](03-fts5.md) | `search_index` (FTS5) | segment inserts + merges | text branch (`MATCH`) |
| [`04-sqlite-vec.md`](04-sqlite-vec.md) | `vector_default` (vec0, binary-quant) | vec0 insert + quantize | two-phase KNN |
| [`05-embedder.md`](05-embedder.md) | embedder + projection runtime | async embed (dominant) | query embed |
| [`06-op-store.md`](06-op-store.md) | `operational_state` / `operational_mutations` | upsert / append + JSON validate | (G3/G7 read seams) |
| [`07-graph-traversal.md`](07-graph-traversal.md) | edges + recursive-CTE walk | edge INSERT | `neighbors()` / expand (G5/G6) |
| [`08-reader-pool.md`](08-reader-pool.md) | reader pool + snapshot tx + page cache | n/a | dispatch + concurrency |
| [`09-fusion-merge.md`](09-fusion-merge.md) | branch merge / dedup / fusion | n/a | merge + G9 RRF/rerank |
| [`10-bindings-ffi.md`](10-bindings-ffi.md) | PyO3 / napi marshalling | write-batch marshalling | result marshalling |
| [`11-sqlite-pragmas.md`](11-sqlite-pragmas.md) | WAL / PRAGMAs / page cache / allocator | checkpoint + sync cost | cache-hit state |
| [`v05-lineage.md`](v05-lineage.md) | v0.5.x feature surface (git history) | reference for G0/G8/G11 substrate | reference for G5/G6 traversal verbs |

## Cross-cutting rules (apply to every component)

1. **Separate the three ingest clocks**: `engine.write` return latency, embed/
   projection cost to `drain()` quiescence, and total wall. The async projection
   boundary means write-return ≠ work-done. (See `05-embedder.md`, `01-writer-thread.md`.)
2. **Pair every retrieval latency with recall@10** from the same run. 0.90 gate /
   0.937 ANN anchor. A "faster" change that drops recall is a regression.
   (See `04-sqlite-vec.md`, `09-fusion-merge.md`.)
3. **Account for the batch-vec0-collapse bug** (`dev/notes/0.7.0-engine-batch-vec0-collapse.md`):
   `engine.write(batch>1)` of vector-indexed nodes collapses to one vec0 row.
   Drive per-node writes or record the collapse ratio. (See `04-sqlite-vec.md`.)
4. **Capture, don't rebuild**: the `Subscriber` trait (`on_profile`,
   `on_slow_statement`, `on_event` with typed `EventCategory`) +
   `set_profiling(true)` + `engine.counters()` are the primary seams. Bucket
   `on_profile` records by statement shape / category. (See `08-reader-pool.md`,
   `11-sqlite-pragmas.md`.)
5. **Honesty**: pin dev-host (CPU, cores, `rusqlite::version()`, embedder
   identity via `dump_profile`); label cache-warm vs cold; flag dev-host-dependent
   numbers (the `SLOW_CTE` fixture in `lifecycle_observability.rs` is the
   cautionary tale).
6. **No production change while profiling**: use the gated `FATHOMDB_PERF_EXPERIMENTS=1`
   knob system for sweeps; emit metrics in a `dev/perf-history/`-compatible JSON
   superset so they can later feed `perf-regression-check`.
