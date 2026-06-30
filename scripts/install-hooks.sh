#!/usr/bin/env bash
# Activate the repo-tracked git hooks (scripts/hooks/*) for THIS clone by
# pointing git's core.hooksPath at them. Idempotent.
#
#   scripts/install-hooks.sh    # activate / refresh tracked-hook activation
#
# WHY core.hooksPath instead of symlinking into .git/hooks:
#   * Zero "remember to run it": once bootstrap.sh sets this, every actor in
#     this clone runs the tracked pre-commit (md auto-fix/enforce, fmt, ruff)
#     and pre-push automatically — there is no per-file adoption decision that
#     could silently leave a stale legacy hook in place (the failure mode that
#     left the markdown guard dormant in every checkout).
#   * Worktrees: core.hooksPath lives in the SHARED common config (.git/config),
#     so linked worktrees inherit it; the value is REPO-RELATIVE (scripts/hooks),
#     which git resolves against each worktree's own root — so a linked worktree
#     activates its own tracked hooks instead of silently having none.
#
# Velocity note: the tracked pre-push is FAST by default (clippy + actionlint);
# the heavy full-verify gate is opt-in via FATHOMDB_PREPUSH_FULL=1. So enabling
# core.hooksPath does NOT make every push slow. See scripts/hooks/pre-push.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

DESIRED="scripts/hooks"

# Keep the tracked hooks executable (a fresh clone can land them mode 0644 on
# some filesystems / archive extractions).
chmod +x scripts/hooks/pre-commit scripts/hooks/pre-push 2>/dev/null || true

CURRENT="$(git config --get core.hooksPath || true)"
if [ "$CURRENT" = "$DESIRED" ]; then
  echo "install-hooks: core.hooksPath already '$DESIRED' (tracked hooks active)."
  exit 0
fi

if [ -n "$CURRENT" ] && [ "$CURRENT" != "$DESIRED" ]; then
  echo "install-hooks: replacing core.hooksPath '$CURRENT' -> '$DESIRED'." >&2
fi

git config core.hooksPath "$DESIRED"
echo "install-hooks: set core.hooksPath='$DESIRED'."
echo "install-hooks: tracked pre-commit (md auto-fix/enforce, fmt, ruff) + pre-push"
echo "install-hooks: (fast clippy/actionlint; full gate opt-in via FATHOMDB_PREPUSH_FULL=1) are now active."
