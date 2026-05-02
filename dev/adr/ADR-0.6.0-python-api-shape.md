---
title: ADR-0.6.0-python-api-shape
date: 2026-04-27
target_release: 0.6.0
desc: Python API is sync-only with snake_case methods; asyncio users wrap in run_in_executor; no engine async surface
blast_radius: src/python/ binding source; PyO3 wrappers; interfaces/python.md; cross-cite ADR-0.6.0-async-surface; cross-cite ADR-0.6.0-typescript-api-shape
status: accepted
---

# ADR-0.6.0 — Python API shape

**Status:** accepted (HITL 2026-04-27, decision-recording — lite batch).

Phase 2 #20 interface ADR. Closure ADR — gates on
ADR-0.6.0-async-surface (Path 1, sync-everywhere except TS) and
ADR-0.6.0-typescript-api-shape (idiomatic methods + 1:1 type names).

## Context

ADR-0.6.0-async-surface decided the engine is sync everywhere except the
TS binding (which gets `Promise<T>` via Path 2 ThreadsafeFunction). That
already constrains Python to a sync surface. This ADR records the
remaining choices: naming convention, asyncio guidance, helper sugar.

ADR-0.6.0-typescript-api-shape established that **type names are shared
1:1 across bindings**, with field-name casing per language convention.
Python inherits that.

## Decision

**Sync-only Python API. Snake_case methods. Type names match Rust /
TS 1:1; field names snake_case. Asyncio users wrap calls themselves; no
`async` surface, no `_async` variants.**

- **Sync-only.** `engine.write(...)`, `engine.search(...)`,
  `engine.open(...)` all return directly. No coroutines.
- **Snake_case methods.** `engine.write_*` (binding sugar), `engine.search`,
  `engine.close`. Match `pep8`.
- **Shared type vocabulary.** `Engine`, `Embedder`, `EmbedderIdentity`,
  `WriteTx`, `Search`, `EngineError` are real Python classes with the
  same names as Rust + TS. Field names within each type are snake_case.
  **`PreparedWrite` / `NodeWrite` / `EdgeWrite` etc. are NOT exposed as
  Python classes in 0.6.0** — Python sees dict-input on the write path
  (`engine.write([{"kind": "node", ...}])`) and the binding marshals to
  the typed Rust enum internally. Promoting them to real Python classes
  is a future ADR if dict-input proves limiting.
- **Asyncio: documented escape hatch, not a binding surface.** Users put
  `await loop.run_in_executor(None, engine.write, ops)` in their own
  code; we document the pattern in `interfaces/python.md` but ship no
  `aengine`, no `engine.write_async`, no anyio glue.
- **No deprecation shims** for 0.5.x Python names (per
  ADR-0.6.0-no-shims-policy). 0.5.x callers see `ImportError` /
  `AttributeError`; that is intentional.
- **Errors as exceptions.** `EngineError` subclasses raised; never
  return `Err`-style results.
- **Type stubs (`.pyi`) ship in the wheel.** mypy / pyright users get
  full coverage at install time, not via `types-fathomdb` extra.
  Generation method (hand-written vs `pyo3-stub-gen` vs other) is an
  implementation detail tracked in `followups.md`; not load-bearing
  here.

## Options considered

**A — Sync-only, snake_case, no asyncio surface (chosen).** Pros: matches
async-surface decision; smallest surface; users that need asyncio do it
the standard library way; no second API to maintain in lockstep. Cons:
asyncio users write boilerplate (`run_in_executor`).

**B — Sync + parallel `_async` methods.** Pros: asyncio users get
ergonomic API. Cons: doubles the surface; PyO3 async wrappers spawn on
asyncio's default executor (footgun — not the engine-owned thread per
async-surface Invariant B); two test matrices; the moment one diverges
from the other we have a bug class. Rejected — speculative knobs +
fights Invariant B.

**C — Async-only Python.** Pros: forces asyncio uptake. Cons: most
Python users (incl. data-science, scripts) are sync; forces every caller
to be inside an event loop; fights default `asyncio.run` ergonomics;
contradicts ADR-0.6.0-async-surface Path 1. Rejected.

**D — Builder-style API (`engine.builder().add_node(...).commit()`).**
Pros: chainable. Cons: no longer Pythonic — list-of-dicts is the Python
norm; fights ADR-0.6.0-prepared-write-shape's `&[PreparedWrite]` slice
semantics. Rejected.

## Consequences

- `interfaces/python.md` documents the sync surface; "Common Types"
  section cross-cites `interfaces/typescript.md` (per
  ADR-0.6.0-typescript-api-shape) for the shared type vocabulary.
- `interfaces/python.md` includes a non-normative example showing
  `loop.run_in_executor` for asyncio integration; explicitly labeled
  as user-side glue, not a fathomdb feature.
- PyO3 wrappers wrap engine calls in `Python::allow_threads`, releasing
  the GIL for the duration of the call so other Python threads —
  including asyncio worker threads doing the `run_in_executor` wrap —
  make progress. Test-plan adds a CI gate: spawn a counter thread,
  assert it advances during a long `engine.write` call.
- Embedder calls from Python embedders run on the engine-owned thread
  pool per ADR-0.6.0-async-surface Invariant B; PyO3 acquires the GIL
  inside that thread for the embedder callback. Asyncio worker threads
  never run an embedder call directly.
- `_async` variants are forbidden in 0.6.x; PR adding one cites this
  ADR for rejection.
- Type stubs (`.pyi`) generated alongside wheel build; CI gate (`mypy
  --strict` against the stubs) added in test-plan.
- 0.5.x → 0.6.0 Python users see hard import / attribute errors per
  ADR-0.6.0-no-shims-policy. No `from fathomdb.legacy import ...`.

## Consequences for ADR-0.6.0-async-surface (cross-cite)

- Confirms Path 1 surface decision applies to Python.
- Invariant B remains binding: PyO3 embedder callbacks run on
  engine-owned thread, never on asyncio worker thread.
- No new invariants needed.

## Citations

- ADR-0.6.0-async-surface (Path 1; Invariant B).
- ADR-0.6.0-typescript-api-shape (1:1 type names; shared vocabulary).
- ADR-0.6.0-no-shims-policy (no 0.5.x compat layer).
- ADR-0.6.0-prepared-write-shape (slice semantics).
- HITL 2026-04-27.
