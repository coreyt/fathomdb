---
title: insta
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for insta (dev-dep)
blast_radius: snapshot tests in fathomdb-query (compiled-SQL snapshots)
status: draft
---

# insta

**Verdict:** keep

## Current usage
- Crates using it: fathomdb-query (dev-deps)
- Surface used: `assert_yaml_snapshot!` for compiled SQL
- Version pin: `1.42.2` features=`yaml`; latest 1.4x.x

## Maintenance signals
- Last release: active (mitsuhiko)
- Open issues / open CVEs: none
- Maintainer count: mitsuhiko + multi; sole-maintainer risk: no
- License: Apache-2.0 — compatible: yes
- MSRV: 1.70; matches: yes

## Cross-platform
- All host platforms.
- C-boundary footguns: none.

## Alternatives considered (≥1)
- `expect-test`: pros — simpler, no companion file; cons — inline expect strings get noisy for multi-line SQL. Migration cost: rewrite of every snapshot assertion (~30+ sites). Not worth it.

## Verdict rationale
Best-in-class snapshot lib; dev-only. Keep.

## What would force replacement in 0.7.0?
Nothing.
