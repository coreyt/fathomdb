---
title: ADR-0.6.0-default-embedder
date: 2026-04-25
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

## Consequences

- `python/pyproject.toml` `stella` and `embedders` extras drop
  `sentence-transformers` (deps F8). Users wanting Python-side ST call
  the embedder protocol with their own `SentenceTransformer` instance.
- `safetensors` direct dep dropped; use the candle re-export (deps F7).
- `hf-hub` replaced with thin `ureq` GET against the resolve URL pattern
  + best-effort cache compat at `~/.cache/huggingface` (deps F10).
- L2-normalization is part of the embedder contract — every vector
  emitted by the default-embedder is unit-norm. Recorded as an
  invariant in `design/vector.md`.
- The Rust `Vec<f32>` → BLOB transfer uses `zerocopy` (or equivalent
  bytemuck `cast_slice`); no per-vector serialize step.

## Citations

- HITL decision 2026-04-25 (deps F6/F7/F8/F10).
- "NOTE 1" architecture supplied by HITL.
- `dev/notes/project-vector-identity-invariant.md` — embedder owns
  identity; vector configs never carry identity strings.
- Stop-doing: per-item variable embedding; speculative knobs.
