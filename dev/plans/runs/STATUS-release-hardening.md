# STATUS — 0.7.2 RELEASE-HARDENING

_Last updated: 2026-05-31 — PR-2 family RESOLVED + **PR-1 (doc drift sweep) CLOSED**
(codex PASS) on local `main` (unpushed, HEAD `10a0e24`, 43 ahead of `origin/main`).
Remaining Phase A (PR-9, PR-3, PR-4) and Phase B (PR-5/6/7) NOT STARTED. `v0.7.0` held
locally; `v0.7.1` not yet tagged (PR-4 creates it); workspace version still `0.7.0`.
Nothing pushed to origin._

Orchestrator: main-thread Claude Code session. Pattern per `dev/design/orchestration.md`
(per-slice prompt → informed subagent implementer/orchestrator (TDD) → codex review →
cherry-pick/ff to `main` on PASS). **No push without explicit HITL OK.**

## Handoff / sources of truth

- **Plan-of-record (spec):** `dev/plans/prompts/0.7.2-RELEASE-HARDENING-HANDOFF.md`
  (ordered PR-N sections + Definition of Done + HITL-gates table).
- **This file:** live execution ledger / ordered run-sheet. The scoreboard below is
  the to-do list; each slice prompt reports its outcome back here.
- PR-2 family resolution: `dev/plans/runs/0.7.2-PR-2bc-decision.md` (RATIFIED).
- Root-cause evidence: `dev/plans/runs/0.7.2-PR-2c-recall-rootcause.md`,
  `dev/plans/runs/0.7.2-EU-8-ir-recall-results.md`.
- Inherited STATUS: `dev/plans/runs/STATUS-embedder-undefer.md` (0.7.1),
  `dev/plans/runs/STATUS-perf-vector-quant.md` (0.7.0).
- Deferred to 0.8.x: `dev/plans/prompts/0.8.x-auto-mean-drift-DEFERRED.md`.

## Baseline

- Branch: `main` (slices work directly on `main` per the PR-2bc precedent, or in
  per-slice worktrees per orchestration.md § 2 — orchestrator's choice per slice).
- Pre-0.7.2 anchor: `v0.7.0` (held, unpushed). PR-1+ branch from `main` HEAD.
- Current `main` HEAD: `5dd0b52`. Ahead of `origin/main` by 39 commits.

## Slice scoreboard (ordered run-sheet — this is the to-do list)

Sequence is top-to-bottom within phase. "Gate" = HITL gate that must clear before the
slice's irreversible/costly step. "Prompt" = per-slice execution prompt (authored
as-needed from the handoff section; — = not yet authored).

### Phase A — release closure (must finish before Phase B)

| Order | ID | Subject | Status | Dep | Gate | Prompt | Notes |
|---|---|---|---|---|---|---|---|
| — | PR-0 | Inherited-state reconciliation | **FOLDED** | — | — | — | Never run as a discrete slice; its facts are captured here + in the decision memo. Tags/version confirmed (`v0.7.0` only; ws 0.7.0). Treat as satisfied. |
| — | PR-2a | Mean-centering recall investigation | **CLOSED (GO, later reframed)** | — | done | `…/prompts/0.7.2-PR-2a-recall-investigation.md` | GO verdict; later shown to address a measurement artifact. |
| — | PR-2(bc) | Recall floor + mean recompute family | **CLOSED / RESOLVED** | PR-2a | ratified | `…/prompts/0.7.2-PR-2bc-{reassessment,S1,S2,S3}.md` | S1 land-harness + S2 carve-auto-drift + S3 floor-reframe landed (`5b69568`/`2ef8c3d..d2c0bf4`/`78164b9`); PR-2c SHELVED. Floor HOLDS 0.90. See decision memo. |
| **1** | PR-1 | Architecture/design doc drift sweep | **CLOSED** | PR-0 | drift-list approved 2026-05-31 | `…/prompts/0.7.2-PR-1-doc-drift-sweep.md` | Audit→drift list (10 items, `4beca5b`)→HITL approved all→corrections `aebf959` + closure `10a0e24` on `main`. Codex **PASS** (`…/runs/0.7.2-PR-1-review-20260531T165936Z.md`). Docs-only; nothing pushed. |
| **2** | PR-9 | Embedder robustness (concurrent-embed safety + Invariant-5 watchdog) | **NOT STARTED** | PR-1 | diff+tests | — | **Gates PR-3's N=1M seed.** RED-first; sequence ahead of PR-3. |
| **3** | PR-3 | Real-corpus canonical-CI N=1M dispatch | **NOT STARTED** | PR-2(bc), PR-9 | **dispatch approval (cost)**; budget approval before ADR | — | Pre-flight: ~10K-doc unserialized real-corpus seed before the N=1M dispatch. Fills `ADR-0.7.0-text-query-latency-gates-revised.md`. |
| **4** | PR-4 | Release notes + **push v0.7.0 + v0.7.1** | **NOT STARTED** | PR-1, PR-2(bc), PR-3 | **explicit push approval — irreversible** | — | CHANGELOG v0.7.0 + v0.7.1 sections; docs/embedder.md; creates `v0.7.1` tag; pushes `main` + both tags. |

### Phase B — testing/perf hardening (after PR-4)

| Order | ID | Subject | Status | Dep | Gate | Prompt | Notes |
|---|---|---|---|---|---|---|---|
| **5** | PR-5 | Corpus-driven test harness (`tests/support/corpus_harness.rs`) | **NOT STARTED** | PR-4 | — | — | Pack 4 tests migrate to it; per-(model,subset) embedding cache. |
| **6** | PR-6 | Dev-loop perf gates (`perf_gates_devloop.rs`) | **NOT STARTED** | PR-5 | devloop budget shape | — | ≤30 s warm; catches batch-collapse + scanner regressions. |
| **6** | PR-7 | Perf regression detection (`dev/perf-history/` + check bin) | **NOT STARTED** | PR-6 | threshold constants (10% lat / 0.02 recall) | — | PR-6 and PR-7 independent of each other; parallelizable. |
| **7** | PR-8 | Campaign closure | **NOT STARTED** | PR-7, PR-9 | v0.7.2 push | — | Final scoreboard here; CHANGELOG 0.7.2 section; HITL sign-off. |

## Open items (carried; not gating their own slice)

- **AC-019 idle-box re-run** — the EU-7/PR-2bc dev-box stress MISS (p99 1201 ms vs
  499 ms bound) was contention-inflated, **NOT a regression** (AC-013 PASSes: p50 36 /
  p99 49 ms). Needs a clean idle-box re-run; folded into PR-3's canonical dispatch.
- **EU-7 harness follow-up** — silent GT-embed phase wants an `EU7_GT_EMBED_PROGRESS`
  periodic log line (cheap, test-only); fold into the next harness touch.
- **Doc-archive hygiene** (out of campaign scope) — ~100 completed-release prompts in
  `dev/plans/prompts/` + run artifacts have no archive convention; `dev/plans/README.md`
  is itself stale (claims the dir is "0.6.0 only"). Decide a convention before moving
  tracked files (cross-refs by path exist).

## Honesty report

- The recall "gap" (0.828) was a **measurement artifact**, not a defect. The corrected
  ANN-fidelity number is **0.937** and the 0.90 floor was always defensible; the ADR was
  corrected to cite the right measurement, NOT re-worded to retcon a pass.
- The automatic mean-drift detector was built and ratified, then **carved out** because
  its sole justification (recall) collapsed and its benefit is unmeasured. It is parked
  for 0.8.x behind a RED guard, not silently dropped.
- Nothing is pushed. `v0.7.1` is intentionally untagged until PR-4.

## Pointer forward

Next actionable slice: **PR-9 (embedder robustness)** — PR-1 is now CLOSED. PR-9 is
independent of the held release push and **gates PR-3's N=1M seed** (concurrent-embed
safety + the Invariant-5 embed watchdog); it is RED-first (engine behavior change).
Author its per-slice prompt from the handoff PR-9 section. After PR-9: PR-3 (dispatch
cost gate) → PR-4 (push gate). Author per-slice prompts from the handoff sections as
each is picked up; update this scoreboard on landing.
