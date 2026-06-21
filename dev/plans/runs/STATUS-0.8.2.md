# STATUS — 0.8.2 (M1: multi-hop answer-accuracy harness)

> Live state board for 0.8.2 / M1. Per `dev/design/orchestration.md` §12.5 the **orchestrator owns
> this board** (one docs commit per transition); slice agents never edit it. **Witnesses (git +
> `output.json` + verdict `.md`) win over this cache** on any conflict (§1.5 invariant 1).
> Plan: [`../plan-0.8.2.md`](../plan-0.8.2.md). Roadmap: [`../../roadmap/0.8.2.md`](../../roadmap/0.8.2.md).

## 1. Current state + next action

- **STATE: ✅ M1 CLOSED — verdict NO-GO (robust), HITL-signed 2026-06-19.** The lexically-seeded PPR
  graph-fusion arm does **not** beat fused-RRF on multi-hop QA. Clean valid run: reader `gpt-5.4`,
  completeness **1.0**, 0 errors, **$2.50**. **Primary ≥3-hop ΔF1 (ppr_fusion − fused) = −0.0405, CI
  [−0.116, +0.031]** (n=144) — point-negative + CI upper below the +0.04 materiality bar ⇒ a material graph
  win is ruled out. 5-arm ≥3-hop F1: dense **0.487** > fused 0.450 > fused_rerank 0.415 > ppr_fusion 0.410 >
  bm25 0.370. `decide()`=NO_GO; **no stage 2**. Findings: `0.8.2-m1-FINDINGS.md`; closure:
  `0.8.2-slice-20-output.json`; verdict: `0.8.2-m1-verdict-gpt54.json`.
- **NEXT (0.8.3):** redirect to the two **non-graph** levers (not graph traversal) per
  [[graph-arm-doesnt-beat-bm25-pivot]] / [[m1-graph-arm-nogo-registered-n300]]: **D1 passage-dense** (the
  single strongest M1 arm, 0.487) and **D2 fielded index-key enrichment**. Roadmap `../../roadmap/0.8.3.md`,
  plan `../plan-0.8.3.md` (DRAFTED 2026-06-20; **EXTENDED 2026-06-21** by the capability-status-report review,
  `0.8.x-capability-status-report.md`). **New ladder:** `0 (design+pre-register+F5-scope+fallback) → 5 (D1a
  CLS-fix+eu7+corrected baseline+latency) → 10 (NEW $0 gates: embedder-ceiling probe ∥ D2 content-at-scale
  proxy) → {15 D1b PRF [gated by the embedder probe] ∥ 20 D2 fielded-FTS engine [gated by the D2 proxy]} →
  25 D2 build+verdict+latency)`, plus a **parallel HITL-approved D0 measurement-unblock track** (D0a gold-
  corpus re-pin + answerer seam; D0b Mem0-OSS baseline) that closes the capability report's #1 gap — no
  end-to-end agentic-memory parity number vs Mem0/Zep. ADJ-6..10: the two $0 gates de-risk both engine
  builds *before* commitment, D0 makes the parity number producible, latency-regression is a DoD gate on
  read-path slices, D1 is reframed corrected-fused-vs-corrected-dense (composition co-primary), and a both-
  null fallback is pre-registered. **The ADJ-6..10 measurement + prerequisite work is HITL-approved
  (2026-06-21); engine commitment (5/20) + priced runs still wait on the Slice-0 gate.**
- **Reader pivot (2026-06-19):** the **gemini** provider hit a hard **$25 airlock budget cap** mid-effort
  (3 invalid/partial runs: a 429 storm, a process-window death, a relaunch loop) → pivoted to **gpt-5.4**
  (separate provider, ~3.5× cheaper/call, 0 errors) after rebuilding the harness to be **resilient by
  construction** (auto-resume from atomic incremental checkpoint, failure≠abstention, 429 backoff,
  completeness validity guard; codex §9 [P2] resume-abstention fix). Direction is reader-robust (gemini
  partial, cheap gpt-5.4, full gpt-5.4 all agree). **pwrfix** landed: power-sim aligned to 0.04 (finding:
  rule underpowered even at N=1165, P(GO)=0.45 — moot under a robust reject).
- **CE reranker was a STUB** (`try_get_loaded()`→None, `score()`→0.0; `lib.rs:4925,4930` TODOs) → so the
  signed `fused+rerank` comparator wasn't real. **HITL (2026-06-18) chose to IMPLEMENT it** (engine slice
  E1) rather than revise amendment 6 — a deliberate, HITL-approved deviation from M1's no-engine-change
  footprint (footprint-preserving: no network at feature-off / `rerank_depth=0`). Amendment 6 **upheld**.
- **DONE:** E1 CLOSED (reranker real, Rust reorder-test proven). **Orchestrator rebuilt the canonical
  extension** `--features pyo3/extension-module,default-embedder,fathomdb-engine/default-reranker` (CPU; no
  `embed-cuda` — no `nvcc`, CPU-pinned embedder unaffected) and **functionally verified** the CE runs
  through the `.so` (weights download + scores change; the Rust test proves it promotes the relevant
  passage). **Fragility:** a plain `pytest` (no `FATHOMDB_TESTS_NO_REBUILD=1`) would rebuild the `.so`
  WITHOUT `default-reranker` and silently drop the reranker — slices must use `FATHOMDB_TESTS_NO_REBUILD=1`.
- **◆ Slice 5 at budget checkpoint — HOLD merge, HITL decision (branch `7037523`, unmerged).** Harness +
  cheap-validate + bounded priced pilot done ($2.39 spent; full pass NOT run). **codex §9 [P1]: the
  `fused_rerank` comparator reranked the engine's capped text-only pool, not the in-harness fused pool →
  its 0.314 is invalid; correctly building it needs a standalone rerank API (more engine work).** [P2]
  power inverted-U mis-centered. **Valid BAR (clean arms): bm25 0.278 · dense 0.363 · fused-RRF 0.376.**
  Underpowered for +0.03 (P(GO)≈0.6 @ full 1165-corpus; feasible only at ρ≥0.7). Full feasible pass $27.54.
- **◆ HITL decided (2026-06-18):** (1) **comparator** — **build the standalone-rerank API (E2)** so
  `fused_rerank` reranks the real fused pool + fix the [P1]; get FRESH valid fused+rerank results (defer the
  fused-RRF-vs-fused+rerank comparator call until valid data); budget **<$30**. (2) **direction** — **raise
  the detectable effect size** so the 1165-corpus is adequately powered (set MATERIAL_F1_LIFT to the
  corpus-feasible MDE — a pre-reg revision; the new value is HITL-confirmed at the next gate).
- **E2 CLOSED + extension rebuilt + VERIFIED:** `fathomdb.rerank` reorders `[1,2,3]→[2,1,3]` (promotes the
  relevant passage) through the `.so`; NaN→`WriteValidationError`; built via plain `maturin develop`
  (pyproject carries `default-reranker` now — NO_REBUILD fragility RESOLVED). The CE genuinely reranks.
- **Slice 5 fix-1 DONE + merged (`57f7464`):** corrected `fused_rerank` (reranks the fused pool via
  `fathomdb.rerank`) → **THE BAR pooled ≥3-hop F1: bm25 0.239 · dense 0.262 · fused 0.306 · fused_rerank
  0.306 (TIED)**. recall@10: dense 0.836 · fused 0.759 · fused_rerank 0.753. The CE reranker neither helps
  nor hurts multi-hop (valid measurement). Pilot $2.58, cum ~$4.97.
- **◆ HITL (2026-06-19):** comparator = **"best-of per the recall signal"** (discuss) → answer-F1 endpoint
  favors fused-RRF (tied w/ fused_rerank); recall@10 favors dense but that's a per-passage proxy — adding
  an **all-bridges@K** metric (fix-2) to decide on the multi-hop-correct retrieval view. threshold =
  **decide after fix-2 re-confirms the BAR**.
- **codex §9 fix-1 [P2] = real engine finding:** the harness CLS pooling is CORRECT for bge-small; the
  **engine `CandleBgeEmbedder` DEFAULTS to `Mean`** (its own comment says bge is CLS + BGE docs warn Mean
  degrades) → a latent **product bug** in the shipped default embedder. Flagged for a separate slice; the
  eval is unaffected (harness uses CLS). So the BAR STANDS — no re-pilot.
- **IN-FLIGHT: Slice 5 fix-2 ($0)** — declare numpy [P1]; lock/document CLS + flag the engine bug [P2]; add
  the all-bridges@K metric. Then HITL finalizes comparator + MATERIAL_F1_LIFT on solid numbers → Slice 15/20.
- **Next action (◆ HITL gate — STOP):** the pre-freeze methodology review (orchestrator-directed)
  returned **NOT sound to freeze as-is** (`runs/0.8.2-slice-0-prereg-methodology-review.md`): the strict
  monotonic dose-response gate + per-hop-max baseline bias the rule toward the expected NO_GO. **4
  load-bearing amendments + 2 advised.** Orchestrator concurs. **Recommendation to HITL: do NOT sign;
  approve the amendment set → spawn a Slice 0-revision (design §4 + `decide()` + tests) → re-review →
  then sign.** Slices 5/10 remain gated behind the (amended) sign-off.
- **Blocked on:** nothing engine-side. Slice 0 has no priced run; the first ◆ HITL gate is the
  Slice-0 design+pre-registration sign-off (must land *before* any priced answerer run at Slice 20).

- **◆ HITL CONFIRMED (2026-06-19):** comparator = **fused-RRF**; threshold = **MATERIAL_F1_LIFT 0.04**.
  Data-backed by the \$0 bridge-vs-answer diagnostic (`runs/0.8.2-m1-bridge-vs-answer-diagnostic.md`):
  complete-bridge retrieval drives F1 (+0.442); conditional on all bridges fused-RRF answers BEST (0.552)
  / dense WORST (0.464) → fused's edge is a real composition advantage, not noise; dense retrieves best but
  answers worst (distractor composition). **IN-FLIGHT: Slice 0-rev2** ($0 pre-reg re-freeze: comparator→
  fused-RRF + MATERIAL_F1_LIFT→0.04 + tests) → then Slice 15 (PPR arm, Δ vs fused-RRF).

## 2. Slice scoreboard

| # | Slice | Type | Depends | State | Witness |
|---|-------|------|---------|-------|---------|
| 0 | Design + pre-registration (**+ TDD: frozen decision-rule module**) | `[design-adr]` | — | **CLOSED (amended); ◆ HITL sign-off ready** | revision+fix merged `2348f95`; codex §9 PASS; 33/33 green; all 6 amendments + trend-test lint |
| 4 | **MuSiQue corpus acquisition (SHARED prerequisite for 5 ∥ 10)** | impl (measurement) | 0 ✅ | **CLOSED** | merged+fix-1 `df1c879`; `musique_hash 3cff37fd…`, reproduce-stable, 8/8 tests; orchestrator-verified |
| 5 | strong baseline + answerer e2e over shared corpus (THE BAR) | impl (measurement) | 4 ✅, E1 ✅, E2 ✅ | **CLOSED** (through fix-2) | BAR pooled≥3hop F1: bm25 .239/dense .262/**fused .306**/fused_rerank .306; all-bridges@10 dense .52 best; $0 diagnostic → comparator=fused-RRF |
| — | **Tracked bug (separate slice):** engine `CandleBgeEmbedder` defaults to `Pooling::Mean` for CLS-pooled bge-small → degrades shipped dense retrieval (BGE docs). Eval unaffected (harness=CLS). | bug | — | FLAGGED | `candle_bge.rs:229`; candidate for its own slice (touches stored-vector compat) |
| 10 | Graph build over MuSiQue (reuse extractor) | impl (measurement) | 4 ✅ | **CLOSED** (fix-1 `f8bc631`) | n=300 graph, coverage 1.0, 50.6k entities/51.2k body-less edges, hash-validated; cache preserved to canonical for Slice 15 |
| E1 | Implement TinyBERT-L-2 CE reranker (engine; unblocks 5) | impl | — | **CLOSED** (fix-1 `b577b11`) | real reorder 3/3 + identity both-states green (orchestrator-verified); codex [P2] + a feature-on test regression fixed |
| E2 | Standalone rerank SDK API (`fathomdb.rerank`) over arbitrary passages | impl | — | **CLOSED** (fix-1 `f2c910f`) | Python-verified reorders [1,2,3]→[2,1,3]; non-finite→WriteValidationError; default-reranker now in dev/test build (durable) |
| 0-rev2 | pre-reg re-freeze: comparator→fused-RRF + MATERIAL_F1_LIFT→0.04 | design-adr+TDD | — | **CLOSED** `0f2129cb` | HITL-confirmed, data-justified; 34/34 tests; f1_delta=0.03→NO_GO |
| 15 | PPR-fusion arm (mechanism KEYSTONE) | impl | 5✅,10✅ | **CLOSED** `f30bca3` | codex PASS; graph arm verified live ≠ BM25 (propagates, not silent-collapse); 4 properties hold |
| 20 | Adjudication verdict (stage 1, ~$10) | impl (measurement) | 15✅ | **IN-FLIGHT** | 5 arms over the 299-graph → ΔF1 vs fused-RRF; decide()=NO_GO/underpowered; read effect size → stage-2 call |
| H1 | Restore repo-wide `pyright -p src/python` to 0/0 (off-ladder hygiene) | impl | — | **CLOSED** | merged `74999b3`; pyright 0/0/0 (orchestrator-verified); 20 tests green; typing-only |

Critical path: `0 → {5 ∥ 10} → 15 → 20`. Slices 5 and 10 are independent off 0 (baseline harness ∥
graph extraction) and may run in parallel.

## 3. $ ledger (budget discipline — cheap-validate before every priced run)

| Date | Slice | Run | Model | $ | Note |
|---|---|---|---|---|---|
| 06-18 | 5 (orig) | cheap-validate + pilots | flash-lite + gemini-3.1-pro | ~2.39 | incl. a crashed N=100 pilot (~$0.95, no artifact) — fixed with answer-phase resilience |
| 06-18 | 5 fix-1 | cheap-validate + bounded re-pilot (N=100) | flash-lite + gemini-3.1-pro | ~2.59 | valid BAR; 399 calls, 1 err |
| | | | **cumulative** | **~4.97** | of the HITL <$30 ceiling for the re-measure. Full graph adjudication (Slice 20) = separate gate (~$30 ≥3-hop). |

**Running total: $0.00.**

## 4. Reuse-asset / environment readiness (from the scoping pre-flight 2026-06-16)

- ✅ Answerer seam: `AirlockAnswerer`/`LLMAnswerer` (`src/python/eval/`), env-driven model, identical-answerer protocol.
- ✅ Extractor seam: `graph_arm_recall.py` (Qwen3.6-27B Airlock vLLM batch, $0).
- ✅ EM/F1 scorer primitives: `r2_parity_eval.py` (`score_answer`/`normalize_answer`/`_match`) — **per-class**; **per-hop(2/3/4) strata is new** (Slice 5/20).
- ⚠️ **scipy + networkx ABSENT from `.venv`** — Slice 15 setup adds them (harness-only CPU deps; footprint-safe).
- ⚠️ Eval path is **`src/python/eval/`**, not `eval/` — fixed in the plan reuse inventory.
- ⚠️ Slice 10 builds edges **body-less** (opposite of `graph_arm_recall.py`'s default) — called out in the plan DoD.

## 5. Outstanding worktrees

None for 0.8.2 (Slice 0 worktree + branch removed at close). *(A stray `/tmp/fdb-g0-…` worktree from a
prior 0.8.0 session exists — out of 0.8.2 scope.)*

## 5a. Open HITL gate — ◆ AMENDED design + pre-registration sign-off (BLOCKS 5/10)

Package for coreyt. **All 6 pre-freeze amendments landed + codex §9 PASS.** Sign to unblock Slices 5 ∥ 10
(and authorize Slice 5's priced baseline run + the whole-rule power sim).
- **Design doc:** `dev/design/0.8.2-m1-multihop-harness.md` (`status: decision-ready`, frozen block
  AMENDED 2026-06-16). Frozen-as-code: `src/python/eval/m1_decision_rule.py::decide()` (33 tests, codex PASS).
- **Frozen primary endpoint:** **pooled ≥3-hop (3+4) ΔF1** of PPR-fusion vs a **single fixed comparator =
  the `fused+rerank` arm**, via **question-level paired bootstrap** (point estimate + BCa CI). Per-hop
  (2/3/4) = pre-registered secondary feeding the trend read.
- **Frozen decision rule (`decide(material, em, trend, confident_wrong, power_ok)`):** GO iff (1) pooled
  ≥3-hop ΔF1 ≥ **0.02** AND its **CI lower bound > 0**; (2) **not** a significantly **negative** ΔF1-vs-hop
  slope (flat/positive passes — no strict monotonicity); (3) ΔEM CI upper bound ≥ 0 (CI-banded); (4) the
  **unanswerable-set** confident-answer rate not significantly raised; (5) adequately powered. Else NO_GO.
  Non-finite inputs raise. Slice 20 imports it, may not redefine it.
- **Strong baseline:** {BM25, passage-dense, fused-RRF (**k=60**), **fused+rerank** (0.8.1 R1 cross-encoder)};
  same answerer (`gemini-3.1-pro-preview`) all arms.
- **Power:** Slice 5 runs a **whole-rule power simulation** (flat-positive/monotonic/inverted-U shapes);
  `power_ok` only if rule-level **P(GO) ≥ 0.8 under flat-positive +0.03**.
- **Budget:** flash-lite cheap-validate before any priced run; $ ledger live. **Honesty flag carried.**
- **Decision needed:** sign as-is / amend a frozen field / hold.

## 5b. In-flight reviews (not slice agents)

- **Methodology review of the M1 pre-registration** (general-purpose subagent, opus, web-grounded):
  adversarially checks the frozen endpoint + decision-rule mechanics (strict dose-response gate, the
  0.02 material bar, per-stratum conjunction, "best baseline" selection, EM guard, baseline strength)
  against the multi-hop QA / GraphRAG literature. Feeds the ◆ HITL sign-off — **sign-off held until this
  returns** (HITL directed it before freezing).

## 6. Open HITL questions

1. **Commit + spawn?** Triad is uncommitted. Approve committing `plan-0.8.2.md` + roadmap
   `0.8.{2,3,4,5}.md` + this board, then spawn Slice 0.
2. **Triad naming:** 0.8.1 used `plan` + `implementation` + `STATUS`. 0.8.2 folds the per-slice
   contracts into `plan-0.8.2.md §4` (no separate `0.8.2-implementation.md`). Accept the folded form,
   or split out an implementation doc for convention parity?

## 7. Recent decisions (newest on top)

- **2026-06-16** — **Slice 4 CLOSED → fan out 5 ∥ 10.** Corpus pinned + reproduce-stable (fix-1 closed
  on orchestrator verification; codex re-review waived for an objectively-verified reproducibility fix).
  Slice 5 structured to STOP at a budget checkpoint (cheap-validate → bounded priced pilot → power-sim →
  projected full-N cost) so HITL confirms the large spend; Slice 10 ($0) fully autonomous. Worktrees clean.
- **2026-06-16** — **◆ HITL SIGNED the amended pre-registration** (coreyt) → fan out. Design doc
  `status: SIGNED`. Orchestrator refinement: **carved corpus acquisition into a shared Slice 4** (the
  plan folded it into Slice 5, but 5 ∥ 10 share the corpus → pin it once, reproducibly, so the two
  parallel worktrees don't author conflicting acquire scripts). Critical path now `0 → 4 → {5∥10} →
  15 → 20`. Slice 4 spawned (reuses `tests/corpus/scripts/acquire_*.py` + `freeze_corpus.py` pattern).
- **2026-06-16** — **Slice 0 CLOSED (amended); ◆ sign-off ready.** Revision + rev-fix-1 merged
  (`2348f95`); codex §9 **PASS** (zero findings) on the final amended rule; 33/33 re-run green by the
  orchestrator (lint enforces all 6 frozen fields incl `trend-test`; flat-positive ⇒ GO). The Slice 0
  family ran v1 → fix-1 (non-finite) → pre-freeze methodology review → revision (6 amendments) → rev-fix-1
  (trend-test lint) → PASS — exactly the tightening the frozen-as-code pre-registration is meant to force
  *before* data. Worktrees cleaned. **Next = ◆ HITL sign-off of the amended pre-reg → unblock 5/10.**
- **2026-06-16** — **HITL adopted all 6 amendments → Slice 0-revision spawned.** Rule rewritten: trend
  gate (negative-slope veto only, no strict monotonic), fixed `fused+rerank` comparator (not per-hop max),
  pooled ≥3-hop ΔF1 ≥ 0.02 with bootstrap CI > 0, CI-banded EM + unanswerable-set confident-wrong role,
  whole-rule power-sim handed to Slice 5, RRF k=60 pinned. Plan §4 Slice-0 contract amended; revision
  worktree off `74999b3`. `decide()` signature changed (consumes summary stats; harness owns the bootstrap).
- **2026-06-16** — **Slice H1 CLOSED** (`74999b3`): repo-wide `pyright -p src/python` → **0/0/0** (9
  pre-existing errors fixed, typing-only; `score_e2e`→`Mapping`, `float|None` guards, `output_file_id`
  Optional). Orchestrator independently verified pyright + 20 tests; closed on that verification (codex
  §9 waived for a mechanically-ground-truthed typing-only diff — documented override). Worktree cleaned.
- **2026-06-16** — **Pre-freeze methodology review → pre-registration NOT sound to freeze as-is**
  (HITL-directed; `runs/0.8.2-slice-0-prereg-methodology-review.md`). Core flaw: the strict monotonic
  `f1[2]<f1[3]<f1[4]` GO gate encodes a literature-contradicted prior (HippoRAG: 4-hop path-finding "out
  of reach" of single-pass PPR) and is noise-fragile (~1/6 pass under a true uniform win); the per-hop
  "best-of-3 max" baseline adds winner's-curse + a dose-response confound. Net: biased toward the
  expected NO_GO ⇒ cannot *earn* the pivot. Orchestrator concurs. **Pending HITL: approve amendments →
  Slice 0-revision before sign-off.** (Codex §9 had reviewed `decide()` only for *correctness*, not
  statistical validity — this review is the complementary axis.)
- **2026-06-16** — Slice 0 **CLOSED (code)**: fix-1 merged (`a50953c`), codex §9 re-review **PASS**
  (`runs/0.8.2-slice-0-fix-1-review-20260617T005328Z.md`); 37/37 re-run green by the orchestrator;
  worktree/branch cleaned. Two log flags examined + dismissed with cause: the `[P1]/[P2]` tags are
  diff-echoes (not findings); the 9 `test_p0a_batch_e2e.py` pyright errors are **pre-existing** at
  `b304147` (untouched file) — pre-existing tech debt, not a Slice 0 regression. **Next = ◆ HITL
  design+pre-registration sign-off (gates 5/10).**
- **2026-06-16** — Pre-existing debt noted (not 0.8.2's): repo-wide `pyright -p src/python` is **not**
  0/0 — 9 errors in `test_p0a_batch_e2e.py` (`score_e2e` `dict[str,str]` vs `dict[str,str|None]`) at
  baseline `b304147`, contradicting the SLICE-TEMPLATE "0/0 standing baseline". Cleanup candidate.
- **2026-06-16** — Slice 0 codex §9: **CONCERN, one [P2]** — `decide()` returns GO on non-finite (NaN)
  EM/F1 because `nan < 0.0` is False, contradicting its "fail loudly" contract. Substantive (not
  structural/prompt-induced) ⇒ **FIX-1**, not override. Verdict promoted
  (`runs/0.8.2-slice-0-review-20260617T004634Z.md`); fix-1 hardens input validation only (rule
  unchanged). Orchestrator independently re-ran the 18 tests green before reviewing (not trusting the
  agent's green claim — [[background-exit-masks-real-exit]]).
- **2026-06-16** — Slice 0 gets a real **TDD** even as `[design-adr]`: the pre-registered GO/NO-GO
  rule is frozen as a pure-Python `decide()` function (+ schema lint on the design doc) at Slice 0, so
  Slice 20 imports it and cannot post-hoc switch the endpoint. Encodes the plan's anti-post-hoc stance
  as an executable contract. Plan Slice-0 contract updated.
- **2026-06-16** — Scoping pre-flight (orchestrator): slice boundaries + dep graph sound; applied 3
  buildability fixes to the plan (eval path `src/python/eval/`; scipy/networkx dep gap declared as
  harness-only/footprint-safe; Slice 10 body-less-edge adaptation flagged vs the reuse asset's
  default). Board created. Triad still uncommitted pending HITL.
