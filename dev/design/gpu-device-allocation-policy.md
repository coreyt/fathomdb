# GPU device-allocation policy — design-on-spec (embedder + CE reranker)

> **DESIGN-ON-SPEC — NOT APPROVED. NO engine code changes.**
>
> This document is a *contingent* design produced so that, **if** HITL elects to harden FathomDB's
> GPU device handling beyond today's raw device-string knob, there is a grounded, source-anchored
> plan ready. It is **not** a build order. The current device seam (`FATHOMDB_EMBED_DEVICE`,
> `FATHOMDB_RERANK_DEVICE`, the `embed-cuda`/`rerank-cuda` features, the loud-CPU-fallback) ships as
> designed; nothing here re-opens it. This proposes the *next* layer — a safe-by-default allocation
> policy so a user who flips on GPU CE-rerank cannot OOM a busy GPU, fight a co-resident LLM server,
> or grab the display GPU and hang the desktop. The load-bearing sections are §0.5 (the **two-tier
> scope**), §1 (what exists today), and §4 (the recommendation).
>
> **Date:** 2026-06-30 · **Owner:** program steward · **Roadmap fit:** device-seam family
> (#3 GPU-seam / #4 ONNX / #5 vector-equivalence); see §5.
>
> **v2 — HITL decisions 2026-06-30.** HITL reviewed v1 and (a) added a **vendor-abstraction**
> requirement (#0: target NVIDIA now, but never NVIDIA-*only*); (b) **endorsed** #1 (optional NVML +
> `nvidia-smi` fallback) and #2 (`auto` never implied; GPU always explicit), the latter with two
> additions — a **startup device-eligibility re-check** and a **`doctor` system-review**; (c) flagged
> #3 (strict-mode scope) and #4 (candle memory budget) as **research-needed**, now resolved below;
> (d) directed a **two-tier scope split** (§0.5). The genuinely-open items were researched (candle
> memory-cap capability, per-call probe overhead, the vendor-abstraction boundary); findings are in
> §2.8 and folded into §3.5/§4/§7.
>
> **Changelog:** v1 (348dd44e, 2026-06-30) initial design-on-spec → **v2 (this commit, 2026-06-30)**
> HITL decisions incorporated + #0 vendor abstraction + two-tier scope + research for #0/#3/#4. **v2
> residual-resolution (same commit, 2026-06-30):** HITL confirmed (1) per-engine strict (#3) stays a
> *proposed-not-locked* default; (2) **Tier 2 → the 0.8.16 slot** (no standalone 0.8.18); (3) the
> `doctor` system-review may be a `doctor`/admin surface and **may be housed in the 0.8.8 EXP-OBS
> telemetry/explain sidecar** rather than a new CLI subcommand. Folded into §0.5/§4.1/§5/§7.

---

## 0. The problem (from HITL)

FathomDB's GPU knob today is a **raw device string** that *assumes the named GPU is free and the
caller's to take*. That assumption breaks across real machines:

- **This dev box** — 3 discrete GPUs: two RTX 3090s that may be **fully utilized** by a co-resident
  LLM service (vLLM in docker), and one Quadro whose job is **driving the video/display**. A naive
  "use `cuda:0`" can OOM a busy 3090, contend with the vLLM server's CUDA context, or (worst) grab
  the display GPU and stall the desktop.
- **Userland machines** — far more limited: weak integrated GPUs (shared system RAM), or no GPU at
  all. "Use my GPU" must degrade safely.
- **Non-NVIDIA hardware** *(v2 / HITL #0)* — userland machines also run AMD and Intel GPUs, and Apple
  Silicon. The policy must **target NVIDIA first but never bake in NVIDIA-only assumptions**: the
  device-probe substrate (NVML) is NVIDIA-only, so it must sit behind a **vendor-neutral trait** with
  a planned path for AMD/Intel (§3.5).

A user flips on GPU CE-rerank expecting it to "just work." The design must make that **safe across the
whole spectrum**. HITL named the mechanisms to evaluate and choose among: **config settings ·
dynamic GPU allocation · user pre-identifies which GPU / how much GPU · CPU fallback · CPU
fallback + HITL · CPU fallback + notification.**

This sits squarely on FathomDB's posture: **function over footprint / user-controlled spend** — GPU
is an **opt-in knob, default CPU**, and turning it on must **never destabilize the host**.

---

## 0.5. Scope — TWO TIERS (HITL directive, v2)

HITL split this work into two tiers so the safety basics needed for the **current memex/fathomdb
experiment campaign** ship immediately, decoupled from the larger vendor-neutral release effort.

### TIER 1 — MVP now (this machine + the V-3/V-7 experiment work)

The **minimal safe basics** to run GPU CE-rerank on this box (2×3090 + display Quadro) for the
0.8.14 GPU-rerank work / V-3/V-7 campaign. The experiments are *already* safe because they pin
`cuda:0`/`cuda:1` explicitly; Tier 1 adds the two things that make **casual use on this machine**
safe too — a display-GPU exclude and an observable fallback. **Exactly three behaviors land:**

| # | Knob / behavior | What it is | New? |
| --- | --- | --- | --- |
| T1.1 | **Explicit device pin** — `FATHOMDB_{EMBED,RERANK}_DEVICE=cpu\|cuda\|cuda:N\|metal` | Already shipped (the `device.rs` parser + per-backend `resolve_device`). No change. | exists |
| T1.2 | **Display-GPU exclude-list** — `FATHOMDB_GPU_EXCLUDE=N[,M,…]` | A **static, user-declared** comma list of CUDA indices that FathomDB must **never** allocate on. Purely index-based string compare against the parsed pin — **no probing, no NVML**. If the pinned (or `cuda`-defaulted `cuda:0`) index is in the exclude set, FathomDB refuses the GPU and takes the typed-notify fallback (T1.3). This is how the user declares "index 2 is the Quadro, hands off." | **new** |
| T1.3 | **Typed-notify CPU fallback** | On any GPU-ineligible/refuse/init-failure path, drop to CPU **and** emit a **typed, in-band event** (`{reason, requested, chosen: Cpu}`) in addition to the existing loud stderr — so an embedder running inside a host agent (Memex/Hermes) that never reads child stderr can *observe* the fallback. Closes gap §1.4-#6. | **new** |

**Explicitly NOT in Tier 1:** no NVML / `nvidia-smi` probing, no `auto` selection, no vendor
abstraction trait, no memory budget/fraction, no UUID pin, no startup re-check, no `doctor`. The
exclude-list in Tier 1 is *user-declared indices only* — FathomDB does **not** detect which device
drives the display (that needs NVML and is Tier 2). Tier 1 is pure, unit-testable with no GPU and no
feature build, and **folds into the 0.8.14 GPU-rerank slice**. Default fallback mode is `notify`
(the `strict`/`silent` mode selector is Tier 2).

> Why this is enough for now: the campaign already pins `cuda:0`/`cuda:1` (the two 3090s) explicitly,
> so it never touches the Quadro. T1.2 + T1.3 generalize that safety to anyone who flips the knob on
> this machine without knowing the topology by index — a wrong/`cuda:0` default that lands on the
> excluded display GPU is refused loudly and observably instead of hanging the desktop.

### TIER 2 — Full release work (the published GPU-capable package)

The complete #0–#4 surface, for the released package that ships GPU functionality to userland
(heterogeneous, multi-vendor, untrusted topology). Each item maps to a device-seam roadmap slot
(§5):

| Item | What | Roadmap slot |
| --- | --- | --- |
| **#0/#1 vendor-neutral probe trait** | `DeviceProbe`/`GpuBackend` trait (§3.5); NVML/`nvidia-smi` = the NVIDIA impl; planned AMD (ROCm-SMI) / Intel (Level-Zero) impls. | with 0.8.16 ONNX |
| **#1 NVML dynamic probing** | optional feature-gated `nvml-wrapper` dep + `nvidia-smi` shell-out fallback, behind the #0 trait. | with 0.8.16 ONNX |
| **#2 startup re-check + `doctor`** | startup device-eligibility re-validation (never trust cached/configured device identity) + a `doctor`/admin system-review that enumerates GPU availability + eligibility (HITL: may be housed in the 0.8.8 EXP-OBS sidecar). | with 0.8.16 ONNX |
| **#3 strict-scope** (researched) | `require_gpu`/`strict` as a **per-engine** setting (per-call rejected on overhead grounds — §3.5/§2.8). | with NVML layer |
| **#4 memory budget** (researched) | **advisory** free-VRAM eligibility gate on candle (cudarc `mem_get_info`); **hard** `gpu_mem_limit` cap on the ONNX path. | advisory: with NVML layer · hard: 0.8.16 ONNX |
| **UUID pin + `auto` select** | stable `cuda:uuid:<UUID>` pin + probe-and-pick `auto`. | with NVML layer |
| **Vendor breadth (AMD/Intel/DirectML)** | reachable only via the ONNX Runtime EP path, not candle. | ONNX path (0.8.16+) |

---

## 1. Current-state survey — what FathomDB does today

Source: `0.8.14-gpu-rerank` branch, shared crate
`src/rust/crates/fathomdb-embedder/src/{device.rs,candle_bge.rs,candle_reranker.rs}`.

### 1.1 The shared parser (`device.rs`)

A single **pure, total** parser, `parse_device_request(raw) -> DeviceRequest`, is shared by both the
embedder (`FATHOMDB_EMBED_DEVICE`) and the reranker (`FATHOMDB_RERANK_DEVICE`). It is deliberately
free of `Device` construction, `#[cfg]` gating, and I/O so the grammar is unit-testable with no GPU
and no feature build. Grammar:

| Input (case-insensitive, trimmed) | `DeviceRequest` |
| --- | --- |
| `""` / whitespace / `cpu` | `Cpu` |
| `cuda` | `Cuda(0)` |
| `cuda:N` | `Cuda(N)` |
| `cuda:x` / `cuda:` (malformed) | `Cuda(0)` (clamps, never panics) |
| `metal` | `Metal` |
| anything else (`rocm`, `gpu`, …) | `Unknown(raw)` |

### 1.2 Request → device mapping (`resolve_device`, per backend)

Each backend owns its `resolve_device()` (embedder and reranker are gated on **independent** features
`embed-cuda` vs `rerank-cuda`). The mapping is identical except for env var + feature names:

- `Cpu` → `Device::Cpu`.
- `Cuda(idx)` → if the `*-cuda` feature is compiled in, `Device::new_cuda(idx)`; on **init failure**
  *or* if the feature is **absent**, emit a **LOUD stderr warning** and return `Device::Cpu`.
- `Metal` → analogous via `Device::new_metal(0)` / `*-metal` feature.
- `Unknown(req)` → loud stderr warning ("expected cpu|cuda|cuda:N|metal"), `Device::Cpu`.

The **default build stays CPU**; GPU is opt-in via feature + env. The fallback is deliberately
**loud, never silent** (avoids the "silent-slow 100× CPU fallback" trap). The warning path is
`#[allow(clippy::print_stderr)]` because it is construction-time only — never in `embed()`.

### 1.3 candle backend constraints

- candle 0.10 supports **only** CPU / CUDA / Metal — **no ROCm, no Vulkan, no DirectML, no SYCL**.
  AMD/Intel discrete + integrated GPUs are **unreachable through candle's `Device`**; only **NVIDIA
  (CUDA) and Apple (Metal)** are reachable at the candle compute layer (CPU is the universal
  fallback). Confirmed v2: AMD ROCm in candle exists only as an unmerged WIP PR (#3424), not
  mainline. The durable cross-vendor path is a new `impl Embedder` behind the trait, e.g. ONNX
  Runtime — see §2.8 / §3.5 and `0.8.1-embedder-gpu-and-portability.md` §2.
- The model is **serialized** (PR-9 guard: one `embed()` at a time), so single-CUDA-stream is the
  intended behavior, not a limitation — it prevents concurrent CUDA calls on the shared model.
- Compute is **F32** on the candle path.
- candle **cannot cap or fraction GPU memory** (v2 research §2.8) — every allocation is on-demand via
  cudarc with no pool/arena ceiling. It also exposes **no VRAM-query API of its own**; free/total VRAM
  is reachable only by dropping to the underlying `cudarc` context (`mem_get_info`). This is why the
  memory budget is **advisory on candle, hard only on ONNX** (§4.3, #4).
- The 0.8.16 ONNX path will **extend `resolve_device()`** (ONNX Runtime CUDA EP carries its own
  device-id + `gpu_mem_limit` hard cap — see §2.4), which is the natural insertion point for a real
  allocation budget.

### 1.4 Gaps vs the problem

What today's seam **does not** do — every one of these is a way the HITL scenario bites:

1. **No free-memory / utilization probe.** `Device::new_cuda(idx)` succeeds as long as the device
   *exists* and a context can be created; it does **not** check whether a 3090 is already saturated
   by vLLM. The CUDA context is created fine; the **OOM arrives later**, at model-load or first
   forward, as a hard failure — *after* the loud-fallback decision has already been made. The
   construction-time fallback cannot catch a *runtime* OOM. *(v2: and a free-VRAM probe cannot
   reliably prevent it either — §2.8.)*
2. **No display-GPU exclusion.** `cuda:0` is a positional index. If the Quadro is enumerated at the
   index the user (or a default) names, FathomDB will happily try to allocate on the display GPU.
   There is no notion of "this device drives the desktop, never auto-pick it." *(Tier 1 closes the
   user-declared-index form; NVML display-detection is Tier 2.)*
3. **No auto-select.** The user must name an exact index. There is no "pick the least-loaded eligible
   GPU." A user who just wants "use a GPU" has to know their topology. *(Tier 2.)*
4. **No memory budget / fraction cap.** FathomDB takes whatever candle's allocator grabs. There is no
   "use at most 2 GiB / at most 20%," so even a *successful* claim on a shared 3090 can starve the
   co-resident vLLM server. *(Tier 2; advisory-only on candle — §4.3.)*
5. **Index instability.** `cuda:N` is positional and **reorders** across reboots / driver changes /
   `CUDA_VISIBLE_DEVICES` (NVML and Ollama both warn on this — §2). There is no UUID pin, so a pin
   that meant "the spare 3090" can silently become "the display Quadro." *(Tier 2 UUID pin; the HITL
   #2 startup re-check is the partial mitigation — see §4.1.)*
6. **Fallback is loud but not *observable in-band*.** The warning is stderr only — fine for a CLI,
   invisible to a library embedder running inside a host agent (Memex/Hermes) that never reads the
   child's stderr. There is no typed signal / telemetry event a host can react to. *(Tier 1 closes
   this — the single highest-value safety item.)*
7. **No vendor abstraction** *(v2 / #0)*. The seam is implicitly NVIDIA+Apple (candle's reach) with no
   trait boundary that a future AMD/Intel probe or ONNX EP could plug into without touching the
   grammar/policy layer. *(Tier 2 — §3.5.)*

These are exactly the failure modes §4 must close. Note the seam is **well-shaped for the fix**: the
parse is pure and the request→device mapping is already the single chokepoint, so an eligibility +
probe layer slots in **between** `parse_device_request` and `Device::new_cuda` without touching the
grammar or the trait.

---

## 2. Web research — how other systems manage GPU selection / limits / fallback

Synthesized below; per-mechanism citations inline, full list in §6. The recurring **patterns** are
called out in §2.7; **v2 research** (candle memory cap, per-call overhead, vendor boundary) is §2.8.

### 2.1 PyTorch

- **Selection** is primarily by the `CUDA_VISIBLE_DEVICES` env var (a comma list of indices or UUIDs);
  the docs explicitly say `torch.cuda.set_device()` is *discouraged* in favor of the env var / device
  context manager. Masking with the env var is the canonical "make only these GPUs exist" lever.
- **Memory cap:** `torch.cuda.set_per_process_memory_fraction(fraction, device)` caps the caching
  allocator at `total_visible_memory * fraction`; over-allocation raises an OOM in the allocator
  rather than silently spilling. This is a *fraction*, not a hard reservation.
- **Busy/absent behavior:** PyTorch does **not** probe utilization for you — it claims the device and
  OOMs on demand. The pattern it *gives* you is masking (visibility) + a fraction cap.
  Sources: [torch.cuda](https://docs.pytorch.org/docs/stable/cuda.html),
  [set_per_process_memory_fraction](https://docs.pytorch.org/docs/stable/generated/torch.cuda.memory.set_per_process_memory_fraction.html),
  [CUDA semantics](https://docs.pytorch.org/docs/stable/notes/cuda.html).

### 2.2 vLLM

- **Selection:** the *recommended* way to pin a GPU is `CUDA_VISIBLE_DEVICES` (e.g. `=2,5` maps
  physical 2/5 to logical `cuda:0`/`cuda:1`); vLLM integrates this into its platform device control.
- **Memory budget:** `--gpu-memory-utilization` (default **0.9**) is the fraction of GPU memory vLLM
  reserves for weights + KV-cache; you tune it up (≤0.95) for throughput or **down** to co-locate
  multiple instances on one GPU (e.g. two instances at 0.45 each). This is the single most-cited
  "budget cap so two tenants share one GPU" knob.
- **Busy/absent behavior:** vLLM reserves its fraction up-front at load; if it cannot, it fails to
  start. The pattern: **explicit visibility mask + an explicit memory-fraction budget**, chosen by
  the operator, not auto-probed.
  Sources: [Conserving Memory](https://docs.vllm.ai/en/latest/configuration/conserving_memory/),
  [assigning a specific GPU (#28132)](https://github.com/vllm-project/vllm/issues/28132),
  [CUDA_VISIBLE_DEVICES (#2387)](https://github.com/vllm-project/vllm/issues/2387).

### 2.3 Ollama (and llama.cpp under it)

- **Auto-detection is the default:** since v0.4 Ollama auto-detects every NVIDIA/AMD GPU, estimates a
  model's VRAM need, and **compares against currently-available GPU memory** to decide placement —
  prefers fitting entirely on one GPU (fewer PCIe transfers), else splits. This is the cleanest
  example of **free-memory-aware auto-placement** in a consumer tool.
- **Visibility control:** `CUDA_VISIBLE_DEVICES` to restrict to a subset; the docs **explicitly
  recommend UUIDs over numeric IDs because "ordering may vary."** (Direct support for the §1.4-#5
  index-instability gap.)
- **Manual override:** `num_gpu` (llama.cpp `-ngl`) sets how many layers go on GPU; `-1` = auto.
- **llama.cpp primitives underneath:** `--main-gpu`, `--split-mode` (none/layer/row),
  `--tensor-split` (per-GPU proportions), and a `--device` selector with `--list-devices` to print
  available devices **and their memory** before choosing.
  Sources: [Ollama GPU docs](https://docs.ollama.com/gpu),
  [Ollama multi-GPU notes](https://knightli.com/en/2026/04/19/ollama-multiple-gpu-notes/),
  [llama.cpp multi-gpu.md](https://github.com/ggml-org/llama.cpp/blob/master/docs/multi-gpu.md).

### 2.4 ONNX Runtime (CUDA EP) — directly relevant to the 0.8.16 path

The CUDA execution provider takes a provider-options struct:

- `device_id` — which GPU.
- `gpu_mem_limit` — **hard byte cap** on the EP's memory arena (the closest thing to a real budget
  reservation in this survey; "total device memory usage may be higher" but the arena is bounded).
- `arena_extend_strategy` — `kNextPowerOfTwo` vs `kSameAsRequested` (growth aggressiveness).
- `cudnn_conv_algo_search` — exhaustive search allocates a scratch workspace; can be narrowed to cut
  memory.

Because FathomDB's ONNX backend will be a new `impl Embedder` that constructs its own session, these
options are exactly where a **per-device memory budget** lands for the cross-vendor path. ONNX EP also
covers **ROCm/MIGraphX (AMD), DirectML (any DX12 GPU on Windows — AMD/Intel/NVIDIA), OpenVINO (Intel),
CoreML (Apple)** — the reason it is the durable cross-vendor seam (§2.8/§3.5).
Sources: [CUDA EP docs](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html),
[OrtCUDAProviderOptions](https://onnxruntime.ai/docs/api/c/struct_ort_c_u_d_a_provider_options.html),
[Execution Providers](https://onnxruntime.ai/docs/execution-providers/).

### 2.5 HuggingFace Accelerate

- `device_map="auto"` / `"balanced"` fills the **fastest devices first** (GPU → CPU → disk), never
  exceeding the available memory of any GPU, and **reserves headroom on GPU 0** for the largest
  offloaded layer.
- `max_memory={0: "10GiB", 1: "20GiB", "cpu": "30GiB"}` is an explicit **per-device budget dict**;
  unset, it defaults to each GPU's full memory.
- Pattern: **auto-fit against a declared (or detected) per-device budget**, with explicit
  per-device caps as the override.
  Sources: [Big Model Inference](https://huggingface.co/docs/accelerate/usage_guides/big_modeling),
  [Loading big models](https://huggingface.co/docs/accelerate/en/concept_guides/big_model_inference).

### 2.6 NVML / nvidia-smi — the probing substrate

NVML (the library behind `nvidia-smi`; Python via `pynvml`; Rust via `nvml-wrapper`) is how every
auto-placer above actually learns the machine. Relevant queries:

- `nvmlDeviceGetMemoryInfo(handle)` → total / free / used VRAM per device (the free-memory probe).
- `nvmlDeviceGetUtilizationRates` / `--query-gpu=utilization.gpu,utilization.memory,memory.free` →
  current load (distinguish "exists" from "busy"). *(v2 caveat: window-averaged, see §2.8.)*
- `nvmlDeviceGetUUID` → the **stable identifier** for a device across reorder (what Ollama recommends
  pinning to).
- **Display detection:** `nvmlDeviceGetDisplayActive` (is a display/X server initialized on this
  device — can be true with no monitor attached) and `nvmlDeviceGetDisplayMode` (is a physical monitor
  on a connector). These are the primitives to **exclude the Quadro that drives the desktop**.
- `nvmlDeviceGetComputeMode` → whether the GPU permits shared / exclusive compute contexts.
  Sources: [NVML Device Queries](https://docs.nvidia.com/deploy/nvml-api/group__nvmlDeviceQueries.html),
  [pynvml](https://github.com/gpuopenanalytics/pynvml),
  [nvml-wrapper (Rust)](https://docs.rs/nvml-wrapper/latest/nvml_wrapper/).

### 2.7 The patterns (what to steal)

| Pattern | Who | FathomDB relevance |
| --- | --- | --- |
| **Visibility mask** (`CUDA_VISIBLE_DEVICES`, UUIDs > indices) | PyTorch, vLLM, Ollama, llama.cpp | Cheapest exclude-the-display-GPU lever; **respect it, don't fight it** |
| **Explicit pin** (`device_id`, `--main-gpu`, `cuda:N`) | all | FathomDB already has this (`cuda:N`); add **UUID pin** for stability (Tier 2) |
| **Memory-fraction / byte budget** (`gpu-memory-utilization`, `set_per_process_memory_fraction`, `gpu_mem_limit`, `max_memory`) | vLLM, PyTorch, ONNX, Accelerate | The lever that lets FathomDB **co-exist** with a busy 3090 instead of evicting it — **but candle can't do it** (§2.8); ONNX path only |
| **Free-memory-aware auto-placement** (probe then place) | Ollama, Accelerate | The "just pick a good GPU" UX; needs NVML (Tier 2) |
| **Probe-before-claim** (`--list-devices` w/ memory; NVML mem/util) | llama.cpp, NVML | Turns a *runtime* OOM into a *construction-time* refusal — **partially**; a probe cannot guarantee the later alloc (§2.8) |
| **Display / compute-mode awareness** | NVML | The only principled way to **never grab the desktop GPU** (Tier 2; Tier 1 uses a user-declared index list) |
| **Auto with safe fallback ladder** (GPU → CPU → disk) | Accelerate, Ollama | Maps to FathomDB's CPU fallback, but FathomDB's default is the *reverse* (CPU unless asked) |

Key contrast: every tool above is **GPU-first** (it's an inference server whose job is the GPU).
FathomDB is a **library embedded in someone else's process** whose default is **CPU**, so it inherits
the patterns but **inverts the default** and weights **host-safety over throughput**.

### 2.8 v2 research — the three genuinely-open questions

Researched 2026-06-30 (WebSearch/WebFetch against candle/cudarc/NVML/ONNX/wgpu sources) to close
HITL's #3/#4 and the #0 vendor boundary.

#### (a) Can candle enforce a GPU memory budget? — NO hard cap; advisory free-VRAM read is feasible

- **Hard cap: NO.** candle-core's CUDA backend (`cuda_backend/device.rs`) is a thin pass-through to
  cudarc (`stream.alloc` → `cuMemAllocAsync` on the stream's default pool) — **allocate-on-demand,
  no per-process limit, no fraction, no arena/pool ceiling**. A repo-wide search for `mem_get_info` /
  `memory_fraction` / `per_process_memory` in candle returns **zero matches**. There is no equivalent
  of torch `set_per_process_memory_fraction`, vLLM `--gpu-memory-utilization`, or ONNX `gpu_mem_limit`.
  An oversize workload OOMs at allocation time, unbounded.
- **Read free/total VRAM: YES — but only via cudarc, not candle's own API.** cudarc exposes
  `mem_get_info() -> (free, total)` (driver `cuMemGetInfo_v2`) at both the result and safe
  (`CudaContext::mem_get_info`) levels. So FathomDB can read free VRAM **before** model-load by
  reaching the cudarc context underneath candle's `CudaDevice`.
- **Therefore:** on candle the budget is **advisory** — a pre-load free-VRAM/min-free **eligibility
  gate** (refuse/route if `free < threshold`), *not* a hard allocator cap. Caveat (NVIDIA docs):
  `cuMemGetInfo` is a point-in-time, racy estimate and the driver may not be able to allocate all
  OS-reported free memory — so it gates *eligibility*, it does not *guarantee* the subsequent alloc.
  The **real hard cap arrives only on the ONNX path** (`gpu_mem_limit`), or by FathomDB building a
  custom capped allocator on cudarc's `cuMemPool*` (which caps by release-threshold semantics, not a
  strict ceiling — not worth it).
  Sources: [candle cuda_backend/device.rs](https://github.com/huggingface/candle/blob/main/candle-core/src/cuda_backend/device.rs),
  [cudarc result.rs](https://github.com/coreylowman/cudarc/blob/main/src/driver/result.rs),
  [cudarc safe/core.rs](https://github.com/coreylowman/cudarc/blob/main/src/driver/safe/core.rs).

#### (b) Per-call vs per-engine device validation overhead — per-engine; per-call rejected

- **NVML in-process is cheap but not free, and scales with what you query.** Static/string fields are
  fast; **sensor-backed fields (notably `utilization`) are slow**, and NVML guidance is explicitly
  "the more metrics you query the more overhead you incur." Worse, `nvmlDeviceGetUtilizationRates` is
  **sample-averaged over a ~30–50 Hz driver window** — a per-call utilization read is *stale and
  coarse*, not the instant of your allocation.
- **`nvidia-smi` per call is an order of magnitude worse:** each invocation **forks a process and
  re-runs `nvmlInit`** (tens of ms). Measured: frequent `nvidia-smi` polling alongside a CUBLAS
  workload caused **~20% throughput degradation** from contending the driver. Unsuited to per-call use.
- **A per-call re-check buys ~nothing.** The device handle / CUDA context is created once and cached;
  device *eligibility* (exists, CUDA usable, `require_gpu` satisfiable) is a **static** property —
  nothing to re-validate per call. The only per-call variable is transient free-VRAM, and **a probe
  cannot prevent OOM**: NVIDIA (Robert Crovella) on checking free memory to avoid OOM — *"No. You
  cannot. Not in any case, in any setting, using either of the functions, for any OS."* It's a TOCTOU
  race; the **authoritative guard is the allocation attempt itself** (+ a memory budget), with OOM
  handled at the call site.
- **Standard pattern confirms it:** connection pools (HikariCP/Tomcat-JDBC/Oracle-UCP) found
  validate-on-every-borrow "may incur significant overhead" and moved to **validate-at-open + trust
  the cached handle + handle-failure-at-use**. Same mapping here.
- **Recommendation:** `require_gpu`/strict is a **per-engine** setting — resolved + validated **once
  at engine open**, plus the HITL #2 **one-time startup re-check**. A per-call override, if wanted at
  all, is **opt-in, off-by-default** for debugging only. *(Proposed default, research-backed — not
  locked; see §7-#3.)*
  Sources: [NVML overhead thread](https://forums.developer.nvidia.com/t/nvml-overhead/70480),
  [utilization sampling period](https://forums.developer.nvidia.com/t/sampling-period-nvmldevicegetutilizationrates/51986),
  [cudaMemGetInfo vs nvmlDeviceGetMemoryInfo](https://forums.developer.nvidia.com/t/cudamemgetinfo-vs-nvmldevicegetmemoryinfo/320791),
  [Tomcat JDBC pool](https://tomcat.apache.org/tomcat-8.5-doc/jdbc-pool.html).

#### (c) The vendor-abstraction boundary (#0) — two independent seams

Research confirms NVIDIA must not leak into the policy/grammar layer, and identifies **two separate
seams** (detailed design in §3.5):

- **Seam A — compute backend** ("where the math runs"). candle today reaches **NVIDIA (CUDA) + Apple
  (Metal) + CPU only**; AMD/Intel discrete are **not** reachable at the candle layer. The ONNX Runtime
  path adds **AMD (ROCm/MIGraphX/DirectML), Intel (OpenVINO/DirectML), cross-vendor Windows
  (DirectML)**. So vendor breadth is an **ONNX-path / device-seam concern, not a candle concern**.
- **Seam B — `DeviceProbe`** ("what hardware is here + how much headroom"). **Inherently
  vendor-specific:** NVML (`nvml-wrapper`) now; planned **AMD `rocm_smi_lib`**, **Intel Level-Zero
  Sysman**. No single Rust crate gives cross-vendor *enumerate + VRAM + display-detect* — notably
  **wgpu can enumerate adapters across vendors but `AdapterInfo` has NO VRAM field** (and its
  `device_type` discrete/integrated hint is unreliable), so wgpu is at best an enumeration-only
  fallback.
- The two seams are **independent** (a device can be probe-visible without being compute-reachable,
  and vice-versa). The policy layer must consume only a **normalized** struct
  (`{vendor, device_type, total_vram, free_vram, has_display}`) from Seam B and a capability flag
  (`backend ∈ {Cpu, Cuda, Metal, Onnx(ep)}`) from Seam A — **never raw NVML/CUDA handles**.
  Sources: [candle README](https://github.com/huggingface/candle),
  [ONNX ROCm EP](https://onnxruntime.ai/docs/execution-providers/ROCm-ExecutionProvider.html),
  [ONNX DirectML EP](https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html),
  [wgpu AdapterInfo](https://docs.rs/wgpu/latest/wgpu/struct.AdapterInfo.html),
  [rocm_smi_lib](https://docs.rs/rocm_smi_lib/latest/rocm_smi_lib/),
  [Level-Zero Sysman](https://oneapi-src.github.io/level-zero-spec/level-zero/latest/sysman/PROG.html).

---

## 3. The mechanism matrix (HITL's options)

Each option below is scored against the four canonical cases: **3090-busy** (a vLLM tenant owns it),
**Quadro-display** (must never be grabbed), **integrated GPU** (weak, shared RAM), **no GPU**.

### 3.1 Static config / user pre-identification

User declares intent ahead of time. Surface: device **pin** (`cuda:N`, ideally also **by UUID**),
an explicit **exclude-list** (never use these — the display GPU), an optional **VRAM budget/fraction
cap**, and enable/disable.

- **Pros:** zero runtime probing; deterministic; matches the existing env-var seam; UUID pin fixes the
  reorder gap; exclude-list is the *direct* answer to Quadro-display; budget cap is the *direct*
  answer to 3090-busy co-existence. Pure, testable, no NVML dependency required for pin/exclude.
  **This is the Tier-1 surface** (pin + index exclude-list).
- **Cons:** the user must **know their topology** (which 3090 is spare, the Quadro's index/UUID).
  Wrong on a userland machine that has no GPU → must still fall back. Static pin to a busy 3090 still
  OOMs unless paired with a budget cap **and** the cap is actually honored by the backend (candle:
  advisory only — §2.8).
- **Cases:** *3090-busy* → safe **iff** paired with a budget cap or the user pins the *other* 3090;
  *Quadro-display* → safe via exclude-list / not-pinning it; *integrated* → user can pin it but it's
  weak (works, slow); *no GPU* → user must not enable, or relies on fallback (§3.3).

### 3.2 Dynamic allocation

FathomDB probes the machine (NVML) at `Engine::open` and **auto-picks** an eligible GPU: enumerate →
exclude display-active devices → filter by **free-VRAM threshold** and **utilization ceiling** →
choose the least-loaded survivor; **refuse** (fall back) if none qualify. **Tier 2.**

- **Pros:** best "just works" UX — the user flips on GPU rerank and FathomDB finds the spare 3090,
  skips the saturated one, and never touches the Quadro. Directly closes gaps #1/#2/#3. Adapts as load
  changes between runs.
- **Cons:** needs an **NVML dependency** (or shelling `nvidia-smi`), NVIDIA-only (so it must sit
  behind the #0 vendor-neutral probe trait — §3.5; Metal/integrated/AMD/Intel need their own probe or
  are treated as non-eligible). Probe is a **point-in-time** snapshot — a GPU free at `open` can be
  claimed by vLLM a second later (TOCTOU); the budget cap / alloc-site handling is still the durable
  guard (§2.8). More moving parts to test (mock the probe trait).
- **Cases:** *3090-busy* → **handled** (skipped by util/free-mem filter); *Quadro-display* →
  **handled** (display-active exclusion); *integrated* → eligible only if it clears the free-mem
  floor, else fall back; *no GPU* → probe reports nothing → clean fall back.

### 3.3 CPU fallback — three flavors

The *terminal* behavior when GPU is unavailable / ineligible / fails. Default for a **library** must
be **safe + observable, never surprising**.

| Flavor | Behavior | When appropriate |
| --- | --- | --- |
| **Silent CPU fallback** | drop to CPU, no signal | **Rejected as default** — the §1.4-#6 invisibility trap; a host agent silently runs 100× slower and never knows. Acceptable only if explicitly opted into (`fallback=silent`). |
| **CPU fallback + notification** | drop to CPU **and** emit an **observable, in-band** signal (typed telemetry/log event *and* the existing loud stderr) | **Recommended default (Tier 1).** Safe (never destabilizes the host), and the host/operator *can* see it happened and why. |
| **CPU fallback + HITL gate** | refuse to proceed on GPU-unavailable; require explicit human/host acknowledgement (or a `require_gpu=true` → hard error) | For users who **must not** silently pay CPU latency (a batch re-embed where CPU would take 27 h). Expressed as a **strict mode** (per-engine — §2.8b), not the default. |

- **Pros (notification default):** the host agent gets a typed event it can surface or act on; aligns
  with the existing loud-stderr philosophy but fixes its library-invisibility; never endangers the
  host.
- **Cons:** requires a small **typed signal** surface (event/return flag) beyond stderr — modest new
  API. HITL-gate flavor needs a way to express "fail instead of fall back" (`require_gpu`).
- **Cases:** all four cases ultimately land here when GPU is out; the *notification* makes "why am I on
  CPU?" answerable on every machine (no GPU, all-busy, display-only, feature-not-compiled).

### 3.5 Vendor abstraction — the `DeviceProbe` / `GpuBackend` boundary (#0, v2)

HITL #0: **target NVIDIA now, but never NVIDIA-only.** Research (§2.8c) shows the clean design is
**two independent seams**, so NVML/CUDA assumptions never leak into the grammar/policy layer:

```text
        +-------------------------- policy / grammar layer --------------------------+
        |  parse_device_request . GpuPolicy . eligibility filter . fallback decision  |
        |  consumes ONLY normalized types -- NEVER raw NVML/CUDA handles              |
        +-------------------^------------------------------------^--------------------+
                            | GpuInfo{vendor, device_type,       | Backend{Cpu|Cuda|Metal|Onnx(ep)}
                            |   total_vram, free_vram,           |  + capability flags
                            |   has_display}                     |
   +------------------------+-----------+        +---------------+-------------------------+
   |  SEAM B -- DeviceProbe (probe)     |        |  SEAM A -- ComputeBackend (run math)    |
   |  trait DeviceProbe {               |        |  candle: Cpu | Cuda(NVIDIA) | Metal     |
   |    fn enumerate() -> Vec<GpuInfo>; |        |  (Apple). ONNX later: + ROCm/DirectML/  |
   |  }                                 |        |  OpenVINO/CoreML -> AMD/Intel/Windows   |
   |  impls:                            |        +-----------------------------------------+
   |   NvmlProbe       (NVIDIA, now)    |
   |   RocmSmiProbe    (AMD, planned)   |   the two seams are INDEPENDENT: a device can be
   |   LevelZeroProbe  (Intel, planned) |   probe-visible without being compute-reachable
   |   WgpuEnumProbe   (enum only,      |   (and vice-versa)
   |                    no VRAM, fallbk)|
   +------------------------------------+
```

- **Seam B — `DeviceProbe`** answers "what hardware is here + headroom." Returns a vendor-neutral
  `Vec<GpuInfo>`. The NVIDIA impl (NVML via `nvml-wrapper`, `nvidia-smi` shell-out fallback) is built
  first (#1); **AMD (`rocm_smi_lib`) and Intel (Level-Zero Sysman) are planned impls** behind the same
  trait. A `WgpuEnumProbe` can back a thin cross-vendor *enumeration-only* fallback (no VRAM — wgpu
  has no memory field). Display-detection + VRAM are vendor-specific and live **inside** the impls.
- **Seam A — `ComputeBackend`** answers "where the math runs." candle = NVIDIA+Apple+CPU; ONNX EP =
  the cross-vendor breadth path. Vendor breadth is therefore a **Seam-A/ONNX concern, not candle**.
- **The boundary rule:** the policy layer consumes only `GpuInfo` + a `Backend` capability flag, never
  NVML/CUDA ordinals or handles directly. This is what lets AMD/Intel slot in later without touching
  the grammar (`device.rs` parser) or the `Embedder` trait. **Tier 1 ships none of this** — it is the
  Tier-2 abstraction that keeps the MVP from baking NVIDIA in (the Tier-1 exclude-list is a pure index
  compare, vendor-agnostic by construction).

---

## 4. Recommendation

**A two-tier, layered policy: Tier-1 static declaration + typed-notification fallback is the MVP that
folds into 0.8.14; Tier-2 dynamic probing (behind the #0 vendor-neutral trait), budget cap, and
strict-mode are the full-release surface.** Concretely:

### 4.1 Default policy (unchanged spirit, safer floor)

1. **Default stays CPU.** GPU is opt-in via feature + env/config. (No regression to the
   function-over-footprint posture.)
2. **`auto` is never implied** *(HITL #2 ENDORSED)*. A `*-cuda` build with no explicit device stays
   **CPU**; `auto` is an explicit opt-in keyword (Tier 2). Preserves "default CPU."
3. **When GPU is requested, default fallback = CPU-fallback-with-notification** (§3.3 middle row):
   loud stderr **plus** a typed, in-band event so an embedding library inside a host agent can observe
   it. Silent fallback is opt-in only; HITL-gate (`require_gpu`/strict) is opt-in **per-engine** mode.
4. **Never auto-select the display GPU.** Even in the simplest config, a display-active device is
   ineligible for **auto** selection (it can still be force-pinned by explicit UUID/index, with a
   warning — the user's machine, the user's call). *(Tier 1: user-declared index exclude; Tier 2:
   NVML display-active detection.)*
5. **Honor `CUDA_VISIBLE_DEVICES`.** FathomDB selects *within* the visible set; the operator's mask is
   the cheapest, most-portable exclude lever and every surveyed tool respects it.
6. **Never trust a configured/cached device identity** *(HITL #2 addition (a))*. The data dir / config
   may be **copied to a different machine** (or GPUs reordered) where the configured `cuda:N` no longer
   exists or is now the display GPU. At **engine startup**, re-validate the configured device's
   eligibility and fall back per policy if it is now absent/ineligible. This is a **one-time** check
   (not per-call — §2.8b). *(Tier 2; Tier-1's index exclude is the cheap partial form.)*
7. **`doctor` / admin system-review** *(HITL #2 addition (b))*. A system-review that enumerates and
   reports GPU availability + eligibility: which devices are visible, which are excluded/display-active,
   free VRAM where probeable, which `*-cuda`/`*-metal` features are compiled in, and what FathomDB
   *would* pick. The user-facing answer to "why am I on CPU?" Surface is **a `doctor`/admin command —
   or, per HITL, housed in the existing 0.8.8 EXP-OBS telemetry/explain sidecar** rather than a new
   CLI subcommand. *(Tier 2 — needs the probe trait.)*

### 4.2 Config surface (proposed concrete names)

Env vars (mirroring the existing `FATHOMDB_{EMBED,RERANK}_DEVICE` pair; both families get the same
knobs, parsed by the same shared module so they cannot drift). The **Tier** column marks what lands
when:

| Knob | Env (embed / rerank) | Values | Meaning | Tier |
| --- | --- | --- | --- | --- |
| Device pin | `FATHOMDB_{EMBED,RERANK}_DEVICE` (exists) | `cpu`\|`cuda`\|`cuda:N`\|`metal` | as today | **1** |
| Device pin (extended) | same | + **`auto`** \| **`cuda:uuid:<UUID>`** | `auto` (probe + pick); UUID pin (stable across reorder) | 2 |
| Exclude-list | `FATHOMDB_GPU_EXCLUDE` | comma list of `N` (Tier 1) / `N`+UUIDs (Tier 2) | never use these (the display Quadro). **Tier 1 = static user-declared index compare, no probe**; Tier 2 extends to `auto` + UUIDs + NVML display-active | **1** (index) / 2 |
| Fallback mode | `FATHOMDB_GPU_FALLBACK` | `notify` (default) \| `silent` \| `strict` | `strict` = `require_gpu`: hard typed error instead of CPU fallback (**per-engine** — §2.8b) | 1 (notify default) / 2 (`strict`/`silent` selector) |
| Memory budget | `FATHOMDB_GPU_MEM_FRACTION` / `FATHOMDB_GPU_MEM_LIMIT_MIB` | `0.0–1.0` / MiB | cap per-process VRAM. **candle = advisory** (pre-load free-VRAM eligibility gate via cudarc `mem_get_info`, §2.8a); **ONNX = hard** (`gpu_mem_limit`) | 2 |
| Eligibility floor | `FATHOMDB_GPU_MIN_FREE_MIB` | MiB | a device must report ≥ this free VRAM to be eligible for `auto` (default e.g. model-size + margin) | 2 |

These map 1:1 to a `GpuPolicy` config struct (so non-env callers — the `EmbedderChoice::Caller` path —
set the same fields programmatically). The struct is the source of truth; env is one populator. Define
it so the 0.8.16 ONNX `resolve_device` extension **inherits the same struct** rather than forking one.

### 4.3 Probing / eligibility algorithm (Tier 2 — `auto` and refusing a bad pin)

```text
resolve_device(policy, probe: &dyn DeviceProbe):   # probe = the #0 vendor-neutral trait
  req = parse(policy.device)            # existing pure parser, + `auto` + uuid form
  if req == Cpu: return Cpu
  if req == Metal: return Metal-or-loud-fallback   # unchanged
  # CUDA path:
  visible = probe.enumerate()           # respects CUDA_VISIBLE_DEVICES; vendor-neutral GpuInfo
  if visible empty: return fallback("no eligible devices", policy)
  if req is explicit pin (cuda:N / uuid):
     d = resolve_pin(req, visible)
     if d absent (copied machine / reorder): return fallback("pinned device not present", policy)  # #2(a)
     if d.has_display: WARN (honor pin anyway -- explicit user intent)
     if d.free_vram < min_free: return fallback("pinned device under floor", policy)
     return claim(d, policy.mem_budget)
  if req == auto:
     elig = visible
            - excluded(policy.exclude)
            - { d : d.has_display }                 # never auto-grab the desktop GPU
            - { d : d.free_vram < min_free }         # skip the busy 3090
            - { d : util(d) > util_ceiling }         # optional
     if elig empty: return fallback("no eligible GPU", policy)
     return claim(pick_most_free(elig), policy.mem_budget)

fallback(reason, policy):
  emit typed event{reason, requested, chosen:CPU}   # in-band, observable  <- Tier 1 ships THIS
  loud_stderr(reason)                               # existing behavior
  if policy.fallback == strict: return Err(GpuUnavailable{reason})
  return Cpu
```

`claim` applies the memory budget **where the backend supports it**: ONNX EP `gpu_mem_limit` is a
**hard** arena cap; on candle the budget is **advisory** — enforced as the pre-load `min_free`
eligibility precondition (cudarc `mem_get_info`), since candle exposes **no per-process cap** (§2.8a).
The probe is **point-in-time**; the `min_free` margin reduces but cannot eliminate the TOCTOU window —
the authoritative guard remains the allocation attempt itself, handled at the call site (§2.8b). Note
the **only validation per call is none** — eligibility is resolved once at open + the #2(a) startup
re-check; nothing re-probes per `embed()`.

### 4.4 Tier-1 MVP — exact scope (folds into 0.8.14)

Restating §0.5 Tier 1 as the build order for the GPU-rerank slice — **pure, NVML-free,
unit-testable**:

1. **`FATHOMDB_GPU_EXCLUDE`** parsing (comma list of CUDA indices) + the refuse-and-fall-back rule
   when a pinned / `cuda`-defaulted index is in the set. *(Static string compare — no probe.)*
2. **Typed, in-band fallback event** (`{reason, requested, chosen:Cpu}`) emitted on every fallback
   path, alongside the existing loud stderr. Closes gap #6 — the single highest-value safety item.
3. **Honor `CUDA_VISIBLE_DEVICES`** explicitly + document it as the first-line exclude lever.
4. *(Already shipped, no change:)* the `device.rs` pin grammar + per-backend `resolve_device` +
   loud-CPU fallback.

**Not in Tier 1:** NVML/`nvidia-smi`, `auto`, UUID pin, the `DeviceProbe` trait, memory budget,
startup re-check, `doctor`, `strict`/`silent` mode selector. Default fallback behavior is `notify`.

### 4.5 Explicitly out of scope (not now, maybe never)

Multi-GPU sharding / tensor-split (FathomDB's model is tiny and serialized — one device is plenty);
MIG partitioning; a custom capped cudarc allocator (CUDA pools cap by release-threshold, not a strict
ceiling — not worth it when ONNX `gpu_mem_limit` gives a real cap); cross-vendor probing beyond what
the `DeviceProbe` impls + ONNX backend surface.

---

## 5. Cross-cutting fit + roadmap slot

This is **device-seam family** work and rides the existing seam without re-opening the grammar:

- **#3 GPU-seam** — owns `resolve_device`; the eligibility/probe layer slots *between*
  `parse_device_request` and `Device::new_cuda`, leaving the pure parser and the `Embedder` trait
  untouched.
- **#4 ONNX (0.8.16)** — the ONNX backend is where a **real (hard) memory budget** (`gpu_mem_limit`)
  and the **cross-vendor device-id** surface naturally live; the `GpuPolicy` struct should be defined
  so the ONNX `resolve_device` extension consumes the same fields. **Sequence the policy struct so
  0.8.16 inherits it** rather than inventing a parallel one. ONNX is also where the #0 **vendor
  breadth** (AMD/Intel/DirectML) actually arrives (§2.8c).
- **#5 vector-equivalence** — orthogonal but adjacent: an `auto`-selected or budget-capped backend can
  change numerics (CPU↔CUDA↔ONNX), which is exactly what the probe-set equivalence guard
  (`0.8.1-embedder-gpu-and-portability.md` §3) must catch. Auto device-selection **raises the
  priority** of that guard: if FathomDB silently moves which backend embeds, the equivalence check is
  the safety net.

**Roadmap mapping (v2):**

| Work | Slot | Rationale |
| --- | --- | --- |
| **Tier 1 MVP** (exclude-list + typed-notify fallback) | **0.8.14** (GPU-rerank fold-in) | small, NVML-free, safety-critical; the natural companion to the GPU-rerank work / V-3/V-7 campaign. Do **not** bundle NVML here. |
| **Tier 2 dynamic** — `DeviceProbe` trait (#0) + NVML probing (#1) + `auto`/UUID + startup re-check + `doctor`/EXP-OBS (#2) + advisory budget (#4) + per-engine strict (#3) | **with 0.8.16 ONNX** *(HITL-confirmed; no standalone 0.8.18)* — shares the ONNX memory-budget + cross-vendor plumbing | NVML dep + probe trait land alongside the cross-vendor seam |
| **Hard memory budget** (`gpu_mem_limit`) | **0.8.16 ONNX path** | candle cannot hard-cap (§2.8a); the real cap is an ONNX-EP option |
| **Vendor breadth** (AMD ROCm/MIGraphX, Intel OpenVINO, DirectML) + their `DeviceProbe` impls (ROCm-SMI, Level-Zero) | **ONNX path (0.8.16+)** | unreachable at the candle layer; an ONNX-EP + per-vendor-probe concern |

---

## 6. Sources

- PyTorch: [torch.cuda](https://docs.pytorch.org/docs/stable/cuda.html) ·
  [set_per_process_memory_fraction](https://docs.pytorch.org/docs/stable/generated/torch.cuda.memory.set_per_process_memory_fraction.html) ·
  [CUDA semantics](https://docs.pytorch.org/docs/stable/notes/cuda.html)
- vLLM: [Conserving Memory](https://docs.vllm.ai/en/latest/configuration/conserving_memory/) ·
  [specific-GPU assignment #28132](https://github.com/vllm-project/vllm/issues/28132) ·
  [CUDA_VISIBLE_DEVICES #2387](https://github.com/vllm-project/vllm/issues/2387)
- Ollama / llama.cpp: [Ollama GPU docs](https://docs.ollama.com/gpu) ·
  [Ollama multi-GPU notes](https://knightli.com/en/2026/04/19/ollama-multiple-gpu-notes/) ·
  [llama.cpp multi-gpu.md](https://github.com/ggml-org/llama.cpp/blob/master/docs/multi-gpu.md)
- ONNX Runtime: [CUDA EP](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html) ·
  [OrtCUDAProviderOptions](https://onnxruntime.ai/docs/api/c/struct_ort_c_u_d_a_provider_options.html) ·
  [Execution Providers](https://onnxruntime.ai/docs/execution-providers/) ·
  [ROCm EP](https://onnxruntime.ai/docs/execution-providers/ROCm-ExecutionProvider.html) ·
  [DirectML EP](https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html)
- HuggingFace Accelerate: [Big Model Inference](https://huggingface.co/docs/accelerate/usage_guides/big_modeling) ·
  [Loading big models](https://huggingface.co/docs/accelerate/en/concept_guides/big_model_inference)
- NVML: [Device Queries](https://docs.nvidia.com/deploy/nvml-api/group__nvmlDeviceQueries.html) ·
  [pynvml](https://github.com/gpuopenanalytics/pynvml) ·
  [nvml-wrapper (Rust)](https://docs.rs/nvml-wrapper/latest/nvml_wrapper/) ·
  [NVML overhead](https://forums.developer.nvidia.com/t/nvml-overhead/70480) ·
  [utilization sampling period](https://forums.developer.nvidia.com/t/sampling-period-nvmldevicegetutilizationrates/51986) ·
  [cudaMemGetInfo vs nvmlDeviceGetMemoryInfo](https://forums.developer.nvidia.com/t/cudamemgetinfo-vs-nvmldevicegetmemoryinfo/320791)
- candle / cudarc (v2 memory research): [candle](https://github.com/huggingface/candle) ·
  [candle cuda_backend/device.rs](https://github.com/huggingface/candle/blob/main/candle-core/src/cuda_backend/device.rs) ·
  [cudarc result.rs](https://github.com/coreylowman/cudarc/blob/main/src/driver/result.rs) ·
  [cudarc safe/core.rs](https://github.com/coreylowman/cudarc/blob/main/src/driver/safe/core.rs)
- Vendor-neutral probe (v2): [wgpu AdapterInfo](https://docs.rs/wgpu/latest/wgpu/struct.AdapterInfo.html) ·
  [wgpu DeviceType](https://docs.rs/wgpu/latest/wgpu/enum.DeviceType.html) ·
  [rocm_smi_lib](https://docs.rs/rocm_smi_lib/latest/rocm_smi_lib/) ·
  [Level-Zero Sysman](https://oneapi-src.github.io/level-zero-spec/level-zero/latest/sysman/PROG.html)
- Connection-pool validate-once pattern (v2): [Tomcat JDBC pool](https://tomcat.apache.org/tomcat-8.5-doc/jdbc-pool.html) ·
  [Oracle UCP — Validating Connections](https://docs.oracle.com/en/database/oracle/oracle-database/21/jjucp/validating-ucp-connections.html)

---

## 7. Open questions for HITL — status after v2

| # | Question | v2 status |
| --- | --- | --- |
| **0** | **Vendor abstraction (NEW).** Target NVIDIA now but never NVIDIA-only — where is the abstraction boundary? | **RESOLVED into design (§3.5).** Two independent seams: `DeviceProbe` (NVML now; AMD ROCm-SMI / Intel Level-Zero planned; wgpu enum-only fallback) + `ComputeBackend` (candle NVIDIA+Apple; ONNX adds AMD/Intel/DirectML). Policy layer consumes normalized `GpuInfo` only. Tier 2. |
| **1** | **NVML dependency posture.** Optional feature-gated NVML dep + `nvidia-smi` fallback? | **ENDORSED.** Feature-gated `nvml-wrapper` (zero-cost to default build) + `nvidia-smi` shell-out fallback, **behind the #0 probe trait** so NVML is not the only code path. Tier 2. |
| **2** | **`auto` as default-ish.** Should `auto` ever be implied? | **ENDORSED: explicit opt-in only** (preserves default-CPU). **PLUS** HITL added (a) a one-time **startup device-eligibility re-check** (never trust a cached/configured device — config may be copied to another machine / GPUs reordered) and (b) a **`doctor` system-review** enumerating GPU availability + eligibility. Both specified (§4.1-6/7). Tier 2. |
| **3** | **Strict-mode scope** — per-engine vs per-engine-with-per-call-override? | **RESEARCHED → recommend PER-ENGINE** (proposed default, not locked). Per-call NVML adds real, scaling overhead (utilization is window-averaged/stale; `nvidia-smi` ~20% throughput hit) for ~zero benefit — eligibility is static + cached, and free-VRAM probing **cannot** prevent OOM ("No. You cannot… for any OS"); the alloc attempt + budget is the real guard. Mirrors connection-pool validate-once. A per-call override, if any, is opt-in/off-by-default. (§2.8b) |
| **4** | **Memory budget on candle.** Advisory-only acceptable? | **RESEARCHED → CONFIRMED advisory on candle.** candle has **no hard cap / fraction / pool ceiling** (allocate-on-demand via cudarc); it can **read** free VRAM only via cudarc `mem_get_info`. So budget = **advisory pre-load eligibility gate** on candle; **hard `gpu_mem_limit` arrives on the ONNX path**. (§2.8a) |

**Residual for HITL — resolved 2026-06-30:**

1. **#3 stays proposed, not locked** *(HITL OK)*. Per-engine strict remains a *research-backed
   recommendation*, not yet a contract. Still open within that: whether the **batch re-embed** path
   should default to `strict` while interactive rerank stays `notify` (revisit when the Tier-2 layer
   is built).
2. **Tier-2 slot = 0.8.16** *(HITL OK)*. The dynamic/NVML layer rides **0.8.16 (with ONNX)**; the
   standalone-0.8.18 alternative is dropped.
3. **`doctor` surface** *(HITL OK)*. Treat as a **`doctor`/admin surface**; **may be housed in the
   existing 0.8.8 EXP-OBS telemetry/explain sidecar** rather than a new CLI subcommand. Final
   placement decided when #2 is built.
