---
title: tracing-subscriber
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for tracing-subscriber
blast_radius: fathomdb-engine (subscriber init when `tracing` feature on)
status: draft
---

# tracing-subscriber

**Verdict:** keep

## Current usage
- Crates using it: fathomdb-engine, dev-deps
- Surface used: `fmt`, `EnvFilter`, json layer
- Version pin: `0.3` features=`json, env-filter`; latest 0.3.x

## Maintenance signals
- Last release: active (tokio-rs)
- Open issues / open CVEs: none
- Maintainer count: tokio-rs org; sole-maintainer risk: no
- License: MIT — compatible: yes
- MSRV: 1.65; matches: yes

## Cross-platform
- Pure Rust, all platforms clean.
- C-boundary footguns: none.

## Alternatives considered (≥1)
- `tracing-log` only + handwritten subscriber: pros — smaller; cons — re-implement env-filter parsing. Not worth it.

## Verdict rationale
Pairs with `tracing`. Keep.

## What would force replacement in 0.7.0?
Nothing.
