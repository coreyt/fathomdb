---
title: ADR-0.6.0-prepared-write-shape
date: 2026-04-27
target_release: 0.6.0
desc: PreparedWrite is per-entity newtypes wrapped in a #[non_exhaustive] enum; one engine entry point
blast_radius: fathomdb-engine writer API; PyO3 + napi-rs bindings; CLI typed-verb dispatch; interfaces/{rust,python,typescript,cli}.md; error-taxonomy boundary
status: accepted
---

# ADR-0.6.0 — PreparedWrite shape

**Status:** accepted (HITL 2026-04-27, decision-recording — lite batch).

Phase 2 #19 design ADR. Closes the shape question left open by
ADR-0.6.0-typed-write-boundary; the boundary itself is already settled.

## Context

ADR-0.6.0-typed-write-boundary closed the typed-vs-raw-SQL question and
left the shape (single enum vs per-entity newtypes vs builder vs trait)
explicitly open. The shape governs:

- How bindings (PyO3, napi-rs) marshal Python / JS dicts into typed
  payloads.
- How CLI typed-verb dispatch parses flags into one of N variants.
- How the engine writer dispatches one transaction across mixed-entity
  batches.
- Whether adding a new entity type is a breaking change downstream.

## Decision

**Per-entity newtype structs wrapped in a `#[non_exhaustive]` enum, with
one engine entry point taking the enum.**

```rust
#[non_exhaustive]
pub enum PreparedWrite {
    Node(NodeWrite),
    Edge(EdgeWrite),
    OpStore(OpStoreInsert),
    AdminSchema(AdminSchemaWrite),
    // future variants added without a major bump (non_exhaustive)
}

pub struct NodeWrite { /* typed fields per ADR-0.6.0-typed-write-boundary */ }
pub struct EdgeWrite { /* ... */ }
// ...

impl Engine {
    pub fn write(&self, ops: &[PreparedWrite]) -> Result<WriteReceipt, EngineError>;
}
```

- **One entry point.** The writer takes `&[PreparedWrite]`; no per-entity
  `write_node`, `write_edge`, etc. — those are conveniences in bindings
  if needed, never additional engine methods.
- **Newtype per entity.** Each variant carries a struct, not a tuple of
  raw fields — gives field documentation, default-impl evolution, and a
  clean PyO3 / napi-rs marshalling target.
- **`#[non_exhaustive]` mandatory.** New entity types added in 0.6.x do
  not break downstream `match` on the enum; aligns with
  ADR-0.6.0-error-taxonomy's `#[non_exhaustive]` posture.
- **Batch transactional semantics deferred.** Whether a
  `&[PreparedWrite]` is one transaction, N transactions, or
  per-variant-grouped is **not** decided here. That is a substantive
  design choice owned by `design/engine.md` (and possibly a sibling ADR
  if it does not reduce to mechanical specification). This ADR commits
  only to the *shape* of the input.

## Options considered

**A — Per-entity newtypes in a `#[non_exhaustive]` enum (chosen).**
Pros: one engine surface; type-safe per-entity fields; easy to evolve
(add variant); marshalling target is obvious; matches taxonomy posture.
Cons: callers must wrap a single-entity write in `PreparedWrite::Node(_)`
(noise mitigated by binding sugar).

**B — Single enum with per-variant tuple/struct fields inline.** Pros:
fewer types. Cons: field documentation stuffed into variant docs; no
default-impl evolution per entity; PyO3 marshalling has to switch on
variant inside one large impl. Rejected.

**C — Trait `Write` with per-entity types implementing it; engine takes
`&[Box<dyn Write>]`.** Pros: open-extension feel. Cons: dyn dispatch in
the writer hot path; bindings cannot enumerate types for marshalling
without registry; downstream crates could implement `Write` and bypass
typed boundary intent. Rejected — re-introduces the
"speculative-extensibility" Stop-doing class.

**D — Builder API: `Engine::write().node(...).edge(...).commit()`.**
Pros: ergonomic for chained writes. Cons: hides the batch shape from the
type system; binding parity (Python / TS) requires re-implementing the
builder per language; partial-build state is footgun-prone. Rejected.

## Consequences

- `interfaces/rust.md`: `Engine::write(&[PreparedWrite]) -> Result<WriteReceipt, EngineError>`.
- `interfaces/python.md`: marshals Python inputs → `Vec<PreparedWrite>`
  via PyO3. Bindings MAY add per-entity convenience helpers; the engine
  surface remains a single entry point. The exact Python input shape
  (dict-input vs typed Python class) is owned by
  ADR-0.6.0-python-api-shape.
- `interfaces/typescript.md`: TS types `NodeWrite`, `EdgeWrite`, etc. are
  exposed per ADR-0.6.0-typescript-api-shape. Helper ergonomics are
  owned by that ADR; this ADR commits only that whatever helpers ship
  construct `PreparedWrite` values, not a parallel engine surface.
- `interfaces/cli.md`: typed verbs (per ADR-0.6.0-typed-write-boundary)
  parse to the matching variant; CLI never builds a batch — one verb =
  one variant.
- `OpStoreInsert` (already named in ADR-0.6.0-typed-write-boundary X-2)
  is a `PreparedWrite::OpStore(_)` variant; not a side path.
- `AdminSchemaWrite` covers admin DDL — same writer thread per
  ADR-0.6.0-single-writer-thread; no separate admin path. The variant's
  existence is required by the accepted `admin.configure` public surface and is
  therefore locked for 0.6.0 even though the exact internal field set remains
  owned by `design/engine.md`.
- `error-taxonomy`: write errors flow as defined in
  ADR-0.6.0-error-taxonomy. **No new fields** added to
  `WriteValidationError` here — any "originating slice index" affordance
  for batch diagnostics is deferred to the same engine-design decision
  that settles batch semantics. Do not treat this ADR as amending
  error-taxonomy.
- Adding a 0.6.x entity type (e.g. `PreparedWrite::Annotation(_)`) is
  non-breaking **at the Rust crate boundary** thanks to
  `#[non_exhaustive]`. **Binding-side exhaustiveness is not protected**:
  Python `isinstance` chains and TypeScript discriminated-union
  switches will silently miss new variants. Per-binding discriminant
  posture (default branch required? runtime check on unknown variant?)
  is owned by ADR-0.6.0-python-api-shape and
  ADR-0.6.0-typescript-api-shape; flagged as a followup if neither
  binding ADR addresses it.

## Citations

- ADR-0.6.0-typed-write-boundary (boundary closed; shape left open).
- ADR-0.6.0-error-taxonomy (`#[non_exhaustive]` posture; boundary table).
- ADR-0.6.0-op-store-same-file (`OpStoreInsert` as a variant).
- ADR-0.6.0-single-writer-thread (one writer dispatches all variants).
- HITL 2026-04-27.
