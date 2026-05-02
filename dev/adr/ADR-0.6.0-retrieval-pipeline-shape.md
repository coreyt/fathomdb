---
title: ADR-0.6.0-retrieval-pipeline-shape
date: 2026-04-27
target_release: 0.6.0
desc: Fixed retrieval pipeline stages with graph expansion; rerank deferred; composable middleware revisited 0.8.0
blast_radius: design/retrieval.md; interfaces/*.md (Search type); ADR-0.6.0-typed-write-boundary spirit; ADR-0.6.0-retrieval-latency-gates
status: accepted
---

# ADR-0.6.0 — Retrieval pipeline: fixed stages

**Status:** accepted (HITL 2026-04-27).

Phase 2 #17 design ADR. Settles the retrieval pipeline shape; reserves composable middleware for 0.8.0 revisit (per HITL adjustment 2026-04-27).

## Context

Search query passes through stages: query embedding → candidate set →
optional graph expansion → return. Pipeline shape affects API surface and
extension points. Cross-cuts ADR-0.6.0-retrieval-latency-gates (#9 —
default-pipeline gate).

## Decision

**Fixed stages with expansion config only in 0.6.0.**

```rust
struct Search {
    query: Query,                        // text or pre-embedded vector
    k: usize,
    expand: Option<ExpandConfig>,
}
```

- Smallest API; clear extension points.
- `expand` is carried forward as shipped graph-query surface.
- Matches typed-write-boundary spirit: typed surface, no escape hatch.
- `rerank` is deferred until a concrete 0.6.x or later consumer exists.

**Composable middleware pipeline: deferred to 0.8.0** (per HITL adjustment 2026-04-27). If users emerge with retrieval needs that fixed-stage config cannot express, 0.8.0 revisits.

## Options considered

**A — Fixed stages with `expand` only (chosen).** Smallest typed API; clear
extension points; matches typed-write-boundary spirit. Default 0.6.0 retrieval
matches #9 latency gate.

**B — Composable middleware pipeline (revisit 0.8.0).** Each stage trait object; users splice arbitrary stages. Flexible; opens unbounded surface; layers-on-layers risk. Deferred — re-evaluated in 0.8.0 with concrete user needs.

**C — Two-call API (candidates first, expand second).** User composes. Doubles latency per search; forces clients to manage state. Rejected.

## Consequences

- `design/retrieval.md` documents the fixed-stage pipeline + `ExpandConfig`.
- `interfaces/*.md` exposes a single `search()` verb taking the `Search` struct.
- Multi-hop graph expansion evolves by extending `ExpandConfig`, not by
  opening the pipeline.
- Rerank is not part of the 0.6.0 `Search` surface. Adding it later requires
  an ADR amendment.
- Tracked: `followups.md` FU-RET17 (composable middleware revisit — 0.8.0).
- Cross-cite #9: the latency gate measures the **default** pipeline (no
  expand). Stage-augmented latency is documented separately.

## Citations

- HITL 2026-04-27 (adjustment: composable middleware deferred to 0.8.0).
- ADR-0.6.0-typed-write-boundary (typed-surface spirit).
- ADR-0.6.0-retrieval-latency-gates (#9).
- Stop-doing: layers-on-layers abstractions.
