#!/usr/bin/env python3
"""Lint FathomDB migration files for schema accretion."""

from __future__ import annotations

import re
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parents[1]
DEFAULT_TARGET = REPO / "src/rust/crates/fathomdb-schema/migrations"
EXEMPTION = "-- MIGRATION-ACCRETION-EXEMPTION: "


def migration_files(paths: list[str]) -> list[Path]:
    if not paths:
        paths = [str(DEFAULT_TARGET)]
    files: list[Path] = []
    for raw in paths:
        path = Path(raw)
        if path.is_dir():
            files.extend(sorted(path.glob("*.sql")))
        else:
            files.append(path)
    return files


def is_post_v1(path: Path) -> bool:
    match = re.match(r"^(\d+)", path.name)
    return match is None or int(match.group(1)) > 1


def violates(path: Path) -> bool:
    if not is_post_v1(path):
        return False
    sql = path.read_text(encoding="utf-8")
    upper = sql.upper()
    adds_schema = "CREATE TABLE" in upper or "ADD COLUMN" in upper
    names_removal = "DROP TABLE" in upper or "DROP COLUMN" in upper
    has_exemption = EXEMPTION in sql
    return adds_schema and not names_removal and not has_exemption


def main() -> int:
    offenders = [path for path in migration_files(sys.argv[1:]) if violates(path)]
    for offender in offenders:
        print(f"migration accretion violation: {offender}", file=sys.stderr)
    return 1 if offenders else 0


if __name__ == "__main__":
    raise SystemExit(main())
