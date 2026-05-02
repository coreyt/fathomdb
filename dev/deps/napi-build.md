---
title: napi-build
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for napi-build
blast_radius: build.rs for fathomdb under `node` feature
status: draft
---

# napi-build

**Verdict:** keep

## Current usage

- Crates using it: fathomdb (build-dep, used by napi tooling at compile time)
- Version pin: `2.1.5`

## Maintenance signals

- Same family as napi/napi-derive. License MIT. No CVEs.

## Cross-platform

- Pure build-time helper. All host platforms supported.
- C-boundary footguns: none.

## Alternatives considered (≥1)

- Inline build.rs: trivial savings, loses upstream fixes. No.

## Verdict rationale

Trivial build helper. Keep.

## What would force replacement in 0.7.0?

napi-rs ecosystem abandonment.
