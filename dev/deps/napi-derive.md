---
title: napi-derive
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for napi-derive
blast_radius: TS SDK macro surface
status: draft
---

# napi-derive

**Verdict:** keep

## Current usage

- Crates using it: fathomdb (feature `node`)
- Surface used: `#[napi]` proc-macros for fns, structs, enums
- Version pin: `2.16.13`; latest 2.16.x

## Maintenance signals

- Same as `napi`. License MIT. No CVEs.

## Cross-platform

- Pure proc-macro, host-only.
- C-boundary footguns: none (it generates code; the runtime is `napi`).

## Alternatives considered (≥1)

- Hand-written N-API bindings: pros — no macro; cons — boilerplate explosion. No.

## Verdict rationale

Pairs with `napi`. Keep.

## What would force replacement in 0.7.0?

Same as `napi`.
