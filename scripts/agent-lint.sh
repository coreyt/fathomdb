#!/usr/bin/env bash
# Lint all language surfaces. Pass-through diagnostics unparaphrased on failure.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/agent-output.sh
. "$SCRIPT_DIR/lib/agent-output.sh"
cd_repo_root

# Rust: clippy with -D warnings (treat warnings as errors)
run_capped lint-rust cargo clippy --workspace --all-targets --quiet -- -D warnings

# Rust: format check
run_capped lint-rustfmt cargo fmt --all --check

# Migration authoring policy
run_capped lint-migrations "$SCRIPT_DIR/agent-lint-migrations.sh"

# Python: ruff if available
ruff_bin=""
if [ -x .venv/bin/ruff ]; then
  ruff_bin=".venv/bin/ruff"
elif command -v ruff >/dev/null 2>&1; then
  ruff_bin="$(command -v ruff)"
fi

if [ -n "$ruff_bin" ]; then
  run_capped lint-python "$ruff_bin" check src/python
else
  skip_notice lint-python "ruff not installed"
fi

# TypeScript: ESLint not configured yet
skip_notice lint-ts "ESLint not configured"

# Markdown: structural + format + link integrity
"$SCRIPT_DIR/agent-lint-md.sh"
