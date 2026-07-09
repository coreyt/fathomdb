---
title: FathomDB 0.8.21 — Plan (state-machine ladder)
subtitle: Free-threading + benchmark harness (OUT-OF-BAND, net-new)
date: 2026-07-09
status: PROPOSED
target_release: 0.8.21
---

# FathomDB 0.8.21 — Plan (state-machine ladder) · **Free-threading + benchmark harness (OUT-OF-BAND)**

> **↩ RE-HOMED from `plan-0.8.19` (master F-19/F-20, 2026-07-07/08).** OPP-12 took the 0.8.19/0.8.20
> slots (Phase-1 lifecycle+id @0.8.19 label-only · Phase-2 + breaking-pair publish @0.8.20), so the
> **free-threading (EXP-FT) + #13 benchmark-and-robustness** work that formerly held 0.8.19 is **bumped to
> 0.8.21**. Substance is unchanged from the original 0.8.19 plan — only the release slot moved. The
> `0.8.19` filename now holds **OPP-12 Phase-1** (`plan-0.8.19.md`). **NB — 0.8.21 is no longer the "end
> of the 0.8.x line":** F-20 pulls the F-17 scale-bound into 0.8.x (0.8.23 *soft* / 0.8.24 *stated*) and
> dep-migrations to 0.8.22, so 0.8.x continues past this release. See master §4 (0.8.21 row) + F-19/F-20/F-21.
>
> **Plan-as-state-machine.** Mod-5 slice ladder + reserved-gap policy + "Immediate Next Slice".
> Authoritative contracts → `0.8.21-implementation.md`; live state → `runs/STATUS-0.8.21.md`;
> deps/decision record → `0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (§3 OOB table, §6 F-5/F-10). Run via
> `/goal complete 0.8.21` as an **orchestrator** session
> (`prompts/0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md`).
>
> **OUT-OF-BAND (odd micro, net-new).** This release carries two independent,
> mostly-$0 tracks: (A) the **EXP-FT free-threaded-Python experiment ladder** (FT-1…5, $0 eval/analysis)
> and (B) the **#13 benchmark-and-robustness harness** (net-new CI/perf substrate + workflow restoration).
> Both are OOB against the even release line (no hard dep on 0.8.18/0.8.20 or later even work); they are
> sequenced here (after OPP-12 @0.8.19/0.8.20, F-19/F-20) because their respective prerequisites — pyo3
> 0.29 + the `gil_used` seam (EXP-FT) and the perf/bench substrate question (delayed intentionally through
> the high-velocity experimentation phase) — are fully resolved by this point. **This is NOT the end of the
> 0.8.x line** — dep-migrations (0.8.22) and the F-17 scale-bound (0.8.23 *soft* / 0.8.24 *stated*) follow.
>
> **Two independent tracks; one convergence.** EXP-FT (Slices 5→10→15) ∥ #13 benchmark (Slices 20→25)
> run in parallel after Slice 0 and converge at Slice 40 for the HITL readout and release verification.
> **FT-4 is the hard gate for EXP-FT productization** — the `gil_used = false` claim cannot be made
> until Slice 10 passes with zero races/deadlocks/aborts. **Productization is NOT in this release** — it
> is a post-0.8.21 contingency decided at the Slice 40 readout (SEQUENCING §6 F-5).
>
> **Footprint.** EXP-FT = EVAL-ONLY (local, $0; no priced runs, no network). #13 substrate = CI/CD +
> minimal IN-LIBRARY (new bench/test source, not in the shipped query path). #13 workflow = CI/CD.
> The library query path stays CPU-only / 1-bit / deterministic throughout.

---

## 1. Goal and scope

### 1.1 EXP-FT — free-threaded-Python experiment ladder (FT-1…5)

**Design authority:** `dev/design/free-threaded-python-value-lift-and-experiments.md` (the definitive
ladder, value analysis, and evaluation criteria; note its "0.8.15" scheduling reference is stale
relative to the F-10 renumber — 0.8.21 is the correct slot per SEQUENCING §4/F-10).

The pyo3 0.29 bump (0.8.8 Slice 1) shipped `#[pymodule(gil_used = true)]` as the correct conservative
default: FathomDB preserves today's GIL semantics while declaring compatibility with pyo3 0.28+'s
free-threaded-by-default assumption. The seam is at
`src/rust/crates/fathomdb-py/src/lib.rs` line 1552. EXP-FT now runs the five experiments to decide
whether — and when — to flip that seam to `gil_used = false`.

The five experiments (run in order; each informs the next):

- **EXP-FT-1 — GIL-held fraction profiling.** Instrument the `detach`/`attach` (formerly
  `allow_threads`/`with_gil`) boundaries in `fathomdb-py/src/lib.rs` to record per-op
  GIL-held vs GIL-released wall-time. Representative ops: `write` (small), `write` (large batch),
  `search`, `search`+rerank, `read_list`, `rerank` over a large passage list, `ingest_with_extractor`.
  Eval criterion: if max GIL-held % < 5% across hot ops, V2 (direct throughput) is negligible and
  the entire case rests on V1 (ecosystem non-poisoning). Output: `{op, total_us, gil_held_us, gil_held_pct}`.

- **EXP-FT-2 — Multi-thread throughput scaling.** Fixed harness, K threads (K ∈ {1,2,4,8,16}) sharing
  one Engine on (A) stock CPython 3.13 `gil_used = true` and (B) CPython 3.13t `gil_used = false`
  dev build. Measure aggregate QPS and p50/p99. Eval criterion: B/A multiplier at K=8. A multiplier
  ≈ 1.0 confirms FathomDB already scales (engine releases the GIL); a multiplier ≥ 1.3 at K≥4 flags a
  material marshalling-bound throughput gain from FT. Cross-check against FT-1 GIL-held fraction.

- **EXP-FT-3 — GIL-poisoning cost.** On 3.13t, measure how much _application-level_ parallelism a
  `gil_used = true` module destroys for its consumers. Synthetic agent-loop workload: P threads
  (P ∈ {2,4,8}) doing Python-side token-wrangling/JSON interleaved with FathomDB calls, comparing
  (a) `gil_used = true` (GIL re-enabled process-wide) vs (b) `gil_used = false` dev build. Eval
  criterion: `parallelism-lost ratio = 1 − tput(a)/tput(b)`. A ratio > 40% makes FathomDB an active
  ecosystem liability for free-threaded consumer agents (Memex / Hermes / OpenClaw). This is the V1
  (citizenship) number.

- **EXP-FT-4 — Concurrency safety under no-GIL (HARD GATE).** On 3.13t with `gil_used = false` dev
  build, run the full existing Py suite (67 tests green is mandatory) plus a new concurrency stress
  suite: M ≥ 1000 iterations of N-thread (N ∈ {2,4,8,16}) mixed `write`/`search`/`read`/`drain` on a
  shared Engine, plus a multi-Engine-instance variant (the historical deadlock shape). Run Rust under
  ThreadSanitizer; enable pyo3/CPython FT debug reference-count assertions. Assert byte-stability of
  results, write-cursor monotonicity, no panics, no deadlocks. **Eval criterion: zero races / zero
  deadlocks / zero aborts over M ≥ 1000 iterations is MANDATORY.** Any failure → BLOCK → root-cause
  → HITL; no `gil_used = false` claim may ship without this gate. See §3 "HITL gate at Slice 10" note.

- **EXP-FT-5 — Wheel-matrix feasibility.** Prototype the maturin / cibuildwheel matrix extension adding
  `cp313t` (and optionally `cp314t`) targets beside the existing `abi3-py310` wheel. Build locally;
  measure added CI wall-time and artifact count; confirm the abi3 wheel cannot serve FT interpreters
  (expected: no — FT ABI is version-specific). Eval criterion: packaging lift is **acceptable** if it
  fits the existing release CI budget (informed by #11-full publish matrix at 0.8.18) without a
  structural overhaul; otherwise FT productization is packaging-gated to a later slot.

**Scope boundary.** EXP-FT _decides_ the productization path; it does **not** implement it. Flipping
`gil_used = false` in shipped code, building the `*t` wheel matrix, and adding the FT CI lane are all
**post-0.8.21 work** — contingent on FT-4 PASS + HITL greenlight at the Slice 40 readout
(SEQUENCING §6 F-5). See §8.

### 1.2 #13 — `benchmark-and-robustness.yml` restoration

**Design authority:** `dev/plans/ci-deferred.md` § `benchmark-and-robustness.yml` (per-job substrate-gap
evidence, pre-0.6.0 shape, required adaptations). The 0.6.0-rewrite stripped all five pre-0.6.0
benchmark/stress jobs because their substrate did not exist. This release **authors the missing
substrate** then restores the workflow.

Three substrate gaps to fill (from ci-deferred.md per-job report):

1. **`rust-benchmarks`** — `fathomdb-engine` has no `benches/` directory, no `[[bench]]` Cargo entry,
   no criterion dep. Need: author `src/rust/crates/fathomdb-engine/benches/` with criterion benchmarks
   over representative engine ops (write-batch, search-fused, read-list, embed-probe-set). This is
   net-new authorship, not restoration of the pre-0.6.0 `production_paths` bench (that crate is now a
   re-export facade; the new bench targets the engine directly).

2. **`rust-scale-tests`** — `fathomdb-engine` has no `scale.rs` test target. Need: author
   `src/rust/crates/fathomdb-engine/tests/scale.rs` (large-corpus ingestion + search + cursor
   correctness at N=10k..100k docs).

3. **`rust-tracing-stress`** — `fathomdb-engine` has no `tracing` cargo feature (the dependency index
   records it as feature-gated / optional) and no `tracing_events` test. Need: add a `tracing` feature
   to `fathomdb-engine/Cargo.toml` (enabling structured event emission via the `tracing` crate already
   in the dependency index) + a stress test that drives concurrent operations and asserts no
   deadlock / panic through the tracing subscriber.

4. **`python-stress-tests`** — `src/python/tests/test_stress.py` is absent. Need: author a PyO3
   stress suite (N-thread shared Engine, mixed ops, surface-boundary assertions). The pre-0.6.0 stress
   suite targeted the prior Python binding; this is reimplemented on the PyO3 surface.

5. **`typescript-observability-harness`** — requires a `@fathomdb/sdk-harness` workspace, which
   requires the multi-workspace layout. This is a topology decision deferred to 0.9.x or whenever the
   TS workspace is created. **This job is dropped and documented.** Scope decision confirmed at Slice 0.

Adapted workflow shape (from ci-deferred.md § adaptations):

- Drop `go-fuzz-smoke` (no `go/` surface in the 0.6.0-rewrite).
- Repath `python/` → `src/python/`, `typescript/` → `src/ts/`.
- Use Phase 11 napi-rs build pattern (`cd src/ts && npm run build:native`).
- Use Phase 11 Python build pattern (`pip install -e src/python/` via maturin).
- TS observability harness: **logged as dropped** (workspace-topology prerequisite unmet).
- Weekly cron (`0 7 * * 1`).

**HITL decision on #13 scope (F-10, already resolved).** The 0.8.18 plan flagged that #13 might be
roadmap-pushed past 0.8.x if its ROI is low at Slice 0. SEQUENCING §6 F-10 records the HITL resolution
(2026-06-28): **#13 is KEPT in 0.8.x at 0.8.21.** This plan implements that decision. No re-decision
needed; the Slice 0 scope-call is on _which_ jobs to build (the TS harness question), not whether
to build #13 at all.

---

## 2. Requirements and acceptance criteria (release DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal |
| --- | --- | --- |
| R-FT-1 | GIL-held fraction measured per representative op | `{op, total_us, gil_held_us, gil_held_pct}` table recorded in `runs/STATUS-0.8.21.md`; max GIL-held % documented with interpretation (V2 negligible vs material) |
| R-FT-2 | Multi-thread throughput A vs B measured | B/A multiplier at K=8 documented; cross-checked against R-FT-1 |
| R-FT-3 | GIL-poisoning cost quantified on 3.13t | `parallelism-lost ratio` at P ∈ {2,4,8} documented; V1 interpretation (negligible vs citizenship-liability) recorded |
| R-FT-4 | Concurrency safety: zero races / zero deadlocks / zero aborts over M ≥ 1000 iterations (HARD GATE) | Full Py suite green on 3.13t + stress suite passes clean under TSan; any failure = BLOCK |
| R-FT-5 | Wheel-matrix feasibility assessed | CI minutes delta + artifact count documented; abi3-on-FT load result confirmed; packaging lift classified as acceptable or gating |
| R-FT-READOUT | All FT-1…5 data available at Slice 40 HITL readout | Recorded in `runs/0.8.21-EXP-FT-readout.md`; HITL decision (greenlit/not-greenlit) and rationale documented |
| R-BR-1 | Benchmark substrate authored and green | `benches/`, `scale.rs`, `tracing` feature, `test_stress.py` all exist, compile, and pass locally; `cargo bench --no-run` green; Python stress suite passes |
| R-BR-2 | Workflow restored on weekly cron | `.github/workflows/benchmark-and-robustness.yml` present; weekly cron configured; built jobs run green in CI on the first scheduled run |
| R-BR-3 | Dropped jobs logged, not silently omitted | `go-fuzz-smoke` and TS observability jobs are absent **with documented rationale** in the workflow file (comments) and in `dev/plans/ci-deferred.md` update |
| R-GATE | All existing frozen gates hold | eu7 recall ≥ 0.90 (at-gate, one-sided CI per `0.8.0-ga-blocked-recall-corpus` memory); latency gates (ac_012/013/019/020); AC-074 governed-surface allowlist (29 types) unchanged; X1 SDK parity |

New ACs: candidates at Slice 0 (concurrency-safety contract, bench-coverage contract) and Slice 40 (GA
readiness), HITL-decided. No invented AC ids; track by EXP-FT tag + TDD test names per the locked
acceptance policy.

---

## 3. Slice ladder (mod-5)

```text
0 → 5 → 10 → 15 → 20 → 25 → 40
```

Two independent tracks fan out from Slice 0:

| Slice | Title | Track | Work-type | Depends-on |
| ---: | --- | --- | --- | --- |
| **0** | Setup + design — confirm pyo3 0.29 seam state; design FT measurement harness (instrumentation points in `fathomdb-py/src/lib.rs`); design FT-4 stress suite (N/M parameters, TSan config); confirm #13 substrate scope (TS harness decision); stand up `runs/STATUS-0.8.21.md` | both | design-adr | — |
| **5** | **EXP-FT-1/2/3 measurements** — instrument `detach`/`attach` boundaries; run GIL-fraction profiling; build 3.13t dev toolchain; run multi-thread throughput A vs B; run poisoning cost on 3.13t; record all data | EXP-FT | implementation (measurement, $0) | 0 |
| **10** | **EXP-FT-4 concurrency safety (HARD GATE)** — author concurrency stress suite; run full Py suite on 3.13t; run stress suite under TSan + FT debug assertions; M ≥ 1000 iterations; **HITL gate if any failure** | EXP-FT | implementation (test harness + HITL gate) | 5 |
| **15** | **EXP-FT-5 wheel-matrix feasibility** — prototype `cp313t` target in maturin / cibuildwheel matrix; measure CI delta + artifact count; confirm abi3-on-FT result; classify packaging lift | EXP-FT | implementation (CI probe) | 10 |
| **20** | **#13 Benchmark substrate** — author `fathomdb-engine/benches/` (criterion); author `fathomdb-engine/tests/scale.rs`; add `tracing` cargo feature + `tracing_events` stress test; author `src/python/tests/test_stress.py` (PyO3 semantics); run all green | #13 | implementation | 0 |
| **25** | **Restore `benchmark-and-robustness.yml`** — adapt pre-0.6.0 workflow to 0.8.x layout; weekly cron; drop `go-fuzz-smoke` + TS harness with logged rationale; confirm CI jobs green on first scheduled run | #13 | implementation (CI) | 20 |
| **40** | **Verification + EXP-FT HITL readout + Release** — X1/X2/X3; R-FT/R-BR AC gate; all frozen gates; present `runs/0.8.21-EXP-FT-readout.md` to HITL; record greenlit/not-greenlit decision; if greenlit, propose post-0.8.21 publish slot | both | verification + HITL readout | 5,10,15,20,25 |

**Keystones / hard gates.**

**FT-4 (Slice 10) is the EXP-FT track keystone and a HARD GATE.** Zero races / zero deadlocks / zero
aborts over M ≥ 1000 iterations is mandatory before any `gil_used = false` claim may ship. This is not
a smoke test — it is the boundary between "believed FT-safe by construction" (§3 of the design doc,
grounded in the engine architecture) and "empirically confirmed FT-safe." If FT-4 fails: document
the failure, root-cause it, escalate to HITL. FT-5 (Slice 15) still runs (wheel feasibility is
independent), but the productization path is blocked until the root cause is resolved.

**HITL gate at Slice 10.** After FT-4 completes (pass or fail): (a) if PASS and FT-1/2/3 data show
the V1/V2 case is compelling → proceed to FT-5; (b) if FAIL → HALT FT productization path → HITL
decides whether to fix-and-rerun or record "productization blocked"; (c) if PASS but FT-1/2/3 show V1
and V2 both negligible → HITL decides whether FT-5 (packaging exercise) is worth the CI cost now or
should be recorded as "data supports keeping `gil_used = true`." In all cases FT-5 can be skipped only
by explicit HITL decision.

**Benchmark Slice 20 is the #13 track keystone.** It is net-new authorship (not restoration), and the
workflow at Slice 25 has no value without it. If any substrate item (benches/, scale.rs, tracing feature,
Py stress) fails to produce useful output, document the failure and adjust scope rather than shipping an
empty job.

**Tracks (parallelizable).** EXP-FT track **5 → 10 → 15** ∥ #13 benchmark track **20 → 25**, off
Slice 0; both converge at Slice 40. The two tracks share no data or code dependency and can be
orchestrated concurrently. **Within each track the slices are sequential** (FT-1/2/3 data drives FT-4
scope; FT-4 toolchain bootstraps FT-5; substrate must exist before the workflow is authored).

**Note on 3.13t toolchain bootstrap.** The 3.13t CPython dev build is first needed at Slice 5 (EXP-FT-2
comparison, EXP-FT-3 poisoning). It should be set up at Slice 5 and reused at Slice 10 (FT-4 heavy
runs). **The 3.13t build runs locally on the MAIN tree only, never in a worktree** (`agent-worktree-stale-
base-trap`: a worktree maturin-develop breaks the shared `.venv` binding; this constraint applies to the
`gil_used = false` dev build built here as well).

---

## 4. Reserved-gap policy

Carried unchanged from `dev/plans/plan-0.8.1.md` §Numbering. Reserved gaps occupy odd slice numbers
within the ladder (e.g. Slice 1, Slice 11, Slice 21). If an unexpected fix is needed mid-release, it
lands in the next available reserved-gap slot with its own PR and a documented rationale, not as scope
creep inside an existing slice. The ladder (`0 → 5 → 10 → 15 → 20 → 25 → 40`) leaves reserved-gap
slots at 1, 11, 21, 30, 35.

---

## 5. Cross-cutting DoD (X1/X2/X3 — bind every slice)

- **X1 — SDK parity.** EXP-FT produces no new public API surface (the `gil_used` seam is in the
  module declaration, not a callable). #13 benchmark substrate is Rust-and-Python-internal (no binding
  surface change). If either track exposes a new binding-visible behavior, it lands in both Py + TS
  bindings. The default assertion is that no library API changes in this release.
- **X2 — `mkdocs build` green.** Documentation build must remain green throughout. Any new docs (the
  EXP-FT readout, the benchmark substrate guide) must not break the build.
- **X3 — Docs + DOC-INDEX per slice.** Each slice's findings (experiment data, substrate authorship
  decisions, dropped-job rationale) are recorded in `runs/STATUS-0.8.21.md`. The EXP-FT full dataset
  goes in `runs/0.8.21-EXP-FT-readout.md` at Slice 40.

`runs/STATUS-0.8.21.md` carries the per-slice X column and is the live state spine.

---

## 6. Acceptance-criteria policy

`dev/acceptance.md` is **status: locked** (max AC-074, HITL-blessed). New AC ids are minted only at
gated slices (Slice 0 and Slice 40), HITL-decided. Do NOT invent AC ids for EXP-FT experiments or #13
substrate items. Track EXP-FT progress by experiment tag (FT-1…5) + TDD names for the stress suite.
Track #13 progress by R-BR-1/2/3 requirement ids above + TDD names for the new bench/stress targets.
If the Slice 40 readout produces a new concurrency-safety contract or a benchmark-coverage contract that
warrants formal acceptance, propose the AC(s) to HITL at that point.

---

## 7. Prerequisites

All prerequisites are **already satisfied** on `origin/main` at plan date (2026-06-28):

1. **pyo3 0.29.0 — DONE @0.8.8 Slice 1 (commit `8c938bb7`).** Confirmed on origin/main:
   `src/rust/crates/fathomdb-py/Cargo.toml` line 46:
   `pyo3 = { version = "0.29", features = ["abi3-py310"] }`.

2. **`gil_used = true` seam — DONE @0.8.8 Slice 1.** Confirmed on origin/main:
   `src/rust/crates/fathomdb-py/src/lib.rs` lines 1547–1552 — the `#[pymodule(gil_used = true)]`
   declaration with inline commentary pointing to the EXP-FT design doc. The seam is the exact flip
   point for EXP-FT productization (post-0.8.21, gated on FT-4).

3. **EXP-OBS landed — DONE @0.8.8 (commit `5c7b9f31`, field-set ratified).** The telemetry
   infrastructure (#10, 0.8.8 Slices 15/20) aids EXP-FT-1/2/3 timing instrumentation but is not a
   hard gate. EXP-FT runs correctly without it; real-gold telemetry is a convenience, not a blocker.

4. **#11-full publish matrix — DONE @0.8.18.** EXP-FT-5 (wheel feasibility) extends the matrix that
   0.8.18 established. The `cp313t` prototype builds on the 0.8.18 cibuildwheel configuration; it does
   not need to pre-empt it.

5. **CI-integrity foundation — DONE @0.8.9.** Honest gate map, bootstrap un-mask, and
   `rust-macos`/`rust-windows` green (PR #104, `1cb1c7ac`). The #13 benchmark workflow restores on top
   of a CI that is not lying.

6. **No 3.13t CPython toolchain yet on any CI runner.** This is the one new-infrastructure item: the
   free-threaded CPython build needs to be set up at Slice 5 (locally) and, if EXP-FT-4 passes and
   FT-5 is greenlit, would extend to CI at a post-0.8.21 slot. **This release does not add a 3.13t CI
   lane** — EXP-FT runs locally and reports results; the CI lane is part of the post-0.8.21
   productization work (FT-5 is a _feasibility probe_, not the actual CI-lane addition).

---

## 8. Dependencies and sequencing

### 8.1 EXP-FT track dependencies

| Dependency | Direction | Class | Detail |
| --- | --- | --- | --- |
| pyo3 0.29 + `gil_used` seam | 0.8.8 → 0.8.21 | physically hard (to start) | Cannot instrument or test FT behavior against an older pyo3 API. **Already satisfied.** |
| FT-4 zero-races gate | FT-4 → `gil_used = false` | physically hard (to flip) | The no-GIL module declaration cannot ship without this gate. FT-4 is the only thing blocking productization code-safety. |
| #11-full publish matrix | 0.8.18 → post-0.8.21 wheel lane | rework-forcing (packaging) | FT wheels extend the 0.8.18 matrix; building them before 0.8.18 means building the matrix twice. **Already satisfied.** |
| EXP-FT telemetry substrate | 0.8.8 #10 → FT-1/2 timing | soft (eval convenience) | Aids instrumentation precision; EXP-FT runs without it. |

**EXP-FT does NOT depend on 0.8.17, 0.8.18, or any dispatcher/router work.** The ladder is purely a
binding-layer analysis and is fully decoupled from the even engine line.

### 8.2 #13 benchmark track dependencies

| Dependency | Direction | Class | Detail |
| --- | --- | --- | --- |
| perf/bench substrate authorship | Slice 20 → Slice 25 | physically hard (within-release) | The workflow restores only what exists; Slice 25 has no content without Slice 20. |
| CI-integrity foundation | 0.8.9 → 0.8.21 | rework-forcing | Restoring a benchmark workflow onto a CI that is masking its own failures would reproduce the lying-gate problem. **Already satisfied.** |
| Pre-0.6.0 workflow source | `git show 39ee271^:.github/workflows/benchmark-and-robustness.yml` | reference (read-only) | The adaptations in §1.2 are grounded in this source. |

**#13 does NOT depend on EXP-FT.** The two tracks share no code or data dependencies and can be
implemented concurrently by independent implementers.

### 8.3 EXP-FT productization — the post-0.8.21 contingency

EXP-FT productization is **not a deliverable of this release.** It is decided at the Slice 40 HITL
readout based on the FT-1…5 data. The decision tree (from the design doc §7, abbreviated):

```text
if V1 (FT-3 parallelism-lost ratio) large OR any consumer ships on 3.1Xt:
    productization = citizenship requirement → proceed if FT-4 PASS + FT-5 ACCEPTABLE
if V2 (FT-2 multiplier) large:
    productization = also directly valuable → same gate
if both small:
    keep gil_used = true indefinitely; revisit when a V3 feature lands
in all cases: this release delivers DATA and a DECISION, never a shipped gil_used = false
```

If HITL greenlights at the Slice 40 readout: a **net-new post-0.8.21 publish slot** (the steward elects
the number — outside the 0.8.x odd line) carries the actual flip, the `*t` wheel matrix additions, and
the FT CI lane. That slot depends on FT-4 PASS (hard) and FT-5 ACCEPTABLE (packaging call); it is NOT
pre-scheduled in the master sequencing doc and must be proposed by the steward at the readout.
Per SEQUENCING §6 F-5 (2026-06-28 HITL record): this is the binding resolution.

### 8.4 Position in the 0.8.x sequence

This release sits **after OPP-12 (0.8.19 Phase-1 / 0.8.20 Phase-2) and ahead of the dep-migrations
(0.8.22) + the F-17 scale-bound (0.8.23/0.8.24)** — it is NOT the end of the 0.8.x line (F-19/F-20).
After 0.8.21 closes:

- The 17 non-measure items from SEQUENCING §1a are fully placed (all 17 have landed or are in their
  respective releases through 0.8.18).
- OPP-12 (Phase-1 @0.8.19 label-only · Phase-2 @0.8.20 publish) is DONE.
- The planner-router track is at its ⏸ PARKED 0.8.15/0.8.17 state (dispatcher/hardening deferred, F-18/F-19).
- EXP-FT data is available for the HITL productization decision.
- #13 benchmark harness is live on a weekly cron.
- Any post-0.8.21 EXP-FT productization slot is an addendum to the 0.8.x line, not counted in the
  17-item backbone.

---

## 9. Immediate next slice

**Slice 0 — EXP-FT design + #13 substrate scope.**

Concrete deliverables:

1. **Confirm the pyo3 seam state on origin/main** — `git show origin/main:src/rust/crates/fathomdb-py/src/lib.rs` line 1552; verify `gil_used = true` is present and the comment block is accurate.

2. **EXP-FT instrumentation design** — identify the exact `detach`/`attach` call sites in
   `fathomdb-py/src/lib.rs` that bound every op's GIL-held region; specify the microsecond-timestamp
   wrapper approach (either a thin inline shim or a feature-flagged timing layer); confirm no shipping
   path is affected.

3. **FT-4 stress suite specification** — finalize N ∈ {2,4,8,16}, M = 1000 (minimum), the thread-load
   mix (`write`/`search`/`read`/`drain` ratio), the multi-Engine-instance variant, and the TSan invocation
   (`RUSTFLAGS="-Z sanitizer=thread" cargo test --target x86_64-unknown-linux-gnu` or equivalent).

4. **3.13t toolchain plan** — confirm CPython 3.13t availability on the dev box; document the build or
   install steps; confirm `maturin develop --features pyo3/extension-module` builds successfully under
   3.13t with a temporary `gil_used = false` dev patch (not committed).

5. **#13 substrate scope decision** — confirm: (a) `benches/`, `scale.rs`, `tracing` feature,
   `test_stress.py` are ALL in scope (criteria: authorship is bounded; each job has a non-trivial
   verified correctness signal); (b) TS observability harness is **dropped with rationale** (workspace
   topology prerequisite unmet; document in `dev/plans/ci-deferred.md` update).

6. **Stand up `runs/STATUS-0.8.21.md`** — state-spine for both tracks; X1/X2/X3 columns; track A (EXP-FT
   Slices 5/10/15) and track B (#13 Slices 20/25) rows; Slice 40 convergence.

Then **fan out Slices 5 ∥ 20** (EXP-FT measurements in parallel with benchmark substrate authorship).
The two tracks have no shared state and can be handed to independent implementer subagents.

---

_Authoritative deps/decision record: `dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md` §4 (0.8.21 row),
§6 F-5 (EXP-FT scheduling + productization contingency), §6 F-10 (release renumber + #13 kept in 0.8.x)._
_EXP-FT design authority: `dev/design/free-threaded-python-value-lift-and-experiments.md`._
_#13 substrate-gap evidence: `dev/plans/ci-deferred.md` § benchmark-and-robustness.yml._
_Seam location: `src/rust/crates/fathomdb-py/src/lib.rs` line 1552 (verified `origin/main`)._
