#!/usr/bin/env python3

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def section(text: str, heading: str) -> str:
    pattern = rf"^## {re.escape(heading)}\n(?P<body>.*?)(?=^## |\Z)"
    match = re.search(pattern, text, flags=re.MULTILINE | re.DOTALL)
    if not match:
        raise ValueError(f"missing section: {heading}")
    return match.group("body").strip()


def parse_non_done_areas(checklist_text: str) -> list[str]:
    matrix = section(checklist_text, "Current Readiness Matrix")
    areas: list[str] = []
    for line in matrix.splitlines():
        if not line.startswith("|"):
            continue
        if line.startswith("| Area |") or line.startswith("|---|"):
            continue
        parts = [part.strip() for part in line.strip("|").split("|")]
        if len(parts) < 2:
            continue
        area, status = parts[0], parts[1]
        if status != "`done`":
            areas.append(area)
    return areas


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate doc/tracker hygiene invariants.")
    repo_root = Path(__file__).resolve().parent.parent
    parser.add_argument(
        "--todo",
        type=Path,
        default=repo_root / "dev" / "TODO-response-cycle-feedback.md",
    )
    parser.add_argument(
        "--checklist",
        type=Path,
        default=repo_root / "dev" / "production-readiness-checklist.md",
    )
    args = parser.parse_args()

    todo_text = read_text(args.todo)
    checklist_text = read_text(args.checklist)
    failures: list[str] = []

    if "- [ ]" in todo_text:
        failures.append(f"unchecked tracker items remain in {args.todo}")

    non_done_areas = parse_non_done_areas(checklist_text)

    mandatory = section(checklist_text, "Mandatory Blockers Before A Production Claim")
    recommended = section(checklist_text, "Strongly Recommended Before Wider Production Use")
    overall = section(checklist_text, "Current Overall Assessment")

    if non_done_areas:
        if "not yet production-ready" not in overall:
            failures.append("checklist overall assessment must say not yet production-ready when non-done areas remain")
    else:
        if mandatory != "None.":
            failures.append("mandatory blockers section must be 'None.' when the readiness matrix has no non-done areas")
        if recommended != "None.":
            failures.append("strongly recommended section must be 'None.' when the readiness matrix has no non-done areas")
        if "production-ready within the documented support contract" not in overall:
            failures.append(
                "checklist overall assessment must say production-ready within the documented support contract when the readiness matrix has no non-done areas"
            )

    if failures:
        for failure in failures:
            print(failure, file=sys.stderr)
        return 1

    print("doc hygiene check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
