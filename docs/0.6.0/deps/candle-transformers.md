---
title: candle-transformers
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for candle-transformers
blast_radius: fathomdb-engine `default-embedder` (BERT/BGE model definition)
status: draft
---

# candle-transformers

**Verdict:** keep

## Current usage
- Crates using it: fathomdb-engine (feature `default-embedder`)
- Surface used: `bert::BertModel` for BAAI/bge-small-en-v1.5
- Version pin: `0.10.2`

## Maintenance signals
- Same family as candle-core. License MIT/Apache-2.0. No direct CVEs.

## Cross-platform
- Same as candle-core.
- C-boundary footguns: none direct.

## Alternatives considered (≥1)
- Inline BERT impl using only candle-nn: pros — drop a dep; cons — duplicates upstream maintenance burden. Not worth it.

## Verdict rationale
Required for BGE forward pass. Keep.

## What would force replacement in 0.7.0?
Same as candle-core.
