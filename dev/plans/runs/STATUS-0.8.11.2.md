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
| **Cause-A** | A (parallel) | **CUT IN PROGRESS** | sizing = **GO**, additive-only (verified from git; `CAUSE-A-sizing.md`) | implementer cutting: `stable_id` on `SearchHit` + sha256(body) NULL-doc fallback + parallel telemetry field + 4 bindings; OOB `margin`/distractor knobs = separate later slice |
| **OPP-6** (EXP-COV-0..3) | A | PENDING | — | run C0/C1 + academic (`$0`) arms; EXP-COV-0 re-measures per-corpus ceiling; gates 0.8.10 #6 |
| **OPP-3** (cascade/CE) | A | PENDING | — | native-gap characterization first (per-corpus, never pooled); bears @ V-7 |
| **OPP-1** (EXP-ITER-D/-P/-POLICY) | V-3 | BLOCKED on V-1 | — | held strictly behind V-1 (NOT pulled forward) |
| MuSiQue re-pull (`question_decomposition`) | A prereq | PENDING | — | retain native per-hop list; verify 2,417 answerable rows; unblocks OPP-1 A3 |
| OOB eval-support (`margin` + distractor/rank knobs) | A | PENDING | — | bundle with the Cause-A landing |

## Cross-repo (Memex) handoffs

Two Memex-side roles: (a) a **read-only monitoring agent** (already running, watches the bus — it
self-labeled `memex-steward` in its hello/status lines but is just a monitor); (b) the **active
memex-steward orchestrator**, which the fathomdb-steward SPAWNS to drive `plan-0.5.1.md` (no Memex pushes).

| ts | direction | ref | state | note |
|----|-----------|-----|-------|------|
| 2026-06-29 | memex monitor→bus | hello/status | RECEIVED | read-only Memex monitoring agent online, polling |
| 2026-06-30 | fathomdb→memex | handshake | POSTED | requested the orchestrator begin driving `plan-0.5.1.md` |
| 2026-06-30 | (spawn) | memex-steward orchestrator | SPAWNED | active orchestrator launched in the memex worktree; honoring $75 cap / no-push / stop posture; ack pending |
| 2026-06-30 | memex-steward→bus | BLOCKER B-1 | RESOLVED→Option B | flat pre-0.6.0 API gone on 0.8.x; reframed as a bounded `0.5.x→0.8.x` adapter refit (Option B tasklist) |
| 2026-06-30 | fathomdb→memex | B-1 answer sheet | AUTHORED | `runs/B-1-fathomdb-answer-sheet.md` — I-2…I-7 + A-1/A-2 contracts (3 investigators, file:line cites) + tasklist corrections; pending codex, then bus handoff |

## Gate log (sequencing preserved — R-U-2)

- **V-1** (keystone, re-run EXP-B′ on live CE) — not started; gates all downstream.
- **V-3 = OPP-1** — starts only after V-1 lands.
- **V-7** — records OPP-3 cascade/CE bearing + `margin` verb-shape decision.

## Run log

- 2026-06-29 — launch authorized (HITL). Plan committed to main (`34af4bbd`); worktree + branch cut off main;
  fathom env-check GREEN (`cargo check --workspace` exit 0; `.venv` `import fathomdb` OK); Memex worktree +
  bus verified live. Bus schema aligned to live wire format `{ts,from,to,type,msg}`.
- 2026-06-30 — Cause-A sizing returned **GO** (additive-only, verified from git); cut implementer dispatched
  with two Steward dispositions: sha256(body) fallback for NULL doc-nodes; NEW parallel telemetry field
  (no in-place id flip, preserves F-8a contract).
- 2026-06-30 — memex-steward orchestrator spawned, online, acked (the prior `memex-steward` bus lines were a
  read-only monitor, now retracted). Orchestrator hard-stopped at plan-0.5.1 `→ SIGNED` gate (correct).
  **HITL granted the sign**; orchestrator resumed to execute Slice 0 ($0/local: local-build wiring pinned to
  current main, capability probe, OPP-5 inventory, then Slices 10/15). Memex build phase ≈ $0 draw.
- 2026-06-30 — note: FathomDB `origin/main` advanced to `ba80866d` (0.8.11.1 Library Sweep merged); the
  0.8.11.2 worktree base (`34af4bbd`) is unaffected (additive ancestor); Memex local-build pin → `ba80866d`.
- FathomDB-side queued (from Memex asks A-1/A-2): add `$.action_kind` to `PREDICATE_PATH_ALLOWLIST`; confirm
  bool-eq server-executable in `read.list`. Gate only Slice-5's hot filter (post-sign) — not on critical path.
- 2026-06-30 — **B-1 reconciliation (Option B).** Three read-only investigators returned the full 0.8.x API
  contract → `runs/B-1-fathomdb-answer-sheet.md`. Findings: **A-2 RESOLVED** (bool-eq already server-side on
  `read.list`); **A-1** = 1-line allowlist add (DO in worktree); **A-3 (new)** = `read.list` has no stable
  paging cursor (DEFER pending Memex audit); **I-4** = no consumer FTS/projection API (Memex deletes
  `m003–m007`, not rewrite) + new risk R-I4-parity; **I-5** = `engine.write([dict…])→WriteReceipt`, several
  flat symbols no-analog; **I-7** = `stable_id` gated behind Cause-A merge (`source_id` already on main).
  Plan §2B + DoD R-U-10 added. **HITL: Q-B1 no-migration, Q-B2 keep-0.5.1, Q-B3 greenlight Slice-15-core
  after codex.** Next: codex review (≤4 cycles) → bus handoff of the corrections → A-1 + Slice-15-core.
- 2026-06-30 — **codex review of B-1 docs: CLEAN (0/4 fix cycles).** `codex exec review --commit 3be2bc57`
  found no actionable defect; spot-verified the two highest-stakes claims from source (PREDICATE_PATH_ALLOWLIST
  contents @ engine 1321; `read.list`/`read_list_filter` take no `after_id` — cursor only on `read.collection`).
- 2026-06-30 — **A-1 LANDED** (commit `9e0a3459`, branch `0.8.11.2-pico-umbrella`, NOT pushed): `$.action_kind`
  added to `PREDICATE_PATH_ALLOWLIST` + guard test (`action_kind_path_allowlisted`: accepts Text+Bool, still
  rejects `$.secret`). Tests 25/0 (`slice35_filter_grammar` 19, `slice40_filter_unification` 6). **Merge-gate:**
  like Cause-A/`stable_id`, A-1 reaches Memex's pinned `origin/main` build only when 0.8.11.2 merges — until
  then Memex keeps the client-side `action_kind` split. A-2 resolved; A-3 deferred (Memex paging audit).
- 2026-06-30 — **Bus handoff posted** (05:02Z): answer-sheet + corrections + Slice-15-core GREENLIT (Q-B3) +
  Q-B1/Q-B2 to memex-steward. Orchestration immediate items DONE; awaiting memex-steward ack. **Experiment
  phase (Phase A → V-1 keystone) ready to start; holding priced arms for HITL info + bus announce vs $75 pool.**
