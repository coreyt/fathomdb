---
title: ADR-0.6.0-sqlite-vec-acceptance
date: 2026-04-25
target_release: 0.6.0
desc: Accept sqlite-vec sole-maintainer risk; no fallback plan; no vendored fork
blast_radius: deps/sqlite-vec.md; vector index path in fathomdb-engine
status: accepted
---

# ADR-0.6.0 — sqlite-vec acceptance (no fallback)

**Status:** accepted (HITL 2026-04-25, decision-recording)

## Context

`sqlite-vec` (asg017) is the only ANN that integrates as a SQLite virtual
table — meaning it is the only option that preserves the single-file
embedded-DB invariant. Critic-B F3 flagged the keep verdict as soft:
sole-maintainer risk; no fallback plan; no vendored fork; no upstream
activity check.

## Decision

**Accept the sole-maintainer risk. No fallback plan. No vendored fork.
No quarterly activity check.**

If asg017 abandons the project AND no community fork picks it up, the
issue is re-opened in a future release — not pre-mitigated in 0.6.0. The
cost of pre-mitigation (vendoring, dual maintenance, watching) outweighs
the probability of needing the fallback within the 0.6.0 lifecycle.

## Options considered

**A. Accept (chosen).** Pros: zero ongoing maintenance cost; reflects
honest probability assessment. Cons: if asg017 disappears, 0.6.0+ users
have no fix path until a fork or replacement appears.

**B. Vendored fork from day one.** Pros: bus-factor mitigation. Cons:
ongoing sync work; we become a co-maintainer of an extension we have no
business co-maintaining; doubles the C-extension surface to debug.

**C. Quarterly activity check + standby fork.** Pros: low cost to start.
Cons: no actual fork until needed; "watching" is the kind of speculative
process that gets dropped under load (Stop-doing on speculative knobs);
HITL preferred a single decision over a process.

**D. Switch to non-VT alternative (usearch / hnswlib bindings + manual
persistence).** Pros: more maintainers. Cons: breaks the single-file
invariant; ~3k LoC engine rewrite; loses transactional vector writes.
Architectural step backwards.

## Consequences

- `deps/sqlite-vec.md` records the acceptance.
- No new monitoring code, no `sqlite-vec-vendored` crate, no scripts
  watching upstream releases.
- If asg017 abandons: that is a future release problem. 0.6.0 ships as-is.

## Citations

- HITL decision 2026-04-25 (deps F3 resolution).
- `dev/deps/sqlite-vec.md`.
