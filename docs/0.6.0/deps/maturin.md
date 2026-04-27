---
title: maturin
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for maturin (Python build backend)
blast_radius: python/ wheel build
status: draft
---

# maturin

**Verdict:** keep

## Current usage
- Where: `python/pyproject.toml` build-system requires `maturin>=1.9,<2`
- Surface used: build backend for PyO3 extension module

## Maintenance signals
- Last release: active (PyO3 org)
- Open issues / open CVEs: none
- Maintainer count: PyO3 org multi; sole-maintainer risk: no
- License: MIT — compatible: yes

## Cross-platform
- Builds wheels for all four target triples; cibuildwheel-friendly.
- C-boundary footguns: build-time only.

## Alternatives considered (≥1)
- `setuptools-rust`: pros — older, widely understood; cons — clunkier for PyO3 abi3 + module layout. Migration cost: rewrite pyproject + build pipeline. Not worth it.

## Verdict rationale
Standard PyO3 build backend. Keep.

## What would force replacement in 0.7.0?
PyO3 deprecating maturin support (not on roadmap).
