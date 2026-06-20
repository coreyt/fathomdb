<!-- Date: 2026-06-19 · Model: codex (via `codex exec`) -->
<!-- Purpose: project-ground-truth Q&A feeding the Round-B engineering recommendation in
     dev/plans/runs/fathomdb-next-steps-codex-hypothesis.md. Record only; not read interactively. -->

# FathomDB next-steps — codex clarifying Q&A (appendix)

Codex (Round A) emitted 82 clarifying questions. They are answered below authoritatively from
the repo (STATUS-0.8.2, roadmap 0.8.3, the experiment compilation) and the orchestrator-supplied
project ground truth. Answers not determinable from the repo are labelled **(assumption)** and are
project-consistent. Grouped where codex's questions overlap.

## Architecture, stack, what is built (Q1–6, 43, 47, 51–55, 58, 62–66, 67–72)

- **Stack:** FathomDB = SQLite + FTS5 (BM25, fixed `b`) for lexical + a **1-bit binary (sign-quant,
  Hamming) ANN vector store** for dense. Two-phase vector retrieval: 1-bit bit-KNN (K=192) → f32
  rerank of that pool. Fusion = **RRF, k=60, byte-deterministic ordering** (tie-breaks stable).
  Core engine is **Rust**; Python + TypeScript are thin bindings over the same engine. Public query
  entrypoint = `engine.search()` (fused vector⊕FTS5).
- **Already built (production engine):** BM25/FTS5; 1-bit dense ANN + f32 rerank; RRF fusion;
  **CPU cross-encoder reranker** (TinyBERT-L-2) — real as of 0.8.2 E1, exposed as a standalone
  `fathomdb.rerank` SDK API (E2). The CE reranks an arbitrary passage list and reorders
  deterministically; NaN → `WriteValidationError`.
- **Built as eval harness only (NOT in the shipped library):** passage-dense bi-encoder arm wiring,
  dense+FTS RRF eval arm, graph BFS arm, PPR-fusion arm, index-key enrichment, bridge diagnostics,
  the LongMemEval recall harness, and the MuSiQue 5-arm identical-answerer QA harness. These live in
  `src/python/eval/`.
- **Graph extraction pipeline:** local **Qwen3.6-27B via Airlock vLLM batch ($0 local)** extracts
  entities + edges; canonical node/edge model is **logical_id-alone identity**. In the 0.8.1/0.8.2
  runs **edges were written body-less** (no edge-fact text) — a deliberate harness choice, not a hard
  constraint; adding edge bodies/fact text is allowed.
- **BM25 `b`:** tunable only in the `rank-bm25` reference harness today. FathomDB's FTS5 path uses
  **fixed `b`**; a tunable-/lower-`b` lexical path needs **custom-ranking engine work** (FTS5 does not
  expose `b`); deferred lever tracked for 0.8.5, potentially promoted to 0.8.3 if D2 reproduces.
- **Index-key enrichment** (0.8.1 R6) was implemented as **append-to-own-FTS-body**; fielded/sidecar
  indexing is **not** yet built (assumption: would be new engine work).
- **Observability:** per-arm candidate logs, RRF ranks, all-bridges@K, qrels, answer traces, and
  versioned JSON manifests exist in the harness; graph paths are logged when the graph arm runs.
  Artifacts under `dev/plans/runs/` are canonical and script-reproducible.
- **Deployment footprint:** embedded library first (Rust core), with CLI + Python + TS bindings. TS
  bindings need not expose new retrieval modes immediately (assumption). **Python is eval-only — the
  serving/query path is Rust core; do not put Python in the query path.**
- **API-change policy:** pre-1.0, **no backward-compat shims** — direct API changes are allowed when
  ADR-backed. Public surface changes go through `dev/interfaces/` + a governed-surface pin.

## Footprint / quantization / dependency invariants (Q23–35, 41–42, 44–46, 48–50, 73–75)

- **HARD footprint invariant:** the library is **CPU-only at query time, NO network/API at the query
  boundary, vectors stored as 1-bit binary (Hamming)**. A **0.90 ANN-quantization fidelity floor**
  (eu7) guards the 1-bit quantization and remains active for any new embedder/index. Any GPU,
  network, or non-1-bit path at the library boundary is a **violation unless explicitly HITL-waived**
  (precedent: 0.8.2 E1 CE reranker was a HITL-approved footprint-preserving engine change).
- **The ONLY LLM seam is BYO-LLM:** the caller's harness/LLM does graph construction / any LLM work.
  **No LLM ever lives inside FathomDB.** Therefore: LLM query rewrite, LLM decomposition, HyDE, LLM
  rerankers, IRCoT-style iterate↔reason are **caller-side only** — inside the library they must be
  **deterministic / non-LLM** or absent.
- **Query-time:** CPU-only, no GPU. Query embedder is currently **CPU-pinned** (bge-small). No
  network. (assumption) latency budget: interactive default search target sub-second on a 10k–100k
  corpus; a "deep-exploratory" mode may be an explicit slower/optional mode but still CPU + no-network.
- **Offline compute IS available:** 2× idle RTX 3090 for **offline** index build / embedding /
  extraction / reranker distillation; local **Qwen3.6-27B** extractor via Airlock ($0). GPU at
  **build time only** is fine. Offline budget: generous wall-clock + $0 local LLM; **priced frontier
  models (gpt-5.4, gemini-3.1-pro) are EVAL-ONLY, never in the product path.**
- **Dependencies:** Rust crates, SQLite/FTS5, Candle (CPU), tokenizers are in-stack. New
  CPU-inference models may be downloaded offline (HF/sentence-transformers/ONNX → vendored/Airlock
  cache). **Forbidden at the library boundary:** GPU runtimes, Python-in-query-path, external vector
  DBs, network calls, non-1-bit vector storage. License hygiene: Apache/MIT-class (assumption).
- **Fine-tuning / distillation:** allowed **offline** on training splits, but **must not train on the
  reporting eval split** (held-out discipline). Labels available: qrels, bridge paragraph IDs, answer
  F1, entity links, graph paths; synthetic data may be generated by the **local** Qwen ($0).
- **Fusion:** RRF k=60 is the shipped form; weighted/learned/per-class/adaptive fusion is **allowed
  if deterministic and CPU** (engine change, footprint-preserving).
- **Multi-query per user query** is allowed if deterministic + bounded (no LLM inside the library).
- **Graph methods** are reconsiderable only if a *fundamentally different* mechanism is proposed —
  BFS and PPR-fusion are **refuted twice** (recall n=40, answer-accuracy n=300) and 0.8.3 explicitly
  **drops graph traversal**. Treat graph as deprioritized unless scoped as a constrained expander with
  a new, evidence-backed rationale.

## Embedder / reranker specifics (Q36–40)

- **Authoritative embedder:** `bge-small-en-v1.5`, **dim 384, CLS pooling is correct**. **PRODUCT
  BUG:** the engine `CandleBgeEmbedder` **defaults to Mean pooling** (`candle_bge.rs:229`),
  degrading shipped dense retrieval; the eval harness uses CLS so eval is unaffected. Fixing
  CLS-vs-Mean is a tracked 0.8.3 D1 lever.
- **New embedders:** allowed if **CPU-fast, 1-bit-quantization-survivable (must hold the 0.90
  fidelity floor), license-clean, locally available, reproducible.** (assumption) single shared
  embedder is preferred over task-specific embedders unless a task-specific one clearly wins, because
  one index = one embedder identity (vector identity belongs to the embedder; do not co-mingle
  embeddings from different models in one index).
- **Reranker in `fused_rerank`:** TinyBERT-L-2 cross-encoder, **CPU**, rerank depth 200 over the
  fused pool. It was a stub during the first M1 pass (returned 0.0) then made real (E1); in the valid
  measurement it **neither helped nor hurt** multi-hop (tied with fused-RRF on F1; recall@10 slightly
  lower). Cross-encoder rerankers ARE allowed in production (CPU, footprint-preserving). A different
  reranker/objective (e.g. evidence-set-level, recall-shaped) is acceptable.
- **Query embeddings:** computed synchronously at query time on CPU (assumption: not precomputed for
  benchmark questions — that would be leakage).

## Benchmarks, splits, comparators, acceptance (Q7–22, 56–61, 76–82)

- **Agentic-memory target = LongMemEval** (needle-recall over chat memory; classes factoid /
  knowledge_update / multi_session / temporal). Real consumer surface = **Memex, Hermes, OpenClaw**
  (local-first agent memory on SQLite + sqlite-vec/FTS5). Document unit for memory = **session**
  (gold key = `answer_session_id`); multiple indexed views (message/session/entity-fact) are allowed.
- **Exploratory + deep-exploratory recall = internal IR corpus**, frozen `corpus_hash fe973fcd`,
  10 sources, 10,506 docs. Two axes: **eu7 = ANN-quantization FIDELITY recall@10 (system health,
  GATED ≥0.90)**; **eu8 = IR/agentic RELEVANCE recall@10 (report-only, embedder-bound ceiling
  ≈0.571)**. **"Deep-exploratory" = the hard discrimination subset (~596 hard queries; dense median
  rank ≈99, dense top-50 ≈37%).** Distinction from exploratory = harder discrimination / deeper-K /
  multi-round, not a different corpus.
- **Multi-hop QA = MuSiQue** (answerable + distractor; hops 2/3/4). Canonical pinned sample: N=300
  answerable, seed 20260617, `musique_hash 3cff37fd…`. **Pooled ≥3-hop ΔF1 is the primary endpoint.**
  HotpotQA/2Wiki/MultiHop-RAG are **dropped** for graph multi-hop (refuted); re-add only for a new
  mechanism.
- **Comparator:** **HITL-CONFIRMED comparator = fused-RRF (k=60)**, MATERIAL_F1_LIFT = **0.04**. This
  was data-justified by the $0 bridge diagnostic: complete-bridge retrieval drives F1 (+0.442); given
  all bridges, **fused-RRF answers BEST (0.552)** while dense retrieves-best-but-answers-worst (0.464,
  distractor-composition). So although `passage_dense` had the highest standalone ≥3-hop F1 (0.487 vs
  fused 0.450) in the gpt-5.4 N=300 run, **fused-RRF stays the registered comparator**; passage_dense
  is promoted to a **registered** D1 question in 0.8.3 (was an observation, not a registered compare).
- **Acceptance form:** paired improvement over the registered comparator, **pre-registered `decide()`
  rule**: GO iff pooled ≥3-hop ΔF1 ≥ materiality (0.04) AND bootstrap CI lower bound > 0; plus a
  negative-slope trend veto, CI-banded ΔEM, an unanswerable-set confident-wrong guard, and a
  **whole-rule power gate (`power_ok` only if P(GO) ≥ 0.8)**. n_boot=2000, fixed seed, question-level
  paired bootstrap. Note: the M1 rule was **underpowered even at N=1165 (P(GO)=0.45 @0.04)** — a real
  constraint; any new registered QA compare must size N to the feasible MDE or relax materiality at a
  gate. The 0.90 eu7 fidelity floor is the other hard bar.
- **Reader/judge for QA eval:** priced run = **gpt-5.4 (temp 0, seed 0)** proved cheap ($2.50) +
  resilient; gemini-3.1-pro hit a $25 cap. Use gpt-5.4 as the priced reader; **flash-lite
  cheap-validate before any priced run**; local qwen3.6-27b as a $0 cross-check. Budget discipline:
  HITL gate before priced spend; M1 full pass ran ~$2.50, prior ceiling <$30.
- **Top-K:** QA reads top-10, but retrieving deeper (depth 50–200) then selecting a final evidence
  set before the answerer is **allowed and encouraged** — the bottleneck is **complete-bridge
  presence in what the reader sees**.
- **Priority + scope:** (assumption) priority order when goals conflict ≈ multi-hop QA ≈ agentic
  memory > exploratory > deep-exploratory, but the program prefers a single lever that lifts several.
  The two steps **need not improve all four simultaneously**; step 1 may primarily lift recall/evidence
  presence and step 2 primarily lift answer assembly. Steps should be **sequential measurement→product
  milestones** reusing the M1 harness wholesale, each with cheap-validate → bounded priced pilot →
  power-sim → HITL gate. **Honesty prior (load-bearing): across 0.8.1+0.8.2, NO cheap retrieval lever
  has yet beaten strong lexical/dense — every prior bet (graph BFS, PPR-fusion, enrichment-as-is, CE
  rerank) returned a null or negative. Pre-register so a null is a clean negative, not a moved goalpost.**

## Direct steer from the repo (roadmap 0.8.3, post-M1 redirect)

The HITL-signed redirect names the **two non-graph levers** as the live next directions:
**D1 — passage-dense promoted to a registered comparison** (incl. the CLS-vs-Mean pooling fix and
**vector pseudo-relevance feedback (PRF), a cheap no-LLM query-side lever**), and **D2 — index-key
enrichment revived + scaled** with the length-matched placebo and the FTS length-norm fix. Graph
multi-hop (2Wiki/MultiHop-RAG/PPR-more-datasets) is **dropped**; IRCoT is deferred (non-graph but
raises per-query LLM cost — caller-side). Codex's recommendation should treat D1/D2 as the
evidence-backed frame and sequence concrete methods within the footprint invariant.
