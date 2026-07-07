# FathomDB — Steward Session Hand-off (2026-07-07)

> **Boot:** run **`/steward`** (loads the role contract `.claude/agents/steward.md` +
> `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md`), do its §3 cold-start reading, then read THIS doc for current
> state, return an orientation paragraph, and **WAIT for HITL** before mutating. You are the **Program
> Steward**: keep the schedule-of-record true to git, place cross-cutting work, commission + verify, propose-first
> to the HITL. **Do not implement code or hand-drive a ladder.** *(Supersedes the 2026-07-03 hand-off.)*

## Git state — read first

- **`main` is clean and pushed** (`origin/main == main == 5122cfd2`). No worktrees mid-flight; the 0.8.14
  orchestrator is retired. Parked/other-session worktrees exist (`0.5.1-memex-build`, `0.8.11.2`,
  `tooling-mermaid-renderer`) — **leave them**. Verify with `git rev-parse --abbrev-ref HEAD` (expect `main`)
  before any commit.
- **Steward ledger** `dev/steward/steward-ledger.jsonl` @ **seq 32**. Read deltas with `ledgerwatch`; append
  with `ledgerwrite` (never open by hand).
- **Todos & considerations ledger** `dev/todos-and-considerations-ledger.jsonl` — NEW this session (id prefix
  `TC`; protocol `dev/todos-and-considerations-ledger-readme.md`). Open items: **TC-1..TC-6** (below).

## What landed this session (context, not action)

1. **OPP-12 RATIFIED (both sides, 2026-07-03).** Enum thread `seq 1→13` CLOSED/`agreed`; MEMEX agreed +
   applied the mirror. It's a **design contract only** — schedules nothing, authorizes no build. Docs
   `dev/design/record-lifecycle-protocol/` (RATIFIED). Roadmap placement **TBD (≥0.9.x)**. See
   `[[opp12-record-lifecycle-protocol]]`.
2. **0.8.14 COMPLETE (label-only, `465b43ac`).** EXP-S substrate + F5 BM25F + gpu-rerank; `SCHEMA 15→17`;
   manifests stay `0.8.9`, **no tag/publish**. F5 shipped per the **D8 Option-C HITL override**. eu7/R-GATE
   closed on the **D6 no-op** basis. See `[[0.8.14-complete-substrate-recall]]`.
3. **GPU-for-eval MANDATE + policy** (`dev/design/gpu-eval-activities-policy.md`): repo eval/embed/rerank runs
   on the 3090s when there's room (CPU only for the two compatibility probes) — **but recall-fidelity gates
   (eu7) run CPU same-backend** (steward self-correction). See `[[repo-must-use-gpu-for-eval-when-3090-room]]`.
4. **Todos-ledger adopted** (ported from memex); **CompMix corpus acquisition tooling merged** (`d2209594`);
   **0.9.x roadmap DRAFT** written (PROPOSED, OPP-12-early) `dev/plans/0.9.x-roadmap-DRAFT.md`.
5. **Process lesson:** a bg orchestrator died silently for 36h — poll git + task-mtime + `ps`, don't wait on a
   notification; give an anti-stall directive. See `[[background-agent-silent-death-proactive-check]]`.

## Owed / open decisions (HITL) — the live queue

1. **0.9.x roadmap** (`0.9.x-roadmap-DRAFT.md`) — **8 scheduling decisions in §7** (which micro slots for
   OPP-12; the atomic 0.9.2 publish; the 0.9.3-is-odd tension; the Memex-pair version; when Cause-A publishes;
   when 0.9.0 opens; C-1 co-land ordering; build authorization). Also: graduate the draft into a real
   `0.9.x-PROGRAM-SEQUENCING.md`? **Nothing scheduled until HITL rules.**
2. **TC-4 placement** — the GPU-default eval-harness tooling (p1): fold into 0.8.16, or a fast-follow pico?
3. **TC-5** — eu7 0.90-floor re-baseline for the grown 18,472-doc corpus (CPU same-backend re-measure + HITL
   threshold ruling).
4. **TC-6** — `embed_batch_cls` py-only → TS-parity decision (a future SDK-parity release).
5. **`release.yml` publish-structure/actionlint red** — pre-existing (git-proven not 0.8.14) → route to the
   **Library-Sweep / CI-integrity** track (HITL confirm).
6. **`dev/research/personal-agent-database-market-*.md`** — 9 MD025/MD001 findings (fix / archive / ignore).
7. **Next even release = 0.8.16** (F9 importance/confidence + cross-vendor ONNX) — commission when ready.
8. **OPP-12 implementation** — cannot start until scheduled on ≥0.9.x (breaking; 0.8.x active through 0.8.20)
   + Memex coordination (C-1 EntityTypeSpec, the 0.5.x↔0.9.x breaking pair). See the 0.9.x draft.
9. **Untracked `dev/design/1-2M-chunk-scaling-*.md`** (×3) — another agent's in-flight decision-support docs;
   **leave them** (HITL: let that session finish).

## Standing guardrails (load-bearing)

- **GPU mandate:** eval/embed/rerank on the 3090s when there's room; **fidelity gates (eu7) run CPU
  same-backend**; CPU otherwise only for the two compatibility probes (policy doc).
- **Push-scope fathomdb-only** — never push memex (or any other repo) without a per-push directive each time.
- **Memex enum-ledger:** no FATHOM append without HITL discuss-and-agree first; full unfiltered tail read before
  every append; listening is fine. (OPP-12 is CLOSED, but the channel remains.)
- **Background agents die silently** → poll git + task-mtime + `ps`; anti-stall directive when commissioning.
- **Two-tier numbering** (`x.y.z` real/publishable-if-even · `x.y.z.p` pico label-only · **`13` forbidden**);
  publish is a separate explicit HITL gate. Label-only unless HITL cuts a real `x.y.z`.
- **codex §9 gates commissioned code** (`/code-review` / adversarial-subagent is the sanctioned fallback when
  codex is offline/derails — commit the evidence). **Verify from git, not narration.**
- **F5-per-override, eu7-D6-no-op, F-16 (HNSW=2.x), F-17 (maturity ladder)** are recorded in the master.

## Memory pointers

`[[0.8.14-complete-substrate-recall]]` · `[[opp12-record-lifecycle-protocol]]` ·
`[[repo-must-use-gpu-for-eval-when-3090-room]]` · `[[background-agent-silent-death-proactive-check]]` ·
`[[gpu-not-retrieval-lever-hnsw-at-0.8.20]]` · `[[release-dod-requires-full-workspace-gate]]` ·
`[[steward-delegate-dont-hand-do]]` · `[[push-scope-fathomdb-only]]`.
