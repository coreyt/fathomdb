---
title: "@types/node"
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for @types/node (TS SDK devDep)
blast_radius: type definitions for Node stdlib in TS SDK + harness
status: draft
---

# @types/node

**Verdict:** keep

## Current usage

- Where: typescript/packages/fathomdb + apps/sdk-harness (devDeps)
- Version pin: `^24.0.0`

## Maintenance signals

- DefinitelyTyped, continuously updated. License MIT. No CVEs.

## Cross-platform

- Type-only.

## Alternatives considered (≥1)

- Hand-written ambient declarations: not viable.

## Verdict rationale

Required for typed Node SDK. Keep.

## What would force replacement in 0.7.0?

Nothing.
