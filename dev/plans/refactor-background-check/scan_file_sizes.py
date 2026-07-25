#!/usr/bin/env python3
"""Enumerate tracked files, classify them, measure LOC, and flag heuristic violations.

Deterministic and reproducible: no model in the loop. Emits
  violations.json  - machine-readable, feeds the Phase-3 review fan-out
  violations.md    - human-readable, ranked by severity
  structure/<slug>.txt - per-violator structural outline (top-level items + line
                     ranges), so a reviewing agent starts with the map instead of
                     spending its budget rediscovering it.

Thresholds come from thresholds.json; this script never invents one.

Usage:
    python3 scan_file_sizes.py [--repo-root PATH] [--out-dir PATH]
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass, asdict
from pathlib import Path

# --------------------------------------------------------------------------
# Classification. Order matters: first match wins.
# --------------------------------------------------------------------------

CLASS_RULES: list[tuple[str, str]] = [
    # agent / context files
    (r"^\.claude/agents/.*\.md$", "agent-file"),
    (r"^\.claude/commands/.*\.md$", "agent-file"),
    (r"^\.claude/skills/.*\.md$", "agent-file"),
    (r"^(.*/)?CLAUDE\.md$", "agent-file"),
    (r"^(.*/)?AGENTS\.md$", "agent-file"),
    # rust
    (r"^src/rust/crates/[^/]+/tests/.*\.rs$", "rust-test"),
    (r"^src/rust/crates/[^/]+/benches/.*\.rs$", "rust-test"),
    (r"^src/rust/.*\.rs$", "rust-src"),
    # python
    (r"^.*\.pyi$", "py-stub"),
    (r"^src/python/tests/.*\.py$", "py-test"),
    (r"^src/python/eval/.*\.py$", "py-eval"),
    (r"^(.*/)?test_[^/]*\.py$", "py-test"),
    (r"^(.*/)?conftest\.py$", "py-test"),
    (r"^.*\.py$", "py-src"),
    # CI definitions: authored control flow, not config data
    (r"^\.github/workflows/.*\.ya?ml$", "ci-workflow"),
    # typescript
    (r"^src/ts/tests/.*\.ts$", "ts-test"),
    (r"^.*\.test\.ts$", "ts-test"),
    (r"^src/ts/.*\.ts$", "ts-src"),
    (r"^.*\.ts$", "ts-src"),
    # shell
    (r"^scripts/tests/.*\.sh$", "shell-test"),
    (r"^(.*/)?test_[^/]*\.sh$", "shell-test"),
    (r"^.*\.sh$", "shell"),
    # docs: append-only / historical records vs. designed reference material
    (r"^dev/plans/.*\.md$", "plan-status-doc"),
    (r"^dev/progress/.*\.md$", "plan-status-doc"),
    (r"^dev/archive/.*\.md$", "plan-status-doc"),
    (r"^(.*/)?STATUS[^/]*\.md$", "plan-status-doc"),
    (r"^(.*/)?CHANGELOG\.md$", "plan-status-doc"),
    (r"^dev/design/.*\.md$", "design-doc"),
    (r"^dev/adr/.*\.md$", "design-doc"),
    (r"^dev/notes/.*\.md$", "design-doc"),
    (r"^dev/research/.*\.md$", "design-doc"),
    (r"^.*\.md$", "doc-other"),
]

# Files that are long by nature and must never count as violators.
# Each entry is (pattern, reason) - the reason is recorded, not silently dropped.
EXCLUSIONS: list[tuple[str, str]] = [
    (r"^.*\.log$", "log artifact: append-only machine output"),
    (r"^.*\.jsonl$", "jsonl ledger/fixture: one record per line, length is the point"),
    (r"^.*\.json$", "data/config, not authored prose or logic"),
    (r"^.*\.lock$", "generated lockfile"),
    (r"^.*\.patch$", "generated diff"),
    (r"^.*/fixtures/.*$", "test fixture data"),
    (r"^.*/golden[^/]*$", "golden expectation data"),
    (r"^.*/out/.*$", "generated experiment output"),
    (r"^.*\.seq$", "counter sidecar"),
    (r"^.*\.stderr$", "captured output artifact"),
    (r"^.*\.txt$", "plain data/output, not authored source or prose"),
]

# --------------------------------------------------------------------------
# Structure extraction: top-level item starts per language.
# --------------------------------------------------------------------------

STRUCTURE_PATTERNS: dict[str, list[str]] = {
    "rust": [
        r"^(pub(\([^)]*\))?\s+)?(async\s+)?(unsafe\s+)?fn\s+\w+",
        r"^(pub(\([^)]*\))?\s+)?struct\s+\w+",
        r"^(pub(\([^)]*\))?\s+)?enum\s+\w+",
        r"^(pub(\([^)]*\))?\s+)?trait\s+\w+",
        r"^(pub(\([^)]*\))?\s+)?mod\s+\w+",
        r"^impl(\s|<)",
        r"^#\[(test|tokio::test|rstest)\]",
        r"^macro_rules!\s+\w+",
    ],
    "python": [r"^(async\s+)?def\s+\w+", r"^class\s+\w+"],
    "ts": [
        r"^(export\s+)?(default\s+)?(async\s+)?function\s+\w+",
        r"^(export\s+)?(abstract\s+)?class\s+\w+",
        r"^(export\s+)?(const|let|var)\s+\w+",
        r"^(export\s+)?(interface|type)\s+\w+",
        r"^(describe|test|it)\s*\(",
    ],
    "shell": [r"^(function\s+)?\w+\s*\(\)\s*\{"],
    "md": [r"^#{1,6}\s+\S"],
    # top-level keys and one-level-in job names carry a CI file's structure
    "yaml": [r"^[A-Za-z_][\w-]*:", r"^  [A-Za-z_][\w-]*:"],
}

LANG_FOR_CLASS = {
    "rust-src": "rust", "rust-test": "rust",
    "py-src": "python", "py-eval": "python", "py-test": "python",
    "py-stub": "python", "ci-workflow": "yaml",
    "ts-src": "ts", "ts-test": "ts",
    "shell": "shell", "shell-test": "shell",
    "design-doc": "md", "plan-status-doc": "md", "doc-other": "md",
    "agent-file": "md",
}


@dataclass
class FileRecord:
    path: str
    file_class: str
    loc: int
    soft: int
    hard: int
    severity: float          # loc / hard; >= 1.0 means hard violation
    band: str                # "ok" | "soft" | "hard"


def first_match(path: str, rules: list[tuple[str, str]]) -> str | None:
    for pattern, label in rules:
        if re.match(pattern, path):
            return label
    return None


def tracked_files(repo_root: Path) -> list[str]:
    out = subprocess.run(
        ["git", "ls-files"], cwd=repo_root, capture_output=True, text=True, check=True
    )
    return [line for line in out.stdout.splitlines() if line]


def count_lines(p: Path) -> int:
    try:
        with p.open("rb") as fh:
            return sum(1 for _ in fh)
    except (OSError, ValueError):
        return 0


def extract_structure(p: Path, lang: str, max_items: int = 400) -> list[tuple[int, str]]:
    """Return [(line_no, first-80-chars-of-line)] for each top-level item start."""
    patterns = [re.compile(pat) for pat in STRUCTURE_PATTERNS.get(lang, [])]
    if not patterns:
        return []
    items: list[tuple[int, str]] = []
    try:
        text = p.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return []
    for i, line in enumerate(text.splitlines(), start=1):
        if any(pat.match(line) for pat in patterns):
            items.append((i, line.rstrip()[:80]))
            if len(items) >= max_items:
                items.append((i, "... [structure map truncated]"))
                break
    return items


def render_structure_map(p: Path, lang: str, total_loc: int) -> str:
    items = extract_structure(p, lang)
    if not items:
        return "(no top-level items detected for this language)\n"
    lines = []
    for idx, (start, text) in enumerate(items):
        end = items[idx + 1][0] - 1 if idx + 1 < len(items) else total_loc
        span = max(end - start + 1, 0)
        lines.append(f"L{start:>6}-{end:<6} ({span:>5}) | {text}")
    return "\n".join(lines) + "\n"


def slugify(path: str) -> str:
    return re.sub(r"[^A-Za-z0-9]+", "_", path).strip("_")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo-root", default=".", type=Path)
    ap.add_argument("--out-dir", default="dev/plans/refactor-background-check", type=Path)
    args = ap.parse_args()

    repo_root = args.repo_root.resolve()
    out_dir = (repo_root / args.out_dir).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    thresholds_path = out_dir / "thresholds.json"
    if not thresholds_path.exists():
        print(f"error: {thresholds_path} not found - Phase 1 must write it first", file=sys.stderr)
        return 2
    thresholds = json.loads(thresholds_path.read_text())

    records: list[FileRecord] = []
    excluded: list[dict[str, str]] = []
    unclassified: list[str] = []

    for rel in tracked_files(repo_root):
        reason = first_match(rel, EXCLUSIONS)
        if reason:
            excluded.append({"path": rel, "reason": reason})
            continue
        file_class = first_match(rel, CLASS_RULES)
        if file_class is None:
            unclassified.append(rel)
            continue
        limits = thresholds.get(file_class)
        if limits is None:
            unclassified.append(rel)
            continue

        loc = count_lines(repo_root / rel)
        soft, hard = int(limits["soft_loc"]), int(limits["hard_loc"])
        if loc >= hard:
            band = "hard"
        elif loc >= soft:
            band = "soft"
        else:
            band = "ok"
        records.append(FileRecord(rel, file_class, loc, soft, hard, round(loc / hard, 3), band))

    violators = sorted(
        [r for r in records if r.band != "ok"], key=lambda r: r.severity, reverse=True
    )

    # Structure maps, for violators only.
    struct_dir = out_dir / "structure"
    struct_dir.mkdir(exist_ok=True)
    for r in violators:
        lang = LANG_FOR_CLASS.get(r.file_class, "")
        body = render_structure_map(repo_root / r.path, lang, r.loc)
        (struct_dir / f"{slugify(r.path)}.txt").write_text(
            f"# {r.path}  ({r.loc} LOC, class={r.file_class}, "
            f"soft={r.soft}, hard={r.hard})\n\n{body}"
        )

    payload = {
        "scanned": len(records),
        "excluded": excluded,
        "unclassified": unclassified,
        "thresholds": thresholds,
        "violators": [asdict(r) for r in violators],
    }
    (out_dir / "violations.json").write_text(json.dumps(payload, indent=2) + "\n")

    # Human-readable report.
    by_class: dict[str, list[FileRecord]] = {}
    for r in violators:
        by_class.setdefault(r.file_class, []).append(r)

    md = ["# File-size heuristic violations", ""]
    md.append(f"Scanned {len(records)} classified files. "
              f"{len(violators)} violate a threshold "
              f"({sum(1 for r in violators if r.band == 'hard')} hard, "
              f"{sum(1 for r in violators if r.band == 'soft')} soft).")
    md.append(f"{len(excluded)} files excluded by rule; {len(unclassified)} unclassified.")
    md.append("")
    for file_class in sorted(by_class, key=lambda c: -len(by_class[c])):
        rows = by_class[file_class]
        soft, hard = rows[0].soft, rows[0].hard
        md.append(f"## {file_class} — soft {soft} / hard {hard} ({len(rows)} violators)")
        md.append("")
        md.append("| LOC | severity | band | file |")
        md.append("|---:|---:|:--|:--|")
        for r in rows:
            md.append(f"| {r.loc} | {r.severity:.2f} | {r.band} | `{r.path}` |")
        md.append("")
    if unclassified:
        md.append("## Unclassified (no rule matched — review the ruleset)")
        md.append("")
        for u in unclassified[:50]:
            md.append(f"- `{u}`")
        if len(unclassified) > 50:
            md.append(f"- ... and {len(unclassified) - 50} more")
        md.append("")
    (out_dir / "violations.md").write_text("\n".join(md) + "\n")

    print(f"scanned={len(records)} violators={len(violators)} "
          f"hard={sum(1 for r in violators if r.band == 'hard')} "
          f"excluded={len(excluded)} unclassified={len(unclassified)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
