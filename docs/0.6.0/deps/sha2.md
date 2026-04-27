---
title: sha2
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for sha2
blast_radius: fathomdb-engine (content/embedder identity hashes), fathomdb-schema (migration checksums)
status: draft
---

# sha2

**Verdict:** keep

## Current usage
- Crates using it: fathomdb-engine, fathomdb-schema
- Surface used: `Sha256::digest` for stable identity hashes
- Version pin: `0.10`; latest 0.10.x

## Maintenance signals
- Last release: active (RustCrypto org)
- Open issues / open CVEs: none
- Maintainer count: RustCrypto multi; sole-maintainer risk: no
- License: MIT OR Apache-2.0 — compatible: yes
- MSRV: 1.71; matches: yes

## Cross-platform
- Pure Rust, all platforms; SHA-NI accel where available.
- C-boundary footguns: none.

## Alternatives considered (≥1)
- `blake3`: pros — faster, modern; cons — different digest, not compatible with already-persisted SHA256 identity hashes. Migration cost: schema migration to rehash all identities (~migration v26 + backfill). Not worth it.

## Verdict rationale
Stable, well-maintained, locked in by persisted identity hashes. Keep.

## What would force replacement in 0.7.0?
A FIPS or perf requirement that is not solvable with SHA-NI.
