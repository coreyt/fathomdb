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

## Release DoD (FROZEN at Slice 0)

| ID | Requirement | Acceptance signal | State |
|----|-------------|-------------------|-------|
| R-COV-1 | `$0` LLM-free coverage probe gates any priced extraction run | Probe reports per-class coverage on a fixed corpus; a failing probe blocks the priced run (records the negative) | ⏳ |
| R-COV-2 | Coverage lift is measured, pre-registered | Δcoverage vs the ~1% baseline on the frozen corpus, power-sized; reported with CI; no claim on an under-powered class | ⏳ |
| R-COV-3 | Extraction runs on the OPP-8 provider protocol | Re-expressed extractor uses the one protocol; no second transport (codex §9) | ⏳ |
| R-CON-1 | Consolidation/recency provider merges/supersedes facts via BYO-LLM callback | Functional harness: ingest conflicting/updated facts → consolidated result with correct supersession + temporal bounds | ⏳ |
| R-CON-2 | Lossiness-vs-latency value test passes before shipping-on | Pre-registered: accuracy gain ≥ tolerance at an acceptable latency/lossiness; a failing test ⇒ provider stays opt-off, negative recorded | ⏳ |
| R-CON-3 | Footprint honesty | Provider is caller-side BYO-LLM; library query path unchanged/CPU-only; tags present | ⏳ |
| R-X-1 | Py + TS SDK parity for both seams | X1 cross-binding harness green | ⏳ |

## Per-slice board

| Slice | Title | State | X (X1/X2/X3) | codex §9 | Cherry-pick SHA |
|------:|-------|-------|--------------|----------|-----------------|
| **0** | Setup + ADRs (coverage-probe + value-test pre-reg; consolidation ADR); STATUS + DoD freeze | IN-FLIGHT | n/a (design) | pending | — |
| **5** | Coverage probe (`$0`) + **OPP-6 EXP-COV academic/`$0` arms** — persist results | not started | — | — | — |
| **10** | ELPS coverage lift (extractor on OPP-8; priced run HITL-gated) | not started | — | — | — |
| **15** | Consolidation/recency provider (BYO-LLM merge/supersede on OPP-8) | not started | — | — | — |
| **20** | Consolidation value-test (lossiness-vs-latency pre-registered gate) | not started | — | — | — |
| **40** | Verification + release readiness (X1/X2/X3 + R-COV/R-CON AC gate) | not started | — | — | — |

## OPP-6 EXP-COV discharge (folded into Slice 5, HITL 2026-07-01)

| Arm | Extractor | Footprint | This-release state |
|-----|-----------|-----------|--------------------|
| EXP-COV-0 census | C0-floor heuristic + **current ELPS baseline** (pre-computed `claude-haiku-4-5` outputs, scored `$0`) | CPU / no new spend | **Slice 5 (this run)** |
| EXP-COV-0 ceiling | per-corpus relevance ceiling re-measure | CPU/GPU local | **Slice 5 (this run)** |
| C1-gliner | GLiNER entity extractor | CPU/GPU local | Slice 5 if `gliner` installs cleanly; else recorded deferred (local-but-needs-model-download, non-blocking) |
| C2/C3/C4 (cheap/frontier/oracle LLM) | priced | network-LLM | **HELD** — separate explicit HITL go required (NOT in this run) |

## Open HITL questions

1. **[HARD-STOP #1]** Slice-10 priced extraction — pending the Slice-5 gate result + `$` ceiling +
   resilience preconditions (to be presented when Slice 5 closes).

## Recent decisions (newest on top)

- 2026-07-01 — Orchestrator run launched. STEP-0 preflight GREEN (`cargo check --workspace` exit 0;
  `.venv` `import fathomdb` OK, bound to shared main-tree build). Slice 0 in flight.

## Next action

Complete Slice 0 (ADRs + this board + DoD freeze), codex §9, then fan out Slice 5 (`$0` EXP-COV
census) ∥ Slice 15 (consolidation provider).
