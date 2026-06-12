# IR-C Roadmap — Deep-research report (architectural levers + graph retrieval)

Status: **deep-research, cited + adversarially verified** · 2026-06-12 · Step 0 of the
IR-C roadmap orchestration (feeds the analysis dossier, Prompt A).
Method: `deep-research` workflow (Opus/Sonnet), 103 agents, fan-out search → fetch →
3-vote adversarial verification → synthesis. All findings below survived verification
(votes shown). **Evidence labeled STRONG vs WEAK per the caveats section.**

## Executive summary

For FathomDB's CPU-only / no-API / 1-bit-quantized footprint, the highest-confidence,
footprint-compatible lever against the exploratory/discourse bottleneck is a **small CPU
cross-encoder reranker** (FlashRank-class, 4–34 MB, ONNX, no Torch/GPU): on zero-shot
BEIR a ~22M MiniLM cross-encoder lifts nDCG@10 **+0.056** over BM25 and even beats a 4.8B
dense retriever — but it is **candidate-recall-bound** (cannot recover the ~38% hard
queries outside the first-stage union, cannot pass the ~0.62 oracle-union ceiling).
**Vector pseudo-relevance feedback** is the one footprint-clean first-stage-recall lever
(no extra neural inference). **Graph retrieval** (HippoRAG-2, LightRAG, Zep/Graphiti) is
the most promising structural answer to multi-hop/temporal discourse — and **Zep's
retrieval-time mechanism maps almost 1:1 onto FathomDB's SQLite+graph substrate** — but
**every published graph system needs an LLM for graph *construction* at indexing time**,
which is the binding footprint constraint, not the retrieval math. Net order: **rerank
first; add vector-PRF; pursue graph where the only added cost is a local extraction LLM at
index time** (retrieval-time traversal is footprint-clean).

## (a) Ranked shortlist of footprint-compatible levers

| # | Lever | Effect (cited) | Footprint | Confidence |
|---|-------|----------------|-----------|------------|
| 1 | **Small CPU cross-encoder reranker** (FlashRank: ms-marco-TinyBERT-L-2 ~4MB → MiniLM-L-12 ~34MB; ms-marco-MiniLM-L6 22.7M) | ~22M MiniLM rerank of BM25 top-1000: **BEIR nDCG@10 0.4328→0.4889 (+0.056)**; beats GTR-4.8B (0.458) & ColBERT-v2 (0.478) | ✅ CPU/ONNX, no Torch/GPU/API | **STRONG** |
| 2 | **Vector pseudo-relevance feedback (PRF)** — avg query vec with top-k passage vecs | modest, condition-dependent; **no extra neural inference**, ~1/20th BM25+BERT time | ✅ footprint-clean (operates on f32 vecs) | **STRONG** |
| 3 | **Graph retrieval — Zep-style** (cosine + BM25 + BFS n-hop, fused RRF/MMR + node-distance rerank) | end-to-end (not recall): Zep **+11 pts LongMemEval** (gpt-4o 71.2 vs 60.2), −90% latency, 115k→1.6k tokens | ⚠️ retrieval-time clean; **needs LLM at index time** | STRONG (mechanism) / WEAK (vendor, end-to-end) |
| 4 | **Graph retrieval — HippoRAG-2** (Personalized PageRank over LLM-built KG) | end-to-end: **+7%** assoc.-memory over NV-Embed-v2; Recall@5 MuSiQue 69.7→74.7, 2Wiki 76.5→90.4 | ⚠️ ref stack assumes multi-GPU + 7B embedders; **index-time LLM** | STRONG (multi-hop) / WEAK (footprint) |
| 5 | **LightRAG dual-level KG+vector** (local/global/hybrid/naive modes) | architecture template, not a single number | ⚠️ index-time LLM (supports *local* open models, e.g. Qwen3-30B) | STRONG (design) |

**Rejected / flagged (footprint violations):**
- **Late-interaction (ColBERTv2 / PLAID / EMVB)** — multi-vector centroid+residual / PQ
  scheme, **architecturally incompatible** with single-vector 1-bit Hamming storage; tens-
  to-hundreds of ms/query on CPU even with PLAID's 45× speedup. **OUT.** [STRONG]
- **HyDE / ReDE-RF query expansion** — require a generative/judge **LLM in the query loop**
  → footprint-marginal unless a local CPU LLM is accepted at query time. [STRONG]
- **Large rerankers** (bge-reranker-v2-m3 568M, mxbai-rerank-large 435M) — better quality
  but heavy; the 60M tier is *net-negative* vs BM25 per Rosa et al., so stay in the
  ~22–34M ONNX band and expect the **low end** of the +0.056 range. [STRONG caveat]

## (b) Graph node / edge / both — the trade-off for an agent-memory store

- **Edge-centric (Graphiti/Zep, bi-temporal):** edges carry four timestamps
  (created/expired transactional + valid/invalid event-time); a new fact **invalidates**
  the old edge rather than statically accumulating. This is Zep's distinctive mechanism
  over flat-embedding Mem0 and is **reproducible in an on-device SQLite+graph store**
  (FathomDB's edge substrate already exists). Best fit for **temporal/contradiction**
  memory queries. [STRONG mechanism; WEAK that it *alone* drives Zep's LongMemEval edge —
  the paper credits selective context retrieval as the headline driver.]
- **Node-centric (HippoRAG PPR over phrase+passage nodes; HyperGraphRAG reified
  fact-nodes):** Personalized PageRank over an LLM-built KG does **multi-hop in one
  retrieval step** — the associativity regime dense+lexical miss. Best fit for
  **multi-hop** discourse. [STRONG for multi-hop; footprint cost = index-time LLM + (ref
  stack) GPU.]
- **Both (LightRAG dual-level; Zep search = cosine + BM25 + BFS):** the production pattern
  is *hybrid* — graph traversal as a **third candidate arm** fused with dense+lexical via
  RRF, not a graph-only system. This is the directly-applicable template and maps onto
  FathomDB's existing dense+FTS5+RRF + graph plan.

**The binding constraint is construction, not retrieval.** Every graph system needs an LLM
for entity/relation/triple extraction at **index time**; substituting a non-LLM extractor
(REBEL) causes large drops, so the LLM is load-bearing. Retrieval-time graph math (BFS,
PPR, RRF) is **GPU/API-free**. So the footprint question for FathomDB is specifically:
*can a small local CPU extraction LLM build a good-enough graph at index time?* (untested;
small models show extraction-quality/JSON-stability degradation).

## (c) Footprint flags (explicit)

| Option | Local-first / CPU / no-API / 1-bit-safe? |
|---|---|
| CPU cross-encoder reranker (FlashRank ONNX) | ✅ fully compatible |
| Vector PRF | ✅ fully compatible (operates on f32 rerank vectors) |
| Graph retrieval-time traversal (BFS/PPR/RRF) | ✅ compatible (no neural inference) |
| Graph **construction** (entity/relation extraction) | ⚠️ needs an LLM at index time; local CPU LLM feasibility untested |
| ColBERT / PLAID / EMVB late interaction | ❌ incompatible with single-vector 1-bit Hamming; CPU-costly |
| HyDE / ReDE-RF query expansion | ❌/⚠️ needs an LLM in the query loop |
| HippoRAG reference stack | ❌ assumes multi-GPU + 7B embedders (the *idea* — PPR — is portable) |

## (d) Evidence strength (verbatim caveats)

- **STRONG (primary, unanimous):** reranker BEIR numbers (Rosa et al. 2022); FlashRank
  CPU-only; ColBERT/EMVB 1-bit incompatibility; PLAID CPU latency; graph systems' LLM-
  indexing dependence; Zep retrieval mechanism + bi-temporal model.
- **WEAK / interpretive:** (1) **graph-system gains (HippoRAG-2 +7%, Zep +11 pts) are
  end-to-end ANSWER accuracy, not first-stage recall** — they do NOT directly predict
  R@10/R@50 lift on FathomDB's bottleneck (the single most important caveat). (2) Zep's
  paper is **vendor-authored**, self-reported. (3) The causal claim that Zep's bi-temporal
  graph drives its edge is plausible but the paper credits selective context retrieval.
  (4) Reranker numbers are from 2022; small distilled rerankers underperform large ones, so
  expect the **low end** of +0.056 and remember it's **candidate-recall-bound** (can't pass
  ~0.62 union). (5) Local-LLM graph indexing on CPU is asserted "supported" but extraction
  quality/latency at FathomDB scale is **untested**. (6) **No source gives a measured recall
  lift on FathomDB's own corpus** — all effect sizes are transferred from external benchmarks.

## Sources (primary)
Rerankers/BEIR: arXiv 2206.02873 (Rosa et al.) · github.com/PrithivirajDamodaran/FlashRank ·
sbert.net cross-encoder efficiency. Query expansion: arXiv 2212.10496 (HyDE), 2410.21242
(ReDE-RF), 2511.19349. PRF: arXiv 2108.11044. Late-interaction: arXiv 2205.09707 (PLAID),
2404.02805 (EMVB). Graph: arXiv 2502.14802 (HippoRAG-2) + github osu-nlp-group/hipporag ·
github HKUDS/LightRAG · arXiv 2501.13956v1 (Zep/Graphiti).
