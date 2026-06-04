---
title: Slice 25 — Governed-surface conformance rewrite (design memo)
date: 2026-06-04
target_release: 0.8.0
desc: Design-first memo for the AC-057a→AC-074 conformance rewrite. Records the governed allowlist, the four falsifiable properties the rewritten suites enforce, the touch-points, and the honest-green approach for the not-yet-live read.* members.
status: design-note
---

# Slice 25 — governed-surface conformance design

The governing ADR `ADR-0.8.0-supersede-five-verb-surface-cap.md` is **SIGNED /
ACCEPTED (HITL 2026-06-03)**: **Q1=A1, Q2=B1, Q3=amend, Q4=confirm, Q5=BIND-RUST**.
This slice executes the Py+TS conformance rewrite the signed ADR authorizes. It is a
docs + test slice — **no feature code, no engine/schema/SDK-runtime change**. The
`read.*` verbs do **not** go live until Slice 30; here they are asserted only at the
**documented-allowlist** level, so the suites are **honestly green now**.

## The governed allowlist (single source of truth)

The allowlist is the set of **application-command callables** across the Python +
TypeScript SDKs (B1 `read.*` namespace):

```text
Core (live today, unchanged):  Engine.open  admin.configure  write  search  close
Read surface (B1 read.*,       read.get  read.get_many  read.collection  read.mutations
  ships 0.8.0 but goes LIVE
  at Slice 30 — documented-
  allowlist members now, NOT
  live symbols)
```

**Not allowlist members** (preserved from AC-057a's measurement): public data types,
config types, error classes, and the engine-attached instrumentation/control methods
(`drain`, `counters`, `set_profiling`/`setProfiling`,
`set_slow_threshold_ms`/`setSlowThresholdMs`,
`attach_logging_subscriber`/`attachSubscriber`). These are NOT application commands.

The recovery denylist is `{recover, restore, repair, fix, rebuild}` (FIVE names).
`doctor` is SDK-absent by **non-membership in the positive allowlist** (it is a CLI
verb), NOT by the recovery denylist.

## The four falsifiable properties the rewrite enforces

- **P1 — allowlist-membership.** Every *live* public application command in Py and in
  TS is a member of the governed allowlist. Because the allowlist is a **superset**
  (it includes the not-yet-live `read.*` verbs), live-surface ⊆ allowlist is the
  honest assertion today; the four core commands per binding are additionally asserted
  to be present.
- **P2 — cross-binding parity.** The Python `GOVERNED_SURFACE_ALLOWLIST` == the
  TypeScript `GOVERNED_SURFACE_ALLOWLIST` (membership-identical). Each binding declares
  the allowlist once as a named constant so the two are trivially diff-able and the
  25.b audit can byte-compare them.
- **P3 — recovery denylist empty-intersection.** allowlist ∩
  `{recover,restore,repair,fix,rebuild}` = ∅. This is an allowlist-level assertion only;
  the byte-frozen `test_no_recovery_surface.{py,ts}` / `no_recovery_surface.rs` remain
  the live enforcement of unreachability and stay **byte-unchanged**.
- **P4 — no-raw-SQL boundary.** No public SDK entrypoint accepts raw SQL; reads use
  typed args + a small fixed filter grammar (equality + range over body-JSON). Asserted
  at the spec/documented level (AC-074); no new runtime in this slice.

## Honest-green approach for the not-yet-live `read.*` members

The `read.*` verbs are documented-allowlist members but **not live symbols** until
Slice 30. The rewritten suites therefore assert:

- P1 as **membership** (live ⊆ allowlist), never equality — so a live surface that is
  currently the core five is honestly green against a 9-member allowlist.
- P2/P3 over the **allowlist constant** (a documented set), which legitimately contains
  the `read.*` names now.

No test asserts a `read.*` symbol is importable/callable; that lands at Slice 30.

## Touch-points

- **RED (tests):** `src/python/tests/test_surface.py`,
  `src/ts/tests/surface.test.ts` — replace cap/presence assertions with the
  `GOVERNED_SURFACE_ALLOWLIST` constant + membership + parity; bind AC-074/REQ-053.
- **GREEN (specs):** `dev/acceptance.md` (AC-057a superseded; new AC-074; trace row
  `:1183` repoint; AC-035d parenthetical repoint), `dev/requirements.md` (REQ-053
  amend in place), `dev/design/bindings.md` §1/§13/§14 (governed-surface invariant;
  **§10 byte-frozen**), `dev/interfaces/rust.md` (AC-057a→AC-074 + Slice-27 forward
  note).
- **Verify-only / byte-frozen:** the three recovery suites + `bindings.md` §10 + the
  signed ADR (no re-author).
- **X3:** `dev/DOC-INDEX.md` rows refreshed.

## Deferred (NOT here)

- Rust-facade positive-allowlist pin + substantive `rust.md` governed-surface docs →
  **reserved-gap Slice 27** (Q5=BIND-RUST). Here: only record that AC-074 binds Rust +
  repoint the stale `rust.md` AC reference with a Slice-27 forward note.
- Live `read.*` verbs / engine reader → **Slice 30** (G2/G3).
- No namespace sweep (B1 chosen), no traceability cascade (Q3=amend).
