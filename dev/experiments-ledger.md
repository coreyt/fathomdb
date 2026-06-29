# FathomDB experiments ledger — distilled results of record

> **Purpose.** The single durable record of *what every FathomDB experiment found* —
> so the raw per-run artifacts (`dev/plans/runs/*-output.json`, codex review logs,
> checkpoints) can be pruned without losing the result. One entry per experiment:
> hypothesis · design/prereg · N & power · numbers+CI · verdict · what closed it · $ cost ·
> sources. Built at the first ledger-prune (`scripts/repo-prune/prompts/prune-docs.md`); every number was verified
> against its source file at distillation time.
>
> **Raw-artifact recovery.** Files later `git rm`'d by the prune are recoverable from git
> history at/below the pre-prune commit (`git log -- <path>`; baseline snapshot SHA
> `25541d88`). **`dev/research/` is untracked/git-ignored** — its results live ONLY here;
> see the Deferred section.
>
> **Status legend:** GO / NO-GO / SPLIT / PARITY / RESOLVED / CLOSED_AS_IS / artifact
> (= a measured-false attribution). Numbers are byte-verified from sources; estimates are
> labelled.

---

## 0.6.x

### 0.6.1 — AC-012 canonical text-query latency re-measure (Pack-7 trigger)

- **Question:** Does the FTS5 MATCH path meet the 0.6.0 budget (p50 ≤ 20 / p99 ≤ 150 ms) at canonical N=1M?
- **N & power:** N=1,000,000 rows, 1000 samples/percentile; single canonical run (4-core EPYC).
- **Result:** p50 **140.95 ms** (7.05×), p99 **458 ms** (3.05×). RED.
- **Verdict:** **RED** → Pack-7 un-defer fires; 0.6.1 BUMP blocked; budget revision escalated to 0.7.0 (patch-release contract forbids ADR change in 0.6.x).
- **Closed by:** `ADR-0.7.0-text-query-latency-gates-revised.md` (p50 ≤ 50 / p99 ≤ 200).
- **$:** $0. **Sources:** `runs/0.6.1-AC012-measure-output.json`.

## 0.7.0

### 0.7.0 — AC-012/AC-020 perf-lever sweep (W1–W5: PRAGMA + top-K LIMIT + PCACHE2)

- **Question:** Which engine levers close revised AC-012 (50/200 ms) and AC-020 concurrency (≥5.33×) at N=1M?
- **N & power:** AC-012 N=1M/1000 samples; AC-020 8 thr × 50 rounds × 4 queries; dev-box pre-screen + 4-core EPYC CI, reruns r1–r5.
- **Result:** AC-012 p50 162→**41 ms** (full PCACHE2+LIMIT stack, dev-box; p99 95–97); canonical-CI W5.3 p50 66 / p99 221. AC-020 speedup ~2.7× → **×7.0–7.9** dev-box, but canonical 4-core only 3.07× (W5.3) — no CI combo cleared 5.33×. (Aggregate table rows all marked "PENDING".)
- **Verdict:** PCACHE2+LIMIT dominant; AC-012 revised budget met dev-box / marginal canonical; AC-020 met only on high-core dev-box.
- **Closed by:** budgets folded into `ADR-0.7.0-text-query-latency-gates-revised.md`.
- **$:** $0. **Sources:** `runs/0.7.0-perf-experiments-results.md`, `…W5.3/W1.1-canonical-output.json`, `notes/0.7.0-vector-cost-research.md`.

### 0.7.0 — AC-013 vector retrieval: f32-brute RED → binary-quant + f32-rerank (recall floor 0.90)

- **Question:** f32 brute vec0 is ~25–40× over the 80/300 ms envelope at N=1M; does sign-bit quant + K-candidate f32 rerank close it while holding recall@10 ≥ 0.90?
- **N & power:** N=1M, dim 768, 1000 samples (4-core EPYC 7763).
- **Result:** pre-quant f32-brute p50 **2048 ms** / p99 **2327 ms** at N=1M (≈ memory-read ceiling). Decision: ship `bit[768]` sibling + two-phase query (K shipped at **192**), retain f32 for rerank/GT. Recall not measured at real scale here (→ EU-7/0.7.2).
- **Verdict:** **GO** on binary-quant + rerank (data-encoding lever, not a 2nd architectural lever).
- **Closed by:** recall validated 0.7.1→0.7.2 (ANN 0.937, floor kept 0.90); ADR later amended by 0.8.0 Slice-40/GA-3. (Related bug: `engine.write` batch→1 vec0 row/call, `notes/0.7.0-engine-batch-vec0-collapse.md`.)
- **$:** $0. **Sources:** `adr/ADR-0.7.0-vector-binary-quant.md`, `notes/0.7.0-vector-cost-research.md`, `design/0.7.0-vector-quant-pack1.md`, `runs/0.7.0-PERF-EXP-W4.1-ac013-canonical-output.json`.

### 0.7.0 — AC-020 concurrency architectural lever (PCACHE2 vs WAL2 vs R/W split vs vendor-swap)

- **Question:** Read-path speedup 3.530× (need ≥5.33×) can't be closed by PRAGMA; which single architectural lever closes it?
- **Result:** Recommended **PCACHE2** (smallest blast radius, contingent on DIAG confirming H1 pcache1-mutex). Others rejected (WAL2 recovery risk; R/W split snapshot redesign; vendor-swap last resort). **No closure-verdict artifact located; ADR still `status: draft, HITL-required`** — final canonical disposition unverifiable from run files.
- **Verdict:** PCACHE2 selected; closure not confirmed in artifacts.
- **$:** $0. **Sources:** `adr/ADR-0.7.0-ac020-architectural-lever.md`, `runs/0.7.0-perf-experiments-results.md`. *(gap: no AC-020 closure JSON)*

## 0.7.1

### 0.7.1 — EU-7 real-corpus ANN recall@10 (real bge-small, N=7667)

- **Question:** Does locked config (bge-small + mean-centering + K=192) clear the 0.90 floor on the real corpus?
- **N & power:** N=7,667 real docs (NOT canonical 1M), 100 queries, 1000 bootstrap.
- **Result:** recall@10 **0.828** (CI 0.796–0.858) — **entire CI below floor**. AC-013 latency PASS (p50 25/p99 40); AC-019 PASS. Surfaced 3 engine defects: A (no projection-worker panic guard), B (mean-centering inert on prod write path), C (no 512-token truncation → long-doc stall).
- **Verdict:** **RED, surfaced to HITL** (no silent floor re-pin). Findings A/B/C fixed in engine slice EU-5f.
- **Closed by:** 0.7.2 PR-2c — the 0.828 root-caused as a **measurement artifact**; corrected ANN recall@10 = 0.937, floor kept 0.90.
- **$:** $0. **Sources:** `runs/0.7.1-EU-7-findings.md`, `runs/0.7.1-EU-7-{output,measurements}.json`.

## 0.7.2

### 0.7.2 — PR-2a mean-centering recall decomposition (offline)

- **Question:** Is EU-7's 0.828 driven by the non-representative first-256-doc (single-source) mean pin?
- **Result:** offline M0 first-256 **0.842** (reproduces real 0.828); M1 full-corpus 0.951; M2 representative-256 **0.945–0.951**; M3 no-centering 0.918. Attribution: mean strategy −10.9pp (dominant). Bad single-source mean is worse than no centering.
- **Verdict:** **GO (offline)** — recommend representative-sample pin.
- **Closed by:** **OVERTURNED by PR-2c** — the offline 0.945 doesn't transfer; on the real engine the mean is a non-lever (+1.9pp).
- **$:** $0. **Sources:** `runs/0.7.2-PR-2a-recall-investigation.md`.

### 0.7.2 — PR-2c recall root-cause (real engine): the EU-7 gap is a measurement artifact

- **Question:** Does the mean fix recover recall on the real engine, or is the gap something else?
- **N & power:** N=7,667, 100 synthetic queries.
- **Result:** real re-measure 0.844; forced full mean **0.847** (+1.9pp → mean NOT the driver). candle vs HF embeddings **bit-identical**. Decomposition: standard-ANN method **0.944**; −4.4pp from excluding self-retrieving target after top-10; −3.9pp residual (RNG + sqlite-vec noise) → real 0.847. Corrected the standard ANN way: recall@10 ≈ **0.937**.
- **Verdict:** EU-7's 0.828 is a **conservative-harness artifact (~6pp)**, not an engine/mean/embedder defect. Floor needs no change; harness does.
- **$:** $0. **Sources:** `runs/0.7.2-PR-2c-recall-rootcause.md`.

### 0.7.2 — EU-8 IR-relevance recall (full corpus, 301 labeled queries) → embedder ceiling ≈0.571

- **Question:** Beyond ANN fidelity, what is the embedder's IR-relevance recall, and is quant or the embedder the bottleneck?
- **N & power:** 7,667 docs + 200 chains, **301 labeled queries**, 1000 bootstrap.
- **Result:** IR **recall@10 = 0.5714** (CI 0.530–0.614), precision@10 0.162, MRR 0.686, NDCG 0.561; zero-hit 66/301. By relation: action_from 0.811 … **contradicts 0.288** (cosine can't capture negation). Companion ANN recall same run: **0.937** (CI 0.913–0.957).
- **Verdict:** **Quantization story CLOSED** (ANN 0.937 ≫ IR 0.571 by +37pp → K/ANN tuning buys ≈0 user value). **Embedder/relevance story OPENED** (levers = better embedder or graph). 0.571 = ceiling, not a gate.
- **$:** $0. **Sources:** `runs/0.7.2-EU-8-ir-recall-results.md`, `notes/0.7.2-EU-8-ir-recall-design.md`.

### 0.7.2 — PR-2bc keep/shelve decision (recall-floor reframe)

- **Result:** Reaffirms ANN 0.937 (CI 0.913–0.957). PR-2c is recall-NEGATIVE for small workspaces (~−5pp).
- **Verdict (HITL 2026-05-31):** **PR-2c SHELVE**; **PR-2b KEEP** recompute core + `doctor recompute-mean`, carve out auto-drift detector + cap → 0.8.x; **LAND** ANN + EU-8 harness; **floor kept 0.90**, ADR amended to cite 0.937 as ANN/quantization fidelity (not IR), 0.571 recorded as separate IR ceiling.
- **$:** $0. **Sources:** `runs/0.7.2-PR-2bc-decision.md`, `adr/ADR-0.7.0-vector-binary-quant.md`.

### 0.7.2 — PR-3 canonical latency/recall → tiered (10k/100k/1M) budget

- **N & power:** synthetic N=10k/100k/1M (dim 384 & 768) + real bge anchor N=7,667; 1000 samples.
- **Result:** AC-013 10k tier **MET** (real bge p50 36/p99 49); 100k MISS (1.8×); 1M ~1,500 ms (MISS, ANN-index-gated). Scaling ≈ O(N) ~1.5 ms/1000 docs. AC-013b recall anchor **0.937** (floor PASS). AC-019 10k MET; synthetic AC-019 report-only.
- **Verdict:** **Tiered budget LOCKED** — 10k is the binding 0.x/1.x gate; 100k/1M post-1.0 (need ANN index). AC-013 asserts only at n ≤ 10,000.
- **$:** $0. **Sources:** `runs/0.7.2-PR-3-perf-data.md`, `adr/ADR-0.7.0-text-query-latency-gates-revised.md`.

## 0.8.0

### 0.8.0 — B2 FTS5 tokenizer latency experiment

- **Question:** Did the Slice-5 tokenizer upgrade cause the Slice-40 AC-012 regression?
- **N & power:** real `ac_012` @10k/100k; 6-config sweep ×1000 samples (box-specific absolutes).
- **Result:** @10k p50 <1/p99 4 ms (PASS); @100k p50 21 ms (1 ms over budget, p99 within). All 6 tokenizers within noise; engine A/B: porter 21 vs unicode61 20 ms (tokenizer **exonerated**). Cost driver = result-set size (1-token 3,212 rows). FTS-quality: porter 8/8 vs unicode61 5/8.
- **Verdict:** Slice-40 attribution **measured-false**; latency is O(N) corpus-scaling → recommend **tier AC-012**; keep porter. (HITL fork pending.)
- **$:** $0. **Sources:** `notes/0.8.0-fts5-tokenizer-latency-experiment.md`, `runs/0.8.0-slice-6-tokenizer-experiment-*.md`.

### 0.8.0 — Graph-model resolution (Slice 32; fact-on-edge, logical_id-alone)

- **Result:** Verdict (B)+(C): edge identity = `logical_id`-alone (signed Slice 31). GraphRAG and Graphiti ontologies reduce to the same substrate primitives; the gap is exactly 3 additive enrichments (no edge `body`, no valid-time/confidence, no per-fact vector) — not a reshape. No router workload in ≤100k–1M needs two native graph engines over one indexed SQLite substrate.
- **Verdict:** **0.8.0 ships binary substrate unchanged**; only H3 (reserve edge-enrichment columns) has a substrate-now footprint.
- **Closed by:** enrichment columns landed as G11 step-14.
- **$:** $0. **Sources:** `runs/0.8.0-graph-model-resolution-20260605T140000Z.md`.

### 0.8.0 — GA-1 OLD-vs-NEW corpus A/B (recall-floor diagnosis)

- **Result:** **PREREQUISITE FAILED → STOP.** No OLD corpus distinct from NEW (8 raw files byte-identical between the 0.937 anchor and the 0.8710 slice-40 run; corpus gitignored, no snapshot in history). eu7 harness byte-identical v0.7.2→slice-40. The real delta = **11 lib.rs retrieval-path commits** (notably G9 unconditional RRF fusion measured by pure-vector-f32 GT) + tokenizer upgrade + structured SearchHit.
- **Verdict:** Classification **(b)** code/measurement-path change, NOT corpus; likely intended RRF-fusion behavior, not a fidelity defect. Overturns the B-1 corpus premise; corpus pinning won't recover 0.937.
- **$:** $0. **Sources:** `runs/GA-1-corpus-ab-20260608T012503Z.md`.

### 0.8.0 — Recall-eval-framework assessment (fidelity vs relevance axes)

- **Result:** FathomDB measures two recall axes — **eu7 fidelity** (bit-KNN+rerank vs exact-f32; 0.937→0.8710; floor 0.90; GA gate AC-075) and **eu8 IR-relevance** (vs qrels; ceiling ~0.571; report-only) — ~37pp apart by design. No chunker, no real reranker (identity stub), no graph yet, no fact-level gold. Absolute thresholds (95/98/90) would falsely brand FathomDB "not valuable" by ignoring the 0.571 embedder ceiling.
- **Verdict:** Keep eu7/AC-075 as the un-weakened fidelity gate (pin to versioned corpus); promote eu8 to a tracked signal; build fact-level gold + pooling + real reranker in 0.8.1.
- **$:** $0. **Sources:** `notes/recall-eval-framework-assessment-20260607T174821Z.md`.

## 0.8.1

### 0.8.1 — Slice-30 LME baseline + graph-vs-Mem0/Zep diagnosis

- **N & power:** 500 Q over 19,195 sessions; only **200 (~1%)** ELPS-extracted.
- **Result:** Per-class Recall@10 FathomDB vs NaiveRAG: factoid 0.442 vs 0.538 (−0.096), temporal 0.090 vs 0.150, knowledge_update 0.359 vs 0.423, multi_session 0.128 vs 0.113 (+0.015). Graph-arm effect ≈0 every class. Model **NOT muddled**; 3 separable causes: (A) read-path `SearchHit.id = write_cursor` not `source_id` → graph hits invisible to scorer (`lib.rs:5459`); (B) ~1% coverage starves the arm; (C) base retrieval trails BM25 ~10pp on factoid (largest lever).
- **Verdict:** "Clear model, incomplete realization + boundary bug + coverage confound + base-retrieval deficit." `use_graph_arm` stays false; HITL go/no-go blocked.
- **Closed by:** the graph-arm NO-GO below.
- **$:** not quantified. **Sources:** `design/fathomdb-graph-vs-mem0-zep-and-longmemeval-diagnosis.md`.

### 0.8.1 — Graph BFS arm vs BM25 (LongMemEval, n=40)

- **N & power:** 40 Q (10/class), 1,907 sessions, 28,883 entities, 38,021 edges; per-class MDE ~15pp (direction-only); ON-vs-OFF paired null robust.
- **Result:** Pooled R@10 naive_bm25 0.70, fts_only **0.80**, graph_OFF 0.65, graph_ON 0.65 → **graph_ON − graph_OFF = 0.00**. Arm DEGRADES its target (multi_session 0.30→0.10). Entity co-mingling in FTS costs −0.15 (length-norm "Verboseness Fission"). Literature corroborates ("graph beats BM25 ~46%" REFUTED 0-3 for raw BFS).
- **Verdict:** **NO-GO (robust NEGATIVE) → PIVOT** to index-key enrichment (HITL 2026-06-16). Graph ships as substrate, not a recall win. Tag `0.8.1-beat-bm25-pivot-2`.
- **$:** $0 (on-prem Qwen3.6-27B, 2× RTX-3090). **Sources:** `runs/0.8.1-beat-bm25-report.md`, `design/0.8.1-graph-experiment-plan.md`.

### 0.8.1 — Dense/fused base retrieval vs BM25 (LongMemEval, n=160)

- **N & power:** n=160 paired; pooled MDE ~11pp.
- **Result:** R@10 naive_bm25 0.625, fts_only 0.562, fused 0.606 → fused − BM25 **−0.019** (tie); fused − FTS +0.044 (dense adds over lexical). MRR/nDCG still trail BM25. End-to-end answer accuracy (gemini-3.1-pro / gemini-3.5-flash judge): bm25 0.406, fused 0.388 → −0.019 (mirrors recall).
- **Verdict:** Dense/fused **ties BM25, doesn't beat it** on either axis; machinery buys ≈0 over strong lexical on this corpus.
- **$:** priced (amount not quantified). **Sources:** `runs/0.8.1-beat-bm25-report.md §3/4`, `runs/0.8.1-p0a-*.json`.

### 0.8.1 — R6 index-key enrichment + BM25 b-tuning (LongMemEval, n=40)

- **Result:** Pooled R@10 bm25 0.70, bm25_enriched 0.75, fts_only **0.80 (best)**, fts_enriched 0.775, fts_placebo 0.70. Placebo proves a real −0.10 FTS length penalty + genuine +0.075 content value. Lower b lifts recall; enrichment helps at every b — but best enriched 0.775 **still < FTS 0.80**.
- **Verdict:** Enrichment does NOT beat plain FTS. **"Beat BM25" CONCLUDED** (3 levers, none decisively beats lexical at n=40). One deferred lever: engine-side tunable/lower-`b` FTS5 ranking.
- **$:** $0 (cached graphs). **Sources:** `runs/0.8.1-beat-bm25-report.md Addendum`, `runs/0.8.1-R6-*.json`.

### 0.8.1 (IR-C) — R0 candidate-recall CDF + cross-encoder latency

- **N & power:** 10,506 docs; exact_fact n=2,888, exploratory n=1,584; CE latency over 1,000 pairs.
- **Result:** rrf_fused found@K — exact_fact 0.950@50→0.984@1000; exploratory 0.510@50→0.865@1000. **Dense plateaus at K=200**. oracle_union@200: 0.973/0.764. CE latency: TinyBERT-L-2 p50 **1.54 ms/pair** (308 ms @K=200, fits budget); MiniLM-L12 16.82 ms (3,364 ms @K=200, exceeds).
- **Verdict:** **K=200** recommended rerank depth; **TinyBERT-L-2** the only budget-compatible reranker.
- **$:** $0. **Sources:** `runs/IR-C-r0-findings.md`, `runs/IR-C-recall-cdf.json`.

### 0.8.1 (IR-C) — R2 end-to-end parity eval (Slice 25)

- **Result:** **DATA-LIMITED (retrieval-only).** Evidence Recall@10: factoid FathomDB 0.8999 vs naive 0.8982 (+0.0017); exploratory 0.327 vs 0.356 (−0.029). Memory classes / Mem0-OSS arm / answerer accuracy all **null** (no answerer LLM, no local Mem0, no memory-class gold). FathomDB ran **lexical-only** → lexical-vs-lexical near-parity by construction.
- **Verdict:** Slice-30 (R3) graph go/no-go **data-limited → HITL**; MUST NOT flip `use_graph_arm`. Durable deliverable = the harness.
- **$:** $0. **Sources:** `runs/IR-C-r2-eval-results.md`, `runs/0.8.1-slice-25-r2-run.json`.

### 0.8.1 (IR-C) — Dense diagnostics (chunking / pooling / model sweep)

- **Result:** Exploratory is the hard class — lexical median gold rank 26 (idf_overlap ~0.70 = discrimination, not vocab); dense median rank **99** (worse than BM25). 38% of exploratory "hard" (neither arm by rank 50). Three levers FAILED on exploratory: chunking (flat/neg), CLS pooling (99→121; REFUTED pooling-bug; cleared 1-bit floor 0.944), stronger model nomic-v1.5 (99→135, worse, 2×/4× cost).
- **Verdict:** **CLOSED** — ship current default (bge-small/Mean/whole-doc/3:1/k=30); exploratory weakness is **structural** for single-vector dense over discourse queries. Parked: long-context embed, late chunking, multi-vector/ColBERT, real reranker, HyDE.
- **$:** $0. **Sources:** `runs/IR-C-retrieval-findings.md`.

### 0.8.1 — Embedder GPU + cross-vendor portability (decision record, not a benchmark)

- **Result:** (1) GPU via `resolve_device()` seam (`FATHOMDB_EMBED_DEVICE`), default build stays CPU; est. ~100–300× CPU (not measured). (2) **candle 0.10 supports only CPU/CUDA/Metal — no ROCm/Vulkan**; AMD/Intel reachable only via new `impl Embedder` (ONNX-Runtime `OrtBgeEmbedder` rec.). (3) Vector-equivalence self-check needed before advertising portable DBs (`EmbedderIdentity` catches dim/model, NOT numeric divergence).
- **Verdict:** GPU device seam ships 0.8.1 (opt-in); vector-equivalence guard = 0.8.2 TODO.
- **$:** $0. **Sources:** `design/0.8.1-embedder-gpu-and-portability.md`.

## 0.8.2

### 0.8.2 — M1 comparator-selection diagnostic (bridge-vs-answer, $0)

- **Result:** Complete-bridge retrieval dominates answering: all bridges in top-10 → F1 0.510 vs any missing 0.068 (+0.442, 7×). Conditional on all bridges present, fused-RRF answers best (0.552), passage_dense worst (0.464).
- **Verdict:** Comparator = **fused-RRF** (overturned dense-as-comparator); MATERIAL_F1_LIFT confirmed 0.04.
- **$:** $0. **Sources:** `runs/0.8.2-m1-bridge-vs-answer-diagnostic.md`.

### 0.8.2 — M1 multi-hop graph-arm answer-accuracy (ppr_fusion vs fused-RRF)

- **N & power:** 300 graph-covered MuSiQue-Answerable Q {2h:156,3h:94,4h:50} → 144 ≥3-hop; power_ok=False (wants 1165).
- **Result:** valid gpt-5.4 run (completeness 1.0): pooled ≥3-hop **ΔF1 = −0.0405, CI [−0.1158, +0.0311]** (CI upper +0.031 < +0.04 materiality); ΔEM −0.0347. Per-hop ΔF1 uniformly negative; trend slope −0.0128 (n.s.). 5-arm F1: passage_dense 0.487 > fused 0.450 > fused_rerank 0.415 > ppr_fusion 0.410 > bm25 0.370. Direction reader-invariant. *(gemini-3.1-pro priced pass INVALID — completeness 0.8087, 429-deflated.)*
- **Verdict:** **NO-GO (robust)**, no stage 2 — graph adds ≈0. Redirect 0.8.3 to enrichment; dense (not graph) held the multi-hop signal.
- **$:** valid gpt-5.4 **$2.5032**; invalid gemini $5.6995 (slice ~$8.20/$10 cap). **Sources:** `runs/0.8.2-m1-FINDINGS.md`, `…-report-gpt54.md`, `…-verdict-gpt54.json`, `design/0.8.2-m1-multihop-harness.md`.

### 0.8.2 — bge CLS-vs-Mean pooling engine defect (flagged, not measured)

- **Result:** `CandleBgeEmbedder` defaults `Pooling::Mean` but bge-small-en-v1.5 was trained CLS-pooled. Engine ships Mean (degraded); M1 harness uses CLS — M1 absolutes in CLS space (internally consistent, not directly comparable to prod Mean-space); relative arm ordering valid.
- **Verdict:** Flagged, NOT fixed (proper fix = re-embed all vectors + embedder-identity ADR + eu7 re-measure). *(0.8.3 eu7 bisect later confirmed the CLS option commit was NOT the eu7 regression cause.)*
- **$:** $0. **Sources:** `notes/0.8.2-bge-cls-mean-engine-bug.md`.

## 0.8.3 (Mem0-parity track)

### 0.8.3 — D0b Mem0 competitor-parity gap (n=606)

- **N & power:** 606 (factoid 156, ku 150, multi_session 150, temporal 150); **every class underpowered** (MDE 0.08–0.14 ≫ ε=0.05).
- **Result:** Accuracy ΔFathomDB−Mem0 (prod α=0.3): factoid **−0.237**, ku **−0.273**, multi_session **−0.200**, temporal −0.033 (tie). Recall deltas smaller → accuracy gap ≫ recall gap. FathomDB also trails naive BM25.
- **Verdict:** `decide_083 = NOT_REACHED` (`blocked_by=eu7` 0.896<0.90 AND underpowered). Large gap, not near-parity. Embedder ruled out (15a).
- **$:** **$10.75** (gpt-5.4, 1818 calls). **Sources:** `runs/0.8.3-d0b-findings-observed-by-s15a-20260622.md`, `runs/0.8.3-d0b-parity-n606.json`, `design/0.8.3-mem0-parity.md`.

### 0.8.3 — Gap-decomposition: retrieval-precision is the cause (n=606)

- **N & power:** 606; **mechanically INCONCLUSIVE** (fit_coverage 0.7195 < 0.80; pooled MDE ~0.067 > 0.05).
- **Result:** RETRIEVAL component `acc_oracle_raw − acc_fathomdb` = **+0.392 [0.346, 0.436]** pooled — positive, largest, consistent 4/4 classes. oracle_raw ≈ Mem0 + 0.19 → perfect retrieval puts FathomDB above Mem0. DISTILLED_FORM −0.362 is a lossy-distiller (gpt-5-nano) artifact, not a clean form signal.
- **Verdict:** Cause = **retrieval PRECISION, not formation** (overturns the formation hypothesis). Parity reachable in-footprint via precision levers. Direction robust despite INCONCLUSIVE label.
- **$:** **$15.95** (/$30 cap). **Sources:** `runs/0.8.3-gap-decomposition-report.md`, `runs/0.8.3-gap-decomposition-n606.json`, `design/0.8.3-gap-decomposition-probe.md`.

### 0.8.3 — eu7 0.937→0.896 fidelity-regression bisect (offline, $0)

- **Result:** anchors 0.937 (CI 0.913–0.957) vs 0.896 (CI 0.864–0.925). **Case B (CLS/embedder) RULED OUT** (embedder src byte-identical; CLS option post-dates 0.896 & defaults Mean). Case C (corpus) ruled out. Within Case A: stored bits byte-identical, KNN ranking-invariant, rerank unchanged → leading cause = **measurement-SUT change** (`engine.search()` → `vector_stage_only` B-1 seam), no genuine fidelity collapse.
- **Verdict:** **Case A (vector-path/SUT), NOT CLS.** Reset baseline 0.896 (passes floor under one-sided GA-3 gate, 0.925≥0.90). Residual quant-path fork (whitening→K>192→2-bit) if fresh eu7 < 0.90 after re-embed.
- **$:** $0. **Sources:** `runs/0.8.3-eu7-bisect-report.md`, `runs/0.8.3-eu7-bisect.json`.

### 0.8.3 — Slice-15a embedder-ceiling probe (NO-SWAP)

- **Result:** base bge-small eu8 0.3994 / hard@10 0.0194. Candidates: bge-base proj_eu7 0.7855 (FAIL); e5-base-v2 0.8960 (FAIL); nomic 0.9317 but not candle/CPU (FAIL); gte-base errored. Stronger embedder lifts eu8 ~+0.05 but nothing on the hard subset.
- **Verdict:** **NO SWAP** — keep CLS-corrected bge-small. Dense/embedder axis can't close the ~20pp gap (granularity-bound); recall gap needs a non-embedder lever.
- **$:** $0. **Sources:** `runs/0.8.3-s15a-report.md`, `runs/0.8.3-s15a-embedder.json`, `design/0.8.3-slice-15a-embedder-probe.md`.

### 0.8.3 — CE-rerank α-lever offline tuning sweep ($0, n=606)

- **Result:** at pool_n=50, raising α concentrates gold into top ranks with ~flat Recall@10: α=0.3 (prod) MRR 0.347, r@1 0.036, r@10 0.548 → α=1.0 MRR **0.589**, r@1 **0.140 (×3.9)**, r@10 0.498. Best balanced **α=1.0, pool_n=10**: MRR 0.587, r@10 0.540 (≈ prod), full-gold-rank ~6.97 (vs ~20). (Recall/MRR are a $0 proxy.)
- **Verdict:** α is the dominant infra-free lever; prod 0.3 captures ~⅓ of available top-rank lift. Recommend priced α=1.0/pool_n=10 arm.
- **$:** $0. **Sources:** `runs/0.8.3-rerank-tune-FINDINGS.md`, `runs/0.8.3-rerank-tune.json`.

### 0.8.3 — CE-rerank accuracy arm, production α=0.3 (n=606, citable)

- **N & power:** 606, completeness 1.0; pooled **powered** (MDE 0.0322 ≤ 0.05); per-class underpowered.
- **Result:** fathomdb 0.137, **fathomdb_reranked 0.1865**, mem0_oss 0.323. Pooled margin **+0.0495 [+0.0281, +0.0726]** → lever **PASS**; gap_to_mem0_closed = 0.265 (~27% < 0.50 bar).
- **Verdict:** **lever PASS, GO=False (marginal NO-GO on parity)** — single CE stage at α=0.3 recovers ~27% of the gap (vs +0.39 oracle ceiling).
- **$:** **+$5.21** (→ $23.33/$30). **Sources:** `runs/0.8.3-rerank-accuracy-n606-VERDICT.md`, `runs/0.8.3-rerank-accuracy-n606.json`.

### 0.8.3 — CE-rerank accuracy arm, tuned α=1.0/pool_n=10 reblend (354/606, ABORTED_INCOMPLETE)

- **N & power:** halted 354/606 (completeness 0.5842) by OpenAI `insufficient_quota` (usage-limit, not budget); pooled MDE 0.078 just-underpowered.
- **Result:** on 354 answered cells: fathomdb 0.137, **fathomdb_reranked 0.525**, mem0_oss 0.323 (**+0.21 surpass on answered cells**). Pooled reranked−baseline **+0.3249 [+0.2740, +0.3814]**; all per-class margins positive. Paired α=0.3→1.0 on identical cells **+0.241** (not a subset artifact). CAVEAT: 42% unanswered skew to retrieval-FAILURE cells → decomposition-adjusted full-606 estimate **~0.33–0.35 vs Mem0 0.323 = marginal PARITY** (estimate, not measured).
- **Verdict:** `ABORTED_INCOMPLETE` (citable=false); direction rock-solid (CI_lo ≫ 0). Provisional parity-or-better; surpass needs RECALL (0.8.4).
- **$:** spent **$38.16/$50** (incl. ~$3.6 wasted). **Sources:** `runs/0.8.3-rerank-accuracy-reblend-a1-INTERIM-VERDICT.md`, `runs/0.8.3-rerank-accuracy-reblend-a1-n606.json`.

### 0.8.3 — Mem0-parity resolution (Slice 30, CLOSED_AS_IS)

- **Result:** synthesis fathomdb 0.137 → prod rerank 0.186 → tuned rerank (α=1.0/pool_n=10) **~0.33–0.35 (est)** vs mem0_oss 0.323. By regime: retrieval-works (354) tuned 0.530 vs Mem0 0.395 (beats); retrieval-fails (252) Mem0 0.222 vs ~0.07 (rerank can't help; claude-sonnet cross-check 0.091 confirms retrieval-bound) → two regimes ~cancel to parity.
- **Verdict:** **PARITY MET (provisional, marginally above Mem0)** via in-footprint CE-rerank α=1.0/pool_n=10. **CLOSED_AS_IS (HITL 2026-06-23)** — accuracy retrieval-gated; substantive result is the precision finding; completion blocked by OpenAI usage-limit. Steward rec: SHIP-AT-PARITY; surpass = RECALL (0.8.4). Engine still hardcodes ALPHA=0.3 → exposed as opt-in knob in **0.8.5 EXP-0**.
- **$:** cumulative track ≈ **$38.16/$50**. **Sources:** `runs/0.8.3-mem0-parity-VERDICT.md`, `runs/0.8.3-resolution-verdict.{md,json}`.

## 0.8.4 (GraphRAG-parity track)

### 0.8.4 — Cross-family premise pilot (self-preference overturn; Qwen answerer)

- **Result:** same-family Qwen judge gave ~0.750; cross-family claude-haiku judge collapsed it to 0.25–0.44 (graphrag_mapreduce vs long_context 0.250/0.237/0.388; vs vector_rag 0.438/0.425/0.438).
- **Verdict:** NOT_REACHED (underpowered); self-preference false-positive demonstrated; leaned AGAINST funding S1.
- **Closed by:** re-characterized by the powered pilot below.
- **$:** ~$0.2–0.3. **Sources:** `runs/0.8.4-xfamily-pilot-RESULT.md`.

### 0.8.4 — Powered cross-family pilot (answerer is the confound; gpt-5.4 answerer)

- **Result:** vs long_context 0.717/0.554/0.571; vs vector_rag 0.825/0.675/0.625 (comp-vs-vector_rag CI lower 0.617 > 0.5). All six win-rates ≥ 0.5. Same arm: Qwen lost 0.25–0.44, gpt-5.4 won 0.55–0.83 → **answerer quality is the dominant confound**.
- **Verdict:** NOT_REACHED (underpowered, NOT below-parity); lean FLIPS to supporting the S1 build.
- **$:** ~$2–3. **Sources:** `runs/0.8.4-xfamily-pilot-powered-RESULT.md`.

### 0.8.4 — GraphRAG community-summary paradigm — provisional resolution ("third graph null")

- **Result:** comprehensiveness win-rate: flat map-reduce over RAW text 0.72/0.83 (wins both); community-S1 (Qwen reports) 0.42/0.39 (loses); community-S1 (gpt-5.4 reports) 0.49/0.32.
- **Verdict:** provisional — do NOT fund full S1 community-summary (lossy compression); ship strong-reader map-reduce over raw text.
- **Closed by:** WITHDRAWN by the literal Microsoft head-to-head (crude reimplementation under-represented real GraphRAG).
- **$:** ≈$10. **Sources:** `runs/0.8.4-graphrag-RESOLUTION.md`.

### 0.8.4 — Literal head-to-head vs running Microsoft GraphRAG 3.1.0 (15 docs)

- **Result:** `fathomdb_mapreduce` comp 0.062 [0,0.19], div 0.319, emp 0.231; `fathomdb_vector` comp 0.000. Far below the 0.45 band — Microsoft GraphRAG WINS decisively.
- **Verdict:** FathomDB NOT at parity; withdrew the "don't fund S1" lean.
- **Closed by:** **REFUTED** by the Tier-1 fair re-run + scale run + gating re-run (a 15-doc + 600-token-cap/top-8 artifact).
- **$:** ≈$14–15. **Sources:** `runs/0.8.4-vs-microsoft-graphrag-RESULT.md`, `runs/0.8.4-COMPREHENSIVE-REPORT.md` (header self-marks CONCLUSION SUPERSEDED).

### 0.8.4 — AutoE powered-run cost projection (deterministic, $0)

- **Result:** answerer ~10,686 in/~400 out tok/call (gpt-5.4); judge ~1,042/~80. Full ~100q 3-pair run ≈ $8–10 (Haiku) / $16–18 (Sonnet) / $24–26 (Opus 4.8).
- **Verdict:** budget concern over-stated; Haiku 4.5 recommended for the first powered pair.
- **$:** $0. **Sources:** `runs/0.8.4-cost-probe-FINDINGS.md`.

### 0.8.4 — Tier-1 FAIR re-run (the 15-doc loss was largely a measurement artifact)

- **N & power:** 8 Q × 5 × 2 = 80 judgments; MDE ~0.22 (diagnostic).
- **Result:** `fathomdb_mapreduce` comp 0.812 [0.562,1.000], div 0.875, emp 0.425; `fathomdb_vector` (k=15+MMR) comp 0.525. vs original: mapreduce comp 0.062→0.812, div 0.319→0.875.
- **Verdict:** map-reduce ABOVE parity on 2/3 metrics → original loss dominated by token-cap + top-8 at 15-doc scale. Fork C withdrawn; live question = SCALE.
- **$:** **$0.324** (gpt-5-nano both sides). **Sources:** `runs/0.8.4-tier1-fair-rerun-RESULT.{md,json}`, `design/0.8.4-closing-graphrag-gap.md`.

### 0.8.4 — SCALE powered run @ 200 docs ("GraphRAG wins at scale" refuted)

- **N & power:** n=500 (50q × 5 × 2); MDE ~0.09–0.11 (NOT_REACHED on power only).
- **Result:** C comp 0.828 [0.730,0.918], div 0.805, emp 0.716; D2 comp 0.719 [0.612,0.825], div 0.771, emp 0.744; length_contradicts=False both. All 6 cells surpass candidates.
- **Verdict:** both arms surpass on all 3 metrics; "GraphRAG wins at scale" refuted (vs gpt-5-nano GraphRAG at community-level 0).
- **Closed by:** partially reversed for D2 by the gating re-run (D2's surpass was substantially a level-0 artifact); C held.
- **$:** **$2.91** + $0.012 D2 build + ~$1–3 est. GraphRAG index. **Sources:** `runs/0.8.4-scale-powered-run-RESULT.{md,json}`.

### 0.8.4 — GATING re-run (SPLIT verdict; full-strength GraphRAG level-1) — CURRENT

- **N & power:** N_Q=200 (corpus max); n=2000/metric; comp MDE 0.058 (C)/0.062 (D2) > ε=0.05 → NOT_REACHED, not resolvable on this corpus.
- **Result:** **C** comp 0.723 [0.663,0.780], div 0.614, emp 0.719, length_contradicts=False (~40% shorter) → **clean surpass ×3**. **D2** comp 0.413 [0.348,0.473] (below 0.5 → GraphRAG wins), div 0.446, emp 0.599 (length-flagged).
- **Verdict:** **SPLIT.** C (expensive, reads everything) surpasses full-strength GraphRAG; D2 (the cheap product) LOSES comprehensiveness+diversity → prior surpass was a level-0 artifact. Do NOT flip OPP-4; **Fork E (entity/Leiden graph) RE-OPENS**. HITL-gated. *(live; current 0.8.4 verdict.)*
- **$:** ≥ **$42.34** LOWER BOUND (fathomdb $21.60 + graphrag $20.74; meter undercounts, answer data unaffected). **Sources:** `runs/0.8.4-gating-rerun-RESULT.{md,json}`, `runs/0.8.4-COMPREHENSIVE-REPORT.md`.

## 0.8.11 — planner-router experiment ladder (Track E; F-11 discharge)

> **F-11 closure.** Before 0.8.11 this ledger had **zero** planner-router rows — the
> `Gate-0/2 → EXP-A/M4 → EXP-B′ → EXP-Fr-acc → EXP-AF` ladder was scheduled as 0.8.7/0.8.9 `$0`
> float and never ran. 0.8.11 owns and discharges it (HITL 2026-06-28). **Pre-registration**
> (hypothesis · KILL · corpus · cost ceiling · script) → `dev/plans/0.8.11-implementation.md §1`;
> live `$` tally → `runs/STATUS-0.8.11.md`. The rows below are **REGISTERED at Slice 0**; each is
> filled with numbers+CI+verdict when its slice lands (R-LEDGER-1). Ladder cap: **~$20 priced-LLM**.

| Tag | Question (short) | KILL | $ ceiling | Slice | Status |
| --- | --- | --- | ---: | :---: | --- |
| Gate-0 | Re-scope golden set to reused assets + decide_083/084; scoped node-labels for gaps only | labeling exceeds the gap (→ fresh golden set) | $1 | 5 | **RESOLVED** — re-scope holds; 1 scoped gap (LOCOMO node-labels, $0 exp/≤$1); EXP-D excluded → detail below |
| Gate-2 | Oracle best-plan-per-query ceiling; per-arm cost tiers; reconcile +0.39-over-Mem0 | ceiling within noise of fused-RRF for all classes (routing buys ≈0) | $0 | 5 | **RESOLVED** — oracle-CONTEXT pooled **+0.392 [0.346,0.436]** (reconciles exactly; fresh recompute=priced→deferred); arm-selection headroom within recall noise → value = config-carrying tuning, not arm routing → detail below |
| EXP-A | Wider candidate-gen lifts F2 recall@K_deep / gold-in-pool | no breadth lifts gold-in-pool (CI clears noise) | $0 | 10 | **RESOLVED — GO.** F2 multi_session gold-in-pool @10=0.20–0.275 → @candidate_k=200=0.65–0.675 (lift **+0.45/+0.40**; best-K CI-lo 0.50/0.525 clears the @10 floor); all 4 classes lift. Max at candidate_k=200 (not saturated → EXP-B′ test ≥200); per-query arm-log persisted (Slice-5 oracle enabler) → detail below |
| EXP-M4 | Embedder swap-candidate beats bge-small net of re-whiten/re-clear (ceiling, GPU) | none beats bge-small (default keep; swap out-of-0.8.11) | $0 | 10 | **RESOLVED — KEEP bge-small.** No swap-candidate clears the gate net of eu7 re-clear+cost (s15a FULL n=10506): bge-base eu8 +0.024 but projected_eu7 0.786<0.90; e5-base-v2 0.896<0.90; nomic 0.932 but not cpu_feasible; gte-base measurement-failed. eu-0 raw r@10 confirms ordering, revises decision. GPU device-invariance ✅ (cosine 1.0, RTX 3090). Swap out-of-0.8.11 → HITL #2 → detail below |
| EXP-B′ | Per-intent `(idx,retr,α,pool_n,MMR,recency)` optimum diverges; α=1.0@pool_n=50 drops r@10 | optima collapse to one global config | $6 | 15 | REGISTERED — pending |
| EXP-B′.5 | A config for feature X must not regress feature Y (joint-regression guard) | — (guard output) | (incl) | 15 | REGISTERED — pending |
| EXP-Fr-acc | 5-class classifier accuracy + asymmetric mis-route matrix (needle→C −0.362) | classifier at chance for ≥2 classes | $3 | 20 | **RESOLVED — NO KILL.** Internal-fallback classifier macro **0.768 [0.732,0.802]**, all 5 classes > 0.20 chance (needle weakest 0.500). Mis-route asymmetry confirmed: **needle is the ONLY negative Δ_C**; needle→C **scales with map-reduce breadth** −0.080 [−0.28,+0.12] @3-distractor → **−0.300 [−0.47,−0.10] @8-distractor** (CI excludes 0; ≈ prior −0.362). $0.05/$3 → detail below |
| EXP-Fr-acc/VoI | value-of-signal + ask-or-not VoI break-even + asymmetric weighting | no `(ce_score,margin)` region with positive VoI | $3 | 25 | REGISTERED — pending |
| EXP-AF | Agent relevance signal beats `ce_score`-only net of round-trip (1–2 depth) | signal does not beat `ce_score` net of round-trip (KILL → drop arm) | $5 | 30 | REGISTERED — pending |

### Gate-0 — golden-set re-scope (Slice 5, RESOLVED 2026-06-28)

- **Assets (inspected, EVAL-ONLY gitignored):** IR gold `eval/ir_gold/all.gold.json` 4,597 Q / 4,472 with `expected_top_k_doc_ids` (exact_fact 2,888 · exploratory 1,584 · negative 125; src enronqa/qaconv/qmsum); LOCOMO `eval/0.8.3-locomo-memory-gold.json` 1,443 Q (factoid 841 · temporal 321 · multi_session 281, CC-BY-NC-4.0); MuSiQue `raw/musique_dev.jsonl` 4,834 total / **2,417 answerable** (2/3/4-hop = 1,252/760/405; `is_supporting` paras, mean 2.65); AP-News BenchmarkQED 1,397 articles + 350 AutoQ (MS-Research NON-REDISTRIBUTABLE); LME memex-elps (8 golden + 60 personal.gold, extraction gold).
- **Result — node-level retrieval labels by class:** **needle** = doc-qrels ✅ (IR gold, derivable); **multi_hop** = paragraph `is_supporting` ✅ (MuSiQue, derivable, no labeling); **global** = none needed (sensemaking → `decide_084` answer-quality, not retrieval recall); **multi_session/temporal** = LOCOMO carries **session-level only** (`conv-N:session_M`) → the single GAP. Rule adoption: `decide_083` (MDE ≤ 0.05) governs needle/multi_session/temporal vs Mem0; `decide_084` (win-rate ε=0.05, question-clustered, **N=200 cap**) governs global vs GraphRAG; MuSiQue/HippoRAG-2 = `[TBD: decide_08x]` (out of scope). Measured discrepancy flagged: PSD "4,834 answerable" is total — usable multi_hop = **2,417**.
- **Verdict:** Re-scope HOLDS. One scoped labeling pass = refine LOCOMO temporal+multi_session (≤602 Q) from session→node-level (deterministic answer/turn match first, cheap-LLM residual only). **EXP-D (~269-Q F4/M6 acquisition) EXCLUDED → stays 0.8.17**; corpus-cap confirmed (`decide_084` N=200 AP-News max, comp MDE 0.058 > ε; question-clustered ⇒ more runs can't tighten, only more questions). No fresh golden set.
- **$:** **$0** (inventory/mapping; labeling pass projected $0 expected, ≤$1 hard cap, unspent at Gate-0). **Sources:** `dev/plans/runs/gate0-rescope-output.md` + `gate0-rescope-output.json` (`src/python/eval/gate0_rescope.py`, $0 re-runnable); PSD §II.A/§III.A; `dev/plans/0.8.11-implementation.md` §1; rules `src/python/eval/decision_rule_083.py`, `decision_rule_084.py`.

### Gate-2 — oracle-routing upper bound (Slice 5, RESOLVED 2026-06-28)

- **Method ($0).** A fresh oracle-context decomposition needs the priced gpt-5.4 reader
  (`gap_decomposition_run.py`) → **deferred** under the $0 constraint; Gate-2 **reuses** the
  already-paid n606 artifact for the oracle-CONTEXT ceiling and **computes at $0** the
  recall-ARM-selection ceiling from existing per-arm recall runs (`gate2_oracle_run.py`).
- **Oracle-CONTEXT ceiling** (`acc_oracle_raw − acc_fathomdb`, value of perfect retrieval):
  factoid(→needle) +0.372 [0.295,0.449]; knowledge_update(→needle) +0.530 [0.435,0.626];
  multi_session +0.412 [0.294,0.529]; temporal +0.247 [0.165,0.340]; **pooled +0.392
  [0.346,0.436]** (n436). **Reconciles exactly** with the 0.8.3 ledger +0.392 — by construction
  (same n606 artifact; not an independent re-measurement).
- **Oracle-ARM-selection ceiling** (best static arm − fused-RRF; class-level = lower bound on
  per-query oracle): LME recall@10 headroom factoid +0.05, knowledge_update +0.05, multi_session
  **0.00** (fused already best), temporal +0.025; MuSiQue ≥3-hop F1 headroom **+0.036**
  (passage_dense 0.487 > fused 0.450; ppr-vs-fused was a tie). All below the per-class recall MDE
  (~0.11–0.17 at n=40) → within noise.
- **Per-arm cost tiers:** `fts_bm25` low (p50<1ms@10k), `vector_ann` low-med (p50 25/p99 40ms),
  `rrf` low, `ce_rerank` med (TinyBERT 308ms@K=200; MiniLM high), `map_reduce_qfs` high (LLM tier,
  reads everything, ≥$21; F4-only), `graph_bfs` ~0-value (REFUTED ×2, default-OFF).
- **KILL check.** **NO KILL on the oracle-CONTEXT axis** — ceiling +0.25..+0.53/class (CI lower
  0.346 ≫ 0), far outside fused-RRF noise: large routing-relevant headroom exists. **Arm-switching
  alone buys ≈0** (within noise every class; multi_session 0.00).
- **Verdict:** Program **not killed**; realizable headroom is in recall/precision **generation**
  (EXP-A wider candidate-gen; EXP-B′ per-intent α/pool_n/candidate_k) captured by a
  **config-carrying** router — NOT static-arm routing. Consistent with the refuted graph arm +
  CE-rerank-is-the-lever findings.
- **$:** **$0.** **Sources:** `dev/plans/runs/gate2-oracle.md` + `gate2-oracle-output.json`
  (`src/python/eval/gate2_oracle_run.py`, $0 re-runnable); reuses `runs/0.8.3-gap-decomposition-n606.json`,
  `runs/0.8.1-p0a-fused-recall-n160.json`, `runs/0.8.2-m1-verdict-gpt54.json`.

### EXP-A — recall generation / candidate breadth (Slice 10, RESOLVED 2026-06-28)

- **Method ($0, LLM-free, deterministic).** LME class-balanced n=160 (40/class: factoid ·
  knowledge_update · multi_session · temporal; seed 20260614; 7,154-session union). Reuse the P0-A
  loader + retrieval variants; sweep candidate breadth `K∈{10,20,50,100,200}`; score **gold-in-pool**
  at each K (multi_session full-gold-set rule); per-(class,arm,K) percentile bootstrap CI (2000
  resamples, seed 0xEA); persist per-query per-arm gold ranks.
- **F2 multi_session gold-in-pool (point [CI]):** `fathomdb_fts_only` @10 **0.20** [0.075,0.325] →
  @50 0.40 → @100 0.525 → @200 **0.65** [0.50,0.775]; `naive_bm25` @10 **0.275** [0.125,0.425] →
  @200 **0.675** [0.525,0.80]. **Lift @200−@10 = +0.45 / +0.40**, and the @200 CI-lo (0.50 / 0.525)
  clears the @10 point estimate → **CI clears noise**.
- **All classes lift** with breadth (fts_only / bm25 @10→@200): factoid 0.65→0.875 / 0.70→0.90;
  knowledge_update 0.80→0.925 / 0.875→1.00; temporal 0.60→0.925 / 0.65→0.95. multi_session is the
  hardest and gains the most absolute headroom — the recall the shipped final_K=10 view misses but a
  widened candidate pool recovers, which a CE-rerank stage (EXP-B′) can then surface.
- **KILL check:** **NOT killed → GO.** Wider candidate generation lifts gold-in-pool with the CI
  clearing the K=10 floor on the F2 focus class (and every class). This is the exact
  config-carrying lever Gate-2 pointed to (recall *generation*, not arm routing).
- **candidate_k that maximizes gold-in-pool (feeds EXP-B′):** **200** (top of the grid; recall is
  still rising at 200 — NOT saturated → EXP-B′ should also probe candidate_k ≥ 200).
- **Per-query arm-selection oracle (deferred at Slice-5 Gate-2):** now computable — `per_query_log`
  (160 questions) carries, per arm, each gold session's 0-based rank, the min gold rank, and
  all-gold-found@200. Lexical arms (bm25, fts_only) measured at $0; the **fused-RRF** arm (the
  shipped candidate set; CPU embedder, GPU idle by design) corroborates and is anchored by Gate-2's
  measured multi_session fused≈bm25 (fused r@10 0.325 ≥ bm25 0.275) — the breadth lift is a
  pool-depth property the fused arm shares.
- **$:** **$0.** **Sources:** `dev/plans/runs/expa-recall-output.json` + `expa-recall.md`
  (`src/python/eval/expa_recall_run.py`, $0 re-runnable: `--with-fused` adds the dense arm).

### EXP-M4 — embedder-ceiling measurement (Slice 10, RESOLVED 2026-06-28)

- **Method ($0 / GPU).** The embedder ceiling is a **device-invariant model-weights property**
  (CPU↔GPU vectors are f32-equivalent), so EXP-M4 **consolidates** two already-paid byte-verified
  offline measurements (the Gate-2 reuse precedent) and **confirms device-invariance on the GPU**:
  the FULL `s15a` probe (eu7 1-bit re-clear + eu8 strict doc-id recall + BM25 hard subset +
  paired-bootstrap margin CIs + cpu cost) over the 10,506-doc frozen IR snapshot, and the `eu-0`
  raw-recall sweep. **GPU confirmation (RTX 3090, cuda:0):** bge-small GPU-vs-CPU mean row cosine
  **1.000000**, max abs elt diff 1.2e-7 → the reused CPU ceiling holds on GPU.
- **s15a candidate verdicts vs CLS-corrected bge-small (base eu8 0.3994, hard@10 0.0194, 11.2ms/q):**
  bge-base eu8 0.4235 (margin +0.024) but **projected_eu7 0.7855 < 0.90** (fails 1-bit re-clear) +
  hard-margin CI-lo −0.0036; e5-base-v2 eu8 0.4674 but **proj_eu7 0.896 < 0.90** + hard CI-lo
  −0.0061; nomic proj_eu7 0.9317 (clears 0.90) but **not cpu_feasible** (36.1ms > 3× base) + hard
  CI-lo −0.0085; gte-base **measurement FAILED** (transformers IndexError; no candle-native encoder
  → not in-library). **No candidate clears `probe_15a_pass`.**
- **eu-0 reconciliation:** raw recall@10 (1-bit Hamming→f32, n=100, 7,667 docs, fanout K=256)
  bge-small **0.933** · bge-base **0.964** · e5-small-v2 **0.664** (bge-small K=192 = 0.933
  [0.912,0.953]). EXP-M4 **CONFIRMS** the eu-0 ordering (bge-base highest raw recall, e5 worst) but
  **REVISES** the naive "bigger is better": net of the 1-bit eu7 re-clear (bge-base proj_eu7 0.786 <
  0.90), the hard-subset margin, and 2× cost, bge-base does **not** clear the swap gate.
- **KILL/GO (HITL #2):** **KEEP CLS-corrected bge-small.** A productized swap is **out of 0.8.11**;
  no ceiling escalation triggered (no passer). Keep-unless: a candidate simultaneously clears the
  0.90 projected_eu7 floor after re-whiten, shows a hard-subset margin CI-lo > 0 vs bge-small, and
  is cpu_feasible (or HITL accepts the GPU/cost tradeoff).
- **$:** **$0.** **Sources:** `dev/plans/runs/expm4-ceiling-output.json` + `expm4-ceiling.md`
  (`src/python/eval/expm4_embedder_ceiling_run.py`); reuses `runs/0.8.3-s15a-embedder.json`
  (`eval.s15a_embedder_probe`) + `research/eu-0/result_*.json`.

### EXP-Fr-acc base — classifier accuracy + asymmetric mis-route cost (Slice 20, RESOLVED 2026-06-28)

- **Deliverable 1 — classifier ($0, no LLM).** The *internal-fallback* intent classifier (PSD
  §II.A pref #3; agent-label #1, provider-callback #2 are preferred and not measured here). Pure-numpy
  **lexical TF-IDF nearest-centroid (Rocchio)**, **stratified 5-fold CV**, **balanced 100/class**
  (chance 0.20; global caps the balanced N). *(No torch/sklearn in env → embedding variant
  unavailable; lexical = honest $0 fallback proxy, likely a lower bound.)* Bootstrap CI 2000×.
- **Per-class accuracy [CI]:** needle **0.500** [0.40,0.60]; multi_session **0.630** [0.54,0.72];
  temporal **0.770** [0.69,0.85]; global **1.000** [1.00,1.00]; multi_hop **0.940** [0.89,0.98];
  **macro 0.768 [0.732,0.802]**. Confusion = the three memory/compositional classes leak into each
  other (needle → multi_session 20 / multi_hop 16 / temporal 14); global + multi_hop near-separable.
- **KILL check (classifier):** **NO KILL** — 0 classes at chance (weakest CI-lo 0.40 ≫ 0.20). The
  internal classifier is a usable low-confidence fallback; prototype keeps the agent-label-first order.
- **Deliverable 2 — asymmetric mis-route matrix (priced).** Oracle-context **answer-quality** delta,
  route `C` (map-reduce/QFS: per-chunk QFS → reduce → answer) vs the correct `retrieval` route; both
  arms see identical chunks (gold evidence + distractors), same `gemini-flash-lite` judge grades both
  (paired bootstrap CI 2000×). LOCOMO (needle/multi_session/temporal) + MuSiQue (multi_hop). Base
  25/class, 3 distractors. **Δ_C (C − retrieval, judge):** needle **−0.080** [−0.28,+0.12];
  multi_session +0.040; temporal +0.040; multi_hop +0.040. **needle is the UNIQUE negative cell**
  (the predicted high-cost cross-wire); others neutral-to-slightly-positive (temporal/multi_hop on a
  low absolute floor 0.16–0.20).
- **Load-bearing needle→C (the −0.362 claim).** The penalty **scales with map-reduce breadth**: a
  sensitivity arm (needle, n=40, **8 distractors** = a QFS that reads more) gives **−0.300
  [−0.47,−0.10]** (judge; CI **excludes 0**) / −0.225 (containment), with retrieval 0.750 → C 0.450.
  **−0.300 ≈ the prior −0.362** — which the 0.8.3 gap-decomposition ledger flagged as a
  weak-distiller (gpt-5-nano) artifact; reproduced here with a competent cheap summarizer once the
  map-reduce reads enough chunks. → Encode `map_reduce_qfs`/`community_summary` as `forbidden_ops`
  on `needle` (EXP-B′.5 forbidden-composition matrix); the asymmetry feeds the Slice-25 VoI policy.
- **Deferred cells.** `global` (both routes) → reference-free `decide_084` win-rate axis (EXP-B′
  priced judge, Slice 15), not gold-answer accuracy. Within-retrieval config cells (needle→
  multi_session *stack*) differ by config (α/pool_n), not route → owned by EXP-B′.5, not measured.
- **Models + $.** Classifier $0. Mis-route = `gemini-flash-lite` (local vLLM qwen3.6-27b/gemma-4
  were HTTP-500 down; cheap hosted tier used; same model both arms → fair delta). Committed runs
  **$0.041** (base $0.0236, 886 calls + needle-deep $0.0173) + ~$0.013 discarded first attempt +
  $0.0004 cheap-validate ≈ **$0.054 total**, ≪ $3. Resilient harness (per-item checkpoint, idempotent
  `--resume`, `BudgetLedger --max-usd 3.0` guard); cheap-validated (2-Q) before spend.
- **$:** **≈ $0.05** (of $3). **Sources:** `dev/plans/runs/fracc-base-output.json` + `fracc-base.md`
  (`src/python/eval/fracc_classifier_run.py`, base mode; reproducible). PSD §II.A/§II.D/§III.D;
  `dev/plans/0.8.11-implementation.md` §1. Prior −0.362 context: ledger 0.8.3 gap-decomposition row.

## research/ (UNTRACKED — git-ignored; results live ONLY here)

### research/eu-0 — eu7 embedder + quantization-path sweep (RESOLVED)

- **N & power:** 100 queries × 7,667 docs per cell; bge-small/e5-small-v2/bge-base; K∈{32…256}; mean-centering ablation; SEED=0xE0, 1000 bootstrap.
- **Result:** bge-small (384d) K=256 recall@10 0.933 [0.904,0.957], K=128 0.882, K=64 0.793; bge-base (768d) K=256 0.964; e5-small-v2 worst (0.664). Mean-centering +0.05 at K=64. bge-small+mean-centering fine sweep: **K=192 = 0.933 [0.912,0.953]** (CI lower 0.912 > 0.90; cushion +0.033); K=128 0.907 (doesn't clear).
- **Verdict:** RESOLVED — leader bge-small + mean-centering at K≥192 clears the floor on the lower CI bound; bge-base higher but 2× cost; e5 rejected. (Quant-path recovery lever for the eu7 0.896 at-gate finding.)
- **$:** $0 (local CPU; HF transformers). **Sources:** `research/eu-0/all_results.json`, `result_*.json`, `run_sweep.py`, `run_k192_check.py`, `sweep.log` — **UNTRACKED**.

### research/pr-2a — EU-0(0.933)→EU-7(0.828) mean-centering decomposition (RESOLVED)

- **Result:** (EU-0 queries) M0 first-256 (single-source) 0.886; M1 full-corpus 0.933; M2 stratified/uniform-256 ≈ 0.933; M3 no-centering 0.910. (EU-7 queries) M0 0.842, M1 0.951, M2 ≈ 0.945–0.952, M3 0.918. The production first-256 mean is unrepresentative (~5pp cost); stratified/uniform-256 recovers it. Mean-centering itself buys modestly.
- **Verdict:** fix = stratified/uniform sample mean; the ~5.8pp M0→real-0.828 residual is query-methodology + real-engine path, NOT the mean.
- **$:** $0. **Sources:** `research/pr-2a/result_mean_decomp.json`, `result_queryset_sensitivity.json`, `result_m2_eu7queries.json`, `run_*.py` — **UNTRACKED**.

### research/pr-2a — offline(0.944)→real(0.847) residual: GT-accounting + title-artifact (RESOLVED)

- **Result:** title+body 0.951 vs body-only 0.944 (Δ0.007 → title-in-doc NOT the driver). GT-accounting: INDEX 0.944 vs BODY-STRING 0.930 (Δ0.014; 26/100 queries have duplicate GT bodies).
- **Verdict:** title confound ruled out (~0.7pp); body-string-GT-with-dup-bodies a real but small (~1.4pp) artifact. These shrink but do NOT fully close the ~9.7pp offline→real gap; remainder = engine path/query methodology (bounded by PR-3). Aligns with "recall GO was a measurement artifact"; floor kept 0.90.
- **$:** $0. **Sources:** `research/pr-2a/result_bodyonly_offline.json`, `result_gt_accounting.json`, `run_*.py` — **UNTRACKED**.

---

## Deferred / revisit

Decisions intentionally NOT made by the first ledger-prune — revisit before resolving.

- **R-2 — `dev/research/` disposition (HOLD).** The eu-0 and pr-2a results above are now distilled
  here, but the source tree is **untracked / git-ignored** (no git recovery). This prune
  **did NOT delete anything** under `dev/research/`; all files remain in place. **Undecided:**
  whether to (a) delete the regenerable artifacts (`*.npy` vectors, `sweep.log`), (b) relocate
  the tree into an `archive/` subdir, or (c) leave as-is. **Before any future removal:** confirm
  these ledger entries fully capture the results, confirm the `.npy`/scripts are regenerable from
  what's recorded, and verify nothing live reads them. **Delete is NOT yet confirmed as the
  correct approach.**
- **0.7.0 AC-020 closure gap.** No AC-020 closure-verdict artifact was located and
  `ADR-0.7.0-ac020-architectural-lever.md` is still `status: draft`. Whether PCACHE2 finally
  landed / AC-020 was closed could not be verified from run files — resolve in a future doc-sync.
