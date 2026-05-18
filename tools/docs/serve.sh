#!/usr/bin/env bash
# tools/docs/serve.sh — local mkdocs preview.
# Creates an isolated venv at tools/docs/.venv (gitignored), installs the
# pinned mkdocs, and serves docs/ at http://127.0.0.1:8000 with live
# reload. Browse + edit; saves auto-rebuild.
#
# Usage:
#   bash tools/docs/serve.sh
#   bash tools/docs/serve.sh --port 8080            # override default port
#   bash tools/docs/serve.sh --bind 0.0.0.0:8000    # bind for LAN preview
#
# First run installs mkdocs (~10 MB / ~10s). Subsequent runs reuse the
# venv (~1s startup).
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
exec mkdocs serve "$@"
