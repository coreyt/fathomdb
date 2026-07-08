# FathomDB — Steward Session Hand-off (2026-07-08)

> **Boot:** run **`/steward`** (loads `.claude/agents/steward.md` + `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md`),
> do its §3 cold-start reading, then read THIS doc for current state, return an orientation paragraph, and
> **WAIT for HITL** before mutating. You are the **Program Steward**: keep the schedule-of-record true to git,
> place cross-cutting work, commission + verify, propose-first to HITL. **Do not implement code or hand-drive a
> ladder.** *(Supersedes the 2026-07-07 hand-off.)*
>
> **★ FIRST TWO ACTIONS THIS SESSION (HITL-directed): (1) CHECK THE PLANS are true to the ledgers/ADRs (they
> were reconciled at this hand-off — verify from git). (2) PREPARE TO COMMISSION 0.8.16** — see §"Next" below.

## Git state — read first

- **`main` is clean and pushed.** No mid-flight worktrees of mine. Parked/other-session worktrees
  (`0.5.1-memex-build`, `0.8.11.2`, `tooling-mermaid-renderer`) — **leave them.** Verify
  `git rev-parse --abbrev-ref HEAD` (expect `main`) before any commit.
- **Ledgers (use `ledgerwrite` to append, `ledgerwatch` to read — NEVER hand-edit; HITL discipline 2026-07-07):**
  - Steward `dev/steward/steward-ledger.jsonl` @ **seq ~40**.
  - Todos/considerations `dev/todos-and-considerations-ledger.jsonl` — **TC-1…TC-8** (TC-7 = C-1 timing,
    `converged-pending-hitl`; TC-8 = Cause-A typed swap, `open`/P0).
  - **OPP-12 C-1 sub-ledger** `dev/design/record-lifecycle-protocol/OPP-12-sub-ledger.jsonl` @ **seq 5**
    (protocol · SHOT-1 · SHOT-2 · reconcile · resolve-parked) — **C-1 CONVERGED**.
- **Memex side:** ratified the FathomDB leverage-ledger reconciliation (`memex 6f7456c`) and committed its C-1 ADR
  (`memex dev/design/entity-schema-registry/ADR-C1-eav-projection-lifecycle.md`). Never push memex.

## What landed this session (context, not action)

1. **Memex leverage-ledger reconciliation — RATIFIED both sides (`memex 6f7456c`).** OPP-6 →
   `RESOLVED — DE-PRIORITIZED` (F-15 EXP-COV-1 negative); **OPP-11 → `SIGNED`**; OPP-1/3/12 landed-state
   (OPP-1 "→ Memex 0.5.5" is aspirational, NOT a Memex schedule). Drift was all FathomDB-ahead-of-ledger.
2. **Memex ranked-needs weighting → master F-18.** **OPP-12 (≥0.9.x) is the single major FathomDB-side blocker
   for Memex** (both P0 needs are OPP-12; #3–5 low/no value). **Memex-value critical path:**
   `0.8.16 (F9) → 0.8.18 (publish matrix) → 0.9.0 → 0.9.1 → 0.9.2`. New edge **F9 (0.8.16) → OPP-12 `rankable`
   role (0.9.1)**. **0.8.15 dispatcher PARKED (HITL)** — serves no top-5 Memex need, V-7-gated, **ZERO OPP-12
   dependency (verified)**; next commission = **0.8.16**.
3. **OPP-12 C-1 co-design — CONVERGED (bounded 2-shot loop).** Contract:
   `dev/design/record-lifecycle-protocol/OPP-12-C1-converged-contract.md` (FathomDB seed for the 0.9.1 P2·S0 ADR)
   + Memex ADR. One engine-owned EAV; Memex persists spec / engine `ProjectionSpec` = derived boot cache; engine
   needs no `provisional` concept; `rankable` graceful-absent until F9; apply-atomicity + defaults resolved at
   contract level (nothing left but the FathomDB-internal 0.9.1 txn boundary). **The break-if-late
   `AttributeSpec.index` role hint is already frozen inert in Memex Commission C (`memex 95ed450`) — timing risk
   retired.** Status: **RATIFIED-pending-HITL**.
4. **Plans reconciled to the ledgers/ADRs:** master F-15/F-18 + §4 (0.8.15 ⏸ PARKED, 0.8.16 next);
   `0.9.x-roadmap-DRAFT.md` §4a/§7.7/§3b/§5 (C-1 CONVERGED). `plan-0.8.16.md` already exists.
5. **Two standing process rules adopted (memory):** `ledgerwrite`/`ledgerwatch` for **all** ledger read/write/watch;
   **write substantive responses to the ledger + ADRs** (durable), chat only points at them.

## ★ Next — prepare to commission 0.8.16 (the active next release)

- **`plan-0.8.16.md` exists** (theme: ranking signal & embedder reach — **#15 F9** + **#4 cross-vendor ONNX**).
- **F9 must be specced with the OPP-12 `rankable` projection-role contract in mind** (master F-18 forward edge;
  0.9.x draft §3/§4a) — F9's signal algebra IS the `rankable` ranking contract that OPP-12's projection registry
  grafts at 0.9.1. This is why 0.8.16 is ON the Memex-value critical path.
- **ONNX (#4)** consumes I-3 (GPU device seam @0.8.7), feeds I-4 (→0.8.18 vector-equivalence).
- **Prereqs before commissioning (release §7):** worktree hygiene off `$(git rev-parse origin/main)`;
  MAIN-tree-only maturin/GPU builds; GPU-eval mandate (fidelity gates CPU same-backend). Commission via
  `/goal complete 0.8.16` against `0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md` + `plan-0.8.16.md`; give an anti-stall
  directive (background agents die silently — poll git + task-mtime + `ps`).

## Owed / open decisions (HITL) — the live queue

1. **Ratify the C-1 converged contract** (`OPP-12-C1-converged-contract.md`) — one ratification closes C-1 as a
   converged design (build ≠ adopt; schedules nothing until 0.9.x).
2. **Commission 0.8.16** (F9 + ONNX) — active next release / Memex critical path (see §Next).
3. **0.9.x roadmap §7 — 7 of 8 decisions still open** (§7.7 C-1-ordering now RESOLVED): micro slots for OPP-12 ·
   atomic 0.9.2 publish · 0.9.3-is-odd tension · Memex-pair version · when Cause-A first publishes · when 0.9.0
   opens · 0.9.0 build-auth. Plus: **graduate the DRAFT → a real `0.9.x-PROGRAM-SEQUENCING.md`** (gains urgency —
   the 0.8.x tail is now explicitly the OPP-12 runway).
4. **TC-5** eu7 0.90-floor re-baseline (grown 18,472-doc corpus, CPU same-backend); **TC-6** `embed_batch_cls`
   TS-parity; **TC-4** GPU-default eval-harness tooling placement.
5. **`release.yml` publish/actionlint red** → Library-Sweep / CI-integrity track (pre-existing, git-proven not
   0.8.14); **market-doc** `dev/research/personal-agent-database-market-*.md` — 9 MD025/MD001 (fix/archive/ignore).
6. **OPP-12 implementation scheduling** — breaking ⇒ ≥0.9.x + a coordinated **Memex 0.5.x-successor** pair; can't
   start until the 0.9.x line is planned (§7). C-1 is now design-converged; C-2 typed swap = **TC-8** (P0, not started).

## Standing guardrails (load-bearing)

- **Push-scope fathomdb-only** — never push memex (or any other repo) without a per-push directive each time.
- **Ledger tools:** `ledgerwrite`/`ledgerwatch` for every ledger read/update/watch (`ledgerwatch --strategy
  section` for the prose `.md` leverage ledger). See `[[use-ledger-tools-for-all-ledger-ops]]`.
- **Durable record:** write substantive positions to the ledger + ADRs; chat summarizes. `[[write-responses-to-ledger-and-adrs]]`.
- **Memex ledger:** no FATHOM append without HITL discuss-and-agree; full unfiltered tail read before any append.
- **Two-tier numbering** (`x.y.z` real · `x.y.z.p` pico label-only · **`13` forbidden**); publish = separate HITL gate.
- **GPU-eval mandate:** eval/embed/rerank on the 3090s when there's room; **fidelity gates (eu7) run CPU same-backend**.
- **codex §9** gates commissioned code (`/code-review` / adversarial-subagent = sanctioned fallback). **Verify from git.**
- **Background agents die silently** → poll git + task-mtime + `ps`; give an anti-stall directive when commissioning.

## Memory pointers

`[[use-ledger-tools-for-all-ledger-ops]]` · `[[write-responses-to-ledger-and-adrs]]` ·
`[[opp12-record-lifecycle-protocol]]` · `[[0.8.14-complete-substrate-recall]]` ·
`[[repo-must-use-gpu-for-eval-when-3090-room]]` · `[[opp1-v3-decomposition-verdict-adopt-hold-0.5.5]]` ·
`[[release-dod-requires-full-workspace-gate]]` · `[[push-scope-fathomdb-only]]` · `[[steward-delegate-dont-hand-do]]`.
