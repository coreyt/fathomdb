#!/usr/bin/env python3
"""Aggregate all 0.7.0-PERF-EXP-*-output.json files into a master
markdown results table. Output target documented in
dev/plans/0.7.0-perf-experiments.md § Aggregator.

Usage:
    python3 aggregate.py --runs-dir dev/plans/runs \
      --out dev/plans/runs/0.7.0-perf-experiments-results.md
"""
from __future__ import annotations

import argparse
import glob
import json
import os
import sys
from typing import Any


def load_all(runs_dir: str) -> list[dict[str, Any]]:
    pattern = os.path.join(runs_dir, "0.7.0-PERF-EXP-*-output.json")
    out: list[dict[str, Any]] = []
    for path in sorted(glob.glob(pattern)):
        try:
            with open(path) as fh:
                out.append({"_path": path, **json.load(fh)})
        except Exception as exc:
            print(f"warn: could not parse {path}: {exc}", file=sys.stderr)
    return out


def fmt_dev(rec: dict[str, Any]) -> str:
    box = rec.get("dev_box_pre_screen") or {}
    res = box.get("results") or {}
    parts = []
    if "ac012" in res:
        ac = res["ac012"]
        parts.append(f"AC012 p50={ac.get('p50_ms','?')}/p99={ac.get('p99_ms','?')} n={ac.get('n','?')}")
    if "ac020" in res:
        ac = res["ac020"]
        parts.append(f"AC020 seq={ac.get('sequential_ms','?')}/conc={ac.get('concurrent_ms','?')}/×{ac.get('speedup','?')}")
    if "ac013" in res:
        ac = res["ac013"]
        parts.append(f"AC013 p50={ac.get('p50_ms','?')}/p99={ac.get('p99_ms','?')} n={ac.get('n','?')}")
    return "<br>".join(parts) if parts else "—"


def fmt_canonical(rec: dict[str, Any]) -> str:
    box = rec.get("canonical_ci") or {}
    res = box.get("results") or {}
    parts = []
    if "ac012" in res:
        ac = res["ac012"]
        parts.append(f"AC012 p50={ac.get('p50_ms','?')}/p99={ac.get('p99_ms','?')}")
    if "ac020" in res:
        ac = res["ac020"]
        parts.append(f"AC020 seq={ac.get('sequential_ms','?')}/conc={ac.get('concurrent_ms','?')}/×{ac.get('speedup','?')}")
    if "ac013" in res:
        ac = res["ac013"]
        parts.append(f"AC013 p50={ac.get('p50_ms','?')}/p99={ac.get('p99_ms','?')}")
    return "<br>".join(parts) if parts else "—"


def render(recs: list[dict[str, Any]]) -> str:
    lines: list[str] = []
    lines.append("# 0.7.0 perf-experiments — aggregated results")
    lines.append("")
    lines.append("Auto-generated from `dev/plans/runs/0.7.0-PERF-EXP-*-output.json` by `scripts/perf-experiments/aggregate.py`.")
    lines.append("")
    lines.append("| Exp | Lever | Verdict | Dev-box | Canonical CI | Workflow URL | Branch | SHA |")
    lines.append("| --- | ----- | ------- | ------- | ------------ | ------------ | ------ | --- |")
    for rec in recs:
        exp = rec.get("experiment_id", "?")
        lever = rec.get("lever_id", "?")
        verdict = rec.get("verdict", "PENDING")
        dev = fmt_dev(rec)
        canon = fmt_canonical(rec)
        url_raw = (rec.get("canonical_ci") or {}).get("workflow_url") or rec.get("canonical_ci_url") or ""
        # Wrap real URLs as an autolink (<...>) so the emitted table cell does not
        # trip MD034/no-bare-urls; keep the em-dash placeholder when absent.
        url = f"<{url_raw}>" if url_raw else "—"
        branch = (rec.get("canonical_ci") or rec.get("dev_box_pre_screen") or {}).get("branch") or "—"
        sha = ((rec.get("canonical_ci") or rec.get("dev_box_pre_screen") or {}).get("head_sha") or "—")[:8]
        lines.append(f"| {exp} | {lever} | {verdict} | {dev} | {canon} | {url} | {branch} | {sha} |")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs-dir", default="dev/plans/runs")
    ap.add_argument("--out", default="dev/plans/runs/0.7.0-perf-experiments-results.md")
    args = ap.parse_args()

    recs = load_all(args.runs_dir)
    text = render(recs)
    with open(args.out, "w") as fh:
        fh.write(text)
    print(f"aggregated {len(recs)} experiment(s) into {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
