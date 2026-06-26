---
name: embed-completeness-and-gpu-readiness
description: "FathomDB dense-embed readiness — completeness verifier + WAL trap, partial-embed evidence, and the GPU/embedder architecture (CPU-pinned, serialized, no ROCm/Vulkan)"
metadata: 
  node_type: memory
  type: project
  originSessionId: 9c081152-5f54-47d2-8c9a-3f3d13a43ba6
---

0.8.1 fused/dense-embed readiness, established 2026-06-15.

**Completeness verification (do this before trusting fused recall or a long embed).**
`drain()`/`_fathomdb_projection_terminal` do NOT prove docs were embedded — a doc is
marked terminal even when NOT embedded (kind unregistered). Ground truth = per-kind
attribution via `_fathomdb_vector_rows(rowid, kind, write_cursor)`: coverage = distinct
doc cursors with a vector / doc-node count, must == 1.0. Tool: `eval/verify_embed_db.py`
(+ `assert_embed_complete` wired as a post-`drain()` gate in
`_build_fathomdb_variant`; tests `test_verify_embed_db.py`). **WAL trap:** open the
verifier `mode=ro` WITHOUT `immutable=1` — immutable reads only the main DB file and
misses committed-but-uncheckpointed rows in `-wal` while the engine is open → falsely
reports an empty DB. Evidence of real partial embeds: `/tmp/r2-lme-s-nograph.sqlite`
= 0 doc vectors (kind never registered), `/tmp/r2-lme-s-v2.sqlite` = 1,408 vectors all
`edge_fact`, 0 doc (~7%). No pre-embedded full-corpus DB exists to reuse.

**Why the embed is slow (4.7 sess/min) + speedup levers.** `Embedder::embed(&str)` is
one-text-per-forward (no batch API); `Device::Cpu` is hardcoded (`candle_bge.rs:168`);
`PROJECTION_WORKERS=2` but the PR-9 guard serializes the embedder to one call at a time.
Net ≈ 1 of 24 cores, 0 of 2 idle RTX 3090s. **GPU is the big lever** (~100-300× → 27h→
minutes): CUDA 12.6 toolkit present, 3090s compute-idle, BGE-small <1GB VRAM. Plan: add
candle `cuda` feature + a `resolve_device()` seam (env `FATHOMDB_EMBED_DEVICE`, default
CPU so tests/byte-stability unaffected) + one maturin rebuild in the MAIN tree (not a
worktree, per [[agent-worktree-stale-base-trap]]). Serialization is benign/helpful on
GPU (no concurrent CUDA calls); future throughput lever is BATCHING, not de-serializing.
**Cross-vendor:** candle 0.10 has NO ROCm/Vulkan (only CPU/CUDA/Metal); AMD/Intel later
= a new `impl Embedder` (e.g. ONNX-Runtime `ort`: CUDA/ROCm/DirectML/OpenVINO) plugged
via the existing `EmbedderChoice::Caller(Arc<dyn Embedder>)` seam — zero engine change.
Guard: different backends must be vector-equivalent or carry a distinct `EmbedderIdentity`
(open-time dim/identity check) so a mismatch is caught, never silently mixed.

**`drain()` for long runs:** `drain(timeout_ms)` blocks on `wait_for_idle` → raises
`Err(Scheduler)` on timeout. Current `drain(timeout_s=3600)`=1h ≪ a multi-hour CPU embed
→ would falsely fail. Embed IS resumable (projection state persisted → reopen+drain
continues). Robust long-run pattern: bounded poll loop + the verifier as completion
oracle. Op note: a 320-req airlock batch takes ~30m (not 25m); raise `--max-polls` and
the answerer spend survives server-side (`finalizing`) even if the poller gives up.
