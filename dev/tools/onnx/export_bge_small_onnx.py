#!/usr/bin/env python3
"""Offline, deterministic safetensors -> ONNX export of BAAI/bge-small-en-v1.5.

FathomDB 0.8.16 Slice 10 (R-ONNX-1). Loads the PINNED bge-small weights from the
LOCAL Hugging Face cache (NO network egress) and exports a deterministic ONNX
graph whose output is the raw token embeddings (`last_hidden_state`, shape
`(batch, seq, 384)`). The Rust `OrtBgeEmbedder` applies CLS pooling + L2-norm
downstream, matching the candle reference exactly, so this graph must NOT pool.

Determinism: the model is loaded in eval mode with a fixed seed, a fixed opset,
and `torch.onnx.export` writes initializers in a stable order, so re-running on
the same host/toolchain yields a byte-identical `.onnx`. The export is verified
byte-stable by hashing two independent exports (see `--verify`).

The exported `.onnx` is a GENERATED EVAL ASSET and is NOT committed (it is
~130 MB); this script + its README are the committed, reproducible tooling.

Toolchain (documented in README.md):
  - An interpreter with `torch` (>=2) + `transformers` (>=4.30). The base
    `python3` and the fathomdb `.venv` LACK these; a read-only interpreter that
    HAS them is at `/home/coreyt/projects/memex/.venv/bin/python`
    (torch 2.11.0 + transformers 5.4.0). This script does NOT install anything.
  - The `onnx` / `optimum` packages are NOT required: `torch.onnx.export`
    (TorchScript path) writes the `.onnx` without them.

Usage:
  /home/coreyt/projects/memex/.venv/bin/python \
      dev/tools/onnx/export_bge_small_onnx.py \
      [--out ~/.cache/fathomdb/embedders/onnx/bge-small-en-v1.5/model.onnx] \
      [--opset 14] [--verify]
"""

from __future__ import annotations

import argparse
import hashlib
import os
import sys
from pathlib import Path

# Pinned HF revision — the same commit the candle loader pins
# (fathomdb-embedder `loader::HF_REVISION` / ort_bge `HF_REVISION`).
HF_REVISION = "5c38ec7c405ec4b44b94cc5a9bb96e735b38267a"
MODEL_ID = "BAAI/bge-small-en-v1.5"
HIDDEN = 384
DEFAULT_OUT = "~/.cache/fathomdb/embedders/onnx/bge-small-en-v1.5/model.onnx"
DEFAULT_OPSET = 14


def _hub_cache() -> str:
    return os.environ.get(
        "HF_HUB_CACHE",
        os.path.expanduser("~/.cache/huggingface/hub"),
    )


def _pinned_snapshot_dir() -> Path:
    """Locate the EXACTLY pinned local HF snapshot; FAIL CLOSED otherwise.

    The export is OFFLINE and **refuses to fall back** to any other local BGE
    snapshot. A different snapshot would silently export NON-reference weights
    while the ADR/README + the HF_REVISION-based ONNX identity still claim the
    candle-pinned weights — invalidating the candle<->ONNX equivalence
    measurement (Slice 15). The pinned revision must be present, or we abort.
    """
    base = Path(_hub_cache()) / "models--BAAI--bge-small-en-v1.5" / "snapshots"
    exact = base / HF_REVISION
    if not exact.is_dir():
        present = sorted(p.name for p in base.glob("*")) if base.is_dir() else []
        sys.exit(
            "FAIL-CLOSED: the pinned bge-small revision is NOT in the local HF "
            "cache.\n"
            f"  expected snapshot: {exact}\n"
            f"  snapshots present: {present or '(none)'}\n"
            "This export is OFFLINE (it will NOT download) and refuses to fall "
            "back to any other snapshot: a non-pinned snapshot would silently "
            "produce a non-reference ONNX asset and break the candle<->ONNX "
            "equivalence measurement.\n"
            "To obtain the EXACT pinned revision offline, on a networked host "
            "run:\n"
            f"  huggingface-cli download {MODEL_ID} --revision {HF_REVISION}\n"
            "then copy ~/.cache/huggingface/hub/models--BAAI--bge-small-en-v1.5 "
            "onto this host."
        )
    for required in ("config.json", "tokenizer.json"):
        if not (exact / required).exists():
            sys.exit(
                f"FAIL-CLOSED: pinned snapshot {exact} is missing {required!r}; "
                "refusing to export from an incomplete snapshot."
            )
    return exact


def _assert_snapshot_revision(snap: Path) -> None:
    """Independently confirm, via the HF cache metadata, that `snap` is the
    snapshot for commit HF_REVISION — not merely a directory named like it.

    Resolves the source snapshot's actual commit hash from Hugging Face's own
    cache index (`scan_cache_dir`) and asserts it equals HF_REVISION, naming the
    expected-vs-found revision on mismatch. This makes the exported ONNX asset
    provably from the same weights as the candle reference, or the export
    refuses to run.
    """
    try:
        from huggingface_hub import scan_cache_dir
    except Exception as e:  # huggingface_hub ships with transformers
        sys.exit(f"cannot import huggingface_hub to verify the revision: {e}")

    seen: list[str] = []
    for repo in scan_cache_dir(_hub_cache()).repos:
        if repo.repo_id != MODEL_ID:
            continue
        for rev in repo.revisions:
            seen.append(rev.commit_hash)
            if rev.commit_hash == HF_REVISION:
                resolved = Path(rev.snapshot_path).resolve()
                if resolved != snap.resolve():
                    sys.exit(
                        "revision/snapshot mismatch: cached commit "
                        f"{HF_REVISION} resolves to {resolved}, expected "
                        f"{snap.resolve()}. Refusing to export."
                    )
                return
    sys.exit(
        "revision-integrity check FAILED: expected pinned commit "
        f"(expected={HF_REVISION}) is not among the cached {MODEL_ID} "
        f"revisions (found={sorted(set(seen)) or '(none)'}). Refusing to export "
        "a non-reference asset."
    )


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def export(out_path: Path, opset: int, snap: Path) -> Path:
    import numpy as np
    import torch
    from transformers import AutoModel

    # Determinism knobs.
    torch.manual_seed(0)
    np.random.seed(0)
    torch.use_deterministic_algorithms(True, warn_only=True)

    print(
        f"loading pinned weights (rev {HF_REVISION}) from {snap}",
        file=sys.stderr,
    )

    # `snap` is the EXACT pinned-revision snapshot (fail-closed + revision-
    # integrity verified by main() before we get here), so loading it directly
    # with local_files_only provably uses the same weights as the candle
    # reference — no silent fallback to another snapshot is possible.
    # local_files_only: hard-offline, never touch the network.
    # attn_implementation="eager": the SDPA mask path in transformers >=5
    # indexes tensor shapes in a way TorchScript tracing cannot follow
    # (IndexError in create_bidirectional_mask); eager attention builds the
    # additive mask with plain ops that trace cleanly and is numerically
    # equivalent for this inference-only export.
    model = AutoModel.from_pretrained(
        str(snap),
        local_files_only=True,
        torch_dtype=torch.float32,
        attn_implementation="eager",
    )
    model.eval()

    # A representative fixed-shape example. dynamic_axes make batch + seq free.
    seq = 16
    input_ids = torch.zeros(1, seq, dtype=torch.long)
    attention_mask = torch.ones(1, seq, dtype=torch.long)
    token_type_ids = torch.zeros(1, seq, dtype=torch.long)

    class LastHiddenState(torch.nn.Module):
        """Wrap BertModel so the SOLE ONNX output is last_hidden_state."""

        def __init__(self, m):
            super().__init__()
            self.m = m

        def forward(self, input_ids, attention_mask, token_type_ids):
            out = self.m(
                input_ids=input_ids,
                attention_mask=attention_mask,
                token_type_ids=token_type_ids,
            )
            return out.last_hidden_state

    wrapped = LastHiddenState(model)
    wrapped.eval()

    out_path = Path(os.path.expanduser(str(out_path)))
    out_path.parent.mkdir(parents=True, exist_ok=True)

    with torch.no_grad():
        torch.onnx.export(
            wrapped,
            (input_ids, attention_mask, token_type_ids),
            str(out_path),
            input_names=["input_ids", "attention_mask", "token_type_ids"],
            output_names=["last_hidden_state"],
            dynamic_axes={
                "input_ids": {0: "batch", 1: "seq"},
                "attention_mask": {0: "batch", 1: "seq"},
                "token_type_ids": {0: "batch", 1: "seq"},
                "last_hidden_state": {0: "batch", 1: "seq"},
            },
            opset_version=opset,
            do_constant_folding=True,
            export_params=True,
            dynamo=False,
        )

    size = out_path.stat().st_size
    print(f"wrote {out_path} ({size} bytes)", file=sys.stderr)
    print(f"sha256 {_sha256(out_path)}", file=sys.stderr)
    print(f"tokenizer: {snap / 'tokenizer.json'}", file=sys.stderr)
    return out_path


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", default=DEFAULT_OUT, help="output .onnx path")
    ap.add_argument("--opset", type=int, default=DEFAULT_OPSET)
    ap.add_argument(
        "--verify",
        action="store_true",
        help="export twice to a temp file and assert byte-identical output",
    )
    args = ap.parse_args()

    # Fail-closed BEFORE the heavy torch import: resolve the exact pinned
    # snapshot (no fallback) and independently verify its commit == HF_REVISION.
    snap = _pinned_snapshot_dir()
    _assert_snapshot_revision(snap)

    out = export(Path(args.out), args.opset, snap)

    if args.verify:
        tmp = out.with_suffix(".verify.onnx")
        export(tmp, args.opset, snap)
        a, b = _sha256(out), _sha256(tmp)
        tmp.unlink()
        if a != b:
            sys.exit(f"NON-DETERMINISTIC export: {a} != {b}")
        print(f"VERIFIED byte-identical across two exports: {a}", file=sys.stderr)


if __name__ == "__main__":
    main()
