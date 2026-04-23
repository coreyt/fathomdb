# Auto-drain error tracing

**Date:** 2026-04-22
**Status:** Design — ready to implement
**Parent release:** managed vector projection (branch `design-db-wide-embedding-per-kind-vec`)
**Targets pack:** H.1 (follow-up to Pack H)

## Context

`Engine::auto_drain_vector_work` at `crates/fathomdb/src/lib.rs:401-412` swallows the result of
`drain_vector_projection` with `let _ = ...`. When `EngineOptions::auto_drain_vector=true`
(test-mode sync drain introduced in Pack H), any failure — embedder error, SQLite contention,
timeout — silently suppresses projection work. The downstream symptom is "semantic_search
returned empty" with no log breadcrumb, which burned review time during Pack H.

The crate does NOT directly depend on `tracing`; it exposes a `tracing` feature that forwards
to `fathomdb-engine/tracing`. The engine-level pattern is
`crates/fathomdb-engine/src/trace_support.rs` (macros `trace_warn!`, `trace_error!`, etc.
gated by `#[cfg(feature = "tracing")]`). The `fathomdb` crate currently has no call sites
using tracing directly.

## Design

Add a single `warn!`-level log on drain failure, gated by `#[cfg(feature = "tracing")]`.

Two options:

**Option A (minimal, recommended):** inline `#[cfg(feature = "tracing")]` guard at the call site
in `lib.rs`. No new macros in the `fathomdb` crate — we already pass the feature through.

```rust
let outcome = self
    .admin()
    .service()
    .drain_vector_projection(&adapter, std::time::Duration::from_secs(30));
#[cfg(feature = "tracing")]
if let Err(err) = &outcome {
    tracing::warn!(
        target: "fathomdb::auto_drain",
        timeout_ms = 30_000u64,
        error = %err,
        "auto_drain_vector_work: drain_vector_projection failed (test-mode)",
    );
}
let _ = outcome;
```

Add `tracing` as an optional direct dep in `crates/fathomdb/Cargo.toml` under the existing
`tracing` feature (`tracing = { workspace = true, optional = true }` + feature entry).

**Option B:** re-export the engine's `trace_warn!` macro into `fathomdb`. More ceremony for
one call site; reject unless a second site appears.

### Event level

`warn!` — auto-drain is a best-effort test convenience; a failure is recoverable (the write
committed; projection work is queued and will be drained on the next call or by a background
drain). `error!` would over-escalate.

### Structured fields

- `timeout_ms = 30_000`
- `error = %err` (display)
- target `"fathomdb::auto_drain"` so subscribers can filter

No rate-limiting. Test-mode only; noise is tolerable and every failure is interesting.

### Work-count fields

`drain_vector_projection` returns a report on success. On failure we only have the error. Do
NOT synthesize per-kind counts in the warn path — the signal is "drain failed," not "drain
partial." If future observability needs partial counts, add a second `info!` on the success
arm.

## Test plan

Single test in `crates/fathomdb/tests/auto_drain_vector.rs` (existing file — see
`EmbedderChoice::InProcess` use at line 107):

- Configure an embedder that returns `Err(EmbedderError::...)` for `batch_embed`.
- Open engine with `auto_drain_vector=true` and that embedder.
- Submit a write that enqueues vector projection work.
- Assert `submit_write` still returns `Ok` (warn is non-fatal).

For the tracing assertion itself: use `tracing-subscriber::fmt::TestWriter` behind
`#[cfg(feature = "tracing")]`, or gate the assertion on the `tracing` feature and use
`tracing-test` crate (not currently a dep — prefer the former to avoid dep churn). If
dep churn is unacceptable, make the test a behaviour-only test (write succeeds,
semantic_search returns empty) and document the warn path in a code comment referencing
this design doc. Recommend: ship without a tracing-assertion test; the log is obvious
in manual repro.

## Scope guardrails

- Do NOT change the `Engine::submit_write` contract (still returns `Ok` if the write itself
  committed; drain failure does not propagate).
- Do NOT add retry logic; a failing auto-drain is a test signal, not something to recover from.
- Do NOT log on the success arm in this pack.
- Do NOT broaden to non-test callers (`auto_drain_vector=false` takes a different path — the
  background drain actor — which has its own error handling).

## Followups / open questions

- Should the background (non-test) drain actor also log? Out of scope — already covered by
  `drain_vector_projection`'s internal tracing in the engine.
- `tracing-test` dep adoption is a cross-crate discussion; skip here.

### Critical files for implementation

- crates/fathomdb/src/lib.rs
- crates/fathomdb/Cargo.toml
- crates/fathomdb/tests/auto_drain_vector.rs
