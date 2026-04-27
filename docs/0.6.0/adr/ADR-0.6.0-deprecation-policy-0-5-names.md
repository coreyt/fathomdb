---
title: ADR-0.6.0-deprecation-policy-0-5-names
date: 2026-04-27
target_release: 0.6.0
desc: 0.5.x names are not reserved; 0.6.0 reuses or drops them freely; no aliases, no warnings, no soft-removals
blast_radius: every public name across Rust / Python / TypeScript / CLI; release-policy.md; ADR-0.6.0-no-shims-policy cross-cite
status: accepted
---

# ADR-0.6.0 — Deprecation policy for 0.5.x names

**Status:** accepted (HITL 2026-04-27, decision-recording — lite batch).

Phase 2 #23 interface ADR. Closure ADR — closes against
ADR-0.6.0-no-shims-policy, which already established zero shims at the
0.5.x → 0.6.0 boundary. This ADR extends the same posture to the
**name-space** question.

## Context

ADR-0.6.0-no-shims-policy forbids deprecation shims and routing of old
verbs through new ones. It does **not** explicitly settle whether 0.5.x
names are "reserved" (avoided in 0.6.0 to prevent confusion) or freely
reusable. Decision-index #23 left this open.

A reservation policy ("don't reuse `configure_vec` in 0.6.0 because it
meant something different in 0.5.x") would carry the cognitive cost of
0.5.x indefinitely without buying any compat. The no-shims posture
already breaks 0.5.x callers; reserving names just makes 0.6.0 worse
without making 0.5.x users any happier.

## Decision

**0.5.x names are not reserved. 0.6.0 reuses, repurposes, or drops them
freely. No aliases, no `DeprecationWarning`, no `legacy_*`, no
soft-removal cycles.**

- A 0.6.0 method may carry a 0.5.x name with **different semantics** if
  the new semantics are clearer **and** the 0.6.0 signature is
  signature-incompatible with the 0.5.x call (different arity,
  different types, different return type — so the 0.5.x caller fails
  to compile / fails to import / raises immediately, not silently does
  the wrong thing). If the 0.6.0 signature *would accept* a 0.5.x call
  unchanged, the name reuse is forbidden — drop the name instead.
- A 0.6.0 method may **drop** a 0.5.x name entirely; callers see
  `AttributeError` / `ImportError` / Rust compile error.
- 0.6.0 may introduce a **new** name that happens to look like a 0.5.x
  name; no cross-checking obligation.
- No `DeprecationWarning` on import; no `__getattr__` shim; no
  module-level `legacy` namespace.
- Changelog discipline (per `no-shims-policy` consequences) makes the
  intentional break visible: 0.6.0 release notes list every removed /
  repurposed 0.5.x name in one section.

## Within-0.6.x naming

Governed entirely by ADR-0.6.0-no-shims-policy ("announced + removed in
same release"; no multi-release deprecation cycles). Whether a removed
within-0.6.x name may later be reused with new meaning is a
release-policy question — tracked in `followups.md` for resolution in
`release-policy.md`, not decided here.

## Options considered

**A — No reservation; free reuse / drop / repurpose (chosen).** Pros:
matches no-shims posture; 0.6.0 namespace optimizes for 0.6.0 clarity;
zero cognitive overhead from 0.5.x; cheapest to maintain. Cons: a 0.5.x
caller on 0.6.0 may hit confusing errors (wrong-type instead of
attribute-error). Acceptable — 0.5.x callers cannot run on 0.6.0 anyway
(fresh-DB-only).

**B — Reserve 0.5.x names; never reuse with different semantics.**
Pros: 0.5.x callers' code raises `AttributeError` cleanly when a name
is gone; never silently does the wrong thing. Cons: requires
maintaining a "do not reuse" list across all four bindings indefinitely;
constrains 0.6.0 naming for the benefit of users who already cannot run;
expands one-time pain into permanent constraint. Rejected.

**C — Reserve + warn (`DeprecationWarning` on first 0.5.x-name access
at import).** Pros: users learn what changed. Cons: requires a shim to
warn from (contradicts `no-shims-policy`); warning machinery is itself
a code path with its own bugs; users on 0.5.x have already seen the
release notes. Rejected — re-introduces shims under another name.

**D — Reserve until 0.7.** Pros: time-bounded. Cons: arbitrary deadline;
nothing happens at 0.7 to make reservation safer; just defers the
decision. Rejected.

## Consequences

- `interfaces/{rust,python,typescript,cli}.md` may freely use any name
  regardless of 0.5.x history.
- `release-policy.md` "0.6.0 changelog" section adds a "Removed /
  repurposed names" subsection with a one-line note per item; this is
  the only documentation owed to 0.5.x users.
- No CI check, no lint, no `deny(deprecated)` machinery for "you reused
  a 0.5.x name."
- Test fixtures may use any name; no 0.5.x-name-avoidance lint.
- Memory `feedback_release_verification` continues to apply — every
  removed / repurposed name's replacement gets test coverage; this ADR
  does not relax that rule.
- Cross-cite ADR-0.6.0-no-shims-policy: this ADR is the name-space
  corollary. Any future "but can we reserve $name?" request cites both.

## Non-consequences

- Does not authorize internal renames after lock — pre-freeze drafts may
  still rename; post-freeze renames are governed by Phase 4 + release
  policy.
- Does not decide naming **conventions** (snake_case vs camelCase per
  binding) — covered by ADR-0.6.0-typescript-api-shape and
  ADR-0.6.0-python-api-shape.
- Does not decide error-class names — covered by
  ADR-0.6.0-error-taxonomy.

## Citations

- ADR-0.6.0-no-shims-policy (parent posture).
- ADR-0.6.0-python-api-shape (Python naming).
- ADR-0.6.0-typescript-api-shape (TS / cross-binding naming).
- ADR-0.6.0-error-taxonomy (error names).
- HITL 2026-04-27.
