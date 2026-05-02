---
title: serde_json
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for serde_json
blast_radius: query JSON filters, projection payloads, all SDK request/response shapes
status: draft
---

# serde_json

**Verdict:** keep

## Current usage

- Crates using it: fathomdb, fathomdb-engine
- Surface used: `Value`, `to_string`, `from_str`, `json!` macro
- Version pin: `1.0.140`; latest `1.0.149`

## Maintenance signals

- Last release: active
- Open issues / open CVEs: none
- Maintainer count: dtolnay + serde org; sole-maintainer risk: no
- License: MIT OR Apache-2.0 — compatible: yes
- MSRV: 1.56; matches: yes

## Cross-platform

- Pure Rust, all platforms clean.
- C-boundary footguns: none.

## Alternatives considered (≥1)

- `simd-json`: pros — faster parse; cons — runtime CPU detect, larger binary, less stable API, mismatched with serde derive everywhere else. Not worth migration cost (~1k LoC) for our payload sizes.

## Verdict rationale

Pairs with serde, no realistic alt at SDK boundary. Keep.

## What would force replacement in 0.7.0?

Nothing.
