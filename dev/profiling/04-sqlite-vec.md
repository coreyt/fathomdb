# 04 — sqlite-vec vector index (`vector_default`)

**Component:** the vec0 virtual table `vector_default` (Pack-1 shape: `embedding`
f32 + `embedding_bin` binary-quant + `source_type` partition key + `kind` +
`created_at` metadata cols). sqlite-vec pinned `=0.1.7`. Reshaped in Rust by
`ensure_vector_partition` (not a SQL migration). The semantic branch of hybrid
search and usually the **dominant cost** on both paths.

## Why it matters

Vector storage + KNN is the most expensive layer at scale and the one most
affected by 0.8.0 (G10 filtered KNN). vec0 at 0.7.2 is **brute-force, no ANN
index** — phase-1 KNN is O(N). This is *the* number that drives the tiered
latency budgets.

## Ingest path — what to measure

- **Embed vs vec0-insert vs quantize** — three separable costs at projection
  time. Embedding dominates (see `05-embedder.md`); but isolate the vec0 INSERT
  and the `vec_quantize_binary` cost (binary quant is a 32× size reduction, cheap
  per row but measure it).
- **THE batch-vec0-collapse bug** (`dev/notes/0.7.0-engine-batch-vec0-collapse.md`):
  `engine.write(batch>1)` of vector-indexed nodes lands **one** vec0 row for the
  whole batch (final-cursor reuse in `write_inner`). **This silently under-counts
  vec0 ingest work.** Drive per-node writes (like `tests/corpus_vector.rs`) OR
  record the collapse ratio explicitly. Never assert `vec0 count == node count`
  without accounting for it.
- **Double-write** — both `embedding` (f32, for rerank) and `embedding_bin`
  (binary, for phase-1) are written per row; both are real ingest cost.
- **`ensure_vector_partition` reshape cost** — on open, a shape change (e.g. a
  future `status`/`importance` column for G10/G12) triggers a staged
  drop+recreate+repopulate of the *whole* vec0 table under one IMMEDIATE
  transaction. This is a one-time open-path cost proportional to corpus size —
  measure it for any column-adding slice.

## Retrieval path — what to measure (the important part)

- **Two-phase split** — phase 1: `embedding_bin MATCH vec_quantize_binary(...)`
  bit-KNN prefilter `ORDER BY distance LIMIT TOP_K_BIT_CANDIDATES`; phase 2:
  `vec_distance_l2(embedding, ...)` exact f32 rerank `LIMIT final_limit`. **Time
  the two phases separately.** Phase 1 is the brute-force O(N) scan (the cost
  center); phase 2 is a small rerank over the candidate set.
- **`final_limit`** is production-pinned to `SEARCH_RERANK_LIMIT` (10); only the
  gated `FATHOMDB_PERF_SEARCH_LIMIT` seam raises it. Vary it to see rerank
  scaling, never below production.
- **Recall pairing** — phase-1 candidate count (`TOP_K_BIT_CANDIDATES`) trades
  recall for speed. Every latency measurement MUST report recall@10 (0.90 gate /
  0.937 anchor, `eu8_ir_validation.rs`). A smaller candidate set is "faster" and
  wrong.
- **`EXPLAIN QUERY PLAN`** on both phase statements — proves the full scan and is
  the before-shot for G10 (does a metadata predicate prune the scan or just
  filter post-scan?).
- **vec0 `k=` vs `LIMIT`** — vec0 rejects `k=` and `ORDER BY distance LIMIT`
  together; the current SQL uses `ORDER BY distance LIMIT`. Don't "fix" it.

## G10 filtered-KNN profiling note

The whole point of G10 is putting `AND source_type=? AND kind=? AND created_at>=?
AND status=?` in the **phase-1 WHERE** so the candidate prefilter is drawn from
the matching subset (vec0 is brute-force — prune via metadata/partition, never
post-fetch). `status` is a plain metadata col (NOT `+status` aux — aux columns
hard-error under any KNN WHERE). Profile: does the predicate actually reduce
phase-1 scan time, or does vec0 still scan all rows and filter? That answer
decides whether G10 needs partition keys to matter at scale.

## Scaling expectation

Phase-1 is **O(N)** (no ANN at 0.7.2): ~2s p50 at 1M×768 f32 pre-quant
(`dev/notes/0.7.0-vector-cost-research.md`); binary-quant prefilter makes it
fast to ~10K–100K but it still grows linearly. This is the layer that motivates
the post-1.0 ANN work and the tiered budgets — profile at 1K/10K/100K to draw
the curve.
