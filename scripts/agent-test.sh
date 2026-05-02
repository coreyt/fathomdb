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
python_bin=""
if [ -x .venv/bin/python ]; then
  python_bin=".venv/bin/python"
elif command -v python3 >/dev/null 2>&1; then
  python_bin="$(command -v python3)"
fi

if [ -n "$python_bin" ] && "$python_bin" -c 'import pytest' >/dev/null 2>&1 && [ -d src/python/tests ]; then
  run_capped test-python "$python_bin" -m pytest -q src/python/tests
else
  skip_notice test-python "pytest not installed or no tests dir"
fi

# TypeScript: no test runner configured yet
skip_notice test-ts "no test runner configured"
