---
title: sentence-transformers
date: 2026-04-25
target_release: 0.6.0
desc: Audit verdict for sentence-transformers — drop (candle Rust path replaces)
blast_radius: src/python/pyproject.toml optional-dependencies (`stella`, `embedders` extras)
status: draft
---

# sentence-transformers

**Verdict:** drop

## HITL decision (2026-04-25)

Critic-B F8: sentence-transformers + candle keep both = ship two heavy
embedder stacks by default. HITL: lean candle for 0.6.0; drop
sentence-transformers (Python optional dep). Embedder architecture per
NOTE 1 — Rust candle + tokenizers + sqlite-vec, manual mean-pool +
L2-normalize, zerocopy BLOB to vec0. Recorded in Phase 1 ADR.

## Migration plan

- Remove `sentence-transformers` from `src/python/pyproject.toml` `[project.optional-dependencies]` (`stella`, `embedders` extras).
- Users wanting Python-side sentence-transformers can call our embedder protocol with their own ST instance — same as any other client-side embedder.
- Estimated diff: ~5 lines in pyproject.toml + extras docs.
- Risk: low; default-embedder still supplied via Rust candle path. No DX regression for users on the default path.

## Current usage (pre-drop)

- Where: `src/python/pyproject.toml` optional-dependencies `stella`, `embedders` (>=2.7)
- Surface used: `SentenceTransformer(...)` for stella-class models in Python-side embedder

## Maintenance signals (pre-drop)

- License: Apache-2.0 — compatible: yes
- Maintainer count: UKPLab + community
