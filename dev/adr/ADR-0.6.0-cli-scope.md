---
title: ADR-0.6.0-cli-scope
date: 2026-04-27
target_release: 0.6.0
desc: CLI = two-root operator surface (`recover` + `doctor`); writes and application queries stay SDK-only
blast_radius: cli/ binding source; interfaces/cli.md; recovery verb set (FU-TWB2); ADR-0.6.0-typed-write-boundary; ADR-0.6.0-async-surface (CLI sync)
status: accepted
---

# ADR-0.6.0 — CLI scope

**Status:** accepted (HITL 2026-04-27).

Phase 2 #22 interface ADR. Decides what verbs the `fathomdb` CLI ships in 0.6.0.

## Context

CLI scope decides whether `fathomdb` binary is operator tool or query interface. Constrained by:
- ADR-0.6.0-typed-write-boundary recovery-verb rule (typed CLI flags, not SQL).
- TWB-3 rejection (no SQL escape hatch / offline diagnostic binary).
- ADR-0.6.0-async-surface (CLI is sync).

## Decision

**Two-root operator CLI.**

Verb set:

- **Lossy / non-bit-preserving:** `fathomdb recover --accept-data-loss <sub-flag>...`
  where the 0.6.0 sub-flag set is
  `{--truncate-wal, --rebuild-vec0, --rebuild-projections, --excise-source <id>, --purge-logical-id <id>, --restore-logical-id <id>}`.
- **Bit-preserving / read-only:** `fathomdb doctor <verb>`
  where the 0.6.0 verb set is
  `{check-integrity, safe-export, verify-embedder, trace, dump-schema, dump-row-counts, dump-profile}`.

`--accept-data-loss` is root-level and mandatory on `recover`; it is rejected on `doctor` verbs. `--json` is the normative machine-readable contract on every verb. `doctor check-integrity` emits a single JSON object; other verb-level JSON shapes are owned by `interfaces/cli.md` and `design/recovery.md`.

**Writes stay binding-only.** No `cli write-node`, no `cli set-config-from-flag`, no SQL escape hatch. **Application query verbs also stay out of the 0.6.0 CLI.** Ad-hoc reads are handled via operator verbs such as `trace`, `dump-*`, and `check-integrity`, not a parallel `search/get/list` application surface.

Specific full flag spelling and exit-code numbers live in `interfaces/cli.md`; canonical verb ownership and recovery semantics live in `design/recovery.md`.

## Options considered

**A — Two-root operator CLI (chosen).** Smallest operator-complete surface; clear mutation split (`recover` vs `doctor`); no SDK-mirroring query layer to maintain.

**B — Operator CLI + read-only application query (`search/get/list`).** Covers ad-hoc inspection. Rejected: introduces a second public query surface with little additional operator value over `trace`, `dump-*`, and integrity tooling.

**C — Full surface (admin + recovery + query + write).** CLI is complete interface. Largest surface; typed writes via CLI flags get unwieldy quickly; query parity doubles public surface. Rejected.

## Consequences

- `interfaces/cli.md` enumerates concrete flag spelling and exit-code numbers for the two roots.
- `design/recovery.md` owns the canonical verb table, bit-preserving vs lossy classification, JSON-output posture, and `check-integrity` report shape.
- Future application query verbs are out of scope for 0.6.0; adding them requires this ADR to be re-opened.
- Future write verbs are out of scope for 0.6.0; adding them also requires this ADR to be re-opened.
- CLI is sync (per async-surface ADR); no `--async` flag, no concurrency knobs.

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-typed-write-boundary (recovery is typed CLI flags, not SQL; TWB-3 rejected).
- ADR-0.6.0-async-surface (CLI sync).
- FU-TWB2 (recovery verb set enumeration in `interfaces/cli.md`).
