# IR-C — Passage Indexing: design spec

Status: draft / scoping. Motivation: the IR-C dense-arm investigation
(`dev/plans/runs/performance-output-and-compare.md`, updates 2026-06-10d/e)
showed that embedding whole bodies dilutes long docs past bge-small's 512-token
window; **chunked passages + max-pool** lifted the dense arm (vector-only R@10
0.753→0.833 exact / 0.350→0.475 exploratory) and improved deep exploratory recall
in the hybrid (R@20 0.725→0.850, R@50 0.887→0.925). This spec scopes turning that
harness result into a real engine feature.

## Validated recipe (from the sweep)

- **Chunk geometry**: word windows, ~128 words / 96 stride is the best single
  compromise (64/48 wins exact_fact, 128/96 wins exploratory; 256/whole lose both).
- **Pooling**: **max** (doc = its single best passage). Mean re-dilutes; top-2 ≈ max.
- **Prefix**: none — the BGE query-instruction prefix is a wash even on passages.
- **Text arm stays document-level** (content-OR over the whole body). The winning
  config fused a *passage-max-pooled* vector arm with a *doc-level* lexical arm.

## Current architecture (the 1-node-1-vector assumption)

Identity unit is the canonical node, keyed by `write_cursor` (interim id) and
optionally `logical_id` (stable, supersession). "1 node = 1 vector" is enforced at:

1. `_fathomdb_vector_rows.write_cursor UNIQUE` (schema) — one vector row per node.
2. `vector_default.rowid = write_cursor` — the vec0 rowid *is* the node cursor.
3. `fuse_rrf()` dedups on `body` — a body surfaces once; no multi-passage routing.

Write→index: `commit_batch` inserts the node into `canonical_nodes` + `search_index`
(FTS5, one row/node); the projection worker (`run_projection_job`) embeds the whole
`body` once and `commit_projection_outcomes` writes one `vector_default` row at
`rowid = write_cursor`. Search: bit-KNN top-192 → L2 rerank top-10 (vector) ⨁
BM25 over `search_index` (text), fused by `fuse_rrf` (RRF_K=60), `SearchHit.id =
write_cursor`.

## Recommended design — Option A: passages as a projection-internal fan-out

Keep the node as the identity/return unit. Only the **vector projection** fans out
(1 node → N passage vectors); passages are index artifacts, not first-class nodes.
Retrieval max-pools passages back to their parent node before fusion. This leaves
`canonical_nodes`, `logical_id`, supersession, `read_get`, and the **entire text
arm** untouched — matching exactly what the experiment did.

(Rejected — Option B: passages as first-class `canonical_nodes` with a `doc_id`
grouping column. Touches identity, supersession, dedup, and read paths everywhere;
much heavier for no retrieval gain over A.)

### Schema changes

- New map table `_fathomdb_passages(passage_rowid INTEGER PK, write_cursor INTEGER,
  passage_idx INTEGER)` — passage → parent node.
- `vector_default.rowid` becomes a **passage_rowid** (new monotonic counter), no
  longer the node `write_cursor`. Columns otherwise unchanged (384-dim embedding +
  bin, source_type/kind/created_at copied from the parent node to each passage row).
- Relax `_fathomdb_vector_rows.write_cursor UNIQUE` → key it on `passage_rowid`;
  `write_cursor` becomes non-unique (or moves into `_fathomdb_passages`).
- **`search_index` (FTS) unchanged** — stays document-level, one row per node.

### Write / projection path

- `run_projection_job`: chunk `body` (`chunk_words`, 128/96 default; see open Q1),
  embed each passage, emit N vector outcomes.
- `commit_projection_outcomes`: insert N `vector_default` rows + N `_fathomdb_passages`
  rows atomically; still terminate the single `write_cursor` in
  `_fathomdb_projection_terminal` (idempotency unchanged — one job, N inserts).
- Mean-vec pin (`MEAN_VEC_PIN_THRESHOLD`) now counts passages — pins sooner; mean is
  computed over passage vectors (correct for a passage index). Minor.

### Search path

- Bit-KNN + L2 rerank now return **passage** hits.
- **New fold step (before `fuse_rrf`)**: join passage hits → parent `write_cursor`
  via `_fathomdb_passages`, **max-pool** to one hit per node (best passage rank/
  score), fetch body from `canonical_nodes`. Then fuse with the (unchanged,
  doc-level) text branch. `fuse_rrf` dedup-on-body still holds (one body/node).
- `SearchHit.id` stays `write_cursor` → downstream `read_get` unchanged.

## Open questions / decisions

1. **Granularity is class-dependent but query class isn't visible at serve time.**
   Pick 128/96 (best compromise) as default; optionally per-`kind` config. (64/48 if
   a kind is exact-fact-dominated.)
2. **Candidate budget.** `TOP_K_BIT_CANDIDATES=192` now competes across passages — a
   long doc can occupy up to N candidate slots and crowd out doc diversity. Either
   raise the budget (~×avg-passages) or fold-to-node *before* applying the rerank
   limit. This is the main retrieval-quality subtlety; needs a recall check.
3. **Supersession / excise.** When a node is tombstoned (`superseded_at`) or excised,
   its passage vector rows must be excluded. The fold step must join
   passages→`canonical_nodes` and skip superseded, or supersession must tombstone
   passage rows too. **Highest-risk correctness surface.**
4. **Storage / ingest cost.** ~3.5× vector rows (5,364 passages / 1,500 docs in the
   IR-C corpus) → ~3.5× vector-index size and embed time at ingest. Acceptable?
5. **Backfill.** Existing corpora need re-projection to gain passages (a migration/
   backfill job re-running the projection over `canonical_nodes`).

## Effort

Medium, multi-day, touching schema + write + search but mostly mechanical. The
identity/supersession edge cases (Q3) and candidate-budget tuning (Q2) are where the
real care goes. The text arm and node-identity model are untouched, which is what
keeps Option A tractable.

## References

- Dense-arm evidence: `dev/plans/runs/performance-output-and-compare.md` (2026-06-10d/e)
- Harness implementing the recipe: `src/rust/crates/fathomdb-engine/tests/ir_c_fusion_experiment.rs`
- Engine seams: `lib.rs` (`run_projection_job` ~4371, `commit_projection_outcomes`
  ~4850, `fuse_rrf` ~3620, `read_search_in_tx` ~3852); schema `fathomdb-schema/src/lib.rs`
  (`_fathomdb_vector_rows` ~176, `vector_default` pack2 ~5591, `search_index` ~158).
