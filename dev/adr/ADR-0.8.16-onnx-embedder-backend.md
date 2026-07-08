---
title: ADR-0.8.16-onnx-embedder-backend
date: 2026-07-07
target_release: 0.8.16
desc: Adds a cross-vendor ONNX-Runtime BGE-small embedder (OrtBgeEmbedder) as a new impl Embedder in fathomdb-embedder, injected via EmbedderChoice::Caller with zero engine changes. Reaches AMD ROCm / Intel OpenVINO / DirectML that candle cannot. Behind a new onnx-embedder feature; the thin default build and the in-library 1-bit query path are unchanged. Includes the candle<->ONNX equivalence-measurement plan that feeds the 0.8.18 #5 vector-equivalence tolerance.
status: SIGNED (HITL coreyt, 2026-07-08 — Slice-0 gate)
---

# ADR-0.8.16 — Cross-vendor ONNX embedder backend (OrtBgeEmbedder)

**Status: SIGNED** (HITL coreyt, 2026-07-08 — Slice-0 gate). ONNX-equivalence measurement AC minted; Slices
10→15 may proceed. Design context: `dev/design/0.8.16-slice-0-f9-onnx-design.md` (§5 equivalence plan, §6
ONNX decision).

## 1. Context

candle reaches only CPU / CUDA / Metal — **no ROCm / Vulkan / DirectML / OpenVINO** — so AMD and Intel
GPUs are unreachable through it. `fathomdb-embedder/Cargo.toml:64-68` already anticipates the fix: the
cross-vendor path is "a separate `impl Embedder` (ONNX-Runtime) via `EmbedderChoice::Caller`." This is
structurally out-of-band (zero engine change) but is scheduled here — not early-OOB — because it is (a)
low-urgency reach-hardware and (b) it manufactures the cross-backend numeric-divergence hazard that
0.8.18's #5 vector-equivalence guard exists to catch, so the two land back-to-back.

## 2. Decision

- **`OrtBgeEmbedder` in `fathomdb-embedder`** (`mod ort_bge`, sibling of `candle_bge` / `nomic`), `impl
  Embedder` from `fathomdb-embedder-api` (`embed`/`embed_batch`/`identity`). Produces BGE-small vectors
  (BAAI/bge-small-en-v1.5, dim 384, CLS-corrected pooling to match the candle reference).
- **Injection: `EmbedderChoice::Caller(Arc::new(OrtBgeEmbedder::…))`** via `Engine::open_with_choice`
  (`fathomdb-engine/src/lib.rs:2449`). **Zero engine diff** — the engine never names a concrete embedder on
  the `Caller` path. The `Default` variant stays candle-only (`open_default_embedder` unchanged), preserving
  the footprint invariant.
- **Feature gating:** a new `onnx-embedder` Cargo feature pulling optional `ort` (+ tokenizer/loader deps),
  mirroring `default-embedder`'s `dep:` gating so the thin `default = []` build stays ML-free (EMB-3
  wheel-size gate). No new dep in the default build.
- **Device selection (R-ONNX-2):** reuse the backend-agnostic `parse_device_request`
  (`fathomdb-embedder/src/device.rs`) for `FATHOMDB_EMBED_DEVICE` grammar parity; add an ONNX-specific
  `resolve_device()` sibling that maps `DeviceRequest → ORT execution provider` (CUDA / ROCm / DirectML /
  OpenVINO / CPU), with a LOUD stderr fallback to CPU when a provider is unavailable (as candle's does).
  Selection happens at `Engine::open` via config/env — not compile-only.

## 3. Equivalence measurement & interim guard (R-ONNX-1, R-ONNX-3 → 0.8.18 #5)

- **Measured (Slice 15):** candle-CPU reference vs `OrtBgeEmbedder` across a fixed deterministic probe set;
  metrics = cosine / L2 / max-abs element Δ **and the load-bearing 1-bit sign-quantization FLIP RATE**
  (the in-library query path is 1-bit Hamming, so the sign-flip rate is what actually threatens fidelity).
  Backend matrix candle-CPU × {ONNX-CPU, ONNX-CUDA, ONNX-ROCm/DirectML/OpenVINO as available}; re-embed on
  the 3090s (cuda:0/1, exclude K620) when there is GPU room. Full plan: design package §5.
- **R-ONNX-1 acceptance:** a fixture text → vector within a *documented* Δ of the candle CPU reference.
- **Interim guard (R-ONNX-3, documented not enforced):** **same-backend build-and-read** — vectors written
  by one embedder backend are read/queried with the SAME backend until 0.8.18 #5 enforces a tolerance.
  Mirrors the 0.8.14 eu7 CPU-same-backend policy (`649a8d45`).
- **Feed-forward:** the recorded Δ (esp. the flip rate) is the input the 0.8.18 #5 vector-equivalence
  tolerance is calibrated against — record it precisely (plan §3).

## 4. Consequences / non-goals

- **Footprint invariant intact:** ONNX is a caller-supplied OFFLINE-BUILD/EVAL backend behind the trait;
  the in-library 1-bit CPU query path is unchanged; the thin default build gains no dep.
- **eu7 / R-GATE:** the shipped default path (candle CPU) is unchanged ⇒ eu7 no-op for the default build.
  ONNX-written vectors are governed by the same-backend read discipline until 0.8.18.
- **X1:** ONNX is Rust-crate-internal; if no SDK verb is added, assert no-new-verb (as 0.8.14 R-X-1).
- **Non-goal:** wiring ONNX into the `Default` variant / making it the shipped default. Non-goal: enforcing
  a candle↔ONNX tolerance (that is 0.8.18 #5). Non-goal: non-BGE models.

## 5. Open items for HITL (Slice-0 gate)

1. Mint the ONNX-equivalence measurement AC (R-ONNX-1 tolerance + R-ONNX-3 recorded Δ).
2. Confirm the `ort` crate + version is acceptable for the offline/eval build surface (no default-build
   footprint change).
