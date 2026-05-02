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

- Each subsystem module owns its error type: `StorageError`, `ProjectionError`, `VectorError`, `EmbedderError`, `SchedulerError`, `OpStoreError`, `WriteValidationError`, `SchemaValidationError`, `EmbedderIdentityMismatchError`, etc.
- Each module error is `thiserror::Error` + `Debug` + `Display`.
- Each per-module enum and the top-level `EngineError` are marked **`#[non_exhaustive]`**. Adding a new variant or module is not a semver break.
- Top-level `pub enum EngineError` at crate root has a variant per module, wrapping via `#[from]`. Returned from every public engine function.
- Bindings map `EngineError` to their language's exception hierarchy (subclass-per-variant; see § Binding mapping below).

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EngineError {
    #[error("storage: {0}")]
    Storage(#[from] StorageError),
    #[error("projection: {0}")]
    Projection(#[from] ProjectionError),
    #[error("vector: {0}")]
    Vector(#[from] VectorError),
    #[error("embedder: {0}")]
    Embedder(#[from] EmbedderError),
    #[error("scheduler: {0}")]
    Scheduler(#[from] SchedulerError),
    #[error("op-store: {0}")]
    OpStore(#[from] OpStoreError),
    #[error("write-validation: {0}")]
    WriteValidation(#[from] WriteValidationError),
    #[error("schema-validation: {0}")]
    SchemaValidation(#[from] SchemaValidationError),
    #[error("embedder-identity-mismatch: {0}")]
    EmbedderIdentityMismatch(#[from] EmbedderIdentityMismatchError),
    #[error("overloaded: queue_depth={queue_depth} threshold={threshold}")]
    Overloaded { queue_depth: usize, threshold: usize },
    #[error("engine closing")]
    Closing,
    // additional variants per module as engine grows; #[non_exhaustive] above
}
```

`#[error("module: {0}")]` is used per variant (not `transparent`) so the module attribution stays visible in `Display`. Reserved for cases where the inner error already self-prefixes.

### Module-error boundary table (ERR-6 — keep distinct)

Three "validation"-flavoured errors stay as **distinct** module variants. They are not redundant; each has a distinct producer ADR and distinct user-facing semantic:

| Module error                    | Producer ADR                             | Surfaces when    | User-facing meaning                                              |
| ------------------------------- | ---------------------------------------- | ---------------- | ---------------------------------------------------------------- |
| `WriteValidationError`          | ADR-0.6.0-typed-write-boundary           | Write submission | Typed input is structurally malformed (wrong field, wrong shape) |
| `SchemaValidationError`         | ADR-0.6.0-json-schema-policy             | Write submission | Payload fails JSON Schema check against registered `schema_id`   |
| `EmbedderIdentityMismatchError` | ADR-0.6.0-vector-identity-embedder-owned | `Engine.open`    | Open-time embedder identity ≠ recorded profile identity          |

Distinctness rationale: each is owned by its producing ADR (clean coupling); each maps to a different user remediation (fix input shape vs fix payload contents vs resolve an open-time embedder mismatch); `EmbedderIdentityMismatchError` doesn't even surface at write time. Collapsing into one `WriteError` would lose this signal and force `EmbedderIdentityMismatch` (an `Engine.open` error) into a misnamed bucket.

### Foreign-error wrapping policy (security)

Module errors that wrap foreign causes (`rusqlite::Error`, `io::Error`, `serde_json::Error`):

- **Sanitize at the module boundary.** SQL fragments, absolute filesystem paths, and `serde_json` byte offsets MUST NOT appear in module-error `Display` strings. The module wraps the foreign error with a sanitised message; the full chain is available via `Error::source` for engine-internal logging only.
- Tension with ADR-0.6.0-typed-write-boundary is resolved here: that ADR closes the SQL surface in the API; this rule prevents SQL from leaking back through error messages.
- Engine internal `tracing` may log the full `Error::source` chain at `debug` level (operator-controlled).

### Message stability + backtraces

- **Variant discriminant is semver-stable.** Adding new variants is allowed (per `#[non_exhaustive]`); removing or renaming is a major-version break.
- **`Display` strings are NOT semver-stable.** Patch releases may reword. Bindings must NOT use `Display` strings as a programmatic discriminator; use the variant.
- **Backtraces captured in debug builds only** via standard `RUST_BACKTRACE` mechanism. Backtraces are NEVER auto-attached to Python/TS exception messages (PII risk: backtraces include local file paths). They are available via engine logging.

### Binding mapping

Both PyO3 and napi-rs bindings expose the per-variant subclass hierarchy:

- **Base class:** `FathomDBError` (Python: subclasses `Exception`; TS: subclasses `Error`).
- **One subclass per top-level `EngineError` variant.** E.g. `StorageError`, `ProjectionError`, `WriteValidationError`, `SchemaValidationError`, `EmbedderIdentityMismatchError`, `OverloadedError`, `ClosingError`.
- Bindings expose the variant→subclass mapping table; this table is the single source of truth across both bindings (per ADR-0.6.0-typescript-api-shape § Common Types).
- New variants land as new subclasses in both bindings in the same release.

## Options considered

**A — Single crate-level `EngineError` enum.** All public errors live here. Smallest binding surface. Biggest enum; weakest module boundaries; module-internal code needs to know the unified enum.

**B — Per-module errors only; no top-level wrap.** Cleanest module boundaries. The real cost is not "mapping table size" — it is that bindings have no single entry-point type to dispatch on; every public function would return a different error type, multiplying binding-side type machinery (PyO3 `IntoPy` impls, napi-rs `From<E> for napi::Error` impls) per module. Rejected on entry-point-count grounds, not mapping-burden grounds.

**C — Per-module + top-level wrap (chosen).** Module errors give engine code clean boundaries; unified surface gives bindings a single type to map. `thiserror` `#[from]` makes wrapping free. Idiomatic Rust.

## Consequences

- `thiserror` confirmed as direct workspace dep (was already audited as keep).
- `design/*.md` per-subsystem docs each list their owned error type + variants.
- Bindings (`fathomdb-py`, `fathomdb-ts`) write a single `EngineError → language-error` mapping table.
- Adding a new module = adding a new variant to `EngineError` (one-line `#[from]`); no changes to existing modules.
- `EmbedderError` is a public-trait error (per ADR-0.6.0-embedder-protocol) and surfaces through this taxonomy.
- `WriteValidationError`, `SchemaValidationError`, `EmbedderIdentityMismatchError` stay as distinct module errors per § Module-error boundary table; specific shapes live in design/engine.md, design/op-store.md, and design/embedder.md respectively.
- `EngineError::Overloaded` produced by adapter-level 429-shed (per ADR-0.6.0-projection-model § Backpressure layer 4) and engine-internal scheduler-pool saturation when no adapter is in front.
- `EngineError::Closing` produced during `Engine.close` ordered shutdown (per ADR-0.6.0-scheduler-shape § Engine.close shutdown protocol).

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-embedder-protocol (EmbedderError surface).
- ADR-0.6.0-typed-write-boundary (validation errors).
- `thiserror` standard Rust idiom.
