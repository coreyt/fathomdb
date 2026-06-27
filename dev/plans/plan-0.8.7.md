# FathomDB 0.8.7 — Plan (state-machine ladder) · **GPU embedder (OUT-OF-BAND)**

> **Plan-as-state-machine.** Mod-5 slice ladder + reserved-gap policy + "Immediate Next Slice".
> Authoritative contracts → `0.8.7-implementation.md`; live state → `runs/STATUS-0.8.7.md`;
> deps/decision record → `0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (§3 OOB, §5 Q1). Run via
> `/goal complete 0.8.7` as an **orchestrator** session (`prompts/0.8.x-RELEASE-ORCHESTRATOR-HANDOFF.md`).
>
> **OUT-OF-BAND (odd micro).** This release runs **in parallel** to the even main line and gates nothing
> and is gated by nothing. It is the textbook drop-in: the default build stays CPU, GPU is opt-in via a
> cargo feature + an env var, so **every existing test and byte-stability gate is unchanged**. Schedule
> it **first** — single-stream GPU is ≈100–300× CPU (a 27 h embed → minutes), so it accelerates every
> re-embed-heavy slice and experiment after it.
>
> **Footprint.** OFFLINE-BUILD / EVAL-ONLY. GPU is a build/eval accelerator; the shipped in-library
> query path stays CPU-only / 1-bit (Hamming) / deterministic. The GPU embedder never enters the library
> query path's footprint contract.

---

## 1. Goal & scope

Implement **#3 — Embedder GPU acceleration** per `dev/design/0.8.1-embedder-gpu-and-portability.md` §1
(HITL-approved 2026-06-15):

- Replace the hardcoded `Device::Cpu` (`fathomdb-embedder/src/candle_bge.rs:168`) with a
  **`resolve_device()`** seam driven by env `FATHOMDB_EMBED_DEVICE` = `cpu` (default) | `cuda` | `metal`,
  with `cuda[:N]` selecting GPU N.
- Add cargo features mirroring the existing `default-embedder` chain through embedder → engine → py:
  - embedder: `embed-cuda = ["default-embedder", "candle-core/cuda", "candle-nn/cuda",
    "candle-transformers/cuda"]` (and `embed-metal` analogously);
  - engine: `embed-cuda = ["default-embedder", "fathomdb-embedder/embed-cuda"]`; py likewise.
- **Default build stays CPU** → all existing tests + byte-stability gates unchanged; GPU is opt-in via
  feature + env. The model loads at `Engine::open`, so the 30 s per-embed watchdog never sees cold-start.
- The PR-9 serialization guard (one concurrent `embed()`) is *correct* on GPU — it prevents concurrent
  CUDA calls on the shared model.

**Out of scope (deferred):** cross-vendor ONNX (#4 → 0.8.14); the vector-equivalence guard
(#5 → 0.8.16). This release ships the GPU device seam with the **interim same-backend discipline**
documented (build-and-read on the same backend until #5 lands).

---

## 2. Requirements + acceptance criteria (release DoD — frozen at Slice 0)

| ID | Requirement | Acceptance signal |
|----|-------------|-------------------|
| R-GPU-1 | `resolve_device()` selects CPU/CUDA/Metal from env; CPU default | Unit test over `FATHOMDB_EMBED_DEVICE` values incl. `cuda:N`; unset → CPU |
| R-GPU-2 | Default build is byte-identical to today | The existing byte-stability + recall gates pass **unchanged** on the default (CPU) build |
| R-GPU-3 | `embed-cuda` feature builds + runs on the GPU box | `maturin develop --features pyo3/extension-module,embed-cuda` on the **MAIN tree**; an embed runs on `cuda:0` |
| R-GPU-4 | Measured speedup recorded | A re-embed of the frozen corpus on CUDA vs CPU; wall-clock Δ logged (expectation: hours → minutes) |
| R-GPU-5 | Serialization holds on GPU | The PR-9 single-`embed()` guard is exercised under the CUDA path (no concurrent CUDA calls) |
| R-GPU-6 | Interim safety documented | The same-backend build-and-read discipline + the `EmbedderIdentity` pre-filter are documented as the guard until #5 (0.8.16) |

New ACs: candidate at Slice 0 (device-seam contract) if HITL elects to mint one; otherwise tracked by
TDD test names per the locked-acceptance policy.

---

## 3. Slice ladder (mod-5)

```
0 → 5 → 10 → 40
```

| Slice | Title | Work-type | Depends-on |
|------:|-------|-----------|-----------|
| **0** | Setup + ADR confirm — board; confirm the §1 device-seam design is still current; pin the interim same-backend discipline | design-adr | — |
| **5** | **Device seam KEYSTONE** — `resolve_device()` + env parsing + the `embed-cuda`/`embed-metal` feature chain (embedder→engine→py); default-CPU byte-identical | implementation | 0 |
| **10** | **GPU validation + speedup measure** — `embed-cuda` build on the GPU box; frozen-corpus re-embed CUDA vs CPU; serialization-under-GPU check; record the wall-clock Δ | implementation (measurement) | 5 |
| **40** | **Verification + Release Readiness (0.8.7)** — X1/X2/X3 + R-GPU AC gate; confirm default build gates unchanged | verification | 5,10 |

**Keystones / hard gates.** Slice 5 is the keystone. **R-GPU-2 (default build byte-identical) is a hard
gate** — the GPU work must not perturb the CPU default; any drift in the byte-stability/recall gates
BLOCKS. **The `embed-cuda` build runs on the MAIN tree only**, never a worktree
(`agent-worktree-stale-base-trap`: a worktree maturin-develop breaks the shared `.venv` binding).

**Tracks.** Single track (5 → 10); small release. No parallel fan-out needed.

---

## 4. Reserved-gap policy

Carried unchanged (`0.8.1-plan.md` §Numbering): mod-5 planned slices; gaps are fully-orchestrated
insertion slices off a fresh `main` baseline; HALT to HITL on band overflow.

## 5. Cross-cutting DoD (X1/X2/X3 — bind EVERY slice)

- **X1 — SDK parity.** The feature flag + env surface is documented in **both** Py and TS SDK refs;
  there is no new runtime API surface (env/feature only), so the harness assertion is that the default
  (CPU) path is unchanged across both bindings.
- **X2 — `mkdocs build` stays green.** The embedder/GPU docs add updates `mkdocs.yml` nav same-slice.
- **X3 — docs + `dev/DOC-INDEX.md` maintained** in the closing docs commit.

`runs/STATUS-0.8.7.md` carries the per-slice X column.

## 6. Acceptance-criteria policy

`dev/acceptance.md` locked (`acceptance-md-locked-no-feature-acs`); track by TDD test names; new ACs only
at gated slices, HITL-decided.

## 7. Prerequisites

1. **GPU box reachable** — 2× RTX 3090 (compute-idle), CUDA 12.6 toolkit present (per the design doc).
2. **MAIN-tree build** — the `embed-cuda` maturin build happens on the MAIN tree; **never** a worktree,
   and **never** `pip install -e` / `maturin develop` from a worktree.
3. No upstream release dependency — this is OOB and can open immediately, in parallel with 0.8.6.

## 8. Out-of-band / parallel notes

- **Runs in parallel to 0.8.6** and shares no files with the provider/CI foundation work.
- **⚠ Shared-build contention with 0.8.6 — worktrees do NOT isolate it (by design).** Both releases are
  source-isolated in their own worktrees, but the `embed-cuda` `maturin develop` here and 0.8.6's
  provider-seam rebuild **both land on the single shared MAIN tree + `.venv`** (R-GPU-3 / §7.2 — never a
  worktree). **Serialize MAIN-tree builds (one `maturin develop` at a time), and remember the `.venv`
  carries one feature-set at a time:** after an `embed-cuda` build the shared env holds the GPU-feature
  `.so`, so 0.8.6's parity harness then runs against *that* build (CPU output is byte-identical per
  R-GPU-2 — but be explicit about which is installed). **GPU is uncontended** (2× idle 3090; 0.8.6 uses
  no GPU). Build-hygiene, not a dependency.
- **⚠ Coordinate the deferred MAIN-tree maturin smoke with 0.8.8's pyo3 bump.** The single deferred
  `maturin develop --features embed-cuda` + py `cuda:0` confirmation (R-GPU-3) will, once 0.8.8 Slice 1
  lands, build the `fathomdb-py` binding at **pyo3 0.29**, not 0.24. Run it **with or after** the 0.8.8
  pyo3 bump (not before), so it validates the binding the program is keeping, and serialize it against the
  0.8.8 Slice-1 build — both touch the shared MAIN-tree `.venv` (the build-contention bullet above).
- **Recommended-before** the re-embed-heavy releases (0.8.10 coverage, 0.8.12 EXP-S/F5, 0.8.14 ONNX
  probe) so their index rebuilds are minutes, not hours — soft acceleration, not a hard gate.
- **Feeds 0.8.14 / 0.8.16:** the `resolve_device()` seam is the pattern the 0.8.14 ONNX backend extends,
  and the CUDA↔CPU numeric Δ this release exercises is part of what 0.8.16 #5 calibrates against.

## 9. Immediate next slice

**Slice 0 — confirm the §1 design + pin the interim discipline.** Stand up `runs/STATUS-0.8.7.md`,
re-confirm the device-seam design against the current `candle_bge.rs`, and document the same-backend
build-and-read discipline. Then Slice 5 (device seam) → Slice 10 (GPU validation on the MAIN tree).
