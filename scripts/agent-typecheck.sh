#!/usr/bin/env bash
# Type-check all language surfaces.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/agent-output.sh
. "$SCRIPT_DIR/lib/agent-output.sh"
cd_repo_root

# Rust: cargo check is the type-only gate (clippy already does this in lint, but check is cheaper).
run_capped typecheck-rust cargo check --workspace --quiet

# Python: pyright if available
if command -v pyright >/dev/null 2>&1; then
  run_capped typecheck-python pyright src/python
else
  skip_notice typecheck-python "pyright not installed"
fi

# TypeScript: tsc --noEmit if installed
if [ -d src/ts/node_modules ]; then
  run_capped typecheck-ts bash -c 'cd src/ts && npm run --silent typecheck'
else
  skip_notice typecheck-ts "src/ts/node_modules not installed"
fi
