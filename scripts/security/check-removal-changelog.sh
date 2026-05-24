#!/usr/bin/env bash
# AC-050c removal-detect linter wrapper. Forwards args to the python
# implementation; defaults base=v0.6.0, head=HEAD.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$SCRIPT_DIR/check_removal_changelog.py" "$@"
