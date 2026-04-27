---
title: pyo3-log
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for pyo3-log
blast_radius: Python SDK logging bridge (fathomdb crate, `python` feature)
status: draft
---

# pyo3-log

**Verdict:** keep

## Current usage
- Crates using it: fathomdb (feature `python`)
- Surface used: `pyo3_log::init` — bridges Rust `log` to Python `logging`
- Version pin: `0.13`; latest 0.13.x

## Maintenance signals
- Last release: active (vorner)
- Open issues / open CVEs: none
- Maintainer count: 1 (vorner); sole-maintainer risk: yes (low blast — small crate)
- License: MIT OR Apache-2.0 — compatible: yes
- MSRV: matches PyO3

## Cross-platform
- Same triples as PyO3.
- C-boundary footguns: none direct.

## Alternatives considered (≥1)
- Hand-written log bridge using PyO3 directly: pros — no extra dep; cons — ~150 LoC + thread-safety footguns. Not worth it.

## Verdict rationale
Cheap, correct, well-scoped. Keep.

## What would force replacement in 0.7.0?
Switch to `tracing` + a tracing-to-logging bridge (would deprecate `log` flow entirely).
