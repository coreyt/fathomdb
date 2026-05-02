---
title: Retrieval Subsystem Design
date: 2026-04-30
target_release: 0.6.0
desc: Fixed-stage retrieval pipeline, query planning, and branch-fallback behavior
blast_radius: search path; REQ-010, REQ-011, REQ-017, REQ-018, REQ-029, REQ-034
status: locked
---

# Retrieval Design

This file owns the fixed-stage retrieval pipeline, safe FTS grammar handling,
hybrid branch composition, and the graph-expansion configuration that survives
the ADR-level pipeline choice.

## 0.6.0 stage surface

0.6.0 supports graph `expand` on search results as carried-forward product
surface. `rerank` is deferred and is not part of the 0.6.0 search contract.

## Soft-fallback signal

REQ-029 / AC-031 make the hybrid fallback signal part of the public search
contract.

The typed branch enum in 0.6.0 is exactly:

- `Vector`
- `Text`

Semantics:

- the fallback record is present only when one non-essential branch could not
  contribute
- `Vector` means the vector branch could not contribute
- `Text` means the text branch could not contribute
- total request failure is not expressed as a soft-fallback record

This file owns the branch enum and its meaning. The per-binding field name on
the returned fallback record is owned by `interfaces/{python,typescript,rust}.md`.
