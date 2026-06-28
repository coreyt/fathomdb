#!/usr/bin/env bash
# md-safe-fix.sh — run `markdownlint-cli2 --fix`, but GUARD every change with the
# CommonMark-AST neutrality check (dev/tools/md_neutrality_guard.py). Any file whose
# *meaning* changed (broken emphasis, word-join, snake_case `_` loss, changed code/link/
# fence) is REVERTED and reported — markdownlint --fix is known to corrupt a few fragile
# constructs (see dev/tools/md-fix-corruption-ledger.md). prettier is intentionally NOT
# used (it is the worse corruptor; removed from the gate 0.8.9.1).
#
# Usage:
#   scripts/md-safe-fix.sh [FILE.md ...]    # default: staged *.md (added/copied/modified)
# Exit: 0 all fixes neutral (or nothing to do); 1 a fix changed meaning (reverted) — resolve
#       the construct by hand per the ledger, then re-run.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

GUARD="dev/tools/md_neutrality_guard.py"
LEDGER="dev/tools/md-fix-corruption-ledger.md"

# Locate markdownlint-cli2 (repo node_modules, else the sibling main checkout).
BIN=""
for c in ./node_modules/.bin/markdownlint-cli2 \
         "$(git rev-parse --show-toplevel)/node_modules/.bin/markdownlint-cli2" \
         /home/coreyt/projects/fathomdb/node_modules/.bin/markdownlint-cli2; do
  [ -x "$c" ] && BIN="$c" && break
done
if [ -z "$BIN" ]; then
  echo "[md-safe-fix] markdownlint-cli2 not found (run scripts/bootstrap.sh) — skipping." >&2
  exit 0
fi

if [ "$#" -gt 0 ]; then
  FILES="$*"
else
  FILES="$(git diff --cached --name-only --diff-filter=ACM | grep '\.md$' || true)"
fi
[ -z "$FILES" ] && exit 0

# Snapshot pre-fix content of the target files.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
for f in $FILES; do
  [ -f "$f" ] || continue
  mkdir -p "$TMP/$(dirname "$f")"
  cp "$f" "$TMP/$f"
done

# markdownlint-cli2 ignores CLI globs when the config sets `globs`, so this is whole-repo;
# it is idempotent (a no-op on already-clean files), so in practice only dirty files change.
"$BIN" --fix >/dev/null 2>&1 || true

corrupt=0
restage=""
for f in $FILES; do
  [ -f "$f" ] || continue
  if ! cmp -s "$TMP/$f" "$f"; then
    if python3 "$GUARD" diff "$TMP/$f" "$f"; then
      restage="$restage $f"            # neutral fix — keep it
    else
      cp "$TMP/$f" "$f"                 # corruption — REVERT to pre-fix
      echo "[md-safe-fix] ^ reverted markdownlint --fix on $f (it changed meaning)." >&2
      corrupt=1
    fi
  fi
done

# Re-stage the neutral fixes if we are in a staged context (no explicit file args).
if [ "$#" -eq 0 ] && [ -n "$restage" ]; then
  # shellcheck disable=SC2086
  git add $restage
fi

if [ "$corrupt" -ne 0 ]; then
  cat >&2 <<EOF
[md-safe-fix] markdownlint --fix corrupted one or more files (reverted above).
  These are fragile constructs (e.g. a #-prefixed prose line read as a heading, a
  literal * read as emphasis, a schemeless host wrapped into a broken autolink).
  Fix the SOURCE construct by hand (escape it, backtick it, or restructure), then
  re-run. See: $LEDGER
EOF
  exit 1
fi
exit 0
