# Memex input needed: `ExpansionKind` enum in 0.5.3 AST

**Date:** 2026-04-20
**Context:** `dev/notes/design-0.5.3-edge-projecting-traversal.md` §3
**Decision pending:** keep or drop `ExpansionKind::{Nodes, Edges}` enum

---

## What the enum is

Current 0.5.3 design introduces:

```rust
pub enum ExpansionKind { Nodes, Edges }

pub struct QueryAst {
    pub expansions: Vec<ExpansionSlot>,         // existing, node-projecting
    pub edge_expansions: Vec<EdgeExpansionSlot>, // new, edge-projecting
    ...
}
```

Two separate vecs already discriminate node vs edge expansions by type.
The `ExpansionKind` enum is a redundant tag — unless some consumer wants
a unified list discriminated by the enum instead of two vecs.

## Why we're asking

Two paths forward, both preserve wm2 hot-path behavior:

- **Keep enum.** Small surface cost (one enum, one tag). Useful if a
  future Cypher-translator IR or plan-cache layer wants a single unified
  `Vec<AnyExpansion>` keyed by `ExpansionKind`.
- **Drop enum.** Simpler AST. Slot vec membership is the discriminator.
  Caller never writes `ExpansionKind::Edges` — they call
  `.traverse_edges(...)` or `.expand(...)` and the builder routes.

## What we need from Memex

1. Does wm2 (or any Memex code) construct `QueryAst` JSON directly, or
   always go through the Python/TS builder?
   - **Builder-only:** enum is invisible to you. Drop has zero client
     impact.
   - **Direct AST JSON:** the enum may show up in your serialized
     payloads. Drop would mean one fewer field to set.

2. Any planned Memex feature that wants to iterate expansions
   generically (node and edge in one pass)? If so, unified list keyed
   by enum is ergonomic; two-vecs forces two loops.

3. Strong preference either way for your codegen / introspection
   tooling?

## Default if no response

We keep the enum. Removal is the riskier choice (feature loss if a
generic-iteration use case shows up later); addition later is harder
than removal later only if we commit to the two-vec shape now. Keeping
the enum costs ~5 LOC and zero runtime.

Reply on this note or in the 2026-04-20 thread. Need answer before
Pack B lands (AST shape freeze).

---

## Resolution (2026-04-20)

Memex response:

1. Builder-only. No `QueryAst` JSON construction in Memex. All access
   via `SearchBuilder.expand(...).execute_grouped()`
   (`src/memex/memory/retrieval.py:344`). Grep for
   `QueryAst|ExpansionKind|edge_expansions` returns hits in FathomDB
   design notes only, zero in `src/`.
2. No generic-iteration feature planned. wm2 edge traversal (per
   `dev/notes/fathomdb-edge-traversal-request.md`) needs edge-
   projecting reads for `_DEPENDENCY_RELATIONSHIPS` replacement —
   filter edges by `dependency=true` attribute. Always routed through
   `.traverse_edges(...)` call site. Node + edge expansion never mixed
   in one pass.
3. Preference: drop enum.

**Decision: drop `ExpansionKind`.** Slot vec membership is the
discriminator. Design doc §3 updated.
