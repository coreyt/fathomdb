# STATUS — 0.8.2 (M1: multi-hop answer-accuracy harness)

> Live state board for 0.8.2 / M1. Per `dev/design/orchestration.md` §12.5 the **orchestrator owns
> this board** (one docs commit per transition); slice agents never edit it. **Witnesses (git +
> `output.json` + verdict `.md`) win over this cache** on any conflict (§1.5 invariant 1).
> Plan: [`../plan-0.8.2.md`](../plan-0.8.2.md). Roadmap: [`../../roadmap/0.8.2.md`](../../roadmap/0.8.2.md).

## 1. Current state + next action

- **State:** **SLICE 0 CLOSED (code); ◆ HITL SIGN-OFF PENDING.** Slice 0 + fix-1 merged to `main`
  (`a50953c`), git-gated, codex §9 **PASS** after fix-1 (one [P2] non-finite-input gap found + fixed,
  re-reviewed clean). Orchestrator independently re-ran 37/37 green; worktree + branch cleaned.
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
| 0 | Design + pre-registration (**+ TDD: frozen decision-rule module**) | `[design-adr]` | — | **CLOSED** (code; ◆ HITL sign-off pending) | merged `a50953c`; codex §9 PASS (post fix-1); 37/37 green |
| 5 | MuSiQue corpus + strong baseline + answerer e2e (THE BAR) | impl (measurement) | 0 | NOT STARTED | `runs/0.8.2-m1-baseline-n{N}.json` |
| 10 | Graph build over MuSiQue (reuse extractor) | impl (measurement) | 0 | NOT STARTED | `runs/0.8.2-m1-graph-coverage-n{N}.json` |
| 15 | PPR-fusion arm (mechanism KEYSTONE) | impl | 5, 10 | NOT STARTED | branch `output.json` + RED sha in `tdd_evidence` |
| 20 | Adjudication run + verdict (GO/NO-GO → 0.8.3) | impl (measurement) | 15 | NOT STARTED | `runs/0.8.2-m1-verdict-n{N}.json` + `runs/0.8.2-m1-report.md` |
| H1 | Restore repo-wide `pyright -p src/python` to 0/0 (off-ladder hygiene) | impl | — | **IN-FLIGHT** | merge to main; `pyright -p src/python` 0/0; touched pytest green |

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

## 5a. Open HITL gate — ◆ design + pre-registration sign-off (BLOCKS 5/10)

Package for coreyt. Sign to unblock Slices 5 ∥ 10 (and authorize Slice 5's priced baseline run).
- **Design doc:** `dev/design/0.8.2-m1-multihop-harness.md` (`status: decision-ready`).
- **Frozen primary endpoint:** paired ΔEM/ΔF1 = (PPR-fusion) − (best baseline), **per hop count
  (2/3/4)**, 2→3→4 dose-response load-bearing. Unanswerable set = confident-wrong guard.
- **Frozen decision rule (as code, `decide()`):** GO iff ΔF1 ≥ **0.02** on hops 3 **and** 4, strictly
  dose-responsive (`f1[2]<f1[3]<f1[4]`), ΔEM ≥ 0 on hops 3/4, **and** adequately powered; else NO_GO.
  Non-finite inputs raise (post fix-1). Slice 20 imports it, may not redefine it.
- **Strong baseline:** BM25 ∪ passage-dense ∪ fused(RRF) — not lexical-only; same answerer all arms.
- **Budget:** flash-lite cheap-validate before any priced `gemini-3.1-pro-preview` run; $ ledger live.
- **Honesty flag carried:** literature expects near-tie-to-modest-loss; value = strongest fair graph test.
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
