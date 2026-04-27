---
title: rstest
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for rstest (dev-dep)
blast_radius: parameterized tests in fathomdb-query
status: draft
---

# rstest

**Verdict:** keep

## Current usage
- Crates using it: fathomdb-query (dev-deps)
- Surface used: `#[rstest]` parameterization, fixtures
- Version pin: `0.24.0`; latest 0.24.x

## Maintenance signals
- Last release: active
- Open issues / open CVEs: none
- Maintainer count: la10736 + multi; sole-maintainer risk: low
- License: MIT OR Apache-2.0 — compatible: yes
- MSRV: matches workspace

## Cross-platform
- All host platforms.
- C-boundary footguns: none.

## Alternatives considered (≥1)
- Hand-rolled table-driven tests: pros — no proc-macro; cons — verbose, less expressive. No.

## Verdict rationale
Standard parameterized-testing tool; dev-only. Keep.

## What would force replacement in 0.7.0?
Nothing.
