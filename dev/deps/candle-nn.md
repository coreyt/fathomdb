---
title: candle-nn
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for candle-nn
blast_radius: fathomdb-engine `default-embedder` (model layer impls)
status: draft
---

# candle-nn

**Verdict:** keep

## Current usage

- Crates using it: fathomdb-engine (feature `default-embedder`)
- Surface used: layer norm, linear, embedding modules used by BGE model
- Version pin: `0.10.2`

## Maintenance signals

- Same family as candle-core. License MIT/Apache-2.0. No direct CVEs.

## Cross-platform

- Same as candle-core.
- C-boundary footguns: none direct.

## Alternatives considered (≥1)

- Hand-written BGE forward pass: pros — drop dep; cons — re-implement attention/norm primitives. Not worth it.

## Verdict rationale

Pairs with candle-core/transformers. Keep.

## What would force replacement in 0.7.0?

Same as candle-core (sidecar embedder decision).
