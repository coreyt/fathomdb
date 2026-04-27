---
title: tokenizers
date: 2026-04-24
target_release: 0.6.0
desc: Audit verdict for tokenizers
blast_radius: fathomdb-engine `default-embedder` (text → token IDs for BGE)
status: draft
---

# tokenizers

**Verdict:** keep

## HITL decision (2026-04-25)

Critic-B F5: aarch64-Linux build claim was asserted, not CI-verified;
`paste` (unmaintained) reaches us via both `tokenizers` and `pulp`
(candle SIMD) — double exposure.

HITL: tokenizers is required for BGE WordPiece; falls under the global
**all-platform CI requirement** (Linux x86_64 + Linux aarch64 + macOS +
Windows must be CI-verified before lock). aarch64-Linux historically had
oniguruma issues; CI evidence on aarch64 is required, not asserted.
`paste` watch carried as a transitive followup in `deps/README.md`; no
direct action.

## Current usage
- Crates using it: fathomdb-engine (feature `default-embedder`)
- Surface used: `Tokenizer::from_file`, `encode`
- Version pin: `0.22.2` default-features=false features=`onig`; latest 0.22.x

## Maintenance signals
- Last release: active (huggingface)
- Open issues / open CVEs: transitive `paste` RUSTSEC-2024-0436 (unmaintained, not vuln). Transitive `rand` RUSTSEC-2026-0097 (unsound only under custom logger; not triggered).
- Maintainer count: huggingface org; sole-maintainer risk: no
- License: Apache-2.0 — compatible: yes
- MSRV: 1.74; matches: yes

## Cross-platform
- All four triples. `onig` (Oniguruma) is C; precompiled via build script. Risk on aarch64 Linux historically — currently builds clean.
- C-boundary footguns: oniguruma binding is upstream-maintained; our usage does not touch raw pointers.

## Alternatives considered (≥1)
- `tiktoken-rs`: pros — pure Rust, no C; cons — only BPE tokenizers, not BERT WordPiece (BGE needs WordPiece). Not viable.
- `rust-bert` `WordPiece` impl: pros — pure Rust; cons — pulls torch-sys (unacceptable). Not viable.

## Verdict rationale
Required for BGE WordPiece. Acceptable transitive risk. Keep.

## What would force replacement in 0.7.0?
Drop default-embedder OR switch to a tokenizer family `tiktoken-rs` covers.
