---
title: ADR-0.6.0-embedder-protocol
date: 2026-04-27
target_release: 0.6.0
desc: Embedder trait contract; reentrancy + GIL + unit-norm + timeout invariants
blast_radius: src/rust/crates/fathomdb-engine embedder dispatch layer; PyO3 binding (Python embedder bridge); napi-rs binding (TS embedder bridge); design/embedder.md; interfaces/python.md; interfaces/typescript.md; interfaces/rust.md
status: accepted
---

# ADR-0.6.0 — Embedder protocol

**Status:** accepted (HITL 2026-04-27).

This ADR records the language-agnostic contract for any `Embedder` impl
plugged into FathomDB. It exists because critic-3 (ASYNC-3) showed
that the user-supplied embedder is the deadlock vector for both Python
(GIL re-entrancy via `pyo3-log`) and TS (napi callback storms). The
protocol is sync, isolated, contractually pure, and language-agnostic.

## Context

Default embedder = candle (per ADR-0.6.0-default-embedder.md). Users
may also supply their own embedder in any binding language (Rust,
Python, TS). The engine dispatches embedder calls from a dedicated
engine-owned thread pool (Invariant B from ADR-0.6.0-async-surface.md).
Within that dispatch, the engine has zero control over what the
user's impl does. The protocol below pins what user impls must
contract to do (and not do).

## Decision

### Trait shape (Rust canonical form)

```rust
pub trait Embedder: Send + Sync {
    /// Identity of the embedding model. NEVER carried by vector configs;
    /// per memory project_vector_identity_invariant.
    fn identity(&self) -> EmbedderIdentity;

    /// Embed a single text. Returns a unit-norm `Vec<f32>` of dimension
    /// `self.identity().dimension`.
    ///
    /// Contract:
    ///   - Pure function: no callbacks into the engine.
    ///   - No logging emission via pyo3-log / tracing-subscriber inside
    ///     this call (Python impls especially).
    ///   - Sync return; impl may use async internally (e.g.
    ///     `asyncio.run(self._async_embed(text))`) but engine sees sync.
    ///   - Output L2-norm must be 1.0 ± 1e-5; engine asserts in debug
    ///     builds.
    ///   - Output length must equal `identity().dimension`.
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedderError>;
}
```

Bindings adapt this trait to language-native shapes:

- **Python**: `class Embedder: def identity(self) -> EmbedderIdentity; def embed(self, text: str) -> list[float]`. PyO3 bridge converts.
- **TypeScript**: `interface Embedder { identity(): EmbedderIdentity; embed(text: string): number[] }`. napi-rs bridge converts.

### Five mandatory invariants

**Invariant 1 — Unit-norm output.** Every vector returned by `embed()`
has L2-norm 1.0 ± 1e-5. Engine asserts in debug builds on every call;
release builds document. Required for sqlite-vec cosine semantics.

**Invariant 2 — Pure function: no engine callbacks.** Inside `embed()`,
the impl must not call back into any FathomDB engine method. No
`engine.write(...)`, no `engine.search(...)`, no `engine.query(...)`.
Violation = re-entrancy deadlock (ASYNC-2 class). Documentation +
binding-side runtime check (debug builds): per-call thread-local
`engine_in_call` flag; `embed()` body that triggers an engine call
panics in debug.

**Invariant 3 — No logging emission from inside `embed()` (Python).**
Python `Embedder.embed` impls must not emit logs via
`logging.getLogger(...).info/...` or any path that `pyo3-log` bridges
back to the Rust `log` crate, while the call is in-flight. Reason:
`pyo3-log` re-acquires the GIL while the engine-owned thread already
holds it for the embedder call → exact GIL deadlock class
(`dev/archive/pyo3-log-gil-deadlock-evidence.md`). Test fixture:
asserting embedder + `logging.basicConfig(level=DEBUG)` does not hang
on N=4 concurrent writes.

**Invariant 4 — Engine-owned thread.** Embedder dispatch is on a
dedicated thread pool owned by the engine, sized `num_cpus::get()`,
configurable per engine via `Engine.open(... embedder_pool_size: N)`.
Asyncio worker threads, libuv blocking pool, V8 main thread, JS
worker_threads — none ever run an embedder call directly. Enforced by
the bindings layer.

The pool-size override exists because embedded deployments vary
substantially: some colocate FathomDB with latency-sensitive app code
and need to cap embedder concurrency to preserve host responsiveness,
while others run bulkier local models and need to raise or lower
parallelism to match actual CPU / memory pressure. This is an
operator-facing throughput-vs-contention control, not a binding-only
escape hatch.

**Invariant 5 — Per-call timeout.** Every `embed()` call is wrapped in
a timeout (default 30s; configurable). Timeout fails the call with
`EmbedderError::Timeout`; does not corrupt writer state; cancels by
allowing the embedder thread to finish + discard, never by aborting
mid-call (no thread-cancel API in standard pthreads / Tokio joinhandle
abort is best-effort).

## Options considered

**A — Sync trait with five invariants (chosen).** Pros: smallest
contract surface; deadlock vectors closed by structural rules + tests;
language-agnostic. Cons: Python/TS users with naturally-async
embedders pay an `asyncio.run` / `await` cost per call; documented.

**B — Async trait with `async fn embed(&self, text: &str)`.** Pros:
matches modern Rust / Python / TS idiom for I/O-bound embedders. Cons:
forces tokio in the engine surface (which Option A of
ADR-0.6.0-async-surface.md explicitly rejected); doubles every binding's
adapter; reopens GIL-vs-runtime questions. Rejected.

**C — Two traits (sync + async, user picks).** Pros: flexibility. Cons:
"two shapes for one verb" — same Stop-doing pattern as 0.5.x's
`configure_vec(kind_or_embedder)`. Doubles tests, docs, dispatch
paths. Rejected.

**D — Thread-isolated impl (Python sub-interpreter per PEP 684).**
Pros: real GIL isolation across embedder calls. Cons: requires CPython
3.12+; ecosystem not ready (most pyo3 / numpy / torch don't yet
support sub-interpreters). Premature; revisit in 0.7+.

## Consequences

- `interfaces/rust.md`: documents the canonical `Embedder` trait.
- `interfaces/python.md`: documents the Python `Embedder` ABC,
  Invariants 2 + 3 + 4 + 5 spelled out with examples; explicit warning
  about `logging.basicConfig + pyo3-log` interaction.
- `interfaces/typescript.md`: documents the TS `Embedder` interface
  (sync at engine boundary; impl may use async internally via
  top-level `await` inside the engine-owned thread); cancellation TBD
  (followup).
- `design/embedder.md`: documents the dispatch layer (engine-owned
  thread pool, per-call timeout impl, debug-build assertions).
- Test plan items:
  - Python embedder + `logging.basicConfig(level=DEBUG)` + 4
    concurrent writes does not hang (regression test for the 0.5.x
    `pyo3-log-gil-deadlock-evidence` class).
  - Embedder returning non-unit-norm vector triggers debug-build
    assertion (and a typed error in release).
  - Embedder calling back into engine triggers debug-build re-entrancy
    panic.
  - Embedder timeout (mock embedder sleeping > 30s) returns typed
    `EmbedderError::Timeout`; does not corrupt subsequent writes.
- Stop-doing: enforces "embedder owns identity" (Invariant in
  `dev/notes/project-vector-identity-invariant.md`); embedders never
  carry identity strings on vector configs.

## Citations

- HITL decision 2026-04-27 (Path 2 + Path 1 invariants from
  ADR-0.6.0-async-surface.md).
- Critic-3 ASYNC-3 (GIL re-entrancy class) + EMB-2 (L2-norm enforcement)
  - EMB-4 (engine-owned thread) + EMB-1 (mean-pool decision).
- Memory `project_vector_identity_invariant`.
- Memory pattern from `dev/archive/pyo3-log-gil-deadlock-evidence.md`,
  commits `cf0b190`, `d09deb4`.
