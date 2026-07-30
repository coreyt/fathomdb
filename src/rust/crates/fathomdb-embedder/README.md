# fathomdb-embedder

Built-in embedder and reranker runtimes for **FathomDB**, a local-first embedded retrieval engine
built on SQLite.

This crate implements the [`fathomdb-embedder-api`](https://crates.io/crates/fathomdb-embedder-api)
`Embedder` trait, and carries the weight-loading, caching and integrity-checking machinery that
turns a pinned model name into a usable embedder.

## Is this the crate you want?

If you just want FathomDB to embed things for you, depend on
[`fathomdb`](https://crates.io/crates/fathomdb) or
[`fathomdb-engine`](https://crates.io/crates/fathomdb-engine) and turn on the engine's
`default-embedder` feature — the engine pulls this crate in.

Depend on it directly if you want to construct an embedder yourself, or want the reranker without
the engine.

## Status: pre-1.0, beta

The 0.8.x line is under active development. The public surface can change between minor releases.

## Defaults cost nothing

With no features enabled, this crate has **one dependency**
(`fathomdb-embedder-api`), pulls in no model runtime, and performs no
network access. Everything below is opt-in at build time.

## Features

| Feature | What it adds |
| --- | --- |
| `default-embedder` | `CandleBgeEmbedder` — the pinned default (`fathomdb-bge-small-en-v1.5`, 384 dimensions) on the [candle](https://github.com/huggingface/candle) runtime — plus `NomicEmbedder` (768 dimensions) and the `loader` module. |
| `onnx-embedder` | `OrtBgeEmbedder`, the same BGE model under ONNX Runtime, carrying its own distinct embedder identity (`fathomdb-bge-small-en-v1.5-onnx`). Useful where the candle stack is not viable. |
| `default-reranker` | `CandleTinyBertReranker` (`fathomdb-ms-marco-TinyBERT-L2-v2`) — cross-encoder scoring of `(query, passage)` pairs. |
| `embed-cuda` / `embed-metal` | GPU execution for the embedder. |
| `rerank-cuda` / `rerank-metal` | GPU execution for the reranker. |

GPU features are build-time opt-ins; the device is then selected at runtime via
`FATHOMDB_EMBED_DEVICE` / `FATHOMDB_RERANK_DEVICE`. The default is always CPU.

## Weight loading

The `loader` module (under `default-embedder`) fetches pinned weights on first use, verifies them by
SHA-256, and caches them under `~/.cache/fathomdb/embedders/`. Subsequent loads are cache hits and
touch no network. Both outcomes are reported as structured `EmbedderEvent`s, which the engine
surfaces in its open report — so "did this open hit the network?" is an answerable question rather
than a guess.

## Install

```bash
cargo add fathomdb-embedder --features default-embedder
```

## License

MIT. See the `LICENSE` file shipped in this crate.

Source, issues and full documentation: <https://github.com/coreyt/fathomdb>
