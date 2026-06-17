# STATUS — 0.8.2 (M1: multi-hop answer-accuracy harness)

> Live state board for 0.8.2 / M1. Per `dev/design/orchestration.md` §12.5 the **orchestrator owns
> this board** (one docs commit per transition); slice agents never edit it. **Witnesses (git +
> `output.json` + verdict `.md`) win over this cache** on any conflict (§1.5 invariant 1).
> Plan: [`../plan-0.8.2.md`](../plan-0.8.2.md). Roadmap: [`../../roadmap/0.8.2.md`](../../roadmap/0.8.2.md).

## 1. Current state + next action

- **State:** PRE-SLICE-0. Plan triad authored (working tree, **uncommitted**); scoping pre-flight done.
- **Next action:** HITL review of the scoping pre-flight (below) → commit the 0.8.2 triad → spawn
  **Slice 0** (design + pre-registration `[design-adr]`).
- **Blocked on:** nothing engine-side. Slice 0 has no priced run; the first ◆ HITL gate is the
  Slice-0 design+pre-registration sign-off (must land *before* any priced answerer run at Slice 20).

## 2. Slice scoreboard

| # | Slice | Type | Depends | State | Witness |
|---|-------|------|---------|-------|---------|
| 0 | Design + pre-registration (**+ TDD: frozen decision-rule module**) | `[design-adr]` | — | NOT STARTED | `dev/design/0.8.2-m1-multihop-harness.md` (`status: decision-ready`) + `src/python/eval/m1_decision_rule.py` GREEN + RED sha in `output.json` |
| 5 | MuSiQue corpus + strong baseline + answerer e2e (THE BAR) | impl (measurement) | 0 | NOT STARTED | `runs/0.8.2-m1-baseline-n{N}.json` |
| 10 | Graph build over MuSiQue (reuse extractor) | impl (measurement) | 0 | NOT STARTED | `runs/0.8.2-m1-graph-coverage-n{N}.json` |
| 15 | PPR-fusion arm (mechanism KEYSTONE) | impl | 5, 10 | NOT STARTED | branch `output.json` + RED sha in `tdd_evidence` |
| 20 | Adjudication run + verdict (GO/NO-GO → 0.8.3) | impl (measurement) | 15 | NOT STARTED | `runs/0.8.2-m1-verdict-n{N}.json` + `runs/0.8.2-m1-report.md` |

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

None.

## 6. Open HITL questions

1. **Commit + spawn?** Triad is uncommitted. Approve committing `plan-0.8.2.md` + roadmap
   `0.8.{2,3,4,5}.md` + this board, then spawn Slice 0.
2. **Triad naming:** 0.8.1 used `plan` + `implementation` + `STATUS`. 0.8.2 folds the per-slice
   contracts into `plan-0.8.2.md §4` (no separate `0.8.2-implementation.md`). Accept the folded form,
   or split out an implementation doc for convention parity?

## 7. Recent decisions (newest on top)

- **2026-06-16** — Slice 0 gets a real **TDD** even as `[design-adr]`: the pre-registered GO/NO-GO
  rule is frozen as a pure-Python `decide()` function (+ schema lint on the design doc) at Slice 0, so
  Slice 20 imports it and cannot post-hoc switch the endpoint. Encodes the plan's anti-post-hoc stance
  as an executable contract. Plan Slice-0 contract updated.
- **2026-06-16** — Scoping pre-flight (orchestrator): slice boundaries + dep graph sound; applied 3
  buildability fixes to the plan (eval path `src/python/eval/`; scipy/networkx dep gap declared as
  harness-only/footprint-safe; Slice 10 body-less-edge adaptation flagged vs the reuse asset's
  default). Board created. Triad still uncommitted pending HITL.
