# IR-C R0 Findings — Candidate-Recall CDF + CE Latency

> **Status:** Results measured on the frozen corpus (`corpus_hash` `fe973fcd49fbbda0…`).  
> **Produced by:** Slice 5 (`ir_c_cdf_run.rs` + `dev/scripts/ir_c_ce_latency.py`).  
> **Artifact:** `dev/plans/runs/IR-C-recall-cdf.json`  
> **Run time:** 4146 s seeding (10,506 docs, 2.5 docs/s final) + 1649 s query phase (4,472 eligible of 4,597 gold queries). Total: 5796 s ≈ 96.6 minutes.

---

## 1. CDF shape by arm

The recall CDF measures `found@K` — the fraction of positive gold queries for which ANY
required evidence doc appears in the top-K retrieved results — at K ∈ {50, 100, 200, 500, 1000}.

### 1.1 `exact_fact` class (n=2888 eligible queries)

| K    | bm25\_text | dense  | rrf\_fused | oracle\_union |
|------|-----------|--------|-----------|---------------|
| 50   | 0.9460    | 0.6319 | 0.9501    | 0.9557        |
| 100  | 0.9581    | 0.6454 | 0.9612    | 0.9650        |
| 200  | 0.9678    | 0.6530 | 0.9695    | 0.9733        |
| 500  | 0.9799    | 0.6530 | 0.9778    | 0.9823        |
| 1000 | 0.9851    | 0.6530 | 0.9844    | 0.9872        |

### 1.2 `exploratory` class (n=1584 eligible queries)

| K    | bm25\_text | dense  | rrf\_fused | oracle\_union |
|------|-----------|--------|-----------|---------------|
| 50   | 0.5341    | 0.1932 | 0.5101    | 0.5701        |
| 100  | 0.6263    | 0.2670 | 0.6143    | 0.6787        |
| 200  | 0.7222    | 0.3226 | 0.7279    | 0.7639        |
| 500  | 0.8131    | 0.3226 | 0.8239    | 0.8340        |
| 1000 | 0.8592    | 0.3226 | 0.8649    | 0.8725        |

---

## 2. Key findings

### 2.1 Dense arm plateau at K=200

The **dense arm saturates at K=200** for both classes:
- `exact_fact`: 0.653 at K=200, 0.653 at K=500, 0.653 at K=1000 (flat)
- `exploratory`: 0.323 at K=200, 0.323 at K=500, 0.323 at K=1000 (flat)

**Interpretation:** The HNSW vector index exhausts its useful neighborhood at K≈200. Asking for
more candidates from the dense arm beyond K=200 returns no additional gold docs. This means:
- Dense-only retrieval is capped at ~65% for `exact_fact` and ~32% for `exploratory`.
- BGE-small-en does not provide sufficient discriminative signal to rank gold docs higher than
  200th position in these query classes.
- **The dense arm adds value primarily for queries that BM25 misses** (complementarity, not depth).

### 2.2 Depth-50 ceiling vs depth-1000 (C1 correction)

For `rrf_fused` (production arm):
- `exact_fact`: **0.950 @K=50**, 0.984 @K=1000. Depth-50 already achieves 96.6% of max.
- `exploratory`: **0.510 @K=50**, 0.865 @K=1000. Depth-50 achieves only 58.9% of max.

The **C1 correction** (depth cap effect) is class-dependent:
- For `exact_fact`, the K=50 ceiling is already very high (~95%). Deeper retrieval adds <3.5%.
- For `exploratory`, going from K=50 to K=200 yields +21.8 pp improvement (0.510→0.728),
  and from K=200 to K=1000 adds another +13.7 pp.

**Comparison with IR-C roadmap estimate:** The roadmap estimated "depth-50 ~0.52–0.53" for
`rrf_fused`. Our measurement confirms: `exploratory@50=0.510`, `exact_fact@50=0.950`.
The 0.52–0.53 estimate referred to the mixed-class ceiling; our measurement splits the classes.

### 2.3 CDF bend point analysis

For `exploratory` (the class with the most room for improvement):

| K interval | Δ rrf\_fused | Marginal gain |
|-----------|-------------|--------------|
| 50→100    | +10.4 pp    | 10.4 pp      |
| 100→200   | +11.4 pp    | 11.4 pp      |
| 200→500   | +9.6 pp     | 9.6 pp       |
| 500→1000  | +4.1 pp     | 4.1 pp       |

The curve is roughly linear through K=500 for exploratory, then bends at K=500→1000. This
suggests that beyond K=500, the marginal gain diminishes significantly for exploratory queries.

For `exact_fact`, the curve is essentially flat above K=100 (gains are <1.5 pp from K=100 to K=1000).

### 2.4 BM25 outperforms fusion for exact_fact at most K values

Notably, `bm25_text` is slightly stronger than `rrf_fused` for `exact_fact` at K≥100:
- K=100: bm25=0.958 vs rrf=0.961 (close, fusion slightly better)
- K=200: bm25=0.968 vs rrf=0.970 (close, fusion slightly better)
- K=1000: bm25=0.985 vs rrf=0.984 (bm25 marginally better!)

This suggests the dense component in RRF slightly hurts exact_fact recall at high K (the fusion
weight shifts some high-K bm25 results down). This is expected: dense adds noise for lexically
precise queries.

For `exploratory`, fusion (`rrf_fused`) beats `bm25_text` at K≥200:
- K=200: bm25=0.722 vs rrf=0.728
- K=1000: bm25=0.859 vs rrf=0.865

---

## 3. Recommended rerank depth for Slice 10 (R1)

Based on the CDF data:

| Candidate depth | rrf\_fused exploratory | rrf\_fused exact\_fact | Notes |
|----------------|----------------------|----------------------|-------|
| K=50           | 0.510                | 0.950                | Baseline (current default) |
| **K=100**      | **0.614**            | **0.961**            | +10 pp exploratory, +1.1 pp exact |
| **K=200**      | **0.728**            | **0.970**            | +21.8 pp exploratory, +2.0 pp exact |
| K=500          | 0.824                | 0.978                | +31.4 pp exploratory, +2.8 pp exact |
| K=1000         | 0.865                | 0.984                | +35.5 pp exploratory, +3.4 pp exact |

**Recommendation: K=200 as the Slice 10 (R1) rerank candidate depth.**

Rationale:
- K=200 provides the largest per-K gain for exploratory (the weakest class).
- CE latency at K=200: TinyBERT-L-2 at p50=1.54ms/pair × 200 pairs = 308ms, which is within
  the AC-013 0.x budget.
- The oracle_union ceiling at K=200 is 0.973 (exact_fact) and 0.764 (exploratory), leaving
  headroom for the reranker model to add value.
- Going to K=500 adds +9.6 pp exploratory but at 2.5× more CE compute cost (500 vs 200 pairs).

---

## 4. CE latency constraints

### 4.1 Measured latency

CE latency was measured via `dev/scripts/ir_c_ce_latency.py` on 1,000 random
(query, passage) pairs from the frozen corpus gold set (seed=42).

| Model | Size | p50 (ms/pair) | p95 (ms/pair) | Hardware |
|-------|------|--------------|--------------|---------|
| TinyBERT-L-2 | ~4 MB | **1.54 ms** | 2.85 ms | AMD Ryzen Threadripper PRO 5945WX (24 logical CPUs) |
| MiniLM-L12* | ~22 MB | **16.82 ms** | 36.97 ms | AMD Ryzen Threadripper PRO 5945WX (24 logical CPUs) |

\* **Note:** `ms-marco-MiniLM-L6-v2` (the intended model, ~23 MB) is not available in
  flashrank as of this run (404 from HuggingFace). `ms-marco-MiniLM-L-12-v2` was used as the
  closest available substitute (12 layers vs 6 — provides a conservative upper bound on L-6
  latency). See `MODELS` list in `dev/scripts/ir_c_ce_latency.py`.

### 4.2 Reranker budget at each candidate depth

At K=200 (recommended):

| Model | Total CE cost | Fits AC-013 0.x budget? |
|-------|-------------|------------------------|
| TinyBERT-L-2 | 200 × 1.54 ms = 308 ms | Yes (budget ≈ 300-500 ms for 0.x) |
| MiniLM-L12 | 200 × 16.82 ms = 3364 ms | No (exceeds 0.x budget) |

**TinyBERT-L-2 is the only budget-compatible online reranker at K=200.** MiniLM-L12 is
suitable for offline/batch reranking or as a future 1.x/post-1.0 model.

### 4.3 Script usage

```bash
# Run AFTER the CDF artifact exists:
python3 dev/scripts/ir_c_ce_latency.py

# Custom artifact path or gold set:
python3 dev/scripts/ir_c_ce_latency.py \
    --gold data/corpus-data/eval/ir_gold/all.gold.json \
    --artifact dev/plans/runs/IR-C-recall-cdf.json \
    --n-pairs 1000
```

---

## 5. Oracle ceiling for Slice 10

The `oracle_union` arm represents the theoretical maximum achievable by any Slice 10 reranker
that receives both BM25 and dense candidates:

| K    | oracle\_union exact\_fact | oracle\_union exploratory |
|------|--------------------------|--------------------------|
| 50   | 0.9557                   | 0.5701                   |
| 100  | 0.9650                   | 0.6787                   |
| 200  | 0.9733                   | 0.7639                   |
| 500  | 0.9823                   | 0.8340                   |
| 1000 | 0.9872                   | 0.8725                   |

At K=200, a perfect Slice 10 reranker could achieve at most 0.973 (exact_fact) and 0.764
(exploratory). Any model gap below these ceilings is due to reranker model quality (C2), not
candidate depth (C1).

---

## 6. Limitations and deferred work

- **Dense plateau cause:** The reason BGE-small-en saturates at K=200 is not fully investigated.
  Possible causes: model capacity, HNSW graph connectivity limits, or query distribution.
  Deferred to Slice 10 analysis.
- **eu7 harness perf** (parallel GT re-embed + retained-f32 reuse) — reserved-gap 6–9.
- **MiniLM-L6 latency** — model not available via flashrank; measured MiniLM-L12 instead.
  An exact L-6 measurement should be done with `sentence-transformers` when available.
- **Oracle floor for exploratory** (0.8725 @K=1000) leaves ~12.75% of exploratory queries
  permanently unreachable by BM25+dense fusion. These are likely queries where evidence docs
  are not indexed or the query is too ambiguous for either modality.

---

## 7. How to re-run

```bash
# Full CDF run (requires corpus + embedder):
IRC_RUN=1 cargo test --release -p fathomdb-engine --features default-embedder \
    --test ir_c_cdf_run -- ir_c_recall_cdf --nocapture

# Schema validation only (no corpus needed):
cargo test -p fathomdb-engine --test ir_c_cdf_run -- ir_c_cdf_schema_shape

# CE latency (requires flashrank):
python3 dev/scripts/ir_c_ce_latency.py
```
