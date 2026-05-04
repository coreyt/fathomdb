# Phase 9 Pack 5 — status board

Single up-to-date progress file for the AC-020 perf packet. Orchestrator
(main thread) updates this file at every plan §0.1 step-5 decision
point. Implementer subagents do **not** edit this file — they write
`<phase>-output.json` instead, which the orchestrator reads.

Last updated: 2026-05-03 (initial).

---

## Current state

- Branch: `0.6.0-rewrite`.
- HEAD: `da9ae05` (4 docs commits on top of `b4a3261` plan baseline).
- Pre-flight: PASS — see `dev/plan/runs/preflight-summary.md`.
- Prompts: PASS — 13 files under `dev/plan/prompts/`.
- Active phase: **none yet** — A.0 next.
- Active worktrees: none.

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
| A.0   | -       | -        | -        | -        | -      | -                       |
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

_(none — populate from each phase's output.json after decision)_

## Outstanding worktrees

_(none — populate when `git worktree add` succeeds; remove on cleanup)_

## Open concerns / overrides

_(none yet — anything CONCERN-severity from reviewer goes here with §12 ref)_

## Next action

Pre-write all phase prompt files (plan §10 step 1) → **DONE**.
Spawn Phase A.0 (test-only harness split, Sonnet medium) once the
orchestrator confirms.

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
