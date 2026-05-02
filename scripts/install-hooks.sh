#!/usr/bin/env bash
# Install repo-tracked git hooks by symlinking .git/hooks/* -> scripts/hooks/*.
# Idempotent: re-running replaces existing symlinks but refuses to clobber
# real files (so a developer's hand-rolled hook is preserved unless removed).
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

REPO_ROOT="$(pwd)"
HOOKS_SRC="$REPO_ROOT/scripts/hooks"
HOOKS_DST="$(git rev-parse --git-path hooks)"

mkdir -p "$HOOKS_DST"

for hook in pre-commit pre-push; do
  src="$HOOKS_SRC/$hook"
  dst="$HOOKS_DST/$hook"

  if [ ! -f "$src" ]; then
    echo "install-hooks: missing $src" >&2
    exit 1
  fi

  chmod +x "$src"

  # Absolute symlink target — works for worktrees where .git is a file.
  if [ -L "$dst" ] || [ ! -e "$dst" ]; then
    ln -snf "$src" "$dst"
    echo "install-hooks: linked $dst -> $src"
  else
    echo "install-hooks: $dst exists and is not a symlink; leaving it alone" >&2
    echo "install-hooks: remove it manually if you want the tracked hook" >&2
  fi
done
