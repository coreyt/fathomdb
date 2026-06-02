# 05 — Embedder + projection runtime

**Component:** the default embedder (candle BGE, dim 768) and the **async
projection runtime** that drives it: `projection_dispatcher_loop` /
`projection_worker_loop` (`PROJECTION_WORKERS = 2`), the `embed_serialize`
mutex, `embed_with_watchdog`, and the `_fathomdb_projection_*` state tables.
`src/rust/crates/fathomdb-engine/src/lib.rs`.

## Why it matters

**Embedding is the single most expensive operation in ingest** — typically an
order of magnitude over every SQL layer combined. And it runs *asynchronously,
off the writer thread*, so it is invisible to `engine.write` return latency. Get
this wrong and your ingest profile is meaningless.

## Ingest path — what to measure (the important part)

- **The async boundary.** `engine.write` enqueues projection jobs and returns;
  embedding happens later on the projection workers. **Embed cost only fully
  lands after `engine.drain(timeout)`.** Measure (a) write-return, (b)
  drain-to-quiescence, (c) total — separately. Conflating them is the #1 way to
  misattribute embed cost.
- **Cold vs warm cache.** `CorpusFixture` surfaces `embed_cache_hit` /
  `cache_miss_reason` / `embedded_live`. A cold run embeds live (the real cost);
  a warm run hits the cache (near-zero). **Always label which.** The
  per-(model,subset) embed cache (`tests/support/corpus_harness.rs`) makes repeat
  runs cheap and deceptive.
- **Serialization.** `embed_serialize` mutex means embedding is effectively
  serialized even with `PROJECTION_WORKERS = 2` (the watchdog drops the guard
  while waiting). Measure achieved embed concurrency vs the worker count — the
  effective throughput, not the nominal 2×.
- **Warmup.** `embedder_warmup_ms` (eager model load at `Engine.open`, Invariant
  D) and, for the default embedder, one-time weight download
  (`embedder_download_ms`, ADR-0.7.1). Both are open-path costs, not per-row —
  report them once, separately from steady-state embed.
- **Per-call timeout / watchdog** — `embed_with_watchdog` enforces a per-call
  timeout (default 30s). A pathological body that times out is a projection
  failure logged to `operational_mutations(projection_failures)`, not a hang;
  count these.
- **Mean-centering** — if the identity requires it, the streaming accumulator
  pins a mean at a threshold and re-quantizes prior rows (a burst cost at the
  pin). Note if it fires during the profiled window.

## Retrieval path — what to measure

- **Query embed** — `search_inner` embeds the query string once to
  `query_vector` + `query_vector_bin`. For short queries this single embed can
  **rival the entire SQL cost** — isolate it as its own retrieval stage. It is on
  the synchronous query path (not the async projection runtime).

## Key signals / seams

- `pr9_embed_microbench.rs` — existing embed microbench pattern.
- `dump_profile()` → embedder identity + dimension (pin in report).
- `engine.counters()` → `cache_hit` / `cache_miss` (embed cache, distinct from
  SQLite page cache).
- `Instant`-based spans already exist around warmup/download (`lib.rs` ~1677,
  ~1380); projection job timing is the seam to add in the harness.

## Scaling expectation

Per-row embed cost is ~constant (model-bound), so total ingest embed scales
linearly with **uncached** row count — the dominant linear term. Query embed is
constant per query. This is why batching ingest and warming the cache matter, and
why the profiler must never report a warm-cache ingest as "the" ingest cost.
