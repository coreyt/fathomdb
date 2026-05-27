# tests/corpus/

FathomDB 0.7.0 composite test corpus — real-data + synthetic
multi-modal retrieval fixture.

**Start here:** [`corpus-card.md`](./corpus-card.md) — sources,
licenses, schema, HITL locks.

## Layout

```
tests/corpus/
├── corpus-card.md   # authoritative source + license table
├── chains/          # synthetic cross-doc chain definitions (Corpus-Pack 2)
└── scripts/         # acquisition / generation / chain-build scripts (in git)
    ├── _corpus_lib.py
    ├── acquire_*.py        # per-source fetch + normalize
    ├── generate_*.py       # deterministic generators
    └── manifest.json       # upstream pins + output sha256 contract

data/corpus-data/    # PRODUCED DATA (gitignored, see top-level .gitignore)
├── downloads/       # raw upstream artifacts (e.g. enron tarball)
└── raw/             # canonical per-source JSONL
```

Scripts in `tests/corpus/scripts/` are the reproducible source of
truth; the data they produce lives outside the repo at
`data/corpus-data/` and is rebuilt locally or restored from CI cache.

## Building the corpus

Each acquisition script is self-contained via `uv run` PEP-723 inline
metadata. From the repo root:

```bash
uv run tests/corpus/scripts/acquire_cnn_dailymail.py
uv run tests/corpus/scripts/acquire_landes_todos.py
uv run tests/corpus/scripts/acquire_bahmutov_dailylogs.py
uv run tests/corpus/scripts/acquire_enron.py        # needs ~443MB tarball
uv run tests/corpus/scripts/generate_synthetic_notes.py
uv run tests/corpus/scripts/acquire_qmsum.py
uv run tests/corpus/scripts/acquire_enronqa.py
```

For Enron, the tarball is fetched once into
`data/corpus-data/downloads/enron_mail_20150507.tar.gz` (or wherever
`$ENRON_CACHE_TARBALL` points if set) and reused on subsequent runs.

Every script's run prints a `sha256 = ...` line that must match the
entry in `scripts/manifest.json` for the run to be considered
reproducible.

## Status

Corpus-Pack 1 in progress (7 of ~10 planned sources acquired). PMC OA
is deferred. Pack 2 (cross-doc chain generator) is next. Implementation
handoff:
[`dev/plans/prompts/0.7.0-CORPUS-BUILD-HANDOFF.md`](../../dev/plans/prompts/0.7.0-CORPUS-BUILD-HANDOFF.md).
