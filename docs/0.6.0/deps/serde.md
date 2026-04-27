---
title: serde
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for serde
blast_radius: every public type that crosses the FFI/JSON boundary
status: draft
---

# serde

**Verdict:** keep

## Current usage
- Crates using it: fathomdb, fathomdb-engine
- Surface used: `Serialize`, `Deserialize` derives; `serde::de::Error` for custom adapters
- Version pin: `1.0.219`; latest `1.0.228`

## Maintenance signals
- Last release: active monthly
- Open issues / open CVEs: none
- Maintainer count: dtolnay + multi; sole-maintainer risk: no
- License: MIT OR Apache-2.0 — compatible: yes
- MSRV: 1.61; matches: yes

## Cross-platform
- Pure Rust, all platforms clean.
- C-boundary footguns: none.

## Alternatives considered (≥1)
- `miniserde` / `nanoserde`: pros — faster compile, no proc-macro; cons — no JSON enum reps, no PyO3/napi interop, weaker ecosystem. Not viable for SDK boundary.

## Verdict rationale
De-facto standard, no risk surface. Keep.

## What would force replacement in 0.7.0?
Nothing realistic.
