# 0.8.7 — status (GPU embedder · OUT-OF-BAND)

> Live state for the 0.8.7 GPU-embedder release. Plan → `dev/plans/plan-0.8.7.md`.
> Design → `dev/design/0.8.1-embedder-gpu-and-portability.md` §1. OOB: gates nothing,
> gated by nothing. Branched from **main as of 0.8.5** (`0a8f3f1a`) on worktree branch
> `0.8.7-gpu-embedder`.

## Verdict — COMPLETE (pending review)

All R-GPU acceptance criteria met. The device seam shipped earlier (it landed with
the 0.8.1 embedder GPU/batch work, commit `02297cb3`, and is in the 0.8.5 baseline);
0.8.7 **validated it on real hardware, added the missing env-grammar unit tests, and
recorded the measured CUDA-vs-CPU speedup**.

## Slice ladder

| Slice | Title | State |
|------:|-------|-------|
| 0 | Setup + ADR confirm | ✅ §1 design confirmed current; interim same-backend discipline pinned (below + `docs/embedder.md`) |
| 5 | Device seam KEYSTONE | ✅ ALREADY LANDED (`02297cb3`); gap closed: pure `parse_device_request` extracted + 7 unit tests (R-GPU-1); default CPU build unperturbed (R-GPU-2) |
| 10 | GPU validation + speedup measure | ✅ `embed-cuda` builds + runs on `cuda:0`; CUDA-vs-CPU re-embed measured; serialization/parity checked |
| 40 | Verification + release readiness | ✅ X1/X2/X3 + R-GPU AC gate (this doc) |

## Acceptance criteria

| ID | Requirement | Result |
|----|-------------|--------|
| R-GPU-1 | `resolve_device()` selects CPU/CUDA/Metal from env; CPU default | ✅ `parse_device_request` is pure + unit-tested over `cpu`/empty/`cuda`/`cuda:N`/`cuda:garbage`/`metal`/unknown (7 tests, default build) |
| R-GPU-2 | Default build byte-identical to today | ✅ embedder default-embedder suite 26/26 green; default path (`FATHOMDB_EMBED_DEVICE` unset → `Device::Cpu`) unchanged; clippy clean |
| R-GPU-3 | `embed-cuda` builds + runs on the GPU box | ✅ `cargo build/clippy/run --features embed-cuda` on CUDA 12.6 / RTX 3090; embed ran on `cuda:0`. Full feature chain compiles end-to-end: `cargo check --features embed-cuda` on `fathomdb-engine` AND `fathomdb-py`. **Deferred (coordinated):** the `maturin develop --features pyo3/extension-module,embed-cuda` Python-extension build + a py embed-on-`cuda:0` smoke must run on the **shared MAIN tree** (see Shared-build coordination) — not run here (worktree scope + .venv mutex with 0.8.6) |
| R-GPU-4 | Measured speedup recorded | ✅ see table below — up to **93×** batched, **15.6×** per-doc, **~46×** best-path bulk re-embed |
| R-GPU-5 | Serialization holds on GPU | ✅ PR-9 single-`embed()` guard unchanged; harness is single-threaded; batched-vs-single parity min-cosine 0.999999 |
| R-GPU-6 | Interim safety documented | ✅ same-backend build-and-read discipline + `EmbedderIdentity` pre-filter documented (`docs/embedder.md`); cross-backend divergence measured (below) |

## Measured speedup (R-GPU-4)

`examples/gpu_speedup.rs`, frozen 2,000-doc synthetic corpus, single RTX 3090
(`cuda:0`), release build, model load excluded from timing:

| Path | CPU | CUDA (`cuda:0`) | Speedup |
|------|-----|-----------------|---------|
| per-document `embed()` | 112.6 s (17.8 docs/s) | 7.22 s (276.8 docs/s) | **15.6×** |
| batched `embed_batch()` (B=64) | 228.2 s (8.8 docs/s) | 2.43 s (821.8 docs/s) | **93.4×** |

- Best-path bulk re-embed (CPU per-doc → CUDA batched): **~46×**.
- On CPU, batching is *slower* than per-doc (padding overhead, no parallelism to
  amortize); on GPU, batching wins ~3×. So GPU re-embed should use `embed_batch`.
- A 27-hour CPU re-embed → ~minutes, matching the design-doc expectation.

## Cross-backend equivalence (informs R-GPU-6)

Same 16-doc probe set embedded on CPU vs `cuda:0`:

- min cosine(CPU, CUDA) = **0.99999983**
- max |Δ| per component = **1.6e-7** (float32 noise floor)
- **1-bit sign-bit (Hamming) disagreement = 0 / 6,144 bits (0.0000%)**

At the representation the retrieval path actually uses (1-bit sign quantization),
CUDA-built and CPU-read codes were **identical** on this probe set. The float-level
divergence is far below the binary-quantization floor. The same-backend discipline
is the conservative interim rule; the §3 probe-set self-check (a later release,
e.g. 0.8.16) will replace it with an enforced, calibrated tolerance. (Caveat: 16
probes — decisive directionally, not a full-corpus certification.)

## Changes (worktree `0.8.7-gpu-embedder`)

- `src/rust/crates/fathomdb-embedder/src/candle_bge.rs` — extract pure
  `parse_device_request` + `DeviceRequest`; `resolve_device` dispatches on it;
  add `device_request_tests` (R-GPU-1). No change to the default CPU device or
  the embed math.
- `src/rust/crates/fathomdb-embedder/examples/gpu_speedup.rs` — CUDA-vs-CPU
  measurement harness (+ `[[example]] required-features = ["default-embedder"]`
  so `cargo build --examples` on the thin no-feature build stays green).
- `docs/embedder.md` — GPU acceleration section (feature + env surface, measured
  speedup, interim cross-backend discipline). `docs/reference/{python,typescript}-api.md`
  — embedder-device note (no new binding API). `dev/DOC-INDEX.md` — index row.

## Footprint

OFFLINE-BUILD / EVAL-ONLY. GPU is an opt-in build/eval accelerator. The **default**
(no-feature / no-env) build is fully CPU. When opted in, the selected device embeds
both ingest AND the per-query query string; what stays CPU-only / 1-bit (Hamming) /
deterministic regardless is the *retrieval* path — the stored sign-bit index, the
Hamming scan, and RRF fusion. The GPU embedder never enters the **default** library
footprint contract.

## Shared-build coordination (0.8.6 ∥ 0.8.7)

0.8.6 and 0.8.7 run in parallel — **source is isolated** (separate worktrees) but
the **build is not**: every `maturin develop` must run on the shared MAIN tree /
shared `.venv` (a worktree maturin rebinds `.venv` — `agent-worktree-stale-base-trap`).
Treat the MAIN-tree build as a **mutual-exclusion resource**:

- Only one `maturin develop` at a time; coordinate build windows via the steward.
- Before running any Py/TS parity or integration test, **confirm `.venv` currently
  holds the 0.8.7 `embed-cuda` build** (not 0.8.6's default build).
- No GPU contention (only this release uses the GPU).

This release needed **no** MAIN-tree build to reach its verdict: all GPU validation
was pure `cargo` in the worktree (which does not touch `.venv`), and 0.8.7 adds no
new binding API, so X1 is a docs-parity assertion, not a Py/TS runtime test. The
single MAIN-tree step (R-GPU-3 maturin build + py `cuda:0` smoke) is deferred to a
coordinated window and is a confirmation, not a blocker — the embedder GPU path is
already proven via cargo and the pyo3 crate compiles with `embed-cuda`.

## Rebase note

Branch cut from **main as of 0.8.5** (`0a8f3f1a`) per the goal, then **rebased onto
current main** (`b9a0c21c`, clean / no conflicts — main had not touched the embedder
crate since 0.8.5) so the branch carries its own `plan-0.8.7.md`, the master
sequencing doc, and the harness preflight gate (`scripts/preflight.sh`).
`scripts/preflight.sh --worktree` → `preflight: pass`.

## Build/run notes

- CUDA 12.6 toolkit at `/usr/local/cuda`; build env `PATH=/usr/local/cuda/bin:$PATH`,
  `CUDA_COMPUTE_CAP=86` (RTX 3090, sm_86). Driver 580.x (CUDA 13 capable, runs 12.6 builds).
- **Only the two RTX 3090s** (`cuda:0`, `cuda:1`) are usable for compute on this box;
  index 2 (Quadro K620) is the display GPU — do not target it.
- The `embed-cuda` validation here is a pure `cargo` build/run, which is worktree-safe.
  The **`maturin develop --features embed-cuda`** Python-extension build must still run
  on the MAIN checkout only (worktree maturin rebinds the shared `.venv` — the
  `agent-worktree-stale-base-trap`).
