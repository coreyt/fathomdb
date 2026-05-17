#!/usr/bin/env bash
# AC-050a TypeScript shim scanner. See ast_scan.py for the patterns.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$SCRIPT_DIR/ast_scan.py" --language ts "$@"
