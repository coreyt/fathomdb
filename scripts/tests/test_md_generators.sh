#!/usr/bin/env bash
# scripts/tests/test_md_generators.sh — anti-regression guard for the two SHELL
# markdown generators under scripts/repo-prune/bin/. Companion to the Python guard
# src/python/tests/test_md_generator_hygiene.py (which covers aggregate.py /
# m1_verdict_run.py / s15a_embedder_probe.py).
#
# WHY a dedicated guard: both scripts write their human-summary `.md` into
# scripts/repo-prune/measurements/**, and the m1/s15a/aggregate reports write into
# dev/plans/runs/** — BOTH trees are in `.markdownlint-cli2.jsonc`'s `ignores`, so the
# normal `scripts/agent-lint-md.sh` gate never lints a regenerated report. This guard
# runs each generator for real and lints its emitted markdown with the repo rule set
# (`.markdownlint.jsonc`), so a generator that re-introduces markdown debt fails CI.
#
# Both generators are read-only over their inputs; we point memory-clarity at a tiny
# synthetic memory dir (CLAUDE_MEMORY_DIR) and run context-clarity over the live repo,
# both with a throwaway LABEL, and clean up the measurement files afterwards.
#
# Skips (exit 0 + notice) only when markdownlint-cli2 is genuinely absent (local dev
# without scripts/bootstrap.sh). CI installs node deps, so the gate is real there.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

CFG="$REPO_ROOT/.markdownlint.jsonc"

# Locate markdownlint-cli2: repo node_modules, then the sibling main checkout
# (worktree case), then PATH. Mirrors scripts/md-safe-fix.sh.
BIN=""
for c in "$REPO_ROOT/node_modules/.bin/markdownlint-cli2" \
         "/home/coreyt/projects/fathomdb/node_modules/.bin/markdownlint-cli2" \
         "$(command -v markdownlint-cli2 2>/dev/null || true)"; do
  [ -n "$c" ] && [ -x "$c" ] && BIN="$c" && break
done
if [ -z "$BIN" ]; then
  echo "SKIP  test-md-generators (markdownlint-cli2 not installed; run scripts/bootstrap.sh)"
  exit 0
fi

WORK="$(mktemp -d)"
LABEL="__md_guard_$$"
cleanup() {
  rm -rf "$WORK"
  rm -f "scripts/repo-prune/measurements/context-clarity/${LABEL}.md" \
        "scripts/repo-prune/measurements/context-clarity/${LABEL}.json" \
        "scripts/repo-prune/measurements/memory-clarity/${LABEL}.md" \
        "scripts/repo-prune/measurements/memory-clarity/${LABEL}.json"
}
trap cleanup EXIT

FAILED=0

# Lint one file by copying it OUT of the repo tree first: inside the repo,
# markdownlint-cli2 auto-discovers .markdownlint-cli2.jsonc (whose globs/ignores would
# re-scope the run / drop the file); from $WORK it does not, so --config is authoritative.
lint_md() {
  local label="$1" src="$2"
  if [ ! -f "$src" ]; then
    printf 'FAIL  %s (generator produced no output at %s)\n' "$label" "$src" >&2
    FAILED=$((FAILED + 1))
    return
  fi
  local dst="$WORK/$(basename "$src")"
  cp "$src" "$dst"
  if ( cd "$WORK" && "$BIN" --config "$CFG" "$(basename "$dst")" ) >"$WORK/out.txt" 2>&1; then
    printf 'PASS  %s\n' "$label"
  else
    printf 'FAIL  %s — markdownlint flagged generator output:\n' "$label" >&2
    grep -E 'MD[0-9]' "$WORK/out.txt" >&2 || cat "$WORK/out.txt" >&2
    FAILED=$((FAILED + 1))
  fi
}

# --- context-clarity.sh : read-only over the live repo --------------------
bash scripts/repo-prune/bin/context-clarity.sh "$LABEL" >/dev/null 2>&1 || true
lint_md test-context-clarity-md "scripts/repo-prune/measurements/context-clarity/${LABEL}.md"

# --- memory-clarity.sh : run against a tiny synthetic memory dir ----------
MEM="$WORK/memory"
mkdir -p "$MEM"
cat > "$MEM/MEMORY.md" <<'EOF'
# Memory index

- [Sample note](sample-note.md) — a one-line index entry for the guard fixture.
EOF
cat > "$MEM/sample-note.md" <<'EOF'
---
name: sample-note
description: guard fixture
type: project
---

# Sample note

Body text for the synthetic memory fixture.
EOF
CLAUDE_MEMORY_DIR="$MEM" bash scripts/repo-prune/bin/memory-clarity.sh "$LABEL" >/dev/null 2>&1 || true
lint_md test-memory-clarity-md "scripts/repo-prune/measurements/memory-clarity/${LABEL}.md"

if [ "$FAILED" -ne 0 ]; then
  printf '\n%d markdown-generator check(s) FAILED.\n' "$FAILED" >&2
  exit 1
fi
echo "ok test-md-generators (2 shell generators emit gate-compliant markdown)"
