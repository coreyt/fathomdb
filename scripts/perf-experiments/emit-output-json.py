#!/usr/bin/env python3
"""Assemble or merge a perf-experiment closure JSON.

Reads the host JSON from --host-json, parses AC012/013/020 numbers
from each log via the existing parse-numbers module, then writes
(or merges with an existing) closure JSON to --out.

The merge rule lets dev-box pre-screen and canonical-ci runs share
one closure JSON: each writes its own sub-section
(dev_box_pre_screen vs canonical_ci) without clobbering the other.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import importlib.util

_spec = importlib.util.spec_from_file_location(
    "parse_numbers",
    str(Path(__file__).resolve().parent / "parse-numbers.py"),
)
_mod = importlib.util.module_from_spec(_spec)  # type: ignore
_spec.loader.exec_module(_mod)  # type: ignore
parse_log = _mod.parse


def aggregate_logs(log_paths: list[str]) -> dict:
    merged: dict = {}
    for p in log_paths:
        if not p or not os.path.exists(p) or os.path.getsize(p) == 0:
            continue
        merged.update(parse_log(p))
    return merged


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--exp-id", required=True)
    ap.add_argument("--lever-id", required=True)
    ap.add_argument("--head-sha", required=True)
    ap.add_argument("--branch", required=True)
    ap.add_argument("--timestamp", required=True)
    ap.add_argument("--run-location", required=True, choices=["dev-box", "canonical-ci"])
    ap.add_argument("--host-json", required=True)
    ap.add_argument("--logs", nargs="*", default=[])
    args = ap.parse_args()

    with open(args.host_json) as fh:
        host = json.load(fh)

    numbers = aggregate_logs(args.logs)

    merged: dict = {}
    if os.path.exists(args.out):
        try:
            with open(args.out) as fh:
                merged = json.load(fh)
        except Exception:
            merged = {}

    if merged.get("experiment_id") != args.exp_id:
        merged = {"experiment_id": args.exp_id, "lever_id": args.lever_id}
    else:
        merged.setdefault("lever_id", args.lever_id)

    section_key = "dev_box_pre_screen" if args.run_location == "dev-box" else "canonical_ci"
    merged[section_key] = {
        "timestamp_utc": args.timestamp,
        "branch": args.branch,
        "head_sha": args.head_sha,
        "host": host,
        "results": numbers,
    }
    merged.setdefault("verdict", "PENDING")
    merged.setdefault("evidence_notes", "")
    merged.setdefault("next_step", "")
    merged.setdefault("canonical_ci_url", None)

    with open(args.out, "w") as fh:
        json.dump(merged, fh, indent=2)

    print(json.dumps(merged, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
