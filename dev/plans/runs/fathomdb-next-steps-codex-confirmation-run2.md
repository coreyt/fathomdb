<!-- Date: 2026-06-19 · Model: codex (via `codex exec`) · INDEPENDENT CONFIRMATION PASS -->
<!-- Inputs: the neutral experiment compilation (dev/plans/runs/fathomdb-retrieval-experiments-compilation.md)
     + the four goals + neutral system facts (architecture, footprint invariant, benchmark/metric defs,
     measurement protocol). Deliberately WITHHELD: any direction/leaning, the D1/D2 frame, the prior
     codex hypothesis, the QA appendix, and the roadmap. codex was instructed to reason from data+facts
     only and not read those files — the purpose is to test whether it independently arrives at the same
     approaches and sequence. -->

# Next Two Engineering Steps

## Step 1: Make Dense Retrieval Correct, Then Recalibrate Fusion

### Recommendation

First, fix the production dense path so the shipped Rust embedder uses **CLS pooling for bge-small-en-v1.5**, then re-measure the full retrieval stack before adding new retrieval machinery.

This is the highest-leverage first move because dense retrieval is already central to exploratory recall and multi-hop QA, but the observed system has a production/eval mismatch: production uses Mean pooling while the eval harness used CLS pooling, and CLS is the model-correct pooling for bge-small.

### Method Sequence

1. **Query transform / embedder**
   - IN-LIBRARY: change Rust query embedding for bge-small-en-v1.5 from Mean pooling to CLS pooling.
   - IN-LIBRARY: rebuild/index with CLS-consistent document embeddings where required.

2. **Dense retrieval**
   - IN-LIBRARY: 1-bit ANN Hamming search.
   - IN-LIBRARY: f32 rerank of the ANN candidate pool.

3. **Lexical retrieval**
   - IN-LIBRARY: FTS5/BM25 unchanged as the lexical arm.

4. **Fusion**
   - IN-LIBRARY: keep deterministic RRF `k=60` as the comparator-compatible fusion path.
   - IN-LIBRARY: evaluate a conservative arm-preserving evidence policy: dense top-N, BM25 top-N, then RRF top-K, with deterministic duplicate removal. Do not introduce LLM logic.

5. **Cross-encoder**
   - EVAL-ONLY for now. The measured QA runs do not justify putting the CPU cross-encoder in the default search path: `fused_rerank` was below fused on full gpt-5.4 pooled >=3-hop F1, `0.4152` vs `0.4502` (§4.7), and essentially flat in the N=100 pilot, `0.3060` vs `0.3064` (§4.3).

### Placement / Footprint Compliance

- CLS pooling fix: IN-LIBRARY, CPU-only, deterministic.
- 1-bit ANN + f32 rerank: IN-LIBRARY, already footprint-compliant.
- BM25/FTS5: IN-LIBRARY.
- RRF `k=60`: IN-LIBRARY, deterministic.
- Arm-preserving evidence selection: IN-LIBRARY, deterministic.
- Cross-encoder: EVAL-ONLY until it beats the default path under the QA and recall bars.

### Goals Advanced

- Best agentic memory: LongMemEval recall and answer accuracy.
- Exploratory recall: dense and fused IR relevance.
- Deep-exploratory recall: hard queries where dense rank matters.
- Multi-hop QA: bridge coverage and >=3-hop F1.

### What To Measure

Use the corrected CLS path as a new measured baseline.

Numeric bars:

- **eu7 ANN fidelity:** pass the hard floor with margin: full 7,667-query vector-stage recall@10 must be `>= 0.900`, compared with the measured `0.8960` full-corpus point and CI `[0.8640, 0.9250]` (§2.1). Prefer CI lower bound `>= 0.900` before treating the quantized store as robust.
- **Agentic memory recall:** LongMemEval pooled R@10 must beat the best measured N=160 baseline, naive BM25 `0.625`, by at least `+0.04`, so bar `>= 0.665` (§3.1).
- **Agentic memory answer accuracy:** answer accuracy should beat the best measured N=160 overall baseline, naive BM25 `0.406`, by at least `+0.04`, so bar `>= 0.446` (§3.2).
- **Multi-hop bridge coverage:** all-bridges-in-top-10 frequency should exceed the best observed arm, passage dense `0.68`, so bar `>= 0.72` on the same diagnostic (§4.4). This is a better first gate than priced answer F1 because all-bridge presence corresponded to mean answer-F1 `0.510` versus `0.068` when any bridge was missing (§4.4).
- **MuSiQue >=3-hop QA:** do not require a formal GO on the existing rule at this stage because the decision rule was observed to be underpowered. Require point-estimate F1 no worse than passage dense `0.4866` and at least `+0.04` over fused `0.4502`, so bar `>= 0.4902` on pooled >=3-hop (§4.7).

### Dependencies / Ordering Rationale

Do this before graph, enrichment, or query decomposition work because the dense arm is a substrate dependency. Passage dense was the strongest full-run >=3-hop QA arm at `0.4866`, above fused `0.4502`, BM25 `0.3700`, fused_rerank `0.4152`, and ppr_fusion `0.4097` (§4.7). If the production dense path is pooling-wrong, every downstream comparison involving fusion, reranking, bridge coverage, and QA is contaminated.

---

## Step 2: Build Offline Enrichment For Bridge Coverage, Expose It Through Deterministic Multi-Query Retrieval

### Recommendation

Second, use the local Qwen extraction pipeline offline to build a structured entity/fact/alias retrieval layer, but do not repeat the measured weak forms: body-less graph traversal alone and append-to-body enrichment are not enough. The target should be **bridge coverage**, because bridge completeness is the clearest measured predictor of QA success.

### Method Sequence

1. **Offline extraction**
   - OFFLINE-BUILD: run local Qwen3.6-27B to extract entities, aliases, facts, temporal markers, and document-level relation hints.
   - OFFLINE-BUILD: store extracted keys in separate deterministic index fields/tables, not blindly appended to the main FTS body.

2. **Index transform**
   - IN-LIBRARY: add structured FTS/searchable fields for entity aliases and extracted facts.
   - IN-LIBRARY: keep original document text BM25 separate to avoid length-normalization damage.

3. **Caller-side query decomposition**
   - CALLER-SIDE BYO-LLM: for QA callers, optionally decompose a multi-hop question into subqueries/entities. The library receives plain deterministic search calls only.
   - IN-LIBRARY fallback: if no caller decomposition is supplied, run the original query unchanged.

4. **Retrieval sequence per query/subquery**
   - IN-LIBRARY: BM25 over original body.
   - IN-LIBRARY: BM25/FTS over extracted entity/fact fields.
   - IN-LIBRARY: CLS-correct dense 1-bit ANN -> f32 rerank.
   - IN-LIBRARY: deterministic RRF across body BM25, enrichment-field BM25, and dense.
   - IN-LIBRARY: deterministic evidence selection that favors bridge diversity across subqueries while preserving stable ordering.

5. **Pseudo-relevance feedback**
   - IN-LIBRARY, optional experiment: deterministic PRF from top retrieved entity/fact fields only, with a capped second pass.
   - No generated text, no in-library LLM.

6. **Graph/PPR**
   - EVAL-ONLY unless this structured bridge layer changes the measured picture. Existing body-less BFS and PPR results do not support making graph traversal a default path.

### Placement / Footprint Compliance

- Qwen extraction: OFFLINE-BUILD only.
- Entity/fact/alias indexes: IN-LIBRARY data structures, deterministic.
- Caller query decomposition: CALLER-SIDE BYO-LLM only.
- BM25/dense/RRF/evidence selection: IN-LIBRARY, CPU-only, deterministic.
- PRF: IN-LIBRARY only if deterministic and bounded.
- Graph/PPR: EVAL-ONLY until it clears the bars.

### Goals Advanced

- Best agentic memory: extracted facts and temporal markers should help factoid, knowledge-update, multi-session, and temporal recall.
- Exploratory recall: aliases/facts add lexical handles for documents dense may miss.
- Deep-exploratory recall: bridge-diverse evidence selection targets hard discrimination.
- Multi-hop QA: directly optimizes all-bridge retrieval.

### What To Measure

Numeric bars:

- **Bridge coverage first:** all-bridges-in-top-10 frequency `>= 0.75`, versus passage_dense `0.68`, fused-RRF `0.65`, fused_rerank `0.64`, and BM25 `0.51` (§4.4).
- **Pooled >=3-hop QA:** only run priced answerer after bridge bar passes. Require point-estimate F1 `>= 0.5266`, which is `+0.04` over the strongest measured full-run arm, passage_dense `0.4866` (§4.7). Because the decision rule was underpowered, treat CI lower bound as directional rather than a hard GO gate for this step; require no large regression signal, e.g. CI lower not below `-0.02` versus passage_dense.
- **Agentic memory:** enriched retrieval must beat the N=160 BM25 R@10 baseline `0.625` by at least `+0.04`, so `>= 0.665` (§3.1), and must not reproduce the append-to-body regression where FathomDB FTS enriched was `0.775` versus FTS-only `0.800` on N=40 (§3.4).
- **BM25/enrichment interaction:** include a b-sweep or equivalent field-isolation check because enrichment varied with BM25 length normalization: b=`0.00` enriched reached `0.775`, while b=`0.75` enriched was `0.75` (§3.5).

### Dependencies / Ordering Rationale

This step depends on Step 1 because enrichment and multi-query retrieval need a trustworthy dense arm. It also depends on avoiding two measured weak paths: graph_ON did not improve over graph_OFF on pooled R@10, both `0.65`, in the post-filter N=40 run (§3.3), and ppr_fusion trailed fused on full gpt-5.4 pooled >=3-hop F1, `0.4097` vs `0.4502` (§4.7). The enrichment should therefore be structured around bridge coverage, not generic graph traversal.

---

## Overall Ordering Rationale

Fix the dense substrate first, then build bridge-focused enrichment on top. Dense retrieval is already the strongest measured QA signal, while bridge completeness is the strongest measured causal-looking diagnostic for answer quality: all bridges present yielded mean answer-F1 `0.510`, missing any bridge yielded `0.068` (§4.4). Graph/PPR and cross-encoder reranking should stay out of the default library path until they beat the simpler corrected dense/fusion baseline.

## Falsifiable Hypothesis

If FathomDB corrects bge-small pooling to CLS and adds structured offline entity/fact enrichment with deterministic multi-query evidence selection, then pooled >=3-hop MuSiQue F1 will reach at least `0.5266`, while all-bridges-in-top-10 reaches at least `0.75`; failure to hit the bridge bar falsifies the enrichment/evidence-selection direction before spending on larger answerer runs.
