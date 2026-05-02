---
title: typescript
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for typescript (TS SDK devDep)
blast_radius: typescript/packages/fathomdb + apps/sdk-harness build-time
status: draft
---

# typescript

**Verdict:** keep

## Current usage

- Where: `typescript/packages/fathomdb/package.json`, `typescript/apps/sdk-harness/package.json` (devDeps)
- Version pin: `^5.8.3`

## Maintenance signals

- Microsoft, monthly releases. No CVEs material to our usage. License Apache-2.0.

## Cross-platform

- Build-time only.

## Alternatives considered (≥1)

- Hand-written JS + JSDoc: loses type safety at SDK boundary. No.

## Verdict rationale

Required for typed SDK. Keep.

## What would force replacement in 0.7.0?

Nothing.
