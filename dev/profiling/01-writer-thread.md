# 01 — Single writer thread + `commit_batch`

**Component:** the one writer thread that serializes all writes; `Engine::write`
→ `write_inner` → `commit_batch` (one SQLite transaction per batch, per-row
`write_cursor`). `src/rust/crates/fathomdb-engine/src/lib.rs`.

## Why it matters

Every write — canonical nodes, edges, op-store rows, and the *enqueue* of vector/
FTS projection work — funnels through this single thread under one lock. It is
the ingest serialization point: if it stalls, the whole ingest stalls. Reads do
**not** go through it (they use the reader pool — see `08-reader-pool.md`), so it
is an ingest-only concern.

## Ingest path — what to measure

- **`engine.write` return latency** — time from call to return, per batch. This
  is *only* canonical INSERT + projection-enqueue; it does **not** include
  embedding (async, see `05-embedder.md`). Report it separately from drain.
- **Per-batch transaction cost** — `commit_batch` runs BEGIN…COMMIT once per
  batch. Measure cost vs `batch.len()` to find the amortization curve (one fsync
  per commit under WAL — see `11-sqlite-pragmas.md`).
- **Writer-lock hold time** — `validate_batch` runs *inside* the writer lock but
  *before* the SQLite transaction. Long validation (JSON-schema for op-store)
  extends lock hold and serializes the next writer.
- **Cursor assignment** — per-row `write_cursor = base + i + 1`. Cheap, but note
  the **batch-vec0-collapse bug**: `write_inner` historically passed the *final*
  batch cursor to every per-row vec0 INSERT, collapsing N vector rows to 1
  (`dev/notes/0.7.0-engine-batch-vec0-collapse.md`). This corrupts any
  per-row vec0 timing taken from a batched ingest — drive per-node writes for
  honest vec0 numbers.

## Key signals / seams

- Recording `Subscriber.on_profile` records bucketed to `EventCategory::Writer`.
- `on_slow_statement` for any write statement crossing the slow threshold
  (`set_slow_threshold_ms`).
- `engine.counters()` → `writes`, `write_rows` deltas per checkpoint.
- `engine.drain(timeout)` to mark the boundary between write-return and
  projection-complete (so writer cost is isolated from embed cost).

## Sharp edges

- Do **not** parallelize writes to speed up the profiler — the single-writer
  invariant is load-bearing (multiple writers on the WAL deadlock per the
  hard-constraint table). Concurrency is a *reader* property only.
- Validation cost is part of writer latency, not "free" — bucket it.
- The G0 identity slice adds an UPDATE-prior-then-INSERT (supersession) inside
  this same `commit_batch` transaction; baseline today's single-INSERT cost so
  G0's extra UPDATE is attributable later.

## Scaling expectation

Canonical INSERT is ~O(1) per row; the writer is rarely the dominant ingest cost
once the embedder is engaged — embed dwarfs it. The writer matters most for
**edge-heavy** or **op-store-heavy** (JSON-validated) batches, and as the
serialization gate when batches are tiny (per-commit fsync overhead dominates).
