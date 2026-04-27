---
title: ADR-0.6.0-recovery-rank-correlation
date: 2026-04-27
target_release: 0.6.0
desc: Post-recovery vector top-k rank-correlation threshold (Kendall tau) for AC-027d
blast_radius: acceptance.md AC-027d; test-plan.md vector-recovery test; design/vector.md recovery semantics
status: accepted
---

# ADR-0.6.0 — Recovery rank-correlation threshold

**Status:** accepted (HITL 2026-04-27, decision-recording — promoted from
FU-AC-PROTOCOL-BACKFILL).

Phase 3b-promoted acceptance ADR. Owns the numeric threshold that
AC-027d gates on.

## Context

Physical recovery rebuilds vector projections from canonical state
(REQ-040; AC-044). Re-embedding the same canonical inputs with the same
embedder produces vectors that are bit-equal modulo (a) embedder
non-determinism on some accelerators, (b) sqlite-vec ANN tie-breaking
order. Top-k results post-recovery may therefore differ from
pre-corruption snapshots only at tied positions.

AC-027d asserts post-recovery rank-correlation against a pre-corruption
baseline. The threshold is user-visible (data-quality property
operators reason about) and deserves an ADR-grade commitment, not a
test-plan.md tolerance knob.

## Decision

**Per-query Kendall tau ≥ 0.9** between pre-corruption top-10 and
post-recovery top-10 result sets, evaluated over the documented
100-query suite (AC-027d fixture).

Aggregate gate: 100% of queries meet the per-query bound (no averaging
that lets one collapsed query hide).

## Options considered

**A — tau ≥ 0.9 per-query, 100% pass (chosen).** Conservative; tolerant
of tie-breaking + minor non-determinism; intolerant of structural
corruption that would actually re-rank top results.

**B — tau ≥ 0.95 per-query.** Tighter; risks flaking on legitimate
tie-breaking variance from sqlite-vec; would require harness control
of tie-break order.

**C — Mean tau ≥ 0.9 across the suite.** Easier; lets one collapsed
query hide behind 99 perfect ones. Rejected (silent-degrade Stop-doing).

**D — Top-k set overlap ≥ N% (Jaccard).** Set-based, ignores order;
weaker correctness signal than rank-correlation. Rejected.

## Consequences

- AC-027d cites this ADR for the 0.9 threshold.
- `design/vector.md` recovery section documents tie-break determinism
  if it materially affects measurement (currently not believed to).
- Embedder non-determinism on accelerators (e.g. CUDA reductions) is
  out of scope for 0.6.0 default-embedder (CPU-only via candle); if
  GPU embedders land later, this ADR re-opens.

## Citations

- HITL 2026-04-27.
- AC-027d (forcing function).
- ADR-0.6.0-vector-identity-embedder-owned (embedder determinism
  contract).
