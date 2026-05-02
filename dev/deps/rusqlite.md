---
title: rusqlite
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for rusqlite
blast_radius: fathomdb-engine, fathomdb-schema (every storage path)
status: draft
---

# rusqlite

**Verdict:** keep

## HITL decision (2026-04-25) — async-surface ADR promoted

Critic-B F4: "sync-only lock-in" not addressed. HITL: **promote
async-surface ADR from Phase 2 to Phase 1**, because the decision frames
every public 0.6.0 API surface (Python, TS, CLI, Rust). The ADR scopes:

- Stay sync (rusqlite-only, sync `Engine` API on every binding) vs.
- Layer async over rusqlite (engine remains sync; bindings expose
  spawn-blocking adapters), vs.
- Move to sqlx (async-native, ~5–8k LoC rewrite, sqlite-vec integration
  work).

ADR records the decision before Phase 3 interface drafts begin.

## Current usage

- Crates using it: fathomdb-engine, fathomdb-schema, fathomdb (dev)
- Surface used: `Connection`, `Transaction`, `params!`, `prepare_cached`, extension loading (sqlite-vec), backup API, optional `trace` feature
- Version pin: `0.32.1` features=`bundled, load_extension, backup`; latest 0.33.x available

## Maintenance signals

- Last release: active (multiple 2025 releases)
- Open issues / open CVEs: no advisories in `cargo audit` for current pin
- Maintainer count: multi (rusqlite org, jgallagher + active contributors); sole-maintainer risk: no
- License: MIT — compatible: yes
- MSRV: ~1.78; matches workspace edition 2024: yes

## Cross-platform

- Builds clean linux x86_64/aarch64, darwin, windows (we already ship 4 triples). `bundled` avoids system-libsqlite mismatch.
- C-boundary footguns: bindings via `libsqlite3-sys` use `c_char`/`c_int` correctly; no hardcoded i8/u8 in our wrappers.

## Alternatives considered (≥1)

- `sqlx` (sqlite backend): pros — async-native, compile-time query check; cons — pulls async runtime, no direct extension-loading parity, harder to integrate sqlite-vec. Migration cost: 5–8k LoC rewrite of engine, behavior delta on transaction semantics. Not viable for embedded synchronous engine.
- `libsqlite3-sys` raw: lower level, would re-implement most of rusqlite. Not net win.

## Verdict rationale

Sole rusqlite alternative is sqlx, which is incompatible with our embedded sync write coordinator and extension loader. No CVEs, healthy maintenance, minimal C-boundary risk. Keep.

## What would force replacement in 0.7.0?

Loss of extension-loading or backup APIs; or a need to share connections across async tasks at scale (would push toward sqlx).
