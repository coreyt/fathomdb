# STATUS — 0.8.11.2 pico umbrella (OPP-1 / OPP-3 / OPP-6 + Cause-A)

> Live verdict board + running `$` ledger for the autonomous `/goal complete` run.
> Plan: `dev/plans/plan-0.8.11.2.md`. Branch: `0.8.11.2-pico-umbrella`
> (worktree `/home/coreyt/projects/fathomdb-worktrees/0.8.11.2`, base main `34af4bbd`).
> Cross-repo bus: `/home/coreyt/projects/memex-worktrees/0.5.1-fathom-chat/fathom-memex-chat.jsonl`.

## Envelopes (HITL 2026-06-29 — pre-set, no mid-run pause)

- **Spend:** single pooled **`$75`** across ALL priced passes; auto-stop at cap.
- **Cross-repo:** Memex arms via `memex-steward` on `plan-0.5.1.md`; coordinate over the bus; **NO pushes to Memex.**
- **Stop posture:** auto-proceed Phase A → V-1 → V-3 → V-7; **hard-stop ONLY** at OPP-1 Adopt-GO + any publishable cut.

## `$` ledger

| ts | item | pass | $ this pass | cumulative | cap | note |
|----|------|------|-------------|------------|-----|------|
| 2026-06-29 | — | (envelope opened) | 0.00 | **0.00** | 75.00 | no priced spend yet |

## Per-item verdict board (R-U-1: landed verdict/artifact, not `AGREED`)

| Item | Phase | State | Verdict / artifact | Next action |
|------|-------|-------|--------------------|-------------|
| **Cause-A** | A (parallel) | **IN PROGRESS** | size-it-first investigation dispatched | confirm additive-only → cut the OOB field+bindings+telemetry (bundles OOB `margin`/distractor knobs) |
| **OPP-6** (EXP-COV-0..3) | A | PENDING | — | run C0/C1 + academic (`$0`) arms; EXP-COV-0 re-measures per-corpus ceiling; gates 0.8.10 #6 |
| **OPP-3** (cascade/CE) | A | PENDING | — | native-gap characterization first (per-corpus, never pooled); bears @ V-7 |
| **OPP-1** (EXP-ITER-D/-P/-POLICY) | V-3 | BLOCKED on V-1 | — | held strictly behind V-1 (NOT pulled forward) |
| MuSiQue re-pull (`question_decomposition`) | A prereq | PENDING | — | retain native per-hop list; verify 2,417 answerable rows; unblocks OPP-1 A3 |
| OOB eval-support (`margin` + distractor/rank knobs) | A | PENDING | — | bundle with the Cause-A landing |

## Cross-repo (Memex) handoffs

| ts | direction | ref | state | note |
|----|-----------|-----|-------|------|
| 2026-06-29 | memex→bus | hello/status | RECEIVED | `memex-steward` online, bus-only, polling |
| 2026-06-29 | fathomdb→memex | handshake | (posting at launch) | request memex-steward begin driving `plan-0.5.1.md` |

## Gate log (sequencing preserved — R-U-2)

- **V-1** (keystone, re-run EXP-B′ on live CE) — not started; gates all downstream.
- **V-3 = OPP-1** — starts only after V-1 lands.
- **V-7** — records OPP-3 cascade/CE bearing + `margin` verb-shape decision.

## Run log

- 2026-06-29 — launch authorized (HITL). Plan committed to main (`34af4bbd`); worktree + branch cut off main;
  fathom env-check GREEN (`cargo check --workspace` exit 0; `.venv` `import fathomdb` OK); Memex worktree +
  bus verified live. Bus schema aligned to live wire format `{ts,from,to,type,msg}`.
