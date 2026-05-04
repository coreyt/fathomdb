# Phase 9 Pack 5 — status board

Single up-to-date progress file for the AC-020 perf packet. Orchestrator
(main thread) updates this file at every plan §0.1 step-5 decision
point. Implementer subagents do **not** edit this file — they write
`<phase>-output.json` instead, which the orchestrator reads.

Last updated: 2026-05-03 (A.0 KEEP `fec71a0` FF-merged; ready to spawn A.1).

---

## Current state

- Branch: `0.6.0-rewrite`.
- Branch tip: `fec71a0` (test(perf-gates): A.0 harness split, FF-merged
  from `pack5-A0-harness-split-20260504T002643Z`). Not yet pushed.
  Prior local commits: `fc8b8d8` docs alignment; `2dc2134` STATUS
  refresh; `1980bf6` Pack 1-4 production.
- A.0 spawn baseline (used): `0.6.0-rewrite` ref at spawn time =
  `fc8b8d8` (descendant of plan-recorded baseline `1980bf6`). FF
  applied cleanly.
- A.1 spawn baseline: **`fec71a0`** (A.0 commit). A.1 needs the
  split-test entry points.
- Baseline drift note: original Pack 5 plan assumed a clean baseline
  with Pack 1-4 already committed, but those changes were sitting in
  the working tree. They were committed at `1980bf6` after running
  `agent-verify.sh` green at that tree. No production changes
  authored in this resume; the commit is a clerical land of existing
  WT state.
- Pre-flight: PASS — see `dev/plan/runs/preflight-summary.md`. (HEAD
  drifted from `da9ae05` to `1980bf6` since pre-flight; no preflight
  amendment required because none of the seven checks depend on the
  engine src state.)
- Prompts: PASS — 13 files under `dev/plan/prompts/`.
- Active phase: **none** — A.0 closed (KEEP, `fec71a0`); A.1 next.
- Active worktrees: none (A.0 worktree force-removed after copying
  `A0-harness-split-output.json` into main repo; phase branch deleted).

## Acceptance scoreboard

| Gate   | Required                                  | Latest reading                                | Status |
| ------ | ----------------------------------------- | --------------------------------------------- | ------ |
| AC-017 | green                                     | green (whitepaper §10)                        | green  |
| AC-018 | green; no regression > 10 % vs baseline   | green (whitepaper §10)                        | green  |
| AC-020 | `concurrent <= sequential * 1.25 / 8`, x5 | seq 456 / conc 127 / bound 85 / speedup 3.59x | red    |

Bound for AC-020 in this packet is the §1 20%-margin form
(`1.25 / 8` ≈ 0.156), tighter than the test's literal `1.5 / 8`. The
test bound stays untouched (hard rule §4.1); this score reflects the
packet's acceptance criterion.

## Phase results

| Phase | Spawned | Decision | Reviewer | Worktree | Commit | Notes / log             |
| ----- | ------- | -------- | -------- | -------- | ------ | ----------------------- |
| A.0   | 2026-05-03 | KEEP  | n/a (test-only) | cleaned | `fec71a0` | harness split; smoke seq=184/conc=117 N=1; output JSON `dev/plan/runs/A0-harness-split-output.json` |
| A.1   | -       | -        | -        | -        | -      | -                       |
| A.2   | -       | -        | -        | -        | -      | main thread             |
| A.3   | -       | -        | -        | -        | -      | -                       |
| A.4   | -       | -        | -        | -        | -      | main thread             |
| B.1   | -       | -        | -        | -        | -      | -                       |
| B.2   | -       | -        | -        | -        | -      | conditional on B.1 KEEP |
| B.3   | -       | -        | -        | -        | -      | conditional             |
| C.1   | -       | -        | -        | -        | -      | conditional             |
| D.1   | -       | -        | -        | -        | -      | parallel track          |
| final | -       | -        | -        | -        | -      | -                       |

Decision values: `KEEP` / `REVERT` / `INCONCLUSIVE` / `RECAPTURE` /
`SKIPPED`. Reviewer values: `PASS` / `CONCERN` / `BLOCK` / `n/a`.

## Latest measurements (N=5 unless noted)

- 2026-05-03 A.0 smoke (N=1, NOT a gate reading): split harness
  seq=184ms, conc=117ms; combined gate at same tree
  seq=182ms / conc=118ms / bound=34ms / speedup=0.19. Numbers
  consistent within noise → fixture parity confirmed. Combined-gate
  bound failure was pre-existing (not introduced by A.0).

## Outstanding worktrees

_(none — populate when `git worktree add` succeeds; remove on cleanup)_

## Open concerns / overrides

_(none yet — anything CONCERN-severity from reviewer goes here with §12 ref)_

## Next action

Pre-write all phase prompt files (plan §10 step 1) → **DONE**.
Land Phase 9 Pack 1-4 baseline → **DONE** (`1980bf6`).
Spawn Phase A.0 → **DONE** (KEEP, `fec71a0`, FF-merged).

**Pause point per resume §8** — confirm with human before spawning
A.1 (perf record + flamegraph capture, Sonnet medium, baseline
`fec71a0`). A.1 spawn block is in
`dev/plan/prompts/A1-perf-capture.md` with Update log carrying
A.0 numbers and baseline SHA.

---

## Update protocol

1. After implementer subagent returns: read its `<phase>-output.json`.
2. After reviewer (codex) returns: read its `<phase>-review-<ts>.md`.
3. Orchestrator decides KEEP / REVERT / INCONCLUSIVE.
4. **Edit this file**:
   - Update "Active phase" / "Current state".
   - Fill the matching row in "Phase results".
   - Append median / min / max numbers to "Latest measurements".
   - Update "Outstanding worktrees" (add on spawn, remove on cleanup).
   - Add any reviewer CONCERN to "Open concerns / overrides".
   - Update "Next action".
5. Append §12 line in the plan file (one-line audit trail).
6. Append §11 narrative in the whitepaper notes (only on KEEP).
7. Update next prompt's `## Update log` with the just-decided numbers.
