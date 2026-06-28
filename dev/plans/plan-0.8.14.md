# FathomDB 0.8.14 — Plan (state-machine ladder) · **Ranking signal & embedder reach**

> **Plan-as-state-machine.** Mod-5 ladder + reserved-gap policy + "Immediate Next Slice". Authoritative
> contracts → `0.8.14-implementation.md`; live state → `runs/STATUS-0.8.14.md`; deps/decision record →
> `0.8.6-0.8.16-PROGRAM-SEQUENCING.md`. Run via `/goal complete 0.8.14` as an **orchestrator** session.
>
> **Theme.** Land the deferred ranking/lifecycle signal **F9 importance/confidence** (#15) — now
> *observable* through the 0.8.8 retrieval `EXPLAIN` surface — and the **cross-vendor ONNX embedder
> backend** (#4), which reaches AMD/Intel via a new `impl Embedder` behind the existing trait with zero
> engine surgery. #4 is placed here, adjacent to 0.8.16's vector-equivalence guard, because cross-vendor
> backends are exactly when silent numeric divergence becomes likely.
>
> **Footprint.** F9 = IN-LIBRARY (schema/ranking). ONNX = OFFLINE-BUILD/EVAL embedder backend behind
> `EmbedderChoice::Caller`; the in-library 1-bit query path is unchanged.

---

## 1. Goal & scope

- **#15 — F9 importance / confidence.** The repeatedly-deferred (G12-importance) ranking/lifecycle
  signal: a REAL importance column + the 3-way sentinel + edge confidence, with its Slice-35 ADR gate.
  Now worth landing because 0.8.8 EXP-OBS can **surface** the signal in the score-breakdown — an
  importance/confidence weight is only useful if a caller can see it acting.
- **#4 — Cross-vendor ONNX embedder (B.2.a).** candle reaches only CPU/CUDA/Metal — no ROCm/Vulkan — so
  AMD/Intel GPUs are unreachable through it. Add an `OrtBgeEmbedder` (`ort` crate: CUDA / ROCm /
  DirectML / OpenVINO / CPU) as a new `impl Embedder` plugged via `EmbedderChoice::Caller` — **zero
  engine changes**. All device/backend logic stays inside the embedder crate behind the trait.

*Why paired / why here:* F9's value is realized through 0.8.8's observability (hard-soft dep on EXP-OBS).
ONNX is structurally a drop-in but is scheduled here, not early-OOB, because (a) it is low-urgency
reach-hardware and (b) it manufactures the cross-backend numeric-divergence hazard that 0.8.16's #5
vector-equivalence guard exists to catch — so the two land back-to-back.

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal |
|----|-------------|-------------------|
| R-F9-1 | Importance column (REAL) + 3-way sentinel + edge confidence | Schema migration lands; write/read round-trips importance + confidence; sentinel semantics tested |
| R-F9-2 | Importance/confidence influences ranking and is **observable** | A weighted query reorders vs unweighted on a fixture; `explain=True` (0.8.8) shows the importance/confidence contribution |
| R-F9-3 | Slice-35 ADR gate honored | The deferred-F9 ADR decision is satisfied; no scope beyond it |
| R-ONNX-1 | `OrtBgeEmbedder` produces BGE-small vectors via the trait | A fixture text → vector within tolerance of the candle CPU reference (documented Δ); plugged via `EmbedderChoice::Caller`, zero engine diff |
| R-ONNX-2 | Backend selected at `Engine::open` via config/env, not compile-only | `FATHOMDB_EMBED_DEVICE`/config selects ONNX; default build unaffected |
| R-ONNX-3 | Numeric-divergence is documented, not yet enforced | The candle↔ONNX Δ is measured + recorded; the *same-backend build-and-read* discipline is documented as the interim guard until 0.8.16 #5 |
| R-X-1 | Py + TS SDK parity for F9 surface | X1 cross-binding harness green |
| R-GATE | eu7 ≥ 0.90 (one-sided CI) on any embedder/index change | `recall_gate.rs` PASS; breach BLOCKS→HITL |

New ACs: candidates at Slice 0 (F9 ranking contract) and the ONNX-equivalence measurement gate.

---

## 3. Slice ladder (mod-5)

```text
0 → 5 → 10 → 15 → 40
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | Setup + ADR — F9 ranking/lifecycle design (honor the Slice-35 ADR); ONNX-backend design (trait-local, `resolve_device()` extension, equivalence-measurement plan) | design-adr | — |
| **5** | **F9 importance/confidence KEYSTONE** — schema + ranking integration; surfaced via `explain` | implementation (schema) | 0 |
| **10** | **ONNX embedder backend** — `OrtBgeEmbedder` behind the trait; config/env selection; zero engine diff | implementation | 0 |
| **15** | **ONNX equivalence measurement** — candle↔ONNX numeric Δ on a probe set; document the interim same-backend discipline (feeds 0.8.16 #5 tolerance) | implementation (measurement) | 10 |
| **40** | **Verification + Release Readiness (0.8.14)** — X1/X2/X3 + R-F9/R-ONNX AC gate + eu7 gate | verification | 5,10,15 |

**Keystones / hard gates.** Slice 5 (F9) keystone. **eu7 ≥ 0.90 hard gate** on any embedder/index
touch. **R-ONNX-3 is a feed-forward gate:** the candle↔ONNX Δ measured at Slice 15 is the input the
0.8.16 vector-equivalence tolerance is calibrated against — record it precisely.

**Tracks (parallelizable).** F9 track **5** ∥ ONNX track **10 → 15**, off Slice 0.

---

## 4. Reserved-gap policy

Carried unchanged (`0.8.1-plan.md` §Numbering).

## 5. Cross-cutting DoD (X1/X2/X3 — bind EVERY slice)

X1 SDK parity + harnesses · X2 `mkdocs build` green · X3 docs + DOC-INDEX per slice. `runs/STATUS-0.8.14.md`
carries the per-slice X column.

## 6. Acceptance-criteria policy

`dev/acceptance.md` locked; track by F-id/G-gap + TDD names; new ACs only at gated slices, HITL-decided.

## 7. Prerequisites

1. **0.8.8 closed** — EXP-OBS exists so F9's signal is observable (R-F9-2).
2. **0.8.7 OOB GPU landed** — the ONNX `resolve_device()` extension builds on the device seam shipped
   there; GPU also accelerates the equivalence-probe re-embeds.
3. **0.8.12 closed** — F9's schema migration coordinates cleanly after the EXP-S substrate migration.
4. Worktrees off `$(git rev-parse main)`; embedder builds (incl. ONNX/`ort`) on the MAIN tree only.

## 8. Out-of-band / parallel notes

- **#4 ONNX is structurally OOB** (zero engine change) but deliberately *not* early-scheduled — see the
  PROGRAM-SEQUENCING §5 Q1 rationale. Its equivalence measurement (Slice 15) is the explicit hand-off to
  0.8.16 #5.
- F9 may interact with the M5 ranking work — coordinate the importance weighting with the M-work owner.

## 9. Immediate next slice

**Slice 0 — F9 + ONNX ADRs.** Honor the Slice-35 F9 ADR; design the trait-local ONNX backend and the
candle↔ONNX equivalence-measurement plan that feeds 0.8.16; stand up `runs/STATUS-0.8.14.md`. Then fan
out Slices 5 ∥ 10.
