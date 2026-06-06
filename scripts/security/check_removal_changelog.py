#!/usr/bin/env python3
"""AC-050c removal-detect linter.

Scans a unified diff for removed public API symbols across the Rust,
Python, and TypeScript surfaces and asserts that every removed symbol
is announced in CHANGELOG.md under a heading containing the word
``Removed`` (case-insensitive).

Exit codes:
    0 — every removal documented (or none found).
    1 — one or more removals missing from CHANGELOG.
    2 — invocation error (bad args, missing CHANGELOG).
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


SCAN_PREFIXES = ("src/rust/crates/", "src/python/", "src/ts/")

RUST_PUBLIC = re.compile(
    r"^\s*pub(?:\([^)]+\))?\s+(?:unsafe\s+|async\s+|const\s+|extern\s+(?:\"[^\"]+\"\s+)?)*"
    r"(fn|struct|enum|trait|const|type|static|mod)\s+([A-Za-z_][A-Za-z0-9_]*)"
)
# Python: top-level def/class. Indented members are not part of the
# public surface (their owning class is what counts).
PY_PUBLIC = re.compile(r"^(def|class)\s+([A-Za-z_][A-Za-z0-9_]*)")
TS_PUBLIC = re.compile(
    r"^\s*export\s+(?:default\s+)?(?:async\s+)?"
    r"(function|class|const|let|var|type|interface|enum)\s+([A-Za-z_$][A-Za-z0-9_$]*)"
)


@dataclass(frozen=True)
class Removal:
    path: str
    kind: str  # rust|python|ts
    symbol_kind: str  # fn|class|...
    name: str


def _classify(path: str) -> str | None:
    # Test files are NOT public API — a renamed/removed test function is not a
    # consumer-visible removal. Excluding any `tests/` directory keeps the
    # removal-detect gate scoped to the shipped public surface and stops false
    # positives on test churn (e.g. the Slice-25 `test_surface.py` rewrite).
    if "/tests/" in path:
        return None
    if path.startswith("src/rust/crates/") and path.endswith(".rs"):
        return "rust"
    if path.startswith("src/python/") and path.endswith(".py"):
        return "python"
    if path.startswith("src/ts/") and (path.endswith(".ts") or path.endswith(".tsx")):
        return "ts"
    return None


def _scan_line(kind: str, line: str) -> tuple[str, str] | None:
    if kind == "rust":
        m = RUST_PUBLIC.match(line)
    elif kind == "python":
        m = PY_PUBLIC.match(line)
    elif kind == "ts":
        m = TS_PUBLIC.match(line)
    else:
        return None
    return (m.group(1), m.group(2)) if m else None


def parse_diff(diff_text: str) -> tuple[set[Removal], set[tuple[str, str, str]]]:
    """Returns (removals, additions) keyed for cancellation matching."""
    removals: set[Removal] = set()
    # Additions keyed (path, kind, name) so a rename WITHIN the same file
    # at the same path cancels out a removal (true delete only when no
    # corresponding + line in the same path).
    additions: set[tuple[str, str, str]] = set()

    current_path: str | None = None
    current_kind: str | None = None
    for raw in diff_text.splitlines():
        if raw.startswith("+++ "):
            # New-path marker is more reliable than the diff header for
            # in-file renames; both old and new paths are the same in
            # normal removals.
            spec = raw[4:].strip()
            spec = spec.removeprefix("b/")
            current_path = spec if spec != "/dev/null" else current_path
            current_kind = _classify(current_path) if current_path else None
            continue
        if raw.startswith("--- "):
            spec = raw[4:].strip()
            spec = spec.removeprefix("a/")
            if spec != "/dev/null":
                current_path = spec
                current_kind = _classify(current_path) if current_path else None
            continue
        if current_kind is None or current_path is None:
            continue
        # Skip hunk headers and diff metadata.
        if raw.startswith("+++") or raw.startswith("---") or raw.startswith("@@"):
            continue
        if raw.startswith("-") and not raw.startswith("--"):
            match = _scan_line(current_kind, raw[1:])
            if match:
                symbol_kind, name = match
                removals.add(
                    Removal(
                        path=current_path,
                        kind=current_kind,
                        symbol_kind=symbol_kind,
                        name=name,
                    )
                )
        elif raw.startswith("+") and not raw.startswith("++"):
            match = _scan_line(current_kind, raw[1:])
            if match:
                additions.add((current_path, current_kind, match[1]))

    return removals, additions


def real_removals(removals: set[Removal], additions: set[tuple[str, str, str]]) -> list[Removal]:
    """A removal cancels if the same symbol name re-appears in the same
    file (move/rename-in-place); cross-file moves still count as removal
    from the public path because consumers may import by full path.
    """
    out = []
    for r in removals:
        if (r.path, r.kind, r.name) in additions:
            continue
        out.append(r)
    return sorted(out, key=lambda r: (r.path, r.name))


def changelog_documents(changelog: Path, removed: Iterable[Removal]) -> list[Removal]:
    text = changelog.read_text(encoding="utf-8")
    # Split CHANGELOG into sections per `##` heading and only consider
    # text under a heading containing "Removed" (case-insensitive). This
    # bounds the match to the announcement region.
    sections = re.split(r"^(#{1,6}\s+.*)$", text, flags=re.MULTILINE)
    removed_text_parts: list[str] = []
    # `sections` is [pre, heading1, body1, heading2, body2, ...]
    for i in range(1, len(sections), 2):
        heading = sections[i]
        body = sections[i + 1] if i + 1 < len(sections) else ""
        if re.search(r"removed", heading, flags=re.IGNORECASE):
            removed_text_parts.append(body)
    haystack = "\n".join(removed_text_parts)
    undocumented = []
    for r in removed:
        if not re.search(r"\b" + re.escape(r.name) + r"\b", haystack):
            undocumented.append(r)
    return undocumented


def load_diff(args: argparse.Namespace) -> str:
    if args.diff_file:
        return Path(args.diff_file).read_text(encoding="utf-8")
    cmd = ["git", "-C", str(args.repo_root), "diff", f"{args.base}..{args.head}", "--"]
    for prefix in SCAN_PREFIXES:
        cmd.append(f"{prefix}**")
    completed = subprocess.run(cmd, check=False, capture_output=True, text=True)
    if completed.returncode != 0:
        sys.stderr.write(completed.stderr)
        sys.exit(2)
    return completed.stdout


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--base", default="v0.6.1", help="base git ref")
    p.add_argument("--head", default="HEAD", help="head git ref")
    p.add_argument("--diff-file", default=None, help="read diff from a file instead of git")
    p.add_argument("--changelog", default=None, help="CHANGELOG.md path (default: <repo>/CHANGELOG.md)")
    p.add_argument("--repo-root", default=None, help="repository root (default: git rev-parse)")
    args = p.parse_args(argv)

    if args.repo_root is None:
        try:
            args.repo_root = subprocess.check_output(
                ["git", "rev-parse", "--show-toplevel"], text=True
            ).strip()
        except subprocess.CalledProcessError:
            sys.stderr.write("not inside a git repo; pass --repo-root\n")
            return 2

    changelog_path = Path(args.changelog) if args.changelog else Path(args.repo_root) / "CHANGELOG.md"
    if not changelog_path.exists():
        sys.stderr.write(f"CHANGELOG.md missing at {changelog_path}\n")
        return 2

    diff_text = load_diff(args)
    removals, additions = parse_diff(diff_text)
    truly_removed = real_removals(removals, additions)
    undocumented = changelog_documents(changelog_path, truly_removed)

    if undocumented:
        sys.stderr.write("AC-050c: removed public symbols missing from CHANGELOG Removed section:\n")
        for r in undocumented:
            sys.stderr.write(f"  {r.path}: {r.symbol_kind} {r.name}\n")
        return 1

    if truly_removed:
        sys.stdout.write(f"AC-050c OK: {len(truly_removed)} removals all documented.\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
