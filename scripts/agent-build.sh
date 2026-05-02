#!/usr/bin/env bash
# Build all language surfaces. Concise on success, structured on failure.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/agent-output.sh
. "$SCRIPT_DIR/lib/agent-output.sh"
cd_repo_root

# Rust
run_capped build-rust cargo build --workspace --quiet

# Python — install in editable mode if pyproject changed.
if [ -f src/python/pyproject.toml ]; then
  python_bin="python3"
  if [ -x .venv/bin/python ]; then
    python_bin=".venv/bin/python"
  fi
  sentinel=".cache/agent/python-installed"
  mkdir -p "$(dirname "$sentinel")"
  if [ ! -f "$sentinel" ] || [ src/python/pyproject.toml -nt "$sentinel" ]; then
    run_capped build-python "$python_bin" -m pip install --quiet -e src/python
    touch "$sentinel"
  fi
else
  skip_notice build-python "no src/python/pyproject.toml"
fi

# TypeScript — only if node_modules already installed.
if [ -d src/ts/node_modules ]; then
  run_capped build-ts bash -c 'cd src/ts && npm run --silent build'
else
  skip_notice build-ts "src/ts/node_modules not installed"
fi
