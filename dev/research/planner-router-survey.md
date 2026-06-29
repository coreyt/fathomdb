# Planner-router / adaptive-retrieval research landscape — survey

Research date: 2026-06-28. Author: research agent (web-sourced via WebSearch/WebFetch; every claim
is cited to a primary source or marked `[UNVERIFIED]`). Companion to the prior shortlist
`dev/research/planner-router-competitors.md` (read that first; this expands it field-wide).

**Scope frame.** FathomDB's "planner-router" = adaptive/routed retrieval: classify query intent
(~5 classes — needle/single-fact, multi-session, temporal, global-sensemaking, multi-hop) → select a
per-intent retrieval **CONFIG** over typed operators (BM25 / vector / RRF / cross-encoder rerank /
map-reduce) and their knobs (candidate_k, rerank pool, blend α, MMR, recency). Planner turns
intent→plan over typed operators; router binds each node to an operator-with-config. Local-first
(SQLite + sqlite-vec/FTS5), CPU-default, optional caller-side LLM. **Distinctive axis:** FathomDB
routes over *retrieval config/knobs*, whereas the field overwhelmingly routes over *retrieve-or-not*,
*which retriever/LLM*, or *RAG-vs-long-context* (see §c).

Coverage: **34 approaches/benchmarks** surveyed across six families. Rows marked "[seed]" were already
in the prior doc and are kept for completeness (facts re-confirmed); the rest are new in this pass.

---

## (a) Per-approach matrix

Oracle column = does the paper report an oracle / optimal-router / best-of-trajectory **upper bound**
(the device analogous to FathomDB's Gate-2 ceiling)?

### Family 1 — Retrieve-or-not / when-to-retrieve (confidence/knowledge/popularity-gated)

| Approach (cite) | Routes ON → BETWEEN; mechanism | Tested AGAINST | Benchmarks | Headline result | Oracle? |
|---|---|---|---|---|---|
| **Self-RAG** [seed] — Asai et al., ICLR 2024, arXiv:2310.11511; selfrag.github.io | LM-generated "reflection tokens" at decode → on-demand retrieve vs not + self-critique | no-retrieval, always-retrieve, Self-CoT, RAG baselines, ChatGPT | PopQA, PubHealth, ARC, bio long-form, ASQA | Outperforms ChatGPT & retrieval-augmented Llama2 on most tasks | No |
| **FLARE** [seed] — Jiang et al., EMNLP 2023, arXiv:2305.06983; github.com/jzbjyb/FLARE | low-confidence tokens in predicted next sentence → retrieve-now vs keep-generating (active, iterative) | no/single/multi-time retrieval, prior active-retrieval | 2WikiMultiHop, ASQA, StrategyQA, WikiAsp | Beats single-time & prior multi-time retrieval on long-form knowledge gen | No |
| **DRAGIN** [seed] — Su et al., ACL 2024, arXiv:2403.10081 | real-time info-need (RIND self-attention) + query-from-attention (QFS); training-free → when/what to retrieve | FLARE, IRCoT, fixed-interval retrieval, no-retrieval | 2WikiMultiHop, HotpotQA, IIRC, StrategyQA | Beats FLARE/IRCoT on the four sets | No |
| **Adaptive Retrieval (popularity-gated)** — Mallen et al., ACL 2023, arXiv:2212.10511; github.com/AlexTMallen/adaptive-retrieval | entity **popularity** (Wikipedia page views), per-relation threshold → parametric-only vs retrieve | vanilla LM; always-retrieve (Contriever/BM25/GenRead) | PopQA (14k, theirs), EntityQuestions | Adaptive (davinci-003, GenRead+Contriever) 46.5% acc, +5.3 over best non-adaptive | Threshold tuned on dev split; no oracle ceiling |
| **SKR** — Wang et al., EMNLP Findings 2023, arXiv:2310.05002; github.com/THUNLP-MT/SKR | model **self-knowledge** (known/unknown) via kNN over training Qs → CoT vs retrieve | Zero/Few-Shot, Manual/Auto-CoT, Manual-CoT-IR, IRCoT, CoT-RR; BERT classifier | TemporalQA, CommonsenseQA, TabularQA, StrategyQA, TruthfulQA | SKR_knn +~4% avg acc over Manual-CoT (InstructGPT & ChatGPT) | No |
| **Rowen** — Ding et al., arXiv:2402.10612 (Feb 2024); SIGIR-AP 2025 `[UNVERIFIED venue]` | answer **consistency under perturbation** (across languages+models) → parametric vs retrieve | CoVe, Multi-agent Debate, Self-Reflection, Factool, FLARE, Self-RAG, Adaptive-RAG, LUQ | TruthfulQA, StrategyQA, TriviaQA, NaturalQuestions | Rowen-Hybrid 59.34% GPT-Judge on TruthfulQA (+16.74 over best baseline) | No |
| **UAR** — Cheng et al., EMNLP Findings 2024, arXiv:2406.12534; github.com/xiami2019/UAR | 4 criteria (Intent/Knowledge/Time-sensitive/Self-aware) → retrieve vs not; MLP probes on frozen hidden states + decision tree | FLARE, Self-RAG, SKR; never-/always-retrieve | AR-Bench (theirs); DROP, GSM8K, TriviaQA, WebQuestions, TAQA, FreshQA | AR-Bench (Llama2-7B) 85.32% acc vs FLARE 56.50 / Self-RAG 60.12 / SKR 62.14 | No |
| **SeaKR** — Yao et al., arXiv:2406.19215 (Jun 2024); github.com/THU-KEG/SeaKR `[UNVERIFIED code]` | internal-state uncertainty (Gram-determinant of hidden states) → iterative retrieve vs not + self-aware rerank | CoT, IRCoT; Self-RAG, FLARE, DRAGIN | 2WikiMultiHop, HotpotQA, IIRC; NQ, TriviaQA, SQuAD | 2Wiki 36.0 F1 (vs DRAGIN 30.0); HotpotQA 39.7 (vs 34.2) | No |
| **Self-DC** — Wang et al., NAACL Findings 2025, arXiv:2402.13514 | LLM **confidence** (verbalized/token-prob) → 3 actions: generate-then-read / retrieve-then-read / recursive decompose | Direct, CoT, GenRead, Retrieve-then-Read, Self-Ask, IRCoT, REFEED, ITER-RETGEN | CuQA (theirs), FreshQA | Matches/beats methods using 2–3× more retrieval calls (CuQA 36.4 EM) | No |
| **SlimPLM** — Tan et al., ACL 2024, arXiv:2402.12052; github.com/plageon/SlimPlm | quality of a **small proxy model's** draft answer → retrieve vs not (+ what); per-claim necessity judge | vanilla Chat, CoT; Direct RAG, FLARE, Self-Eval, Self-Ask, ITER-RETGEN, SKR-KNN | NQ, TriviaQA, ASQA, MuSiQue, ELI5 | Matches/exceeds SOTA at lower LLM cost (ASQA 30.73 EM/65.00 Hit@1) | No |

### Family 2 — Query-complexity / pipeline-depth routing (closest to FathomDB intent→strategy)

| Approach (cite) | Routes ON → BETWEEN; mechanism | Tested AGAINST | Benchmarks | Headline result | Oracle? |
|---|---|---|---|---|---|
| **Adaptive-RAG** [seed] — Jeong et al., NAACL 2024, arXiv:2403.14403; official repo public | learned **query-complexity** classifier (small LM, 3 classes A/B/C, auto-labeled from outcomes) → no-retrieval / single-step / multi-step iterative | no-retrieval, single-step, multi-step (adaptive vs each fixed strategy); Self-RAG, adaptive baselines | SQuAD, NQ, TriviaQA; MuSiQue, HotpotQA, 2WikiMultiHop | Best quality/efficiency trade-off; **reports Adaptive-RAG w/ Oracle classifier** | **Yes** (oracle classifier) |

### Family 3 — Agentic / iterative / multi-hop retrieval (decompose, interleave, stop-or-continue)

| Approach (cite) | Routes/decides ON → BETWEEN; mechanism | Tested AGAINST | Benchmarks | Headline result | Oracle? |
|---|---|---|---|---|---|
| **ReAct (retrieval)** [seed] — Yao et al., ICLR 2023, arXiv:2210.03629 | LLM interleaves reason+act (search) tool calls → continue vs answer | CoT, act-only, standard prompting | HotpotQA, FEVER (+ALFWorld/WebShop) | Reduces hallucination vs CoT; competitive HotpotQA/FEVER | No |
| **Self-Ask** — Press et al., EMNLP Findings 2023, arXiv:2210.03350; github.com/ofirpress/self-ask | LLM decides if a follow-up sub-question is needed → decompose + optional per-subq search | Direct prompting, Chain-of-Thought | Compositional Celebrities, 2WikiMultiHop, MuSiQue, Bamboogle | +11% abs over CoT on Bamboogle | No |
| **Iter-RetGen** — Shao et al., EMNLP Findings 2023, arXiv:2305.15294 | feeds whole prior output back as next query over fixed N iters → continue vs stop | Direct, CoT, ReAct, Self-Ask, DSP | HotpotQA, 2WikiMultiHop, MuSiQue, Bamboogle, Feverous, StrategyQA | HotpotQA Acc 71.2 vs Self-Ask 64.8 (+6.4); up to +8.6 abs | No (reports answer-recall diagnostic) |
| **IRCoT** [seed] — Trivedi et al., ACL 2023, arXiv:2212.10509; github.com/StonyBrookNLP/ircot | interleave retrieval with CoT; each step's retrieval guided by prior CoT | one-step retrieval, no-CoT retrieval, QA baselines | HotpotQA, 2WikiMultiHop, MuSiQue, IIRC | +up to 21 retrieval / +15 QA points | No |
| **ProbTree** — Cao & Zhang et al., EMNLP Findings 2023, arXiv:2311.13982; github.com/THU-KEG/ProbTree | query tree; per leaf **closed-book vs open-book by confidence**; probabilistic leaf→root aggregation | SOTA open-domain CQA `[UNVERIFIED names]` | HotpotQA, 2WikiMultiHop, MuSiQue `[UNVERIFIED set]` | "Outperforms SOTA" `[UNVERIFIED numbers]` | No `[UNVERIFIED]` |
| **BeamAggR** — Chu et al., ACL 2024, arXiv:2406.19820 | question tree + **beam search over reasoning paths**, probabilistic multi-source aggregation | SOTA multi-hop `[UNVERIFIED names]` | HotpotQA, 2WikiMultiHop, MuSiQue `[UNVERIFIED set]` | +8.5% over SOTA (avg) | No `[UNVERIFIED]` |
| **RQ-RAG** — Chan et al., COLM 2024 `[UNVERIFIED venue]`, arXiv:2404.00610; github.com/chanchimin/RQ-RAG | trained Llama2-7B emits special tokens → route among rewrite / decompose / disambiguate / answer-directly | Self-RAG-7B, SAIL-7B, Llama2-7B, CoT, Chain-of-Note | ARC-C, PopQA, OBQA; HotpotQA, 2WikiMultiHop, MuSiQue | +1.9% avg over Self-RAG (single-hop); +22.6% (multi-hop) | **Yes** (best-of-trajectory, e.g. 80.5% HotpotQA) |
| **ReSP (Retrieve-Summarize-Plan)** — Jiang et al., arXiv:2407.13101 (2024) `[UNVERIFIED venue]` | reasoner decides exit vs next sub-question from memory queues; dual summarizer curbs context overload | Standard RAG, SuRe, RECOMP, REPLUG; Iter-RetGen, IRCoT | HotpotQA, 2WikiMultiHop | HotpotQA F1 47.2 vs IRCoT 43.1 (+4.1); 2Wiki 38.3 vs 32.4 | No |
| **Search-o1** — Li et al., EMNLP 2025, arXiv:2501.05366; github.com/RUC-NLPIR/Search-o1 | LRM (QwQ-32B) triggers retrieval mid-reasoning on uncertainty; Reason-in-Documents distills docs | Direct reasoning (QwQ/Qwen2.5/Llama3.3-70B/GPT-4o/o1-preview); Standard RAG; RAG-Agent | GPQA, math/coding; HotpotQA, 2WikiMultiHop, MuSiQue, Bamboogle | Multi-hop QA +29.6% avg EM over RAG-QwQ; +5.3% over RAgent | No |
| **HippoRAG** [seed] — Gutiérrez et al., NeurIPS 2024, arXiv:2405.14831; github.com/OSU-NLP-Group/HippoRAG | KG + Personalized PageRank (single-step associative retrieval) | IRCoT, ColBERTv2, Contriever, dense baselines | MuSiQue, 2WikiMultiHop, HotpotQA | Up to +20% over baselines on multi-hop; cheaper/faster than IRCoT | No |
| **HippoRAG 2** — Gutiérrez et al., **ICML 2025**, arXiv:2502.14802; same repo | dense+sparse KG (passage + phrase nodes), unified recall (continual memory) | HippoRAG, dense retrievers, GraphRAG-style | MuSiQue, 2WikiMultiHop, HotpotQA, LongMemEval-style | MuSiQue F1 **48.6** (vs HippoRAG 35.1, dense/NV-Embed-v2 45.7; Llama-3.3-70B reader, Table 2 v1); 2Wiki R 76.5→90.4 `[UNVERIFIED]` — **corrected** (earlier "44.8→51.9" was wrong/version-dependent; direct spot-check) | No |

> Correction to prior doc: **HippoRAG 2 venue = ICML 2025** (arXiv:2502.14802), not NeurIPS. Original
> HippoRAG = NeurIPS 2024.
> **Number correction (secondary-review spot-check, arXiv:2502.14802v1 Table 2):** HippoRAG 2 MuSiQue
> **F1 48.6** vs HippoRAG 35.1 vs dense/NV-Embed-v2 45.7 (Llama-3.3-70B reader). The earlier
> "44.8→51.9" cell was wrong/version-dependent — direction (HippoRAG 2 > dense on MuSiQue) is firm,
> exact deltas are table/version-specific.

### Family 4 — Which-retriever / retrieval-strategy routing (no LLM-gen needed; pure-IR)

| Approach (cite) | Routes ON → BETWEEN; mechanism | Tested AGAINST | Benchmarks | Headline result | Oracle? |
|---|---|---|---|---|---|
| **RouterRetriever** [seed] — Lee et al., 2024, arXiv:2409.02685 | per-query routing to a domain-expert embedding model (pilot-embedding gate) → mixture of expert embedders | single general (MSMARCO) model; multi-task model | BEIR | +2.1 nDCG@10 vs single, +3.2 vs multi-task | `[UNVERIFIED]` |
| **MoR (Mixture of Retrievers)** — arXiv:2506.15862 (Jun 2025) | query–retriever–doc signals → zero-shot **weighted blend** of sparse/dense retrievers (~0.8B) | BM25, SimCSE, Contriever, DPR, ANCE, TAS-B, GTR, MPNet, RepLLaMA-7B, GritLM-7B | NFCorpus, SciDocs, SciFact, SciQ | +3.9% nDCG@20 over GritLM-7B; +10.8% over best unsup. component | **Yes** ("Route Oracle", +13.5% over GritLM) |
| **Blended RAG** — Sawarkar et al. (IBM), MIPR 2024, arXiv:2404.07220; github.com/ibm-ecosystem-engineering/blended-rag | selects/fuses best hybrid query strategy over dense+sparse indexes (not learned per-query) | individual dense/sparse index configs; fine-tuned QA | NQ, TREC-COVID (IR); SQuAD (QA) | New IR SOTA on NQ/TREC-COVID; beats fine-tuning on SQuAD QA | No |
| **LexBoost** — Kulkarni et al., ACM DocEng 2024, arXiv:2409.05882; github.com/Georgetown-IR-Lab/LexBoost | doc lexical score **fused** with dense corpus-graph neighbor scores at rank time (offline graph) | BM25 / lexical-only | TREC DL-style collections `[UNVERIFIED exact sets]` | Improves lexical ranking with minimal online overhead | No |

### Family 5 — LLM routing for RAG (which reader/generator; cost-vs-quality)

| Approach (cite) | Routes ON → BETWEEN; mechanism | Tested AGAINST | Benchmarks | Headline result | Oracle? |
|---|---|---|---|---|---|
| **Self-Route (RAG vs LC)** [seed] — Li et al., EMNLP 2024 Industry, arXiv:2407.16833 (Google DeepMind/UMich) | LLM self-reflection ("answerable from chunks?") → RAG vs full long-context | RAG-only, long-context-only | ∞Bench and long-context QA suites | ~65% cost cut at ~long-context quality | No |
| **RouteLLM** — Ong et al. (LMSYS), 2024, arXiv:2406.18665; github.com/lm-sys/RouteLLM | query → strong vs weak LLM; 4 routers (sim-weighted / matrix-factorization / BERT / causal-LLM) on Arena prefs | random router; commercial routers (Martian, Unify) | MT-Bench, MMLU, GSM8K | >85% / 45% / 35% cost cut at 95% GPT-4 quality | **Yes** (random + optimal via APGR/PGR) |
| **Hybrid LLM** — Ding et al., ICLR 2024, arXiv:2404.14618 | predicted query difficulty + quality target → small(edge) vs large(cloud); DeBERTa router | small-only, large-only, random | MixInstruct, others `[UNVERIFIED set]` | 22% fewer large-model calls at 1% quality drop | `[UNVERIFIED]` |
| **RouterDC** — Chen et al., NeurIPS 2024, arXiv:2409.19886; github.com/shuhao02/RouterDC | query+LLM embeddings → best of N LLMs; dual contrastive training | best individual LLM; ZOOTER, CosineClassifier | mixed task suite (MMLU, GSM8K, …) `[UNVERIFIED full list]` | +2.76% in-dist, +1.90% out-of-dist over best baseline | `[UNVERIFIED]` |
| **FrugalGPT** — Chen, Zaharia, Zou, 2023, arXiv:2305.05176 | sequential LLM **cascade** with learned scoring/stopping | individual APIs (GPT-4/3.5/3, J1-Jumbo); best single | HEADLINES, OVERRULING, COQA, AGNEWS, SCIQ | Matches GPT-4 at up to 98% lower cost | No (cost-accuracy frontier) |
| **ZOOTER** — Lu et al., NAACL 2024, arXiv:2311.08692 | query → expert LLM via **reward-distillation** routing + tag enhancement | best single model; reward-model ranking ensembles | 26-subset multi-domain collection | Beats best single; #1 on 44% of tasks at far lower compute | `[UNVERIFIED]` |
| **AutoMix** — Madaan, Aggarwal et al., NeurIPS 2024, arXiv:2310.12963; github.com/automix-llm/automix | small-LM few-shot self-verification → POMDP escalation to larger LM (cascade) | FrugalGPT-style cascade/routing | 5 context-grounded reasoning sets (CNLI, QASPER, …) | >50% cost reduction at comparable performance | `[UNVERIFIED]` |

### Family 6 — Routing benchmarks & framework selectors (not standalone systems)

| Item (cite) | What it is | Routers/baselines included | Data | Oracle? |
|---|---|---|---|---|
| **RAGRouter-Bench** — Wang et al., arXiv:2602.00296 (v1 30 Jan 2026, v2 4 Apr 2026); github.com/ziqiwang0908/RAGRouter-Bench (MIT) | First purpose-built **adaptive-RAG routing** benchmark: 3 query types (factual/reasoning/summarization) over 5 paradigms (LLM-only, NaiveRAG, GraphRAG, HybridRAG, IterativeRAG) | fixed-paradigm selection vs adaptive routing (LLM-as-judge: quality+cost) | 7,727 queries / 21,460 docs across MuSiQue, QuALITY, UltraDomain, GraphRAG-Bench | benchmark provides reference points |
| **Baseline study on RAGRouter-Bench** — Bansal & Agarwal, arXiv:2604.03455 (3 Apr 2026, CC BY 4.0) | lightweight query routers: 5 classifiers × 3 feature types | TF-IDF+SVM (best), others | RAGRouter-Bench | best: macro-F1 0.928 / 93.2% acc, ~28% token savings |
| **RouterBench** — Hu et al. (Martian), 2024, arXiv:2403.12031; github.com/withmartian/routerbench | multi-**LLM**-routing benchmark + theory (NOT RAG-paradigm routing) | predictive & cascading routers | 11 LLMs × 8 task datasets; 405k+ outcomes | **Yes** (zero/random + oracle reference routers) |
| **RouterQueryEngine / LangChain selectors** [seed] — LlamaIndex & LangChain (OSS frameworks) | LLM selector (text or Pydantic/function-call), single/multi-route | named query engines / retrievers / data sources | none (framework, not benchmarked) | n/a |

> Verification note: the prior doc flagged RAGRouter-Bench (2602.00296) and its baseline study
> (2604.03455) as unverified/possibly hallucinated. **Both are confirmed real** with populated public
> repos. Correct the prior doc accordingly.

---

## (b) Who-tested-against-whom — the competitive web

Tallying the explicit baseline sets above, the **de-facto standard comparison set** (systems repeatedly
used as baselines) is:

1. **Self-RAG** — baseline in UAR, Rowen, RQ-RAG, and the seed adaptive-RAG cluster (the single most
   cited adaptive-RAG comparator).
2. **FLARE** — baseline in DRAGIN, UAR, SeaKR, Rowen.
3. **IRCoT** — baseline in SKR, SeaKR, ReSP, HippoRAG/2 (and the canonical multi-hop iterative anchor).
4. **Self-Ask** — baseline in Iter-RetGen, Self-DC, SlimPLM.
5. **Adaptive-RAG** — baseline in Rowen and the reference point RAGRouter-Bench generalizes.

Secondary recurring comparators: **no-retrieval** and **always-retrieve** (universal floors/ceilings in
Family 1), **best single retriever/LLM** + **random router** (universal in Families 4–5), **CoT** and
**ITER-RETGEN** (multi-hop). Convergence on corpora is even tighter: **HotpotQA + 2WikiMultiHop +
MuSiQue** is the multi-hop triple shared by IRCoT, HippoRAG/2, Self-Ask, Iter-RetGen, DRAGIN-adjacent,
SeaKR, RQ-RAG, Search-o1, Adaptive-RAG. **BEIR** is the standard for retriever-routing (RouterRetriever);
**MMLU/GSM8K/MT-Bench** for LLM-routing.

Implication: a credible FathomDB head-to-head should cite numbers against **Adaptive-RAG / Self-RAG /
IRCoT** on **MuSiQue** to land in the field's accepted comparison frame.

---

## (c) Routing-axis taxonomy — and where FathomDB sits

**Route-ON axes (the signal the decision is made from):**

- **Query complexity / hop-count** — Adaptive-RAG (trained classifier), RAGRouter-Bench query types.
- **Model confidence / uncertainty** — FLARE (token prob), SeaKR (hidden-state Gram-det), Self-DC
  (verbalized/token-prob), Rowen (consistency under perturbation), AutoMix (self-verification).
- **Self-knowledge / known-vs-unknown** — SKR (kNN), SlimPLM (proxy-answer quality), UAR (probes).
- **Entity popularity / corpus stats** — Mallen popularity-gating.
- **Token-level info need** — DRAGIN (attention), FLARE (per-sentence).
- **Domain / topic** — RouterRetriever (embedding domain), ZOOTER/RouterDC (LLM expertise).
- **Query difficulty for cost** — Hybrid LLM, RouteLLM, FrugalGPT.

**Route-BETWEEN axes (what is being selected):**

- **Retrieve vs not** — Family 1 (Self-RAG, FLARE, SKR, Mallen, Rowen, UAR, SeaKR, Self-DC, SlimPLM).
- **Single vs iterative/multi-hop depth** — Adaptive-RAG, IRCoT, Iter-RetGen, ReSP, Search-o1, Self-Ask.
- **Which retriever/embedder/strategy** — RouterRetriever, MoR, Blended RAG, LexBoost.
- **Which LLM (reader/generator)** — RouteLLM, RouterDC, ZOOTER, Hybrid LLM, FrugalGPT, AutoMix.
- **RAG vs long-context** — Self-Route.
- **Which RAG paradigm** — RAGRouter-Bench (Naive/Graph/Hybrid/Iterative).

**Where FathomDB sits — the honest read.** FathomDB routes **intent → per-intent retrieval CONFIG**:
which arms (BM25/vector/RRF/CE-rerank/map-reduce) *and their knobs* (candidate_k, rerank pool, blend α,
MMR, recency). No surveyed system routes over **per-intent operator-config/knobs**. The nearest neighbors:

- **Adaptive-RAG** is closest in *spirit* (intent/complexity → strategy) but selects **pipeline depth**
  (no/single/multi retrieval), not arm-knobs.
- **MoR / Blended RAG** blend retrieval **arms**, but with a fixed global policy, not per-intent config.
- **RAGRouter-Bench** routes among **paradigms**, the coarsest version of strategy routing — adjacent but
  still not knob-level.

So the differentiator is **real but narrow**: config/knob-granularity routing per intent class is
genuinely unoccupied territory. Caveat (carry from the internal finding): FathomDB's own result was that
*static arm-selection ≈ strong RRF-fused hybrid* — the value was in **per-intent knob tuning**, which is
exactly the part the literature does not study. Frame it as "config-granularity routing", not as
"a new router class"; do not inflate it into a paradigm claim.

---

## (d) Standard benchmarks & metrics for routing eval

**Benchmarks** (canonical cites verified):

- Multi-hop QA: **HotpotQA** (Yang et al., EMNLP 2018, arXiv:1809.09600); **MuSiQue** (Trivedi et al.,
  TACL 2022, arXiv:2108.00573); **2WikiMultiHopQA** (Ho et al., COLING 2020, arXiv:2011.01060);
  **StrategyQA** (Geva et al., TACL 2021, arXiv:2101.02235); Bamboogle (in Self-Ask).
- Single-hop / open-domain QA: **Natural Questions** (Kwiatkowski et al., TACL 2019); **TriviaQA**
  (Joshi et al., ACL 2017, arXiv:1705.03551); **SQuAD** (Rajpurkar et al., EMNLP 2016, arXiv:1606.05250 /
  2.0 arXiv:1806.03822); **PopQA** (Mallen et al., ACL 2023, arXiv:2212.10511).
- Retrieval: **BEIR** (Thakur et al., NeurIPS Datasets 2021, arXiv:2104.08663).
- Long-form / citations: **ASQA** (Stelmakh et al., EMNLP 2022, arXiv:2204.06092); **ALCE** (Gao et al.,
  EMNLP 2023, arXiv:2305.14627).
- LLM-routing: MMLU, GSM8K, MT-Bench (in RouteLLM/RouterBench/RouterDC).
- Routing-specific: **RAGRouter-Bench** (arXiv:2602.00296); **RouterBench** (arXiv:2403.12031).

**Metrics:**

- End-task quality: **EM / F1 / Accuracy** (QA); **nDCG@10 / Recall@k** (retrieval).
- Routing quality: router **classification accuracy / macro-F1** (Adaptive-RAG; RAGRouter-Bench baseline
  TF-IDF+SVM 0.928 macro-F1).
- Efficiency / cost-vs-quality **Pareto**: avg retrieval steps, latency, token/$ savings (Adaptive-RAG
  steps & rel-time; Self-Route ~65% cut; FrugalGPT up to 98%; RouteLLM APGR/PGR; RAGRouter-Bench joint).
- **Oracle / upper-bound ceiling** — reported by **Adaptive-RAG** (oracle classifier), **MoR** (Route
  Oracle), **RouteLLM** (optimal router), **RouterBench** (oracle reference), **RQ-RAG**
  (best-of-trajectory). This is exactly FathomDB's **Gate-2 oracle-router ceiling** device, and these
  five give precedent to report learned-vs-oracle gap in the field's vocabulary.

---

## (e) Gaps — FathomDB's distinctive intent classes vs the literature

- **needle / single-fact** — fully covered (PopQA, NQ, TriviaQA, SQuAD; every Family-1 system).
- **multi_hop** — fully covered and saturated (HotpotQA/MuSiQue/2Wiki triple; Families 2–3).
- **multi_session** — **no routing-literature analogue.** Lives in the long-term-memory benchmark line:
  **LongMemEval** (Wu et al., ICLR 2025, arXiv:2410.10813) and **LOCOMO** (Maharana et al., ACL 2024,
  arXiv:2402.17753). No router paper routes on or evaluates multi-session intent.
- **temporal** — **no dedicated routing analogue.** Touched only as a *signal* (UAR's "Time-sensitive"
  criterion; FreshQA/TAQA as datasets), never as a routed retrieval class. Temporal QA benchmarks exist
  (TempLAMA/TAQA) but are not used in routing eval.
- **global-sensemaking** — **no routing analogue at all.** Lives in the GraphRAG/QED sensemaking line
  (AP-News / BenchmarkQED). RAGRouter-Bench's "summarization" query type + GraphRAG paradigm is the
  *only* faint adjacency, but it routes paradigm, not a sensemaking intent class.

Net: **3 of FathomDB's 5 intent classes (multi_session, temporal, global-sensemaking) have no analogue
in the routing literature.** A router head-to-head can only be apples-to-apples on **needle** and
**multi_hop**; the distinctive three require comparison against memory/sensemaking systems
(Mem0, GraphRAG), not routers.

---

## (f) Implications for FathomDB's V-6 head-to-head

The field gives a clear, defensible frame. (1) Run the apples-to-apples comparison where the literature
lives: **Adaptive-RAG on MuSiQue, EM/F1 + avg retrieval steps, with the oracle-router ceiling reported
alongside** — Adaptive-RAG is the closest conceptual competitor, reports its own oracle, and MuSiQue is
on-disk and is the multi-hop standard shared by IRCoT/HippoRAG. Reproduce its baseline locally (its
numbers assume a large hosted generator; quote reproduced numbers, not paper headlines, given FathomDB's
CPU-default/optional-LLM posture). (2) Add the cheap pure-IR test: **RouterRetriever/MoR-style arm
routing on BEIR (FiQA/Touché-2020/NFCorpus/ArguAna — already in the `beir-acquire` scripts), nDCG@10 /
Recall@k** — this directly pressure-tests FathomDB's internal "static arm-selection ≈ RRF" finding
against the published "+2.1 nDCG@10 / Route-Oracle +13.5%" claims, CPU-only, no LLM. (3) Report results
in the field's vocabulary — **learned-router vs oracle-router gap** (precedented by Adaptive-RAG, MoR,
RouteLLM, RouterBench, RQ-RAG) — and **honestly scope the differentiator**: FathomDB routes
config/knobs per intent (unoccupied territory), but the *measured win is knob-tuning, not arm-selection*,
so claim "per-intent config-granularity routing", not a new router paradigm. (4) Note that
multi_session/temporal/global-sensemaking fall outside the router comparison frame entirely — those go
head-to-head against memory/sensemaking systems on LongMemEval/LOCOMO/AP-News, and should be reported as
a separate track, not folded into the router numbers.

---

## Sources (primary)

Family 1: Self-RAG arXiv:2310.11511 · FLARE arXiv:2305.06983 · DRAGIN arXiv:2403.10081 · Mallen
arXiv:2212.10511 · SKR arXiv:2310.05002 · Rowen arXiv:2402.10612 · UAR arXiv:2406.12534 · SeaKR
arXiv:2406.19215 · Self-DC arXiv:2402.13514 · SlimPLM arXiv:2402.12052.
Family 2: Adaptive-RAG arXiv:2403.14403.
Family 3: ReAct arXiv:2210.03629 · Self-Ask arXiv:2210.03350 · Iter-RetGen arXiv:2305.15294 · IRCoT
arXiv:2212.10509 · ProbTree arXiv:2311.13982 · BeamAggR arXiv:2406.19820 · RQ-RAG arXiv:2404.00610 ·
ReSP arXiv:2407.13101 · Search-o1 arXiv:2501.05366 · HippoRAG arXiv:2405.14831 · HippoRAG 2
arXiv:2502.14802 (ICML 2025, PMLR v267).
Family 4: RouterRetriever arXiv:2409.02685 · MoR arXiv:2506.15862 · Blended RAG arXiv:2404.07220 ·
LexBoost arXiv:2409.05882.
Family 5: Self-Route arXiv:2407.16833 · RouteLLM arXiv:2406.18665 · Hybrid LLM arXiv:2404.14618 ·
RouterDC arXiv:2409.19886 · FrugalGPT arXiv:2305.05176 · ZOOTER arXiv:2311.08692 · AutoMix
arXiv:2310.12963.
Family 6: RAGRouter-Bench arXiv:2602.00296 · baseline study arXiv:2604.03455 · RouterBench
arXiv:2403.12031 · LlamaIndex/LangChain routers (docs).
Surveys: Gao et al. RAG survey arXiv:2312.10997 · Modular RAG arXiv:2407.21059 · Agentic RAG survey
arXiv:2501.09136 · LLM-routing survey arXiv:2502.00409.
Benchmarks: HotpotQA 1809.09600 · MuSiQue 2108.00573 · 2WikiMultiHop 2011.01060 · TriviaQA 1705.03551 ·
SQuAD 1606.05250/1806.03822 · PopQA 2212.10511 · BEIR 2104.08663 · ASQA 2204.06092 · ALCE 2305.14627 ·
StrategyQA 2101.02235 · LongMemEval 2410.10813 · LOCOMO 2402.17753.

## Unverified / flagged items

- Venue tags not confirmed to canonical proceedings: Rowen (SIGIR-AP 2025), Self-DC (NAACL Findings
  2025), RQ-RAG (COLM 2024), ReSP (venue unknown). arXiv ids and technical claims confirmed.
- ProbTree & BeamAggR exact baseline names / dataset sets / per-dataset numbers are abstract-level only.
- Oracle/upper-bound presence not confirmed from full PDF for: RouterRetriever, Hybrid LLM, RouterDC,
  ZOOTER, AutoMix (abstract/blog only) — verify before citing those cells.
- Iter-RetGen and SeaKR official code URLs not confirmed.
- arXiv id 2602.00296 carries a Feb-2026 prefix while v1 is dated 30 Jan 2026 (submission/announcement
  boundary); the paper itself is confirmed genuine.
