#!/usr/bin/env bash
# scripts/agent-lint-docs.sh — markdownlint the PUBLIC docs (docs/**).
#
# WHY a separate script: `.markdownlint-cli2.jsonc` *ignores* `docs/**` (the comment
# there says it is "gated by mkdocs build --strict already"). But `mkdocs build
# --strict` only catches broken links / nav / bad config — it does NOT enforce
# markdownlint *style* (fence languages, blank-line framing, list indentation, ...).
# So docs/** had no structural-markdown gate at all, and `dev/update-docs.md`
# regenerates docs/** from dev/ — a regeneration could silently re-introduce debt.
#
# This lints docs/** with the SAME rule set (`.markdownlint.jsonc`) as the rest of
# the repo. markdownlint-cli2 always auto-discovers the repo-root
# `.markdownlint-cli2.jsonc` (whose `ignores` drop docs/**), so we cannot lint docs/
# in place; instead we copy docs/ into a scratch dir OUTSIDE the repo (no cli2 config
# discoverable there) and lint with `--config` pointing at the rule set.
#
# Pass: silent (or one-line ok). Fail: the markdownlint findings + nonzero exit.
#
# TC-37 follow-up: this used to skip (exit 0) when markdownlint-cli2 was genuinely
# absent from all three resolution tiers below. Invoked unconditionally from
# scripts/agent-lint-md.sh (the same script whose OWN direct absent-binary check
# vacuously passed for three weeks), a residual silent-skip here is the same hazard
# class, so it is now a hard failure too.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

CFG="$REPO_ROOT/.markdownlint.jsonc"

# Locate markdownlint-cli2: repo node_modules, then sibling main checkout
# (worktree case), then PATH. Mirrors scripts/md-safe-fix.sh.
BIN=""
for c in "$REPO_ROOT/node_modules/.bin/markdownlint-cli2" \
         "/home/coreyt/projects/fathomdb/node_modules/.bin/markdownlint-cli2" \
         "$(command -v markdownlint-cli2 2>/dev/null || true)"; do
  [ -n "$c" ] && [ -x "$c" ] && BIN="$c" && break
done
if [ -z "$BIN" ]; then
  {
    echo "[lint-docs] FAIL — markdownlint-cli2 not found (checked repo node_modules, the"
    echo "  primary checkout's node_modules, and PATH)."
    echo "  A missing structural markdown linter must never report a silent pass (TC-37)."
    echo "  Fix: run scripts/bootstrap.sh to install it, OR (inside a linked worktree)"
    echo "  symlink the primary checkout's node_modules:"
    echo "    ln -s /home/coreyt/projects/fathomdb/node_modules node_modules"
  } >&2
  exit 1
fi

if [ ! -d "$REPO_ROOT/docs" ]; then
  echo "[lint-docs] no docs/ directory — nothing to lint."
  exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cp -r "$REPO_ROOT/docs" "$WORK/docs"

if ( cd "$WORK" && "$BIN" --config "$CFG" 'docs/**/*.md' ) >"$WORK/out.txt" 2>&1; then
  if [ "${AGENT_VERBOSE:-0}" = "1" ]; then echo "[lint-docs] ok (docs/** clean)"; fi
  exit 0
fi

echo "[lint-docs] markdownlint flagged docs/** (gated only by mkdocs --strict otherwise):" >&2
# Rewrite the scratch path back to the real repo path in the diagnostic.
sed "s#$WORK/##g" "$WORK/out.txt" | grep -E 'MD[0-9]|error|Summary' >&2 || cat "$WORK/out.txt" >&2
exit 1
