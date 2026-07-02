# tests/corpus/scripts/ — the config-driven acquire standard

Reproducible acquisition / generation scripts for the FathomDB test corpus.
These scripts are the source of truth; the JSONL they produce lives under
`data/corpus-data/` (gitignored) and is rebuilt locally or restored from CI
cache. For the corpus itself — sources, licenses, schema, checksums — see
[`../corpus-card.md`](../corpus-card.md).

## The standing directive: every acquire script is typed-config

An acquire run is a **config, not a code edit**. Each `acquire_*.py` exposes a
typed dataclass config resolved through the shared helper
[`_config.py`](./_config.py):

- `--config file.yaml` — load a YAML/JSON config (bare-JSON fallback when PyYAML
  is absent; JSON is a valid YAML subset either way).
- `--override key=val` — repeatable, dotted-key one-off overrides (values are
  JSON-coerced).
- **Every field is consumed-or-loudly-rejected.** An unknown key fails at load
  (`config_from_dict` raises); value shape/ranges are checked by the config's
  own `validate()` before the run starts.

The baked defaults for each script live in [`configs/`](./configs) as
`acquire-<source>.yaml`, so a plain `uv run …/acquire_<source>.py` reproduces
the registered artifact and overrides are explicit and diffable.

### The exemplar: `acquire_wec_eng.py`

WEC-Eng is the **conforming exemplar** future acquire scripts copy. It defines a
typed `WecEngConfig(split, sample_size, seed)`, resolves it via
`add_config_cli` + `resolve_config`, and bakes its defaults in
[`configs/acquire-wec-eng.yaml`](./configs/acquire-wec-eng.yaml):

```bash
uv run tests/corpus/scripts/acquire_wec_eng.py                    # baked defaults
uv run tests/corpus/scripts/acquire_wec_eng.py --override split=dev
uv run tests/corpus/scripts/acquire_wec_eng.py --config my-wec-eng.yaml
```

Copy that shape for a new source: a small `@dataclass` config with a
`validate()`, then `add_config_cli(parser)` + `resolve_config(Cfg, args, Cfg())`.

### Pending migration

The other legacy `acquire_*.py` still use bespoke `argparse` flags. Retrofitting
them to typed-config is a **separate fathomdb-program migration** — not done in
this slice. New scripts must follow the WEC-Eng exemplar from the start.

## Acquisition is a data-registry concern, not an experiment

Corpus acquisition registers a **dataset**, not an experiment run. A new or
refreshed source is recorded in the DATA REGISTRY:

- [`manifest.json`](./manifest.json) — upstream pins + output sha256 contract
  (the byte-stability check).
- [`../corpus-card.md`](../corpus-card.md) — source catalogue, license posture,
  schema, provenance.

It does **not** get an entry in the experiment index (`experiments/`). That
index tracks EVAL RUNS over the corpus — retrieval/answer-quality experiments —
which are a distinct concern. Registry answers "what data exists and is it
byte-stable"; the experiment index answers "what did we measure over it". See
[`../../../experiments/README.md`](../../../experiments/README.md).

## Shared helpers

- [`_config.py`](./_config.py) — the typed-config helper (this standard).
- [`_corpus_lib.py`](./_corpus_lib.py) — the canonical `CorpusDoc` /
  `EntityRef` schema, deterministic id hashing, and JSONL writers every script
  emits through, so the on-disk shape is identical across sources.
