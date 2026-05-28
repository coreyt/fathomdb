# FathomDB 0.7.0 corpus creation

How the composite real-data + synthetic test corpus that lives under
`data/corpus-data/` was sourced, built, ingested, and validated. The
intent of this directory is **reproducibility**: a reader picking
this up cold should be able to rebuild the corpus end-to-end (or
extend it) without re-deriving the design.

This is a working-doc directory, not a hard ADR. It mirrors the
shipped state on `main` as of the Corpus-Pack 1..4 landings
(2026-05-27). When the corpus shape changes (new source, new chain
type, new validation gate), update the relevant section here.

## Read order

1. **[`architecture.md`](./architecture.md)** — the long-form design
   doc. Source selection rationale, license posture, document
   schema, chain-generator design, ingest harness, validation gates,
   known issues. Read end-to-end before changing anything.
2. **[`extending.md`](./extending.md)** — how to add a new source,
   chain shape, or validation test without breaking determinism or
   the license posture story.

The shipped artifacts referenced here:

- `tests/corpus/corpus-card.md` — operational summary (sources,
  licenses, output checksums).
- `tests/corpus/scripts/manifest.json` — machine-readable upstream
  pin + per-output sha256 contract.
- `tests/corpus/scripts/` — acquisition + generation scripts.
- `tests/corpus/chains/` — synthetic cross-doc chain definitions
  (committed; 200 JSON files).
- `data/corpus-data/` — produced JSONL + raw downloads
  (**gitignored**; rebuilt locally or restored from CI cache).
- `src/rust/crates/fathomdb-engine/examples/ingest_corpus.rs` —
  Rust CLI ingest harness.
- `src/rust/crates/fathomdb-engine/tests/corpus_{fts,vector,graph}.rs`
  + `tests/support/corpus_subset.rs` — Pack-4 validation gates.
- `dev/notes/0.7.0-engine-batch-vec0-collapse.md` — open engine bug
  surfaced during this work; affects how the harness batches.

The original implementation handoff is
[`dev/plans/prompts/0.7.0-CORPUS-BUILD-HANDOFF.md`](../plans/prompts/0.7.0-CORPUS-BUILD-HANDOFF.md);
the source-selection research is
[`dev/notes/0.7.0-test-corpus-research.md`](../notes/0.7.0-test-corpus-research.md).
HITL decisions are locked in
[`dev/plans/0.7.0-HITL-recommendations.md`](../plans/0.7.0-HITL-recommendations.md).

## Quick rebuild (TL;DR)

From the repo root, on a machine with `uv` and a Rust toolchain
installed:

```bash
# Acquire / regenerate every source. Order doesn't matter except
# that Pack 2 (chain generator) reads the Pack-1 JSONLs and so
# must run after them.
uv run tests/corpus/scripts/acquire_cnn_dailymail.py
uv run tests/corpus/scripts/acquire_landes_todos.py
uv run tests/corpus/scripts/acquire_bahmutov_dailylogs.py
uv run tests/corpus/scripts/acquire_enron.py        # downloads 443 MB tarball
uv run tests/corpus/scripts/acquire_qmsum.py
uv run tests/corpus/scripts/acquire_enronqa.py
uv run tests/corpus/scripts/generate_synthetic_notes.py
uv run tests/corpus/scripts/generate_chain_corpus.py   # depends on the above

# Validate every produced JSONL's sha256 matches the manifest
python3 - <<'EOF'
import hashlib, json
m = json.load(open('tests/corpus/scripts/manifest.json'))
for name, src in m['sources'].items():
    p = src['output']
    h = hashlib.sha256(open(p, 'rb').read()).hexdigest()
    status = 'OK' if h == src['sha256'] else 'MISMATCH'
    print(f"  {status:8s} {name:20s} {p}")
EOF

# Ingest into a fresh FathomDB instance. ~2s on dev-box for the
# 7,667-doc full corpus.
cargo run --example ingest_corpus -p fathomdb-engine -- \
  --db tests/corpus/.cache/db \
  --jsonl-dir data/corpus-data/raw \
  --chains-dir tests/corpus/chains

# Run the validation gates (FTS / vector wiring / graph).
cargo test --tests -p fathomdb-engine corpus_
```

If you only have one of those toolchains, see
[`architecture.md`](./architecture.md) §"Build sequencing" for the
minimal path.

## What this corpus is for

Three jobs, ordered by load-bearing:

1. **Drive Pack 2's recall-floor RED test** (recall@10 ≥ 0.90 vs
   f32 brute-force ground truth at canonical scale). The
   200-chain `ground_truth_queries` set provides the
   `expected_top_k_doc_ids` the test scores against.
2. **Validate the post-PVQ-Pack-1 vec0 schema** (`bit[768]` + f32
   rerank + metadata + `source_type` partition_key) on realistic
   doc shapes — not just the synthetic AC-013 latency fixture.
3. **Establish a cross-document retrieval baseline** — measure
   whether retrieval reconstructs the user's working context
   (email → meeting → todo → note chains) rather than just
   surfacing semantically-similar docs.

## What it deliberately is NOT

- **Not a perf benchmark.** Latency / cost / throughput are owned by
  `tests/perf_gates.rs` (`AC-012`, `AC-013`, `AC-019`, `AC-020`).
  The corpus feeds those tests as a fixture; it does not measure
  them.
- **Not a production-redistributable dataset.** Several sources are
  cache-only per the license posture table. Don't import this work
  into a downstream product without re-checking each source's
  current license.
- **Not a replacement for the synthetic AC-013 fixture.** That
  fixture still exists and still owns the 1M-row latency gate.
  This corpus is roughly 10K docs — a different scale, a different
  shape, and a different purpose.

## What landed when

| Pack | What | Commit on main |
|---|---|---|
| Scaffold | corpus-card.md + dir layout | `b1ce692` |
| Pack 1 | 7 acquisition sources, 7,100 docs | `5c1e92a` … `bc92807` |
| Pack 2 | chain generator (200 chains, 367 connectives) | `9d50093` |
| Pack 3 | ingest harness (Rust example) | `021d2a0` + `5c4db46` |
| Pack 4 | FTS / vector wiring / graph gates | `d9a219d` |
| (merged via PR #80) | | `974cd67` |
