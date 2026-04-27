---
title: click
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for click (Python optional CLI dep)
blast_radius: Python SDK `cli` extra (fathomdb._cli)
status: draft
---

# click

**Verdict:** keep

## Current usage
- Where: `python/pyproject.toml` optional-dependencies `cli` (>=8.1)
- Surface used: command groups for `fathomdb` console script

## Maintenance signals
- Last release: active (pallets)
- Open issues / open CVEs: none
- Maintainer count: pallets multi; sole-maintainer risk: no
- License: BSD-3-Clause — compatible: yes

## Cross-platform
- Pure Python. All platforms.
- C-boundary footguns: none.

## Alternatives considered (≥1)
- `typer`: thin wrapper over click; would not remove click as a transitive. No.
- `argparse`: pros — stdlib; cons — verbose, no group ergonomics. Not worth migration.

## Verdict rationale
Standard, stable, optional. Keep.

## What would force replacement in 0.7.0?
Nothing.
