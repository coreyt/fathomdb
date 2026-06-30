# FathomDB 0.8.12 — Plan (GPU-rerank micro) · **opt-in GPU cross-encoder, default CPU unchanged**

> ⚠️ **NUMBERING CONFLICT — UNRESOLVED, NEEDS HITL/Steward.** The label "0.8.12" is
> **already assigned** to the *substrate & recall* release (EXP-S kind-tag substrate #2 + fielded-FTS/BM25F
> #16) in `dev/plans/plan-0.8.12.md` and in `0.8.6-0.8.16-PROGRAM-SEQUENCING.md` (rows I-2 / I-6 / the
> 0.8.12 program row — it is the *long pole*). This GPU-rerank micro was branched as `0.8.12-gpu-rerank`
> but it is **a different, much smaller piece of work** and **cannot share the 0.8.12 slot** with the
> substrate release. This plan is filed under `plan-0.8.12-gpu-rerank.md` (NOT over the existing
> `plan-0.8.12.md`) precisely so nothing is clobbered. **Resolve the real label before any publish:**
> candidate resolutions include shipping this as a later even micro (e.g. **0.8.14** GPU-perf, which is
> currently the GPU-perf-themed slot), folding it into the substrate release, or assigning a fresh even
> number — but **`0.8.13` is HITL-forbidden** (odd line skips 13) and an **odd** number is not publishable
> under the two-tier policy. **Do not publish this under "0.8.12" while the substrate release also claims
> it.** The engine change below stands on its own regardless of which number it eventually carries.
>
> **Theme.** Make the cross-encoder (CE) reranker able to run on **GPU as an opt-in**, mirroring the
> existing GPU *embedder* path (0.8.7). **Default is unchanged (CPU).** This is a self-contained engine
> port: a new Cargo feature + a new runtime env knob + shared device-parse reuse. No schema change, no
> re-embed, no new dependency (the CUDA/Metal candle backends already exist for the embedder).

---

## 1. Goal & scope

**In scope (this port only):**

- The default reranker `CandleTinyBertReranker`
  (`src/rust/crates/fathomdb-embedder/src/candle_reranker.rs`, model
  `cross-encoder/ms-marco-TinyBERT-L2-v2`) previously hard-coded `let device = Device::Cpu;`. Replace it
  with a resolved device so the **same BERT forward** can run on GPU when explicitly requested.
- A new Cargo feature **`rerank-cuda`** (and `rerank-metal`) on `fathomdb-embedder`, mirroring `embed-cuda`,
  plumbed up through `fathomdb-engine` and `fathomdb-py` exactly like `embed-cuda`.
- A new runtime knob **`FATHOMDB_RERANK_DEVICE`** (`cpu` | `cuda` | `cuda:N` | `metal`), **separate** from
  the embedder's `FATHOMDB_EMBED_DEVICE` so the CE reranker and the embedder can target devices
  independently.
- The device-grammar parser is **shared, not duplicated**: `parse_device_request` + `DeviceRequest` moved
  to a new `crate::device` module (`src/rust/crates/fathomdb-embedder/src/device.rs`), reused by both the
  embedder and the reranker. The pure R-GPU-1 grammar tests moved with it.

**Out of scope (explicitly NOT this micro):**

- Python/maturin/`.venv` exposure and the CPU↔GPU tuple-stability check (see §6 — that is the *next* step,
  run separately with an isolated venv).
- Any change to scoring math, blending (α=0.3), `rerank_depth`, batching (`MAX_CE_BATCH=32`), or the
  pinned model/revision/shas.
- Cross-vendor (AMD ROCm / Intel) — not a candle backend, same exclusion as the embedder.

---

## 2. Default-CPU-preserved posture (Decision 2 stands)

**Decision 2** (the reranker is latency-budgeted for CPU) **remains the default.** GPU is **purely
additive opt-in**:

- When **neither** `rerank-cuda` nor `rerank-metal` is compiled in (the default `default-reranker` build,
  and every shipped wheel/crate), `resolve_device()` reduces to "always `Device::Cpu`" — so the build is
  **byte-identical** to the prior hard-coded `Device::Cpu`. A GPU env request without the feature emits a
  **loud stderr warning** and falls back to CPU (the silent-slow-fallback trap is avoided), never silently.
- Device is **not** part of any identity (mirrors the embedder's `dev/design/0.8.1-embedder-gpu-and-portability.md`
  §3 cross-backend equivalence guard): the same logits are expected on CPU and GPU within tolerance, so
  the opt-in does not change rankings (validated — §5).

---

## 3. The `rerank-cuda` feature + `FATHOMDB_RERANK_DEVICE` knob

| Layer | Feature added | Mirrors |
|-------|---------------|---------|
| `fathomdb-embedder/Cargo.toml` | `rerank-cuda = ["default-reranker", "candle-core/cuda", "candle-nn/cuda", "candle-transformers/cuda"]` (+ `rerank-metal`) | `embed-cuda` (but pulls `default-reranker`, not `default-embedder`) |
| `fathomdb-engine/Cargo.toml` | `rerank-cuda = ["default-reranker", "fathomdb-embedder/rerank-cuda"]` (+ metal) | `embed-cuda` passthrough |
| `fathomdb-py/Cargo.toml` | `rerank-cuda = ["default-reranker", "fathomdb-engine/rerank-cuda"]` (+ metal) | `embed-cuda` passthrough |

- **Runtime knob:** `FATHOMDB_RERANK_DEVICE` (default `cpu`). Grammar identical to `FATHOMDB_EMBED_DEVICE`
  (`cpu`|`cuda`|`cuda:N`|`metal`), parsed by the shared `crate::device::parse_device_request`.
- `try_load()`'s **signature is unchanged**; device resolution happens inside `from_weights` via the env
  knob, exactly as the embedder resolves its device internally.

---

## 4. Slice map (single-slice micro)

1. Shared device module + reuse (no duplicate parse logic).
2. Reranker `resolve_device()` (gated arms on `rerank-cuda`/`rerank-metal`, `FATHOMDB_RERANK_DEVICE`).
3. Swap the hard-coded `Device::Cpu` → `resolve_device()`; keep + update the Decision 2 comment.
4. Cargo feature + 2-hop passthrough (engine, py).
5. Tests: shared R-GPU-1 parse tests; GPU smoke + CPU↔GPU closeness (gated on `rerank-cuda`).

---

## 5. Verification (done in this micro)

- `cargo build -p fathomdb-embedder` (default, CPU) — **green**.
- `cargo build -p fathomdb-embedder --features default-reranker` / `--features default-embedder` — **green**.
- `cargo test -p fathomdb-embedder --features default-reranker --lib` — **12/12 green** (real reranker
  forwards + shared device-parse tests). Behavior unchanged.
- `cargo build -p fathomdb-embedder --features rerank-cuda` — **green** (CUDA 12.6 toolkit;
  `CUDA_HOME=/usr/local/cuda`, `nvcc` on PATH).
- GPU smoke (`FATHOMDB_RERANK_DEVICE=cuda:0`): loads on a CUDA device, scores one pair → **finite logit
  7.385**.
- CPU↔GPU closeness: max abs logit diff = **1.4e-6** (tolerance 1e-2) — the port is numerically faithful;
  rankings are unaffected. The closeness test deliberately uses a **tolerance, not bit-equality** — GPU
  reductions are non-associative (~1e-4 drift is normal); the embedder's "1-bit identical" property does
  NOT generalize to the cross-encoder.

### 5.1 GPU speedup (RTX 3090, release build, ~250-token passages)

| batch (passages) | CPU | GPU | speedup |
|---|---|---|---|
| 1 | 6.30 ms | 0.79 ms | ~8x |
| 32 | 100.6 ms | 4.92 ms | ~20x |
| 128 | 648 ms | 19.5 ms | ~33x |
| 256 | 922 ms | 38.2 ms | ~24x |

Single-pair scoring is ~8x; the realistic batched rerank pool (`rerank_depth` ≫ 1, chunked at
`MAX_CE_BATCH=32`) is ~20-33x. (Numbers are from a one-off `--ignored` timing test, not committed.)

### 5.2 candle CPU→CUDA portability audit (web-researched, then verified against this build)

A web survey of candle 0.10.x CUDA gotchas was cross-checked against the actual code/build. Outcome —
**one real fix needed (applied); everything else already satisfied or empirically cleared by a full GPU
forward succeeding** (a live batched forward of embedding→2 attention layers w/ LayerNorm+softmax+mask→
CLS→pooler→classifier would have *panicked* if any of these were live):

- **matmul needs contiguous on CUDA, not CPU** (candle #2373) — **FIXED:** `.contiguous()` on the CLS slice
  (`narrow(1,0,1).squeeze(1)`) before the pooler `Linear` in both `score` and `score_chunk` (this is the
  only matmul this module authors; the attention matmuls live inside upstream `BertModel` and ran clean).
- **`cuda` feature must be on candle-core AND candle-nn AND candle-transformers** (candle #2217/#1330/#1916
  — else LayerNorm/softmax panic) — **SATISFIED:** `rerank-cuda` enables all three, mirroring `embed-cuda`.
- **mask/scalar constants must be on the CUDA device** — **SATISFIED:** all tensors built with
  `&self.device`; the extended-mask path is upstream and the forward succeeded.
- **stay F32 on CUDA** (F16 matmul not cuBLAS-routed; mixed-dtype errors) — **SATISFIED:** loaded + run in
  `DType::F32`, matching the CPU path.
- **process-wide `OnceLock` across reader threads** — **SAFE:** candle tensors are `Arc`-backed +
  `Send+Sync`; a single CUDA stream serializes concurrent forwards (throughput, not correctness).

---

## 6. CPU↔GPU tuple-stability check (bridge before V-3/V-7) — NEXT step, separate

This micro proves *unit-level* CPU↔GPU logit equivalence. Before any retrieval experiment runs the
reranker on GPU, a **product-level** stability check must confirm that moving the CE reranker onto the GPU
does not perturb the V-1 results enough to invalidate a mixed-device comparison. **Protocol (run NEXT, in
an isolated venv with the Python/maturin GPU build):**

1. Build the Python extension with `rerank-cuda` (`maturin develop --features
   pyo3/extension-module,rerank-cuda`) in an **isolated venv** (do not disturb the default CPU `.venv`).
2. Re-run **V-1's chosen tuples** on GPU and compare to V-1's CPU baseline:
   - needle: 200 / 200 / 0.7
   - multi_session: 300 / 200 / 1.0
   - temporal: 500 / 50 / 1.0
   - multi_hop (MuSiQue): 20 / 0.3
3. Confirm the **rankings and headline metrics hold within tolerance** (per-query rank stability + metric
   deltas inside noise). If they do, **V-3 and V-7 may run on GPU** without mixed-device contamination
   vs V-1's CPU baseline. If they do **not**, V-3/V-7 stay CPU, and the divergence is characterized →
   HITL (per the "characterize under-performance, then HITL" rule).

This is a **bridge experiment**, deliberately *not* part of the engine port, so the port can land and be
reviewed on its own merits.

---

## 7. Publish governance

- This is a **publishable micro** in principle (even-number, real engine change), **HITL-gated publish**
  like every `x.y.z` (two-tier policy: even + HITL approval = publishable; odd = not).
- **Merges AFTER 0.8.11.2** to preserve numbering order.
- **BLOCKED on the §0 numbering conflict:** it must **not** be published as "0.8.12" while the substrate
  release also owns that label. Resolve the number first (HITL/Steward), then publish via the normal
  `v*`-tag path. No `v*` tag, no manifest bump, and no publish were done in this micro — branch
  `0.8.12-gpu-rerank`, source + tests + this plan only.
