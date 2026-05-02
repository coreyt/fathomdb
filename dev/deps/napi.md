---
title: napi
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for napi (napi-rs runtime)
blast_radius: TypeScript SDK binding surface (fathomdb crate, `node` feature)
status: draft
---

# napi

**Verdict:** keep

## Current usage

- Crates using it: fathomdb (feature `node`)
- Surface used: `JsObject`, `Env`, async task helpers, error conversion
- Version pin: `2.16.13` default-features=false features=`napi8`; latest 2.16.x

## Maintenance signals

- Last release: active (napi-rs org, Brooooooklyn)
- Open issues / open CVEs: none
- Maintainer count: small core but well-funded; sole-maintainer risk: low
- License: MIT — compatible: yes
- MSRV: 1.65; matches: yes

## Cross-platform

- Builds on all four declared triples in `package.json`; relies on N-API ABI so no recompile per Node version.
- C-boundary footguns: bindings use `c_char` / `c_void` correctly internally. Our wrapper code does not transmute fn pointers.

## Alternatives considered (≥1)

- `neon`: pros — older, stable; cons — no async ergonomics parity, less community momentum. Migration cost: rewrite of every #[napi] surface (~500 LoC). Not worth it.
- WASM-only SDK: pros — no native build; cons — perf cliff for vector ops, no extension loading. Not viable for parity.

## Verdict rationale

Only viable Node binding for a SQLite + sqlite-vec + extension-loading core. Keep.

## What would force replacement in 0.7.0?

napi-rs abandonment, or N-API ABI change Node refuses to support.
