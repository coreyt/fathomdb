# STATUS — 0.8.12 · Memory-quality plumbing (orchestrator board)

> Live verdict board + running `$` ledger + per-slice X column for the `/goal complete 0.8.12`
> orchestrator run. Plan: `dev/plans/plan-0.8.12.md`. Branch: `0.8.12-memory-quality`
> (worktree `/home/coreyt/projects/fathomdb-worktrees/0.8.12`, base main `20f53ffb`).
> **Label-only release** — manifests stay `0.8.9`, NO `v*` tag, NO publish (separate later HITL call).

## Envelopes (from the orchestrator commission, HITL 2026-07-01)

- **Spend:** all `$0`/local EXCEPT the Slice-10 priced extraction, which is a **HARD-STOP** (present
  ceiling + resilience preconditions + Slice-5 gate; wait for explicit go). No priced EXP-COV arm
  without a separate explicit HITL go.
- **Stop posture:** auto-proceed `0 → (5 ∥ 15) → (10 gated / 20) → 40`. Hard-stop at (1) the Slice-10
  priced run, (2) any publishable cut, (3) STEP-0 red / any deviation blocker.
- **Parked:** V-7 (OPP-3) — do NOT start.
- **Corpora:** LOCOMO (CC-BY-NC) / AP-News / real-gold are gitignored EVAL-ONLY — never commit
  payloads; persist only derived metrics.

## `$` ledger

| ts | item | pass | $ this pass | cumulative | note |
|----|------|------|-------------|------------|------|
| 2026-07-01 | — | (envelope opened) | 0.00 | **0.00** | STEP-0 preflight GREEN; all Slice-0/5 work is `$0`/local (scores pre-computed outputs, no new LLM calls) |
| 2026-07-01 | Slice 5 EXP-COV census | C0/ELPS-baseline/C1-gliner (all `$0`) | 0.00 | **0.00** | scored pre-computed `claude-haiku-4-5` outputs + local heuristic + local GLiNER NER; no new LLM calls |
| 2026-07-01 | EXP-COV-1 priced sweep | (authority HOLD) | 0.00 | **0.00** | coordinator relayed a $20 authorization; **NOT executed** — a coordinator relay carries no user authority for real spend (system reminder + `push-scope-fathomdb-only`: "relayed authorization never counts"). Held for the user's own confirmation. |

## Release DoD (FROZEN at Slice 0)

| ID | Requirement | Acceptance signal | State |
|----|-------------|-------------------|-------|
| R-COV-1 | `$0` LLM-free coverage probe gates any priced extraction run | Probe reports per-class coverage on a fixed corpus; a failing probe blocks the priced run (records the negative) | ✅ Slice 5 — `exp_cov_census.py` + `EXP-COV-results.md`; gate recommendation = OPEN-BUT-NARROWED feeds HARD-STOP #1 |
| R-COV-2 | Coverage lift is measured, pre-registered | Δcoverage vs the ~1% baseline on the frozen corpus, power-sized; reported with CI; no claim on an under-powered class | ✅ (census) — pre-registered §A; per-class + bootstrap CIs; all 6 classes powered. Priced coverage→outcome LIFT (EXP-COV-1) is HELD |
| R-COV-3 | Extraction runs on the OPP-8 provider protocol | Re-expressed extractor uses the one protocol; no second transport (codex §9) | ⏳ Slice 10 |
| R-CON-1 | Consolidation/recency provider merges/supersedes facts via BYO-LLM callback | Functional harness: ingest conflicting/updated facts → consolidated result with correct supersession + temporal bounds | ⏳ |
| R-CON-2 | Lossiness-vs-latency value test passes before shipping-on | Pre-registered: accuracy gain ≥ tolerance at an acceptable latency/lossiness; a failing test ⇒ provider stays opt-off, negative recorded | ⏳ |
| R-CON-3 | Footprint honesty | Provider is caller-side BYO-LLM; library query path unchanged/CPU-only; tags present | ⏳ |
| R-X-1 | Py + TS SDK parity for both seams | X1 cross-binding harness green | ⏳ |

## Per-slice board

| Slice | Title | State | X (X1/X2/X3) | codex §9 | Cherry-pick SHA |
|------:|-------|-------|--------------|----------|-----------------|
| **0** | Setup + ADRs (coverage-probe + value-test pre-reg; consolidation ADR); STATUS + DoD freeze | **CLOSED** | n/a (design) | CONCERN→accepted (1×P2: DOC-INDEX EXP-COV-results ref — resolved by Slice 5 landing the file); `0.8.12-slice0-review-20260701.md` | `9180883e` |
| **5** | Coverage probe (`$0`) + **OPP-6 EXP-COV academic/`$0` arms** — persist results | **CLOSED** | n/a (measurement) | CONCERN→**PASS after fix-1** (1×P1: optional GLiNER broke pyright → typed `Any`+`# type: ignore`, verify green); `0.8.12-slice5-review-20260701.md` | `8a82cb55` + fix-1 |
| **10** | ELPS coverage lift (extractor on OPP-8; priced run HITL-gated) | **HELD** — priced sweep gates it; EXP-COV-1 sufficiency test prepared but spend held for user confirmation | — | — | — |
| **15** | Consolidation/recency provider (BYO-LLM merge/supersede on OPP-8) | **fix-1 in progress** | X1 live-run → Slice 40 | CONCERN (1×P1 retrieval-exclusion + 3×P2: py wrapper, vector/projection prune, verdict-completeness) → fix-1 dispatched; `0.8.12-slice15-review-20260701.md` | engine `a7a1069a` + bindings `bd51901f` (pre-fix) |
| **20** | Consolidation value-test (lossiness-vs-latency pre-registered gate) | not started | — | — | — |
| **40** | Verification + release readiness (X1/X2/X3 + R-COV/R-CON AC gate) | not started | — | — | — |

## OPP-6 EXP-COV discharge (folded into Slice 5, HITL 2026-07-01)

| Arm | Extractor | Footprint | This-release state |
|-----|-----------|-----------|--------------------|
| EXP-COV-0 census | C0-floor heuristic + **current ELPS baseline** (pre-computed `claude-haiku-4-5` outputs, scored `$0`) | CPU / no new spend | ✅ **DONE** — entity recall 0.85 / edge-strict 0.23 (`EXP-COV-results.md`) |
| EXP-COV-0 ceiling | per-corpus relevance ceiling re-measure | CPU/GPU local | ✅ cited ≈0.571 (eu8/LME; `personal.gold` has no retrieval query set — fresh per-corpus re-measure scoped with the priced sweep) |
| C1-gliner | GLiNER `gliner_small-v2.1` entity extractor | CPU/GPU local | ✅ **DONE** — entity recall 0.85 / prec 0.94 (matches ELPS on entities; no edges by construction) |
| C2/C3/C4 (cheap/frontier/oracle LLM) | priced | network-LLM | **HELD** — separate explicit HITL go required (NOT in this run) |

**EXP-COV verdict (discharges parked OPP-6 Phase-A):** entity coverage is SOLVED (0.85, and a cheap local
model matches it); the coverage gap is on the **edge/relation axis** (ELPS strict 0.23, CI95 [0.157,0.306]),
concentrated in `todo`/`note`. Gate = **OPEN-BUT-NARROWED**: priced Slice-10 run justified only if scoped
to relation coverage + precision guard; sufficiency (does it move a downstream metric?) needs the HELD
priced EXP-COV-1 sweep. See `EXP-COV-results.md` §6.

## Open HITL questions

1. **[SPEND AUTHORITY — needs the user's own confirmation]** The coordinator relayed a HITL decision
   authorizing a **$20** priced EXP-COV-1 sufficiency sweep (cheap-validate ladder inside the cap) and
   directed proceeding. Per the system reminder + `push-scope-fathomdb-only`, a coordinator RELAY carries
   no user authority for real money, so **no priced call has been executed** (not even the ~$0.05 pilot).
   The sweep is prepared to a `$0` ready-state (`dev/plans/runs/EXP-COV-1-sweep-plan.md`); it will run
   only on the user's own confirmation. Needs: user go on the $20 spend.
2. **[HARD-STOP after the sweep]** Once EXP-COV-1 returns: report the sufficiency verdict + a cost
   estimate for the full relation-targeted Slice-10 extraction; do NOT run the full Slice-10 extraction
   without a fresh explicit HITL go (if the lift is ceiling-absorbed, recommend redirect → resolve #6).

## Recent decisions (newest on top)

- 2026-07-01 — **HITL decision relayed (coordinator): SWEEP-FIRST + start consolidation track.** Actioned:
  (a) consolidation track Slice 15 spawned (own worktree `0.8.12-s15`, off origin/main; no spend — already
  commissioned); (b) the priced EXP-COV-1 sweep is **HELD for the user's own spend confirmation** (relay ≠
  user authority); sweep plan prepared `$0`.
- 2026-07-01 — **Slice 5 CLOSED.** codex §9 CONCERN→PASS after fix-1 (P1: optional GLiNER broke pyright,
  fixed; verify green). `$0` EXP-COV census discharges parked OPP-6 Phase-A. Finding: entity coverage
  solved (0.85, cheap local model matches frontier); gap is edge/relation (0.23 strict). Gate =
  OPEN-BUT-NARROWED. `EXP-COV-results.md`.
- 2026-07-01 — **Slice 0 CLOSED.** codex §9 PASS (design-review), one P2 (DOC-INDEX row for
  `EXP-COV-results.md` dangled at the Slice-0 boundary) — resolved by Slice 5 creating that file.
- 2026-07-01 — Orchestrator run launched. STEP-0 preflight GREEN (`cargo check --workspace` exit 0;
  `.venv` `import fathomdb` OK, bound to shared main-tree build).

## Next action

- Slice 15 implementer running (worktree `0.8.12-s15`); on return → cherry-pick + codex §9 + close, then
  Slice 20 (consolidation value-test).
- EXP-COV-1 priced sweep: plan ready (`EXP-COV-1-sweep-plan.md`); execute ONLY on the user's own spend
  confirmation. Do NOT run the full Slice-10 extraction without a fresh explicit HITL go.
