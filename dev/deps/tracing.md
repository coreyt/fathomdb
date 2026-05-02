---
title: tracing
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for tracing
blast_radius: optional observability across engine/query/schema
status: draft
---

# tracing

**Verdict:** keep

## Current usage

- Crates using it: fathomdb-engine, fathomdb-query, fathomdb-schema, fathomdb (all behind `tracing` feature)
- Surface used: `info!`, `debug!`, `instrument`, `Span`
- Version pin: `0.1` features=`log, release_max_level_info`; latest 0.1.x

## Maintenance signals

- Last release: active (tokio-rs)
- Open issues / open CVEs: none
- Maintainer count: tokio-rs org multi; sole-maintainer risk: no
- License: MIT — compatible: yes
- MSRV: 1.65; matches: yes

## Cross-platform

- Pure Rust, all platforms clean.
- C-boundary footguns: none.

## Alternatives considered (≥1)

- `log` only: pros — simpler; cons — no spans, no structured fields, no instrument macro. Loses observability needed by 0.6.0 retrieval-gates work.

## Verdict rationale

Standard observability. Feature-gated keeps default builds light. Keep.

## What would force replacement in 0.7.0?

Nothing.
