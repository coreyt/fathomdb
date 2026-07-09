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

## Current state — **Slice 0 SIGNED · U3 CLOSED · Slice 20 LANDED · Slice 5 (#5 keystone) §9 PASS → AWAITING HITL LANDING · then Slice 40**
- **Slice 0 — CLOSED / HITL SIGNED (`beee25a4`, 2026-07-09).** Design review CLEAN after 4 codex rounds (BLOCKs resolved, never overridden). Frozen: design approved; **R-VEQ-5 = additive-only**; **D4 floor set after U3**; npm dist-tag deferred to Slice 20/40.
- **U3 canary — IN FLIGHT (calibration harness).** Implementer spawned on worktree `0.8.18-u3-calibration` (branch same), base `beee25a4`, preflight PASS. Builds the device-parameterized harness (subprocess-per-leg, Cls pinned both backends, hard-assert 0 flips/45 on the CPU baseline, real pinned-mean fixture for P1). **CPU legs validated in the worktree; candle-CUDA leg runs on the MAIN tree (`cuda:0`; D3 OOB agent has `cuda:1`; K620 excluded) by the orchestrator after §9-PASS/land.** ONNX-GPU-EP = D3's OOB track.
- **Sequencing (Steward-directed):** U3 measures → orchestrator runs candle-CUDA leg on main → **report both D4 components (P1 mean-centered flip count + P2 un-centered L2) to Steward → STOP; Steward sets the D4 floor** (backstopped by HITL-gated Slice-5 landing) → then fan **Slice 20 (#11-full, no floor needed)** and **Slice 5 (#5 probe, waits for D4 floor)** straight to RED/GREEN TDD → codex §9 (no second design review — Slice-0 signed the reqs+ACs+design).
- **Landing:** standing mandate lands clean §9-PASS non-migration slices (Steward verifies from git); **Slice-5 18→19 migration landing stays HITL-gated regardless**; Slice-40 `v*` tag HITL-gated. codex now via `dev/agent-tools/codex-nostdin.sh`.
- **Canary discipline:** U3 is the first real spawn this release — letting it finish its full cycle before any parallel launch (Slice 20 held until U3's machinery validates, unless Steward directs earlier).

### Live update (2026-07-09, orchestrator)
- **U3 §9: CONCERN → fix-1 → re-review PASS (codex, clean).** Both skip-classification CONCERNs cleared (inverted cuda assertion reframed; fallback legs now `skipped+fallback`, never a clean `ran`); accessors + CPU baseline + scope PASS. **Harness LANDED to main `91ccd794`** (cherry-pick of 3 commits `6b94b83c`/`0f06b0b8`/`91ccd794` onto `4e3cc674`; non-migration, standing mandate; D3 doc preserved). **candle-CUDA 45-probe leg now RUNNING on MAIN (`cuda:0`, `CUDA_VISIBLE_DEVICES=0`, nvcc `/usr/local/cuda-12.6`)** — auto-populates the durable doc; then one docs commit (board + doc) + push + worktree cleanup + report D4 to Steward.
- **main advanced to `4e3cc674`** (docs/ledger only — no embedder/engine changes; U3 branch NOT stale, lands clean).
- **D3 OOB ONNX-GPU-EP result LANDED** (`76568975`, `runs/0.8.18-slice-0-onnx-gpu-ep-calibration.md`): candle-CPU↔ONNX-CUDA-EP = **2/17280 raw flips** (1/17280 proxy-mean), cosine 0.99999996, deterministic ×3 — **first non-byte-identical cross-backend leg**. Effective provider VERIFIED CUDA (550 MiB, no fallback). **D4 bearing:** #5's actual guarded scenario is SAME-identity (candle CPU↔CUDA backend-swap); ONNX has a distinct `-onnx` identity + #5 is additive-only, so a 0-flip floor for the guarded scenario is unaffected — but a FUTURE cross-vendor portability ADR must budget ≥2/17280.
- **SHIPPED finding (→ TC-9, Steward to log):** `OrtBgeEmbedder` under `ort=2.0.0-rc.10` (default-features=false) CANNOT instantiate the CUDA EP as shipped — no `ort::init()` → "DefaultLogger not registered" → loud CPU fallback. ONNX GPU/cross-vendor EPs unreachable as shipped; fix = one-time ORT env init, re-verify at the ort 2.0-stable bump. **The D3 agent drafted a TC-D3 line for the Steward (not written).**
- **My candle-CUDA 45-probe refresh is now load-bearing:** D3 showed GPU FP divergence flips bits (ONNX-CUDA 2/17280); the SAME-identity candle-CPU↔candle-CUDA leg (0.8.7 said 0/6144 on the old 16-probe set) must be re-measured at 45 probes + real mean before the D4 floor is set.

### Slice 5 keystone — §9 PASS, HANDED TO HITL FOR LANDING (2026-07-09)
Head `600adbf8` on `0.8.18-slice-5-veq` (base `12f732a5`; branch not stale vs main `ff79a8ea` — Slice-20 landing was CI/packaging only, no engine/schema overlap; cherry-picks clean at landing). codex §9 drove to terminal PASS across fix-1/fix-2/fix-3 (BLOCKs never overridden). Transcripts in scratchpad (`codex-s9-slice5-out.txt`, `-fix-out.txt`, `-fix2-out.txt`).
**HITL decisions at the landing gate:**
- **(a) DEFECT #4 open-path deviation — codex-ACCEPTED, needs HITL awareness.** Baseline established at OPEN (identity-gated), NOT at write-path registration (literal R-VEQ-1 wording): 45 synchronous probe embeds in the write path violates the load-bearing "a write must never block on the embedder" async-projection invariant (it hung the pr9 watchdog test 915s). Residual (a same-identity backend that drifted *before* the baseline-establishing open isn't retroactively caught) = the already-accepted upgrade residual (additive-only per R-VEQ-5; U3 same-identity candle CPU↔CUDA = 0/17280). codex: "HITL/design residual, not a code defect."
- **(b) Final P2 L2 ε look** (currently the frozen `1e-5`; single named const `VECTOR_EQUIVALENCE_L2_EPSILON`, trivially tunable).
- **(c) X1 compiled-module parity to run on the MAIN tree at landing** (pyo3 + napi build — NOT built from the worktree): `src/python/tests/test_vector_equivalence_probe.py`, `test_surface.py::test_search_text_only_verb_is_live`, `src/ts/tests/vector-equivalence-probe.test.ts`, `surface.test.ts` searchTextOnly live guard. Divergence behavior is proven at the engine layer in Rust; X1 verifies the binding surfaces (leaf class/code, `OpenReport.dense_disabled`, `search_text_only`).
Shipped: SCHEMA 18→19 `_fathomdb_embed_probe` (UN-centered f32 refs only) + degraded-open (`dense_disabled` on OpenReport Rust/Py/TS + counter) + fail-SAFE probe (any unverifiable/partial/tampered baseline ⇒ refuse dense, open still succeeds, FTS serves) + query-time `EngineError::VectorEquivalenceMismatch` at the single choke point `search_inner_with_stats` (all vector-dependent arms) + two-stage P1(mean-centered flip=0)/P2(un-centered L2 ε=1e-5) + `search_text_only`/`searchTextOnly` verb. DoD green (clippy/check 0; engine+schema suites 0; probe binary 23 passed).

### D4 FLOOR FROZEN (Steward, HITL-delegated, 2026-07-09) — binds Slice 5 (#5) R-VEQ-3
- **P1 binary-code flip count floor = 0 (exact)** — every same-identity/same-vendor leg = 0/17280; a single flip ⇒ divergence ⇒ refuse (two-sided: same-backend noise stays 0; a real backend change trips).
- **P2 un-centered L2 ε = 1e-5** — benign same-identity CPU↔CUDA noise ≤ 1.4e-6; real cross-identity (ONNX-GPU-EP) L2 ≤ 5.4e-4 → 1e-5 is ~7× above benign / ~50× below real (clean separation). Soft knob (defensible 1e-5..1e-4); final HITL look at the HITL-gated Slice-5 landing.
- **U3 measured (MAIN `cuda:0`):** candle-CPU↔ONNX-CPU 0/17280 (reproduces `70c2dad6`); candle-CPU↔candle-CUDA **0/17280** (fresh 45-probe, was 0/6144 on the old 16-probe @0.8.7); candle-CUDA↔ONNX-CPU 0/17280; cosine 1.0 all; P2 L2 max ≤ 1.4e-6. Doc: `runs/0.8.18-slice-0-cross-backend-calibration.md`. codex §9 re-review transcript: `scratchpad/codex-s9-u3-fix-out.txt`.

## Slice scoreboard
| Slice | Title | State | Base SHA | Branch | codex | Cherry-pick/merge |
|------:|-------|-------|----------|--------|-------|-------------------|
| 0 | Setup + ADR — vector-equivalence design (probe set, tolerance calibration vs quant floor + 0.8.16 Δ + fresh cross-backend Δ, refuse-to-serve); full-publish design; board | **CLOSED (SIGNED 2026-07-09)** | (closure) | design review CLEAN (4 codex rounds) | n/a | HITL-signed; docs closure on main |
| 5 | **Vector-equivalence KEYSTONE** — `_fathomdb_embed_probe` (SCHEMA 18→19) + degraded-open + two-stage P1(=0)/P2(L2 ε=1e-5) check | **§9 PASS — AWAITING HITL LANDING (schema)** | 12f732a5 | 0.8.18-slice-5-veq (600adbf8) | **PASS** (fix-1/2/3; DEFECT#4-deviation ACCEPTED) | HITL-gated: NOT landed |
| 10 | *(void reserved gap)* — #13 benchmark substrate MOVED to 0.8.19 | VOID | — | — | — | — |
| 15 | *(void reserved gap)* — #13 `benchmark-and-robustness.yml` MOVED to 0.8.19 | VOID | — | — | — | — |
| 20 | **Full publish pipeline** — reconcile + exercised verify (real npm/PyPI round-trips) + OPP-12 resilience (idempotency/poll/rollback-forward) + GA-tag matrix gate (x86_64-linux) + napi split topology | **CLOSED / LANDED** | 12f732a5 | 0.8.18-slice-20-publish (fc8a6016) | **PASS** (fix-1 npm/pypi + fix-2 cargo safety) | cherry-picked → main `3bdfaea8` (mandate) |
| 40 | **GA Verification + Release** — X1/X2/X3 + R-VEQ/R-REL AC gate + all frozen gates (eu7/latency); HITL-gated real tagged release | PENDING (blocked on 5,20) | — | — | — | — |

**Tracks (parallelizable off Slice 0):** equivalence track **5** ∥ publish track **20**; converge at **40**.
(Benchmark track 10 → 15 is VOID — #13 moved to 0.8.19.)

## Requirements / AC status (DoD frozen at Slice 0 — HITL-gated)
| ID | Requirement | State |
|----|-------------|-------|
| R-VEQ-1 | Probe set stored at first vector-kind registration (`_fathomdb_embed_probe`) | PENDING (Slice 5) |
| R-VEQ-2 | Open-time re-embed + tolerance assert at retrieval representation; two-sided (trips on true backend change, not on same-backend float-noise) | PENDING (Slice 5) |
| R-VEQ-3 | Tolerance calibrated vs quant floor + 0.8.16 Δ + fresh cross-backend Δ | **FROZEN (Steward, HITL-delegated, from U3): P1 binary-flip floor = 0 (exact); P2 un-centered L2 ε = 1e-5** (final HITL look at Slice-5 landing). U3 legs: candle-CPU↔ONNX-CPU / candle-CPU↔candle-CUDA / candle-CUDA↔ONNX-CPU all **0/17280**, P2 L2 ≤ 1.4e-6; D3 ONNX-GPU-EP = 2/17280 (distinct identity, additive-only ⇒ no floor change) |
| R-VEQ-4 | Loud typed error, never silent degradation | PENDING (Slice 5) |
| R-REL-4 | Full publish pipeline + a real tagged release (HITL-gated tag fires the real 8-tier publish) | **Slice-20 machinery IMPLEMENTED** (R-REL-4a reconciliation + 4b exercised verification + 4c resilience + 4e matrix-gate + 4f npm split; 4d cross-platform matrix **deferred-to-follow-on**). The real `v*` tag remains Slice 40 (HITL-gated). |
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
