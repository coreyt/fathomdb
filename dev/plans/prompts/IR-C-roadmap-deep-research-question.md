# Deep-research question (Step 0 — feeds Prompt A)

**Engine:** `deep-research` workflow (Workflow tool). **Model:** Opus or Sonnet (NOT Fable).
**Output:** save the synthesized report to `dev/plans/runs/IR-C-roadmap-deep-research.md`.

## Question (pass as `args`)

Research the **research-oriented architectural changes** that would most improve retrieval
quality for **FathomDB**, a local-first, on-device, CPU-only, no-API agent-memory store
whose goal is **retrieval/answer quality as-good-or-better than Mem0 and Zep**, under a hard
footprint constraint: small models, **1-bit binary vector quantization** (Hamming + f32
rerank), SQLite/FTS5 lexical arm, a property-graph substrate (opaque-id edges), and **no GPU
/ no API calls**.

Measured starting point (do not re-derive; treat as given): bge-small-en-v1.5 hybrid
(BM25/FTS5 + 1-bit dense, weighted RRF). Factoid retrieval is ~solved by lexical (R@10 ~0.90);
**exploratory/discourse retrieval is the bottleneck** (fused R@10 ~0.33, R@50 ~0.53, dense
median gold rank 99, oracle-union ceiling ~0.62, ~38% "hard" queries unfound by either arm).
A stronger dense embedder (nomic-v1.5) did NOT help in the chunked setup — so the dense-quality
lever is closed; the open levers are **reranking, graph retrieval, and whole-doc long-context**.

Two buckets, each filtered to the footprint (flag anything needing GPU/API or that breaks
1-bit quantization):

1. **Reranking / retrieval architecture.** Small CPU cross-encoder rerankers (bge-reranker-base
   / -v2-m3, ms-marco-MiniLM, jina-reranker, mxbai-rerank), late-interaction (ColBERT/PLAID —
   and its 1-bit incompatibility), listwise/LLM rerankers (cost). For each: realistic nDCG@10 /
   recall lift over a BM25+dense hybrid *first stage* (cite BEIR/benchmark numbers),
   CPU latency, model size, and whether it is candidate-recall-bound (can't beat first-stage
   recall). Also: query-side techniques that raise first-stage recall (HyDE, query expansion/
   decomposition, pseudo-relevance feedback) and their CPU/local feasibility.

2. **Graph retrieval for agent memory.** GraphRAG (Microsoft), Graphiti/Zep, HippoRAG &
   HippoRAG-2, LightRAG, and node-vs-edge-centric designs. For each: the candidate-generation /
   multi-hop mechanism, how it helps discourse/temporal/multi-hop queries that dense+lexical
   miss, reported gains on **LoCoMo / LongMemEval** (with the caveat these are end-to-end
   answer-accuracy, not first-stage recall), and CPU/local/no-API feasibility. Specifically:
   what does Zep's graph add over Mem0 that yields its LongMemEval edge, and is that mechanism
   reproducible in an on-device SQLite+graph store?

Deliver: a cited report with (a) a ranked shortlist of footprint-compatible levers with
expected effect sizes and confidence, (b) the node/edge/both graph trade-off for an agent-
memory store, (c) explicit flags on which options violate the local-first/CPU/1-bit footprint,
and (d) clear strong-vs-weak evidence labeling. This report is consumed by an analysis dossier
(Prompt A) and a Fable-5 roadmap (Prompt B); precision and citations matter more than breadth.
