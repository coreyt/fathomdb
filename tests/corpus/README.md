# tests/corpus/

FathomDB composite test corpus: real-data documents, synthetic
cross-document chains, and eval-only QA artifacts for retrieval
validation.

Start here for operation. For source/license details, see
[`corpus-card.md`](./corpus-card.md). For design rationale and ingest
semantics, see
[`dev/corpus-creation/architecture.md`](../../dev/corpus-creation/architecture.md).

## Layout

```
tests/corpus/
├── corpus-card.md       # source catalogue, licenses, schema, checksums
├── chains/              # committed eval-only cross-doc chain specs
└── scripts/             # reproducible acquisition / generation scripts
    ├── _corpus_lib.py   # shared CorpusDoc + eval QA helpers
    ├── acquire_*.py     # per-source fetch + normalize
    ├── generate_*.py    # deterministic synthetic generators
    └── manifest.json    # upstream pins + output sha256 contract

data/corpus-data/        # produced data; gitignored
├── downloads/           # raw upstream artifacts, reused across runs
├── raw/                 # canonical corpus document JSONL
└── eval/                # eval-only QA JSONL, not ingested as documents
```

Scripts in `tests/corpus/scripts/` are the reproducible source of
truth. Produced data is rebuilt locally or restored from CI cache.

## Build

From the repo root, with `uv`, Python, and a Rust toolchain available:

```bash
uv run tests/corpus/scripts/acquire_cnn_dailymail.py
uv run tests/corpus/scripts/acquire_landes_todos.py
uv run tests/corpus/scripts/acquire_bahmutov_dailylogs.py
uv run tests/corpus/scripts/acquire_enron.py        # downloads ~443 MB once
uv run tests/corpus/scripts/acquire_qmsum.py
uv run tests/corpus/scripts/acquire_enronqa.py
python3 tests/corpus/scripts/acquire_qaconv.py
python3 tests/corpus/scripts/acquire_qasper.py
uv run tests/corpus/scripts/generate_synthetic_notes.py
uv run tests/corpus/scripts/generate_chain_corpus.py
```

Enron caches its tarball at
`data/corpus-data/downloads/enron_mail_20150507.tar.gz`; override with
`ENRON_CACHE_TARBALL` if needed.

## Verify Outputs

Every raw corpus output must match `scripts/manifest.json`:

```bash
python3 - <<'EOF'
import hashlib, json

manifest = json.load(open("tests/corpus/scripts/manifest.json"))
for name, src in manifest["sources"].items():
    path = src["output"]
    digest = hashlib.sha256(open(path, "rb").read()).hexdigest()
    status = "OK" if digest == src["sha256"] else "MISMATCH"
    print(f"{status:8s} {name:20s} {path}")
EOF
```

Some sources also emit `eval_output` files under
`data/corpus-data/eval/`. Those rows are evaluation artifacts only; do
not ingest them as corpus documents.

## Ingest Into FathomDB

Use the Rust ingest harness after the raw JSONLs and chain specs exist:

```bash
cargo run --example ingest_corpus -p fathomdb-engine -- \
  --db tests/corpus/.cache/db \
  --jsonl-dir data/corpus-data/raw \
  --chains-dir tests/corpus/chains
```

The harness maps each JSONL row to a FathomDB node. When
`parent_doc_id` points to another ingested corpus doc, it also writes an
edge whose kind comes from the child document's `relation:<kind>` tag.
Chain JSON files and eval QA JSONLs are not ingested; they are ground
truth for validation and accuracy work.

See
[`src/rust/crates/fathomdb-engine/examples/ingest_corpus.rs`](../../src/rust/crates/fathomdb-engine/examples/ingest_corpus.rs)
for the exact CLI and mapping.

## Run Validation Gates

```bash
cargo test --tests -p fathomdb-engine corpus_
```

The corpus tests skip when `data/corpus-data/raw/` is absent. If the
directory exists but contains only a partial corpus, the tests may run
against incomplete data and fail. Remove the partial generated data or
finish the full corpus build before using the gates.

## Extend

Use [`dev/corpus-creation/extending.md`](../../dev/corpus-creation/extending.md)
for source additions, chain-shape additions, and validation gates. Keep
`source_type` within the locked vocabulary:

```
email, meeting, paper, article, note, todo
```

Do not add a seventh `source_type` without updating the corpus design
and the ingest/test contracts.
