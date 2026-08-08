# Default Embedder

FathomDB can embed your documents for you with a built-in, in-process embedder,
so you do not have to wire up an embedding model yourself. It is **opt-in**: a
fresh engine has no embedder configured and vector writes fail with
`EmbedderNotConfiguredError` until you either enable the default embedder or supply
your own (Rust only, today).

> Status: shipped; 0.8.21 is the current published release and 0.8.22 remains
> an unpublished candidate. The ANN-fidelity recall@10,
> measured on the pre-fusion **vector stage** (the SUT the 0.90 floor gates), is
> **0.896** (95% CI 0.864–0.925, N=7,667) and holds the **0.90 floor under the
> one-sided CI gate** (`recall_ci_hi ≥ 0.90`) — see
> [Caveats](#caveats-and-limitations).

## What it is

| | |
|---|---|
| Model | `BAAI/bge-small-en-v1.5` |
| Dimensions | 384 |
| Runtime | `candle-transformers` BERT, pure-Rust, in process (no Python, no sidecar) |
| Pipeline | WordPiece tokenization (max 512 tokens) → mean-pool → L2-normalize → mean-centering → sign-bit quantization → bit-KNN (K=192) → f32 rerank → top-10 |

Documents longer than 512 tokens are truncated to the model's context window.

## Enabling it

The default embedder is gated behind a build feature so users who never use it
pay no dependency or binary-size cost. Install the embedder-enabled
distribution, then opt in at `open`.

### Python

```python
from fathomdb import Engine

engine = Engine.open("mydb.sqlite", use_default_embedder=True)
report = engine.open_report()
print(report.default_embedder.name)   # "fathomdb-bge-small-en-v1.5"
```

The published Python wheel already includes the default embedder; there is no
`default-embedder` extra to install. Install the package normally:

```bash
pip install fathomdb
```

### TypeScript

```ts
import { Engine } from "fathomdb";

const engine = await Engine.open("mydb.sqlite", { useDefaultEmbedder: true });
console.log(engine.openReport().defaultEmbedder.name); // "fathomdb-bge-small-en-v1.5"
```

The embedder-enabled native binary is larger; see your platform package notes.

### Default OFF

If you do not pass the flag, the engine opens normally but has no embedder, and
vector writes raise `EmbedderNotConfiguredError`. This is unchanged from earlier
releases. Supplying your own custom embedder is available in the Rust API today;
custom Python/TypeScript embedders are planned for a later release.

## First use: weight download

The model weights are **not** bundled. The first time you open an engine with
the default embedder, FathomDB downloads the pinned weight files (~133 MB total)
from a fixed Hugging Face URL set, caches them under your platform cache
directory, and verifies every file by sha256 before loading. Subsequent opens
read from the cache and do no network I/O.

This first-use download is the single, narrowly-scoped exception to FathomDB's
"no implicit network access" rule, and it is **visible**: the open report records
what happened.

```python
report = engine.open_report()
print(report.embedder_download_ms)   # Some(ms) on a cold cache, None when warm
for ev in report.embedder_events:    # per-file url, bytes, sha256, cache path
    print(ev)
```

Notes:

- **Offline / pre-warming.** Warm the cache ahead of time (e.g. in CI or an
  air-gapped build step) so the first real open does no network I/O. A cold
  cache with no network will fail the open with a clear loader error rather than
  hanging silently.
- **`HF_TOKEN`.** If set, it is sent as a bearer token for token-gated mirrors.
  The public `bge-small-en-v1.5` does not require one. No other credential
  source is consulted, and the token is never persisted.
- **Integrity.** A file whose sha256 does not match the pinned value is removed
  and the open fails; there is no trust-on-first-use.

## Mean-centering

Real embeddings cluster on a narrow cone of the vector space, which hurts
sign-bit quantization. FathomDB corrects for this by subtracting a per-workspace
**corpus-mean vector** before the sign-bit step (the full-precision rerank stays
un-centered). The mean is computed once, from the first 256 vectors you ingest,
pinned in the workspace, and not silently recomputed thereafter.

A consequence worth knowing: if the **first 256 documents** you ingest are not
representative of the rest of your corpus (for example, you load one topic
first and pivot later), the pinned mean can be skewed and retrieval quality may
suffer. The remedy is to reindex; an automatic reindex/refresh path is planned
for a later release.

## GPU acceleration (opt-in)

The default build runs the embedder on **CPU** and is byte-for-byte the shipped,
deterministic path — nothing below changes that. GPU acceleration is an opt-in
**build-and-eval accelerator** whose largest win is bulk embedding (initial
ingest, re-indexing, offline evaluation).

Scope note: when you opt in (`FATHOMDB_EMBED_DEVICE=cuda|metal`), that device is
used for **all** embedding that engine instance performs — both ingest **and** the
per-query embedding of the query string (the engine embeds the query on the same
device the embedder was opened with). What stays **CPU-only / 1-bit (Hamming) /
deterministic regardless** is the *retrieval* machinery: the stored sign-bit
vector index, the Hamming scan over it, and RRF fusion. Because query embedding
follows the selected backend, you should query a workspace with the **same
backend that built its index** (see the cross-backend discipline below) — that is
the point of the env knob, not a footgun.

Two things turn it on, both off by default:

1. **A cargo feature** selects the backend at build time:
   - `embed-cuda` — NVIDIA CUDA (needs the CUDA toolkit, e.g. 12.6).
   - `embed-metal` — Apple Metal.
   Build the Python extension on the **main checkout** (never a worktree):
   `maturin develop --features pyo3/extension-module,embed-cuda`.
2. **The `FATHOMDB_EMBED_DEVICE` environment variable** selects the device at
   runtime: `cpu` (default) · `cuda` · `cuda:N` (GPU N) · `metal`. The device is
   resolved once, when the engine opens. If a GPU device is requested but the
   build lacks the matching feature, or device init fails, the embedder falls
   back to CPU and prints a **loud** stderr warning (it never silently runs 100×
   slower).

The device is **not** part of the embedder identity — there is no new API on
either the Python or TypeScript binding; the surface is the feature flag plus the
env var, and the default (CPU) behavior is identical across both bindings.

### Measured speedup

Re-embedding a 2,000-document corpus, single-stream, on one RTX 3090 vs CPU
(`cargo run --release --example gpu_speedup`):

| Path | CPU | CUDA (`cuda:0`) | Speedup |
|------|-----|-----------------|---------|
| per-document `embed()` | 17.8 docs/s | 276.8 docs/s | **15.6×** |
| batched `embed_batch()` | 8.8 docs/s | 821.8 docs/s | **93×** |

Bulk re-embed end-to-end (CPU per-doc → CUDA batched) is **~46×**. On CPU,
batching is *slower* than per-document calls (padding overhead with no
parallelism to amortize it); on GPU, batching wins decisively. A 27-hour CPU
re-embed becomes minutes.

### Cross-backend discipline

Stored vectors are valid only for the exact embedding function that produced
them — model, revision, **and backend numerics**. `EmbedderIdentity`
(name/revision/dim) is the cheap pre-filter: it catches a model/revision/dim
mismatch but **not** numeric divergence between two backends sharing one
identity.

Since 0.8.18 the engine also runs a **vector-equivalence self-check** at open:
45 pinned probes are compared against a stored reference baseline, and a
divergence past the floor sets `dense_disabled` so every vector-dependent arm
refuses at query time with `VectorEquivalenceMismatchError`. The FTS-only path
(`search_text_only` / `searchTextOnly`) stays serviceable.

⚠ **Since 0.8.20 that verdict is cached.** The engine fingerprints the embedder
identity, the pinned mean vector, the probe fixture, the divergence floors and
the stored baseline; an open whose fingerprint is unchanged does **zero** probe
embeds and reuses the previous verdict. So `dense_disabled` reports the arm's
status *as verified at the last open whose fingerprint differed*, not a fresh
per-open re-verification — a same-identity backend drift (CPU↔GPU, a rebuilt
library or driver) is no longer caught per-open. An identity *change* still
refuses the open ahead of any cache. **The self-check is not tamper evidence**:
nothing at rest is authenticated, so an actor with write access to the database
file can rewrite the baseline, the cached marker, or the vectors. Do not read
`dense_disabled` as a tamper signal.

A **same-backend build-and-read** discipline therefore remains the conservative
default: read an index with the same backend that built it.

In practice the divergence for this model is tiny: measured CPU-vs-CUDA cosine
was **0.99999983** (max per-component Δ ≈ 1.6e-7), and at the 1-bit sign-bit
quantization the retrieval path actually uses, **0 of 6,144 bits disagreed** —
the CUDA-built and CPU-read codes were identical on the probe set. The
same-backend rule is the conservative default; the probe-set guard will replace
it with an enforced, calibrated tolerance.

## Caveats and limitations

- **Recall.** The ANN-fidelity measurement — how faithfully the 1-bit
  sign-quant index reproduces the same model's exact f32 top-10 — is
  **recall@10 = 0.896** (95% CI 0.864–0.925) on the real bge-small embedder
  (N=7,667, K=192, mean-centering), measured on the pre-fusion **vector stage**
  (the SUT the floor gates). The **0.90 floor holds under the one-sided CI
  gate** (`recall_ci_hi 0.925 ≥ 0.90`). This is an ANN/quantization-fidelity
  number, **not** an IR-relevance number. (An earlier **0.937** figure was
  measured on a different, pre-correction `search()` SUT; the 0.937→0.896
  difference is a measurement-SUT change, not a fidelity regression, and is not
  caused by embedder pooling. An earlier ~0.83 dev-box figure was a measurement
  artifact — exclude-after-top-10 plus body-string ground truth over a corpus
  with duplicate bodies.) A full-scale N=1M run remains infeasible on commodity
  hardware; the N≈7.7k value is treated as a near-upper bound (recall declines
  slowly with N).
- **Retrieval latency grows with corpus size.** There is no ANN index — the
  vector arm is a full scan. See
  [compatibility § performance posture](compatibility/index.md).
- **Topic-drift mean** (see above): pinned on the first 256 docs; reindex to
  refresh.
- **Custom Python/TypeScript embedders** are not exposed as of 0.8.20; the
  binding surface is binary (built-in default embedder, or none).

## Upgrading an existing workspace

A workspace that was previously opened **without** the default embedder recorded
a `fathomdb-noop` embedder identity. Re-opening it with the default embedder
fails closed with an identity mismatch — this is intentional: vector identity
belongs to the embedder, and silently re-embedding under a new model would
corrupt retrieval. To adopt the default embedder on existing data, create a
fresh workspace and re-ingest (wipe-and-rewrite). There is no in-place swap.

## See also

- `OpenReport` fields: [Python](install/python.md), [TypeScript](install/typescript.md)
- Vector identity rationale: [Embedder Identity](positions/embedder-identity.md)
