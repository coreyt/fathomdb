---
title: tempfile
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for tempfile (dev-dep)
blast_radius: integration tests across all crates
status: draft
---

# tempfile

**Verdict:** keep

## Current usage

- Crates using it: fathomdb (dev), fathomdb-engine (dev)
- Surface used: `TempDir`, `NamedTempFile` for sqlite databases in tests
- Version pin: `3.19.1`; latest 3.x

## Maintenance signals

- Last release: active (Stebalien)
- Open issues / open CVEs: none
- Maintainer count: multi; sole-maintainer risk: no
- License: MIT OR Apache-2.0 — compatible: yes
- MSRV: 1.63; matches: yes

## Cross-platform

- All platforms; uses windows-sys / nix internally.
- C-boundary footguns: none direct.

## Alternatives considered (≥1)

- Hand-rolled `std::env::temp_dir() + cleanup`: pros — no dep; cons — race conditions on parallel tests, no auto-cleanup on panic. No.

## Verdict rationale

Standard, safe, dev-only. Keep.

## What would force replacement in 0.7.0?

Nothing.
