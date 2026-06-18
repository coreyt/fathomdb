# STATUS — 0.8.2 (M1: multi-hop answer-accuracy harness)

> Live state board for 0.8.2 / M1. Per `dev/design/orchestration.md` §12.5 the **orchestrator owns
> this board** (one docs commit per transition); slice agents never edit it. **Witnesses (git +
> `output.json` + verdict `.md`) win over this cache** on any conflict (§1.5 invariant 1).
> Plan: [`../plan-0.8.2.md`](../plan-0.8.2.md). Roadmap: [`../../roadmap/0.8.2.md`](../../roadmap/0.8.2.md).

## 1. Current state + next action

- **State:** **Slice 10 RE-SPAWNED (running); Slice 5 HELD on a reranker prereq.** The first 5∥10 spawns
  both **died on a transient API outage during read-only exploration** — git + worktrees + session
  transcripts confirm **zero work written, nothing to recover** (main untouched `4fd5828`, branches at
  baseline, worktrees clean). Infra re-checked UP (answerer 401=auth-ok; extractor host responds).
- **CE reranker was a STUB** (`try_get_loaded()`→None, `score()`→0.0; `lib.rs:4925,4930` TODOs) → so the
  signed `fused+rerank` comparator wasn't real. **HITL (2026-06-18) chose to IMPLEMENT it** (engine slice
  E1) rather than revise amendment 6 — a deliberate, HITL-approved deviation from M1's no-engine-change
  footprint (footprint-preserving: no network at feature-off / `rerank_depth=0`). Amendment 6 **upheld**.
- **IN-FLIGHT now:** **Slice E1** (reranker impl, Rust/cargo TDD; fills `try_get_loaded`+`score` reusing
  the embedder's Candle BERT loader) ∥ **Slice 10 fix-1** ([P2] hash-validate + [P3] honor-n). Slice 5
  HELD until E1 merges + the orchestrator rebuilds the canonical extension `--features …,default-reranker`
  and functionally verifies real reranking.
- **Next action:** E1 merge → main-thread extension rebuild + functional rerank verify → re-spawn Slice 5.
  Slice 10 fix-1 merge → re-review + close Slice 10.
- **Next action (◆ HITL gate — STOP):** the pre-freeze methodology review (orchestrator-directed)
  returned **NOT sound to freeze as-is** (`runs/0.8.2-slice-0-prereg-methodology-review.md`): the strict
  monotonic dose-response gate + per-hop-max baseline bias the rule toward the expected NO_GO. **4
  load-bearing amendments + 2 advised.** Orchestrator concurs. **Recommendation to HITL: do NOT sign;
  approve the amendment set → spawn a Slice 0-revision (design §4 + `decide()` + tests) → re-review →
  then sign.** Slices 5/10 remain gated behind the (amended) sign-off.
- **Blocked on:** nothing engine-side. Slice 0 has no priced run; the first ◆ HITL gate is the
  Slice-0 design+pre-registration sign-off (must land *before* any priced answerer run at Slice 20).

## 2. Slice scoreboard

| # | Slice | Type | Depends | State | Witness |
|---|-------|------|---------|-------|---------|
| 0 | Design + pre-registration (**+ TDD: frozen decision-rule module**) | `[design-adr]` | — | **CLOSED (amended); ◆ HITL sign-off ready** | revision+fix merged `2348f95`; codex §9 PASS; 33/33 green; all 6 amendments + trend-test lint |
| 4 | **MuSiQue corpus acquisition (SHARED prerequisite for 5 ∥ 10)** | impl (measurement) | 0 ✅ | **CLOSED** | merged+fix-1 `df1c879`; `musique_hash 3cff37fd…`, reproduce-stable, 8/8 tests; orchestrator-verified |
| 5 | strong baseline + answerer e2e over shared corpus (THE BAR) | impl (measurement) | 4 ✅ | **IN-FLIGHT (→ budget checkpoint)** | `runs/0.8.2-m1-baseline-n{N}.json`; stops at cost projection before full priced pass |
| 10 | Graph build over MuSiQue (reuse extractor) | impl (measurement) | 4 ✅ | merged `f267061`; **fix-1 pending** ([P2] hash-validate + [P3] sample-size) | n=299 graph, coverage 1.0, 50.5k nodes/edges, body-less; codex CONCERN |
| 15 | PPR-fusion arm (mechanism KEYSTONE) | impl | 5, 10 | NOT STARTED | branch `output.json` + RED sha in `tdd_evidence` |
| 20 | Adjudication run + verdict (GO/NO-GO → 0.8.3) | impl (measurement) | 15 | NOT STARTED | `runs/0.8.2-m1-verdict-n{N}.json` + `runs/0.8.2-m1-report.md` |
| H1 | Restore repo-wide `pyright -p src/python` to 0/0 (off-ladder hygiene) | impl | — | **CLOSED** | merged `74999b3`; pyright 0/0/0 (orchestrator-verified); 20 tests green; typing-only |

Critical path: `0 → {5 ∥ 10} → 15 → 20`. Slices 5 and 10 are independent off 0 (baseline harness ∥
graph extraction) and may run in parallel.

## 3. $ ledger (budget discipline — cheap-validate before every priced run)

| Date | Slice | Run | Model | $ | Note |
|---|---|---|---|---|---|
| — | — | — | — | 0.00 | No priced run yet. Cheap-validate = `gemini-2.5-flash-lite`; strong reader = `gemini-3.1-pro-preview`. |

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
