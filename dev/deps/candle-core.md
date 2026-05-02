---
title: candle-core
date: 2026-04-24
target_release: 0.10.2
desc: Audit verdict for candle-core
blast_radius: fathomdb-engine `default-embedder` feature only — in-process BGE-small embedder
status: draft
---

# candle-core

**Verdict:** keep

## HITL decision (2026-04-25)

Settled: candle is the chosen default-embedder stack for 0.6.0. Architecture
per NOTE 1 (Phase 1 ADR) — Rust candle + tokenizers + sqlite-vec; manual
mean-pool over BERT token outputs + L2-normalize (cosine readiness for
`sqlite-vec`); zerocopy `f32` slice → BLOB into the `vec0` virtual table via
`zerocopy`. The Python `sentence-transformers` parallel path is dropped —
clients wanting ST can call our embedder protocol with their own ST instance.

Wheel-size cost (~+130 MB) accepted as the cost of a local-first agentic
backend posture.

## Current usage

- Crates using it: fathomdb-engine (feature `default-embedder`)
- Surface used: tensor + device for running BGE-small via candle-transformers
- Version pin: `0.10.2`; latest 0.10.x (huggingface)

## Maintenance signals

- Last release: active (huggingface org)
- Open issues / open CVEs: transitive `rand 0.9` advisory RUSTSEC-2026-0097 (unsound with custom logger) — not triggered by our usage. Transitive `paste 1.0.15` unmaintained (RUSTSEC-2024-0436) via tokenizers + pulp — low risk, macro-only.
- Maintainer count: huggingface org; sole-maintainer risk: no
- License: MIT OR Apache-2.0 — compatible: yes
- MSRV: ~1.75; matches: yes

## Cross-platform

- Pulls `gemm` family with SIMD per-arch; builds on all four triples. CUDA/Metal features off in our config.
- C-boundary footguns: candle ops use raw pointers but with explicit element-typed slices; no fn-pointer transmutes in our integration.

## Alternatives considered (≥1)

- `ort` (ONNX Runtime): pros — broader model support, mature; cons — large native dep, complicates wheel building, abi3 issues. Migration cost: ~600 LoC embedder rewrite. Not net win.
- Drop default-embedder, require external embedder always: pros — much smaller dep tree, faster compile; cons — DX regression for "just works" path. Worth ADR in Phase 2.

## Verdict rationale

Only pure-Rust path to in-process embedding. Heavy but isolated behind feature gate. Keep — but Phase 2 should decide whether default-embedder ships in 0.6.0 or moves to a sidecar.

## What would force replacement in 0.7.0?

Compile-time pain or wheel-size pain forcing sidecar embedder; or a new model class candle cannot run.
