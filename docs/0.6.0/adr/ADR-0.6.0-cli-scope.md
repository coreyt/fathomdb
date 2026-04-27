---
title: ADR-0.6.0-cli-scope
date: 2026-04-27
target_release: 0.6.0
desc: CLI = admin + recovery + read-only query (search/get/list); writes stay binding-only
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

**Admin + recovery + read-only query.**

Verb set:

- **Admin / lifecycle:** `open`, `init`, `close`, `vacuum`.
- **Recovery / inspection:** `dump-schema`, `dump-row-counts`, `dump-profile`, `integrity-check`, `export`, `regenerate` (vector projections).
- **Read-only query:** `search`, `get`, `list`.

**Writes stay binding-only.** No `cli write-node`, no `cli set-config-from-flag`, no SQL escape hatch.

Specific full verb enumeration with flags and examples lives in `interfaces/cli.md`; FU-TWB2 enumerates the recovery verb set in detail.

## Options considered

**A — Admin + recovery only.** Smallest CLI; clear operator role. Operators wanting ad-hoc reads must write a script using a binding. Pushes inspection burden onto bindings.

**B — Admin + recovery + read-only query (chosen).** Covers operator inspection use case (TWB-2) without re-opening SQL escape-hatch class. Reads are valuable for operators and CI inspection scripts. Writes stay binding-only.

**C — Full surface (admin + recovery + query + write).** CLI is complete interface. Largest surface; writes via CLI flags get unwieldy quickly (typed inputs collapse poorly into command-line flags); tempting but speculative.

## Consequences

- `interfaces/cli.md` enumerates the verb set; each verb has typed CLI flags (no SQL).
- Recovery verbs (TWB-2 followup) land here as concrete typed commands.
- `search` / `get` / `list` are read-only typed verbs over the engine read surface.
- Future write verbs are out of scope for 0.6.0; require this ADR to be re-opened.
- CLI is sync (per async-surface ADR); no `--async` flag, no concurrency knobs.

## Citations

- HITL 2026-04-27.
- ADR-0.6.0-typed-write-boundary (recovery is typed CLI flags, not SQL; TWB-3 rejected).
- ADR-0.6.0-async-surface (CLI sync).
- FU-TWB2 (recovery verb set enumeration in `interfaces/cli.md`).
