# GPU device-allocation policy — design-on-spec (embedder + CE reranker)

> **DESIGN-ON-SPEC — NOT APPROVED. NO engine code changes.**
>
> This document is a *contingent* design produced so that, **if** HITL elects to harden FathomDB's
> GPU device handling beyond today's raw device-string knob, there is a grounded, source-anchored
> plan ready. It is **not** a build order. The current device seam (`FATHOMDB_EMBED_DEVICE`,
> `FATHOMDB_RERANK_DEVICE`, the `embed-cuda`/`rerank-cuda` features, the loud-CPU-fallback) ships as
> designed; nothing here re-opens it. This proposes the *next* layer — a safe-by-default allocation
> policy so a user who flips on GPU CE-rerank cannot OOM a busy GPU, fight a co-resident LLM server,
> or grab the display GPU and hang the desktop. The load-bearing sections are §1 (what exists today)
> and §4 (the recommendation + MVP/later split).
>
> **Date:** 2026-06-30 · **Owner:** program steward · **Roadmap fit:** device-seam family
> (#3 GPU-seam / #4 ONNX / #5 vector-equivalence); see §5.

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

A user flips on GPU CE-rerank expecting it to "just work." The design must make that **safe across the
whole spectrum**. HITL named the mechanisms to evaluate and choose among: **config settings ·
dynamic GPU allocation · user pre-identifies which GPU / how much GPU · CPU fallback · CPU
fallback + HITL · CPU fallback + notification.**

This sits squarely on FathomDB's posture: **function over footprint / user-controlled spend** — GPU
is an **opt-in knob, default CPU**, and turning it on must **never destabilize the host**.

---

## 1. Current-state survey — what FathomDB does today

Source: `0.8.12-gpu-rerank` branch, shared crate
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

- candle 0.10 supports **only** CPU / CUDA / Metal — **no ROCm, no Vulkan, no DirectML**. AMD/Intel
  discrete + integrated GPUs are **unreachable through candle's `Device`** (the durable cross-vendor
  path is a new `impl Embedder` behind the trait, e.g. ONNX Runtime — see
  `0.8.1-embedder-gpu-and-portability.md` §2, and §1.4 below).
- The model is **serialized** (PR-9 guard: one `embed()` at a time), so single-CUDA-stream is the
  intended behavior, not a limitation — it prevents concurrent CUDA calls on the shared model.
- Compute is **F32** on the candle path.
- The 0.8.14 ONNX path will **extend `resolve_device()`** (ONNX Runtime CUDA EP carries its own
  device-id + memory-limit options — see §3.4), which is the natural insertion point for an
  allocation policy.

### 1.4 Gaps vs the problem

What today's seam **does not** do — every one of these is a way the HITL scenario bites:

1. **No free-memory / utilization probe.** `Device::new_cuda(idx)` succeeds as long as the device
   *exists* and a context can be created; it does **not** check whether a 3090 is already saturated
   by vLLM. The CUDA context is created fine; the **OOM arrives later**, at model-load or first
   forward, as a hard failure — *after* the loud-fallback decision has already been made. The
   construction-time fallback cannot catch a *runtime* OOM.
2. **No display-GPU exclusion.** `cuda:0` is a positional index. If the Quadro is enumerated at the
   index the user (or a default) names, FathomDB will happily try to allocate on the display GPU.
   There is no notion of "this device drives the desktop, never auto-pick it."
3. **No auto-select.** The user must name an exact index. There is no "pick the least-loaded eligible
   GPU." A user who just wants "use a GPU" has to know their topology.
4. **No memory budget / fraction cap.** FathomDB takes whatever candle's allocator grabs. There is no
   "use at most 2 GiB / at most 20%," so even a *successful* claim on a shared 3090 can starve the
   co-resident vLLM server.
5. **Index instability.** `cuda:N` is positional and **reorders** across reboots / driver changes /
   `CUDA_VISIBLE_DEVICES` (NVML and Ollama both warn on this — §2). There is no UUID pin, so a pin
   that meant "the spare 3090" can silently become "the display Quadro."
6. **Fallback is loud but not *observable in-band*.** The warning is stderr only — fine for a CLI,
   invisible to a library embedder running inside a host agent (Memex/Hermes) that never reads the
   child's stderr. There is no typed signal / telemetry event a host can react to.

These are exactly the failure modes §4 must close. Note the seam is **well-shaped for the fix**: the
parse is pure and the request→device mapping is already the single chokepoint, so an eligibility +
probe layer slots in **between** `parse_device_request` and `Device::new_cuda` without touching the
grammar or the trait.

---

## 2. Web research — how other systems manage GPU selection / limits / fallback

Synthesized below; per-mechanism citations inline, full list in §6. The recurring **patterns** are
called out in §2.7.

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

### 2.4 ONNX Runtime (CUDA EP) — directly relevant to the 0.8.14 path

The CUDA execution provider takes a provider-options struct:

- `device_id` — which GPU.
- `gpu_mem_limit` — **hard byte cap** on the EP's memory arena (the closest thing to a real budget
  reservation in this survey; "total device memory usage may be higher" but the arena is bounded).
- `arena_extend_strategy` — `kNextPowerOfTwo` vs `kSameAsRequested` (growth aggressiveness).
- `cudnn_conv_algo_search` — exhaustive search allocates a scratch workspace; can be narrowed to cut
  memory.

Because FathomDB's ONNX backend will be a new `impl Embedder` that constructs its own session, these
options are exactly where a **per-device memory budget** lands for the cross-vendor path (ONNX EP also
covers ROCm/DirectML/OpenVINO/CoreML — the reason it is the durable cross-vendor seam).
Sources: [CUDA EP docs](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html),
[OrtCUDAProviderOptions](https://onnxruntime.ai/docs/api/c/struct_ort_c_u_d_a_provider_options.html).

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

NVML (the library behind `nvidia-smi`; Python via `pynvml`) is how every auto-placer above actually
learns the machine. Relevant queries:

- `nvmlDeviceGetMemoryInfo(handle)` → total / free / used VRAM per device (the free-memory probe).
- `nvmlDeviceGetUtilizationRates` / `--query-gpu=utilization.gpu,utilization.memory,memory.free` →
  current load (distinguish "exists" from "busy").
- `nvmlDeviceGetUUID` → the **stable identifier** for a device across reorder (what Ollama recommends
  pinning to).
- **Display detection:** `nvmlDeviceGetDisplayActive` (is a display/X server initialized on this
  device — can be true with no monitor attached) and `nvmlDeviceGetDisplayMode` (is a physical monitor
  on a connector). These are the primitives to **exclude the Quadro that drives the desktop**.
- `nvmlDeviceGetComputeMode` → whether the GPU permits shared / exclusive compute contexts.
  Sources: [NVML Device Queries](https://docs.nvidia.com/deploy/nvml-api/group__nvmlDeviceQueries.html),
  [pynvml](https://github.com/gpuopenanalytics/pynvml).

### 2.7 The patterns (what to steal)

| Pattern | Who | FathomDB relevance |
| --- | --- | --- |
| **Visibility mask** (`CUDA_VISIBLE_DEVICES`, UUIDs > indices) | PyTorch, vLLM, Ollama, llama.cpp | Cheapest exclude-the-display-GPU lever; **respect it, don't fight it** |
| **Explicit pin** (`device_id`, `--main-gpu`, `cuda:N`) | all | FathomDB already has this (`cuda:N`); add **UUID pin** for stability |
| **Memory-fraction / byte budget** (`gpu-memory-utilization`, `set_per_process_memory_fraction`, `gpu_mem_limit`, `max_memory`) | vLLM, PyTorch, ONNX, Accelerate | The lever that lets FathomDB **co-exist** with a busy 3090 instead of evicting it |
| **Free-memory-aware auto-placement** (probe then place) | Ollama, Accelerate | The "just pick a good GPU" UX; needs NVML |
| **Probe-before-claim** (`--list-devices` w/ memory; NVML mem/util) | llama.cpp, NVML | Turns a *runtime* OOM into a *construction-time* refusal |
| **Display / compute-mode awareness** | NVML | The only principled way to **never grab the desktop GPU** |
| **Auto with safe fallback ladder** (GPU → CPU → disk) | Accelerate, Ollama | Maps to FathomDB's CPU fallback, but FathomDB's default is the *reverse* (CPU unless asked) |

Key contrast: every tool above is **GPU-first** (it's an inference server whose job is the GPU).
FathomDB is a **library embedded in someone else's process** whose default is **CPU**, so it inherits
the patterns but **inverts the default** and weights **host-safety over throughput**.

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
- **Cons:** the user must **know their topology** (which 3090 is spare, the Quadro's UUID). Wrong on
  a userland machine that has no GPU → must still fall back. Static pin to a busy 3090 still OOMs
  unless paired with a budget cap **and** the cap is actually honored by the backend.
- **Cases:** *3090-busy* → safe **iff** paired with a budget cap or the user pins the *other* 3090;
  *Quadro-display* → safe via exclude-list / not-pinning it; *integrated* → user can pin it but it's
  weak (works, slow); *no GPU* → user must not enable, or relies on fallback (§3.3).

### 3.2 Dynamic allocation

FathomDB probes the machine (NVML) at `Engine::open` and **auto-picks** an eligible GPU: enumerate →
exclude display-active devices → filter by **free-VRAM threshold** and **utilization ceiling** →
choose the least-loaded survivor; **refuse** (fall back) if none qualify.

- **Pros:** best "just works" UX — the user flips on GPU rerank and FathomDB finds the spare 3090,
  skips the saturated one, and never touches the Quadro. Directly closes gaps #1/#2/#3. Adapts as load
  changes between runs.
- **Cons:** needs an **NVML dependency** (or shelling `nvidia-smi`), NVIDIA-only (Metal/integrated need
  their own probe or are treated as non-eligible). Probe is a **point-in-time snapshot** — a GPU free
  at `open` can be claimed by vLLM a second later (TOCTOU); the budget cap is still needed as the
  durable guard. More moving parts to test (mock NVML).
- **Cases:** *3090-busy* → **handled** (skipped by util/free-mem filter); *Quadro-display* →
  **handled** (display-active exclusion); *integrated* → eligible only if it clears the free-mem
  floor, else fall back; *no GPU* → NVML reports nothing → clean fall back.

### 3.3 CPU fallback — three flavors

The *terminal* behavior when GPU is unavailable / ineligible / fails. Default for a **library** must
be **safe + observable, never surprising**.

| Flavor | Behavior | When appropriate |
| --- | --- | --- |
| **Silent CPU fallback** | drop to CPU, no signal | **Rejected as default** — the §1.4-#6 invisibility trap; a host agent silently runs 100× slower and never knows. Acceptable only if explicitly opted into (`fallback=silent`). |
| **CPU fallback + notification** | drop to CPU **and** emit an **observable, in-band** signal (typed telemetry/log event *and* the existing loud stderr) | **Recommended default.** Safe (never destabilizes the host), and the host/operator *can* see it happened and why. |
| **CPU fallback + HITL gate** | refuse to proceed on GPU-unavailable; require explicit human/host acknowledgement (or a `require_gpu=true` → hard error) | For users who **must not** silently pay CPU latency (a batch re-embed where CPU would take 27 h). Expressed as a **strict mode**, not the default. |

- **Pros (notification default):** the host agent gets a typed event it can surface or act on; aligns
  with the existing loud-stderr philosophy but fixes its library-invisibility; never endangers the
  host.
- **Cons:** requires a small **typed signal** surface (event/return flag) beyond stderr — modest new
  API. HITL-gate flavor needs a way to express "fail instead of fall back" (`require_gpu`).
- **Cases:** all four cases ultimately land here when GPU is out; the *notification* makes "why am I on
  CPU?" answerable on every machine (no GPU, all-busy, display-only, feature-not-compiled).

---

## 4. Recommendation

**A layered policy: static declaration is MVP; dynamic probing is the next slice; budget cap and
typed-notification fallback are the safety floor under both.** Concretely:

### 4.1 Default policy (unchanged spirit, safer floor)

1. **Default stays CPU.** GPU is opt-in via feature + env/config. (No regression to the
   function-over-footprint posture.)
2. **When GPU is requested, default fallback = CPU-fallback-with-notification** (§3.3 middle row):
   loud stderr **plus** a typed, in-band event so an embedding library inside a host agent can observe
   it. Silent fallback is opt-in only; HITL-gate (`require_gpu`) is opt-in strict mode.
3. **Never auto-select the display GPU.** Even in the simplest config, a display-active device is
   ineligible for **auto** selection (it can still be force-pinned by explicit UUID/index, with a
   warning — the user's machine, the user's call).
4. **Honor `CUDA_VISIBLE_DEVICES`.** FathomDB selects *within* the visible set; the operator's mask is
   the cheapest, most-portable exclude lever and every surveyed tool respects it.

### 4.2 Config surface (proposed concrete names)

Env vars (mirroring the existing `FATHOMDB_{EMBED,RERANK}_DEVICE` pair; both families get the same
knobs, parsed by the same shared module so they cannot drift):

| Knob | Env (embed / rerank) | Values | Meaning |
| --- | --- | --- | --- |
| Device pin | `FATHOMDB_{EMBED,RERANK}_DEVICE` (exists) | `cpu`\|`cuda`\|`cuda:N`\|`metal`\|**`auto`**\|**`cuda:uuid:<UUID>`** | adds `auto` (probe + pick) and a **UUID pin** (stable across reorder) |
| Exclude-list | `FATHOMDB_GPU_EXCLUDE` | comma list of `N` / UUIDs | never auto-pick these (the display Quadro); applies to `auto` |
| Memory budget | `FATHOMDB_GPU_MEM_FRACTION` / `FATHOMDB_GPU_MEM_LIMIT_MIB` | `0.0–1.0` / MiB | cap per-process VRAM (co-exist with vLLM); enforced where the backend allows (ONNX `gpu_mem_limit`; candle = best-effort + eligibility floor) |
| Eligibility floor | `FATHOMDB_GPU_MIN_FREE_MIB` | MiB | a device must report ≥ this free VRAM to be eligible for `auto` (default e.g. model-size + margin) |
| Fallback mode | `FATHOMDB_GPU_FALLBACK` | `notify` (default) \| `silent` \| `strict` | `strict` = `require_gpu`: hard typed error instead of CPU fallback |

These map 1:1 to a `GpuPolicy` config struct (so non-env callers — the `EmbedderChoice::Caller` path —
set the same fields programmatically). The struct is the source of truth; env is one populator.

### 4.3 Probing / eligibility algorithm (for `auto` and for refusing a bad pin)

```text
resolve_device(policy):
  req = parse(policy.device)            # existing pure parser, + `auto` + uuid form
  if req == Cpu: return Cpu
  if req == Metal: return Metal-or-loud-fallback   # unchanged
  # CUDA path:
  visible = nvml_enumerate()            # already respects CUDA_VISIBLE_DEVICES
  if visible empty: return fallback("no CUDA devices", policy)
  if req is explicit pin (cuda:N / uuid):
     d = resolve_pin(req, visible)
     if d is display-active: WARN (honor pin anyway — explicit user intent)
     if free_vram(d) < min_free: return fallback("pinned device under floor", policy)
     return claim(d, policy.mem_budget)
  if req == auto:
     elig = visible
            - excluded(policy.exclude)
            - display_active                       # never auto-grab the desktop GPU
            - { d : free_vram(d) < min_free }       # skip the busy 3090
            - { d : util(d) > util_ceiling }        # optional
     if elig empty: return fallback("no eligible GPU", policy)
     return claim(pick_most_free(elig), policy.mem_budget)

fallback(reason, policy):
  emit typed event{reason, requested, chosen:CPU}   # in-band, observable
  loud_stderr(reason)                               # existing behavior
  if policy.fallback == strict: return Err(GpuUnavailable{reason})
  return Cpu
```

`claim` applies the memory budget where the backend supports it (ONNX EP `gpu_mem_limit`; on candle,
the budget is advisory — enforced as an eligibility precondition rather than a hard allocator cap,
since candle exposes no per-process fraction). The probe is **point-in-time**; the memory budget +
`min_free` margin are what keep a *successful* claim from starving a co-resident tenant between probe
and load.

### 4.4 MVP vs later

**MVP (static + safety floor — no NVML hard dependency):**

- `auto` keyword and **UUID pin** in the shared parser (pure, unit-testable).
- `FATHOMDB_GPU_EXCLUDE` + `FATHOMDB_GPU_FALLBACK={notify,silent,strict}` + the `GpuPolicy` struct.
- **Typed, in-band fallback notification** (closes gap #6) — the single highest-value safety item;
  works on every machine with no NVML.
- Honor `CUDA_VISIBLE_DEVICES` explicitly + document it as the first-line exclude lever.

**Later (dynamic — NVML-backed):**

- NVML probe (`nvmlDeviceGetMemoryInfo`, `…Utilization`, `…DisplayActive`, `…GetUUID`) behind a thin
  feature/optional-dep so the default build keeps zero new deps; shell-out to `nvidia-smi` as the
  dependency-free fallback probe.
- `auto` selection (eligibility filter + most-free pick) and display-GPU auto-exclusion.
- Memory-budget enforcement: real on the ONNX path (`gpu_mem_limit`, lands with 0.8.14); advisory on
  candle.
- Optional utilization ceiling.

**Explicitly out of scope (not now, maybe never):** multi-GPU sharding / tensor-split (FathomDB's
model is tiny and serialized — one device is plenty); MIG partitioning; cross-vendor probing beyond
what the ONNX backend surfaces.

---

## 5. Cross-cutting fit + roadmap slot

This is **device-seam family** work and rides the existing seam without re-opening the grammar:

- **#3 GPU-seam** — owns `resolve_device`; the eligibility/probe layer slots *between*
  `parse_device_request` and `Device::new_cuda`, leaving the pure parser and the `Embedder` trait
  untouched.
- **#4 ONNX (0.8.14)** — the ONNX backend is where a **real memory budget** (`gpu_mem_limit`) and the
  cross-vendor device-id surface naturally live; the `GpuPolicy` struct should be defined so the ONNX
  `resolve_device` extension consumes the same fields. **Sequence the policy struct so 0.8.14 inherits
  it** rather than inventing a parallel one.
- **#5 vector-equivalence** — orthogonal but adjacent: an `auto`-selected or budget-capped backend can
  change numerics (CPU↔CUDA↔ONNX), which is exactly what the probe-set equivalence guard
  (`0.8.1-embedder-gpu-and-portability.md` §3) must catch. Auto device-selection **raises the
  priority** of that guard: if FathomDB silently moves which backend embeds, the equivalence check is
  the safety net.

**Recommended roadmap slot.** The **MVP** (static config + typed-notification fallback) is small,
NVML-free, and safety-critical — it is the natural companion to **0.8.12 EXP-S** (which now also
carries the GPU-rerank fold-in), so the fallback/notification + `GpuPolicy` struct + `auto`/UUID
grammar should land **with the GPU-rerank work in 0.8.12**. The **dynamic NVML layer** is its own
device-seam roadmap item — recommend a dedicated **`0.8.14`-adjacent slice (alongside / just after
the ONNX path)** so it shares the ONNX memory-budget plumbing, or a standalone **0.8.16 "GPU
allocation policy"** item if 0.8.14 is already full. Do **not** bundle the NVML dependency into
0.8.12.

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
  [OrtCUDAProviderOptions](https://onnxruntime.ai/docs/api/c/struct_ort_c_u_d_a_provider_options.html)
- HuggingFace Accelerate: [Big Model Inference](https://huggingface.co/docs/accelerate/usage_guides/big_modeling) ·
  [Loading big models](https://huggingface.co/docs/accelerate/en/concept_guides/big_model_inference)
- NVML: [Device Queries](https://docs.nvidia.com/deploy/nvml-api/group__nvmlDeviceQueries.html) ·
  [pynvml](https://github.com/gpuopenanalytics/pynvml)

---

## 7. Open questions for HITL

1. **NVML dependency posture.** Acceptable to add an **optional** NVML dep (feature-gated, zero-cost to
   the default build) for the dynamic layer, or must dynamic probing shell out to `nvidia-smi` to keep
   the dependency tree clean?
2. **`auto` as a default-ish.** Should `auto` ever be implied when a `*-cuda` build runs with no
   explicit device, or must GPU always be *explicitly* named (current posture) and `auto` be an
   explicit opt-in? (Recommendation: explicit opt-in — preserves "default CPU.")
3. **Strict mode scope.** Is `require_gpu`/`strict` a per-call or per-engine setting, and should it be
   the default for the **batch re-embed** path (where silent CPU = 27 h) even while interactive rerank
   stays `notify`?
4. **Memory budget on candle.** Advisory-only (eligibility precondition) is accepted as the candle
   limitation, with the real cap arriving on the ONNX path — confirm that's acceptable rather than
   blocking GPU rerank on a hard cap.
