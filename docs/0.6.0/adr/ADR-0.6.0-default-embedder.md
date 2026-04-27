---
title: ADR-0.6.0-default-embedder
date: 2026-04-27
target_release: 0.6.0
desc: Default-embedder = candle + tokenizers + sqlite-vec; mean-pool + L2-normalize; zerocopy BLOB
blast_radius: crates/fathomdb-engine feature `default-embedder` (model load + inference + vector write); deps candle-core, candle-nn, candle-transformers, tokenizers, hf-hub, sqlite-vec
status: accepted
---

# ADR-0.6.0 — Default embedder architecture

**Status:** accepted (HITL 2026-04-25, decision-recording)

## Context

The Phase 1a deps audit and critic-B F6 / F8 surfaced a dual-path embedder
problem: 0.5.x shipped both a Rust candle path (default-embedder feature)
and a Python `sentence-transformers` path under `python/pyproject.toml`
optional extras. Critic flagged this as "two heavy embedder stacks
shipped without ADR." HITL chose to consolidate on candle for 0.6.0.

The choice is an **agentic-backend / local-first** posture: keep
embedder, vector index, and storage co-located in one process, one file.
The architecture is a Sentence-Transformer (bi-encoder) pattern with the
orchestration in Rust because Candle is minimalist and does not provide a
high-level `SentenceTransformer` class.

## Decision

**Default embedder runs in-process via candle. Architecture (NOTE 1):**

1. **Tokenization.** `tokenizers` crate converts text → input IDs (BERT
   WordPiece for BGE-class models).
2. **Inference.** `candle-transformers::BertModel` runs a forward pass on
   the token IDs and produces token-level embeddings.
3. **Pooling + normalization.** Mean-pool over token outputs along the
   token dimension; L2-normalize the resulting sentence vector. Both
   operations happen in Rust on the candle tensor before the vector
   leaves the embedder. L2-normalization is mandatory for `sqlite-vec`
   cosine similarity.
4. **Storage transfer.** The Rust `Vec<f32>` (or `&[f32]`) is cast to
   bytes via `zerocopy` and inserted as a BLOB into a `vec0` virtual
   table. No round-trip serialization.
5. **Query.** SQL via `rusqlite`:
   ```sql
   CREATE VIRTUAL TABLE vec_items USING vec0(
     embedding FLOAT[<dim>]
   );
   SELECT rowid, distance
     FROM vec_items
    WHERE embedding MATCH ?1 AND k = 10
    ORDER BY distance;
   ```

The default-embedder runs on a worker thread (or `Arc`-shared state); it
must not block the agent reasoning loop on the same async executor.

## Options considered

**A. Rust candle, in-process, bi-encoder (chosen).** Pros: single binary;
no Python runtime needed for the default path; pre-compute document
embeddings, runtime-compute query embedding only; integrates cleanly with
sqlite-vec via zerocopy BLOB. Cons: ~+130 MB wheel size; candle-* + gemm
+ paste transitives (paste flagged unmaintained but not vulnerable).

**B. Python sentence-transformers, in-process.** What the optional path
looked like. Pros: standard library; faster initial DX. Cons: requires
torch (large wheel); duplicates the Rust path's responsibility; ships
two heavy stacks where one suffices; agentic clients may not have torch.

**C. Cross-encoder.** More accurate per query. Cons: requires running the
model **during** search for every (query, doc) pair → too slow for an
agentic system that queries memory frequently. Rejected for default.

**D. Sidecar process / external embedder.** Pros: smaller core wheel.
Cons: loses the local-first single-process posture; adds an IPC seam;
requires deployment coordination. Reserved as 0.7+ option if wheel-size
or compile-time pain forces it.

## Critic findings + resolutions (2026-04-27)

- **EMB-1 BGE pooling.** Critic flagged that BGE checkpoints are often
  trained with CLS-token pooling, so unconditional mean-pool may
  diverge from the Python ST baseline. **HITL: mean-pool stays.**
  Rationale: fathomdb's vectors store *canonical information* for
  agentic search; mean-pool empirically gives better search accuracy
  for that use case than CLS pooling on BGE-class models. Recorded
  here so the choice is not silently re-litigated.
- **EMB-2 L2-norm enforcement.** Embedder protocol requires unit-norm
  vectors. Engine asserts `(‖v‖ - 1.0).abs() < 1e-5` in debug builds
  on every vector handed back from `embed()`. Release builds document
  the contract; no runtime cost. Same assertion applies to Rust /
  Python / TS embedder impls — language-agnostic. Detailed in
  ADR-0.6.0-embedder-protocol.md.
- **EMB-3 Wheel size per-platform.** "+130 MB" approximate; actual
  per-platform sizes measured in CI (Linux x86_64, Linux aarch64,
  macOS, Windows). Per-platform CI gate flags regressions > 20 MB.
  Followup tracked in `deps/README.md`.
- **EMB-4 Embedder runs on engine-owned thread.** Per ADR-0.6.0-async-surface
  Invariant B: candle inference runs on a dedicated engine-owned
  thread pool (sized = `num_cpus::get()`), never on a binding's
  caller thread, asyncio worker, or libuv thread. Mechanically
  enforced in the embedder dispatch layer.
- **EMB-5 hf-hub replacement is its own design.** The "thin ureq GET"
  is a non-trivial decision (auth tokens, 302 to CloudFront,
  range-resume, sha256 verification, atomic rename into cache). Lands
  as a sub-design under `design/embedder.md` before code. Followup.
- **EMB-6 Endianness.** Vectors stored as little-endian f32 BLOBs.
  Documented as an invariant in ADR-0.6.0-zerocopy-blob.md (M-1).
  Workspace targets are all LE (x86_64 + aarch64); BE platforms would
  require an explicit byte-swap step before the cast.
- **X-1 Cold-load × sync writer.** Per ADR-0.6.0-async-surface
  Invariant D: BGE model loaded eagerly at `Engine.open`. Open
  reports model-load duration in its status output (per
  raw-req "schema migration auto-applies + reports per-step duration").
  Cold-load inside a write transaction is a startup-time error in
  debug builds; release builds log + prevent.

## Consequences

- `python/pyproject.toml` `stella` and `embedders` extras drop
  `sentence-transformers` (deps F8). Users wanting Python-side ST call
  the embedder protocol with their own `SentenceTransformer` instance.
- `safetensors` direct dep dropped; use the candle re-export (deps F7).
- `hf-hub` replaced with thin `ureq` GET against the resolve URL pattern
  + best-effort cache compat at `~/.cache/huggingface` (deps F10);
  detailed sub-design in `design/embedder.md`.
- L2-normalization is part of the embedder contract — every vector
  emitted by the default-embedder is unit-norm. Engine asserts in
  debug builds. Detailed in ADR-0.6.0-embedder-protocol.md.
- The Rust `Vec<f32>` → BLOB transfer uses `zerocopy` (or equivalent
  bytemuck `cast_slice`); no per-vector serialize step. Endianness
  + alignment + dim×4 byte-length invariant pinned in
  ADR-0.6.0-zerocopy-blob.md.
- `Engine.open` blocks on model load (Invariant D); exposes load
  duration in the open status report.
- Embedder runs on engine-owned thread pool (Invariant B); no
  caller-thread / asyncio / libuv execution.
- Per-`embed()` timeout default 30s; configurable; failure is typed,
  does not corrupt the writer.

## Citations

- HITL decision 2026-04-25 (deps F6/F7/F8/F10).
- "NOTE 1" architecture supplied by HITL.
- `dev/notes/project-vector-identity-invariant.md` — embedder owns
  identity; vector configs never carry identity strings.
- Stop-doing: per-item variable embedding; speculative knobs.
