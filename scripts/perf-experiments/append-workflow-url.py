#!/usr/bin/env python3
"""Append a workflow URL into an experiment closure JSON.

Usage: append-workflow-url.py <closure_json_path> <workflow_url>
"""
from __future__ import annotations

import json
import sys


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print("usage: append-workflow-url.py <closure_json_path> <workflow_url>", file=sys.stderr)
        return 2
    path, url = argv[1], argv[2]
    with open(path) as fh:
        doc = json.load(fh)
    doc.setdefault("canonical_ci", {})["workflow_url"] = url
    doc["canonical_ci_url"] = url
    with open(path, "w") as fh:
        json.dump(doc, fh, indent=2)
    print(f"appended workflow_url={url} to {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
