# ONNX embedder export tooling (FathomDB 0.8.16 Slice 10)

Reproducible, **offline** tooling that exports `BAAI/bge-small-en-v1.5` from the
pinned local Hugging Face cache to an ONNX graph for the cross-vendor
`OrtBgeEmbedder` (`fathomdb-embedder`, feature `onnx-embedder`). This backend
reaches AMD ROCm / Intel OpenVINO / Windows DirectML that the candle default
cannot (ADR-0.8.16-onnx-embedder-backend).

The exported `.onnx` is a **generated eval asset (~133 MB) and is NOT
committed** (gitignored `*.onnx`, default export path is under `~/.cache`). The
script + this README are the committed, reproducible tooling.

## What it exports

`export_bge_small_onnx.py` loads the PINNED weights (HF revision
`5c38ec7c405ec4b44b94cc5a9bb96e735b38267a` — the same commit / weights the
candle reference pins) from the local HF cache and exports a graph whose SOLE
output is `last_hidden_state` (shape `(batch, seq, 384)`). It does **not** pool:
the Rust `OrtBgeEmbedder` applies CLS pooling + L2-norm downstream, matching the
candle path exactly. Fixed opset (14) + eval mode + fixed seed + stable
initializer order → byte-deterministic output (`--verify` asserts this).

**No model network egress.** The weights are read `local_files_only=True` from
the on-host HF cache. R-ONNX-1 is a deterministic offline export per HITL
2026-07-08.

## Toolchain

The base `python3` and the fathomdb `.venv` LACK `torch`/`transformers`, so the
export runs in a **dedicated throwaway venv** (never mutate the shared
`.venv`s):

```sh
python3 -m venv /tmp/onnx-export-venv
/tmp/onnx-export-venv/bin/pip install "torch==2.4.1" --index-url https://download.pytorch.org/whl/cpu
/tmp/onnx-export-venv/bin/pip install "transformers==4.44.2" "numpy<2" onnx
```

- `torch 2.4.1` (CPU) + `transformers 4.44.2`: transformers >=5 routes the BERT
  attention mask through a `masking_utils` path that `torch.onnx.export`'s
  TorchScript tracer cannot follow (`IndexError` in `create_bidirectional_mask`),
  so 4.44.2 is used for a clean trace. `onnx` is needed only for the final proto
  write step of the TorchScript exporter.
- `numpy<2` for torch 2.4 ABI.

(Only the pip installs above touch the network — toolchain setup, not the model.
The interpreter at `/home/coreyt/projects/memex/.venv/bin/python`, torch 2.11 +
transformers 5.4, can be read read-only but its transformers 5.x breaks the
TorchScript trace, so the throwaway venv is preferred.)

## Run

```sh
/tmp/onnx-export-venv/bin/python dev/tools/onnx/export_bge_small_onnx.py --verify
```

Default output: `~/.cache/fathomdb/embedders/onnx/bge-small-en-v1.5/model.onnx`
(override with `--out`). `--verify` exports twice and asserts a byte-identical
SHA-256.

## ONNX Runtime native lib (Task B — offline, no download)

`ort` is built `default-features = false` + `load-dynamic`, so the ONNX Runtime
native lib is dlopen'd at RUNTIME via `ORT_DYLIB_PATH` — `cargo build`/`check`
never download or link a native binary. An **on-host** `libonnxruntime.so.1.26.0`
(bundled in the fathomdb `.venv` onnxruntime wheel) is ABI-compatible with
`ort =2.0.0-rc.10`; no download is needed:

```sh
ORT_DYLIB_PATH=/home/coreyt/projects/fathomdb/.venv/lib/python3.12/site-packages/onnxruntime/capi/libonnxruntime.so.1.26.0
```

## Run the R-ONNX-1 real-vector test (CPU same-backend baseline)

The test is env-gated (skips cleanly when the asset is absent, e.g. on CI):

```sh
export ORT_DYLIB_PATH=/home/coreyt/projects/fathomdb/.venv/lib/python3.12/site-packages/onnxruntime/capi/libonnxruntime.so.1.26.0
export FATHOMDB_ONNX_MODEL_PATH=~/.cache/fathomdb/embedders/onnx/bge-small-en-v1.5/model.onnx
export FATHOMDB_ONNX_TOKENIZER_PATH=~/.cache/huggingface/hub/models--BAAI--bge-small-en-v1.5/snapshots/5c38ec7c405ec4b44b94cc5a9bb96e735b38267a/tokenizer.json
cargo test -p fathomdb-embedder --features onnx-embedder ort_bge_embeds_384 -- --nocapture
```

It forces the ONNX **CPU** EP (the same-backend fidelity baseline per policy
`649a8d45`; GPU is a Slice-15 re-embed speed concern) and asserts a 384-dim,
finite, L2-normalized, deterministic vector via the `Embedder` trait, with the
identity revision self-describing the loaded asset digest.
