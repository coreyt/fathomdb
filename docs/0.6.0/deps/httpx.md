---
title: httpx
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for httpx (Python optional dep)
blast_radius: Python SDK optional `openai`/`jina`/`embedders` extras (HTTP embedder clients)
status: draft
---

# httpx

**Verdict:** keep

## Current usage
- Where: `python/pyproject.toml` optional-dependencies `openai`, `jina`, `embedders`
- Surface used: HTTP POST to embedder providers (OpenAI, Jina) under `>=0.25`

## Maintenance signals
- Last release: active (encode org)
- Open issues / open CVEs: none material
- Maintainer count: encode multi; sole-maintainer risk: no
- License: BSD-3-Clause — compatible: yes

## Cross-platform
- Pure Python. All platforms.
- C-boundary footguns: none.

## Alternatives considered (≥1)
- `requests`: sync-only, no HTTP/2, no async option. Worse for our use case.
- stdlib `urllib.request`: pros — zero dep; cons — no connection pooling, ergonomic regression. Not worth it.

## Verdict rationale
Modern, standard. Keep.

## What would force replacement in 0.7.0?
Nothing.
