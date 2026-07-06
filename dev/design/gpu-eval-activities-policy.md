# Policy — the repo MUST use the 3090 GPUs for eval/embed activities when there is room

> **Standing HITL mandate (2026-07-05). This is a repo MUST, not a suggestion.**

## Rule

Any repo-internal, GPU-acceleratable activity — the **eu7 fidelity harness** (corpus seed / re-embed),
**corpus re-embeds**, **eval sweeps**, **CE reranking in eval**, and similar embedding/rerank-heavy dev/CI
work — **MUST run on the 3090s (`cuda:0` / `cuda:1`).** Full eval/embed/rerank **workloads and suites always
run on GPU.** See [CPU is only for compatibility probes](#cpu-is-only-for-compatibility-probes) for the two
narrow exceptions — CPU is **not** a general fallback for real workloads.

## CPU is only for compatibility probes

CPU is permitted for a GPU-able activity in **exactly two** narrow cases, both **small and targeted**:

1. **Simple CPU-compatibility testing** — a minimal check that the CPU code path still builds and runs (the
   default-CPU library works).
2. **CPU-library repeatability / compatibility checks** — a targeted check that the CPU libraries produce
   repeatable / compatible results.

**"Feature compatibility" means *check compatibility with the CPU libraries* — NOT *run the full suite on both
CPU and GPU*.** Never mirror a full suite onto CPU; full runs are GPU-only. When the 3090s are momentarily
busy, **wait for room or use the other 3090** — do not fall back to a full CPU run.

**Fidelity/recall gates are a case-(2) repeatability check and MUST run same-backend as their baseline.** The
**eu7 ANN-fidelity gate**'s canonical baseline is a **CPU** measurement (0.896, ci_hi 0.925 PASS). Running it
cross-backend on GPU introduces near-threshold 1-bit-quantization flips that thin large-N margins expose — a
GPU N=7667 run read **0.833** (ci_hi 0.864) vs the CPU **0.896**: a *measurement artifact, not a regression*.
So **run the eu7 gate (and any recall-fidelity gate) on CPU**, same-backend as its baseline — this is the
permitted case-(2) repeatability check, **not** a mandate violation. GPU accelerates eval **throughput**
(corpus prep, sweeps, ad-hoc re-embeds); it does **not** run the fidelity-gate *measurement*.

## Why

- The 3090s are **15–93× faster** than CPU for BGE embedding (0.8.7 GPU embedder). A fresh 0.8.14 eu7 run
  crawled on the CPU embedder (~0.43 docs/sec, seed-timeout-prone) with **both 3090s at 0% idle** — pure
  waste, and the direct cause of the eu7 seed-drain timeout.
- **Throughput, not fidelity-measurement.** GPU embedding is for eval *throughput* (corpus prep, re-embeds,
  sweeps). CPU↔CUDA embeddings are *near*-identical (0.8.7: 1-bit-identical on a 6144-bit probe, max |Δ|
  1.6e-7) — close enough for throughput work, but at large N the near-threshold quant flips **do** shift a
  sensitive recall-fidelity measurement. So GPU-accelerate the embedding *work*, but run recall-**fidelity
  gates** (eu7) on CPU, same-backend as their baseline (see above). **Correction (2026-07-05):** an earlier
  version of this doc claimed GPU "does not change the eu7 fidelity result" — that was too strong; a GPU eu7
  run read 0.833 vs the CPU 0.896 baseline.

## How

- Build with the **`embed-cuda`** feature (and **`rerank-cuda`** for reranking); set
  **`FATHOMDB_EMBED_DEVICE=cuda:0`** (and `FATHOMDB_RERANK_DEVICE=cuda:0`) — a 3090.
- **Exclude the K620 display GPU** (`FATHOMDB_GPU_EXCLUDE=<its index>`) — display-only, never allocate.
- **Check for room first** (`nvidia-smi` utilization + free memory) before dispatching; do not oversubscribe
  a busy 3090 — when both 3090s are busy, **wait/queue or use the other 3090**. Do NOT fall back to a full
  CPU run (CPU is reserved for the two compatibility probes above).
- **Verify the GPU actually engaged** — `nvidia-smi` should show `>0%` util + memory on the chosen device.
  Do not assume the feature/env took.

## Scope

This governs the **repo's internal eval / dev / CI activities**. It is DISTINCT from — and does not change —
the **shipped library's** default device stance (default CPU, GPU opt-in, per the footprint invariant) or
the shipped GPU device-allocation design (`gpu-device-allocation-policy.md`). Those are separate.

## Enforcement (fix the tooling, not per-run reminders)

The eu7 / eval harness and eval runners **MUST default to GPU** for full runs (never a CPU full-suite; CPU is
reserved for the two compatibility probes above), so no agent has to remember the flags. Tracked as **`TC-4`** in `dev/todos-and-considerations-ledger.jsonl`
(the GPU-default enforcement item). Until that tooling default lands, every eval/eu7 invocation passes the
`embed-cuda` + `FATHOMDB_EMBED_DEVICE=cuda:0` env/flags explicitly.
