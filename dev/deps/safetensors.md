---
title: safetensors
date: 2026-04-25
target_release: 0.6.0
desc: Audit verdict for safetensors — drop (use candle re-export)
blast_radius: fathomdb-engine feature `default-embedder` weight loading; Cargo.toml direct dep removal
status: draft
---

# safetensors

**Verdict:** drop

## HITL decision (2026-04-25)

Critic-B F7: candle-* re-exports safetensors. Direct dep is dead weight.
HITL: use the candle re-export. ~10 LoC migration delta in
`default-embedder` weight loader.

## Current usage (pre-drop)

- Crates using it: fathomdb-engine (feature `default-embedder`)
- Surface used: load BGE `.safetensors` weights
- Version pin: `0.7.0`; latest 0.7.x

## Maintenance signals (pre-drop)

- License: Apache-2.0 — compatible: yes
- Maintainer: huggingface

## Migration plan

- Replace `use safetensors::...` with `use candle_core::safetensors::...` (or whichever submodule candle exposes) in default-embedder weight loader. Drop the `safetensors` line from `src/rust/crates/fathomdb-engine/Cargo.toml`.
- Estimated diff: ~10 LoC. Behavior delta: none (candle wraps the same loader).
- Risk: low. CI matrix verifies BGE weight load on all four platforms.
