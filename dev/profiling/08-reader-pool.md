# 08 — Reader pool + snapshot transaction + concurrency

**Component:** `ReaderWorkerPool` (8 thread-affine connections, round-robin
`dispatch`, `ReaderRequest::Search`), the DEFERRED snapshot transaction in
`read_search_in_tx`, and the per-connection lookaside / page-cache config (Pack
6.G). `src/rust/crates/fathomdb-engine/src/lib.rs`.

## Why it matters

All reads route here, off the single writer. It is the retrieval concurrency
substrate — the AC-020 concurrency-ratio work lived in this layer. New 0.8.0
reads (G2 `get`, G3 `read_collection`, G4 `list`, G5 `neighbors`) all dispatch
through this same pool, so its overhead is a fixed tax on every read verb.

## Retrieval path — what to measure

- **Dispatch overhead vs in-tx work.** Separate the cost of `ReaderRequest`
  enqueue + round-robin pick + response channel from the actual SQL inside the
  DEFERRED transaction. For tiny queries the dispatch overhead is non-trivial.
- **Snapshot-tx cost.** `read_search_in_tx` opens a DEFERRED transaction (WAL
  snapshot). Measure BEGIN/COMMIT overhead and whether the snapshot reads
  contend with an in-flight checkpoint (see `11-sqlite-pragmas.md`).
- **Concurrency ratio** — the AC-020 metric: speedup of 8 concurrent readers vs
  sequential. Measure achieved parallelism; the historical residual was
  `pcache1` mutex + WAL shared-memory atomics + the `Mutex<Vec<Connection>>`
  borrow, not raw SQL. Profile concurrent vs single-reader throughput.
- **Lookaside / page-cache hit state.** `CounterSnapshot.cache_hit/cache_miss`
  and the per-connection lookaside high-water (`reader_lookaside_used_per_worker_for_test`,
  `lookaside_used_per_worker`). A cold reader pays page-cache misses; warm it and
  compare. Report the cache-hit ratio with every latency number.

## Key signals / seams

- Recording `Subscriber.on_profile` bucketed to `EventCategory::Search`.
- `CounterSnapshot` (`queries`, `cache_hit`, `cache_miss`) snapshotted per run.
- `#[cfg(debug_assertions)]` reader cache-status / lookaside-status seams
  (`ReaderRequest::CacheStatus` / `LookasideStatus`) — debug-build introspection.
- `FATHOMDB_PERF_READER_PRAGMAS` + pagecache/pcache2/lookaside knobs (gated) —
  the read-side sweep axis.

## Sharp edges

- Thread-affine workers: a connection's page cache is warm only for the pages it
  has touched — round-robin dispatch can spread a working set across 8 cold
  caches. Measure warm vs cold per-worker.
- Reads must use the real `search` path (through the pool), not hand-rolled SQL on
  a fresh connection, or the dispatch + snapshot overhead is excluded and the
  number is optimistic.
- The pool is 8 connections regardless of host core count; on a 4-core canonical
  runner, 8 readers oversubscribe — note the host topology.

## Scaling expectation

Dispatch + snapshot overhead is ~constant per query (doesn't grow with N); its
*share* of the budget shrinks as N grows and the vec0 scan dominates. It matters
most for small-N / high-QPS and for the cheap new read verbs (G2/G4) where it can
be the majority of the cost.
