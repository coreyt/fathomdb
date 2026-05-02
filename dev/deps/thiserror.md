---
title: thiserror
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for thiserror
blast_radius: every public error type across all three crates
status: draft
---

# thiserror

**Verdict:** keep

## Current usage

- Crates using it: fathomdb-engine, fathomdb-query, fathomdb-schema
- Surface used: `#[derive(Error)]`, `#[from]`, `#[source]`
- Version pin: `2.0.12`; latest 2.0.x

## Maintenance signals

- Last release: active
- Open issues / open CVEs: none
- Maintainer count: dtolnay; sole-maintainer risk: yes (but bus-factor mitigated by simplicity + ubiquity)
- License: MIT OR Apache-2.0 — compatible: yes
- MSRV: 1.61; matches: yes

## Cross-platform

- Pure Rust, all platforms clean.
- C-boundary footguns: none.

## Alternatives considered (≥1)

- `snafu`: pros — context-rich; cons — heavier API, would force rewrite of every error variant. Migration cost: ~30 error types touched. Not net positive.
- Hand-written `Display`/`Error` impls: pros — zero dep; cons — boilerplate explosion. No.

## Verdict rationale

Standard, minimal, no friction. Keep.

## What would force replacement in 0.7.0?

Nothing realistic.
