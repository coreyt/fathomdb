# Slice 5 Design Memo — R0: Candidate-Recall CDF + Rerank Cost Model

**Status:** implementation memo  
**Slice:** 0.8.1 Slice 5  
**Binding spec:** `dev/adr/ADR-0.8.1-ir-measure-eval-design.md` §2  
**Authored:** 2026-06-13

---

## 1. Architecture overview

Slice 5 is a **read-path measurement** slice. Zero engine source changes. The
`ir_c_cdf_run.rs` integration test (gated `IRC_RUN=1`) seeds the frozen corpus
into a fresh temp engine, then queries each of four retrieval arms over all five
K values and all gold query classes, producing `dev/plans/runs/IR-C-recall-cdf.json`.

The test file is self-contained: it includes `corpus_subset.rs`, `ir_eval.rs`,
and `ir_retrieval.rs` via `#[path = ...]` (the same pattern as the adjacent
`ir_c_gold_diagnostics.rs` and `ir_c_recall_run.rs`).

---

## 2. Four retrieval arms

### 2.1 `bm25_text` arm

**Implementation:** Direct FTS5 SQL via `fts_bodies()` + `map_bodies()` from
`support/ir_retrieval.rs` — the same seam as `ir_c_gold_diagnostics.rs`. Uses
content-OR compilation (`compile_content_or`) to avoid AND-join near-zero recall
on natural-language questions.

**Query pattern:**

```sql
SELECT body FROM search_index
  WHERE search_index MATCH <content_OR_expression>
  ORDER BY bm25(search_index)
  LIMIT 1000
```

The bm25_text arm opens a **read-only SQLite connection** to the engine's DB
file after seeding (FTS is written synchronously by `engine.write()`). Retrieve
at K=1000 once per query, then slice the result list for smaller K values.

**Engine setup:** Seed with the real BGE embedder (same as dense/fused arms);
FTS is committed synchronously regardless of whether the projection scheduler
has drained. No separate FTS-only seeding pass is needed — seeding with drain
writes both FTS and vector projections.

### 2.2 `dense` arm

**Implementation:** `engine.set_vector_stage_only_for_test(true)` around
`engine.search(query)`. The vector-stage-only path bypasses the FTS arm of
RRF fusion, giving a dense-only ranking.

Before any query on the dense arm, call:

```rust
engine.set_search_limit_for_test(1000);
```

This raises the KNN fanout to 1000 so that top-1000 dense results are actually
available. Without this, the default fanout cap would truncate at a lower K.

After the arm completes, reset with `engine.set_vector_stage_only_for_test(false)`.

**Feature gate:** `#[cfg(feature = "default-embedder")]` — the entire IRC_RUN
runner is gated on this feature (dense/fused require real embeddings).

### 2.3 `rrf_fused` arm

**Implementation:** `engine.search(query)` (the unconditional RRF-hybrid
production path). Same fanout override (`set_search_limit_for_test(1000)`)
before queries. This is the existing production arm.

Map result bodies → doc_ids via the body→doc_id table (first-occurrence rule
for duplicate bodies, same as `ir_c_recall_run.rs`).

### 2.4 `oracle_union` arm

**Definition:** A query is "found@K" in the oracle if the gold document appears
in **either** the bm25_text top-K **or** the dense top-K. This is the best
achievable by a reranker that has perfect knowledge of which arm is better per
query — an upper bound on reranker recall at any given depth.

**Implementation:** Compute from the two ranked lists. For each K in {50, 100,
200, 500, 1000}:

- `bm25_found = any required gold doc in bm25_text_results[:K]`
- `dense_found = any required gold doc in dense_results[:K]`
- `oracle_found = bm25_found OR dense_found`

No additional engine queries are needed; oracle is derived from the bm25_text
and dense arm results already collected.

---

## 3. `found@K` calculation

For each combination of (arm, query_class, K):

1. Iterate over positive gold queries (skip `Negative` class).
2. Filter to queries whose required gold doc is present in the seeded corpus
   (same "evidence-presence filter" as `ir_c_recall_run.rs`).
3. Retrieve the top-K results for this arm.
4. A query is "found@K" if **ANY** required gold doc appears in the top-K result
   list. This is the standard candidate recall definition (not strict all-of,
   which is Evidence Recall@K — here we measure found@K for the CDF).
5. `found_at_k = n_found / n_eligible` where `n_eligible` = queries in this
   class with at least one required doc in the corpus.

**Gold class mapping:** The gold set uses `exact_fact` and `exploratory` labels
(from `QueryClass::label()`). The ADR §2.1 says "factoid and exploratory" but
the gold set encodes `exact_fact`. The artifact uses the gold set labels
(`exact_fact` / `exploratory`) for consistency with all other IR-C infrastructure.
This mapping is noted in the design memo per the slice prompt.

Other classes (commitment, action, preference) are also present in the gold set.
The artifact emits rows for ALL non-negative classes that have at least one
eligible query, so the total row count may exceed 40.

---

## 4. K=1000 handling for dense arm

`engine.set_search_limit_for_test(1000)` raises the vector fanout (KNN candidate
pool) to 1000. This must be called BEFORE any dense or fused query. The setting
persists for the duration of the test (no per-query reset needed since all our K
values ≤ 1000). After the full run, no reset is needed (the test engine is
discarded).

Per the existing `run_experiment` pattern: the deepest K in the ladder determines
the fanout. For the CDF ladder {50, 100, 200, 500, 1000}, the fanout is set to
1000 at the start.

---

## 5. Artifact schema and corpus_hash pin

Output artifact: `dev/plans/runs/IR-C-recall-cdf.json`

Written to the **canonical** path (`$CANON/dev/plans/runs/IR-C-recall-cdf.json`),
not the worktree — this is the same deliberate exception as `output.json`.

The `corpus_hash` in the artifact is read from `tests/corpus/snapshot.json` and
verified against the gold set's pinned hash. A mismatch aborts the run.

**ADR §2.3 / slice prompt discrepancy noted:** The ADR §2.3 shows `"latency"` as
a single JSON object. The slice prompt §3.3 shows `"latency"` as a list of two
objects (one per model). This slice implements the list form (two objects, one per
model) which is the only form that can carry both TinyBERT-L-2 and MiniLM-L6.
The discrepancy is flagged in `output.json`.

---

## 6. CE latency benchmark

Implemented as `dev/scripts/ir_c_ce_latency.py` (Python script, separate step).

**Libraries:** FlashRank (`pip install flashrank`) as the primary choice; falls
back to sentence-transformers if FlashRank is unavailable. Both provide
cross-encoder scoring.

**Models:**

- TinyBERT-L-2 (~4 MB): `cross-encoder/ms-marco-TinyBERT-L-2`
- MiniLM-L6 (~22.7 MB): `cross-encoder/ms-marco-MiniLM-L6-v2`

**Sampling:** ≥1,000 random (query, passage) pairs from the gold queries and
corpus docs. Each query paired with a random passage from a random doc.

**Timing:** Measure the `score()` call time (ms/pair) over the sample, compute
p50 (median) and p95 using `numpy.percentile`. Include hardware note (CPU model
from `/proc/cpuinfo` or `platform.processor()`).

**Output:** Read the existing `IR-C-recall-cdf.json`, patch in the `latency`
list (two entries), write back.

---

## 7. Sequence and commit plan

1. `dev/design/slice-5-design.md` (this file) — commit 1
2. `ir_c_cdf_run.rs` schema test (RED) — commit 2
3. `ir_c_cdf_run.rs` full CDF runner (GREEN) — commit 3
4. `dev/scripts/ir_c_ce_latency.py` — commit 4
5. `dev/plans/runs/IR-C-recall-cdf.json` artifact (after IRC_RUN=1 run) — commit 5
6. `dev/plans/runs/IR-C-r0-findings.md` findings — commit 6
7. `dev/DOC-INDEX.md` update — commit 7 (folded with findings)
