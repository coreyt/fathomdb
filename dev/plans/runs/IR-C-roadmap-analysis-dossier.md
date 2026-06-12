# IR-C Roadmap — Analysis Dossier (input for the Fable-5 roadmap review, Prompt B)

Status: **analysis dossier, fully source-grounded + verified** · 2026-06-12 · Branch
`claude/recent-changes-state-a6wth3` · Produced by Prompt A (general-purpose, Opus).
Purpose: give the Step-B roadmap reviewer everything needed to design FathomDB's
retrieval roadmap — every quantitative claim cites a source file (`path:line`/`path:§`)
or an external URL. **This is a dossier, not a recommendation** (Prompt B decides).

**Labeling convention (used throughout):**
- **MEASURED-json** — re-derivable from a committed result JSON (independently checked in §4).
- **MEASURED-prose** — a real FathomDB measurement recorded only as prose (the raw
  per-query JSON is gitignored: `data/corpus-data/eval/ir_gold/all.gold.diagnostics.json`).
  Verified against the prose source; **not** marked ✗ for lacking a committed JSON.
- **CLAIMED** — external/literature number (carries its citation + the report's confidence).
- **INFERRED** — derived/arithmetic here (e.g. ceiling math), labeled as such.

**Corpus provenance** (`IR-C-retrieval-findings.md` §caveats/provenance; `ir-c-full-run.7d3011d.log:44-48`):
frozen **10,506-doc** corpus; **4,472 positive** queries (2,888 exact_fact + 1,584
exploratory) + 125 negatives = **4,597 eval queries**; `qrels_version = ir-c-reused-v2`;
`corpus_hash fe973fcd…`. Embedder = `bge-small-en-v1.5`, 384-d, mean-pool, 1-bit binary.

---

## 1. Experiments completed to date

Chronological. Each verdict is one line; numbers cited to file. (Slice runs are
directional/low-distractor; full-corpus runs are the arbiter.)

| # | Experiment | Hypothesis | Result (key numbers) | Source | Verdict |
|---|---|---|---|---|---|
| 1 | **WS1 fusion — 1,200-doc slice** (AND→OR, weight, k) | content-OR + weighted RRF beats AND; chunking + 1:1 wins deep-K | hybrid looked **lexical-bound**: text-only R@10/20/50 = 0.636/0.748/0.848 ≈ ceiling; 1:1 *hurt* shallow; "1:1 deep-K win" did **not** reproduce | `IR-C-api-surface-knobs-to-review.md:100-141` (MEASURED-prose, slice) | Slice biased (~9× too few distractors); inconclusive |
| 2 | **Complementarity diagnostic** (slice, oracle-union) | dense arm is redundant | *not* redundant but thin: union−text = **+0.044/+0.044/+0.048** @R@10/20/50; rescues ~4–5% (11–12/250) | `…knobs:143-175` (MEASURED-prose, slice) | Dense thinly complementary; fusion cashes none |
| 3 | **Full-corpus LEXICAL diagnostic** (model-free, 4,472 q) | exploratory is lexical-bound | exact_fact median BM25 rank **1** (74% @1); exploratory median rank **26** (10% @1, found@1000=0.859); `idf_overlap≈0.70` ⇒ **discrimination**, not vocabulary | `IR-C-retrieval-findings.md:80-94` (MEASURED-prose) | exact_fact lexical; exploratory = rank-26 rerank opportunity |
| 4 | **Full-corpus DENSE diagnostic** (bge, 128/96 max-pool) | chunked dense seizes the rank-26 opportunity | exploratory dense **median rank 99**, top-10/50 = 16%/37% (*worse* than BM25); **9%** semantic-rescued, **38% hard**; oracle union top-50 **53%→62%** | `IR-C-retrieval-findings.md:96-117` (MEASURED-prose) | Chunked dense too weak; 38%-hard stratum dominates |
| 5 | **WS1 fusion — FULL corpus** (the FX_ROW table) | chunking lifts hybrid deep-K at 3:1 vs 1:1 | shipped `h_whole_1:3` exploratory R@10/50 = **0.307/0.520**; `text_only_ORc` 0.327/0.533; `h_128/96_1:3` 0.323/0.532 — chunking ≈ flat at 3:1; 1:1 hurts shallow | `IR-C-ws1-fusion-experiment-full.json`; `ir-c-full-run.7d3011d.log:53-59` (**MEASURED-json**) | Defer chunking/re-weight; hybrid is lexical-bound at scale |
| 6 | **Negative-abstain text variant** (`text_only_ORc`) | text arm abstains on negatives | `negative_abstain_rate = 0.008` (all hybrids = 0.0) | `…full.json:102`; log:53 (**MEASURED-json**) | Text-only abstains on ~0.8% of negatives; hybrids never abstain |
| 7 | **Vector-arm prefix probe** (BGE query instruction on/off) | prefix helps dense | "no material effect"; swept & closed (tiny exact gain, hurts exploratory, every geometry) | `IR-C-retrieval-findings.md:24`; `…knobs:64` (MEASURED-prose) | Closed — no win; not exposed |
| 8 | **Pooling A/B** (CLS vs mean, +prefix) | bge is CLS-native; mean is a bug | exploratory median rank **99→121** (*worse*), top-50 37%→34%; exact_fact 78%→80%; **binary floor: mean 0.946 / CLS 0.944** (both PASS 0.90) | `IR-C-embedder-options-research.md:225-259`; floor: `IR-C-pooling-floor-gate.json` (**MEASURED-json** floor; MEASURED-prose ranks) | Pooling hypothesis **REFUTED**; not a usage bug; stay Mean |
| 9 | **Nomic A/B** (bge vs nomic-embed-text-v1.5) | a stronger model fixes exploratory | exploratory median **99→135** (*worse*), top-50 37%→32%; exact_fact +6 pts (already solved); ~1.4× wall / ~2.2× CPU / ~4× disk | `IR-C-embedder-options-research.md:261-308`; `findings:138-147` (MEASURED-prose) | **REFUTED** — not a model-capacity problem; structural |
| 10 | **BEIR empirical anchor** (bge on SciFact/NFCorpus/ArguAna) | bge runs at its leaderboard level in our pipeline; mean-pool penalty | NFCorpus R@10 = **0.170** (mean) ≈ our exploratory dense ~0.16; SciFact 0.843, ArguAna 0.853; mean-vs-published ≤0.7 nDCG pts | `IR-C-bge-small-beir-anchor.json`; note `…literature-benchmark.md:41-61` (**MEASURED-json**) | Our corpus behaves like a *published hard task*; pooling penalty ≈0 |

**Converged conclusion (the "exploratory is structural" finding,
`IR-C-retrieval-findings.md:9-47`):** three independent dense-quality levers (chunk
geometry, pooling, stronger model) all failed to lift exploratory ⇒ chunk-based
single-vector dense retrieval is **structurally weak** for discourse/summary queries
over long transcripts; BM25 (median 26) is the better exploratory component. Dense
investigation **CLOSED under current knobs**; ship the shipped default.

---

## 2. Current code architecture, knobs, IR numbers

### (a) Retrieval pipeline end-to-end (confirmed against source)

1. **Text arm** — `fathomdb-query::compile_text_query` builds a **content-OR** FTS5
   `MATCH` (stopword/short-token-stripped content tokens OR-joined; all-stopword queries
   fall back to raw-token OR) (`src/rust/crates/fathomdb-query/src/lib.rs:59-73`); the
   engine ranks `ORDER BY bm25(search_index), write_cursor`
   (`fathomdb-engine/src/lib.rs:3983-3987`).
2. **Vector arm** — **whole-doc** embedding (one vector per node, bge-small mean-pool
   384-d) → 1-bit **sign-quant bit-KNN** Hamming with `TOP_K_BIT_CANDIDATES = 192`
   candidates (`lib.rs:3411`) → **f32 rerank** of those candidates.
3. **Fusion** — **unconditional** weighted Reciprocal Rank Fusion: each branch contributes
   `weight / (RRF_K + rank)`, keyed on `SearchHit.body`, deterministic
   (`fuse_rrf`, `lib.rs:3633-3676`). No `fusion_mode` knob (HITL Q3). `rerank_fused`
   (`lib.rs:3707`) is an **identity stub** — the documented MMR/cross-encoder **rerank
   seam** (returns fused order unchanged). Optional G12 recency reweight is gate-off by
   default (`apply_recency_reweight`, `lib.rs:3681`).
   (NB: `RrfHybrid` is a **test-harness** enum, `tests/support/ir_eval.rs:621`, not a lib symbol.)

### (b) Knob table (name → file:line → value → controls)

| Knob | Where set | Current value | Controls |
|---|---|---|---|
| `RRF_K` | `lib.rs:3604` | **30.0** | rank-curve steepness (low=top-heavy); `k10>k30>k60>k100` on sweep, 30 = conservative middle |
| `RRF_WEIGHT_TEXT` : `RRF_WEIGHT_VECTOR` | `lib.rs:3611-3612` | **3.0 : 1.0** (3:1) | lexical-vs-dense dominance; text-dominant per IR-C (1:1 = net drag on exploratory) |
| `TOP_K_BIT_CANDIDATES` | `lib.rs:3411` | **192** | bit-KNN Hamming candidate pool before f32 rerank (bumped 64→192, EU-5a2, above recall-plateau knee) |
| Embedding granularity | engine whole-doc path (`…knobs:25`, `lib.rs` embed path) | **whole-doc** (one vector/node) | passage fan-out vs whole-doc; chunking (128/96) measured but **not shipped** |
| Embedder | `fathomdb-embedder` (`candle_bge.rs:178-185`) | **bge-small-en-v1.5**, 384-d, **mean-pool**, **1-bit binary** + f32 rerank | the dense representation; mean-pool selected EU-0 (cleared binary floor) |
| `RECENCY_WEIGHT` | `lib.rs:3617` | `0.5/RRF_K ≈ 0.0164` | G12 additive recency; off unless caller sets the flag |
| Pooling | `candle_bge.rs` | **Mean** (CLS A/B'd, not adopted) | pooling mode; CLS binary-safe (0.944) but no relevance win |

Proposed-but-unshipped API knobs (`IR-C-api-surface-knobs-to-review.md`): expose
per-request **arm weights (#1)**, **result depth K (#3)**, maybe **RRF k (#2)**; **bake**
max-pool (#6); **do not build** an ingest classifier (#7) or query-intent classifier (#11);
**#8 passage locators** survive on the citation argument independent of recall.

### (c) Headline IR numbers (grounded; **MEASURED-json** unless noted)

All from `IR-C-ws1-fusion-experiment-full.json` (= FX_ROW log table), full corpus, k=30:

| config | exact_fact R@10 | exploratory R@10 | exploratory R@50 | neg-abstain |
|---|---|---|---|---|
| **`h_whole_1:3` (SHIPPED DEFAULT)** | **0.905** | **0.307** | **0.520** | 0.0 |
| `text_only_ORc` (the ~0.33 ceiling arm) | 0.900 | **0.327** | 0.533 | 0.008 |
| `h_128/96_1:3` (chunked, 3:1) | 0.907 | 0.323 | 0.532 | 0.0 |
| `h_whole_1:1` | 0.864 | 0.232 | 0.472 | 0.0 |
| `h_128/96_1:1` | 0.887 | 0.268 | 0.528 | 0.0 |
| `v_whole_max` (dense solo) | 0.590 | 0.073 | 0.230 | 0.0 |
| `v_128/96_max` (dense solo) | 0.686 | 0.163 | 0.368 | 0.0 |

> **Do NOT conflate** the **shipped default `h_whole_1:3` exploratory R@10 = 0.307** with
> the **~0.33 ceiling** (`text_only_ORc`/chunked text arm, 0.323–0.327). The whole-doc
> dense arm at 3:1 sits *slightly below* text-only — the dense arm currently adds nothing
> net on exploratory (it displaces good lexical hits).

Supporting (MEASURED-prose, `IR-C-retrieval-findings.md`):
- exact_fact ≈ **0.90 fused R@10** = the **lexical ceiling** (BM25 median rank 1, 74% @1; vector adds ~1%).
- exploratory: BM25 **median rank 26**; chunked dense **median rank 99**; dense top-50 = 37%.
- **oracle-union** exploratory top-50 = **0.62** (lexical 53% → +9 pts union).
- **~38% hard** (596/1,584): gold in *neither* arm's top-50 — the dominant, chunking-immune stratum.

---

## 3. Graph-related discussion — node / edge / both

**Read the ADR's "conflation to avoid" first** (`ADR-0.8.0-graph-model-and-edge-addressing.md:53-73`):
the shorthand "GraphRAG = fact-on-node, Graphiti = fact-on-edge" is **wrong**. GraphRAG's
*relationships are EDGES*; its node value is **entity / community-summary** nodes (its
optional claims/covariates are the only node-reified statements). Graphiti is
**fact-on-EDGE** (the fact is an edge property; its *nodes* are the provenance/episode
tier). The genuine **fact-on-NODE** tradition is **HyperGraphRAG** (reified n-ary
fact-nodes, arXiv 2503.21322).

### (a) What FathomDB's substrate already is

A **single ontology-neutral binary property-graph substrate** (`…edge-addressing.md:274-279`,
H1 **HITL-SIGNED 2026-06-05**): `canonical_nodes` + `canonical_edges`, active identity =
**`logical_id` alone** (signed Slice 31), invalidate-not-delete via `superseded_at`, opaque-id
edge addressing. Today an edge = `{kind, from_id, to_id, source_id, logical_id, superseded_at}`
— **no body / valid-time / confidence column, and edges are not projected to vector/FTS**
(`…edge-addressing.md:188-205`). Folded traversal indexes `canonical_edges(from_id)/(to_id)`
already landed (Slice 15, SCHEMA_VERSION 12). **No graph verbs ship in 0.8.0**; G4–G7 deferred.

### (b) Three options mapped to the ADR's real taxonomy (`…edge-addressing.md:58-62`)

| Option | Reification | Canonical system | Retrieval implication | Candidate-generation mechanism |
|---|---|---|---|---|
| **(1) Binary edges** [status quo] | typed directed edge row, no body/time | Neo4j default; Mem0 | structure + 1..3-hop traversal; no per-fact embedding, no temporal | bounded recursive-CTE BFS (G5/G6) over `from_id/to_id` — a **third candidate arm** fused via RRF |
| **(2) Temporal fact-EDGES** [Graphiti] | the **edge** carries `fact` text + valid-time (`t_valid`/`t_invalid`) + confidence; episodes as provenance | **Graphiti/Zep** (arXiv 2501.13956) | best for **temporal / contradiction** ("what did we know last week"); invalidate-not-accumulate | point-in-time valid-time range scan + per-fact embed/FTS of edge text |
| **(3) Reified fact-NODES** [HyperGraphRAG] | a **node** with text/valid-time/confidence/embedding + n-ary SUBJECT/OBJECT roles | **HyperGraphRAG** (arXiv 2503.21322) | best for **n-ary facts + heavy per-fact embedding / multi-hop association** | fact-node projects through the existing node vector/FTS path "for free" |

### (c) What the signed ADR leans to, and what's open

- **Memory half → recommended end-state is Option 2 (temporal fact-EDGES)**
  (`…edge-addressing.md:282-291`); **corpus half → GraphRAG entity/community ontology**, equally
  supported on the same neutral substrate. **Engine commits to none** — ontology-neutrality
  (R9) is the load-bearing property.
- **HITL-SIGNED now:** H1 (neutral-both substrate) + H3 (prose-**reserve** edge-enrichment
  columns `body`/`valid_at`/`invalid_at`/`confidence` as additive — v0.5.6 proved them
  portable, `…edge-addressing.md:198-204,337-352`). **Zero 0.8.0 code/schema change.**
- **Open / deferred (decided when built):** H2 addressing (opaque-id now; hybrid
  `(from,to,kind)` MERGE later, **never** identity); H4 provenance; H5 G7-history edge
  scope; H6 fact-node adoption (escape hatch on n-ary demand). **Traversal scope**
  (`ADR-0.8.0-graph-traversal-scope.md`) = **0.8.1 roadmap direction, revisable**: SDK
  depth ceiling ≤3 (typed reject, not clamp), engine hard cap 50, filter
  `superseded_at IS NULL` only (edge valid-time G11 deferred), **G6 = G1+G4+G5+G9** built
  before standalone G5, ported from v0.5.6 BFS — **no new migration** (indexes already folded).

**Why graph is the only lever past the ~0.62 ceiling:** reranking/dense merely *reorder*
the lexical+dense candidate pool, so they cannot exceed the oracle union (~0.62). A graph
traversal arm injects an **orthogonal candidate signal** (multi-hop / temporal), the only
mechanism that can raise the candidate-recall ceiling — concentrated on the **relational /
deep-exploratory** slice, limited on single-transcript summary *discrimination*. **Binding
cost = construction, not retrieval** (see §5): retrieval-time BFS/PPR/RRF is GPU/API-free,
but every published graph system needs an **LLM at index time** for entity/relation extraction.

---

## 4. Check the work — verification log

Each load-bearing number → source → status. (Re-derived FX_ROW from the committed JSON;
prose deltas verified against their note.)

| # | Number | Claimed source | Status |
|---|---|---|---|
| V1 | Shipped `h_whole_1:3` exploratory R@10 = **0.307** | `…full.json:75` (0.30682) = log:56 (0.307) | ✓ MEASURED-json |
| V2 | `text_only_ORc` exploratory R@10 = **0.327** (the ~0.33 ceiling) | `…full.json:96` (0.32702) = log:53 | ✓ MEASURED-json — **distinct from V1** |
| V3 | exact_fact fused R@10 ≈ **0.90** | `…full.json:69` h_whole_1:3 = 0.9048; text_only = 0.8999 | ✓ MEASURED-json |
| V4 | exploratory R@50 (shipped) = **0.520**; ceiling 0.533 | `…full.json:76` (0.5196); `:99` (0.5335) | ✓ MEASURED-json |
| V5 | negative_abstain_rate = **0.008** (text_only); 0.0 hybrids | `…full.json:102`,`:81` = log:53,56 | ✓ MEASURED-json |
| V6 | dense solo exploratory R@10: whole **0.073** / 128/96 **0.163** | `…full.json:138,117` | ✓ MEASURED-json |
| V7 | BM25 median rank: exact **1**, exploratory **26**; rank1-frac 0.738/0.102 | `findings:85-88`; `…knobs:183-186` | ✓ MEASURED-prose (gitignored raw) |
| V8 | dense median rank exploratory = **99**, top-50 37% | `findings:104-106`; `…knobs:218-220` | ✓ MEASURED-prose |
| V9 | oracle-union exploratory top-50 = **0.62** (53%→62%, +9) | `findings:110-113`; `…knobs:230-234` | ✓ MEASURED-prose (846/1584=0.534 lexical re-derived ✓) |
| V10 | **38% hard** = 596/1,584 | `findings:105`; `…knobs:220` (596/1584 = 0.376 ✓) | ✓ MEASURED-prose (arithmetic checks) |
| V11 | **Nomic A/B**: exploratory median **99→135**, top-50 37→32% | `embedder-options-research.md:267-275`; `findings:138-147` | ✓ **MEASURED-prose** (no raw JSON by design — NOT a ✗) |
| V12 | **Pooling A/B**: exploratory median **99→121**, top-50 37→34% | `embedder-options-research.md:231-237` | ✓ **MEASURED-prose** (NOT a ✗) |
| V13 | Pooling **binary floor**: mean **0.946**, CLS **0.944** (both PASS 0.90) | `IR-C-pooling-floor-gate.json:4-15` | ✓ MEASURED-json |
| V14 | BEIR anchor: NFCorpus R@10 **0.170**, SciFact 0.843, ArguAna 0.853 (mean-pool) | `IR-C-bge-small-beir-anchor.json:30-48` | ✓ MEASURED-json |
| V15 | Knobs: RRF_K=30, 3:1, K=192 | `lib.rs:3604,3611-3612,3411` | ✓ code-confirmed |
| V16 | `rerank_fused` = identity stub (rerank seam) | `lib.rs:3707-3709` | ✓ code-confirmed |
| V17 | Corpus 10,506 docs / 4,597 eval (4,472 positive) | `…full.json:152-153`; log:44 | ✓ MEASURED-json |

**Discrepancies / flags:** none material. The log's `FX_ROW` rounds the JSON to 3 dp
(e.g. log 0.307 = json 0.30682) — consistent, not a conflict. **The string "FX_ROW"
appears only in the log, not the JSON** (the JSON `configs` object holds the same rows).
The whole-doc "one vector per node" and the mean-pool embedder are architecture-confirmed
(`lib.rs` embed path; `candle_bge.rs:178-185` per the embedder note) — no committed numeric
artifact, but consistent with every dense-arm result. **No number is asserted by the docs
that cannot be traced to a result file or code.**

---

## 5. External research — the deep-research report, integrated & filtered

Primary external source: `dev/plans/runs/IR-C-roadmap-deep-research.md` (deep-research
workflow, 103 agents, 3-vote adversarial verification, 2026-06-12). **Present** (prerequisite
satisfied). Below: its candidates filtered to FathomDB's constraints (local-first, CPU,
no-API, **1-bit-binary-safe**, small footprint). All effect sizes are **CLAIMED** (external);
the report's own confidence is carried.

### (a) Reranking / retrieval-architecture levers

| Candidate | Expected benefit (CLAIMED) | Footprint / CPU fit | 1-bit compat | Integration cost | Report confidence |
|---|---|---|---|---|---|
| **Small CPU cross-encoder reranker** (FlashRank: ms-marco-TinyBERT-L-2 ~4MB → MiniLM-L-12 ~34MB; ms-marco-MiniLM-L6 22.7M) | BEIR nDCG@10 **0.4328→0.4889 (+0.056)** rerank of BM25 top-1000; beats GTR-4.8B & ColBERTv2 | ✅ CPU/ONNX, no Torch/GPU/API | ✅ operates on text, not vectors — **binary-safe** | **drops into the `rerank_fused` identity stub** (`lib.rs:3707`) — seam already exists | **STRONG** (Rosa et al. 2022) |
| **Vector pseudo-relevance feedback (PRF)** — avg query vec w/ top-k passage vecs | modest, condition-dependent; **no extra neural inference**, ~1/20 BM25+BERT time | ✅ footprint-clean | ✅ operates on f32 rerank vectors | new query-side step over existing f32 vectors | **STRONG** (arXiv 2108.11044) |
| **Late-interaction (ColBERTv2 / PLAID / EMVB)** | higher quality | tens–hundreds ms/query CPU even at PLAID 45× | ❌ multi-vector, **incompatible with single-vector 1-bit Hamming** | **REJECTED — footprint-violating** | STRONG (out) |
| **Listwise / HyDE / ReDE-RF LLM query expansion** | quality | needs a **generative/judge LLM in the query loop** | ❌/⚠️ marginal unless local CPU LLM at query time | **REJECTED unless a CPU LLM is accepted** | STRONG (out) |
| **Large rerankers** (bge-reranker-v2-m3 568M, mxbai-large 435M) | better than small | heavy; 60M tier *net-negative* vs BM25 (Rosa) | n/a | stay in **22–34M ONNX band; expect the LOW end of +0.056** | STRONG caveat |

### (b) Graph-retrieval levers (candidate-generation / multi-hop)

| Candidate | Mechanism + benefit (CLAIMED, **end-to-end answer accuracy, NOT recall**) | Footprint | Integration into FathomDB | Confidence |
|---|---|---|---|---|
| **Zep / Graphiti** (cosine + BM25 + BFS n-hop, fused RRF/MMR + node-distance rerank; bi-temporal edges) | **+11 pts LongMemEval** (gpt-4o 71.2 vs 60.2), −90% latency, 115k→1.6k tokens | ⚠️ retrieval-time clean; **LLM at index time** | **maps ≈1:1** onto FathomDB SQLite+graph substrate (the §3 Option-2 end-state) | STRONG mechanism / WEAK (vendor, end-to-end) |
| **HippoRAG-2** (Personalized PageRank over an LLM-built KG) | **+7%** assoc-memory vs NV-Embed-v2; Recall@5 MuSiQue 69.7→74.7, 2Wiki 76.5→90.4 | ⚠️ ref stack = multi-GPU + 7B embedders; **index-time LLM** | the **PPR idea** is portable to the edge substrate; ref stack is not | STRONG multi-hop / WEAK footprint |
| **LightRAG** (dual-level KG + vector; local/global/hybrid/naive) | architecture template (no single number) | ⚠️ index-time LLM (supports *local* open models) | template for "graph as a third RRF arm" | STRONG design |
| **GraphRAG** (entity/relationship + community summaries) | global sense-making | ⚠️ index-time LLM; app-side map-reduce | corpus-half ontology already representable (§3) | STRONG design |

### (c) The binding constraint (verified against the report's own caveats §d)

- **Construction, not retrieval, is the footprint gate** (`deep-research.md:65-70`): every
  graph system needs an **LLM for entity/relation/triple extraction at index time**;
  non-LLM extractors (REBEL) cause large drops. Retrieval-time math (BFS/PPR/RRF) is
  GPU/API-free. **Open question for FathomDB: can a small *local CPU* extraction LLM build a
  good-enough graph at index time? — untested** (small models show extraction-quality /
  JSON-stability degradation). **INFERRED roadmap pivot:** this, not the retrieval math, is
  the graph go/no-go.
- **Single most important caveat (report §d, WEAK):** the graph gains (HippoRAG-2 +7%, Zep
  +11) are **end-to-end ANSWER accuracy, not first-stage Recall@K** — they do **not**
  directly predict R@10/R@50 lift on FathomDB's bottleneck. Zep's paper is vendor-authored;
  the bi-temporal-drives-the-edge causal claim is contested (paper credits selective context
  retrieval). Reranker numbers are 2022; **candidate-recall-bound** (cannot pass ~0.62 union).
  **No source gives a measured recall lift on FathomDB's own corpus** — all effect sizes are
  transferred from external benchmarks.

**Report load-bearing claims spot-checked against its cited primaries:** reranker +0.056
(arXiv 2206.02873, Rosa et al.); ColBERT/EMVB 1-bit incompatibility + PLAID CPU latency
(2205.09707, 2404.02805); HippoRAG-2 (2502.14802); Zep/Graphiti (2501.13956). These are
labeled STRONG and internally consistent; the graph *gains* are correctly self-flagged WEAK
(metric mismatch). **No targeted gap-fill needed** — the report covers both buckets the
roadmap needs; the one genuine gap (local-CPU extraction-LLM feasibility) is explicitly
flagged untested by the report itself, so it is an **experiment**, not a literature lookup.

---

## 6. FathomDB's goal (stated explicitly)

**Target:** retrieval/answer quality **as-good-or-better than Mem0 and Zep**, achieved
**within the local-first / on-device / CPU / no-API / 1-bit-binary-compact footprint**
(`…literature-benchmark.md:119-136`). FathomDB is the embedded substrate the named
consumers (Memex/Hermes/OpenClaw) otherwise hand-roll on raw SQLite+sqlite-vec/FTS5
(`…edge-addressing.md:78-83`); the deliberate trade is *peak* quality for **zero-API,
private, on-device** operation (`…literature-benchmark.md:128-136`).

**Metric-comparability caveat (load-bearing, `…literature-benchmark.md:192-212`):** peers
report **end-to-end LoCoMo/LongMemEval answer accuracy** (an LLM reads retrieved memory and
answers; an LLM judge scores) — a **different metric and corpus** than FathomDB's
**first-stage Recall@K**. The two are **not directly comparable**; peer figures (Mem0
LoCoMo 92.5 self-reported; Zep vs Mem0 LongMemEval 63.8% vs 49.0%; the disputed LoCoMo
84%→58.44%→75.14%) are **CLAIMED + vendor-contested** and do **not** translate to "N points
above FathomDB." Peers' own lesson: they **do not win with bigger embedders** — same small
model class (`text-embedding-3-small`/small-open), win on **architecture** (hybrid + memory
graph + reranking); Zep's edge over Mem0 is its **graph**.

**What FathomDB would need to measure for a fair comparison:** an **end-to-end QA eval over
its own corpus** (retrieve → LLM-answer → LLM-judge on a LoCoMo/LongMemEval-style task),
not the current first-stage Recall@K. That eval does not exist yet — building it is the
prerequisite to any "as-good-as-Zep" claim.

---

## Open questions surfaced for Prompt B (not decided here)

1. **Candidate-recall ceiling math (INFERRED):** a reranker only reorders the lexical+dense
   pool, so exploratory R@10 is bounded by **R@50 ≈ 0.53** and ultimately the **oracle union
   ≈ 0.62**; the **~38% hard** stratum is untouched by rerank/dense. Expected exploratory
   R@10 lift from a small CPU reranker ≈ 0.307 → ~0.45–0.50 (`…literature-benchmark.md:144-147`)
   — **bounded, real, capped, and zero for factoid (already 0.90) and for the hard 38%.**
2. **Does a local CPU extraction LLM build a good-enough graph at index time?** (the graph
   go/no-go; untested — needs an experiment).
3. **Will the orthogonal graph arm actually raise candidate recall past 0.62 on FathomDB's
   own corpus?** (all external gains are end-to-end answer accuracy, not Recall@K — needs the
   §6 end-to-end QA eval to know).
4. **Whole-doc long-context dense** (the one untested dense angle, `findings:147`): exercises
   a long-context model's real window (unlike the chunked diagnostic) — research probe, gate
   on the 1-bit binary floor; not a committed lever.
5. **Which API knobs to expose** (arm weights / depth K / RRF k) vs bake — `…knobs:78-84`.
