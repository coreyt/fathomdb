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

# Python: ruff if available
if command -v ruff >/dev/null 2>&1; then
  run_capped lint-python ruff check src/python
else
  skip_notice lint-python "ruff not installed"
fi

# TypeScript: ESLint not configured yet
skip_notice lint-ts "ESLint not configured"
