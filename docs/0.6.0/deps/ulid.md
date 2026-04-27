---
title: ulid
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for ulid
blast_radius: fathomdb-engine (entity / event ID generation)
status: draft
---

# ulid

**Verdict:** keep (with note: pulls `rand` 0.9 — see RUSTSEC-2026-0097 transitive)

## Current usage
- Crates using it: fathomdb-engine
- Surface used: `Ulid::new`, monotonic generator, `to_string`
- Version pin: `1`; latest 1.2.x

## Maintenance signals
- Last release: active (dylanhart)
- Open issues / open CVEs: none direct; transitive `rand 0.9.2` flagged RUSTSEC-2026-0097 (unsound with custom logger). Not exploitable in our usage (we don't install a custom rand logger).
- Maintainer count: 1 active; sole-maintainer risk: yes (low impact — small crate)
- License: MIT — compatible: yes
- MSRV: 1.60; matches: yes

## Cross-platform
- Pure Rust, all platforms clean.
- C-boundary footguns: none.

## Alternatives considered (≥1)
- `uuid` v7: pros — broader ecosystem, IETF standard for time-ordered UUIDs; cons — different stringification, would touch every persisted ID and the SDK boundary. Migration cost: schema migration + SDK type rewrite. Not worth it for 0.6.0.

## Verdict rationale
Drop-in stable. Transitive rand advisory does not affect our path. Keep, track upstream rand bump.

## What would force replacement in 0.7.0?
ULID-vs-UUIDv7 standardization pressure from SDK consumers.
