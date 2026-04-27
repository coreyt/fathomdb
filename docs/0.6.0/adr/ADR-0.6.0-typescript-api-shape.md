---
title: ADR-0.6.0-typescript-api-shape
date: 2026-04-27
target_release: 0.6.0
desc: TS API — idiomatic methods (camelCase, Promise) with type names shared 1:1 across bindings
blast_radius: ts/ binding source; interfaces/typescript.md; interfaces/python.md (cross-ref); ADR-0.6.0-async-surface Path 2; napi-rs binding code
status: accepted
---

# ADR-0.6.0 — TypeScript API shape

**Status:** accepted (HITL 2026-04-27).

Phase 2 #21 interface ADR. Decides TS binding philosophy.

## Context

Two binding philosophies. Mirror = same verb names, signatures, error shapes as Python (1:1). Idiomatic = TS conventions (camelCase, Promise everywhere, interface literals). Already constrained by ADR-0.6.0-async-surface Path 2: TS surface is Promise-returning.

## Decision

**Idiomatic TS methods + type names shared 1:1 across bindings.**

- **Method names + shape:** TS conventions. `engine.write(...)` not `engine.write_node(...)`; camelCase; `Promise<T>` per Path 2; errors as exceptions; options as `interface` literals.
- **Type names:** shared across Python and TS. `Engine`, `Embedder`, `WriteTx`, `Search`, `EmbedderIdentity`, `EngineError` are the same names in both bindings. Field names within types follow each language's casing convention (Python snake_case, TS camelCase).
- **Cross-binding portability:** users moving between bindings know the type vocabulary; only call-site idioms differ.

## Options considered

**A — Idiomatic TS, ignore cross-binding.** camelCase methods, Promise returns, interface options. Fights neither TS nor Python conventions. Cross-binding users translate vocabulary as well as syntax.

**B — Mirror Python 1:1.** Same names, snake_case, sync-feeling. Lowest cross-binding learning curve; fights TS conventions; surprises TS users; loses idiomatic napi-rs alignment.

**C — Idiomatic methods + 1:1 type names (chosen).** Type names portable cross-binding; method shape stays native. Best of both. Matches async-surface Path 2 (TS gets Promise<T> per language convention).

## Consequences

- `interfaces/typescript.md` documents the TS surface.
- `interfaces/python.md` and `interfaces/typescript.md` share a "Common Types" section listing the type names that appear identical across bindings.
- napi-rs binding code uses idiomatic TS naming on the JS side.
- TS field-name convention (camelCase) is documented; PyO3 bridge uses snake_case on the Python side.
- Cross-cite ADR-0.6.0-embedder-protocol § Trait shape: `Embedder`, `EmbedderIdentity`, `EmbedderError` are the shared type names.
- Cross-cite ADR-0.6.0-async-surface Path 2: TS Promise returns confirmed here.

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-async-surface (Path 2 — TS Promise<T>).
- ADR-0.6.0-embedder-protocol (cross-binding type shape).
