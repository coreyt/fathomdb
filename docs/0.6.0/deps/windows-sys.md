---
title: windows-sys
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for windows-sys
blast_radius: fathomdb-engine on `cfg(windows)` only — file/security syscalls
status: draft
---

# windows-sys

**Verdict:** keep (bump 0.59 → 0.61 approved)

## HITL decision (2026-04-25, refreshed 2026-04-27)

Critic-B F9: pin `0.59` causes dup-version bloat on Windows builds because
multiple transitives (rustls, hyper) pull `windows-sys` at 0.52 / 0.59 / 0.60
simultaneously. HITL: **bump approved** to debloat. `cargo-outdated`
(2026-04-27 run) reports latest as 0.61.2; bump target updated from 0.60 to
0.61 to ride the most-recently-released line.

Lands as separate implementer change to `Cargo.toml` (out of audit scope per
dep-auditor contract). After bump, verify `cargo tree -d` no longer shows
`windows-sys` duplicates triggered by our direct dep.

## Current usage
- Crates using it: fathomdb-engine (target.cfg(windows))
- Surface used: Win32 file/security APIs (foundation, security, storage filesystem, system services)
- Version pin: `0.59`; latest 0.60.x

## Maintenance signals
- Last release: active (microsoft)
- Open issues / open CVEs: none
- Maintainer count: microsoft; sole-maintainer risk: no
- License: MIT OR Apache-2.0 — compatible: yes
- MSRV: 1.65; matches: yes

## Cross-platform
- Windows-only. Pulled `cfg(windows)`, no impact on other platforms.
- C-boundary footguns: bindings are auto-generated from Win32 metadata; types use `c_char`/`c_void` correctly. Our usage does not transmute fn pointers.

## Alternatives considered (≥1)
- `winapi`: maintenance-frozen, replaced by windows-sys. No.
- `windows` (high-level): pros — safer wrappers; cons — much larger, COM machinery we don't need. Not worth it.

## Verdict rationale
Standard Microsoft-maintained Win32 binding. Keep.

## What would force replacement in 0.7.0?
Nothing.
