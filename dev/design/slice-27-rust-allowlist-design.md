---
title: Slice 27 design memo — Rust-facade governed-surface positive-allowlist / parity pin
date: 2026-06-05
target_release: 0.8.0
owning_slice: 27 (Q5 = BIND-RUST; fills AC-074 Rust-facade measurement)
status: accepted
desc: >
  The curated governed Rust-facade allowlist, its partition from the operator-seam
  re-exports, the three load-bearing properties (P1 positive-allowlist · P2
  parity-in-intent · P3 recovery-denylist-absence), how AC-074's Rust-facade
  measurement reads, and why this mirrors the best-effort no_recovery_surface.rs
  style (no Rust runtime symbol introspection).
---

# Slice 27 — Rust-facade governed-surface allowlist/parity pin

## 0. Why this slice exists

`ADR-0.8.0-supersede-five-verb-surface-cap` was SIGNED 2026-06-03 with **Q5 =
BIND-RUST** — a deliberate deviation from the recommended SDK-only governance.
HITL elected to bind the **Rust facade** (`dev/interfaces/rust.md`), not just the
Python + TypeScript SDKs, into the governed-surface AC (AC-074). Slice 25 landed
the Py+TS allowlist/parity rewrite and *recorded* that AC-074 also binds Rust,
forward-referencing a reserved-gap **Slice 27** for the Rust half. This memo is
that half: the Rust-facade positive-allowlist / parity pin + the fill of AC-074's
`:906` "Rust-facade measurement is defined at Slice 27" clause.

It is **additive governance**: it pins behavior the facade *already* satisfies so
it cannot silently regress. It does not block Slice 30 and changes no engine or
schema code.

## 1. The Rust facade is a *different consumer contract* than the Py/TS SDK

The Python/TypeScript governed surface is a set of **application-command verbs**
(`{Engine.open, admin.configure, write, search, close, read.get, read.get_many,
read.collection, read.mutations}`) — introspectable at runtime via `dir()` /
`__all__`. The Rust facade is **not** shaped that way:

- The application verbs are **methods on `Engine`** (`Engine::open/write/search/
  close`), not free functions, so there is no free-verb set to enumerate.
- The facade's public surface is a set of **types** re-exported from
  `fathomdb-engine` (`pub use fathomdb_engine::{…}` in
  `src/rust/crates/fathomdb/src/lib.rs`).
- **Rust has no runtime symbol-table introspection** for a crate (no
  `dir(module)` equivalent; `compile_fail` doctests run only for items under
  `src/`, not `tests/`).

So we do **not** force the Py/TS 5-verb+`read.*` SDK allowlist onto Rust (P2 is
*parity-in-intent*, not membership-identity). The Rust governed allowlist is the
**typed application surface `dev/interfaces/rust.md` owns** — and the pin is the
established Rust-facade conformance style: a **compile-time resolves-check**
(`std::any::type_name::<…>()` over the allowlisted types) **plus** a
source-inspection-documented contract — exactly how `reexports.rs` and the
byte-frozen `no_recovery_surface.rs` already work.

## 2. The partition — governed application surface vs operator-seam re-exports

`lib.rs` re-exports **37** symbols. Not all of them are *governed application
surface*: the facade also legitimately re-exports recovery / integrity / dump
**report types** purely so `fathomdb-cli` (the only public consumer of those
types) compiles against the public Rust surface, not engine internals. Per
`rust.md` § "Recovery / operator seam re-exports" + § "Non-presence", those are
CLI-only ergonomic types — present as compile-time symbols, **NOT** runtime SDK
verbs. The governance question is therefore *partition*, not *count*.

### 2a. Governed application-surface allowlist (17) — `GOVERNED_SURFACE_ALLOWLIST`

The typed surface a Rust consumer (Memex / Hermes / OpenClaw) programs against —
the contract `rust.md` § "Public surface" / § "Caller-visible data shapes" / §
"Errors" / the open-diagnostics + instrumentation sections own:

| # | Symbol | Role |
|---|--------|------|
| 1 | `Engine` | carries the `open/write/search/close` verbs + instrumentation methods |
| 2 | `OpenedEngine` | open result (`engine` + `report`) |
| 3 | `OpenReport` | open-time report shape |
| 4 | `WriteReceipt` | write result (`cursor`) |
| 5 | `SearchResult` | search result (`projection_cursor` + hits) |
| 6 | `PreparedWrite` | typed-write boundary input |
| 7 | `EngineError` | typed runtime error |
| 8 | `EngineOpenError` | typed open error |
| 9 | `CorruptionDetail` | open-path diagnostic |
| 10 | `CorruptionKind` | open-path diagnostic |
| 11 | `CorruptionLocator` | open-path diagnostic |
| 12 | `OpenStage` | open-path diagnostic |
| 13 | `RecoveryHint` | open-error hint **type** (NOT a recovery verb — see §3) |
| 14 | `SoftFallback` | retrieval soft-fallback record |
| 15 | `SoftFallbackBranch` | retrieval soft-fallback branch enum |
| 16 | `CounterSnapshot` | instrumentation payload |
| 17 | `Subscription` | host-subscriber attachment handle |

### 2b. Operator-seam re-exports (20) — present, but NOT governed application surface

`CheckIntegrityOpts`, `IntegrityReport`, `SafeExportArtifact`, `TraceReport`,
`TraceEvent`, `RebuildReport`, `RebuildKind`, `ExciseReport`,
`VerifyEmbedderReport`, `VerifyEmbedderStatus`, `DumpSchemaReport`,
`SchemaObject`, `DumpRowCountsReport`, `TableRowCount`, `DumpProfileReport`,
`TruncateWalReport`, `TruncateWalStatus`, `Finding`, `MeanRecomputeReport`,
`Section`.

These are the Rust analogue of "recovery is CLI-only, not an SDK verb": their
backing `Engine` methods are owned by `design/recovery.md` and exist for
`fathomdb-cli`. `reexports.rs` already proves they *resolve*; this slice records
that they are **deliberately excluded** from the governed application allowlist.
17 + 20 = 37 = the full `lib.rs` re-export set (no symbol unaccounted for).

## 3. The three load-bearing properties

- **P1 — positive allowlist.** Every member of `GOVERNED_SURFACE_ALLOWLIST`
  resolves through the `fathomdb` facade (compile-time `type_name::<…>()`). If
  someone deletes a governed type from the facade, the test stops compiling.

- **P2 — parity-in-intent (NOT membership-identity).** The Rust governed surface
  is *posture-consistent* with the Py/TS governed surface — a governed allowlist,
  recovery-denylist-absent, typed / no-raw-SQL — but it is a **different consumer
  contract** (a type set, not a free-verb set). The one element that is genuinely
  shared across all three bindings is the recovery denylist, declared **once** in
  `src/conformance/governed-surface-allowlist.json` (`recovery_denylist`); the
  Rust test pins the same five names and points at that single source so the
  lineage is explicit. We do **not** assert membership-equality with the Py/TS
  verb allowlist (that would be a category error).

- **P3 — recovery-denylist absence (verb-level).** No governed-surface symbol
  *is* a recovery verb in `{recover, restore, repair, fix, rebuild}`. The match
  is **exact, case-insensitive** — deliberately *not* substring — because the
  denylist targets recovery **verbs**, whereas `RecoveryHint` is a typed
  open-error hint and `RebuildReport`/`RebuildKind` are operator-seam report
  *types* (and are not even in the governed allowlist). A substring rule would
  false-positive on `RecoveryHint`; the verb-level rule is the correct contract
  and mirrors the Py/TS P3 set-intersection (`allowlist ∩ {recover,…} = ∅`). The
  **canonical** denylist enforcement remains the byte-frozen
  `no_recovery_surface.rs` (Rust) / `test_no_recovery_surface.{py,ts}`; this
  slice adds the *positive* allowlist half + an allowlist-scope denylist check.

## 4. Test design — `tests/governed_surface.rs`

A new sibling to `reexports.rs` / `no_recovery_surface.rs` (kept unchanged). It:

1. Declares `GOVERNED_SURFACE_ALLOWLIST` (the 17 names) + `RECOVERY_DENYLIST`
   (the five, identical to the shared JSON `recovery_denylist`).
2. **P1**: 17 explicit `type_name::<fathomdb::T>()` resolves-checks, and asserts
   the allowlist length is 17 so the const and the resolves-checks stay in lock-step.
3. **P3**: asserts `denylist_hits(GOVERNED_SURFACE_ALLOWLIST, RECOVERY_DENYLIST)`
   is empty (exact, case-insensitive).
4. **Non-vacuous guard** (Slice-25 vacuous-green lesson, `conformance-rewrite-
   vacuous-green-trap`): a meta-test feeds a *poisoned* allowlist containing a
   denylist verb into the **same** `denylist_hits` helper and asserts it is
   flagged — proving the P3 detector actually bites and can never pass vacuously.

The test's doc comment binds **AC-074** (Rust-facade measurement) — **not** a new
AC id. **RED demonstration**: the first commit lands the test with a deliberate
`"rebuild"` injected into `GOVERNED_SURFACE_ALLOWLIST`, so `cargo test -p
fathomdb` fails the P3 assertion (the catch, demonstrated in RED per the project's
conformance discipline); the GREEN commit removes the injection.

## 5. Doc/AC alignment (GREEN)

- `dev/interfaces/rust.md`: rewrite § Support posture + add a § governed-surface
  contract so it states the **landed** governed Rust allowlist + parity-in-intent
  + denylist-absence as binding (replacing "pin lands at reserved-gap Slice 27
  (not yet executed here)"). `rust.md` stays the OWNER of the Rust surface set.
- `dev/acceptance.md` AC-074 `:906`: replace "Rust-facade measurement is defined
  at Slice 27" with the actual measurement (facade re-exports exactly the governed
  Rust allowlist, no recovery-denylist verb, parity-consistent with Py+TS;
  asserted by `tests/governed_surface.rs` + the byte-frozen
  `no_recovery_surface.rs`). Py/TS clauses untouched; no new AC id.

## 6. Scope / non-goals

No engine or schema change; no new AC id; the byte-frozen recovery suites
(`no_recovery_surface.rs`, `test_no_recovery_surface.{py,ts}`) stay byte-unchanged;
no Py/TS surface-suite change; no recovery verb added to the facade; no
release/CI/version action. `dev/interfaces/` is not currently indexed in
`DOC-INDEX.md`; per §3.3 of the slice prompt this slice adds the
`dev/interfaces/rust.md` row (the doc it makes the landed-contract owner) and
leaves the other unindexed interface docs as a pre-existing X3 gap.
