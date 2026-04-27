---
title: hf-hub
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for hf-hub
blast_radius: fathomdb-engine `default-embedder` (model + tokenizer download from HF)
status: draft
---

# hf-hub

**Verdict:** replace

## HITL decision (2026-04-25)

Default-embedder is decided for 0.6.0 (HITL F6 → candle stack). hf-hub
replacement promotes from "Phase 2 ADR" to active followup — thin `ureq`
downloader (~+120/-40 LoC).

Critic-B F10 wanted a vendored test fixture replicating the current HF
cache layout. **HITL declined: use what is there + works** —
`~/.cache/huggingface` content-addressed blob+symlink layout is preserved
by best-effort, not pinned by fixture. If a future huggingface_hub layout
change breaks compat, that becomes a new release issue.

## Current usage
- Crates using it: fathomdb-engine (feature `default-embedder`)
- Surface used: `Api::new`, `model.get(<filename>)` to fetch BGE weights/tokenizer
- Version pin: `0.5.0` default-features=false features=`ureq, rustls-tls`; latest 0.5.x

## Maintenance signals
- Last release: 2024–2025 (huggingface)
- Open issues / open CVEs: pulls `reqwest` → `quinn` → `rand 0.9` (RUSTSEC-2026-0097 transitive). The `ureq` feature is supposed to avoid this; investigate why both pull paths appear in lock — likely default-feature leakage through deps.
- Maintainer count: huggingface; sole-maintainer risk: no
- License: Apache-2.0 — compatible: yes
- MSRV: 1.74; matches: yes

## Cross-platform
- All four triples in principle; rustls-tls keeps build self-contained.
- C-boundary footguns: none direct.

## Alternatives considered (≥1)
- Direct `ureq` (or `reqwest`) GET against `https://huggingface.co/<repo>/resolve/<rev>/<file>`: pros — drop hf-hub + transitive reqwest+quinn entirely (large lockfile reduction); cons — manual cache directory + revision pinning (~150 LoC). Behavior delta: lose hf token + auth helpers (we don't use them).
- Bundle BGE weights in the wheel: pros — no network; cons — +130MB wheel, license/distribution complexity.

## Verdict rationale
hf-hub drags in reqwest+quinn+tokio when our needs are "GET 4 files once and cache them". The transitive surface is the largest in the workspace. Replace with thin `ureq` downloader IF default-embedder remains shipped. If Phase 2 moves embedder to sidecar, this dep dies entirely — defer decision.

## Migration plan (only if verdict = replace)
- Steps:
  1. Add `ureq` (already pulled transitively today via hf-hub features) as direct dep behind `default-embedder`.
  2. Implement `download_to_cache(repo, rev, file)` in fathomdb-engine — ~120 LoC.
  3. Replace `hf_hub::Api` calls (single file in embedder bootstrap).
  4. Remove `hf-hub` from Cargo.toml; verify reqwest/quinn drop from `Cargo.lock`.
- Estimated LoC delta: +120 / −40.
- Risk areas: cache layout compatibility (consumers may already have ~/.cache/huggingface populated — preserve that path).

## What would force replacement in 0.7.0?
N/A — flagged for replace now (or drop with default-embedder).
