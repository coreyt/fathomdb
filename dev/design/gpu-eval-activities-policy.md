# Policy — the repo MUST use the 3090 GPUs for eval/embed activities when there is room

> **Standing HITL mandate (2026-07-05). This is a repo MUST, not a suggestion.**

## Rule

Any repo-internal, GPU-acceleratable activity — the **eu7 fidelity harness** (corpus seed / re-embed),
**corpus re-embeds**, **eval sweeps**, **CE reranking in eval**, and similar embedding/rerank-heavy dev/CI
work — **MUST run on the 3090s (`cuda:0` / `cuda:1`) whenever they have compute + memory room.** CPU is a
**fallback only** — for when the 3090s are busy/full, or GPU is genuinely unavailable.

## Why

- The 3090s are **15–93× faster** than CPU for BGE embedding (0.8.7 GPU embedder). A fresh 0.8.14 eu7 run
  crawled on the CPU embedder (~0.43 docs/sec, seed-timeout-prone) with **both 3090s at 0% idle** — pure
  waste, and the direct cause of the eu7 seed-drain timeout.
- **Fidelity-safe:** 0.8.7 verified CPU↔CUDA embeddings are **1-bit identical**, so GPU does not change the
  eu7 0.90 one-sided-CI fidelity result — it is the *same measurement*, just faster. There is no accuracy
  reason to seed eval corpora on CPU.

## How

- Build with the **`embed-cuda`** feature (and **`rerank-cuda`** for reranking); set
  **`FATHOMDB_EMBED_DEVICE=cuda:0`** (and `FATHOMDB_RERANK_DEVICE=cuda:0`) — a 3090.
- **Exclude the K620 display GPU** (`FATHOMDB_GPU_EXCLUDE=<its index>`) — display-only, never allocate.
- **Check for room first** (`nvidia-smi` utilization + free memory) before dispatching; do not oversubscribe
  a busy 3090 — that is exactly when CPU fallback is correct.
- **Verify the GPU actually engaged** — `nvidia-smi` should show `>0%` util + memory on the chosen device.
  Do not assume the feature/env took.

## Scope

This governs the **repo's internal eval / dev / CI activities**. It is DISTINCT from — and does not change —
the **shipped library's** default device stance (default CPU, GPU opt-in, per the footprint invariant) or
the shipped GPU device-allocation design (`gpu-device-allocation-policy.md`). Those are separate.

## Enforcement (fix the tooling, not per-run reminders)

The eu7 / eval harness and eval runners **MUST default to GPU-when-room** (with a CPU fallback), so no agent
has to remember the flags. Tracked as **`TC-4`** in `dev/todos-and-considerations-ledger.jsonl`
(the GPU-default enforcement item). Until that tooling default lands, every eval/eu7 invocation passes the
`embed-cuda` + `FATHOMDB_EMBED_DEVICE=cuda:0` env/flags explicitly.
