---
title: Nested-source projections for Memex B15
date: 2026-08-04
target_release: 0.8.21 Slice 60
status: ACTIVE
---

# ADR-0.8.21 — Nested-source projections for Memex B15

## Decision

FathomDB 0.8.21 Slice 60 will implement the contract in
[Nested-Source Projections](../design/nested-source-projections.md): an
application may declare a literal nested member path on `ProjectionSpec`; the
engine derives its own scalar EAV and property-FTS rows from the canonical node
body; and the matching exact-attribute and projected-text query surfaces ship
together in Rust, Python, and TypeScript.

The canonical body is the only authority. Memex keeps its `EntityTypeSpec` and
canonical entity body, derives declarations idempotently at startup, and does
not maintain a second attribute index or duplicate scalar attributes at the
body root.

The public delta is intentional: `ProjectionSpec.source`, portable
`SearchFilter.attributes`, and `search_projected_text` / `searchProjectedText`
must be added to the governed surface and binding contracts in the same slice.
No merge to `main`, push, or publication is authorized by this ADR; those remain
separate HITL gates.

## Type-equality ruling

Memex B15 explicitly accepts canonical-text, type-collapsed equality for this
delivery. A terminal JSON string `"1"` and JSON number `1` both project the
canonical text `"1"`, and an equality predicate for that text matches either.
This is a deliberate, tested semantic—not an implicit SQLite coercion.

Type-distinct equality, a typed value union, ranges, negation, and cardinal
multi-value projections are not part of Slice 60. A consumer needing
type-distinct equality must encode the distinction in its canonical string until
a separate typed-property ADR is accepted.

## Consequences

- The registry schema gains only the additive source-declaration representation;
  it performs no `INSERT ... SELECT` or canonical-body rewrite.
- A missing/null source produces no row; an object/array terminal rejects the
  whole relevant write or registry configuration transaction with the existing
  write-validation family.
- Changing a source is destructive and needs the existing explicit-drop flow.
- Tests must use normal engine writes and a real database, including literal
  path segments, cleanup across rewrite/lifecycle/erasure, cross-binding parity,
  and the accepted `"1"`/`1` equality case.
