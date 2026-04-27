---
title: ADR-0.6.0-crate-topology
date: 2026-04-27
target_release: 0.6.0
desc: Keep fathomdb-engine monolith (+ fathomdb-schema); module boundaries via pub(crate) not crate splits
blast_radius: workspace Cargo.toml; every crates/* dir; design/*.md subsystem boundaries; compile loop; future implementer task split
status: accepted
---

# ADR-0.6.0 — Crate topology

**Status:** accepted (HITL 2026-04-27).

Phase 2 #11 architecture ADR. Decides whether 0.6.0 ships as a monolithic `fathomdb-engine` crate or splits into per-subsystem crates.

## Context

0.5.x is largely `fathomdb-engine` monolith plus `fathomdb-schema` and a few small support crates. Splitting (storage / projection / vector / query / etc.) gives clean module boundaries but adds workspace complexity, slows compile loops, and risks turning internal types into semver-stable surfaces. Decision affects every downstream implementer task.

## Decision

- **Keep monolith `fathomdb-engine`** for 0.6.0.
- **Keep `fathomdb-schema`** as a separate crate (existing split — schema migration owns its own surface).
- **Module boundaries inside `fathomdb-engine` enforced by `pub(crate)`** + the typed-write boundary (per ADR-0.6.0-typed-write-boundary). No internal types become semver-stable.
- Bindings (`fathomdb-py`, `fathomdb-ts`, `fathomdb-cli`) remain separate crates (existing); they depend on `fathomdb-engine`.

## Options considered

**A — Monolith `fathomdb-engine` + `fathomdb-schema` (chosen).** Cheapest; smallest API surface; module boundaries via `pub(crate)`; fastest compile loop. Speculative split deferred until forcing function (parallel-team development, public consumers of internal crates).

**B — Four-crate split: `-storage`, `-projection`, `-vector`, `-query`, `-engine` as facade.** Cleanest module boundaries; slowest compile; internal types become public — hard to evolve; multiplies workspace complexity. Layers-on-layers Stop-doing applies.

**C — Two-crate split: `fathomdb-engine` (sync core) + `fathomdb-async` (binding adapters).** Matches async-surface ADR shape. Modest split; doesn't expose engine internals. Considered but: bindings are already separate crates; an additional `-async` layer is redundant.

## Consequences

- Workspace stays at the current shape: `fathomdb-engine`, `fathomdb-schema`, plus binding crates.
- `design/*.md` subsystem boundaries are module-level inside `fathomdb-engine`, not crate-level.
- Net-negative LoC posture: no new crate skeletons.
- If a forcing function emerges (parallel team, public-internal-API consumer), this ADR is re-opened — splits are easier to do later than to undo.

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-typed-write-boundary (module boundary enforcement).
- Stop-doing: layers-on-layers abstractions.
- `feedback_reliability_principles` memory: net-negative LoC.
