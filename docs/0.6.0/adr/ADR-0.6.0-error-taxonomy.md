---
title: ADR-0.6.0-error-taxonomy
date: 2026-04-27
target_release: 0.6.0
desc: Per-module errors composed via thiserror into a unified top-level EngineError surface
blast_radius: every module in fathomdb-engine; every binding error-mapping table; design/*.md error-handling sections; thiserror dep
status: accepted
---

# ADR-0.6.0 — Error taxonomy

**Status:** accepted (HITL 2026-04-27).

Phase 2 #18 design ADR. Decides Rust error design across the engine and its bindings.

## Context

Single crate-level enum is exhaustive but large; per-module errors compose via `From` but multiply binding mapping work. Affects PR review burden + binding complexity + how errors surface to client code in every binding language.

## Decision

**Per-module errors + top-level `EngineError` that wraps via `#[from]`.**

- Each subsystem module owns its error type: `StorageError`, `ProjectionError`, `VectorError`, `EmbedderError`, `SchedulerError`, `OpStoreError`, etc.
- Each module error is `thiserror::Error` + `Debug` + `Display`.
- Top-level `pub enum EngineError` at crate root has a variant per module, wrapping via `#[from]`. Returned from every public engine function.
- Bindings map `EngineError` once to their language's exception/Result shape.

```rust
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Projection(#[from] ProjectionError),
    #[error(transparent)]
    Vector(#[from] VectorError),
    #[error(transparent)]
    Embedder(#[from] EmbedderError),
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
    #[error(transparent)]
    OpStore(#[from] OpStoreError),
    // additional variants per module as engine grows
}
```

## Options considered

**A — Single crate-level `EngineError` enum.** All public errors live here. Smallest binding surface. Biggest enum; weakest module boundaries; module-internal code needs to know the unified enum.

**B — Per-module errors only; binding maps each.** Cleanest module boundaries. Binding burden multiplies (one mapping table per module).

**C — Per-module + top-level wrap (chosen).** Module errors give engine code clean boundaries; unified surface gives bindings a single type to map. `thiserror` `#[from]` makes wrapping free. Idiomatic Rust.

## Consequences

- `thiserror` confirmed as direct workspace dep (was already audited as keep).
- `design/*.md` per-subsystem docs each list their owned error type + variants.
- Bindings (`fathomdb-py`, `fathomdb-ts`) write a single `EngineError → language-error` mapping table.
- Adding a new module = adding a new variant to `EngineError` (one-line `#[from]`); no changes to existing modules.
- `EmbedderError` is a public-trait error (per ADR-0.6.0-embedder-protocol) and surfaces through this taxonomy.
- Errors from the typed-write boundary (validation failures) live in a `WriteValidationError` module variant (specific shape lives in design/engine.md).

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-embedder-protocol (EmbedderError surface).
- ADR-0.6.0-typed-write-boundary (validation errors).
- `thiserror` standard Rust idiom.
