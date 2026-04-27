---
title: sqlite-vec
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for sqlite-vec
blast_radius: fathomdb-engine (vector search path), fathomdb-schema (vec0 virtual table migrations)
status: draft
---

# sqlite-vec

**Verdict:** keep

## HITL decision (2026-04-25)

Critic-B F3 flagged sole-maintainer risk (asg017). HITL: **risk accepted; no
fallback plan, no vendored fork, no quarterly upstream-activity check.**
Decision recorded; no further work on contingency planning. If asg017
abandons the project AND no fork picks it up, the issue is re-opened in a
future release — not pre-mitigated in 0.6.0.

## Current usage
- Crates using it: fathomdb-engine, fathomdb-schema (feature-gated `sqlite-vec`)
- Surface used: extension auto-load via `sqlite3_vec_init`; `vec0` virtual tables for ANN
- Version pin: `0.1` (workspace); latest 0.1.x

## Maintenance signals
- Last release: 2025 (active, asg017)
- Open issues / open CVEs: no advisories
- Maintainer count: 1 (asg017); sole-maintainer risk: yes
- License: Apache-2.0 OR MIT — compatible: yes
- MSRV: low; matches: yes

## Cross-platform
- Builds clean on all four target triples; ships precompiled C source.
- C-boundary footguns: extension entry uses `c_int` / `c_char` correctly; no hardcoded i8/u8 in our integration.

## Alternatives considered (≥1)
- `usearch` / `hnswlib` bindings: pros — better recall on large indices; cons — separate process or extra C dep, no SQLite VT integration, breaks single-file invariant. Migration cost: ~3k LoC + new query plan for hybrid graph+vector joins. Behavior delta: loses transactional vector writes.
- pure-Rust `instant-distance`: pros — no C; cons — no SQLite integration, would require maintaining our own ANN persistence layer. Not viable.

## Verdict rationale
Only embedded vector index that integrates as a SQLite virtual table. Sole-maintainer risk is real but mitigated by small surface and our feature gate. Keep.

## What would force replacement in 0.7.0?
asg017 abandons project AND a fork doesn't pick it up; or recall ceiling forces a richer ANN (HNSW/IVF) we cannot get from vec0.
