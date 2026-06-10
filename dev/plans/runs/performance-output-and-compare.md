# FathomDB IR Performance — Output & External Comparison

**Date:** 2026-06-10
**Run:** IR-C full-corpus Evidence Recall@K (`dev/plans/runs/IR-C-recall-full.json`, commit `fee78ea`)
**Embedder:** `fathomdb-bge-small-en-v1.5` (BAAI bge-small-en-v1.5, 384-dim) · **Mode:** real `Engine::search`
**Corpus:** 10,506 docs embedded · **Queries:** 4,597 (resolved gold, pinned to frozen `corpus_hash`)
**Wall time:** ~2h07m (seed 6,245s + scoring)

---

## 1. Corpus composition (what's actually ingested)

| source_type | docs | sources |
|---|---:|---|
| article | 2,500 | CNN/DailyMail news |
| note | 2,296 | synthetic notes, bahmutov daily-logs, qaconv, chains |
| email | 2,257 | raw Enron, EnronQA, chains |
| **paper** | **1,585** | **QASPER (arXiv/NLP research papers)** |
| meeting | 1,225 | QMSum, QAConv |
| todo | 643 | Landes to-dos, chains |
| **TOTAL** | **10,506** | |

- **arXiv / research papers: YES** — QASPER (1,585 `paper` docs) is in the ingested corpus.
- **Web pages: NO** — no general web crawl; CNN/DailyMail news articles (2,500) are the closest. A `url_or_external_id` field exists but only as provenance metadata.
- **Important:** QASPER (papers) and CNN/DailyMail (articles) have **no eval queries** targeting them. The 4,597 gold queries come only from **EnronQA + QAConv + QMSum**. So ~4,085 docs (papers + articles) act purely as **cross-domain retrieval distractors**, making the pool harder than any single-dataset benchmark.

---

## 2. FathomDB results — Evidence Recall@K

**Metric note:** every resolved-gold query has exactly **one** required evidence doc, so `strict == graded` at every K. This makes "Evidence Recall@K" identical to **standard Recall@K with a single relevant document** (a.k.a. Hit-Rate@K / Success@K) — directly comparable to MS MARCO / EnronQA / QAConv recall@k.

**Class → dataset map:** `exact_fact` (n=2,888) = EnronQA + QAConv factoid QA · `exploratory` (n=1,584) = QMSum meeting-summary queries · `negative` (n=125) = abstention.

### rrf_hybrid (headline mode)
| K | overall | exact_fact | exploratory |
|---:|---:|---:|---:|
| 5  | 0.376 | 0.489 | 0.171 |
| **10** | **0.443** | **0.557** | **0.236** |
| 20 | 0.492 | 0.590 | 0.314 |
| 50 | 0.545 | 0.620 | 0.410 |

### vector_only (dense-only ablation)
| K | overall | exact_fact | exploratory |
|---:|---:|---:|---:|
| 5  | 0.337 | 0.497 | 0.045 |
| 10 | 0.369 | 0.533 | 0.071 |
| 20 | 0.409 | 0.564 | 0.127 |
| 50 | 0.455 | 0.592 | 0.205 |

### rerank_stub
Identity passthrough → **numerically identical to rrf_hybrid** (no real reranker exists yet).

### negative class
n=125, **false-positive rate = 1.0** — `Engine::search` always returns top-k and never abstains (known property, not a regression).

**Hybrid lift on the hard slice:** exploratory R@10 = **0.236 (hybrid) vs 0.071 (vector-only)** — 3.3×. BM25 fusion carries the summary-style queries; pure dense collapses there.

---

## 3. External rule-of-thumb picture (what's "good")

| Anchor | Value | Note |
|---|---|---|
| bge-small-en-v1.5 (our exact model) | BEIR avg **nDCG@10 ≈ 0.52–0.54** | hugely dataset-dependent (Quora 0.89 → SCIDOCS 0.20) |
| Dense Recall@10, MS MARCO (short passages) | **0.55 (DPR) → 0.92 (GTR-XXL)** | pre-chunked passages flatter these |
| Hard-domain Recall@10 (e.g. TREC-COVID) | **0.21–0.50** | long/technical docs collapse recall |
| RAG practitioner thresholds | Recall@5 **≥ 0.80** standard; **0.60–0.70** hard long-tail pre-tuning | context-recall < 0.8 → LLM fabricates |
| nDCG@10 vs Recall@10 | Recall@10 ≥ nDCG@10 | recall ignores rank position |

Bands: **≥0.80 R@5 = production-ready · 0.60–0.70 = workable pre-tuning · <0.5 = needs work / hard domain.**

---

## 4. Same-dataset external benchmarks (the cleanest comparison)

| Dataset | Project | Retriever | Recall@k (their pool) |
|---|---|---|---|
| **EnronQA** | *EnronQA: Personalized RAG over Private Documents* (arXiv 2505.00263, 2025) | **BM25** | **R@5 = 0.875** |
| | | ColBERTv2 (dense) | R@5 = 0.541 |
| | | *pool:* ~492 emails/inbox, single relevant | |
| **QAConv** | *QAConv* (ACL 2021, arXiv 2105.06912) | **BM25** | R@1 0.580 · R@3 0.752 · **R@5 0.800** · R@10 0.848 |
| | | DPR-wiki (dense) | R@1 0.429 · R@3 0.601 · R@5 0.661 · R@10 0.740 |
| | | *pool:* QAConv convs chunked ≤512 tok, single relevant | |
| **QMSum** | *QMSum* (NAACL 2021) "Locator"; *Learning to Rank Utterances* (arXiv 2305.12753) | hierarchical ranker | **ROUGE-L span recall 72.5** (1/6 of turns) — *span metric, not recall@k; not comparable* |

**Cross-cutting finding:** these datasets **reward lexical (BM25) retrieval**; dense retrievers trail badly. EnronQA's authors note this is by construction — questions embed proper nouns to "pick one email out of a batch of ten," creating high query↔doc lexical overlap.

---

## 5. FathomDB vs the same-dataset baselines (exact_fact = EnronQA+QAConv)

| | R@5 | R@10 |
|---|---:|---:|
| BM25 on EnronQA (paper) | **0.875** | — |
| BM25 on QAConv (paper) | 0.800 | **0.848** |
| Dense (ColBERT/DPR, papers) | 0.54–0.66 | 0.74 |
| **FathomDB exact_fact (hybrid)** | **0.489** | **0.557** |
| FathomDB exact_fact (vector_only) | 0.497 | 0.533 |

**Read:** FathomDB lands in **dense-baseline territory (≈ColBERT/DPR), well below the BM25 ceiling these datasets hand you** — even though its headline mode *includes* BM25 in RRF fusion. Its hybrid (0.557) barely beats its own vector-only (0.533) on exact_fact, whereas the literature says BM25 alone reaches 0.80–0.87 here.

### Fairness caveats (explain part of the gap)
1. **Harder pool** — papers retrieve within one dataset (~492-email inbox; QAConv-only chunks); FathomDB retrieves out of a **10,506-doc cross-domain mix** with ~4,085 paper/article distractors.
2. **No chunking** — QAConv's baseline chunks to ≤512 tokens; FathomDB embeds whole bodies (one vector each), the worst case for long QMSum/meeting docs.

### What the caveats do *not* explain
On data that demonstrably favors BM25, **FathomDB isn't capturing the BM25 advantage.** The RRF fusion (hardcoded K=60, equal arm weights) appears to dilute the strong lexical signal with the weaker dense arm. This is a concrete, testable lever (§6).

---

## 6. Improvement levers (in priority order)

| Lever | Current state | Expected effect |
|---|---|---|
| **RRF weighting** (BM25-only / BM25-heavy on exact_fact) | hardcoded equal weights, K=60 (`lib.rs:3600,3620-3659`) | if exact_fact jumps toward 0.7–0.8 → fusion-weighting is the bug, not the embedder |
| **Chunking** | ABSENT — whole body embedded as one vector (`lib.rs:4434`) | biggest lift on QMSum/long-doc exploratory slice |
| **Re-ranking** | STUB — identity passthrough (`lib.rs:3694`) | recall climbs to K=50 (0.545/0.620), so relevant doc is usually in the deeper pool → ranking problem, not findability |
| **NER / augmentation** | ABSENT | entity-aware indexing could capture the proper-noun signal EnronQA rewards |

**Bottom line:** by RAG production rules of thumb FathomDB is **below the workable bar in aggregate but at the embedder ceiling (~0.57) on factoid retrieval.** The same-dataset evidence says these corpora are BM25-friendly with dense trailing — and FathomDB currently performs like a dense baseline rather than the BM25-strong systems the data rewards. The highest-leverage fixes (fusion weighting + chunking + a real reranker) are all visible in the K-ladder and are engineering work, not an embedder swap.

---

---

## Update (2026-06-10) — WS1/WS4 experiment: root cause found

Ran the fusion experiment (`tests/ir_c_fusion_experiment.rs`, harness-side, zero
production change; report `IR-C-ws1-fusion-experiment.json`). **Directional** —
230-query slice over a 1,500-doc corpus, so absolute recall is optimistic; only
**cross-config deltas** are load-bearing. Harness validated against the engine's
real `RrfHybrid` (identical, exact_fact R@10 0.773).

**The bug is query compilation, not fusion.** `compile_text_query`
(`fathomdb-query/src/lib.rs`) ANDs every query token, so the BM25 arm almost
never matches a natural-language question:

| Config (exact_fact / exploratory R@10) | exact_fact | exploratory |
|---|---:|---:|
| `bm25_only` (production **AND**-join) | **0.080** | 0.362 |
| `bm25_only_OR` (bag-of-words **OR**) | **0.933** | 0.650 |
| current hybrid (`RrfHybrid`) | 0.773 | 0.438 |
| `hybrid_OR_3x` (OR text, BM25-weighted 3×) | **0.940** | 0.613 |

**Findings:**
1. **RRF arm-weighting is a null lever** under AND-join (BM25-heavy *hurt* exact_fact);
   under OR it *helps* (3× best) because the arm is finally informative.
2. **Text-arm ordering** (`write_cursor` vs `bm25()`, `lib.rs:3968`) is second-order
   — only matters where the arm returns docs (exploratory).
3. **AND→OR query compilation is the dominant lever**: exact_fact R@10 0.773→0.940,
   exploratory 0.438→0.613. `bm25_only_OR` alone (0.933) nearly matches the best
   hybrid, recovering the literature's BM25 dominance on these datasets.

**Caveat before shipping:** the metric is single-evidence *Recall@K* (precision-blind).
Pure OR maximizes recall but likely lowers precision and would *worsen* the
negative/abstention class (more spurious matches). A guarded variant
(OR + `bm25()` ranking + score threshold, or N-of-M token match) may beat pure OR
on the precision/abstention axis. Needs full-corpus + negative-class validation
before a production `compile_text_query` change.

## Update (2026-06-10) — guarded variant tested; OR is a clean win

Re-ran with the negative/abstention class (60 negatives) + an N-of-M
content-token coverage guard with an abstention gate (`hybrid_OR_3x_gateNN`).

| Config | exact_fact R@10 | explor R@10 | negative abstain |
|---|---:|---:|---:|
| `hybrid_current` (production today) | 0.773 | 0.438 | **0.00** |
| `bm25_only_AND` (today's lexical arm alone) | 0.080 | 0.362 | 0.87 |
| `hybrid_OR_3x` (OR, ungated) | **0.940** | 0.613 | 0.00 |
| `hybrid_OR_3x_gate50` | 0.907 | 0.675 | 0.05 |
| `hybrid_OR_3x_gate67` | 0.687 | 0.600 | 0.50 |
| `hybrid_OR_3x_gate100` | 0.227 | 0.338 | 0.88 |

**Conclusions:**
1. **Coverage-gating REJECTED as a precision lever** — no good knee: gate50 barely
   abstains (0.05) while keeping recall; gate67 buys 0.50 abstention at ~25 recall
   points; gate100 collapses to AND. Token-overlap ≠ relevance.
2. **OR is a near-pure win for the real hybrid path** — +0.167 exact / +0.175
   exploratory R@10, with **no abstention regression**: the production hybrid
   *already* abstains 0.00 (the vector arm always returns neighbors), so OR doesn't
   make abstention worse than it already is.
3. **Abstention (FPR=1.0) is pre-existing and orthogonal** to query semantics — it
   needs a real confidence gate (a reranker, WS3, or a calibrated score threshold),
   not the AND-join's accidental over-strictness and not coverage-gating.

**Recommendation:** ship the OR query-compilation fix (`compile_text_query`
AND→OR, order the text arm by `bm25()`) as the WS4 recall lever — validated as a
clean win on the hybrid path. Track abstention/precision as a separate WS3 effort.
All directional (1,500-doc slice); confirm on a full-corpus run before landing.

## Update (2026-06-10b) — re-sweep: the OR fix UN-buried the "null" levers

We moved sequentially (weight → root cause → OR fix). Re-running the *initial*
levers on the now-fixed OR base shows WS1's "weighting/k are null" verdict was an
artifact of the broken AND-join (text arm ≈ 0.08 had no signal to weight). On the
fixed base they are real levers:

| Config | exact_fact R@10 | exploratory R@10 |
|---|---:|---:|
| `hybrid_current` (today) | 0.773 | 0.438 |
| `vector_only` | 0.733 | 0.375 |
| `text_only_OR` | 0.933 | 0.650 |
| `hybrid_OR_1:1` | 0.840 | 0.487 |
| `hybrid_OR_1:3` | **0.940** | 0.613 |
| `hybrid_OR_1:5` | 0.940 | 0.637 |
| `hybrid_OR_2:1` / `3:1` (vector-heavy) | 0.80 / 0.79 | 0.388 / 0.400 |
| `hybrid_OR_1:2_k10` | 0.940 | 0.650 |
| `hybrid_OR_1:2_k30` | 0.940 | 0.600 |
| `hybrid_OR_1:2_k100` | 0.900 | 0.525 |
| `text_only_ORc` (content-OR) | 0.933 | **0.688** |
| `hybrid_ORc_1:3` | 0.933 | 0.613 |

**Levers, re-judged:**
1. **RRF weight — now REAL (was "null").** The optimum is strongly *text-dominant*:
   exact_fact climbs with text weight and plateaus ~0.940 by 1:3; vector-heavy
   (2:1/3:1) collapses back toward `vector_only`. Exploratory is *monotonically hurt*
   by vector — `text_only` (0.650) beats every hybrid ratio. The vector arm
   (0.733/0.375) is now the weak link and a net drag when over-weighted.
2. **RRF k — now a MILD lever (was "null").** Lower k is better: at 1:2,
   k10 (0.940/0.650) > k60 > k100 (0.900/0.525). The gain is concentrated at low K
   (top-of-list); the production default (k≈60) is slightly too high.
3. **content-OR (NEW lever).** Stripping stopwords from the OR query lifts
   exploratory R@10 0.650 → **0.688** with no exact_fact cost — the best exploratory
   number in the sweep. Clean text-arm improvement over raw-OR.
4. **Vector-arm quality (NEWLY the ceiling).** With the lexical arm fixed, the
   embedding arm is the bottleneck (0.733/0.375). The highest-value *untested* lever
   now is vector quality (embedding model/dims, query-side embedding, candidate
   depth) — bigger than any fusion knob, and the only path past ~0.94.

**Recommended operating stack (directional):** content-OR query compilation +
text-dominant fusion (text:vector ≈ 3:1–5:1, or text-only on exploratory-heavy
workloads) + lower RRF k (~10–30). Components individually hit exact_fact R@10
≈ 0.94 / exploratory ≈ 0.65–0.69; not yet measured as a single combined config.
Still a 1,500-doc slice — confirm on full corpus before landing.

## Sources
- EnronQA — https://arxiv.org/html/2505.00263v1
- QAConv — https://ar5iv.labs.arxiv.org/html/2105.06912
- QMSum (NAACL 2021) — https://aclanthology.org/2021.naacl-main.472.pdf
- Learning to Rank Utterances (QMSum) — https://arxiv.org/pdf/2305.12753
- BEIR — https://arxiv.org/pdf/2104.08663 · Large Dual Encoders — https://arxiv.org/pdf/2112.07899
- BGE MTEB eval — https://bge-model.com/tutorial/4_Evaluation/4.2.1.html
- RAG recall@k thresholds — https://towardsdatascience.com/how-to-evaluate-retrieval-quality-in-rag-pipelines-precisionk-recallk-and-f1k/
