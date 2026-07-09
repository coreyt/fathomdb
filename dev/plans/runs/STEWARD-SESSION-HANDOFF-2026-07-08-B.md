# FathomDB — Steward Session Hand-off (2026-07-08-B)

> **Boot:** run **`/steward`** (loads `.claude/agents/steward.md` + `dev/plans/prompts/0.8.x-STEWARD-HANDOFF.md`),
> do its §3 cold-start reading, then read THIS doc, return a short orientation, and **WAIT for HITL** before
> mutating. You are the **Program Steward**: keep the schedule-of-record true to git, commission + verify
> release orchestrators, propose-first to the HITL. **Do not implement code or hand-drive a ladder.**
> *(Supersedes 2026-07-08-A. 0.8.16 closed this session; the line is now pointed at 0.8.18.)*

---

## ★ THE NEXT STEP — commission 0.8.18 (do this first)

**0.8.18 is the next release and the pivot point of the whole tail.** It carries **#5 vector-equivalence +
#11-full publish matrix** — and **#11-full is the publish prerequisite for OPP-12's 0.8.20 breaking-pair
publish** (the single thing Memex is waiting on). So 0.8.18 is squarely on the Memex-value critical path.

**Before you commission it — reconcile the plan to the record first (this is the load-bearing habit).**
`plan-0.8.18.md` exists but predates the F-19/F-20 resequence. Just as `plan-0.8.16` silently omitted the
F9→`rankable` constraint last session, check `plan-0.8.18` against master §4 + F-19/F-20 and inject what's
missing **before** the orchestrator freezes its Slice-0 DoD. Specifically confirm the plan states:
- **#11-full publish matrix is the enabler for the 0.8.20 OPP-12 coordinated breaking-pair publish** (not a
  standalone GA nicety) — 0.8.18 hardens the publish machinery that 0.8.20 uses.
- **#5 vector-equivalence** should **consume the R-ONNX-3 hand-off from 0.8.16**: the candle↔ONNX Δ was
  measured **same-arch only (ONNX-CPU vs candle-CPU, 0 flips)**; the **cross-backend Δ (GPU-EP vs CPU) is
  unmeasured** and is exactly what #5's tolerance must calibrate against. This is a real 0.8.18 input, not a
  footnote.

**Then commission** `/goal complete 0.8.18` as a background orchestrator, exactly as 0.8.16 ran (the workflow
below is proven). **Re-propose the standing landing mandate to the HITL** if you want it — the 0.8.16 one was
release-scoped and is now **spent**. Note #5 and #11-full both touch the vector path / release machinery, so
expect closer HITL involvement than 0.8.16 (publish-matrix changes, any tolerance-floor call).

**After 0.8.18:** OPP-12 **@ 0.8.19 (Phase-1, label-only) + 0.8.20 (Phase-2 + breaking-pair publish)** —
**build-authorized (F-21)**. Their ladders don't exist yet (see Drift, below). Author them as the slot nears.

---

## Verified state (git, this session's close)

- **`main` = `origin/main` = `5653253c`**, clean. Verify `git rev-parse --abbrev-ref HEAD` = `main` before any commit.
- **0.8.16 ✅ COMPLETE** (label-only `8c6b92aa`, HITL close — manifests stay `0.8.9`): F9 (OFF-by-default,
  OPP-12-`rankable`-forward, SCHEMA 17→18) + cross-vendor ONNX (opt-in `onnx-embedder` feature, **zero engine
  diff**, offline export, candle↔ONNX cosine≡1.0 / 0-flip). All mod-5 slices closed; eu7 no-op basis. See
  `[[0.8.16-complete-f9-onnx]]`.
- **Ledgers** (use `ledgerwrite`/`ledgerwatch`, never hand-edit): steward `dev/steward/steward-ledger.jsonl`
  **@ seq 57**; todos `dev/todos-and-considerations-ledger.jsonl` **TC-1…TC-9** (TC-8 = OPP-12 C-2 id-swap
  **build-authorized**, 0.8.19; TC-9 = `ort` 2.0-stable bump; TC-5 = eu7 grown-corpus floor re-baseline);
  OPP-12 C-1 sub-ledger **@ seq 8** (C-1 RATIFIED both sides).
- **Other-session worktrees** (`0.5.1-memex-build`, `0.8.11.2`, `tooling-mermaid-renderer`) — **leave them.**

## Program shape — OPP-12 + scale-bound are 0.8.x deliverables (master §4 + F-19/F-20/F-21)

| Micro | Pub | Content | Status |
|---|---|---|---|
| 0.8.16 | even | F9 + ONNX | ✅ COMPLETE (label-only) |
| **0.8.18** | even | **#5 vec-equiv + #11-full publish matrix** | **← NEXT** (OPP-12 publish prereq) |
| 0.8.19 | odd (label-only) | **OPP-12 Phase-1** — existence axis · `transition`/`purge` · **C-2 id swap (TC-8)** · schema mig · X1 | build-auth (F-21); ladder TBA |
| 0.8.20 | even (publish) | **OPP-12 Phase-2 + breaking-pair publish** — read-modes · node-validity · **projection registry (C-1 co-land)** · `dense_readiness`; Memex `0.5.x-successor` pairs | build-auth (F-21); ladder TBA |
| 0.8.21 / 0.8.22 | odd / even | free-threading + #13 bench · dep migrations (napi/rusqlite) | plans mis-homed (see Drift) |
| 0.8.23 / 0.8.24 | odd / even | **F-17 scale-bound** soft/stated — 0.8.x-cohesive capstone | — |
| 0.9.0 | — | opens **only after** 0.8.x cohesive; identity **OPEN** | — |

0.8.15 / 0.8.17 stay ⏸ **PARKED** (dispatcher + hardening; zero OPP-12 dependency).

## Memex collaboration (the essential model)

- **Memex is the primary consumer.** Its **entire remaining FathomDB need = OPP-12** (F-18); F9/ONNX were
  **not** on its critical path. **Memex-value critical path:** `0.8.16 (F9) ✓ → 0.8.18 (#11-full publish) →
  0.8.19 (OPP-12 P1) → 0.8.20 (OPP-12 P2 + breaking-pair publish; Memex 0.5.x-successor pairs)`.
- **OPP-12 C-1 (EntityTypeSpec→ProjectionSpec) is RATIFIED both sides**, co-lands **@ 0.8.20** (design
  unchanged; the break-if-late `AttributeSpec.index` field is frozen inert in Memex Commission C). Contract:
  `dev/design/record-lifecycle-protocol/OPP-12-C1-converged-contract.md`. See `[[opp12-record-lifecycle-protocol]]`.
- **Memex self-maintains its own ledger.** This session Memex independently committed the OPP-12 **re-slot to
  0.8.19/0.8.20** into its `dev/fathomdb/LEVERAGE-OPPORTUNITIES-LEDGER.md` + ROADMAP (memex `89067a2`). **So
  read cross-repo OPP-12 state from the Memex ledger (`ledgerwatch`), don't push it.** The only unmirrored
  FathomDB fact is the **F-21 build-auth** — low-value (build≠adopt is FathomDB-internal); mention it if
  co-designing, don't chase a push. **Never push memex** (per-push HITL directive each time); no FATHOM
  ledger append without HITL agreement.

## Drift / follow-ups to reconcile (none physically-hard)

1. **OPP-12 ladders don't exist.** `plan-0.8.19.md` still holds the **pre-F-19 "free-threading + bench"**
   content and `plan-0.8.20.md` the **"dep-migrations"** content (both now allocated to **0.8.21 / 0.8.22**).
   When the OPP-12 slot nears: author the Phase-1/Phase-2 ladders and **re-home** free-threading→`plan-0.8.21`,
   deps→`plan-0.8.22`. Recorded in **F-21**. (Not urgent — two releases out, after 0.8.18.)
2. **Master §5/§9 narrative prose** still carries pre-F-19 one-liners (`OPP-12@0.9.x`, `0.8.19=free-threading`,
   `0.8.18=end-of-line`). Banner/§4/findings are reconciled; the prose sweep is the residual (F-20). Low priority.
3. **Carry-forwards:** 0.8.18 #5 = cross-backend ONNX Δ (unmeasured, above); **TC-5** eu7 floor re-baseline;
   **TC-9** `ort` 2.0-stable bump (LBS candidate).

## The workflow that worked this session (reuse it)

- **Commission** a release as a **background orchestrator** pointed at `plan-0.8.z.md` + the ORCHESTRATOR-HANDOFF
  **§0/§1/§5/§6/§7 ONLY** (**§2/§3/§4/§8 are STALE 0.8.3-era — tell the orchestrator to ignore them**).
- **Standing landing mandate** (HITL, per-release): clean **codex §9 PASS** slices land under **Steward
  authority** (you still **verify from git** every time); **schema migration / codex override / BLOCK / publish
  / adoption-default change** stay **HITL**. It streamlined 0.8.16's non-migration slices; re-propose per release.
- **codex §9 is a real gate** — it caught genuine correctness + parity gaps on *every* 0.8.16 slice (vacuous
  tests, dropped SDK-wrapper fields, a broken ONNX fallback). Trust it; have the orchestrator **drive the
  fix→review loop to a terminal PASS/BLOCK in-turn** (background agents stop between detached codex runs — tell
  it to block in-foreground so you don't ping-pong resumes).
- **Verify every orchestrator claim from git before relaying/recording** — this session it repeatedly mattered
  (a "sub-ledger already current" finding, a landing that was really on a slice branch not main, etc.).
- **Shared-tree coordination:** during any orchestrator cherry-pick/push, **hold off `main` yourself** and let
  it report the landed sha, then verify. Guardrail added this session: root-anchored `/output.json` gitignore
  so a worktree closure witness can't be stray-committed (`[[guardrail-failures-fix-tooling-not-people]]`).
- **eu7 no-op basis** is legitimate when the **default vector path is byte-unchanged** (grounded from git) —
  don't run the full empirical suite just to re-hit the TC-5 floor (policy `649a8d45`, precedent 0.8.14 D6).

## Standing guardrails (load-bearing)

- **Push-scope fathomdb-only**; **two-tier numbering** (`x.y.z` real · `x.y.z.p` pico · `13` forbidden);
  **even=publishable, odd may publish by HITL exception**; **publish is always a separate per-`x.y.z` HITL call**
  (0.8.16 was label-only close). **build ≠ adopt** (F-21 OPP-12 build-auth ≠ adoption, ≠ publish).
- **Footprint invariant:** library query path CPU-only, 1-bit Hamming, deterministic; only LLM seam = BYO-LLM
  in the caller; GPU/frontier = OFFLINE-BUILD/EVAL. **GPU-eval mandate** (3090s cuda:0/1, exclude K620);
  **fidelity gates CPU same-backend.**
- **Release DoD:** full-workspace `cargo clippy` + `cargo check` both exit 0 (`[[release-dod-requires-full-workspace-gate]]`).
- **Ledger tools** for every ledger op; the prose leverage ledger is direct-Edit, JSONL is `ledgerwrite`-only.
- **Direction / record / release-slot changes are always the HITL's** — propose + recommend, never self-widen.

## Memory pointers

`[[0.8.16-complete-f9-onnx]]` · `[[opp12-record-lifecycle-protocol]]` · `[[0.8.x-release-numbering-publish-governance-policy]]` ·
`[[gpu-not-retrieval-lever-hnsw-at-0.8.20]]` · `[[repo-must-use-gpu-for-eval-when-3090-room]]` ·
`[[release-dod-requires-full-workspace-gate]]` · `[[steward-delegate-dont-hand-do]]` ·
`[[background-agent-silent-death-proactive-check]]` · `[[use-ledger-tools-for-all-ledger-ops]]` ·
`[[write-responses-to-ledger-and-adrs]]` · `[[steward-handoff-filename-format]]` · `[[push-scope-fathomdb-only]]`.
