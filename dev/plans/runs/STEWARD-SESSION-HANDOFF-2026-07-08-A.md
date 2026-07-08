# FathomDB — Steward Session Hand-off (2026-07-08-A)

> **Boot:** run **`/steward`** (loads `.claude/agents/steward.md` + `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md`),
> do its §3 cold-start reading, then read THIS doc for current state, return an orientation paragraph, and
> **WAIT for HITL** before mutating. You are the **Program Steward**: keep the schedule-of-record true to git,
> place cross-cutting work, commission + verify, propose-first to HITL. **Do not implement code or hand-drive a
> ladder.** *(Supersedes the 2026-07-07 hand-off. Updated through the F-19/F-20 resequence.)*
>
> **★ FIRST TWO ACTIONS THIS SESSION (HITL-directed): (1) CHECK THE PLANS are true to the ledgers/ADRs.
> (2) PREPARE TO COMMISSION 0.8.16** — see §"Next" below.

## Git state — read first

- **`main` clean and pushed.** No mid-flight worktrees of mine. Parked/other-session worktrees
  (`0.5.1-memex-build`, `0.8.11.2`, `tooling-mermaid-renderer`) — **leave them.** Verify
  `git rev-parse --abbrev-ref HEAD` (expect `main`) before any commit.
- **Ledgers (use `ledgerwrite` to append, `ledgerwatch` to read — NEVER hand-edit):**
  - Steward `dev/steward/steward-ledger.jsonl` @ **seq ~47**.
  - Todos `dev/todos-and-considerations-ledger.jsonl` — **TC-1…TC-8** (TC-7 = C-1 timing **resolved**;
    **TC-8 = Cause-A typed `SearchHit.id` C-2 swap, `open`/P0**, now a 0.8.19 build item).
  - **OPP-12 C-1 sub-ledger** `dev/design/record-lifecycle-protocol/OPP-12-sub-ledger.jsonl` @ **seq 8**
    — **C-1 RATIFIED both sides**.
- **Memex side:** ratified the leverage-ledger reconciliation (`memex 6f7456c`), committed its C-1 ADR, and
  **ratified C-1 (sub-ledger seq 7)**. Never push memex.

## ★ Program shape now — OPP-12 is a 0.8.x deliverable (F-19/F-20)

**HITL resequenced (2026-07-07):** ALL OPP-12 work + the F-17 scale-bound are pulled **INTO the 0.8.x line**,
DONE before 0.9.0. 0.8.x micros keep counting up (odd/even; even=publishable w/ HITL, **odd may publish by
exception**; `13` forbidden). **Current 0.8.x tail (master §4 + F-19/F-20 — authoritative):**

| Micro | Pub | Content |
|---|---|---|
| 0.8.16 | even | **F9 + ONNX** — F9 specced with the OPP-12 `rankable` contract in mind |
| 0.8.15 / 0.8.17 | — | ⏸ **PARKED** (dispatcher + hardening) |
| 0.8.18 | even | #5 vec-equiv + **#11-full publish matrix** (OPP-12 publish prereq) |
| **0.8.19** | odd (label-only) | **OPP-12 Phase-1** — existence axis · `transition`/`purge` · **C-2 id swap (TC-8)** · schema mig · X1 |
| **0.8.20** | even (publish) | **OPP-12 Phase-2 + breaking-pair publish** — read-modes · node-validity · **projection registry (C-1 co-land)** · `dense_readiness`; Memex `0.5.x-successor` pairs |
| 0.8.21 | odd | free-threading + #13 bench *(was .19)* |
| 0.8.22 | even | dep migrations napi/rusqlite *(was .20)* |
| 0.8.23 / 0.8.24 | odd / even | **F-17 scale-bound** soft/stated — 0.8.x-cohesive capstone (add .25+ if needed) |
| **0.9.0** | — | opens **only after** 0.8.x cohesive; **identity now OPEN** (likely 1.0 exit-beta runway) |

## What landed this session (context, not action)

1. **Memex leverage-ledger reconciliation — RATIFIED both sides (`memex 6f7456c`):** OPP-6 → `RESOLVED —
   DE-PRIORITIZED` (F-15); **OPP-11 → `SIGNED`**; OPP-1/3/12 landed-state (OPP-1 "→0.5.5" aspirational).
2. **Memex ranked-needs → master F-18:** OPP-12 is the single major FathomDB-side blocker for Memex; F9→OPP-12
   `rankable` edge; **0.8.15 dispatcher PARKED** (zero OPP-12 dependency, verified).
3. **OPP-12 C-1 co-design — CONVERGED → RATIFIED both sides** (2-shot loop; `OPP-12-C1-converged-contract.md`
   + Memex ADR). One engine-owned EAV; Memex persists spec / engine `ProjectionSpec` = derived boot cache;
   engine needs no `provisional` concept; `rankable` graceful-absent until F9; the two P2·S0-parked items
   (apply-atomicity, defaults) **resolved at contract level**; the break-if-late `AttributeSpec.index` field
   **frozen inert in Memex Commission C** (`memex 95ed450`). **Memex-isolation confirmed** (Commission C proceeds
   without P2·S-F; only coupling = the co-land).
4. **★ F-19/F-20 RESEQUENCE (HITL):** OPP-12 + F-17 scale-bound pulled into 0.8.x (table above). Plans
   reconciled: master §4/F-19/F-20; C-1 contract landing `0.9.1 → 0.8.20`; `0.9.x-roadmap-DRAFT.md`
   **SUPERSEDED-IN-PART** (OPP-12/scale-bound out; 0.9.0 identity OPEN); numbering memory updated.
5. **Process rules adopted (memory):** `ledgerwrite`/`ledgerwatch` for all ledger ops; write responses to
   ledger+ADRs; hand-off filename `YYYY-MM-DD-A` (sequential alpha).

## ★ Next — prepare to commission 0.8.16

- **`plan-0.8.16.md` exists** (F9 + ONNX). **F9 must be specced with the OPP-12 `rankable` projection-role
  contract** (F-18/F-20; F9's signal algebra IS the `rankable` ranking contract that 0.8.20's projection
  registry grafts). ONNX (#4) consumes I-3, feeds I-4 (→0.8.18).
- **Prereqs:** worktree off `$(git rev-parse origin/main)`; MAIN-tree maturin/GPU; GPU-eval mandate (fidelity
  gates CPU same-backend). Commission `/goal complete 0.8.16` against `0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md`
  + `plan-0.8.16.md`; anti-stall directive (poll git + task-mtime + `ps`).

## Owed / open decisions (HITL) — the live queue

1. **Commission 0.8.16** (F9 + ONNX) — active next release; on the Memex-value critical path.
2. **OPP-12 build authorization** for the **0.8.19/0.8.20** slots (build ≠ adopt; C-1 ratified both sides, but
   the build is a separate gate). Then author `plan-0.8.19`/`plan-0.8.20` ladders as they approach.
3. **0.9.0 identity** — now OPEN (OPP-12 + scale-bound moved out). Decide 0.9.x's role (likely the 1.0
   exit-beta runway) as the 0.8.x tail nears close; the old `0.9.x-roadmap-DRAFT.md` §7 decisions largely
   collapsed (only publish-cadence / 1.0-runway calls remain).
4. **TC-5** eu7 0.90-floor re-baseline (grown corpus); **TC-6** `embed_batch_cls` TS-parity; **TC-4** GPU-default
   eval-harness tooling placement.
5. **`release.yml`** publish/actionlint red → Library-Sweep / CI-integrity; **market-doc** 9 MD025/MD001
   (fix/archive/ignore).
6. **Follow-up (Steward): §5/§9 narrative-prose sweep** in the master — still carries pre-F-19 sequencing
   (banner/finding-reconciled, but the §5b/§9 one-liners reference the old order).

## Standing guardrails (load-bearing)

- **Push-scope fathomdb-only** — never push memex without a per-push directive each time.
- **Ledger tools:** `ledgerwrite`/`ledgerwatch` for every ledger op (`--strategy section` for the prose `.md`
  leverage ledger). `[[use-ledger-tools-for-all-ledger-ops]]`.
- **Durable record:** write substantive positions to the ledger + ADRs; chat only points at them.
  `[[write-responses-to-ledger-and-adrs]]`.
- **Hand-off filename** `STEWARD-SESSION-HANDOFF-YYYY-MM-DD-A.md` (sequential alpha). `[[steward-handoff-filename-format]]`.
- **Memex ledger:** no FATHOM append without HITL agree; full unfiltered tail read before any append.
- **Numbering:** two-tier (`x.y.z` real · `x.y.z.p` pico label-only · `13` forbidden); **even=publishable w/
  HITL, odd may publish by HITL exception**; publish always a separate per-`x.y.z` gate.
- **GPU-eval mandate:** eval/embed/rerank on the 3090s when there's room; **fidelity gates run CPU same-backend**.
- **codex §9** gates commissioned code (`/code-review` / adversarial-subagent = sanctioned fallback). **Verify from git.**
- **Background agents die silently** → poll git + task-mtime + `ps`; anti-stall directive when commissioning.

## Memory pointers

`[[use-ledger-tools-for-all-ledger-ops]]` · `[[write-responses-to-ledger-and-adrs]]` ·
`[[steward-handoff-filename-format]]` · `[[opp12-record-lifecycle-protocol]]` ·
`[[0.8.x-release-numbering-publish-governance-policy]]` · `[[repo-must-use-gpu-for-eval-when-3090-room]]` ·
`[[release-dod-requires-full-workspace-gate]]` · `[[push-scope-fathomdb-only]]` · `[[steward-delegate-dont-hand-do]]`.
