# experiments/ — durable, git-friendly experiment tracking (fathomdb side)

A file-based experiment index for fathomdb **eval runs**. The machine
source-of-truth is append-only; the human table is generated. This mirrors
memex's scheme: the two repos' indices share one record + index-row schema and
are cross-compatible — the index row's `repo` field (`"fathomdb"` here,
`"memex"` there) distinguishes them.

> Scope note: this slice provides the CAPABILITY. No fathomdb eval run is wired
> to it yet; the eval program adopts it when it lands.

## Layout

```text
experiments/
  index.jsonl                 # append-only; ONE JSON line per run (source-of-truth)
  INDEX.md                    # GENERATED human table (never hand-edit)
  runs/<run_id>/
    record.json               # canonical per-run record (typed schema)
    config.resolved.yaml      # the config AFTER defaults+overrides merge
    metrics.json              # flat metrics (also embedded in record.json)
  _lib.py                     # the shared, pure, typed helper (TDD'd)
```

`run_id = <experiment-slug>-<UTC-ts:YYYYMMDDTHHMMZ>-<config_sha8>`, where
`config_sha8` is the first 8 hex of the sha256 of the canonical-JSON of the
resolved config. Given a fixed timestamp + config, the `run_id` is deterministic.

## The standing rules

1. **An experiment is a typed CONFIG + an index line + a durable record — never
   a forked script.** New experiments are new config files (the
   `eval/*/config.py` / acquire `_config.py` convention), not bespoke runners
   with inlined constants.
2. **Config is typed and consumed-or-loudly-rejected.** An unknown or missing
   field raises at load; the same discipline governs `record.json` (see
   `_lib.record_from_dict`).
3. **Every eval run writes a `record.json` + appends `index.jsonl`** (via
   `_lib.write_record`, then `_lib.regen_index_md`) BEFORE it is "closed".
4. **Verdicts distill into the human layer.** The one-line `read` + `verdict`
   on each record is the honest finding that rolls up into
   [`dev/experiments-ledger.md`](../dev/experiments-ledger.md).

## Rules for the writer

- `index.jsonl` is **append-only**: never rewrite or reorder existing lines.
- `INDEX.md` is **generated** from `index.jsonl` (`_lib.regen_index_md()` is
  idempotent). Do not hand-edit it.
- `_lib.py` is **pure/no-network** and the timestamp is passed IN by the caller
  (a live runner supplies `datetime.now(UTC)`), so hashing/`run_id` stay testable.
- Git subprocess calls strip `GIT_DIR`/`GIT_WORK_TREE` (`_lib.git_env`) so a
  pre-push hook's inherited repo-location can never redirect them.

## Registry vs. experiment — the split

Two distinct ledgers, do not conflate:

- **Data registry** — corpus acquisition. A dataset is registered in
  `tests/corpus/scripts/manifest.json` + `tests/corpus/corpus-card.md`. See
  [`../tests/corpus/scripts/README.md`](../tests/corpus/scripts/README.md).
  Answers "what data exists and is it byte-stable".
- **Experiment index** — this directory. Answers "what did we measure over the
  data". A retrieval/answer-quality eval run gets a record + an index line here;
  it never lands in the corpus manifest, and a corpus acquisition never lands
  here.

`dev/experiments-ledger.md` is the human-distillation layer above this machine
index — prose findings and narrative; `index.jsonl` is the structured ledger.
