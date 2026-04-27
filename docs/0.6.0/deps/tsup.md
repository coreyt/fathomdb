---
title: tsup
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for tsup (TS SDK devDep)
blast_radius: TS SDK + harness build pipeline
status: draft
---

# tsup

**Verdict:** keep

## Current usage
- Where: typescript/packages/fathomdb + apps/sdk-harness (devDeps)
- Surface used: ESM bundle + dts emission via `tsup src/... --format esm --dts`
- Version pin: `^8.3.5`

## Maintenance signals
- Active (egoist). License MIT. No CVEs material.
- Sole-maintainer risk: yes — but well-scoped wrapper over esbuild; replaceable.

## Cross-platform
- Build-time only, all host platforms.

## Alternatives considered (≥1)
- Plain `esbuild` + `tsc --emitDeclarationOnly`: pros — drop tsup; cons — two-step build, more config. Migration cost: ~30 LoC of build script. Reasonable Phase 2 followup but not urgent.
- `tsc` only: pros — zero deps; cons — no bundling. Not viable for SDK distribution.

## Verdict rationale
Convenient wrapper, low risk. Keep.

## What would force replacement in 0.7.0?
tsup abandonment; trivial migration to direct esbuild.
