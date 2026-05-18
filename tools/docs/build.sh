#!/usr/bin/env bash
# tools/docs/build.sh — local mkdocs build (strict).
# Same venv discipline as serve.sh; produces a static site at site/
# (gitignored). Use this to verify --strict cleanliness before pushing
# docs/ changes; CI runs the same check.
#
# Usage:
#   bash tools/docs/build.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
VENV="$SCRIPT_DIR/.venv"

if [ ! -d "$VENV" ]; then
  echo "==> creating venv at $VENV"
  python3 -m venv "$VENV"
  # shellcheck disable=SC1091
  source "$VENV/bin/activate"
  pip install --quiet --upgrade pip
  pip install --quiet -r "$SCRIPT_DIR/requirements.txt"
  echo "==> mkdocs installed; venv ready"
else
  # shellcheck disable=SC1091
  source "$VENV/bin/activate"
fi

cd "$REPO_ROOT"
exec mkdocs build --strict
