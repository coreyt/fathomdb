---
title: criterion
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for criterion (dev-dep)
blast_radius: benches only (production_paths bench in fathomdb crate)
status: draft
---

# criterion

**Verdict:** keep

## Current usage
- Crates using it: fathomdb (dev-deps), bench `production_paths`
- Surface used: `criterion_group!`, `Criterion::bench_function`
- Version pin: `0.5.1`; latest 0.5.x

## Maintenance signals
- Last release: 2025
- Open issues / open CVEs: none direct (pulls `rand 0.9` transitively → RUSTSEC-2026-0097 not triggered in dev-only path)
- Maintainer count: bheisler + multi; sole-maintainer risk: no
- License: MIT OR Apache-2.0 — compatible: yes
- MSRV: 1.70; matches: yes

## Cross-platform
- All host platforms.
- C-boundary footguns: none.

## Alternatives considered (≥1)
- `divan`: pros — newer, faster, simpler; cons — fewer reporters. Migration cost: rewrite of one bench file (~100 LoC). Worth a Phase 2 followup but not this audit's scope.

## Verdict rationale
Standard Rust bench harness; dev-only blast radius. Keep.

## What would force replacement in 0.7.0?
Reporting/output ergonomics if `divan` matures further.
