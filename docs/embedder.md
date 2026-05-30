# Default Embedder

FathomDB can embed your documents for you with a built-in, in-process embedder,
so you do not have to wire up an embedding model yourself. It is **opt-in**: a
fresh engine has no embedder configured and vector writes fail with
`EmbedderNotConfigured` until you either enable the default embedder or supply
your own (Rust only, today).

> Status: the default embedder ships in 0.7.1. The real-corpus recall floor and
> full-scale acceptance validation are being finalized in the 0.7.2 hardening
> release — see [Caveats](#caveats-and-limitations). Treat the retrieval-quality
> numbers in this release as preliminary.

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

Install the embedder extra:

```bash
pip install "fathomdb[default-embedder]"
```

### TypeScript

```ts
import { engineOpen } from "fathomdb";

const engine = await engineOpen("mydb.sqlite", { useDefaultEmbedder: true });
console.log(engine.openReport().defaultEmbedder.name); // "fathomdb-bge-small-en-v1.5"
```

The embedder-enabled native binary is larger; see your platform package notes.

### Default OFF

If you do not pass the flag, the engine opens normally but has no embedder, and
vector writes raise `EmbedderNotConfigured`. This is unchanged from earlier
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

## Caveats and limitations

- **Retrieval quality is still being validated.** In 0.7.1, dev-box measurement
  over the reference corpus put recall@10 around 0.83 — below the 0.90 figure
  used by the synthetic test fixture. The real-corpus recall floor and
  full-scale (N=1M) acceptance validation are being finalized in 0.7.2. Do not
  treat a specific recall number from this release as final.
- **Topic-drift mean** (see above): pinned on the first 256 docs; reindex to
  refresh.
- **Custom Python/TypeScript embedders** are deferred to a later release; the
  0.7.1 binding surface is binary (default-on or none).

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
