#!/usr/bin/env python3
"""AC-050a AST no-shim scanner.

Scans Rust, Python, and TypeScript source trees for banned shim
patterns that would let 0.5.x compatibility code linger past the
0.6.0 rewrite cutover. Banned patterns:

  * Module / file names matching ``legacy_*`` or ``compat_v0_5*``.
  * Symbol declarations (fn/class/struct/etc.) with those prefixes.
  * Crate-root ``#![allow(deprecated)]`` (Rust ``lib.rs`` / ``main.rs``).
  * Re-route stubs from a configured set of 0.5.x verb names
    (empty by default — no-op until the verb list lands).

The Python surface is parsed with the stdlib ``ast`` module. Rust
and TypeScript surfaces are tokenized with grammar-scoped regex
(line-anchored, comment-aware where it matters). Full ``syn`` /
``ts-morph`` integration is filed as a follow-up; the patterns this
scanner hunts are textual and bounded.
"""

from __future__ import annotations

import argparse
import ast
import re
import sys
from dataclasses import dataclass
from pathlib import Path


# 0.5.x verb names whose re-route stubs are banned at the public API
# surface. Empty until the canonical 0.5.x verb list is enumerated —
# the scanner is still wired so that adding a name here begins to
# enforce immediately.
V05_VERBS: tuple[str, ...] = ()

FORBIDDEN_PREFIX = re.compile(r"^(legacy_|compat_v0_5)")

PATH_EXCLUDES = (
    "node_modules",
    "target",
    ".venv",
    "dist",
    "build",
    "__pycache__",
)


@dataclass(frozen=True)
class Finding:
    path: Path
    line: int
    rule: str
    detail: str

    def render(self, root: Path) -> str:
        rel = self.path.relative_to(root) if self.path.is_absolute() else self.path
        loc = f"{rel}:{self.line}" if self.line else str(rel)
        return f"{loc}: [{self.rule}] {self.detail}"


def excluded(path: Path, scan_root: Path) -> bool:
    try:
        rel = path.relative_to(scan_root)
    except ValueError:
        rel = path
    parts = set(rel.parts)
    return any(token in parts for token in PATH_EXCLUDES)


def _check_name(path: Path, line: int, name: str, rule: str) -> list[Finding]:
    if FORBIDDEN_PREFIX.match(name):
        return [Finding(path, line, rule, f"forbidden name: {name}")]
    return []


def scan_rust(root: Path) -> list[Finding]:
    findings: list[Finding] = []
    for path in sorted(root.rglob("*.rs")):
        if excluded(path, root):
            continue
        findings.extend(_check_name(path, 0, path.stem, "rust-module-name"))
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        lines = text.splitlines()
        is_crate_root = path.name in ("lib.rs", "main.rs")
        for i, raw in enumerate(lines, 1):
            stripped = raw.strip()
            if stripped.startswith("//"):
                continue
            # crate-root inner attribute that suppresses deprecation
            if is_crate_root and stripped.startswith("#![allow(") and "deprecated" in stripped:
                findings.append(
                    Finding(
                        path,
                        i,
                        "rust-crate-root-allow-deprecated",
                        "crate-root #![allow(deprecated)] is banned",
                    )
                )
            # mod legacy_foo; / pub mod compat_v0_5_admin;
            m = re.match(r"(?:pub\s+(?:\([^)]+\)\s+)?)?mod\s+([A-Za-z_][A-Za-z0-9_]*)", stripped)
            if m:
                findings.extend(_check_name(path, i, m.group(1), "rust-mod-decl"))
            # pub fn / pub struct / pub enum / pub trait names
            m = re.match(
                r"pub(?:\([^)]+\))?\s+(?:unsafe\s+|async\s+|const\s+)*"
                r"(?:fn|struct|enum|trait|type|const|static)\s+([A-Za-z_][A-Za-z0-9_]*)",
                stripped,
            )
            if m:
                findings.extend(_check_name(path, i, m.group(1), "rust-public-symbol"))
            for verb in V05_VERBS:
                if re.search(rf"\b(?:fn|pub\s+fn)\s+{re.escape(verb)}\b", stripped):
                    findings.append(
                        Finding(
                            path,
                            i,
                            "rust-05x-verb-reroute",
                            f"0.5.x verb re-route stub: {verb}",
                        )
                    )
    return findings


def scan_python(root: Path) -> list[Finding]:
    findings: list[Finding] = []
    for path in sorted(root.rglob("*.py")):
        if excluded(path, root):
            continue
        findings.extend(_check_name(path, 0, path.stem, "python-module-name"))
        try:
            tree = ast.parse(path.read_text(encoding="utf-8", errors="replace"))
        except (SyntaxError, OSError):
            continue
        for node in ast.walk(tree):
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                findings.extend(
                    _check_name(path, node.lineno, node.name, "python-symbol")
                )
                if node.name in V05_VERBS:
                    findings.append(
                        Finding(
                            path,
                            node.lineno,
                            "python-05x-verb-reroute",
                            f"0.5.x verb re-route stub: {node.name}",
                        )
                    )
    return findings


def scan_ts(root: Path) -> list[Finding]:
    findings: list[Finding] = []
    decl = re.compile(
        r"(?:export\s+(?:default\s+)?(?:async\s+)?)?"
        r"(?:class|function|interface|const|let|var|type|enum|namespace|module)\s+"
        r"([A-Za-z_$][A-Za-z0-9_$]*)"
    )
    for path in sorted(list(root.rglob("*.ts")) + list(root.rglob("*.tsx"))):
        if excluded(path, root):
            continue
        findings.extend(_check_name(path, 0, path.stem, "ts-module-name"))
        try:
            lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError:
            continue
        in_block_comment = False
        for i, raw in enumerate(lines, 1):
            stripped = raw.strip()
            # Strip simple block-comment regions to avoid matching banned
            # names that only appear in JSDoc examples.
            if in_block_comment:
                if "*/" in stripped:
                    in_block_comment = False
                continue
            if stripped.startswith("/*"):
                if "*/" not in stripped[2:]:
                    in_block_comment = True
                continue
            if stripped.startswith("//"):
                continue
            m = decl.search(stripped)
            if m:
                findings.extend(_check_name(path, i, m.group(1), "ts-symbol"))
                if m.group(1) in V05_VERBS:
                    findings.append(
                        Finding(
                            path,
                            i,
                            "ts-05x-verb-reroute",
                            f"0.5.x verb re-route stub: {m.group(1)}",
                        )
                    )
    return findings


SCANNERS = {
    "rust": scan_rust,
    "python": scan_python,
    "ts": scan_ts,
}


def repo_root() -> Path:
    try:
        import subprocess

        out = subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True)
        return Path(out.strip())
    except Exception:
        return Path.cwd()


DEFAULT_PATHS = {
    "rust": "src/rust/crates",
    "python": "src/python",
    "ts": "src/ts",
}


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--language", choices=sorted(SCANNERS), required=True)
    p.add_argument(
        "--path",
        default=None,
        help="override scan root (default: <repo>/src/<lang>...)",
    )
    p.add_argument("--repo-root", default=None)
    args = p.parse_args(argv)

    root_dir = Path(args.repo_root) if args.repo_root else repo_root()
    scan_target = Path(args.path) if args.path else root_dir / DEFAULT_PATHS[args.language]
    if not scan_target.exists():
        sys.stderr.write(f"scan target missing: {scan_target}\n")
        return 2

    findings = SCANNERS[args.language](scan_target)
    if findings:
        for f in findings:
            sys.stderr.write(f.render(root_dir) + "\n")
        sys.stderr.write(
            f"AC-050a: {len(findings)} {args.language} shim violation(s)\n"
        )
        return 1
    sys.stdout.write(f"AC-050a OK: {args.language} surface clean ({scan_target})\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
