# Planner-router SECONDARY REVIEW — FathomDB's approaches positioned against the research survey

Review date: 2026-06-28. Author: secondary-review agent. **Recommendations-first.**

Primary inputs (on-disk): `dev/research/planner-router-survey.md` (34 approaches / 6 families),
`dev/research/planner-router-competitors.md` (shortlist), `dev/plans/runs/0.8.11-handoff-to-0.8.15.md`
(confidence ledger + corrected findings + function ledger), the 0.8.11 per-experiment docs
(`gate2-oracle.md`, `expa-recall.md`, `expb-joint-tune.md`, `fracc-base.md`, `fracc-voi.md`,
`expaf-value-report.md`), and the experiments-ledger 0.8.11 section.

Web spot-check this pass: **HippoRAG 2** main table (arXiv:2502.14802v1) — verified directly (see §2).
All other paper numbers are carried from the survey with the survey's own verification status
preserved; cells the survey flagged `[UNVERIFIED]` are **not** laundered into firm claims here.

**Throughline (inherited, HITL 2026-06-28):** the embedder ceiling (~0.571 IR relevance), the
arm-switching ≈0 read, and the EXP-AF "recall-bound" finding all say the **binding constraint is
recall/substrate, not routing-cleverness.** Every recommendation below is scored against that: *does
the method attack recall, or only re-rank within an already-capped pool?* — and against FathomDB's
**CPU-default / local-first / optional-caller-LLM** posture (many paper "wins" are hosted-LLM/GPU
artifacts that do not transfer).

---

## §0 Recommendations (lead with this) — ordered by expected value

| # | Item | Verdict | One-line why |
|---|---|---|---|
| 1 | **Iterative/agentic multi-hop arm** (IRCoT- / Iter-RetGen-lite, optional caller-LLM) tested on **MuSiQue** | **TEST-ONLY** (behind the maturity guard; = V-3 multi_hop arm) | Highest-stakes gap: the whole family wins *big* in-paper on multi-hop — the **one class EXP-AF's KILL explicitly EXCLUDED** — and it works by **issuing new sub-queries**, i.e. it attacks the recall-bound constraint FathomDB itself named as binding, instead of reshuffling a capped pool. |
| 2 | **Per-query arm-routing head-to-head on BEIR** (RouterRetriever / MoR style), nDCG@10 / Recall@k | **RECOMMEND** (V-2 + V-6 secondary) | Cheapest decisive test of the core internal thesis ("static arm-selection ≈ RRF"); **CPU-only, no LLM**, corpora already in the `beir-acquire` scripts; MoR's "Route Oracle" *is* the per-query oracle V-2 needs. |
| 3 | **Stronger cross-vendor embedder vs the ~0.571 ceiling** (NV-Embed / GritLM / domain-expert-mixture lessons) | **RECOMMEND — but out-of-router-scope** (the separate embedder gate, not router work) | FathomDB's own §3/§5 call the embedder the single highest-impact lever; the survey confirms embedder upgrades buy real nDCG (MoR +3.9% over GritLM-7B; RouterRetriever +2.1). It attacks the *actual* binding constraint. Heavy/GPU/cross-vendor → stays separately gated. |
| 4 | **HippoRAG-2-style LLM-extracted-KG + PPR** re-examination on MuSiQue | **TEST-ONLY** (= V-6 multi-hop head-to-head; report the number, do **not** rebuild the arm) | FathomDB's graph/PPR arm is refuted (M1 NO-GO) — **but by a different mechanism** (entity-co-occurrence BFS), whereas HippoRAG2 (LLM-KG triples + NV-Embed) **beats dense on MuSiQue (F1 48.6 vs 45.7, verified)**. The refutation does not cover this mechanism; get the benchmark number before re-closing. |
| 5 | **Adaptive-RAG framing head-to-head on MuSiQue** (EM/F1 + steps + oracle-vs-learned gap) | **RECOMMEND** (V-6 primary; **reproduce locally**, don't quote hosted-LLM headline) | The field's closest conceptual competitor and the natural framing anchor — it reports an **oracle classifier ceiling = FathomDB's Gate-2 device**, letting the learned-vs-oracle gap be stated in the field's own vocabulary. |
| 6 | **Real intent classifier** vs the RAGRouter-Bench TF-IDF+SVM 0.928 baseline | **TEST-ONLY** (= V-4) | The published lightweight baseline (0.928 macro-F1) beats FathomDB's **0.768 lexical *proxy lower bound*** — but different taxonomy (3-class vs 5-class) and, decisively, **production intent comes from Memex (preference #1)**, so the internal classifier is a fallback, not the hot path. Low stakes. |
| 7 | **Confidence-gated retrieve-or-not** (Self-RAG / FLARE / SKR / Self-DC / Rowen / UAR) | **DON'T-RECOMMEND** | These are **LLM-generation** properties (skip retrieval when parametric knowledge suffices); they need a generator in the loop and don't transfer to a CPU-default *retrieval substrate*. That decision belongs to the caller's agent, not FathomDB. |
| 8 | **LLM-reader routing** (RouteLLM / FrugalGPT / RouterDC / ZOOTER / AutoMix / Hybrid-LLM / Self-Route) | **DON'T-RECOMMEND** | Routes over *which generator LLM* — FathomDB does not own the reader (optional caller-side). Out of posture entirely; the cost-savings "wins" are hosted-LLM-fleet artifacts. |

**Single highest-stakes finding.** The **iterative/agentic multi-hop family is FathomDB's largest
untested exposure.** FathomDB is deliberately single-shot, and its EXP-AF agent-feedback KILL —
already flagged *current-substrate provisional* — **never tested `multi_hop` at all** (the class
where iterative reasoning is most theorized to pay, and where IRCoT/Search-o1/HippoRAG2 post their
biggest in-paper gains). EXP-AF's own root-cause was *recall-bound* ("an agent cannot manufacture
recall the substrate never produced; it can only reshuffle what is there"); the iterative family is
the one category that **manufactures recall** by generating fresh sub-queries. So the single-shot +
agent-feedback-KILL posture is **currently unjustified for `multi_hop`** and must be re-tested under
V-3 before being read as a lasting verdict.

---

## §1 Mapping table — bidirectional (us → analogue → its competitors → who won)

| FathomDB approach | Closest paper analogue(s) | Analogue's in-paper competitors | Who won (metric / benchmark) | Direction-A read (our analogue win/lose in-paper) | Direction-B (who bested our analogue) |
|---|---|---|---|---|---|
| **RRF-fused BM25+vector** (strong floor; "absorbs the arms") | **Blended RAG** (IBM); **MoR** (weighted blend) | Blended: individual dense/sparse configs · MoR: BM25, Contriever, DPR, GTR, RepLLaMA-7B, **GritLM-7B** | Blended: new IR SOTA on NQ/TREC-COVID · MoR: **+3.9% nDCG@20 over GritLM-7B; Route-Oracle +13.5%** | **WIN** — fixed/blended fusion beats single retrievers (mirrors our "RRF is the strong floor") | **MoR's per-query *weighted* blend** beats fixed blends — a learned-per-query fuser tops our static RRF [survey-cited] |
| **CE-rerank** (TinyBERT, per-intent α/pool_n — the measured win) | *No dedicated survey row*; rerank appears as a stage. Nearest: **LexBoost** (fuse lexical + dense-neighbor at rank) | LexBoost: BM25/lexical-only | LexBoost: "improves lexical ranking, minimal online overhead" `[survey: exact sets UNVERIFIED]` | n/a clean analogue — CE-rerank is a standard component, not a routing paper | None in-survey reranks *per-intent*; the **knob-tuning** part is unoccupied territory |
| **candidate-gen breadth** (`candidate_k`) | *No routing analogue* (a pool-depth knob) | — | — | n/a | — (no paper routes on pool depth) |
| **Per-intent CONFIG/knob routing** (THE differentiator) | **Adaptive-RAG** (intent/complexity→strategy; *spirit*, but routes **pipeline depth** not arm-knobs); **RAGRouter-Bench** (routes **paradigm**) | Adaptive-RAG: no-retrieval / single-step / multi-step fixed strategies; Self-RAG | Adaptive-RAG: "best quality/efficiency trade-off"; **reports w/ Oracle classifier** | **WIN** — adaptive beats every fixed strategy in-paper | No survey system routes **config/knobs per intent** → genuinely unoccupied (but our measured win is *knob tuning*, not arm-selection — frame as "config-granularity routing", not a new paradigm) |
| **map-reduce / QFS for `global` only** (router-isolated) | GraphRAG sensemaking line; **RAGRouter-Bench "summarization"** query type | (no router head-to-head) | — (sensemaking lit, not routing) | n/a | — (no routing-lit benchmark for sensemaking) |
| **5-class lexical intent classifier** | Adaptive-RAG complexity classifier; **RAGRouter-Bench baseline (TF-IDF+SVM)** | TF-IDF+SVM vs 4 other classifiers × 3 feature types | **TF-IDF+SVM 0.928 macro-F1, ~28% token savings** (RAGRouter-Bench, 3-class) | n/a (their baseline, not a system) | **TF-IDF+SVM 0.928 > our 0.768** lexical *proxy lower bound* — different taxonomy (3 vs 5 class); see §2 |
| **Embedder = CLS-bge-small** (~0.571 ceiling) | Embedder-choice / routed-embedder line: **RouterRetriever, MoR**; **HippoRAG2** uses NV-Embed-v2 | single general (MSMARCO) embedder; multi-task; GritLM-7B; dense baselines | RouterRetriever **+2.1 nDCG@10** vs single; MoR Route-Oracle **+13.5%**; HippoRAG2 NV-Embed beats dense | n/a (we *are* the single small embedder) | **Bigger/expert/mixture embedders beat a single small one** — directly attacks our ceiling |
| **NEG: graph/PPR arm OFF** (M1 NO-GO, ΔF1 −0.0405) | **HippoRAG / HippoRAG 2** (KG + Personalized PageRank) | HippoRAG2: HippoRAG, dense retrievers, GraphRAG-style | **HippoRAG2 MuSiQue F1 48.6 vs HippoRAG 35.1 vs dense 45.7** (verified, §2); orig HippoRAG +up to 20% multi-hop | n/a (we turned it off) | **HippoRAG/2 graph-PPR beats dense** — *but different mechanism* (LLM-KG triples vs our entity-co-occurrence BFS); our refutation does not cover it → V-6 |
| **NEG: single-shot retrieval** (no iterative loop) | **Family 3 entire**: IRCoT, Self-Ask, Iter-RetGen, ProbTree, BeamAggR, ReSP, Search-o1, ReAct | one-step retrieval; CoT; Direct; Self-Ask; IRCoT; ITER-RETGEN | IRCoT +up to 15 QA pts; Iter-RetGen HotpotQA 71.2 vs Self-Ask 64.8; Search-o1 +29.6% EM; ReSP HotpotQA F1 47.2 vs IRCoT 43.1 | n/a (we don't iterate) | **The whole family beats single-step retrieval on multi-hop** — see §3 (NOVEL, highest stakes) |
| **NEG: agent-feedback loop dropped** (EXP-AF KILL; *multi_hop untested, recall-bound, single-corpus*) | Confidence-/uncertainty-gated iterative: **FLARE, DRAGIN, SeaKR, Self-DC** | FLARE, IRCoT, no-retrieval; DRAGIN beats FLARE/IRCoT; SeaKR beats DRAGIN | SeaKR 2Wiki 36.0 F1 vs DRAGIN 30.0; HotpotQA 39.7 vs 34.2 | n/a (we killed it) | **In-paper, iterative-feedback signals do pay** — but on hosted LLMs and untested-by-us on multi_hop → V-3 |
| **NEG: route on internal `ce_score`** (no agent round-trip) | Internal-signal gating ≈ SeaKR (hidden-state uncertainty), FLARE (token prob) — but *engine-internal*, not LLM | (as above) | (as above) | **Validated internally** — EXP-Fr/VoI: free `ce_score` AUC 0.667 beat cheap-agent 0.545; EXP-AF: even frontier agent didn't beat it | None — internal `ce_score` is *cheaper and better* than the agent round-trip on current substrate |

---

## §2 BETTER-THAN bucket — surveyed methods that beat the FathomDB-resembling approach on a shared benchmark

1. **HippoRAG 2 beats dense retrieval on MuSiQue — the cleanest shared-benchmark loss for our (refuted) graph arm.**
   *Verified this pass (arXiv:2502.14802v1, Table 2):* MuSiQue **F1 48.6** (HippoRAG2) vs **45.7** (NV-Embed-v2,
   best dense) vs **35.1** (orig HippoRAG); 2Wiki **71.0** vs 61.5 dense (HippoRAG2 trails orig HippoRAG 71.8
   on 2Wiki). Reader = **Llama-3.3-70B-Instruct**, retriever = **NV-Embed-v2 (7B)**.
   - FathomDB's MuSiQue (Gate-2, m1, ≥3-hop subset n=144, **hosted gpt-5.4 reader**): best arm `passage_dense`
     **F1 0.487**, `fused` 0.450, `ppr_fusion` 0.410 (graph arm refuted, ΔF1 −0.0405 [−0.116, +0.031]).
   - **Read carefully:** our 0.487 ≈ HippoRAG2's 0.486 is a *coincidence* across different subsets (our ≥3-hop is
     harder), different readers, different embedders. The load-bearing point is directional: **a graph-PPR method
     beats dense on MuSiQue, with an LLM-extracted-KG mechanism we never tested.** Our NO-GO refuted entity-
     co-occurrence BFS, not LLM-KG-triples + PPR. → TEST-ONLY (V-6), report the number before re-closing.
   - Survey discrepancy noted: survey cited HippoRAG2 MuSiQue "44.8→51.9"; the v1 main table reads 35.1→48.6.
     Treat the *direction* as firm, the exact deltas as version/table-dependent.

2. **MoR's per-query weighted blend beats fixed fusion — pressure on our static RRF.** Survey-cited:
   MoR **+3.9% nDCG@20 over GritLM-7B**, **+10.8% over best unsup. component**, **Route-Oracle +13.5%**.
   FathomDB's internal finding is that *static* arm-selection ≈ RRF on LME (per-class, single corpus, recall noise).
   MoR is the published claim that **per-query** arm-weighting *does* add value on BEIR-family corpora — exactly the
   gap V-2 (per-query oracle) + the BEIR head-to-head are built to settle. CPU-feasible (~0.8B). → RECOMMEND (item 2).
   `[survey-cited; Route-Oracle cell confirmed present in survey, not independently re-verified this pass]`

3. **RouterRetriever beats a single embedder on BEIR — +2.1 nDCG@10.** Survey-cited (oracle cell `[UNVERIFIED]`).
   This is the embedder-ceiling story in routing form: a mixture of domain-expert embedders tops one general
   embedder. FathomDB runs one small embedder at the ~0.571 ceiling. → folds into items 2/3.

4. **RAGRouter-Bench TF-IDF+SVM 0.928 macro-F1 beats our 0.768 intent classifier.** Survey-cited + the baseline
   study (arXiv:2604.03455, "both confirmed real" per the survey). **Caveats that blunt this:** (a) ours is an
   explicitly-flagged **lexical proxy lower bound** (no torch/sklearn in-env), not the real classifier; (b) different
   taxonomy — their 3 classes (factual/reasoning/summarization) vs our 5; (c) production intent is **Memex-supplied
   (preference #1)**, so the internal classifier is a fallback. → TEST-ONLY (V-4), low stakes.

5. **The iterative multi-hop family beats single-step retrieval** (IRCoT, Iter-RetGen, ReSP, Search-o1, HippoRAG/2).
   This is simultaneously a better-than (vs our single-shot posture) and the NOVEL bucket — detailed in §3.

> Not in this bucket: Family-1 retrieve-or-not and Family-5 LLM-routing wins are **not on a shared benchmark with
> FathomDB** and are out of posture (LLM-generation / reader-fleet decisions) — see §0 items 7–8 and §4.

---

## §3 NOVEL bucket — surveyed approaches FathomDB does NOT do at all

### 3a. Iterative / agentic multi-hop retrieval — **front and center, the highest-stakes gap**

FathomDB is **single-shot** (one retrieval, then rerank within the pool). It has **no** decompose / interleave /
stop-or-continue loop. The EXP-AF KILL (which dropped the *agent-relevance-feedback* loop) is **(a)** about a
different mechanism (re-rank-the-pool, not re-query), **(b)** *current-substrate provisional*, **(c)** single-corpus
(LME), and **(d)** **explicitly EXCLUDED `multi_hop`**. So this entire family is untested against FathomDB.

| Method | Mechanism (route-between) | In-paper headline (survey-cited) | Transfer to CPU-default? |
|---|---|---|---|
| **IRCoT** | interleave retrieval with CoT, each step guided by prior CoT | +up to 21 retrieval / +15 QA pts (HotpotQA/2Wiki/MuSiQue/IIRC) | Needs an LLM per step → optional-caller-LLM only; **GPU/hosted artifact** if frontier |
| **Iter-RetGen** | feed whole prior output back as next query, fixed N iters | HotpotQA Acc 71.2 vs Self-Ask 64.8 (+6.4); up to +8.6 abs | same |
| **Self-Ask** | decide if a follow-up sub-Q is needed → decompose + per-subq search | +11% abs over CoT on Bamboogle | same |
| **ReSP** | reasoner exits vs next sub-Q from memory queues; dual summarizer | HotpotQA F1 47.2 vs IRCoT 43.1 (+4.1); 2Wiki 38.3 vs 32.4 | same |
| **Search-o1** | LRM (QwQ-32B) retrieves mid-reasoning on uncertainty | multi-hop QA **+29.6% avg EM** over RAG-QwQ | **32B LRM = GPU-heavy**; least transferable |
| **ProbTree / BeamAggR** | query tree; per-leaf closed/open-book by confidence; beam over reasoning paths | "outperforms SOTA" / "+8.5% avg" — **survey-flagged `[UNVERIFIED]` numbers** | LLM-tree-search, heavy |
| **HippoRAG / HippoRAG 2** | KG + PPR single-step associative recall (not iterative, but the multi-hop SOTA) | §2 above (verified) | NV-Embed-v2 7B + Llama-70B = GPU |

**Why this is #1 EV despite the CPU tension:** these methods win precisely by **attacking recall** — they generate
fresh sub-queries that pull in evidence a single shot misses. FathomDB's own diagnosis is that it is **recall-bound**
(EXP-A: multi_session gold-in-pool only 0.20→0.65 even at candidate_k=200; EXP-AF: agent realized ~6% of an 11.8%
headroom because "the answer often isn't in the pool"). An iterative re-query loop is the one surveyed mechanism that
*adds* to the pool rather than reshuffling it. **Verdict: TEST-ONLY behind the maturity guard** — wire a lightweight
IRCoT/Iter-RetGen arm on MuSiQue using the **optional caller-side LLM** (reproduce locally; do not claim the
hosted-LLM headline), as the V-3 `multi_hop` arm. If it pays, it reopens the agentic loop EXP-AF only conditionally
killed.

### 3b. Other novel families (lower EV for FathomDB's posture)

- **Confidence-gated retrieve-or-not** (Self-RAG, FLARE, SKR, Self-DC, Rowen, UAR, Mallen popularity-gate, SlimPLM,
  SeaKR): decide *whether* to retrieve from LM confidence/self-knowledge/popularity. FathomDB always retrieves and
  exposes `ce_score` as an internal signal. These are **LLM-generation** decisions owned by the caller's agent →
  **DON'T-RECOMMEND** as engine work (§0 item 7).
- **LLM-reader routing** (RouteLLM, FrugalGPT, RouterDC, ZOOTER, AutoMix, Hybrid-LLM, Self-Route RAG-vs-LC):
  route over *which generator*. FathomDB doesn't own the reader → **DON'T-RECOMMEND** (§0 item 8).
- **RAGRouter-Bench paradigm routing** (Naive/Graph/Hybrid/Iterative RAG): coarsest strategy routing; adjacent to
  FathomDB's config routing but at paradigm granularity. Useful as an **eval frame** (V-6), not a mechanism to adopt.

---

## §4 Fairness caveats — where the head-to-heads are NOT apples-to-apples

1. **Frontier-LLM / GPU wins vs CPU-default.** Most §2/§3 headline numbers ride a large hosted generator and/or a
   GPU embedder: HippoRAG2 = NV-Embed-v2 (7B) + Llama-3.3-70B (verified); Search-o1 = QwQ-32B; IRCoT/Iter-RetGen/
   ReSP issue an LLM call per step. FathomDB is CPU-default with an **optional** caller-LLM. A paper "win" under that
   stack is **not evidence it transfers** to FathomDB's posture — hence "reproduce locally, don't quote headlines"
   on every router head-to-head (V-6). This is also why §0 items 7–8 are DON'T-RECOMMEND, not just lower-priority.

2. **Re-rank-within-pool vs manufacture-recall.** FathomDB's measured wins (Gate-2, EXP-B′, CE-rerank) operate
   *inside* a fixed candidate pool — so they are capped by the embedder ceiling (~0.571) and the recall envelope.
   Methods that issue new queries (Family 3) or swap embedders (RouterRetriever/MoR/HippoRAG2) attack a *different*,
   binding constraint. Comparing "our re-rank gain" to "their recall gain" is not like-for-like; the right framing is
   **which constraint each attacks**, per §0.

3. **Our distinctive classes have no routing-lit benchmark.** `multi_session`, `temporal`, and `global-sensemaking`
   (3 of FathomDB's 5 intents) have **no router-literature analogue** (survey §e). A router head-to-head is
   apples-to-apples **only on `needle` and `multi_hop`**; the other three must go against memory/sensemaking systems
   (Mem0, GraphRAG) on LongMemEval / LOCOMO / AP-News as a *separate track* — do not fold them into router numbers.

4. **Our intent-classifier number is a flagged lower bound.** The 0.768 is a lexical TF-IDF proxy (no torch/sklearn
   in-env); comparing it to RAGRouter-Bench's 0.928 (different 3-class taxonomy) is directional only. V-4 installs
   the real classifier; and the production path is Memex-supplied intent regardless.

5. **Subset / metric drift on MuSiQue.** FathomDB's MuSiQue is a ≥3-hop *subset* (n=144) with a hosted gpt-5.4
   reader; the papers use full MuSiQue with their own readers. Numerically-close F1s are coincidental, not a tie.

6. **Survey-flagged unverified cells, kept honest here.** ProbTree/BeamAggR per-dataset numbers, and the oracle
   cells for RouterRetriever/Hybrid-LLM/RouterDC/ZOOTER/AutoMix, were `[UNVERIFIED]` in the survey and are **not**
   upgraded to firm claims in this review. The two cells verified *this pass* are HippoRAG2's MuSiQue/2Wiki F1.

---

## Cross-reference to the V-gate (which recommendation maps where)

| Recommendation | V-gate item | New experiment or re-test? |
|---|---|---|
| §0-1 iterative multi-hop arm | **V-3** (multi_hop arm) + new | **New experiment** (no iterative loop exists today) |
| §0-2 BEIR per-query arm-routing | **V-2** (per-query oracle) + **V-6** secondary | Re-test of the arm-switching thesis |
| §0-3 stronger embedder | EXP-M4 re-open trigger (separate gate) | Out-of-router; separately gated |
| §0-4 HippoRAG2 KG+PPR | **V-6** multi-hop head-to-head | Re-examination (report number) |
| §0-5 Adaptive-RAG framing | **V-6** primary | Head-to-head (reproduce locally) |
| §0-6 real intent classifier | **V-4** | Re-test (already scheduled) |
| §0-7, §0-8 (retrieve-or-not / LLM-routing) | — | Out of scope (DON'T-RECOMMEND) |
