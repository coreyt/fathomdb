#!/usr/bin/env python3
"""Token-frugal markdown-gate summarizer for 0.8.9.1.

Runs markdownlint-cli2 + prettier --check (binaries from $MD_BIN, default the
main-checkout node_modules), captures FULL output to files under dev/plans/runs/,
and prints ONLY aggregates: {rule: count}, {top-dir: count}, totals. Never echoes
the thousands of raw lines — read the spill files on demand when fixing a rule.

Usage: MD_BIN=/path/to/node_modules/.bin python3 dev/plans/runs/md_lint_summary.py [tag]
Run with the worktree as CWD (globs + config resolve from CWD).
"""
import collections
import os
import pathlib
import re
import subprocess
import sys

BIN = os.environ.get("MD_BIN", "/home/coreyt/projects/fathomdb/node_modules/.bin")
TAG = sys.argv[1] if len(sys.argv) > 1 else "snapshot"
OUT = pathlib.Path("dev/plans/runs")
OUT.mkdir(parents=True, exist_ok=True)
ml_raw = OUT / f"0.8.9.1-mdlint-{TAG}.txt"
pr_raw = OUT / f"0.8.9.1-prettier-{TAG}.txt"

def topdir(path):
    p = path.split(":", 1)[0]
    parts = pathlib.PurePath(p).parts
    return "/".join(parts[:2]) if len(parts) > 1 else (parts[0] if parts else ".")

# --- markdownlint-cli2 (findings go to stderr) ---
ml = subprocess.run([f"{BIN}/markdownlint-cli2"], capture_output=True, text=True)
ml_text = ml.stdout + ml.stderr
ml_raw.write_text(ml_text)
rule_ct, mdir_ct, mtotal = collections.Counter(), collections.Counter(), 0
line_re = re.compile(r"^(\S+?):\d+(?::\d+)? (?:error|warning) (MD\d+)/")
for ln in ml_text.splitlines():
    m = line_re.match(ln)
    if m:
        mtotal_inc = True
        rule_ct[m.group(2)] += 1
        mdir_ct[topdir(m.group(1))] += 1
        mtotal = sum(rule_ct.values())
mtotal = sum(rule_ct.values())

# --- prettier --check (lists would-reformat files on stderr) ---
pr = subprocess.run([f"{BIN}/prettier", "--check", "**/*.md", "--log-level", "warn"],
                    capture_output=True, text=True)
pr_text = pr.stdout + pr.stderr
pr_raw.write_text(pr_text)
pdir_ct, pfiles = collections.Counter(), 0
for ln in pr_text.splitlines():
    s = ln.strip()
    if s.startswith("[warn]") and s.endswith(".md"):
        f = s[len("[warn]"):].strip()
        pdir_ct[topdir(f)] += 1
        pfiles += 1

print(f"== md-gate summary [{TAG}] ==  (full spill: {ml_raw} / {pr_raw})")
print(f"markdownlint: {mtotal} findings  (exit {ml.returncode})")
for r, c in rule_ct.most_common(15):
    print(f"  {r:8} {c}")
print("  by-dir:", dict(mdir_ct.most_common(10)))
print(f"prettier: {pfiles} files would reformat  (exit {pr.returncode})")
print("  by-dir:", dict(pdir_ct.most_common(10)))
