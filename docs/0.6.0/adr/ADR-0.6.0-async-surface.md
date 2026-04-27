---
title: ADR-0.6.0-async-surface
date: 2026-04-27
target_release: 0.6.0
desc: Async-surface posture for engine API across Rust / Python / TypeScript / CLI
blast_radius: every public API on every binding; rusqlite usage; bindings (PyO3, napi-rs); Python and TS wrapper layers
status: accepted
---

# ADR-0.6.0 — Async surface

**Status:** accepted (HITL 2026-04-27 — Path 2 + Path 1 invariants).

This is a **deliberation** ADR (not decision-recording). It was promoted
from Phase 2 to Phase 1 by HITL F4 because the choice frames every
public 0.6.0 API surface; deciding it after `interfaces/*.md` were drafted
would mean rewriting them. Critic-3 (architecture-inspector) attacked
Option A on 4 axes (ASYNC-1/2/3 + cross-ADR X-1); HITL chose **Path 2**
(structural fix to TS binding) plus **Path 1** invariants (A–D below).

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

**Sync engine API (Option A core) + Path 2 TS-binding fix + Path 1
invariants A–D.**

- Engine public surface is sync everywhere (Rust, Python, CLI); async
  only on the TS binding (Promise-returning).
- TS binding does **not** use napi-rs's default blocking-task pool
  (4-thread libuv). Instead: `ThreadsafeFunction` + a Rust-owned
  thread pool sized at `num_cpus::get()` (configurable per engine).
  Decouples engine work from libuv's `fs/dns/crypto` contention.
- Four Invariants (mandatory; tested):
  - **Invariant A — Scheduler post-commit.** Vector-projection scheduler
    tasks always run after the originating write commits. Writer lock
    is released before any scheduler dispatch. Prevents the
    re-entrancy deadlock class (ASYNC-2).
  - **Invariant B — Engine-owned embedder thread.** All embedder calls
    (Rust, Python, TS — any language) run on a dedicated engine-owned
    thread pool. Asyncio worker threads / libuv threads / V8 main
    thread never run an embedder call directly.
  - **Invariant C — Embedder-protocol no-reentrancy.** `Embedder.embed`
    is a pure function in contract: no calls back into the engine, no
    `pyo3-log` / Python logging emission from within `embed()`, no
    napi callbacks back to JS during a single `embed()` execution.
    Detailed in ADR-0.6.0-embedder-protocol.md.
  - **Invariant D — Eager model warmup.** Default-embedder model is
    loaded into memory at `Engine.open`. Cold-load is forbidden inside
    any write transaction. Plus per-embedder timeout (default 30s) on
    `embed()`.

Internal scheduler (Phase 2 ADR #14) still uses Arc/async — that is
implementation, not surface.

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

## Critic findings + resolutions (2026-04-27)

Critic-3 raised four HIGH severity attacks; HITL resolved each:

- **ASYNC-1** napi-rs default blocking pool is libuv's 4-thread pool —
  contends with `fs/dns/crypto`. **Fixed by Path 2:** TS binding owns
  its own thread pool via `ThreadsafeFunction`; decoupled from libuv.
- **ASYNC-2** Sync writer holding writer lock while async scheduler
  task tries to re-acquire it = deadlock. **Fixed by Invariant A:**
  scheduler dispatches post-commit; writer lock released before any
  scheduler work begins.
- **ASYNC-3** Python embedder protocol: GIL re-entrancy via user-supplied
  embedder + pyo3-log = exact deadlock class from
  `dev/archive/pyo3-log-gil-deadlock-evidence.md`. **Fixed by Invariants
  B + C:** embedder runs on engine-owned thread (not asyncio worker);
  embedder protocol forbids logging emission and reentrant engine
  calls.
- **X-1** First write triggers HF-Hub model cold-load while writer
  lock is held → seconds of stall. **Fixed by Invariant D:** model is
  loaded eagerly at `Engine.open`; cold-load inside write tx is a
  startup-time error, not a runtime hazard.

## Why not Option C (sqlx)

Considered and rejected:

1. Conflicts with already-accepted ADRs (rusqlite keep, sqlite-vec keep
   with no fallback, default-embedder in-process).
2. ~5–8k LoC engine rewrite; sqlite-vec integration via sqlx is
   non-trivial.
3. MVCC / concurrent-readers future work is a separate decision in
   Phase 2 (#12) — does not require sync-vs-async to be resolved now.
4. Path 2 + invariants give us per-binding async **where it matters**
   (TS) without the architectural cost.

If Phase 2 MVCC ADR ever requires async-native engine, this ADR is
re-opened then.

## Why not Option B (per-binding sync+async dual surfaces)

Doubles every binding's public surface. "Two shapes for one verb" is
the exact Stop-doing pattern the rewrite is supposed to delete (see
0.5.x `configure_vec(kind_or_embedder)`). Rejected.

## Consequences

- `interfaces/rust.md`: sync API only. No `async` keyword on engine
  methods. Embedder protocol = sync trait.
- `interfaces/python.md`: sync API only. Document `loop.run_in_executor`
  pattern for asyncio users; document Invariant B + C constraints on
  user-supplied `Embedder` impls. Tests verify a buggy Python embedder
  emitting a log line is caught at install/runtime.
- `interfaces/typescript.md`: `Promise<...>`-returning API via
  `ThreadsafeFunction` + Rust-owned thread pool; pool sized
  `num_cpus::get()` configurable per engine. Document cancellation
  semantics (initially: not supported; followup).
- `interfaces/cli.md`: sync.
- Engine internals retain freedom to use Arc/async (vector projection
  scheduler, embedder worker thread); bindings see sync.
- Embedder protocol details land in **ADR-0.6.0-embedder-protocol.md**
  (Invariant C semantics + per-language constraints).
- `Engine.open` blocks until default-embedder model is loaded into
  memory (Invariant D). open's status reporting includes model-load
  duration.
- Per-`embed()` timeout: default 30s; configurable. Timeout fails the
  embed call with a typed error; does not corrupt the writer.
- Cancellation, MVCC future-proofing, alternative embedder lifecycle
  hooks → Phase 2+ followups.

## Citations

- HITL F4 promotion decision 2026-04-25.
- `docs/0.6.0/deps/rusqlite.md` (keep verdict; sync constraint).
- `docs/0.6.0/deps/sqlite-vec.md` (keep verdict; constrains away from sqlx).
- Stop-doing: layers-on-layers configure verbs (informs "no two shapes
  per verb").
