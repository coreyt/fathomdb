---
title: vitest
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for vitest (TS SDK devDep)
blast_radius: TS SDK + harness test runner
status: draft
---

# vitest

**Verdict:** keep

## Current usage

- Where: typescript/packages/fathomdb + apps/sdk-harness (devDeps)
- Version pin: `^3.2.4`

## Maintenance signals

- Active (vitest org). License MIT. No CVEs material.
- Multi-maintainer.

## Cross-platform

- Test-time only, all host platforms.

## Alternatives considered (≥1)

- `jest`: pros — older, more docs; cons — heavier, ESM-awkward, slower. Not net better.
- `node:test`: pros — stdlib; cons — fewer features (snapshot, mocking, coverage). Not yet a substitute.

## Verdict rationale

Modern ESM-native test runner; matches our package type. Keep.

## What would force replacement in 0.7.0?

Nothing.
