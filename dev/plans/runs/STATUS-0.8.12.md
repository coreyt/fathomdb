# STATUS — 0.8.12 · Memory-quality plumbing (orchestrator board)

> Live verdict board + running `$` ledger + per-slice X column for the `/goal complete 0.8.12`
> orchestrator run. Plan: `dev/plans/plan-0.8.12.md`. Branch: `0.8.12-memory-quality`
> (worktree `/home/coreyt/projects/fathomdb-worktrees/0.8.12`, **tip `63d19c2d`** as of 2026-07-02 Phase-3/4
> completion). Base-main pointer refreshed: the branch diverged from `main` at merge-base `20f53ffb`;
> **`main` is now `fe103a81`** (16 main-only docs/tooling commits the branch legitimately lacks — none
> touch `fathomdb-engine`/`src/ts`/`fathomdb-napi`; reconciled at the Phase-5 label-only merge, NOT rebased).
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
| 2026-07-01 | EXP-COV-1 priced sweep | (authority HOLD) | 0.00 | **0.00** | coordinator relayed a $20 authorization; held (relay ≠ user confirmation per the standing system reminder). |
| 2026-07-02 | EXP-COV-1 priced sweep | **HOLD LIFTED** — user's OWN message: "I approve the $20 spend. This is HITL approval." | 0.00 | **0.00** | direct user turn = valid HITL confirmation. Ceiling $20; cheap-validate ladder + §4 resilience mandatory; auto-stop. Delegated to a dedicated EXP-COV-1 implementer. |
| 2026-07-02 | EXP-COV-1 C-relation extraction | LOCOMO 272 sessions, relation-focused prompt (airlock) | **4.79** | **4.79** | ✅ COMPLETE, 0 failures, completeness guard satisfied; cost model $0.0179/doc; resilience PROVEN (real crash@229/272 → clean resume). Priced asset sits UNCOMMITTED in worktree `0.8.12-expcov1` (not lost; needs commit to preserve). |
| 2026-07-02 | EXP-COV-1 downstream sufficiency read | (`$0`, environment-BLOCKED) | 0.00 | **4.79** | CPU-embedder `.so` defect (~13s/doc, stalls @1500% CPU; no `embed-cuda`/GPU) forced dropping CE-rerank + dense arms → degraded held-fixed stack = FTS + structural graph-arm BFS (itself ~2.5–3s/query = an OPP-6 latency finding). C-none baseline measured: multi_session gold-in-pool@10 **0.468** (headroom class), temporal 0.913, factoid 0.979. C-relation-vs-C-none verdict NOT yet computed (sweep paused ~350/590 by HITL hold). |

## Release DoD (FROZEN at Slice 0)

| ID | Requirement | Acceptance signal | State |
|----|-------------|-------------------|-------|
| R-COV-1 | `$0` LLM-free coverage probe gates any priced extraction run | Probe reports per-class coverage on a fixed corpus; a failing probe blocks the priced run (records the negative) | ✅ Slice 5 — `exp_cov_census.py` + `EXP-COV-results.md`; gate recommendation = OPEN-BUT-NARROWED feeds HARD-STOP #1 |
| R-COV-2 | Coverage lift is measured, pre-registered | Δcoverage vs the ~1% baseline on the frozen corpus, power-sized; reported with CI; no claim on an under-powered class | ✅ (census) — pre-registered §A; per-class + bootstrap CIs; all 6 classes powered. Priced coverage→outcome LIFT (EXP-COV-1) is HELD |
| R-COV-3 | Downstream sufficiency verdict for coverage-lift (Phase 1-2) | Pre-registered decision rule applied on the full held-fixed GPU stack; verdict recorded | ✅ **RESOLVED-NEGATIVE** — EXP-COV-1 GPU verdict = **`CEILING-ABSORBED`** (`813d9a22`); every powered Δ vs same-stack C-none negative ⇒ OPP-6 #6 de-prioritized (HITL 2026-07-02). Slice 10 CLOSED. Cross-ref master **F-15** (`dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md`) + `EXP-COV-1-results.md` (on `0.8.12-expcov1-sweep`) |
| R-CON-1 | Consolidation/recency provider merges/supersedes facts via BYO-LLM callback | Functional harness: ingest conflicting/updated facts → consolidated result with correct supersession + temporal bounds | ✅ Slice 15 — `consolidate_provider.rs` 12/12 (recency invalidate w/ temporal bound + supersede + retrieval-exclusion) |
| R-CON-2 | Lossiness-vs-latency value test passes before shipping-on | Pre-registered: accuracy gain ≥ tolerance at an acceptable latency/lossiness; a failing test ⇒ provider stays opt-off, negative recorded | ✅ Slice 20 (value-test ran; outcome = **STAY-OFF**) — `$0` mechanism: precision 0.50→1.00, lossiness 0, query-latency ≈0. Default-ON NOT cleared for TWO named reasons: (1) no real-corpus at-power evidence (deferred); (2) exclusion not rebuild-durable → **blocker-before-default-ON** = add `t_invalid` filter to FTS/vec projection SQL (codex §9) — **CLEARED by Slice A** (`0c26703d`, codex §9 PASS): FTS/vec edge projection now rebuild-durable. Provider still ships default-OFF/opt-in (reason (1) real-corpus at-power evidence remains out of `$0` scope). `consolidation-value-test-results.md` |
| R-CON-3 | Footprint honesty | Provider is caller-side BYO-LLM; library query path unchanged/CPU-only; tags present | ✅ Slice 15 — no-egress guard for consolidate; CPU-only deterministic cluster assembly; tagged |
| R-X-1 | Py + TS SDK parity for both seams | X1 cross-binding harness green | ✅ **Py-live + TS-live** — Slice 40 Py live-verified (isolated venv, surface 3/3 + live consolidate); **Slice B** (`a1f6f5a3`+`79fbad6c`, codex §9 PASS) adds the live TS X1 (`functional-consolidate.test.ts`: real napi build + `fathomdb.consolidate.v1` subprocess + verdict applied end-to-end; `npm test` 131/131 exit 0). Also closed a masked governed-surface gap: `consolidate_with_provider`/`consolidateWithProvider` added to the shared allowlist → `test_surface.py` 15/15 + `surface.test.ts` green on BOTH bindings. |

## Per-slice board

| Slice | Title | State | X (X1/X2/X3) | codex §9 | Cherry-pick SHA |
|------:|-------|-------|--------------|----------|-----------------|
| **0** | Setup + ADRs (coverage-probe + value-test pre-reg; consolidation ADR); STATUS + DoD freeze | **CLOSED** | n/a (design) | CONCERN→accepted (1×P2: DOC-INDEX EXP-COV-results ref — resolved by Slice 5 landing the file); `0.8.12-slice0-review-20260701.md` | `9180883e` |
| **5** | Coverage probe (`$0`) + **OPP-6 EXP-COV academic/`$0` arms** — persist results | **CLOSED** | n/a (measurement) | CONCERN→**PASS after fix-1** (1×P1: optional GLiNER broke pyright → typed `Any`+`# type: ignore`, verify green); `0.8.12-slice5-review-20260701.md` | `8a82cb55` + fix-1 |
| **10** | ELPS coverage lift (extractor on OPP-8; priced run HITL-gated). **Held the EXP-COV-1 sufficiency experiment.** | **CLOSED (2026-07-02) — verdict `CEILING-ABSORBED`.** EXP-COV-1 GPU downstream sweep (`813d9a22` on `0.8.12-expcov1-sweep`): every powered Δ vs same-stack C-none negative (multi_session Δgip@10 −0.123 [−0.167,−0.078]/ΔMRR −0.227; temporal −0.069/−0.244) ⇒ coverage is not the retrieval lever; the embedder/retrieval ceiling binds. **R-COV-3 = resolved-negative; OPP-6 #6 de-prioritized** (HITL 2026-07-02; do NOT fund the ~$340 full relation pass). Master reconciled: **F-15**. Productization OUT of 0.8.12 (separate later HITL). `$0` (cache reused). | n/a (eval verdict) | n/a (verdict; not a code slice) | — (no code landed; recorded only) |
| **15** | Consolidation/recency provider (BYO-LLM merge/supersede on OPP-8) | **CLOSED** | X1 surface both bindings; live-run → Slice 40 | CONCERN(4)→fix-1(resolved 4, +1 new P2)→fix-2→**PASS**; `0.8.12-slice15-review-20260701.md` | `a7a1069a`,`bd51901f`,`065ffcc2`,`90261612`,`ffdda578` |
| **20** | Consolidation value-test (lossiness-vs-latency pre-registered gate) | **CLOSED** | n/a (eval) | CONCERN(1×P2 rebuild-durability)→fix-1 (reframe scope + named default-ON blocker; gate kept negative)→resolved; `0.8.12-slice20-review-20260702.md` | `bd9164f3` + fix-1 |
| **40** | Verification + release readiness (X1/X2/X3 + R-COV/R-CON AC gate) | **COMPLETE (2026-07-02)** — X1 Py-live + TS-live (Slice B), X2 `mkdocs build --strict` exit 0, X3 DOC-INDEX ok; **full R-COV/R-CON AC gate GREEN** with R-COV-3 resolved-negative. `0.8.12-slice40-verification.md` | X1 Py✅/TS✅ · X2✅ · X3✅ | n/a (verification) | — |
| **A** | `t_invalid` FTS/vec projection durability (R-CON-2 named default-ON blocker; Slice-20 codex §9 P2) | **CLOSED (2026-07-02)** — FTS/vec edge projection SQL now applies the `t_invalid > now` recency filter graph traversal already uses (3 sites: rebuild SELECT, vec queue arm, pending-work probe); RED→GREEN via the real `consolidate_with_provider` path asserting exclusion from both `search_index_edges` + `_fathomdb_vector_rows`. | n/a (code) | **PASS** (no findings); `0.8.12-sliceA-review-20260702T204034Z.md` | `2022c9f9` (RED test), `0c26703d` (fix) |
| **B** | Live TS X1 for `consolidate_with_provider` (R-X-1 TS-live) + governed-surface parity | **CLOSED (2026-07-02)** — committed `functional-consolidate.test.ts` builds the real napi binding, spawns a live `fathomdb.consolidate.v1` provider subprocess, asserts a verdict applied end-to-end; `npm test` 131/131 exit 0. fix-1 governed `consolidate_with_provider`/`consolidateWithProvider` in the shared allowlist → `surface.test.ts` + Python `test_surface.py` 15/15 green (isolated venv; shared `.venv` untouched). | X1 TS✅ | **PASS** (no findings); `0.8.12-sliceB-review-20260702T210411Z.md` | `a1f6f5a3` (test), `79fbad6c` (allowlist), `63d19c2d` (closure) |

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

1. **[SPEND AUTHORITY]** — **RESOLVED.** The user's own message (2026-07-02) approved the $20 EXP-COV-1
   sweep; it ran at **$4.79/$20**, 272/272, 0 failures. No further priced call pending in 0.8.12.
2. **[HARD-STOP after the sweep]** — **RESOLVED.** EXP-COV-1 GPU downstream verdict = **`CEILING-ABSORBED`**;
   HITL de-prioritized OPP-6 #6 (do NOT fund the ~$340 full relation-targeted extraction). R-COV-3
   resolved-negative; Slice 10 CLOSED (F-15).
3. **[Phase 5 — label-only merge]** — **OPEN, Steward/HITL-gated.** Phases 3-4 are complete on
   `0.8.12-memory-quality` (tip `63d19c2d`). The label-only merge to `main` (manifests stay `0.8.9`, no
   `v*` tag, no publish) is the Steward's Phase-5 step; this orchestration does NOT merge to `main`.

## Recent decisions (newest on top)

- 2026-07-02 — **Phases 3-4 COMPLETE (orchestrator, `$0`/local).** Two code slices landed on
  `0.8.12-memory-quality` via TDD + codex §9 PASS each, plus the release DoD: **Slice A** (`0c26703d`) made
  the consolidation `t_invalid` recency-exclusion rebuild-durable (FTS/vec edge projection SQL now mirrors
  graph traversal's `t_invalid > now` filter; RED→GREEN through the real consolidate path; R-CON-2 named
  default-ON blocker CLEARED). **Slice B** (`a1f6f5a3`+`79fbad6c`) added the live TS X1
  (`functional-consolidate.test.ts`, real napi + `fathomdb.consolidate.v1` subprocess + verdict applied;
  `npm test` 131/131) and closed a previously-masked governed-surface gap by governing the consolidate seam
  (both name forms) in the shared allowlist → `surface.test.ts` + Python `test_surface.py` 15/15 green.
  **Slice 10 CLOSED / R-COV-3 resolved-negative** (verdict `CEILING-ABSORBED`, F-15). Slice-40 DoD: X1
  Py-live+TS-live, X2 `mkdocs build --strict` exit 0, X3 DOC-INDEX ok, full R-COV/R-CON AC gate GREEN.
  Both slice worktrees cleaned (§11). Branch tip `63d19c2d`. **Phase 5 (label-only merge) is the
  Steward/HITL step — NOT done here.** All slice worktrees cut off the branch tip (the branch legitimately
  diverged from `main` at `20f53ffb` and must not be rebased; preflight's stale-base line vs `main` is a
  known false positive — the real base-contained invariant was verified per handoff §0, and the 16 main-only
  commits touch none of the changed code paths).
- 2026-07-02 — **ORCHESTRATION WIND-DOWN (HITL Option A: preserve + defer verdict to GPU re-run).**
  EXP-COV-1 priced extraction COMPLETE at **$4.79/$20** (0 failures, resilience proven). Downstream
  sufficiency read **environment-BLOCKED** (CPU-embedder defect → CE/dense unrunnable;
  `dev/notes/0.8.12-cpu-embedder-defect-blocks-dense-eval.md`) → verdict **DEFERRED to a GPU-embedder
  re-run** (reuses the preserved extraction; no re-anchoring on a degraded FTS+graph screening number).
  Division of labor: sweep implementer `a8f76783` (sole writer of `0.8.12-expcov1`, now under the user's
  direct direction) preserves the $4.79 asset + authors the GPU re-run plan; THIS session (sole writer of
  `0.8.12`) wrote the finding + `0.8.12-handoff.md` + this update, and **stands down**. Fresh HITL session
  picks up the GPU re-run + Slice-10 decision + label-only merge.
- 2026-07-02 — **Slice 40 driven as-far-as-it-can-go.** X2 mkdocs --strict GREEN; X3 DOC-INDEX resolves;
  X1 Python public API **live-verified** in an isolated venv (shared `.venv` untouched — mutex respected),
  TS surface present + napi compiles (live TS deferred, no `node_modules`). AC gate all ✅ except **R-COV-3
  GATED ON SLICE 10**. `0.8.12-slice40-verification.md`.
- 2026-07-02 — **Slice 20 CLOSED.** Value-test STAY-OFF (default-OFF). codex §9 CONCERN (rebuild-durability
  P2) → fix-1: reframed test scope + elevated the FTS/vec `t_invalid`-filter fix to a named
  blocker-before-default-ON; gate kept negative. Also corrected the `$` ledger note (spend-hold basis =
  the system-reminder relayed-consent guard, not the memex-push memory).
- 2026-07-02 — **Spend HOLD reaffirmed.** Coordinator re-asserted a user-approved $20 EXP-COV-1 sweep;
  per the standing system reminder a coordinator relay is not the user's own confirmation for irreversible
  spend → NOT executed. Non-spend work (Slice 20, Slice 40) proceeded. Awaiting the user's OWN message.
- 2026-07-01 — **Slice 15 CLOSED.** Consolidation/recency provider on the one OPP-8 transport (no second
  transport). codex §9 arc: CONCERN(4) → fix-1 (resolved 4, +1 new P2: phantom pending work) → fix-2
  (retain terminal on invalidate) → **PASS**. consolidate_provider 12/12; back-compat intact; bindings
  (Py public wrapper + TS/napi) wired. R-CON-1 + R-CON-3 met. X1 live-run deferred to Slice 40.
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

## Next action — Phase 5 (Steward/HITL): label-only merge

Phases 0-4 are COMPLETE on `0.8.12-memory-quality` (tip `63d19c2d`). All code slices landed with codex §9
PASS; the release DoD is GREEN (verified from git with real exit codes). Remaining:

1. **Phase 5 — label-only merge** `0.8.12-memory-quality` → `main` (**Steward/HITL-gated**; manifests stay
   `0.8.9`, NO `v*` tag, NO publish). Fold in the preserved sweep artifacts as appropriate; retire the
   `plan-0.8.12-finish.md` redirect stub. **This orchestration does NOT merge to `main`.**

DEFERRED / out-of-scope (carried forward, not 0.8.12 blockers):

- **Real-corpus at-power consolidation value test** (LOCOMO multi_session/temporal, priced) → the eventual
  consolidation default-ON decision (reason (1) of the STAY-OFF verdict; pairs with a future priced budget).
- **Productization of relation-focused extraction** — explicitly OUT of 0.8.12 (a separate later HITL call;
  the coverage-lift premise is de-prioritized per F-15).
