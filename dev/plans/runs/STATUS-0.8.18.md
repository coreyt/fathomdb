# STATUS — 0.8.18 (Production-safety & CI hardening capstone: #5 vector-equivalence + #11-full publish + GA tag)

> Live state board (source of truth = git witnesses per orchestration.md §1.5; this is a cache).
> Plan: `dev/plans/plan-0.8.18.md` · Slice-0 design: `dev/design/0.8.18-slice-0-vector-equivalence-publish-design.md` ·
> ADRs: `dev/adr/ADR-0.8.18-vector-equivalence-self-check.md`, `dev/adr/ADR-0.8.18-full-publish-pipeline.md`.
> Deps/decision record: `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md` §4 (consumes I-4; F-19/F-20/F-21).
> Landing authority: **STANDING LANDING MANDATE GRANTED for 0.8.18** (HITL coreyt, 2026-07-08). On a clean
> codex §9 PASS with no exception trigger, the orchestrator lands (cherry-pick to `main` + push) and reports
> the sha; the Steward verifies from git. **ALWAYS still HITL-gated (mandate does NOT cover):** schema
> migration (`SCHEMA_VERSION` bump), any codex §9 override, any BLOCK, publish (the Slice-40 `v*` tag fires
> the real 8-tier publish; label-vs-publish is a per-`x.y.z` HITL call), any adoption-default change. Slice 0's
> design/ADR + new ACs + DoD freeze are HITL-gated (contract-setting, not a mechanical PASS). #5 tolerance-floor
> and #11-full publish-matrix decisions are HITL regardless.

## Current state — **Slice 0: SIGNED / CLOSED (HITL 2026-07-09)** — design review CLEAN after 4 codex rounds (Steward-verified); next = U3 canary measures → set D4 floor → fan Slices 5 ∥ 20 (D3 OOB in parallel)
- **NEW standing process gate (HITL 2026-07-08):** every 0.8.18 unit needs A. requirements+RED-testable ACs → B. independent design review (codex; adversarial-subagent fallback) → HITL sign-off, BEFORE any code; then RED/GREEN TDD → codex §9 diff review. Gate governs U1 #5-probe / U2 #11-full / U3 canary / U4 D3-OOB.
- **Design review — 4 codex rounds driven to terminal (all exit 0, verbatim outputs in scratchpad):** R1 U1/U2/U3 BLOCK + U4 CONCERN → R2 confirmed 7 fixes cleared, found deeper integration BLOCKs (SDK error-mapping, choke point, npm platform contract, provider seam) → R3 confirmed those, found 4 consistency/concreteness gaps → **R4: U1 PASS · U3 PASS · U4 PASS · U2 CONCERN**; the U2 CONCERN (R-REL-4f wording) FIXED (fix applied). **No BLOCK remains — clean terminal design-review verdict.** BLOCKs were never overridden; every fix was codex-confirmed.
- **Design contract now precise:** U1 = degraded-open (`dense_disabled` on `OpenReport` Rust/Py/TS + telemetry) + query-time `EngineError::VectorEquivalenceMismatch` at the single choke point `search_inner_with_stats` (covers search/searchExpand/explain-rerank/graph-arm) + explicit text-only/FTS path + two-stage assert (P1 mean-centered binary flips over `embedding_bin`, P2 un-centered L2) + UN-centered-f32-only refs + check-after-mean-recovery + **R-VEQ-5 additive-only**. U2 = reconcile+exercised-dry-run+OPP-12-resilience(not-atomicity)+GA-tag matrix gate (x86_64-linux only)+concrete napi split-package topology (non-`latest` dist-tag). U3 = subprocess-per-leg + Cls-pinned hard-assert + real pinned-mean fixture. U4 = pinned ORT supply-chain + stored `effective_provider()`; non-blocking (U1 re-probes the live backend).
- **HITL-reserved once design signs (3 items):** R-VEQ-5 additive-only (confirm), D4 floor = P1 flip count + P2 L2 ε (set after U3 measures), npm dist-tag label + publish sign-off. Plus always: Slice-0 docs commit, Slice-5 schema-bump (18→19) landing, the Slice-40 `v*` tag.
- **State:** nothing committed; canary/D3 executor NOT spawned; Slices 5 ∥ 20 NOT fanned.

## Slice scoreboard
| Slice | Title | State | Base SHA | Branch | codex | Cherry-pick/merge |
|------:|-------|-------|----------|--------|-------|-------------------|
| 0 | Setup + ADR — vector-equivalence design (probe set, tolerance calibration vs quant floor + 0.8.16 Δ + fresh cross-backend Δ, refuse-to-serve); full-publish design; board | **CLOSED (SIGNED 2026-07-09)** | (closure) | design review CLEAN (4 codex rounds) | n/a | HITL-signed; docs closure on main |
| 5 | **Vector-equivalence KEYSTONE** — probe-set store + open-time re-embed + post-quant tolerance check + typed refuse-to-serve | PENDING (blocked on 0) | — | — | — | — |
| 10 | *(void reserved gap)* — #13 benchmark substrate MOVED to 0.8.19 | VOID | — | — | — | — |
| 15 | *(void reserved gap)* — #13 `benchmark-and-robustness.yml` MOVED to 0.8.19 | VOID | — | — | — | — |
| 20 | **Full publish pipeline** — napi prebuild matrix + cross-ecosystem gate + tiered publish; dry-run | PENDING (blocked on 0) | — | — | — | — |
| 40 | **GA Verification + Release** — X1/X2/X3 + R-VEQ/R-REL AC gate + all frozen gates (eu7/latency); HITL-gated real tagged release | PENDING (blocked on 5,20) | — | — | — | — |

**Tracks (parallelizable off Slice 0):** equivalence track **5** ∥ publish track **20**; converge at **40**.
(Benchmark track 10 → 15 is VOID — #13 moved to 0.8.19.)

## Requirements / AC status (DoD frozen at Slice 0 — HITL-gated)
| ID | Requirement | State |
|----|-------------|-------|
| R-VEQ-1 | Probe set stored at first vector-kind registration (`_fathomdb_embed_probe`) | PENDING (Slice 5) |
| R-VEQ-2 | Open-time re-embed + tolerance assert at retrieval representation; two-sided (trips on true backend change, not on same-backend float-noise) | PENDING (Slice 5) |
| R-VEQ-3 | Tolerance calibrated vs quant floor + 0.8.16 candle↔ONNX Δ **+ freshly-measured cross-backend Δ** | **Slice 0 — measuring** |
| R-VEQ-4 | Loud typed error, never silent degradation | PENDING (Slice 5) |
| R-REL-4 | Full publish pipeline + a real tagged release (HITL-gated tag fires the real 8-tier publish) | PENDING (Slice 20 dry-run; Slice 40 tag) |
| R-GATE | eu7 ≥ 0.90 + AC-012/013/020 latency hold at GA | PENDING (Slice 40) |

New ACs: candidates minted at Slice 0 (vector-equivalence contract) and Slice 40 (GA release-readiness) — HITL-decided.

## Cross-cutting DoD (X1/X2/X3 — bind EVERY slice)
| Slice | X1 (SDK parity) | X2 (mkdocs) | X3 (docs+DOC-INDEX) |
|------:|-----------------|-------------|---------------------|
| 0 | n/a | n/a | board + 2 ADRs + design doc |
| 5 | pending | pending | pending |
| 20 | n/a (CI/CD) | pending | pending |
| 40 | pending | pending | pending |

## Prereqs (verified from git 2026-07-08)
- 0.8.7 GPU embedder (`8ec73464`) ✓ · 0.8.16 ONNX + Slice-15 equivalence (`c9e0ec74`, Slice-15 `70c2dad6`) ✓ ·
  0.8.6 minimal publish path (`21f1e804`) ✓ · 0.8.9 CI integrity (`8c513222`, `ab5058b9`) ✓.
- Base for all worktrees = live tip of `main` = `e4d1464e`. Preflight on main tree: `preflight: pass`.

## Notes / carry-forwards
- **TC-5** — eu7 grown-corpus (18,472-doc) 0.90-floor re-baseline (from 0.8.14). Relevant to R-GATE at Slice 40.
- **TC-9** — `ort` 2.0-stable bump (currently `ort =2.0.0-rc.10`). Relevant to the ONNX-GPU-EP measurement path.
- **#11-full is the publish PREREQUISITE for 0.8.20's OPP-12 coordinated breaking-pair publish** (F-19/F-21),
  not a standalone GA nicety — harden the publish machinery 0.8.20 uses.
- eu7 fidelity gate MUST run **CPU same-backend** (policy `649a8d45`); the cross-backend Δ measurement is a
  #5 calibration input, NOT the eu7 gate.
