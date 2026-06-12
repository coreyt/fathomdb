# IR-C Retrieval Roadmap — Review + Prioritized Roadmap + Goal Assessment

Status: **decision artifact (Prompt B, Fable-5, high reasoning)** · 2026-06-12 ·
Branch `claude/recent-changes-state-a6wth3`.
Input: `dev/plans/runs/IR-C-roadmap-analysis-dossier.md` (Prompt A) ·
`dev/plans/runs/IR-C-roadmap-deep-research.md` (Step 0) · raw sources re-checked
(`IR-C-ws1-fusion-experiment-full.json`, `IR-C-pooling-floor-gate.json`,
`IR-C-bge-small-beir-anchor.json`, `ir-c-full-run.7d3011d.log`,
`IR-C-retrieval-findings.md`, `IR-C-api-surface-knobs-to-review.md`,
`dev/notes/IR-C-bge-small-literature-benchmark.md`,
`dev/notes/IR-C-embedder-options-research.md`,
`dev/adr/ADR-0.8.0-graph-model-and-edge-addressing.md`, engine/query source).
Labeling convention carried from the dossier: MEASURED-json / MEASURED-prose /
CLAIMED (external) / INFERRED (derived here).

> **◆ TARGETING (HITL coreyt, 2026-06-12): R0–R4 are 0.8.1 scope — option ②-B accepted.**
> R0–R4 are brought into 0.8.1 (`dev/roadmap/0.8.1.md` §5), including the **full R3 temporal
> fact-edge mechanism with G11 edge valid-time activated** (no R3a/R3b split). **Decision ①:**
> AC-077 (Evidence Recall@K) stays the IR-eval product gate; R2's end-to-end Mem0/Zep eval is a
> report-only north-star (wired into `dev/plans/0.8.0-GA-and-IR-eval-roadmap.md` IR-E/IR-2). R5
> (vector-PRF) is opportunistic/post-R1, not in the committed R0–R4 set. The graph arm (R3) rides
> the existing 0.8.1 graph-traversal substrate (0.8.1.md §1); CLOSED levers unchanged.

---

## 1. Review of the dossier (adversarial audit)

### 1.1 Correctness — spot-check results

I independently re-derived the dossier's load-bearing numbers against the raw
artifacts. **All checks pass:**

| Check | Result |
|---|---|
| FX table (V1–V6, V17): h_whole_1:3 explor R@10 0.30682 / R@50 0.5196; text_only_ORc 0.32702 / 0.5335; exact 0.9048/0.8999; neg-abstain 0.008/0.0; dense solo 0.0726/0.1629; 10,506 docs / 4,597 queries | ✓ exact match, `IR-C-ws1-fusion-experiment-full.json` = `ir-c-full-run.7d3011d.log:53-59` (3-dp rounding only) |
| Binary floor (V13): mean 0.946 / CLS 0.944, both PASS 0.90 | ✓ `IR-C-pooling-floor-gate.json:6-14` |
| BEIR anchor (V14): NFCorpus R@10 mean-pool 0.16957; SciFact 0.84289; ArguAna 0.85277; mean-vs-published Δ ≤0.7 nDCG pts, sign varies | ✓ `IR-C-bge-small-beir-anchor.json` (mean-pool actually *beats* published on SciFact/NFCorpus) |
| Lexical/dense diagnostics (V7–V10): exact median 1 (0.738@1), explor median 26 (0.102@1, found@1000 0.859, idf_overlap 0.704); dense median 99, top-50 37%; buckets 846/142/596 → union 988/1584 = 0.6237; hard 596/1584 = 0.376 | ✓ `IR-C-retrieval-findings.md:85-105`; arithmetic re-derived |
| Pooling/Nomic A/Bs (V11, V12): 99→121 / 37→34%; 99→135 / 37→32%, exact +6; ~1.4× wall / ~2.2× CPU / ~4× disk | ✓ `IR-C-embedder-options-research.md:231-237,267-275` |
| Code claims (V15, V16): `RRF_K=30.0` (`lib.rs:3604`), 3:1 weights (`lib.rs:3611-3612`), `TOP_K_BIT_CANDIDATES=192` (`lib.rs:3411`), `rerank_fused` identity stub (`lib.rs:3704-3709`), content-OR compile (`fathomdb-query/src/lib.rs:59-73`), `ORDER BY bm25(...), write_cursor` (`lib.rs:3983-3987`) | ✓ all confirmed in source |
| ADR claims (§3): conflation taxonomy, edge shape, H1/H3 SIGNED, H2/H4/H5/H6 deferred, v0.5.6 portability | ✓ `ADR-0.8.0-graph-model-and-edge-addressing.md:53-73,188-205,270-295,330-352` |

No mislabeled MEASURED/CLAIMED/INFERRED found. Trivial nit: the dossier writes
`RECENCY_WEIGHT = 0.5/RRF_K ≈ 0.0164` (it is 0.0167; the engine comment's own
"1/(RRF_K+1) ≈ 0.0164" is also loose) — immaterial. Naming nit: calling
`text_only_ORc` R@10 = 0.327 "the ~0.33 ceiling" is shorthand — it is the text
*arm's* R@10, not a proven fused ceiling at R@10 (the slice oracle-union R@10
beat text-only by +0.044, `…knobs:149-159`); the dossier's own deep-K framing is
the correct one.

### 1.2 Framing corrections (these change the roadmap math)

**C1 — The "~0.53–0.62 candidate-recall ceiling" is DEPTH-CONDITIONAL, and the
dossier under-carries its own escape hatch.** The 0.53 (fused/text R@50) and
0.62 (oracle-union top-50) are ceilings **at candidate depth 50**. The dossier's
own row 3 records **exploratory BM25 found@1000 = 0.859**
(`IR-C-retrieval-findings.md:88`) but never propagates it into the ceiling
math (§Open question 1). A reranker's true bound is the candidate recall **at
whatever depth you rerank** — and the external evidence the dossier cites for
the reranker (+0.056 BEIR nDCG@10, Rosa et al., arXiv 2206.02873) is itself a
**top-1000** rerank. Quoting that magnitude while capping the ceiling at
depth-50 is internally inconsistent. Corrected framing: rerank depth is a free
knob (latency-bounded); the reachable ceiling rises from 0.52–0.53 (depth 50)
toward 0.859 (depth 1000, lexical arm alone). Found@100/found@200 were not
committed; log-interpolating (50, 0.534)→(1000, 0.859) gives **≈0.61 @100,
≈0.68 @200, ≈0.78 @500 (INFERRED — must be measured, R0 below)**.

**C2 — The "~38% irreducible hard core" is a depth-50 artifact, not an absolute.**
"Hard" is defined as *gold in neither arm's top-50*
(`IR-C-retrieval-findings.md:98-100`). At depth 1000 the lexically-unreachable
stratum is only **~14%** (1 − 0.859), before counting dense rescues. So the
truly reorder-proof core is ≈14%, not 38% — *if* you pay for deep candidates.
The correct statement: **38% is unreachable by reordering at the current K=50
economy; ~14% is unreachable by lexical retrieval at any practical depth.** A
second, under-carried caveat compounds this: the qrels are **single-gold** and
the findings note itself flags exploratory labels as "single-doc/sparse"
(`IR-C-retrieval-findings.md:40-44`) — part of the hard core is plausibly label
noise (defensible non-gold answers), which deflates measured R@K for *every*
system and slightly inflates the apparent hard core.

**C3 — "Graph is the only lever past the ~0.62 ceiling" is FALSE as written.**
Deep-candidate reranking passes 0.62 lexically (C1). The corrected claim:
graph is the only lever for (a) the **~14% lexically-unreachable-at-depth-1000**
stratum, and (b) **query classes the current qrels barely contain** —
multi-hop, temporal, knowledge-update. The measured hard core is dominated by
single-transcript summary *discrimination*, which the dossier's own §3 admits
graph helps least ("limited on single-transcript summary discrimination").
Consequence: **graph's payoff is mostly invisible to the current Recall@K
instrument** — it can only be seen on an end-to-end memory eval (R2). This
inverts part of the sequencing logic: the eval is a *prerequisite* for the
graph decision, not a parallel nicety.

**C4 — The 0.62 union is the CHUNKED-arm union; production ships whole-doc
dense.** The semantic/hard buckets used the 128/96 arm (whole-doc geometry was
skipped, `IR-C-retrieval-findings.md:152-154`). Production whole-doc dense solo
is far weaker (explor R@50 0.230 vs 0.368, MEASURED-json), so the **production**
oracle union is unmeasured and sits somewhere in (0.533, 0.62). Any rerank-over-
production-pool projection should use the conservative end unless the chunked
arm (or a deeper text arm) feeds the reranker pool.

**C5 — "Dense-embedder lever closed by the Nomic A/B" — carried CORRECTLY,
keep the scope qualifier.** Three independent negatives (chunk geometry,
pooling, stronger model: `IR-C-retrieval-findings.md:18-33`;
`IR-C-embedder-options-research.md:225-308`) close **chunk-based single-vector
dense quality** specifically. Whole-doc long-context (and late chunking) is
**parked-untested, not refuted** — the Nomic A/B wasted nomic's 8192-ctx on
128-word windows, so it cannot speak to that mechanism. The dossier carries
this correctly (open question 4); the roadmap must not let "closed" creep over
the untested angle, nor let the untested angle masquerade as likely (the
literature's "~2×" is vendor-directional and was already superseded once for
the chunked case, `IR-C-bge-small-literature-benchmark.md:76-89`).

**C6 — Factoid headroom is not exactly zero.** exact_fact R@10 0.905 vs R@50
0.950, found@1000 0.985 (MEASURED). A reranker has ~4.5 pts of depth-50 headroom
there — small, but the real point is the **regression risk**: BM25's within-pool
top-10 conversion on factoid is 0.905/0.950 ≈ 0.95, which a cross-encoder can
easily *underperform*. Any rerank gate must pin factoid R@10 ≥ 0.90 (no-regress),
and score-blending (RRF+CE interpolation) is safer than pure reorder.

**C7 — The footprint constraint on graph construction has a load-bearing
loophole the dossier doesn't draw.** "No-API / CPU" binds **FathomDB the
library**. Its named consumers (Memex/Hermes/OpenClaw, `…edge-addressing.md:78-83`)
are **LLM agents** — they already have an LLM in the loop at ingest time. The
graph-construction LLM can therefore be **caller-supplied** (the consumer's
agent extracts facts/edges and writes them through a graph ingest API), exactly
as Mem0/Zep operate as libraries. This decouples "does the graph mechanism
help" from "can a local CPU LLM extract well enough," splitting one low-odds
bet into two independent gates (R3a/R3b below) and materially raising graph
feasibility. The deep-research report's go/no-go framing
(`IR-C-roadmap-deep-research.md:65-70`) should be read through this lens.

**C8 — "All graph gains are end-to-end answer accuracy, not recall" is slightly
overstated.** HippoRAG-2 reports **passage Recall@5** lifts (MuSiQue 69.7→74.7,
2Wiki 76.5→90.4; arXiv 2502.14802) — first-stage recall evidence, albeit on
multi-hop QA corpora unlike ours. The correct caveat: *no graph recall evidence
on FathomDB-like discourse/transcript corpora*; multi-hop recall evidence
exists but its transfer is unknown.

### 1.3 Completeness — gaps for a roadmap decision

1. **BM25/union candidate-recall CDF at depths 100/200/500** — absent; it sizes
   the reranker (C1). Recoverable from the existing per-query diagnostics
   (`bm25_gold_rank` in the gitignored `all.gold.diagnostics.json`) at ~zero cost.
2. **No end-to-end QA metric** — acknowledged by the dossier (§6); without it no
   parity claim is possible and graph gains are invisible (C3).
3. **No reranker latency budget** — a CPU cross-encoder at depth 100–200 costs
   real per-query milliseconds-to-seconds; must be reconciled with the tiered
   AC-013 budget (10k-doc binding) or shipped as an opt-in knob.
4. **Production (whole-doc) union unmeasured** (C4).
5. **Graph candidate-generation design unspecified** beyond the ADR's taxonomy —
   edge bodies are not projected to FTS/vector today (ADR "cracks" 1–3,
   `…edge-addressing.md:188-198`); the third-arm design needs that projection.
6. **Label-quality bound** (C2) — un-quantified; a small human audit of "hard"
   exploratory queries would bound how much of the 38% is real.

---

## 2. Prioritized roadmap

Goal: retrieval/answer quality as-good-or-better than Mem0 and Zep **within
local-first / CPU / no-API / 1-bit-binary**. Footprint is a hard constraint;
every violation is flagged. Baseline: shipped `h_whole_1:3` — exact_fact R@10
**0.905**, exploratory R@10/R@50 **0.307/0.520** (MEASURED-json). All effect
sizes below are derived from the corrected headroom math (§1.2 C1/C2), not from
fixed anchors.

### R0 — Candidate-recall CDF + rerank cost model (measurement; do first)

- **What & why:** Recompute per-class found@K for K ∈ {10,20,50,100,200,500,1000}
  for the text arm, both dense arms, and their unions, from the existing
  diagnostics (`bm25_gold_rank`/`dense_gold_rank` per-query records; runbook in
  `dev/plans/IR-C-test-query-quality-instrumentation-plan.md`). Plus: measure
  CPU cross-encoder ms/pair at 2 model sizes (TinyBERT-L-2 ~4MB,
  MiniLM-L6 22.7M) on this corpus's passages. This converts the INFERRED
  interpolation (≈0.61@100 / ≈0.68@200) into MEASURED numbers and fixes the
  rerank depth knob *before* any integration work.
- **Probability of success:** ≈99% it produces the decision numbers — it is
  arithmetic over data that already exists (found@1000 already measured,
  `IR-C-retrieval-findings.md:88`) plus a micro-benchmark. No external citation
  needed; nothing is being bet on.
- **Effect size:** none directly; it bounds R1's ceiling (0.53 → up to 0.86).
- **Footprint:** ✅ trivially (offline analysis).
- **Cost & sequencing:** ~days. **First** — R1's design depends on it.
- **Measurement/gate:** the CDF table itself, committed as a result JSON
  (avoid the MEASURED-prose-only trap the dossier had to label around).

### R1 — Small CPU cross-encoder reranker in the `rerank_fused` seam (the
highest-probability win)

- **What & why:** Replace the identity stub (`lib.rs:3704-3709`, code-confirmed
  seam) with a 4–34MB ONNX/candle cross-encoder scoring `(query, passage)` over
  a **deepened** candidate pool (text arm depth 100–200 + vector arm ≤192
  bit-candidates), blending CE score with the RRF score (not pure reorder — C6
  guard). Mechanism fit is exact: the measured exploratory bottleneck is
  **discrimination, not vocabulary** — gold's terms are present
  (idf_overlap ≈ 0.70) but BM25 can't separate the right transcript (median
  rank 26, 10% @1; `IR-C-retrieval-findings.md:85-94`). Token-level
  cross-attention is precisely a discrimination instrument. Targets:
  exploratory (primary); factoid no-regress (guard).
- **Probability of success — with reasoning and citations:**
  - *P(helps at all)* ≈ **85%**. Break-even math: in-pool, BM25's top-10
    conversion is 0.307/0.520 ≈ 0.59 (MEASURED-json derived) — the CE merely has
    to out-order BM25 within a 50–200 candidate pool. Externally, a ~22M MiniLM
    CE reranking BM25 top-1000 lifts zero-shot BEIR nDCG@10 0.4328→0.4889
    (+0.056), beating GTR-4.8B and ColBERTv2 (Rosa et al., arXiv 2206.02873;
    carried STRONG in `IR-C-roadmap-deep-research.md:30`); CE-over-BM25 is the
    standard peer component (verified 3-0, `IR-C-bge-small-literature-benchmark.md:113-117`).
    The residual 15% covers: MS-MARCO-trained CEs transferring poorly to
    discourse/summary queries over transcript passages (our corpus ≈ a published
    *hard* task — NFCorpus anchor, MEASURED-json), and 512-token CE windows
    forcing best-passage aggregation over long docs.
  - *P(material, exploratory R@10 ≥ +0.05)* ≈ **70%**, given the small-CE "low
    end of +0.056" caveat (deep-research §d) and the depth economics below.
  - *Key failure risk:* CE conditional precision ≤ ~0.6 on this query class
    (≈ BM25's own), i.e. discourse-query transfer failure — detectable in week 1
    on the harness.
- **Effect size (derived from headroom, not anchored):** realized R@10 =
  P(gold in pool) × P(CE ranks it top-10 | in pool). Depth-50 fused pool
  (0.52 MEASURED) × CE conditional 0.70–0.85 (Rosa-class) → **0.36–0.44**
  (+0.05 to +0.13). Depth-200 pool (≈0.68 INFERRED, pending R0) × 0.55–0.75 →
  **0.37–0.51**. Bounded by the depth-K candidate CDF, NOT by 0.62 (C1);
  contributes ~0 to the ~14% lexically-unreachable core (C2) and ~0–0.04 to
  factoid (C6). The literature note's own 0.33→0.45–0.50 projection
  (`IR-C-bge-small-literature-benchmark.md:144-147`) sits at this range's
  optimistic end and used the text-arm baseline; from the shipped 0.307 the
  honest central estimate is **≈0.40–0.47 at depth 100–200**.
- **Footprint fit:** ✅ CPU/ONNX (FlashRank-class, no Torch/GPU/API;
  `deep-research.md:30`); ✅ no-API; ✅ 1-bit-safe (operates on text, not
  vectors). ⚠️ **Latency** is the one footprint pressure: ship as an opt-in
  per-request knob (`rerank_depth`, aligning with the parked API-knob review,
  `IR-C-api-surface-knobs-to-review.md:78-84`) and verify against the tiered
  AC-013 budget. ❌ Large rerankers (bge-reranker-v2-m3 568M, mxbai 435M)
  rejected — heavy, and the 60M tier is net-negative vs BM25 per Rosa
  (deep-research §a) — stay in the 4–34M band.
- **Cost & sequencing:** ~1–2 weeks engineering (new model asset 4–34MB vs the
  existing 133MB bge; candle/ort inference; harness wiring). After R0; before
  everything else — cheapest high-probability win, and it raises the floor every
  later initiative is measured against.
- **Measurement/gate:** fusion harness (`ir_c_fusion_experiment` + a rerank
  config row) on the frozen corpus: **PASS = exploratory R@10 ≥ 0.37 (+0.06)
  AND exact_fact R@10 ≥ 0.90 AND negative-abstain not degraded AND opt-in
  latency documented.** Binary floor untouched (text-side change).

### R2 — End-to-end QA eval (the fairness instrument for Mem0/Zep; the
uncertainty-reducer)

- **What & why:** Build a LongMemEval-style end-to-end eval: retrieve from
  FathomDB → fixed **identical local answerer LLM** → LLM-judge scoring, over
  (a) LongMemEval proper (500 public questions, harness at
  github.com/xiaowu0162/longmemeval; arXiv 2410.10813, ICLR 2025) and/or
  (b) a FathomDB-corpus QA task. Run **locally-hosted Mem0 OSS** (and naive-RAG)
  as baselines under the *same answerer*. This is the only way to make the peer
  comparison fair: peers report end-to-end answer accuracy on
  LoCoMo/LongMemEval, vendor-contested (Mem0 92.5 self-reported; Zep 63.8 vs
  Mem0 49.0; the 84%→58.44%→75.14% LoCoMo dispute —
  `IR-C-bge-small-literature-benchmark.md:192-212`), which is **not comparable**
  to first-stage Recall@K. It is also the only instrument that can *see* graph
  gains (C3) and abstention quality (hybrids currently never abstain,
  neg-abstain 0.0 MEASURED-json).
- **Probability of success:** ≈**90%** the harness gets built and yields stable
  relative numbers — it is engineering on public assets (LongMemEval is
  released with code; Mem0 is OSS, arXiv 2504.19413). Residual risk: LLM-judge
  variance with a *local* judge; mitigate with fixed seeds + answer-key classes
  (LongMemEval's design includes abstention/temporal classes that are
  deterministic to score).
- **Effect size:** none on retrieval; it converts the parity goal from
  unfalsifiable to measurable.
- **Footprint fit:** ✅ for the *product* (nothing ships). The eval harness
  itself needs an answerer/judge LLM — a **dev-time** dependency, not a product
  API dependency; prefer a local model for reproducibility. Flagged, not a
  violation.
- **Cost & sequencing:** ~2–3 weeks. Start in parallel with R1; **must complete
  before the graph go/no-go** (C3).
- **Measurement/gate:** the eval itself: FathomDB (R1 build) vs naive-RAG vs
  local Mem0-OSS, identical answerer, per-class accuracy + abstention. Any
  external parity claim cites *this*, never vendor numbers.

### R3 — Graph-aware retrieval: temporal fact-EDGES as a third RRF arm
(two-gate bet; the only lever for the lexically-unreachable + temporal/multi-hop
classes)

- **What & why — and the node/edge/both pick:** Adopt **Option 2, fact-on-EDGE
  (Graphiti-shaped temporal fact-edges)**, with entity nodes as endpoints (so
  "edge-carried facts over an entity-node skeleton") and HyperGraphRAG-style
  fact-NODES kept as the signed H6 escape hatch for n-ary demand. Why edge:
  (i) it is the ADR's recommended memory-half end-state, with the enrichment
  columns (`body`/`valid_at`/`invalid_at`/`confidence`) already HITL-signed as
  prose-reserved additive and **proven portable from v0.5.6**
  (`…edge-addressing.md:282-291,330-352`); (ii) the goal is Mem0/Zep parity and
  Zep's differentiator over flat Mem0 is precisely its bi-temporal edge graph
  (arXiv 2501.13956); (iii) the eval classes graph can win — temporal reasoning,
  knowledge updates, multi-session — are LongMemEval classes whose mechanism is
  edge valid-time + invalidate-not-accumulate; (iv) retrieval-time
  candidate-generation comes from projecting **edge `body` text into FTS +
  vector** (closing ADR cracks 1–3) plus bounded BFS over the already-landed
  `canonical_edges(from_id)/(to_id)` indexes — fused as a third RRF arm
  (the LightRAG/Zep production template, `deep-research.md:60-63`). Node-centric
  PPR (HippoRAG) remains portable *later* as a ranking pass over the same
  substrate — it is a scoring choice, not a schema choice.
  **Construction is caller-supplied first (C7):** define the graph ingest API so
  the consumer's agent (Memex/Hermes/OpenClaw-class — they have LLMs) writes
  extracted facts; an optional local-CPU extraction path is a separate gate.
- **Probability of success — with reasoning and citations (two gates):**
  - **R3a — mechanism gate** (given good extraction, does the third arm lift the
    R2 memory-class metrics?): ≈**55–65%**. For: Zep +11 pts LongMemEval over a
    full-context baseline (arXiv 2501.13956 — vendor-authored, WEAK per
    deep-research §d); HippoRAG-2 multi-hop **Recall@5** lifts (MuSiQue
    69.7→74.7, 2Wiki 76.5→90.4; arXiv 2502.14802 — real recall evidence, C8);
    the peer pattern "the graph is the differentiator" (verified,
    `…literature-benchmark.md:101-117`). Against: Mem0's own graph variant shows
    only mixed/class-specific gains over flat Mem0 (arXiv 2504.19413); all
    transfer to a discourse/transcript corpus is unmeasured ("no source gives a
    measured recall lift on FathomDB's own corpus," deep-research §d).
  - **R3b — strict-local construction gate** (can a ≤4B-class CPU LLM extract a
    good-enough graph?): ≈**40–50%**. Non-LLM extractors (REBEL) cause large
    drops; small models show extraction-quality/JSON-stability degradation
    (deep-research §b, explicitly untested); LightRAG supports local open models
    (HKUDS/LightRAG) but at sizes (Qwen3-30B-class) above a comfortable CPU
    budget, and index-time CPU throughput over long transcripts is a real cost
    even when quality holds. Under the **BYO-LLM ingest API (C7)** this gate
    moves to the consumer and FathomDB-side success ≈ **85%** (substrate +
    projection + BFS arm are conventional engineering on a signed ADR).
  - *Key failure risk:* the corpus-measured hard core is single-transcript
    summary discrimination, which graph does not address — so graph may lift R2
    temporal/multi-hop classes while leaving the headline exploratory R@K flat.
    That is acceptable *if* R2 exists to show it; fatal to justify otherwise.
- **Effect size (derived, two metrics):** on the **current exploratory R@K**:
  ~**0 to +0.05** (INFERRED — most of the 38%-at-depth-50 core is
  discrimination, not relational; graph contributes via entity-bridging on a
  minority and via the ~14% lexically-unreachable stratum). On the **R2
  memory-class end-to-end metrics**: the external CLAIMED range is **+7 to +15
  pts on temporal/multi-hop/update classes** (HippoRAG-2 +7% associative;
  Zep +11 overall — both metric-transfer-caveated, vendor-flagged). Do not
  promise the latter as Recall@K.
- **Footprint fit:** retrieval-time ✅ (BFS/RRF, no neural inference,
  `deep-research.md:72-79`); storage ✅ (additive columns, signed H3); edge-text
  embedding ✅ 1-bit-safe via the existing node projection path. ⚠️ **Index-time
  LLM**: caller-supplied = footprint-clean for FathomDB; the optional bundled
  local-CPU extractor is CPU-legal but throughput-heavy (flagged); ❌ shipping
  any API-LLM dependency inside FathomDB — rejected.
- **Cost & sequencing:** the largest item — schema additive migration + edge
  projection + BFS arm + ingest API ≈ 3–5 weeks, **plus** the R3b experiment
  (~2 weeks). Strictly **after R2** (C3: without R2 the gains are unmeasurable)
  and after R1 (the reranker also reranks the third arm's candidates).
  Traversal scope per the 0.8.1 roadmap direction (depth ≤3, hard cap 50,
  `ADR-0.8.0-graph-traversal-scope.md`).
- **Measurement/gate:** (R3a) oracle-extraction run: build the graph for the
  eval corpus with a strong offline LLM (dev-time), measure R2 class deltas —
  **GO if temporal/multi-hop/update classes improve materially with factoid
  flat**; (R3b) repeat with a local ≤4B extractor; ship the bundled path only
  if R3b ≈ R3a. Recall@K harness as regression guard, binary floor untouched.

### R4 — Whole-doc long-context dense + late chunking (research probe; the one
unexplored dense mechanism)

- **What & why:** The only dense angle the three negatives never exercised
  (C5): embed the **full document** with a long-context model
  (nomic-embed-text-v1.5, 8192-ctx, MRL — already integrated for the A/B) so
  the representation can span the whole discussion a summary query asks about;
  variant: **late chunking** (long-context encode → pool to chunks; arXiv
  2409.04701) which keeps chunk granularity with global context. Targets the
  exploratory *discrimination* subset — the stratum graph helps least.
- **Probability of success:** ≈**25–35%** that the dense diagnostic materially
  improves (exploratory dense top-50 from 37% to ≥50%). For: the long-context
  small-model literature shows ~2× hard-task recall on *long-document*
  benchmarks (granite-r2, arXiv 2508.21085 — CLAIMED, vendor-directional,
  explicitly flagged "directional" in `…literature-benchmark.md:84-89`); late
  chunking shows consistent gains without training (arXiv 2409.04701); the
  mechanism genuinely differs from the refuted knobs. Against: the same
  literature note records this claim was already **superseded once** when
  tested in chunked form (Nomic A/B); bge's QMSum-class performance is ~0.208
  (CLAIMED, `…literature-benchmark.md:67-70`); and a single whole-doc vector
  must *discriminate* among ~10.5k long transcripts sharing vocabulary — the
  exact failure mode measured at median-99. *Key risk:* global vectors blur,
  not sharpen, discrimination.
- **Effect size (derived):** if dense top-50 reaches ~55–60%, the semantic
  bucket grows from 9% toward ~15–20% and the oracle union from 0.62 toward
  ~0.70 (INFERRED bucket math); with R1 cashing the pool, fused exploratory
  R@10 gains ≈ **+0.03 to +0.08** best-case; **0** if discrimination doesn't
  improve. Costs 768-d/4× disk (~522MB weights, MEASURED-prose
  `IR-C-embedder-options-research.md:274-275`) and a full re-embed.
- **Footprint fit:** ✅ CPU (slow: ~1.4× wall vs bge per chunk, whole docs
  worse — hours-scale re-index, flagged); ✅ no-API; ⚠️ **must re-clear the
  1-bit binary floor ≥0.90 at 768-d** (the V13 gate pattern,
  `IR-C-pooling-floor-gate.json`) — nomic binary retention is unmeasured here;
  treat as a hard gate, not an assumption. ❌ ColBERT-style multi-vector
  variants of "more context" remain out (1-bit incompatible).
- **Cost & sequencing:** ~1–2 weeks, mostly compute. After R1/R2; in parallel
  with R3 if resources allow. Explicitly a **probe** — kill on a flat dense
  diagnostic, do not pre-commit fusion work.
- **Measurement/gate:** existing dense diagnostic (median rank / top-50 /
  bucket shift) → binary-floor gate → only then a fusion-harness row.

### R5 — Vector pseudo-relevance feedback (cheap opportunistic, post-R1)

- **What & why:** Average the query f32 vector with top-k *reranked* passage
  vectors and re-run the dense arm — a query-side recall lever with **no extra
  neural inference** (arXiv 2108.11044, carried STRONG by deep-research §a).
  Post-R1 ordering matters: PRF quality tracks feedback precision, and the
  reranked top-k is far cleaner than today's fused top-k.
- **Probability of success:** ≈**20–30%** material exploratory gain. For: the
  cited PRF evidence (STRONG label, ~1/20th BM25+BERT cost); feedback precision
  improves substantially post-R1 (from R@10 0.307 to ~0.40+). Against: PRF
  feeds the *dense* arm, whose representation is the measured structural
  weakness (median 99); "modest, condition-dependent" is the source's own
  characterization. *Key risk:* query drift on exactly the ambiguous
  discrimination queries that dominate.
- **Effect size (derived):** bounded by the union headroom the dense arm can
  add (+0.09 oracle at depth-50, MEASURED); realistic **+0.00 to +0.03** R@10.
- **Footprint fit:** ✅ fully (operates on existing f32 rerank vectors;
  1-bit pipeline untouched).
- **Cost & sequencing:** ~days on the harness. Strictly after R1; drop without
  regret if flat.
- **Measurement/gate:** fusion harness A/B row; binary floor untouched.

### Supporting work (not quality levers)

- **Expose `rerank_depth` + arm weights + result-depth K** as optional
  per-request params (the parked knob review's #1/#3 lean,
  `IR-C-api-surface-knobs-to-review.md:78-84`) — required by R1's opt-in
  latency posture; governed-surface + determinism rules apply.
- **Label audit of the hard core** (C2): human-review ~50 of the 596 "hard"
  exploratory queries to bound label noise — cheap, sharpens every ceiling
  number; fits the findings note's own test-label caveat
  (`IR-C-retrieval-findings.md:40-44`).

### CLOSED levers (do not revisit without new mechanism)

| Lever | Why closed | Evidence |
|---|---|---|
| **Stronger chunked dense embedder** | three independent A/Bs failed: chunk geometry (flat/negative fused), CLS pooling (99→121, worse), nomic +10.6 MTEB (99→135, worse) — structural, not capacity | MEASURED: `IR-C-retrieval-findings.md:18-33`; `IR-C-embedder-options-research.md:225-308`; BEIR anchor confirms bge runs at its documented level (MEASURED-json) |
| **BGE query prefix** | swept every geometry; tiny exact gain, hurts exploratory | `IR-C-api-surface-knobs-to-review.md:64` |
| **ColBERT / PLAID / EMVB late interaction** | multi-vector — architecturally incompatible with single-vector 1-bit Hamming; CPU-costly even at PLAID 45× | footprint ❌ (arXiv 2205.09707, 2404.02805; deep-research §a) |
| **HyDE / ReDE-RF / listwise-LLM query expansion** | generative LLM in the **query loop** — footprint ❌ unless a query-time CPU LLM is ever accepted (it is not, today) | deep-research §a (arXiv 2212.10496, 2410.21242) |
| **Large rerankers (≥400M)** | heavy; 60M tier already net-negative vs BM25 zero-shot | arXiv 2206.02873 caveat, deep-research §a |
| **Ingest/query intent classifiers** | rejected on determinism + latency; superseded by caller params | `IR-C-api-surface-knobs-to-review.md:61-65` |
| **Chasing factoid** | 0.905 ≈ lexical ceiling; vector adds ~1% | MEASURED `IR-C-retrieval-findings.md:85-104` |

---

## 3. Goal assessment — can FathomDB reach Mem0/Zep parity in-footprint?

**The comparability caveat first (load-bearing):** every published peer number
(Mem0 LoCoMo 92.5 self-reported; Zep vs Mem0 LongMemEval 63.8 vs 49.0; the
disputed LoCoMo 84→58.44→75.14) is **end-to-end answer accuracy on a different
corpus, vendor-authored and vendor-contested**
(`IR-C-bge-small-literature-benchmark.md:192-212`). FathomDB's 0.307/0.905 are
**first-stage Recall@K on its own corpus**. No statement of the form "N points
behind Zep" is currently meaningful; parity becomes a real claim only on the
R2 identical-answerer eval, and any single peer benchmark number must be
treated as contested.

**Structural position.** Peers do not win with bigger embedders — same small
model class, winning on architecture: hybrid + rerank + memory graph
(verified pattern, `…literature-benchmark.md:101-117`). FathomDB already has
the hybrid (BM25/FTS5 + 1-bit dense + weighted RRF) and the graph substrate
(signed, neutral, edge-enrichment reserved). The architectural gap to peers is
exactly two components — **reranking (R1)** and an **exploited graph (R3)** —
plus the eval to prove it (R2). The structural *disadvantage* is one component:
peers use API LLMs for memory construction; FathomDB's answer is the BYO-LLM
ingest API (C7) with an optional local extractor (R3b).

**Aggregate outlook** (judgments composed from the per-initiative probabilities
above; "parity" = within noise of the peer-class baseline on the R2
identical-answerer eval):

- **Best case (~25–30%):** R1 lands at the high end (exploratory R@10 ≈
  0.45–0.51 at depth 200), R3a+R3b both pass, R4 adds a few points → FathomDB
  beats local Mem0-OSS overall and matches Zep-class systems on
  temporal/update/multi-hop classes with a fully-local stack — a genuinely
  differentiated result (nobody else does this with zero API calls + 1-bit
  vectors).
- **Likely (~50%):** R1 lands mid-range (R@10 ≈ 0.40–0.45); R3a passes with
  caller-supplied extraction but R3b is marginal → **parity-or-better vs
  Mem0-class on most classes under the identical-answerer eval; behind
  Zep-class on temporal/graph-heavy classes when restricted to strictly-local
  construction**, at-parity when the consumer supplies the ingest LLM. Honest
  net: P(demonstrable "as-good-or-better than Mem0" on R2) ≈ **0.55–0.65**;
  P(also matching Zep-class incl. its graph) ≈ **0.30–0.40 strictly-local**,
  ≈ **0.45–0.55 with BYO-LLM ingest**.
- **Floor (~85% at least this):** R1 alone clears its gate → exploratory R@10
  ≥ ~0.37, factoid held at 0.90, parity on factoid/simple-retrieval classes,
  measurable deficit on temporal/multi-hop — still the strongest result in the
  zero-API/on-device class, but short of the stated goal.

**Biggest single risk:** the **graph-construction dependency** — Zep-parity on
the classes that define Zep requires a graph, every credible graph needs an LLM
at index time, and the strictly-in-footprint version of that (R3b, local ≤4B
CPU extraction) is the lowest-probability link (~40–50%) with real throughput
cost. The C7 BYO-LLM ingest API is the designed mitigation, but it makes part
of the parity claim conditional on the consumer's LLM. (Second-order risk: the
hard core is partly label noise — C2 — which would mean some headroom we're
chasing doesn't exist; the cheap label audit bounds this.)

**The one measurement that most reduces uncertainty:** **R2 — the
identical-answerer end-to-end eval with a local Mem0-OSS baseline.** It (a)
replaces incomparable vendor numbers with a real denominator for "parity," (b)
reveals whether reranked-hybrid alone already reaches Mem0-class (collapsing
the roadmap's tail if yes), and (c) is the only instrument that can see the
graph's class-specific gains before the largest investment is committed.

---

*Every effect size above is derived from the dossier's verified headroom
numbers (R@50 0.520/0.533; oracle union 0.62 chunked; found@1000 0.859/0.985;
semantic 9%; hard 38%-at-depth-50 → ~14%-at-depth-1000) or carries an external
citation with the deep-research report's own STRONG/WEAK grade. INFERRED
interpolations (found@100/200/500) are flagged and gated on R0.*
