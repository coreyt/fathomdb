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

`0 (design+pre-register) → 5 (MuSiQue corpus + strong baseline + answerer e2e = THE BAR)
→ 10 (graph build over MuSiQue, reuse extractor) → 15 (PPR-fusion arm = mechanism KEYSTONE)
→ 20 (adjudication run + verdict → GO/NO-GO to 0.8.3)`.

Slice 5 and Slice 10 are independent off Slice 0 (baseline harness vs graph extraction) and may run
in parallel; Slice 15 joins them; Slice 20 closes on all.

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
(the plan's explicit anti-post-hoc stance). This is the design slice's analogue of RED→GREEN —
**falsifiable, not "working against an existing ADR."** RED before GREEN:
- **(a) Frozen decision rule as a pure function.**
  `src/python/eval/m1_decision_rule.py::decide(deltas_by_hop, power_ok) -> "GO" | "NO_GO"` mechanically
  encodes the frozen rule. The test pins the truth table at the boundaries: flat-or-negative ⇒ NO_GO;
  positive-but-not-dose-responsive ⇒ NO_GO; dose-responsive (2→3→4 growing) ≥3-hop EM/F1 lift but
  **underpowered** ⇒ NO_GO; dose-responsive **and** adequately powered ⇒ GO. Determinism: same input →
  same verdict.
- **(b) Pre-registration schema lint.** A test asserts `0.8.2-m1-multihop-harness.md` carries the
  required **frozen, dated** fields (primary endpoint, per-hop(2/3/4) strata, decision rule, MDE/power
  plan); it fails RED if any is missing or undated.
- Pure-Python — **no `fathomdb` / `scipy` / `networkx` import** (runs anywhere, no native-extension or
  `.venv`-binding dependency). RED sha recorded in `output.json` `tdd_evidence`. **Slice 20 imports
  `decide()`; it may not redefine the rule.**
**Acceptance bar.** Design `status: decision-ready` with a falsifiable spec each downstream slice can
test; the decision-rule function + schema lint are GREEN; the primary endpoint + GO/NO-GO rule are
frozen and dated.
**HITL gate:** design + pre-registration signed before any priced run (Slice 20).
**Reserved follow-on (1–4):** a power re-estimate if Slice 5's baseline variance is wider than assumed.

### Slice 5 — MuSiQue corpus + strong baseline + answerer e2e (THE BAR) · `[implementation (measurement)]` · depends-on: 0 · gaps: 6–9
**Corpus prerequisite.** Acquire MuSiQue-Ans (CC-BY-4.0, StonyBrookNLP/musique) via an acquire script;
**gitignored**; pin a `musique_hash`. Build the per-question paragraph corpus + FTS5 index + passage
embeddings.
**Objective.** Establish the bar: **BM25 ∪ passage-dense ∪ fused(RRF)** retrieval → identical answerer
→ **EM/F1 per hop count (2/3/4)** + unanswerable-set behavior. No graph yet. This is the number the
graph arm must beat.
**TDD.** RED: harness test asserting (a) retrieval+answer pipeline runs over the pinned `musique_hash`,
(b) EM/F1 + hop-stratified metrics emit a structured artifact, (c) the **same answerer/depth** is used
across arms (a parity assertion). GREEN: the runner. Cheap-validate with flash-lite before the priced
baseline pass. Output → `runs/0.8.2-m1-baseline-n{N}.json`.
**DoD.** Power/MDE computed from baseline variance and recorded; N chosen so MDE < the smallest
graph lift worth chasing (single-digit F1 per the research → N must be non-trivial; sample the dev set
accordingly and `log()` the sampling). X1 n/a (eval harness, no SDK surface); X3 findings + DOC-INDEX.
**Reserved follow-on (6–9):** passage-dense embedder/model-dim choice if the default underperforms (the
baseline must be *strong* — a weak dense arm invalidates the verdict).

### Slice 10 — Graph build over MuSiQue (reuse extractor) · `[implementation (measurement)]` · depends-on: 0 · gaps: 11–14
**Objective.** Reuse the Qwen3.6-27B Airlock-vLLM-batch extractor ($0, local) to build entities +
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
| Answerer | gemini-3.1-pro-preview (0.8.1 strong reader); cheap-validate = gemini-2.5-flash-lite |
| Harness patterns | `src/python/eval/graph_arm_recall.py`, `r6_index_key_enrichment.py`, `r2_parity_eval.py`, `verify_embed_db.py` |
| New (small) | MuSiQue acquire+hash; PPR pass (~tens of lines); EM/F1 + **per-hop(2/3/4) strata** scorer |
| New harness deps | **`scipy` + `networkx`** — currently ABSENT from `.venv` (only numpy + the SDK are present); add at Slice 15 setup. **Harness-only CPU deps; NOT linked into the FathomDB library → footprint-invariant-safe.** |
