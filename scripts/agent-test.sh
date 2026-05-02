#!/usr/bin/env bash
# Run unit tests across language surfaces.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/agent-output.sh
. "$SCRIPT_DIR/lib/agent-output.sh"
cd_repo_root

# Rust
run_capped test-rust cargo test --workspace --quiet --no-fail-fast

# Python
if command -v pytest >/dev/null 2>&1 && [ -d src/python/tests ]; then
  run_capped test-python pytest -q src/python/tests
else
  skip_notice test-python "pytest not installed or no tests dir"
fi

# TypeScript: no test runner configured yet
skip_notice test-ts "no test runner configured"
