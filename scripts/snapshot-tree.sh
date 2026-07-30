#!/usr/bin/env bash
# scripts/snapshot-tree.sh — content fingerprint of a checkout, for proving a
# tree was NOT touched across some window.
#
# WHY: `.claude/hooks/sealed-worktree-guard.sh` PREVENTS a sealed agent from
# naming the primary checkout, but prevention there is heuristic at the edges —
# it reads the command STRING, so a path assembled at runtime, read from a file,
# or produced by a subshell is invisible to it. This script is the DETECTION
# half, and it is the half the acceptance criterion is written against: it is
# independent of the agent, runs in the Steward's own session before and after,
# and cannot be reasoned around by anything the agent does.
#
# WHAT IT FINGERPRINTS, and why each part is needed:
#   * HEAD + branch          — a commit, reset or checkout in the tree
#   * `git status --porcelain` — staged/unstaged/untracked changes
#   * hash of every TRACKED file's content — an edit that was then `git add`ed
#     and committed would move HEAD, but an edit reverted before we look would
#     not; hashing content catches modification-in-flight that porcelain misses
#     once restored, and catches content drift with an unchanged mtime
#   * the sorted list of UNTRACKED paths — a stray transcript swept in by
#     `git add -A` (TC-132) starts life untracked
#   * key config — `core.bare` specifically, because TC-128's `git init` with an
#     unscrubbed GIT_DIR set `core.bare = true` on the PRIMARY twice
#
# DELIBERATELY EXCLUDED: mtimes (a read can update atime/mtime on some setups
# and would produce false alarms), and `.git/index` itself (it is rewritten by
# ordinary read-only commands such as `git status`, so including it would make
# every snapshot differ and the check would be worse than useless — a guard that
# always fires is a guard nobody reads).
#
# USAGE
#   scripts/snapshot-tree.sh <tree> [out-file]     # write a fingerprint
#   scripts/snapshot-tree.sh --compare <a> <b>     # diff two fingerprints
#
# Exit: 0 clean / 1 differs or bad usage. Capture rc=$? on the VERY NEXT LINE
# (seq-108/seq-109 — a `$(basename …)` in between reports basename's status).
set -uo pipefail

usage() {
  cat >&2 <<'EOF'
usage:
  scripts/snapshot-tree.sh <tree> [out-file]   fingerprint <tree> (stdout if no out-file)
  scripts/snapshot-tree.sh --compare <a> <b>   compare two fingerprints; exit 1 if they differ
EOF
}

if [ "${1:-}" = "--compare" ]; then
  A="${2:-}"; B="${3:-}"
  if [ -z "$A" ] || [ -z "$B" ]; then usage; exit 1; fi
  if [ ! -f "$A" ] || [ ! -f "$B" ]; then
    echo "FAIL snapshot-tree: missing fingerprint ($A or $B)" >&2; exit 1
  fi
  if diff -u "$A" "$B" >/tmp/.snapshot-diff.$$ 2>&1; then
    rm -f /tmp/.snapshot-diff.$$
    echo "ok    snapshot-tree: fingerprints identical — the tree was NOT modified"
    exit 0
  fi
  echo "FAIL  snapshot-tree: THE TREE CHANGED between the two fingerprints." >&2
  echo "      This is a hard failure: a sealed agent must not modify it." >&2
  sed -n '1,60p' /tmp/.snapshot-diff.$$ >&2
  rm -f /tmp/.snapshot-diff.$$
  exit 1
fi

TREE="${1:-}"
OUT="${2:-}"
if [ -z "$TREE" ] || [ ! -d "$TREE" ]; then usage; exit 1; fi
if ! git -C "$TREE" rev-parse --git-dir >/dev/null 2>&1; then
  echo "FAIL snapshot-tree: $TREE is not a git checkout" >&2; exit 1
fi

emit() {
  echo "# snapshot-tree fingerprint"
  echo "tree=$(cd "$TREE" && pwd -P)"
  echo "head=$(git -C "$TREE" rev-parse HEAD 2>/dev/null || echo UNKNOWN)"
  echo "branch=$(git -C "$TREE" rev-parse --abbrev-ref HEAD 2>/dev/null || echo UNKNOWN)"
  echo "core.bare=$(git -C "$TREE" config --get core.bare 2>/dev/null || echo unset)"
  echo "--- porcelain ---"
  git -C "$TREE" status --porcelain 2>/dev/null | LC_ALL=C sort
  echo "--- untracked ---"
  git -C "$TREE" ls-files --others --exclude-standard 2>/dev/null | LC_ALL=C sort
  echo "--- tracked content ---"
  # `git ls-files -s` prints the INDEX blob sha per tracked path. That is the
  # committed/staged content, so pair it with the porcelain section above: a
  # working-tree edit that is not staged shows up there, and a staged one moves
  # the blob sha here. Together they cover both.
  git -C "$TREE" ls-files -s 2>/dev/null | LC_ALL=C sort
}

if [ -n "$OUT" ]; then
  emit > "$OUT"
  echo "ok    snapshot-tree: fingerprint of $TREE written to $OUT"
else
  emit
fi
