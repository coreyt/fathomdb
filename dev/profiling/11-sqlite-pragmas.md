# 11 — SQLite substrate: WAL / PRAGMAs / page cache / allocator

**Component:** the SQLite engine config beneath everything — WAL mode +
checkpointing, durability (`synchronous`), page cache, lookaside, and the
allocator. Tunable for profiling via the gated `FATHOMDB_PERF_EXPERIMENTS=1` knob
system (`lib.rs` ~4353-5483).

## Why it matters

These settings move *both* paths at once and are the cheapest levers (no schema,
no code) — but they trade durability for speed, so they're a profiling axis, not
a free win. The 0.7.0 perf campaign (pcache2, pagecache, lookaside, PRAGMA
sweeps) lived here. Profiling must record the substrate config or numbers aren't
reproducible.

## The gated knobs (use for sweeps; never production defaults)

- `FATHOMDB_PERF_WRITER_PRAGMAS=<pragma;pragma>` — ingest-side sweep:
  `synchronous` (NORMAL vs FULL — the big durability/speed trade under WAL),
  `wal_autocheckpoint`, `cache_size`, `journal_size_limit`.
- `FATHOMDB_PERF_READER_PRAGMAS=<...>` — read-side sweep.
- `FATHOMDB_PERF_SQLITE_PAGECACHE=<bytes>:<count>` — global page-cache pool size.
- `FATHOMDB_PERF_SQLITE_PCACHE2=1` — per-instance page-cache allocator (the
  AC-020 residual lever).
- `FATHOMDB_PERF_SQLITE_MEMSTATUS_OFF=1` — disable memstatus mutex overhead.
- All no-op unless `FATHOMDB_PERF_EXPERIMENTS=1`.

## Ingest path — what to measure

- **Checkpoint cost.** Under WAL, writes append to `-wal`; checkpoints fold it
  back into the main DB. `wal_autocheckpoint` (default ~1000 pages) triggers
  periodic checkpoints *on the writer* — a bursty cost like FTS5 merges. Capture
  the spikes (`on_slow_statement`) and the `PRAGMA wal_checkpoint` frame counts
  (`TruncateWalReport` shape: `busy`, `log_frames`, `checkpointed_frames`).
- **fsync per commit.** `synchronous=FULL` fsyncs every commit; `NORMAL` under
  WAL fsyncs only at checkpoint. For small batches this dominates writer latency
  (`01-writer-thread.md`). Sweep both and report the durability trade explicitly.
- **WAL growth** — a long ingest without checkpoint grows `-wal`; reads then scan
  a large WAL. Correlate `-wal` size with read latency.

## Retrieval path — what to measure

- **Page-cache hit ratio.** `CounterSnapshot.cache_hit/cache_miss` +
  per-connection lookaside high-water (`08-reader-pool.md`). Cold cache = page
  faults to disk; warm = in-memory. Always report cache state with latency; a
  "fast" number on a warm cache is not the cold-start number.
- **Snapshot vs checkpoint contention** — a reader's DEFERRED snapshot taken
  while a checkpoint runs can pay extra. Note if a checkpoint fired mid-run.

## Key signals / seams

- `rusqlite::version()` — pin the exact SQLite version (canonical CI ships
  bundled 3.45.x via `libsqlite3-sys`); patch version affects numbers.
- `TruncateWalReport` / `PRAGMA wal_checkpoint(TRUNCATE)` for WAL frame accounting.
- The gated knobs above; record the exact config string in every report row.

## Sharp edges

- These knobs are **measurement-only** — they trade durability. Never quote a
  `synchronous=OFF`/`NORMAL` number as the product's latency without the
  durability caveat. Production durability is owned by
  `ADR-0.6.0-durability-fsync-policy`.
- Allocator/page-cache effects are host-specific (the AC-020 residual was
  aarch64-Tegra-specific) — pin the host and don't generalize across architectures.
- Bundled vs system SQLite can differ; canonical CI is the bundled reference.

## Scaling expectation

Substrate effects are multiplicative on every other layer, not additive — a small
page-cache or `synchronous=FULL` penalizes both paths proportionally. Their job
in the profile is to (a) be held constant for honest cross-run comparison and (b)
be swept deliberately to bound the cheap-lever headroom before any code change.
