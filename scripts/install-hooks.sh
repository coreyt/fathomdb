#!/usr/bin/env bash
# Install repo-tracked git hooks by symlinking .git/hooks/* -> scripts/hooks/*.
# Idempotent + self-healing: it replaces an existing hook when that hook is one of
# OURS (a symlink, or a regular file carrying the SENTINEL below — e.g. a stale
# hand-copied older version), but refuses to clobber a genuine developer-authored
# hook (a regular file WITHOUT the sentinel) unless --force is given.
#
#   scripts/install-hooks.sh            # install / refresh tracked hooks
#   scripts/install-hooks.sh --force    # also replace a non-sentinel custom hook
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

REPO_ROOT="$(pwd)"
HOOKS_SRC="$REPO_ROOT/scripts/hooks"
HOOKS_DST="$(git rev-parse --git-path hooks)"
SENTINEL="fathomdb-tracked-hook"
FORCE=0
[ "${1:-}" = "--force" ] && FORCE=1

# If core.hooksPath points somewhere OTHER than this repo's .git/hooks, git uses THAT
# directory and ignores symlinks we'd create in .git/hooks — install into it instead, and
# warn so a dead hook is explainable.
HOOKS_PATH_CFG="$(git config --get core.hooksPath || true)"
if [ -n "$HOOKS_PATH_CFG" ]; then
  abs_cfg="$(cd "$HOOKS_PATH_CFG" 2>/dev/null && pwd || echo "$HOOKS_PATH_CFG")"
  abs_dst="$(cd "$HOOKS_DST" 2>/dev/null && pwd || echo "$HOOKS_DST")"
  if [ "$abs_cfg" != "$abs_dst" ]; then
    echo "install-hooks: note core.hooksPath=$HOOKS_PATH_CFG; installing there." >&2
    HOOKS_DST="$HOOKS_PATH_CFG"
  fi
fi

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
  elif grep -q "$SENTINEL" "$dst" 2>/dev/null; then
    # a stale hand-copied version of OUR hook — safe to replace with the live symlink
    ln -snf "$src" "$dst"
    echo "install-hooks: refreshed stale tracked hook $dst -> $src"
  elif [ "$FORCE" -eq 1 ]; then
    ln -snf "$src" "$dst"
    echo "install-hooks: --force replaced $dst -> $src"
  else
    echo "install-hooks: $dst is a non-symlink without the '$SENTINEL' marker;" >&2
    echo "install-hooks: leaving it (looks developer-authored). Re-run with --force to adopt the tracked hook." >&2
  fi
done
