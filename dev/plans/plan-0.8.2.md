# FathomDB 0.8.2 — Plan: M1 multi-hop answer-accuracy harness (graph adjudication, step 1 of 3)

> **What this version is.** 0.8.2 is the **first of three versions that fairly adjudicate
> "do graphs help where they're supposed to."** The 0.8.1 "beat BM25" investigation reached a
> robust NEGATIVE on LongMemEval *needle-recall* with a raw-BFS graph arm
> (`runs/0.8.1-beat-bm25-report.md`; [[graph-arm-doesnt-beat-bm25-pivot]]). That result is narrow:
> it measured the **memory/fact-on-edge** structure on the **graph-disfavored axis** (first-stage
> recall) with **disfavored seeding** (the graph's own FTS, not the lexical top-K). It says nothing
> about multi-hop **answer accuracy** or sensemaking — the axes where graphs are *claimed* to help —
> because we never measured those and lacked the instruments. 0.8.2–0.8.4 build the instruments:
>
> - **0.8.2 — M1:** multi-hop **answer accuracy** on MuSiQue with a **lexically-seeded PPR arm**
>   (this plan). The cheapest fair test; reuses the existing extractor + graph + FTS5 + answerer.
> - **0.8.3 — M2:** the full multi-hop study (+2Wiki +MultiHop-RAG, +IRCoT iterative arm, +R5 PRF).
> - **0.8.4 — S1:** the GraphRAG **sensemaking** paradigm (Leiden communities + community summaries,
>   BenchmarkQED-style LLM-judge eval) — a *different* structure on data suited to it.
>
> **✅ OUTCOME — M1 EXECUTED → verdict NO-GO (robust), HITL-signed 2026-06-19.** This plan ran to
> completion (Slices 0→4→{5∥10}→15→20, plus engine slices **E1/E2** that implemented a real CE reranker
> after the signed `fused+rerank` comparator was found to be a stub, and a resilient-by-construction
> priced harness after a gemini provider cap — HITL-approved deviations, see `runs/STATUS-0.8.2.md`).
> **Result:** the lexically-seeded PPR arm fused with BM25 does **not** beat fused-RRF on MuSiQue
> multi-hop QA — **≥3-hop ΔF1 −0.0405, CI [−0.116, +0.031]** (n=300, reader **gpt-5.4**, not the
> gemini-3.1-pro this plan named — the reader pivoted after a $25 provider cap), CI upper **< +0.04**
> materiality, per-hop uniformly negative, `passage_dense` (0.487) strongest. **No stage 2; redirect
> 0.8.3 → index-key enrichment + passage-dense** ([`../roadmap/0.8.3.md`](../roadmap/0.8.3.md)).
> Findings: [`runs/0.8.2-m1-FINDINGS.md`](runs/0.8.2-m1-FINDINGS.md); board:
> [`runs/STATUS-0.8.2.md`](runs/STATUS-0.8.2.md). The slice contracts below are retained as the
> as-built record.

Ladder shape + reserved-gap policy: reuse [`0.8.1-plan.md`](0.8.1-plan.md) §"Ladder".
Process: [`../design/orchestration.md`](../design/orchestration.md) (three-role separation,
codex §9, worktrees §11). Slice prompts generate from
[`prompts/0.8.0-SLICE-TEMPLATE.md`](prompts/0.8.0-SLICE-TEMPLATE.md) (version-neutral).
Roadmap home: [`../roadmap/0.8.2.md`](../roadmap/0.8.2.md). Re-sequencing of the items 0.8.2
*used* to hold (R3b, portable-DB vector guard) → [`../roadmap/0.8.5.md`](../roadmap/0.8.5.md).

**Research basis (validated 2026-06-16).** A focused literature pass confirmed this sequencing,
with one correction folded in below:

- **Step 1 must NOT be the raw BFS arm.** Uniform BFS is the documented anti-pattern (subgraph
  blow-up + hub drift — exactly our entity-co-mingling failure). The principled lightweight form is
  **HippoRAG-style lexically-seeded Personalized PageRank, fused with BM25** (HippoRAG, NeurIPS 2024,
  arXiv:2405.14831; HippoRAG 2, arXiv:2502.14802). PPR strictly dominates BFS, so if PPR-fusion can't
  beat the baseline, BFS never would have — a clean adjudication.
- **Honesty flag (carry it).** Against a *strong* baseline the expected outcome is **near-tie to
  modest loss**: graph wins in the literature are single-digit F1, conditional on genuine
  entity-bridging, and shrink to ~0 vs a strong dense baseline (GraphRAG-Bench, ICLR'26
  arXiv:2506.05690 "GraphRAG frequently underperforms vanilla RAG"; "RAG vs GraphRAG" arXiv:2502.11371
  "complementary strengths, not a consistent winner"). M1 is worth running because it converts *"we
  tried a weak BFS arm"* into *"we tried the literature's best lightweight graph method and it still
  didn't win"* — the result that actually earns the index-key-enrichment pivot. The proven multi-hop
  lever is **iterative retrieval (IRCoT, ACL 2023 arXiv:2212.10509), not graph topology** — it lands
  in 0.8.3, not here.

---

## 0. Goal (M1)

**Fairly adjudicate whether a graph index improves multi-hop *answer accuracy* (EM/F1), at minimal
new infrastructure, by reusing what already exists.** Concretely: build a per-question graph over
MuSiQue paragraphs with the existing Qwen3.6-27B extractor, run a **lexically-seeded PPR arm fused
with BM25**, and measure EM/F1 **broken out by hop count (2/3/4)** against a **strong** baseline
(BM25 + passage-level dense + fused) using the **same** answerer across all arms.

**This is an eval-harness initiative, not an engine change.** PPR runs in the Python harness over the
graph read out of the engine (canonical_nodes/edges via the SDK), like `eval/graph_arm_recall.py` and
`eval/r6_index_key_enrichment.py`. **Footprint invariant holds: CPU-only, no-API at the library
boundary, 1-bit-safe; the only LLM seams are the offline extractor (local, $0) and the answerer.**

**Pre-registered primary endpoint (frozen at Slice 0, before any priced run):** paired
**ΔEM / ΔF1 = (PPR-fusion) − (best baseline)** on the MuSiQue-Ans **answerable** set, reported
**per hop count**, with the 2→3→4-hop **dose-response** as the load-bearing signal (does graph benefit
*grow* with hops?). The unanswerable contrast set is a confident-wrong-answer guard, not a primary.

**Pre-registered decision rule (frozen at Slice 0):**

- **GO → 0.8.3 (M2 full study):** PPR-fusion shows a material, dose-responsive lift on ≥3-hop
  answerable EM/F1 over the strong baseline at adequate power (see Slice 5 MDE).
- **NO-GO → record the negative, redirect to index-key enrichment:** flat-or-negative ⇒ graph
  *topology* adds no multi-hop answer signal on this corpus — the strongest fair version of the graph
  hypothesis, refuted. 0.8.3/0.8.4 may still proceed (they test *different* structures/axes), but M2's
  graph-traversal arm inherits a "prior: negative" framing.

---

## 1. Why MuSiQue, and what "fair" requires

- **MuSiQue-Ans, distractor setting (~20 paragraphs/question)** is the arbiter: engineered to defeat
  shortcuts (3× the human–machine gap; ~30-point F1 drop for single-hop/disconnected models vs prior
  sets — MuSiQue, TACL 2022, arXiv:2108.00573). Its **2/3/4-hop split is a built-in dose-response**;
  its **unanswerable contrast set** guards against the graph inducing confident wrong answers. Both
  properties are why a measured lift is *attributable to the index*, not to artifacts (the HotpotQA
  trap). 2Wiki and MultiHop-RAG are **0.8.3** scope, not here.
- **The baseline must be strong (harness requirement, not optional).** Graph wins vanish against a
  strong dense baseline, so the baseline arm is **BM25 ∪ passage-level dense ∪ fused (RRF)** — *not*
  lexical-only. Passage-level dense, **not** whole-doc (R4): whole-doc vectors blur discrimination
  (our own measured failure mode; R4 stays parked in 0.8.5). Measuring graph against a lexical-only
  strawman would be the methodological error that invalidates the verdict.
- **Same answerer, same depth, deterministic.** Every arm feeds the identical answerer
  (gemini-3.1-pro, the 0.8.1 strong reader) the identical top-K passage budget; retrieval is the only
  variable. Greedy/seeded answerer; versioned output JSONs (never overwrite — per the standing rule).

---

## 2. Cross-cutting Definition of Done (binds every M1 slice)

- **Footprint invariant:** CPU-only, no-API at the library boundary, 1-bit-safe. Extraction is the
  local offline Qwen3.6-27B (Airlock vLLM batch, $0); the answerer is the one priced seam.
- **Budget discipline ([[0.8.1-budget-discipline-cheap-validate-and-ledger]]):** before ANY priced
  answerer run, cheap-validate field population + harness wiring with `gemini-2.5-flash-lite`; keep a
  running $ ledger in `runs/STATUS-0.8.2.md`. Strong reader = `gemini-3.1-pro-preview`.
- **Pre-registration:** primary endpoint + decision rule frozen in the Slice-0 design before data;
  no post-hoc endpoint switching. Power/MDE stated before the priced run.
- **Determinism + provenance:** PPR is deterministic (fixed iteration count / tolerance, stable node
  ordering); every run emits a versioned artifact pinned to the MuSiQue corpus hash + extractor model
  id + arm config. Never overwrite a prior run JSON ([[perf-tuning-design-sweeps-not-adhoc]]).
- **Reviews:** design reviewed before build; implementation reviewed (codex §9) before the verdict is
  recorded. TDD for the PPR mechanism + harness (RED sha recorded).
- **No silent caps:** any sampling (question subset, distractor cap, traversal cap) is `log()`-ed in
  the artifact; a truncated run is labelled, not reported as full coverage.

---

## 3. Critical path

`0 (design+pre-register, SIGNED) → 4 (MuSiQue corpus, SHARED) → {5 (strong baseline + answerer e2e =
THE BAR) ∥ 10 (graph build, reuse extractor)} → 15 (PPR-fusion arm = mechanism KEYSTONE)
→ 20 (adjudication run + verdict → GO/NO-GO to 0.8.3)`.

Slice 4 pins the shared corpus once; **Slice 5 and Slice 10 then run in parallel** (baseline harness vs
graph extraction, both reproducing the Slice-4 corpus); Slice 15 joins them; Slice 20 closes on all.

---

## 4. Per-slice contracts

### Slice 0 — Design + pre-registration · `[design-adr]` · depends-on: — · gaps: 1–4

**Objective.** Author and sign the M1 design: the MuSiQue-Ans harness spec, the PPR-fusion mechanism
spec, the **strong-baseline definition**, the **pre-registered primary endpoint + decision rule**, and
the power/MDE plan. Confirms reuse seams (extractor, FTS5, passage-dense, answerer) and the $ ledger.
**Deliverables:** (1) `dev/design/0.8.2-m1-multihop-harness.md` — datasets, arms, endpoint, decision
rule, ablations, bias controls; (2) the falsifiable AC list M1 tests against (Slice 5/15/20 bars);
(3) budget plan (answerer-call count × arms × N, cheap-validate gate); (4) the frozen decision-rule
module + its test (see TDD).
**TDD — yes, a design slice has an executable core.** Pre-registration is only credible if the
GO/NO-GO computation is frozen as *code* now, so Slice 20 cannot post-hoc switch the endpoint
(the plan's explicit anti-post-hoc stance). RED before GREEN.

> **AMENDED 2026-06-16 (HITL — all 6 pre-freeze review amendments adopted;
> `runs/0.8.2-slice-0-prereg-methodology-review.md`).** The original strict-monotonic dose-response gate +
> per-hop-max baseline biased the rule toward the expected NO_GO. The frozen rule below replaces them.

**The frozen endpoint + rule (amended):**

- **Primary endpoint = the POOLED ≥3-hop (hops 3+4) ΔF1** of PPR-fusion vs a **single fixed comparator =
  the `fused+cross-encoder-rerank` arm** (the strongest baseline), via **question-level paired bootstrap**
  (point estimate + BCa CI). Per-hop (2/3/4) ΔF1/ΔEM are pre-registered **secondary** splits.
- **Comparator is fixed, never per-hop max** (removes winner's-curse inflation + the dose-response
  confound). Baseline arm set = {BM25, passage-dense, fused-RRF (**k=60 pinned**), **fused+rerank**}.
- **`decide()` consumes already-computed summary statistics** (the harness runs the bootstrap; `decide()`
  stays pure/deterministic, no RNG). Signature:
  `decide(material, em, trend, confident_wrong, power_ok) -> "GO" | "NO_GO"` where
  `material={"f1_delta","f1_ci_low"}`, `em={"ci_high"}`, `trend={"neg_significant": bool}`,
  `confident_wrong={"increase_significant": bool}`.
- **GO iff ALL:** (1) `material.f1_delta ≥ MATERIAL_F1_LIFT (0.02)` **and** `material.f1_ci_low > 0`
  (pooled ≥3-hop lift material **and** CI excludes 0); (2) `not trend.neg_significant` (**veto only on a
  significantly NEGATIVE** ΔF1-vs-hop slope — flat/positive passes; no strict-monotonic requirement);
  (3) `em.ci_high ≥ 0` (EM **not significantly worse** — CI-banded, not a point-estimate veto);
  (4) `not confident_wrong.increase_significant` (the **unanswerable-set** confident-answer-rate carries
  the confident-wrong role, not EM); (5) `power_ok`. Else **NO_GO**. Non-finite floats **raise** (kept).
- **0.02 is at/above the Slice-5 pooled ≥3-hop MDE** (wording fixed: material threshold ≥ MDE, not below).

**TDD:**

- **(a)** `src/python/eval/m1_decision_rule.py::decide(...)` encodes the rule above; the test pins: GO only
  when all five hold; NO_GO on each single-gate failure (sub-material f1_delta, f1_ci_low ≤ 0, significant
  negative trend, em.ci_high < 0, significant confident-wrong increase, underpowered); a **flat-positive**
  (non-growing but positive, CI>0, powered) case ⇒ **GO** (the old rule wrongly returned NO_GO here — the
  load-bearing regression test); determinism; non-finite ⇒ raise.
- **(b)** Pre-registration schema lint: `0.8.2-m1-multihop-harness.md` carries the **frozen, dated**
  fields (primary-endpoint = pooled-≥3-hop, comparator = fused+rerank, decision-rule, baseline-arms incl.
  RRF k=60, mde-power-plan = **whole-rule power simulation** under flat/monotonic/inverted-U shapes); fails
  RED if any missing/undated.
- Pure-Python — **no `fathomdb`/`scipy`/`networkx` import**. RED sha in `output.json` `tdd_evidence`.
  **Slice 20 imports `decide()`; may not redefine it.**
**Acceptance bar.** Design `status: decision-ready`; `decide()` + schema lint GREEN; the amended endpoint +
GO/NO-GO rule frozen and dated; the whole-rule power-sim spec handed to Slice 5.
**HITL gate:** the **amended** design + pre-registration signed before any priced run (Slice 20).
**Reserved follow-on (1–4):** a power re-estimate if Slice 5's baseline variance is wider than assumed.

### Slice 4 — MuSiQue corpus acquisition (SHARED prerequisite for 5 ∥ 10) · `[implementation (measurement)]` · depends-on: 0 (SIGNED) · gaps: 1–4 (Slice-0-adjacent)

**Why carved out (orchestrator refinement 2026-06-16).** The plan folded corpus acquisition into Slice 5,
but **both** Slice 5 (baseline) and Slice 10 (graph) score on the **same** MuSiQue corpus. To run 5 ∥ 10
truly in parallel without two worktrees authoring a conflicting acquire script, the corpus is a **shared,
once-pinned** prerequisite (the 0.8.1 reproducible-frozen-corpus model). This is a Slice-0-adjacent
follow-on the design revealed (the shared-corpus dependency between 5 and 10).
**Objective.** Acquire MuSiQue-Ans (CC-BY-4.0, `StonyBrookNLP/musique`) via a **deterministic acquire
script** (committed); the raw data is **gitignored**; **pin a `musique_hash`**. Materialize the
**per-question paragraph corpus** (raw text per question — the shared input both 5 and 10 read). Verify
coverage (every sampled question has its ~20 distractor paragraphs; 2/3/4-hop labels present).
**TDD.** RED: a test asserting (a) the acquire script reproduces the pinned `musique_hash` bit-identically,
(b) the per-question paragraph corpus is materialized with hop labels + the unanswerable contrast set.
GREEN: the acquire+materialize runner. Output → `runs/0.8.2-m1-corpus-manifest.json` (hash, counts, paths).
**DoD.** $0 (dataset download only). The acquire script + manifest are committed; raw corpus gitignored +
locally reproducible. `log()` any sampling. X1 n/a; X3 + DOC-INDEX.
**Unblocks:** Slice 5 (FTS5 + embeddings + arms) ∥ Slice 10 (graph build) — both reproduce the corpus from
this committed acquire script + `musique_hash`.

### Slice 5 — strong baseline + answerer e2e over the shared corpus (THE BAR) · `[implementation (measurement)]` · depends-on: 4 · gaps: 6–9

**Corpus.** Reproduce the **Slice-4** corpus locally from its committed acquire script + `musique_hash`
(do **not** re-author acquisition). Build the **FTS5 index + passage embeddings** over it.
**Objective.** Establish the bar: **BM25 ∪ passage-dense ∪ fused(RRF)** retrieval → identical answerer
→ **EM/F1 per hop count (2/3/4)** + unanswerable-set behavior. No graph yet. This is the number the
graph arm must beat.
**TDD.** RED: harness test asserting (a) retrieval+answer pipeline runs over the pinned `musique_hash`,
(b) EM/F1 + hop-stratified metrics emit a structured artifact, (c) the **same answerer/depth** is used
across arms (a parity assertion). GREEN: the runner. Cheap-validate with flash-lite before the priced
baseline pass. Output → `runs/0.8.2-m1-baseline-n{N}.json`.
**DoD.** Power/MDE computed from baseline variance and recorded. **Amendment 4: run a whole-`decide()`-rule
power simulation** — draw paired-bootstrap resamples under ≥3 effect shapes (flat-positive +0.03,
monotonic 2<3<4, inverted-U peaking at 3-hop) at the measured baseline variance and report **P(GO)** for
each; size N (the pooled ≥3-hop cell) so the rule attains **≥0.8 P(GO) under flat-positive +0.03** — not
merely a marginal per-hop MDE. `power_ok=True` only if that holds. Build all four baseline arms incl.
`fused+rerank` (the fixed comparator) and pin RRF **k=60**. `log()` all sampling. X1 n/a; X3 + DOC-INDEX.
**Reserved follow-on (6–9):** passage-dense embedder/model-dim choice if the default underperforms (the
baseline must be *strong* — a weak dense arm invalidates the verdict).

### Slice 10 — Graph build over MuSiQue (reuse extractor) · `[implementation (measurement)]` · depends-on: 4 · gaps: 11–14

**Objective.** Reproduce the **Slice-4** corpus locally (committed acquire script + `musique_hash`), then
reuse the Qwen3.6-27B Airlock-vLLM-batch extractor ($0, local) to build entities +
fact-edges over the MuSiQue paragraph corpus; load into canonical_nodes/canonical_edges; cache
incrementally. **Verify coverage** (per-question graph present, entity/edge counts) the way
`eval/verify_embed_db.py` verifies embeds — drain/terminal status can lie
([[embed-completeness-and-gpu-readiness]]). Disable pii_redact for synthetic extraction (standing rule).
**TDD.** RED: a coverage test asserting every sampled question has a non-empty graph and the
node/edge tables are populated for the pinned corpus. GREEN: the build+load+verify runner.
Output → `runs/0.8.2-m1-graph-coverage-n{N}.json` + cached graphs (versioned path).
**DoD.** $0 (local GPU extraction); $ ledger unchanged. **Note the reuse asset diverges here:**
`graph_arm_recall.py::build_graph_engine` builds edges **with** `body` (the relation-triple text);
M1 must build edges **body-less** to dodge the engine `edge_fact` vector-projection scale bug (the
known 0.8.1 follow-up). The slice prompt must call out this adaptation explicitly (it is the opposite
of the asset's default), or document the workaround used. X3 findings + DOC-INDEX.
**Reserved follow-on (11–14):** extractor throughput tuning only if the build exceeds the time budget —
**design a sweep, do not improvise probes** ([[perf-tuning-design-sweeps-not-adhoc]]).

### Slice 15 — PPR-fusion arm (mechanism KEYSTONE) · `[implementation]` · depends-on: 5, 10 · gaps: 16–19

**Objective.** Implement the lexically-seeded PPR arm and the fusion:

1. **Seed** from BM25/FTS5 top-K → map to graph nodes (entities from the extractor); **weight seeds by
   IDF / node specificity** (FTS5 term stats, free) to suppress hub drift.
2. **Propagate** one Personalized-PageRank pass biased to those seeds (scipy sparse power iteration /
   networkx `personalized_pagerank`); deterministic (fixed tol + iteration cap + stable node order).
3. **Rank** passages by summed PPR mass; **fuse with BM25** (RRF), per HippoRAG-2's lesson that
   graph+lexical fusion beats graph-alone and avoids entity-only context loss.
**TDD.** RED before GREEN, asserting the load-bearing properties: **(a)** determinism (identical
input → byte-identical ranking); **(b)** the **restart→1.0 collapse** — at teleport probability 1.0 the
arm degenerates to seeds ≈ BM25 (the built-in sanity ablation); **(c)** IDF-weighting changes ranking
(knob is live); **(d)** fusion never drops a BM25-top-K passage below the floor it would have alone
(no-regression pin). RED sha recorded in `output.json` `tdd_evidence`.
**DoD.** CPU-only, no-API (PPR in-harness, not in the engine). **Setup prerequisite:** install
`scipy` + `networkx` into `.venv` (absent today — see §6); they are harness-only CPU deps, **not**
linked into the FathomDB library, so the footprint invariant holds. Codex §9 review before Slice 20.
**Reserved follow-on (16–19):** **entity-seeds vs passage-node seeds** granularity ablation, and
**F9 confidence-weighted seeds/edges** (folded from the deferred F9 ADR as an *ablation knob only* —
do not open the F9 slice). Run only if plain PPR-fusion is close to the bar.

### Slice 20 — Adjudication run + verdict (GO/NO-GO → 0.8.3) · `[implementation (measurement)]` · depends-on: 15 · gaps: 21–24

**Objective.** Run all arms — **BM25 / passage-dense / fused / PPR-fusion** — on MuSiQue-Ans, same
answerer, and evaluate the **pre-registered** primary endpoint: ΔEM/ΔF1 per hop count with the 2→3→4
dose-response, plus the unanswerable-set guard. Apply the frozen decision rule → **GO or NO-GO** to
0.8.3.
**TDD.** RED: the verdict harness asserts it computes the *pre-registered* endpoint over the pinned
corpus and emits the decision artifact with the GO/NO-GO call derived mechanically from the rule (no
post-hoc endpoint). GREEN: the runner. Cheap-validate before the priced multi-arm pass.
Output → `runs/0.8.2-m1-verdict-n{N}.json` + a written report `runs/0.8.2-m1-report.md`.
**DoD.** $ ledger finalized. Codex §9 review of the harness + the report's claims (cross-check the
green call against printed per-hop numbers — [[background-exit-masks-real-exit]]). **HITL gate:** the
verdict (and whether to fund 0.8.3's full study) is HITL-signed. X3 findings + DOC-INDEX; update
`runs/STATUS-0.8.2.md` §"decisions" and the 0.8.3 roadmap's "prior" framing.
**Reserved follow-on (21–24):** if PPR-fusion is *borderline* GO, the seed-quality / node-granularity
ablations (Slice-15 follow-ons) decide it before committing 0.8.3 budget.

---

## 4a. Hygiene + engine-prerequisite slices (off the eval ladder)

### Slice E1 — implement the TinyBERT-L-2 CE reranker (engine) · `[implementation]` · depends-on: — · unblocks Slice 5

**Why (HITL 2026-06-18).** Amendment 6's `fused+rerank` fixed comparator requires a real cross-encoder.
The 0.8.1 R1 slice built the seam/API/feature-gate/RRF-blend but left the model a **stub**
(`CandleCrossEncoder::try_get_loaded()` → `None`; `score()` → `0.0`; `lib.rs:4925,4930` TODOs). HITL chose
to **implement it** rather than revise the comparator. **This is a deliberate, HITL-approved deviation
from M1's "not an engine change" footprint**, scoped to the reranker only; the footprint invariant
(CPU-only, no-network at `rerank_depth=0` / feature-off) is **preserved**.
**Spec:** `dev/design/0.8.1-slice-10-reranker-design.md` (TinyBERT-L-2 CE, Candle BERT, ureq+sha2 lazy
weight fetch cached under `~/.cache/fathomdb/reranker/`, no network unless feature-on AND
`rerank_depth>0` AND weights absent). **Reuse:** `fathomdb-embedder` candle stack + `candle_bge.rs` /
`loader.rs` (BERT load+forward template) — the CE adds (query,passage) pair tokenization + a
classification head over CLS. Pin a real model (e.g. `cross-encoder/ms-marco-TinyBERT-L-2-v2`) +
sha256.
**TDD (Rust, cargo — NOT via the Python .so):** RED `cargo test -p fathomdb-engine --features
default-reranker` asserting a `rerank_depth>0` call **reranks** (loads weights, blends CE+RRF, changes
order) vs the existing depth=0 identity test (`rerank_fused_soft_fallback_preserves_fused_order` must
still pass); preserve the no-network contracts. GREEN: implement `try_get_loaded()` + `score()` (+ Cargo
feature wiring to expose candle to the engine reranker). RED sha recorded.
**DoD.** `cargo test -p fathomdb-engine --features default-reranker` green; depth=0 + feature-off remain
byte-identical/no-network; clippy clean. Rust source merged to local `main`. **Then the orchestrator
(main thread) rebuilds the canonical extension `--features …,default-reranker`** (preserving the current
feature set) + functionally verifies `search(rerank_depth>0)` reranks, before re-spawning Slice 5.
X3: DOC-INDEX + update the reranker design doc's status (stub → implemented).
**Do NOT** `maturin develop` from the worktree ([[agent-worktree-stale-base-trap]]); the extension build
is the main thread's job after merge.

## 4a-bis. Hygiene slices (off-ladder, parallel — no dependency on the M1 ladder or the ◆ sign-off)

### Slice H1 — restore repo-wide `pyright -p src/python` to 0/0 · `[implementation]` · depends-on: —

**Why.** Surfaced during Slice 0 codex §9: the repo-wide pyright baseline is **not** 0/0 (contradicting
the SLICE-TEMPLATE standing baseline). **9 pre-existing errors** at `b304147`, in files M1 never touches
— pure tech debt, blocks nothing in M1, but should be cleared so future slices' `pyright` self-check is
meaningful again.
**The 9 errors (3 clusters):**

- 6× `eval/p0a_batch_e2e.py::score_e2e` `answers: dict[str, Optional[str]]` — `dict` is invariant, so
  `dict[str, str]` call sites fail. **Fix:** widen the param to `Mapping[str, Optional[str]]` (covariant
  value type; pyright's own suggestion). Touches the signature + the import.
- 2× `eval/r6_index_key_enrichment.py:237,239` — `Operator "-"` on `float | None` operands. **Fix:**
  narrow/guard the Optionals (assert-not-None or default) before the subtraction.
- 1× `tests/test_p0a_batch_e2e.py:267` — `None` passed to `output_file_id: str`. **Fix:** make the
  param/`None` consistent (Optional the param, or pass a value).
**Bar (pyright IS the test here).** RED = the current 9 errors; GREEN = `pyright -p src/python` reports
**0 errors / 0 warnings** AND `pytest` for the touched test files still passes (no behavior change). No
runtime/logic change beyond typing/guards. Single merge. X3: note in DOC-INDEX only if a doc changes
(likely none). Codex §9 light review (typing-only diff).
**Out of scope:** any M1 file (`m1_decision_rule.py`, the design doc), any new feature, any non-typing
refactor.

## 5. What M1 deliberately does NOT do (lives in 0.8.3 / 0.8.4)

- **2WikiMultiHopQA + MultiHop-RAG, IRCoT iterative-retrieval arm, R5 vector-PRF** → **0.8.3 (M2)**.
  IRCoT is the *proven* multi-hop lever; M1 isolates the *graph-topology* question first so M2 can
  attribute gains to iteration vs structure.
- **GraphRAG sensemaking (Leiden communities + summaries, BenchmarkQED LLM-judge)** → **0.8.4 (S1)**.
  Different structure, different axis; a likely-negative M1 must **not** prejudge it.
- **Bundling a CPU extractor (R3b), whole-doc dense (R4), tunable-b FTS5, the portable-DB vector
  guard** → **0.8.5**, with R3b explicitly **gated on the M1–S1 verdict** (don't productize a graph
  that hasn't earned it).

## 6. Reuse inventory (no new infra unless listed)

> **Paths.** The eval harness lives at **`src/python/eval/`** (run via the repo `.venv`, which
> carries the `fathomdb` SDK + numpy). All `eval/...` references below are `src/python/eval/...`.
> The answerer is **env-driven** (`AirlockAnswerer`/`LLMAnswerer`, model id via `R2_ANSWERER_MODEL`
> over the Airlock proxy — no `litellm` needed), so "gemini-3.1-pro-preview" is the configured value,
> not a hard-coded import.

| Need | Reused asset |
|---|---|
| Entity/fact extraction | Qwen3.6-27B via Airlock vLLM batch (`enable_thinking:false`, conc=8 knee, mt=3072), $0 local |
| Graph substrate | canonical_nodes / canonical_edges + SDK read path (0.8.1 Slice 15/20) |
| Lexical arm | FTS5/BM25 (existing) |
| Dense arm | passage-level embedder (existing; **not** whole-doc R4) |
| Reranker arm | the **0.8.1 R1 CPU cross-encoder** (`rerank_fused`) — the `fused+rerank` arm is the fixed primary comparator (amendment 6) |
| Answerer | gemini-3.1-pro-preview (0.8.1 strong reader); cheap-validate = gemini-2.5-flash-lite |
| Harness patterns | `src/python/eval/graph_arm_recall.py`, `r6_index_key_enrichment.py`, `r2_parity_eval.py`, `verify_embed_db.py` |
| New (small) | MuSiQue acquire+hash; PPR pass (~tens of lines); EM/F1 + **per-hop(2/3/4) strata** scorer |
| New harness deps | **`scipy` + `networkx`** — currently ABSENT from `.venv` (only numpy + the SDK are present); add at Slice 15 setup. **Harness-only CPU deps; NOT linked into the FathomDB library → footprint-invariant-safe.** |
