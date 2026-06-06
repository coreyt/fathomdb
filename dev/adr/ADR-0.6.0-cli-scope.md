---
title: ADR-0.6.0-cli-scope
date: 2026-04-27
target_release: 0.6.0
desc: CLI = two-root operator surface (`recover` + `doctor`); writes and application queries stay SDK-only
blast_radius: cli/ binding source; interfaces/cli.md; recovery verb set (FU-TWB2); ADR-0.6.0-typed-write-boundary; ADR-0.6.0-async-surface (CLI sync)
status: accepted
---

# ADR-0.6.0 — CLI scope

**Status:** accepted (HITL 2026-04-27); amended (HITL 2026-05-16) — `--purge-logical-id` and `--restore-logical-id` removed from 0.6.0 recover sub-flag set, originally deferred to 0.7.x (blocked on canonical identity substrate; see `design/recovery.md § Logical-id purge and restore`); further amended (HITL 2026-05-24) — deferral target moved from 0.7.x to **0.8.0** alongside Memex knowledge-store / retrieval features that consume the substrate (`dev/roadmap/0.8.0.md`). 0.7.0 is now perf-only. Further amended (2026-06-06, Slice 34 / reserved-gap-34) — the 0.8.0 op-store diagnostic read-back `fathomdb doctor dump-mutations <collection>` is scoped **IN** under the `doctor` root as a `dump-*` diagnostic over the mutation log (`operational_mutations`). This is **not** a re-opening of Option B: it adds no `search` / `get` / `list` application query surface over `canonical_nodes`; application query verbs still require this ADR to be re-opened. See the Consequences bullet below and `dev/design/slice-34-cli-op-store-readback-design.md`.

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
  `{--truncate-wal, --rebuild-vec0, --rebuild-projections, --excise-source <id>}`. (`--purge-logical-id` and `--restore-logical-id` deferred to 0.8.0 per 2026-05-24 amendment above; originally deferred to 0.7.x per 2026-05-16.)
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
- **2026-06-06 (Slice 34):** the 0.8.0 op-store read-back `fathomdb doctor dump-mutations <collection> [--after-id n] [--limit n] [--json] <db_path>` is added under `doctor` as a read-only, bit-preserving **diagnostic** over the mutation log (`operational_mutations`), in the same family as `trace` / `dump-*`. It reads back op-store rows over the existing `read.collection` / `read.mutations` engine seam and is **CLI-only** (no SDK-parity obligation). It is **not** the rejected Option B: it adds no `search` / `get` / `list` application query surface over `canonical_nodes`, so Option B stays rejected and a true application query surface still requires re-opening this ADR.
- Future write verbs are out of scope for 0.6.0; adding them also requires this ADR to be re-opened.
- CLI is sync (per async-surface ADR); no `--async` flag, no concurrency knobs.

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-typed-write-boundary (recovery is typed CLI flags, not SQL; TWB-3 rejected).
- ADR-0.6.0-async-surface (CLI sync).
- FU-TWB2 (recovery verb set enumeration in `interfaces/cli.md`).
