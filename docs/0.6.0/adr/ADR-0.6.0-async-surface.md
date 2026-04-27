---
title: ADR-0.6.0-async-surface
date: 2026-04-25
target_release: 0.6.0
desc: Async-surface posture for engine API across Rust / Python / TypeScript / CLI
blast_radius: every public API on every binding; rusqlite usage; bindings (PyO3, napi-rs); Python and TS wrapper layers
status: proposed
---

# ADR-0.6.0 — Async surface

**Status:** proposed — pending HITL decision

This is a **deliberation** ADR (not decision-recording). It was promoted
from Phase 2 to Phase 1 by HITL F4 because the choice frames every
public 0.6.0 API surface; deciding it after `interfaces/*.md` were drafted
would mean rewriting them.

## Context

`rusqlite` is sync. The engine's writer is single-threaded and blocking
by design (see Stop-doings on writer-thread-safety patches and
SQLITE_SCHEMA flooding). Reads are also currently sync. The question is
how the public API surfaces this to:

- **Rust callers** that may live in either sync or async runtimes,
- **Python callers** (default sync, but `asyncio` users exist),
- **TypeScript callers** (always async; Node has no synchronous I/O
  pattern for libraries),
- **CLI** (one-shot processes; sync is fine).

The decision interacts tightly with the scheduler ADR (Phase 2 #14),
which specifies that the vector projection scheduler uses Arc/async and
must surface "vec-not-yet-consistent" to clients. The two ADRs must
agree.

## Decision

**Pending HITL.** Three options below; recommendation at the bottom.

## Options considered

**A. Sync-only engine; bindings expose sync (Rust + Python + CLI) and
spawn-blocking adapters (TypeScript).**
- Engine API: `Engine::write(...) -> Result<...>` — fully sync.
- Python: `engine.write(...)` returns directly. Python `asyncio` users
  call from `loop.run_in_executor()` themselves.
- TypeScript: napi-rs adapter wraps every call in `napi`'s blocking-task
  pool; binding presents `async write(...)` returning `Promise<...>`.
- CLI: trivially sync.
- Internal scheduler can still use Arc/async — that is implementation,
  not surface.

Pros: smallest engine surface; matches what `rusqlite` actually is;
simplest correctness story; least binding code on Python and Rust
(where most consumers live); typescript path is the only one that needs
adapter code, and napi-rs handles it idiomatically. The Python `asyncio`
usage pattern (`run_in_executor`) is well-understood and used across
rusqlite-fronted Python projects.

Cons: Python `asyncio`-heavy users get a slightly less ergonomic API
than a native `async def` would feel. They can wrap once in their own
helper.

**B. Sync engine + async wrapper layer per binding.**
- Engine: sync.
- Rust: `engine_async` module wraps every fn in `tokio::task::spawn_blocking`.
- Python: ships both `engine.write_sync(...)` and `engine.write_async(...)`.
- TypeScript: native async via napi (same as A).

Pros: native-feeling async on every binding without forcing users into
spawn-blocking. Cons: doubles every binding's public surface; doubles
docs; doubles tests; binding maintainers must keep two paths in sync;
"two shapes for one verb" is the Stop-doing the rewrite is supposed to
remove.

**C. Async-native engine via `sqlx`-on-sqlite (or equivalent).**
- Engine API: `async fn write(...) -> Result<...>`.
- Python: native `async def`; sync wrappers via `pyo3-asyncio`.
- Rust: native async.
- TypeScript: native async (same as A and B).
- CLI: sync wrapper around `block_on`.

Pros: most ergonomic API across all bindings. Cons: 5–8k LoC engine
rewrite; sqlx loses extension-loading parity (sqlite-vec integration is
non-trivial to recover); transaction semantics differ from rusqlite;
every Stop-doing the rewrite already chose against (`rusqlite` keep,
sqlite-vec keep with no fallback) gets re-opened. Net negative for a
rewrite that explicitly wants smaller surface.

## Recommendation

**Option A.** Reasons:

1. Smallest surface area; matches the underlying tool (`rusqlite`).
2. Avoids the "two shapes for one verb" Stop-doing class that B
   re-introduces.
3. Avoids the architectural cost of C, which would conflict with the
   already-accepted ADRs on rusqlite (keep), sqlite-vec (keep, accept
   sole-maintainer risk), default-embedder (in-process candle).
4. The TypeScript binding is the only one that needs adapter code, and
   napi-rs's blocking-task pool is the standard pattern for this exact
   case.
5. Python `asyncio` users can write a 5-line wrapper around
   `loop.run_in_executor` — a tax that affects a small fraction of users
   without forcing the cost on everyone else.

The internal vector-projection scheduler (Phase 2 ADR #14) still uses
Arc/async — that is independent of the public surface posture chosen
here.

## Consequences (assuming A)

- `interfaces/rust.md`: sync API only. No `async` keyword on engine
  methods.
- `interfaces/python.md`: sync API only; example showing
  `loop.run_in_executor` for asyncio users.
- `interfaces/typescript.md`: native `Promise<...>`-returning API via
  napi-rs blocking-task pool.
- `interfaces/cli.md`: sync.
- Engine internals retain freedom to use Arc/async (e.g. for vector
  projection scheduler, embedder worker thread); bindings see sync.

## Citations

- HITL F4 promotion decision 2026-04-25.
- `docs/0.6.0/deps/rusqlite.md` (keep verdict; sync constraint).
- `docs/0.6.0/deps/sqlite-vec.md` (keep verdict; constrains away from sqlx).
- Stop-doing: layers-on-layers configure verbs (informs "no two shapes
  per verb").
