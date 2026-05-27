# tests/corpus/

FathomDB 0.7.0 composite test corpus — real-data + synthetic
multi-modal retrieval fixture.

**Start here:** [`corpus-card.md`](./corpus-card.md) — sources,
licenses, schema, HITL locks.

## Layout

```
tests/corpus/
├── corpus-card.md       # authoritative source + license table
├── raw/                 # canonical per-source JSONL (committable subset in-tree)
│   └── .gitignore       # cache-only sources excluded here
├── chains/              # synthetic cross-doc chain definitions (Corpus-Pack 2)
└── scripts/             # acquisition / cleaning / chain-generation
```

## Building the corpus

(Acquisition pipeline lands in Corpus-Pack 1; this section will
be filled in then.)

## Status

Scaffold only — Corpus-Pack 1 not yet started. Implementation
handoff: [`dev/plans/prompts/0.7.0-CORPUS-BUILD-HANDOFF.md`](../../dev/plans/prompts/0.7.0-CORPUS-BUILD-HANDOFF.md).
