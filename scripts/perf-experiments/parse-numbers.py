#!/usr/bin/env python3
"""Extract AC012_NUMBERS / AC013_NUMBERS / AC020_NUMBERS lines from a
perf-gates run log. Emits a single JSON object on stdout with all
fields it could parse; missing fields are absent (not null).

Usage:
    python3 parse-numbers.py <log_path>
"""
from __future__ import annotations

import json
import re
import sys

PATTERNS = {
    "ac012": re.compile(
        r"AC012_NUMBERS\s+n=(?P<n>\d+)\s+samples=(?P<samples>\d+)"
        r"\s+seed_ms=(?P<seed>\d+)\s+p50_ms=(?P<p50>\d+)\s+p99_ms=(?P<p99>\d+)"
    ),
    "ac013": re.compile(
        r"AC013_NUMBERS\s+n=(?P<n>\d+)\s+samples=(?P<samples>\d+)"
        r"\s+seed_ms=(?P<seed>\d+)\s+p50_ms=(?P<p50>\d+)\s+p99_ms=(?P<p99>\d+)"
    ),
    "ac020": re.compile(
        r"AC020_NUMBERS\s+sequential_ms=(?P<seq>\d+)\s+concurrent_ms=(?P<conc>\d+)"
        r"\s+bound_ms=(?P<bound>\d+)"
    ),
}


def parse(path: str) -> dict:
    out: dict = {}
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        text = fh.read()
    m = PATTERNS["ac012"].search(text)
    if m:
        out["ac012"] = {
            "n": int(m["n"]),
            "samples": int(m["samples"]),
            "seed_ms": int(m["seed"]),
            "p50_ms": int(m["p50"]),
            "p99_ms": int(m["p99"]),
        }
    m = PATTERNS["ac013"].search(text)
    if m:
        out["ac013"] = {
            "n": int(m["n"]),
            "samples": int(m["samples"]),
            "seed_ms": int(m["seed"]),
            "p50_ms": int(m["p50"]),
            "p99_ms": int(m["p99"]),
        }
    m = PATTERNS["ac020"].search(text)
    if m:
        seq = int(m["seq"])
        conc = int(m["conc"])
        bound = int(m["bound"])
        speedup = round(seq / conc, 3) if conc > 0 else 0.0
        out["ac020"] = {
            "sequential_ms": seq,
            "concurrent_ms": conc,
            "bound_ms": bound,
            "speedup": speedup,
        }
    return out


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: parse-numbers.py <log_path>", file=sys.stderr)
        return 2
    print(json.dumps(parse(argv[1]), separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
