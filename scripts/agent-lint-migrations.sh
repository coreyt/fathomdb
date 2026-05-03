#!/usr/bin/env bash
# Enforce migration accretion guard over repo migration files.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 "$SCRIPT_DIR/agent-lint-migrations.py" "$@"
