# FathomDB 0.8.18 — Plan (state-machine ladder) · **Production-safety & CI hardening capstone**

> **Plan-as-state-machine.** Mod-5 ladder + reserved-gap policy + "Immediate Next Slice". Authoritative
> contracts → `0.8.18-implementation.md`; live state → `runs/STATUS-0.8.18.md`; deps/decision record →
> `0.8.6-0.8.16-PROGRAM-SEQUENCING.md`. Run via `/goal complete 0.8.18` as an **orchestrator** session.
>
> **Theme.** The GA-hardening capstone of the non-measure line. Make portable DBs / runtime
> backend-swap *safe* with the **vector-equivalence self-check** (#5, now meaningful because 0.8.7 CUDA
> and 0.8.16 ONNX backends exist) and complete the **full publish pipeline** (#11-full) on top of
> 0.8.6's minimal path. This makes the 0.8.x line **beta-grade shippable** (release-engineering GA —
> publish machinery + frozen safety gates; **pre-1.0.0 is beta**, so this is not a stability/scale
> guarantee — those are staged 0.9.x → 1.1.0, F-17).
>
> **0.8.18 is NOT the end of the 0.8.x line (F-19/F-20).** OPP-12 (0.8.19/0.8.20) and the F-17 scale-bound
> (0.8.23/0.8.24) follow. Critically, **#11-full is the publish prerequisite for 0.8.20's OPP-12
> coordinated breaking-pair publish** — the atomic Memex `0.5.x-successor` pairing (F-19/F-21). 0.8.18
> hardens the publish machinery that 0.8.20 *uses*; it is on the Memex-value critical path, not a
> standalone GA nicety. (The Slice-40 tag here remains HITL-gated and label-vs-publish is a per-`x.y.z`
> HITL call.)
>
> **Reconciled 2026-07-02 (Steward):** #13 benchmark-and-robustness harness, originally co-scoped here,
> belongs at **0.8.19** per master §4 + F-10 ("#13 kept in 0.8.x → 0.8.19"); `plan-0.8.19.md` §1.2 owns
> it. Struck from this plan — **0.8.18 = #5 vector-equivalence + #11-full publish + the GA tag.**
> *(NB — F-19/F-20 later re-homed #13 to 0.8.21 and dep-migrations to 0.8.22; those ladders are drift-flagged
> in F-21, to re-home as their slots near. Does not change 0.8.18 scope.)*
>
> **Footprint.** #5 = IN-LIBRARY (open-time check, opt-in/enforced; CPU-only). #11 = CI/CD. Library
> query path stays CPU-only/1-bit/deterministic.

---

## 1. Goal & scope

- **#5 — Vector-equivalence self-check (B.2.b).** Store a small canonical probe set
  (`_fathomdb_embed_probe(probe_text, reference_vector)`, N≈16–32 diverse strings) at first vector-kind
  registration; on `Engine::open` (and on any backend/machine change) re-embed the probe set with the
  **live** embedder and assert each within tolerance **at the post-1-bit-quantization representation**
  (Hamming, calibrated against the binary-quant floor + the candle↔ONNX Δ measured in 0.8.16 Slice 15).
  Divergence ⇒ refuse to serve the dense/fused arm (loud typed error, never silent). Subsumes the
  `EmbedderIdentity` pre-filter (identity = *claims* same embedder; probe = *proves* equivalent vectors).
  **Calibration input — the 0.8.16 Δ is SAME-ARCH ONLY (unmeasured cross-backend gap, Steward 2026-07-08):**
  the R-ONNX-3 hand-off measured ONNX-CPU vs candle-CPU (**0 flips, same-arch**). The **cross-backend Δ
  (GPU-EP vs CPU, and candle-CUDA vs candle-CPU) is UNMEASURED** — and it is exactly the divergence #5's
  tolerance must bound. Slice 0 must **measure the cross-backend Δ** (GPU-eval mandate: 3090 cuda:0/1) as a
  real calibration input, not inherit the same-arch 0-flip number as if it covered backend swaps. Related
  carry-forwards: **TC-5** (eu7 grown-corpus floor re-baseline), **TC-9** (`ort` 2.0-stable bump).
- **#13 — benchmark-and-robustness harness — moved to 0.8.19 (struck from 0.8.18).** Per master §4 +
  F-10, the net-new benchmark substrate (`benches/`, `scale.rs`, `tracing` feature) + weekly workflow are
  owned by **0.8.19** (`plan-0.8.19.md` §1.2), not the GA capstone. Not 0.8.18 scope.
- **#11-full — Full publish pipeline.** On top of 0.8.6's minimal path: multi-OS napi prebuild matrix,
  the cross-ecosystem `all-builds-passed` gate, tiered `publish-rust`/`publish-pypi`/`publish-npm` with
  index-propagation, and a real (HITL-gated) tagged release of the 0.8.x line.

*Why last:* #5 is only meaningful once ≥2 backends can read each other's index (0.8.7 + 0.8.16), and is
deliberately deferred out of the high-re-embed experimentation phase (PROGRAM-SEQUENCING §5 Q2).
\#11-full is the GA publish that the whole line builds toward. (#13, heavy net-new authorship, is at
0.8.19 — see above.)

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal |
|----|-------------|-------------------|
| R-VEQ-1 | Probe set stored at first vector-kind registration | `_fathomdb_embed_probe` populated; migration test green |
| R-VEQ-2 | Open-time re-embed + tolerance assert at the retrieval representation | RED: a deliberately-divergent backend trips the check and refuses the dense arm; GREEN: same-backend float-noise does **not** trip it |
| R-VEQ-3 | Tolerance calibrated against the quant floor + the 0.8.16 candle↔ONNX Δ **+ a freshly-measured cross-backend Δ (GPU-EP vs CPU / candle-CUDA vs CPU — the 0.8.16 Δ is same-arch only, unmeasured across backends)** | Documented calibration; a true backend change (CUDA→CPU, candle→ONNX) trips, identical-backend does not |
| R-VEQ-4 | Loud typed error, never silent degradation | Error path is a typed `Engine::open`/serve error; no silent fallback |
| ~~R-BR-1 / R-BR-2~~ | ~~benchmark/robustness substrate + weekly workflow~~ — **MOVED to 0.8.19** (master §4 / F-10) | Owned by `plan-0.8.19.md` §1.2; not a 0.8.18 gate |
| R-REL-4 | Full publish pipeline + a real tagged release | Tiered publish dry-run green; HITL-gated tag fires the real 8-tier publish; versions consistent on both axes |
| R-GATE | eu7 ≥ 0.90 + AC-012/013/020 latency hold at GA | All frozen gates PASS on the release candidate |

New ACs: candidates at Slice 0 (vector-equivalence contract) and Slice 40 (GA release-readiness).

---

## 3. Slice ladder (mod-5)

```text
0 → 5 → 20 → 40      (10, 15 = void reserved gaps — #13 moved to 0.8.19)
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | Setup + ADR — vector-equivalence design (probe set, tolerance calibration vs quant floor + 0.8.16 Δ, refuse-to-serve semantics); full-publish design | design-adr | — |
| **5** | **Vector-equivalence KEYSTONE** — probe-set store + open-time re-embed + post-quant tolerance check + typed refuse-to-serve | implementation (schema + open-path) | 0 |
| **10** | *(void reserved gap)* — #13 benchmark substrate **MOVED to 0.8.19** (master §4 / F-10) | — | — |
| **15** | *(void reserved gap)* — #13 `benchmark-and-robustness.yml` **MOVED to 0.8.19** | — | — |
| **20** | **Full publish pipeline** — napi prebuild matrix + cross-ecosystem gate + tiered publish; dry-run | implementation (CI) | 0 |
| **40** | **GA Verification + Release** — X1/X2/X3 + R-VEQ/R-REL AC gate + all frozen gates (eu7/latency); HITL-gated real tagged release | verification + release | 5,20 |

**Keystones / hard gates.** **Slice 5 (vector-equivalence) is the keystone** — it is the prerequisite
to advertising portable DBs / runtime backend-swap (gate the claim on it). **R-VEQ-2 two-sided test is
hard:** must trip on a true backend change *and* not trip on same-backend float-noise. **Slice 40 real
tag is HITL-gated** (`release-publish-gotchas`: a `v*` tag fires the real publish).

**Tracks (parallelizable).** Equivalence track **5** ∥ publish track **20**, off Slice 0; both converge
at Slice 40. (The former benchmark track 10 → 15 is void — #13 moved to 0.8.19.)

---

## 4. Reserved-gap policy

Carried unchanged (`0.8.1-plan.md` §Numbering).

## 5. Cross-cutting DoD (X0/X1/X2/X3 — bind EVERY slice)

**X0 — Elevated process gate (HITL 2026-07-08; binds every unit that ships code).** BEFORE any
implementation, each unit requires, in order: **(A) written requirements with explicit, RED-testable
acceptance criteria**, then **(B) an independent design review** (mechanism: **codex**, adversarial-subagent
fallback) **→ HITL sign-off**. Only then does **RED/GREEN TDD → codex §9 code review** proceed. This is a
higher bar than 0.8.16 — the *design* is reviewed before code, not just the diff after. Applies to Slice-0
ADRs, Slice 5 (#5 probe), Slice 20 (#11-full publish), and the calibration harness.

X1 SDK parity + harnesses · X2 `mkdocs build` green (release gate) · X3 docs + DOC-INDEX per slice.
`runs/STATUS-0.8.18.md` carries the per-slice X column (incl. X0 design-review state).

### 5a. Slice-0 rulings (HITL 2026-07-08 — freeze into the Slice-0 DoD)

- **D1 (sharpened by the D4 path trace — the ranking is TWO-STAGE, verified from engine source).** The
  dense arm ranks on a **composition of two representations**, both load-bearing, so #5's probe must assert
  **both**: **(1) Phase-1 candidate recall = Hamming over `vec_quantize_binary(sign(x − mean_vec))`** — the
  mean-centered 1-bit `embedding_bin` codes (`lib.rs` `build_vector_phase1_sql` ~6525-6542; ingest/query
  centering symmetric, `lib.rs` ~4064 / ~4938); **(2) Phase-2 final ordering = `vec_distance_l2` over the
  UN-centered float `embedding`** — **L2, not cosine** (earlier "cosine" framing was wrong). A probe that
  checks only one stage passes while the other silently drifts. **Centering is identity-dependent** (only
  `fathomdb-bge-small-en-v1.5` = the default embedder; NoopEmbedder is a no-op) AND pin-dependent
  (`identity_requires_mean_centering` ∧ `mean_vec` pinned), and the gate is identical on ingest and query.
- **D2** on divergence, **refuse the dense/fused arm, keep FTS**, loud typed `EngineOpenError` (never silent).
- **D4** tolerance floor is a **design-review/HITL parameter** — NOT frozen yet. It must cover **both** the
  Phase-1 binary-code flip count AND a Phase-2 float/L2 tolerance (lean: 0 binary flips / 45-probe
  mean-centered + a calibrated L2 epsilon, not ruled).
- **D5 — platform scope.** Only **`x86_64-unknown-linux-gnu`** (this host arch) is on the 0.8.18 **critical
  path**. The repo's other declared support targets (Python `aarch64-linux-gnu` · `x86_64/aarch64-darwin` ·
  `x86_64-windows-msvc`; napi `darwin-x64/arm64` · win32/musl per `release.yml`) **remain supported but are
  OFF the critical path** — completed by the **deferred follow-on orchestrator**
  (`prompts/0.8.18-CROSS-PLATFORM-FOLLOWON-ORCHESTRATOR-PROMPT.md`) that starts AFTER x86_64-linux GA works.
  Slice 20 scopes publish-hardening + dry-run to x86_64-linux; the matrix rows are **deferred-to-follow-on**
  on the board (not dropped — a platform that truly can't be supported is a HITL call).
- **D6** #11-full hardening targets the **0.8.20 OPP-12 breaking-pair-publish contract** (F-19/F-21), not a
  standalone GA nicety.
- **D3 — ONNX-GPU-EP (L3) Δ** is measured **OOB by a Steward-commissioned subagent** (procure a CUDA
  `libonnxruntime.so`, measure, write durable results regardless of outcome → **TC-track**); **non-blocking**
  for the capstone. The candle-CUDA/CPU calibration harness is on the Slice-0 track (X0-gated).

## 6. Acceptance-criteria policy

`dev/acceptance.md` locked; track by B.2.b / G-gap + TDD names; new ACs only at gated slices (Slice 0,
Slice 40 GA), HITL-decided.

## 7. Prerequisites

1. **0.8.7 (OOB GPU) + 0.8.16 (ONNX) landed** — #5 is only meaningful with ≥2 real backends, and its
   tolerance is calibrated against the candle↔ONNX Δ measured in 0.8.16 Slice 15.
2. **0.8.6 minimal release path landed** — #11-full builds on it.
3. **0.8.9 (OOB) CI integrity landed** — the perf gates are honest before the GA verification leans on
   them.
4. Worktrees off `$(git rev-parse main)`; GPU/maturin/`ort` builds on the MAIN tree only; the real tag
   is HITL-gated.

## 8. Out-of-band / parallel notes

- **#13 benchmark harness is at 0.8.19, not here** (master §4 / F-10; `plan-0.8.19.md` §1.2). The
  vector-equivalence (#5) and full-publish (#11) work is the non-negotiable GA-safety core of 0.8.18.
- **GA makes NO scale claim; pre-1.0.0 is beta.** The supported-scale envelope is a staged ladder —
  **0.9.0 soft · 0.9.3 stated · 1.0.0 exit-beta · 1.1.0 hard** (a supportable promise, with 1.0.0→1.1.0
  break/fix room) (HITL 2026-07-02, **F-17**); the 2.x HNSW/ANN work (F-16) later raises the bound.
  0.8.18's "GA" is **release-engineering GA** (publish machinery + frozen eu7/latency gates), NOT a
  maturity/stability/scale guarantee — **do not add a scale-envelope label here.**
- Coordinate the GA tag with the experiment program — this release tags the **whole 0.8.x line**, so
  the M-work and router-design states should be at a coherent stopping point.

## 9. Immediate next slice

**Slice 0 — vector-equivalence + publish ADRs.** Calibrate the equivalence tolerance against the quant
floor and the 0.8.16 candle↔ONNX Δ; design the full publish pipeline; stand up `runs/STATUS-0.8.18.md`.
Then fan out Slices 5 ∥ 20. (#13 benchmark harness is at 0.8.19 — no benchmark slice here.)
