# FathomDB retrieval / recall / QA experiments — neutral data compilation

This document compiles the test setups and measured outputs of FathomDB's retrieval,
recall, and question-answering experiments across the 0.8.0, 0.8.1, and 0.8.2 cycles. It
records **data only**: for each experiment it states what was tested (question, corpus, N,
arms, models, metric, endpoint, caps) and the measured numbers (as tables). It carries no
verdicts, conclusions, or recommendations; where a source document stated a verdict, only
the test description and the measured numbers are reproduced here.

---

## 1. Glossary of metrics used

- **recall@K** — fraction of queries for which a gold/relevant item appears within the top-K
  retrieved results. K values appearing below: 5, 10, 20, 50.
- **MRR** — mean reciprocal rank of the first relevant item.
- **nDCG@10** — normalized discounted cumulative gain at rank 10 (binary relevance unless
  noted).
- **precision@10** — fraction of the top-10 that are relevant.
- **EM (exact match)** — fraction of answers that exactly match the gold answer after answer
  normalization.
- **F1 (token F1)** — token-overlap F1 between predicted and gold answer after normalization.
- **ΔF1 / ΔEM** — paired difference (treatment − comparator) of F1 / EM on the same questions.
- **paired-bootstrap 95% CI** — confidence interval for a paired Δ estimated by resampling
  questions with replacement (n_boot resamples, fixed seed); reported as [low, high].
- **pooled ≥3-hop** — questions requiring 3 or 4 reasoning hops, pooled into one cell.
- **per-hop split** — the same metric computed separately for 2-hop, 3-hop, 4-hop subsets.
- **hop-trend OLS slope** — slope of per-question ΔF1 regressed on hop count; `neg_significant`
  is true only if the slope's bootstrap CI lies entirely below 0.
- **MDE / power** — minimum detectable effect; `power_ok` is a flag set true only when the
  pre-registered whole-rule power simulation attains P(GO) ≥ 0.8 at the target N under the
  modeled effect shape.
- **MATERIAL_F1_LIFT** — pre-registered materiality threshold for ΔF1 (0.8.2/M1: 0.04).
- **answerable / distractor setting** — MuSiQue questions that have a gold answer, retrieved
  against a pool of ~20 paragraphs per question that includes distractors.
- **answer_completeness** — fraction of answerer cells in a run that returned a (non-failed)
  answer.
- **ANN/quantization fidelity recall** — recall@10 of the quantized index measured against
  the exact full-precision top-K over the *same* embedding model (system-internal axis).
- **IR-relevance recall** — recall@10 of `search()` measured against externally-labelled
  relevant doc_ids (qrels / product-value axis).
- **LLM-free recall** — recall computed deterministically by matching retrieved doc keys to
  gold session ids, with no LLM in the scoring loop.

---

## 2. Cycle 0.8.0 — recall fidelity vs relevance (eu7 / eu8 / AC-075)

### 2.1 eu7 — ANN/quantization fidelity recall@10

**Test.** Question: does the production two-phase retrieval (1-bit sign-quant bit-KNN, K=192,
→ f32 rerank) reproduce the exact full-precision f32 top-10 over the same embedding model?
Metric = recall@10 against exact-f32 vector top-10 as ground truth (system-internal fidelity
axis). Endpoint = the engine vector stage (`eu7_real_corpus_ac.rs`). Floor constant = 0.90.
Corpus = the expanded 8-dataset real-embedder corpus (byte-identical between the 0.7.x anchor
and the 0.8.0 measurement). Embedder = bge-small-en-v1.5 (dim 384).

**Output.**

| Measurement | N | recall@10 | CI / σ |
|---|---|---|---|
| 0.7.x anchor (vector stage) | 7,667 | 0.937 | CI 0.913–0.957 (pre-expansion anchor) |
| 0.8.0 vector stage | 1,000 | 0.9240 | — |
| 0.8.0 vector stage | 7,667 | 0.8960 | CI [0.8640, 0.9250], σ 0.0157 |
| 0.8.0 fused `search()` (vector⊕FTS5 RRF) | 7,667 | 0.8710 | — |

### 2.2 eu8 — IR-relevance recall@10 (report-only)

**Test.** Question: when `engine.search()` runs, are the externally-labelled relevant doc_ids
retrieved? Metric = recall@10 (binary relevance) against per-chain `ground_truth_queries`
qrels; precision@10, MRR, nDCG@10 also computed, bucketed per relation-type and chain-shape.
Endpoint = `engine.search()` (`eu8_ir_validation.rs`). N = 301 labelled queries. Embedder =
bge-small-en-v1.5 (dim 384).

**Output.**

| Metric | Value | CI |
|---|---|---|
| recall@10 (IR-relevance ceiling) | 0.571 | 0.530–0.614 |

(precision@10, MRR, nDCG@10 are computed by the harness; the recorded headline figure is the
recall@10 ceiling above.)

---

## 3. Cycle 0.8.1 — "beat BM25" recall + index-key enrichment + fused dense

Common substrate: LongMemEval sessions; knowledge graph extracted by a local Qwen3.6-27B
(Airlock vLLM batch, $0); scoring is LLM-free recall (retrieved doc key vs gold
`answer_session_id`) unless an e2e answerer is noted. Classes = {factoid, knowledge_update,
multi_session, temporal}.

### 3.1 Dense / fused base-retrieval recall (N=160)

**Test.** Question: does a passage-dense / fused (dense+FTS) arm beat BM25 on LongMemEval
recall@K? Corpus = `xiaowu0162/longmemeval-cleaned`, split `longmemeval_s_cleaned`, seed
20260614; N=160 (40/class). Arms = {naive_bm25 (rank-bm25 reference), fathomdb_fts_only,
fathomdb_fused (dense+FTS)}. Metric = recall@{5,10,20}, MRR, nDCG@10, pooled.
Source: `0.8.1-p0a-fused-recall-n160.json`.

**Output — pooled.**

| Variant | R@5 | R@10 | R@20 | MRR | nDCG@10 |
|---|---|---|---|---|---|
| naive_bm25 | 0.506 | 0.625 | 0.694 | 0.510 | 0.479 |
| fathomdb_fts_only | 0.500 | 0.562 | 0.662 | 0.405 | 0.402 |
| fathomdb_fused (dense+FTS) | 0.481 | 0.606 | 0.688 | 0.422 | 0.416 |

Reported deltas: fused R@10 − BM25 = −0.019; fused − FTS = +0.044; multi_session fused 0.325
vs BM25 0.275.

### 3.2 End-to-end answer accuracy (N=160)

**Test.** Same N=160 retrieval feeding an answerer. Reader = `gemini-3.1-pro` (aistudio
batch); judge = `gemini-3.5-flash` (LLM-judge). Metric = per-class + overall answer accuracy.
Source: `0.8.1-p0a-base-e2e-n160-gemini31pro.json` (bm25/fts),
`0.8.1-p0a-fused-e2e-n160-gemini31pro.json` (fused).

**Output.**

| Variant | factoid | knowledge_update | multi_session | temporal | Overall | n_ans |
|---|---|---|---|---|---|---|
| naive_bm25 | 0.675 | 0.475 | 0.250 | 0.225 | 0.406 | 94 |
| fathomdb_fts_only | 0.700 | 0.350 | 0.150 | 0.200 | 0.350 | 89 |
| fathomdb_fused | 0.675 | 0.500 | 0.200 | 0.175 | 0.388 | 95 |

### 3.3 Graph-arm (BFS) recall sweep (N=40)

**Test.** Question: does the BFS graph arm (entity/edge graph + BFS traversal fused via RRF)
beat / add recall over the same engine with the arm off? Corpus = 1,907 LongMemEval sessions,
28,883 entities, 38,021 edges (post anti-pollution run); N=40 (10/class). Arms = {naive_bm25,
fathomdb_fts_only (docs-only index), fathomdb_graph_OFF (entities in index, arm off),
fathomdb_graph_ON (BFS arm on)}. Metric = recall@{5,10,20}, MRR, nDCG@10, pooled. The
load-bearing comparison is graph_ON vs graph_OFF on the same engine. Source:
`/tmp/gar_n40_nopoll.json` (post-filter), `/tmp/gar_n40.json` (pre-filter). Note: edges were
written body-less (source-A edge-fact seeding off).

**Output — post anti-pollution filter, per-class + pooled R@10.**

| Variant | factoid | knowledge_update | multi_session | temporal | Pooled R@10 |
|---|---|---|---|---|---|
| naive_bm25 | 0.60 | 1.00 | 0.30 | 0.90 | 0.70 |
| fathomdb_fts_only | 0.90 | 1.00 | 0.30 | 1.00 | 0.80 |
| fathomdb_graph_OFF | 0.50 | 1.00 | 0.30 | 0.80 | 0.65 |
| fathomdb_graph_ON | 0.60 | 1.00 | 0.10 | 0.90 | 0.65 |

**Output — post-filter pooled secondary metrics.**

| Variant | R@5 | R@10 | R@20 | MRR | nDCG@10 |
|---|---|---|---|---|---|
| naive_bm25 | 0.700 | 0.700 | 0.825 | 0.701 | 0.658 |
| fathomdb_fts_only | 0.725 | 0.800 | 0.800 | 0.640 | 0.633 |
| fathomdb_graph_OFF | 0.575 | 0.650 | 0.675 | 0.489 | 0.468 |
| fathomdb_graph_ON | 0.575 | 0.650 | 0.725 | 0.512 | 0.636 |

**Output — pre anti-pollution filter, per-class + pooled R@10.**

| Variant | factoid | knowledge_update | multi_session | temporal | Pooled R@10 |
|---|---|---|---|---|---|
| naive_bm25 | 0.60 | 1.00 | 0.30 | 0.90 | 0.70 |
| fathomdb_fts_only | 0.90 | 1.00 | 0.30 | 1.00 | 0.80 |
| fathomdb_graph_OFF | 0.40 | 1.00 | 0.20 | 0.80 | 0.60 |
| fathomdb_graph_ON | 0.50 | 0.90 | 0.20 | 0.80 | 0.60 |

A 4-question dry run (`/tmp/gar_dry.json`, 201 sessions, 1 question/class) was recorded as a
plumbing smoke; not statistically meaningful.

### 3.4 R6 index-key enrichment recall (N=40)

**Test.** Question: does appending a session's extracted entities/facts to that session's own
doc FTS content change recall? Corpus = 1,907 sessions (cached graphs); N=40 (10/class).
Variants = {naive_bm25, naive_bm25_enriched, fathomdb_fts_only, fathomdb_fts_enriched,
fathomdb_fts_placebo (length-matched placebo)}. Metric = recall@{5,10,20}, MRR, nDCG@10,
pooled. Source: `0.8.1-R6-recall-n40.json`.

**Output — pooled R@10.**

| Variant | Pooled R@10 |
|---|---|
| naive_bm25 | 0.70 |
| naive_bm25_enriched | 0.75 |
| fathomdb_fts_only | 0.80 |
| fathomdb_fts_enriched | 0.775 |
| fathomdb_fts_placebo | 0.70 |

### 3.5 BM25 `b`-tuning sweep (N=40)

**Test.** Question: how does the BM25 length-normalization parameter `b` interact with
enrichment on recall? Same N=40 corpus. Arms = BM25 at b ∈ {0, 0.25, 0.5, 0.75} × {plain,
enriched}. Metric = pooled R@10. Source: `0.8.1-R6-bsweep-n40.json`.

**Output — pooled R@10 (plain / enriched).**

| b | plain | enriched |
|---|---|---|
| 0.00 | 0.75 | 0.775 |
| 0.25 | 0.725 | 0.775 |
| 0.50 | 0.725 | 0.75 |
| 0.75 | 0.70 | 0.75 |

---

## 4. Cycle 0.8.2 / M1 — knowledge graph vs strong retrieval baseline on multi-hop QA

**Instrument (shared across §4 runs; from the pre-registered design).** A 5-arm,
identical-answerer adjudication on MuSiQue-Answerable (distractor setting, ~20 paragraphs per
question). All arms retrieve over the same per-question candidate pool, top-K=10, read by the
same LLM under the same prompt; retrieval ordering is the only variable.

Arms:

| Arm | Retrieval |
|---|---|
| `bm25` | FTS5/BM25 lexical |
| `passage_dense` | passage-level dense bi-encoder |
| `fused` *(comparator)* | RRF(bm25, passage_dense), k=60 |
| `fused_rerank` | `fused` re-ranked by a CPU cross-encoder |
| `ppr_fusion` *(treatment)* | lexically-seeded Personalized PageRank (α=0.85) over the per-question doc graph, RRF-fused with BM25 |

Primary endpoint = pooled ≥3-hop ΔF1 = F1(ppr_fusion) − F1(fused), paired bootstrap 95% CI
(n_boot=2000, seed=0). Pre-registered MATERIAL_F1_LIFT = 0.04. Secondary = per-hop ΔF1/ΔEM,
hop-trend OLS slope, ΔEM. Mechanical verdict from a frozen `decide()` rule (material / trend /
EM / confident-wrong / power gates). Corpus = MuSiQue-Ans, `musique_hash` =
`3cff37fd7221506a343a125cf7ca20aab7cd09877e376122da9627e1b935b26f`; source
`bdsaglam/musique` rev `22873a405dd809893b22ada0b499299fb612d2df` (re-hosts StonyBrookNLP
v1.0, CC-BY-4.0). Edges built body-less. Graph extracted by local Qwen3.6-27B ($0).

### 4.1 Corpus + graph coverage (N=300 graph)

**Test.** Build statistics and per-question graph coverage for the pinned 300-question
answerable sample (seed 20260617). Source: `0.8.2-m1-graph-coverage-n300.json`,
`0.8.2-m1-corpus-manifest.json`.

**Output.**

| Field | Value |
|---|---|
| docs | 5,999 |
| entities | 50,644 |
| edges | 51,158 (0 with body) |
| total nodes | 56,643 |
| questions | 300 (coverage 1.0; all non-empty) |
| median entities / question | 166.0 |
| median edges / question | 167.0 |
| priced API calls | 0 |

MuSiQue dev manifest counts: total 4,834 (2,417 answerable / 2,417 unanswerable); per-hop
2-hop 2,504, 3-hop 1,520, 4-hop 810.

### 4.2 Baseline e2e — cheap-validate (N=40, flash-lite)

**Test.** 4-arm baseline e2e to validate harness wiring. Reader = `gemini-3.1-flash-lite`.
N=40 (32 answerable / 8 unanswerable). RRF k=60, rerank depth 200, top_k=10. Metric = EM/F1,
pooled ≥3-hop primary cell. Source: `0.8.2-m1-baseline-cheapval.json`.

**Output — pooled ≥3-hop primary cell.**

| arm | n | EM | F1 |
|---|---|---|---|
| bm25 | 24 | 0.0833 | 0.1111 |
| passage_dense | 24 | 0.1667 | 0.1944 |
| fused | 24 | 0.1250 | 0.1556 |
| fused_rerank | 24 | 0.1250 | 0.1528 |

### 4.3 Baseline e2e — pilot (N=100, gemini-3.1-pro)

**Test.** 4-arm baseline e2e pilot, used for variance/power inputs. Reader =
`gemini-3.1-pro`. N=100 (80 answerable / 20 unanswerable). RRF k=60, rerank depth 200,
top_k=10. Metric = EM/F1 + variance, pooled ≥3-hop and per-hop. Source:
`0.8.2-m1-baseline-pilot.json`.

**Output — pooled ≥3-hop primary cell.**

| arm | n | EM | F1 | EM var | F1 var |
|---|---|---|---|---|---|
| bm25 | 60 | 0.1833 | 0.2388 | 0.1523 | 0.1677 |
| passage_dense | 60 | 0.2167 | 0.2617 | 0.1726 | 0.1741 |
| fused | 60 | 0.2333 | 0.3064 | 0.1819 | 0.1915 |
| fused_rerank | 60 | 0.2500 | 0.3060 | 0.1907 | 0.1983 |

**Output — 2-hop split (per-hop secondary).**

| arm | n | EM | F1 |
|---|---|---|---|
| bm25 | 20 | 0.400 | 0.4521 |
| passage_dense | 20 | 0.350 | 0.5070 |
| fused | 20 | 0.450 | 0.5691 |
| fused_rerank | 20 | 0.350 | 0.4656 |

### 4.4 Bridge-vs-answer diagnostic ($0, N=100 pilot answers)

**Test.** Question: does retrieving the complete bridge set predict answer F1? $0 re-run of
retrieval over the 100 pilot-answered questions, joined per-question all-bridges@10 with
stored answer-F1. Source: `0.8.2-m1-bridge-vs-answer-diagnostic.md`.

**Output — complete-bridge effect.**

| Condition | mean answer-F1 | n (q-arm pairs) |
|---|---|---|
| all bridges in top-10 | 0.510 | 186 |
| any bridge missing | 0.068 | 114 |

**Output — conditional on all bridges present.**

| arm | gets-all-bridges freq | F1 when all bridges present |
|---|---|---|
| bm25 | 0.51 | 0.498 |
| passage_dense | 0.68 | 0.464 |
| fused-RRF | 0.65 | 0.552 |
| fused_rerank | 0.64 | 0.526 |

### 4.5 Adjudication cheap-validate (N=15, 5-arm)

**Test.** 5-arm cheap-validate before the priced adjudication pass. N=15 questions (10 in the
pooled ≥3-hop cell). Comparator = fused, treatment = ppr_fusion. Sources:
`0.8.2-m1-verdict-cheapval.json` (flash-lite), `0.8.2-m1-verdict-gpt54-cheap.json` (gpt-5.4).

**Output — primary endpoint (pooled ≥3-hop ΔF1, ppr_fusion − fused).**

| Reader | n | ΔF1 | CI | ΔEM | CI |
|---|---|---|---|---|---|
| flash-lite | 10 | 0.0000 | [−0.300, 0.300] | 0.0000 | [−0.200, 0.300] |
| gpt-5.4 | 10 | −0.016667 | [−0.150, 0.100] | 0.0000 | [−0.300, 0.300] |

**Output — gpt-5.4 cheap five-arm pooled ≥3-hop (n=10).**

| arm | F1 | EM |
|---|---|---|
| bm25 | 0.3452 | 0.20 |
| passage_dense | 0.2667 | 0.20 |
| fused | 0.3952 | 0.30 |
| fused_rerank | 0.3952 | (—) |

### 4.6 Adjudication priced pass — gemini-3.1-pro partial (N=300, completeness 0.8087)

**Test.** Priced 5-arm adjudication pass. Reader = `gemini-3.1-pro`. N=300 answerable.
Comparator = fused (k=60). Run completed 287/1500 calls failed (HTTP 429 rate-limit;
failures clustered in late 3/4-hop questions); answer-matrix completeness = 0.8087; budget
$5.6995 (hit a $25 provider cap). Distinctness: ppr_fusion ≠ bm25 on 66/300 (0.22). Source:
`0.8.2-m1-verdict-n300.json`, `0.8.2-m1-report.md`.

**Output — five-arm pooled ≥3-hop (n=144).**

| arm | F1 | EM |
|---|---|---|
| bm25 | 0.2042 | 0.1609 |
| passage_dense | 0.3541 | 0.2644 |
| fused | 0.2396 | 0.1724 |
| fused_rerank | 0.2637 | 0.2093 |
| ppr_fusion | 0.2270 | 0.1744 |

**Output — primary endpoint (pooled ≥3-hop ΔF1, ppr_fusion − fused).**

| ΔF1 | CI | ΔEM | CI | n |
|---|---|---|---|---|
| −0.015448 | [−0.073453, 0.038302] | 0.0000 | [−0.05814, 0.05814] | 86 |

**Output — per-hop ΔF1.**

| hop | n | ΔF1 | CI low | CI high | ΔEM |
|---|---|---|---|---|---|
| 2 | 156 | −0.0817 | −0.1441 | −0.0248 | −0.0641 |
| 3 | 86 | −0.0154 | −0.0748 | 0.0424 | 0.0000 |
| 4 | 0 | — | — | — | — |

Hop-trend OLS slope = 0.066249 (neg_significant = false). decide() inputs: power_ok = false.

### 4.7 Adjudication priced pass — gpt-5.4 full (N=300, completeness 1.0)

**Test.** Priced 5-arm adjudication pass. Reader = `gpt-5.4`, temperature 0, seed 0. N=300
answerable; 144 in the pooled ≥3-hop cell (hop split {2-hop 156, 3-hop 94, 4-hop 50}).
Comparator = fused (k=60), treatment = ppr_fusion. Run integrity: 1500/1500 cells, 0 errors,
completeness 1.0, budget $2.5032. Distinctness: ppr_fusion ≠ bm25 on 66/300 (0.22). Sources:
`0.8.2-m1-verdict-gpt54.json`, `0.8.2-m1-report-gpt54.md`, `0.8.2-m1-FINDINGS.md`.

**Output — five-arm pooled ≥3-hop F1/EM (n=144).**

| arm | F1 | EM |
|---|---|---|
| bm25 | 0.3700 | 0.2778 |
| passage_dense | 0.4866 | 0.3681 |
| fused | 0.4502 | 0.3542 |
| fused_rerank | 0.4152 | 0.3125 |
| ppr_fusion | 0.4097 | 0.3194 |

**Output — primary endpoint (pooled ≥3-hop ΔF1, ppr_fusion − fused).**

| ΔF1 | CI | ΔEM | CI | n |
|---|---|---|---|---|
| −0.040469 | [−0.115765, 0.031080] | −0.034722 | [−0.111111, 0.041667] | 144 |

**Output — per-hop ΔF1.**

| hop | n | ΔF1 | CI low | CI high | ΔEM |
|---|---|---|---|---|---|
| 2 | 156 | −0.015551 | −0.077178 | 0.048502 | 0.012821 |
| 3 | 94 | −0.044307 | −0.125191 | 0.033901 | −0.042553 |
| 4 | 50 | −0.033254 | −0.179171 | 0.111575 | −0.020000 |

Hop-trend OLS slope = −0.012774 (neg_significant = false). decide() inputs: material.f1_delta
= −0.040469, material.f1_ci_low = −0.115765, em.ci_high = 0.041667, trend.neg_significant =
false, confident_wrong.increase_significant = false (placeholder — unanswerable contrast set
not present), power_ok = false.

**Output — cross-reader direction summary (pooled ≥3-hop, fused vs ppr_fusion F1).**

| Reader | run | fused F1 | ppr_fusion F1 |
|---|---|---|---|
| gpt-5.4 | full (n=144) | 0.4502 | 0.4097 |
| gpt-5.4 | cheap (n=10) | 0.3952 | (per §4.5) |
| gemini-3.1-pro | partial 0.81 (n=144) | 0.2396 | 0.2270 |

A local qwen3.6-27b reader (gold-passage F1 ≈ 0.53) was recorded as a free cross-check.

---

## 5. Index of source artifacts

| Section | Numbers sourced from |
|---|---|
| 2.1 eu7 fidelity recall | `dev/notes/recall-eval-framework-assessment-20260607T174821Z.md`; `dev/plans/runs/STATUS-0.8.0.md` (§§ AC-075 / 2026-06-08 rulings); `eu7_real_corpus_ac.rs`; `ADR-0.7.0-vector-binary-quant.md` |
| 2.2 eu8 IR recall | `dev/notes/recall-eval-framework-assessment-20260607T174821Z.md`; `eu8_ir_validation.rs`; `ADR-0.7.0-vector-binary-quant.md` |
| 3.1 dense/fused recall N=160 | `dev/plans/runs/0.8.1-p0a-fused-recall-n160.json`; `0.8.1-beat-bm25-report.md` §4.4 |
| 3.2 e2e N=160 | `0.8.1-p0a-base-e2e-n160-gemini31pro.json`; `0.8.1-p0a-fused-e2e-n160-gemini31pro.json` |
| 3.3 graph-arm BFS N=40 | `/tmp/gar_n40_nopoll.json`, `/tmp/gar_n40.json`, `/tmp/gar_dry.json`; `0.8.1-beat-bm25-report.md` §4.1–4.3 |
| 3.4 R6 enrichment N=40 | `dev/plans/runs/0.8.1-R6-recall-n40.json`; `0.8.1-beat-bm25-report.md` addendum |
| 3.5 b-sweep N=40 | `dev/plans/runs/0.8.1-R6-bsweep-n40.json`; `0.8.1-beat-bm25-report.md` addendum |
| 4 instrument | `dev/design/0.8.2-m1-multihop-harness.md` |
| 4.1 corpus / graph | `dev/plans/runs/0.8.2-m1-graph-coverage-n300.json`; `0.8.2-m1-corpus-manifest.json` |
| 4.2 baseline cheapval | `dev/plans/runs/0.8.2-m1-baseline-cheapval.json` |
| 4.3 baseline pilot | `dev/plans/runs/0.8.2-m1-baseline-pilot.json` |
| 4.4 bridge diagnostic | `dev/plans/runs/0.8.2-m1-bridge-vs-answer-diagnostic.md` |
| 4.5 adjudication cheap-validate | `dev/plans/runs/0.8.2-m1-verdict-cheapval.json`; `0.8.2-m1-verdict-gpt54-cheap.json` |
| 4.6 gemini-3.1-pro partial | `dev/plans/runs/0.8.2-m1-verdict-n300.json`; `0.8.2-m1-report.md` |
| 4.7 gpt-5.4 full | `dev/plans/runs/0.8.2-m1-verdict-gpt54.json`; `0.8.2-m1-report-gpt54.md`; `0.8.2-m1-FINDINGS.md` |
