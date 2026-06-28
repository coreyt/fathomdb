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
# Skips (exit 0 + notice) only when markdownlint-cli2 is genuinely absent.
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
  echo "[lint-docs] SKIP — markdownlint-cli2 not installed (run scripts/bootstrap.sh)"
  exit 0
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
