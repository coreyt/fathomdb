# FathomDB 0.8.16 — Plan (state-machine ladder) · **Production-safety & CI hardening capstone**

> **Plan-as-state-machine.** Mod-5 ladder + reserved-gap policy + "Immediate Next Slice". Authoritative
> contracts → `0.8.16-implementation.md`; live state → `runs/STATUS-0.8.16.md`; deps/decision record →
> `0.8.6-0.8.16-PROGRAM-SEQUENCING.md`. Run via `/goal complete 0.8.16` as an **orchestrator** session.
>
> **Theme.** The GA-hardening capstone of the non-measure line. Make portable DBs / runtime
> backend-swap *safe* with the **vector-equivalence self-check** (#5, now meaningful because 0.8.7 CUDA
> and 0.8.14 ONNX backends exist), restore the heavy **benchmark-and-robustness regression harness**
> (#13), and complete the **full publish pipeline** (#11-full) on top of 0.8.6's minimal path. After
> this release the whole 0.8.x line is GA-grade shippable.
>
> **Footprint.** #5 = IN-LIBRARY (open-time check, opt-in/enforced; CPU-only). #13/#11 = CI/CD. Library
> query path stays CPU-only/1-bit/deterministic.

---

## 1. Goal & scope

- **#5 — Vector-equivalence self-check (B.2.b).** Store a small canonical probe set
  (`_fathomdb_embed_probe(probe_text, reference_vector)`, N≈16–32 diverse strings) at first vector-kind
  registration; on `Engine::open` (and on any backend/machine change) re-embed the probe set with the
  **live** embedder and assert each within tolerance **at the post-1-bit-quantization representation**
  (Hamming, calibrated against the binary-quant floor + the candle↔ONNX Δ measured in 0.8.14 Slice 15).
  Divergence ⇒ refuse to serve the dense/fused arm (loud typed error, never silent). Subsumes the
  `EmbedderIdentity` pre-filter (identity = *claims* same embedder; probe = *proves* equivalent vectors).
- **#13 — `benchmark-and-robustness.yml` restoration.** Net-new authorship of the substrate the weekly
  workflow needs (criterion `benches/`, `scale.rs`, a `tracing` cargo feature, stress suites), then the
  workflow on a weekly cron. Build only the jobs whose substrate is justified; `log()` what is dropped.
- **#11-full — Full publish pipeline.** On top of 0.8.6's minimal path: multi-OS napi prebuild matrix,
  the cross-ecosystem `all-builds-passed` gate, tiered `publish-rust`/`publish-pypi`/`publish-npm` with
  index-propagation, and a real (HITL-gated) tagged release of the 0.8.x line.

*Why last:* #5 is only meaningful once ≥2 backends can read each other's index (0.8.7 + 0.8.14), and is
deliberately deferred out of the high-re-embed experimentation phase (PROGRAM-SEQUENCING §5 Q2). #13 is
heavy net-new authorship with low near-term ROI whose substrate partly accretes from earlier perf work.
#11-full is the GA publish that the whole line builds toward.

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal |
|----|-------------|-------------------|
| R-VEQ-1 | Probe set stored at first vector-kind registration | `_fathomdb_embed_probe` populated; migration test green |
| R-VEQ-2 | Open-time re-embed + tolerance assert at the retrieval representation | RED: a deliberately-divergent backend trips the check and refuses the dense arm; GREEN: same-backend float-noise does **not** trip it |
| R-VEQ-3 | Tolerance calibrated against the quant floor + the 0.8.14 candle↔ONNX Δ | Documented calibration; a true backend change (CUDA→CPU, candle→ONNX) trips, identical-backend does not |
| R-VEQ-4 | Loud typed error, never silent degradation | Error path is a typed `Engine::open`/serve error; no silent fallback |
| R-BR-1 | Benchmark/robustness substrate authored | `benches/` + `scale.rs` + `tracing` feature exist; jobs run green locally |
| R-BR-2 | Weekly workflow restored, dropped jobs logged | `benchmark-and-robustness.yml` runs on cron; any omitted pre-0.6.0 job is documented, not silently dropped |
| R-REL-4 | Full publish pipeline + a real tagged release | Tiered publish dry-run green; HITL-gated tag fires the real 8-tier publish; versions consistent on both axes |
| R-GATE | eu7 ≥ 0.90 + AC-012/013/020 latency hold at GA | All frozen gates PASS on the release candidate |

New ACs: candidates at Slice 0 (vector-equivalence contract) and Slice 40 (GA release-readiness).

---

## 3. Slice ladder (mod-5)

```
0 → 5 → 10 → 15 → 20 → 40
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | Setup + ADR — vector-equivalence design (probe set, tolerance calibration vs quant floor + 0.8.14 Δ, refuse-to-serve semantics); benchmark-substrate scope; full-publish design | design-adr | — |
| **5** | **Vector-equivalence KEYSTONE** — probe-set store + open-time re-embed + post-quant tolerance check + typed refuse-to-serve | implementation (schema + open-path) | 0 |
| **10** | **Benchmark/robustness substrate** — author `benches/` (criterion) + `scale.rs` + `tracing` feature + stress suites | implementation | 0 |
| **15** | **Restore `benchmark-and-robustness.yml`** — weekly cron over the Slice-10 substrate; document dropped jobs | implementation (CI) | 10 |
| **20** | **Full publish pipeline** — napi prebuild matrix + cross-ecosystem gate + tiered publish; dry-run | implementation (CI) | 0 |
| **40** | **GA Verification + Release** — X1/X2/X3 + R-VEQ/R-BR/R-REL AC gate + all frozen gates (eu7/latency); HITL-gated real tagged release | verification + release | 5,10,15,20 |

**Keystones / hard gates.** **Slice 5 (vector-equivalence) is the keystone** — it is the prerequisite
to advertising portable DBs / runtime backend-swap (gate the claim on it). **R-VEQ-2 two-sided test is
hard:** must trip on a true backend change *and* not trip on same-backend float-noise. **Slice 40 real
tag is HITL-gated** (`release-publish-gotchas`: a `v*` tag fires the real publish).

**Tracks (parallelizable).** Equivalence track **5** ∥ benchmark track **10 → 15** ∥ publish track
**20**, off Slice 0; all converge at Slice 40.

---

## 4. Reserved-gap policy

Carried unchanged (`0.8.1-plan.md` §Numbering).

## 5. Cross-cutting DoD (X1/X2/X3 — bind EVERY slice)

X1 SDK parity + harnesses · X2 `mkdocs build` green (release gate) · X3 docs + DOC-INDEX per slice.
`runs/STATUS-0.8.16.md` carries the per-slice X column.

## 6. Acceptance-criteria policy

`dev/acceptance.md` locked; track by B.2.b / G-gap + TDD names; new ACs only at gated slices (Slice 0,
Slice 40 GA), HITL-decided.

## 7. Prerequisites

1. **0.8.7 (OOB GPU) + 0.8.14 (ONNX) landed** — #5 is only meaningful with ≥2 real backends, and its
   tolerance is calibrated against the candle↔ONNX Δ measured in 0.8.14 Slice 15.
2. **0.8.6 minimal release path landed** — #11-full builds on it.
3. **0.8.9 (OOB) CI integrity landed** — the perf gates are honest before the GA verification leans on
   them.
4. Worktrees off `$(git rev-parse main)`; GPU/maturin/`ort` builds on the MAIN tree only; the real tag
   is HITL-gated.

## 8. Out-of-band / parallel notes

- **#13 benchmark harness** is the one heavy net-new CI item; if its ROI is still low at this point it
  may be roadmap-pushed past 0.8.x rather than built — decide at Slice 0 with the HITL. The
  vector-equivalence (#5) and full-publish (#11) work is the non-negotiable GA-safety core.
- Coordinate the GA tag with the experiment program — this release tags the **whole 0.8.x line**, so
  the M-work and router-design states should be at a coherent stopping point.

## 9. Immediate next slice

**Slice 0 — vector-equivalence + benchmark + publish ADRs.** Calibrate the equivalence tolerance against
the quant floor and the 0.8.14 candle↔ONNX Δ; scope the benchmark substrate (build vs roadmap-push);
design the full publish pipeline; stand up `runs/STATUS-0.8.16.md`. Then fan out Slices 5 ∥ 10 ∥ 20.
