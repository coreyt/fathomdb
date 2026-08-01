#!/usr/bin/env bash
# Lint all language surfaces. Pass-through diagnostics unparaphrased on failure.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/agent-output.sh
. "$SCRIPT_DIR/lib/agent-output.sh"
cd_repo_root

# Python preflight: use the exact version proven by CI, so local prework cannot
# report a false green from an older Ruff that misses newly added diagnostics.
readonly RUFF_VERSION="0.16.1"
ruff_bin=""
if [ -x .venv/bin/ruff ]; then
  ruff_bin=".venv/bin/ruff"
elif command -v ruff >/dev/null 2>&1; then
  ruff_bin="$(command -v ruff)"
fi

if [ -z "$ruff_bin" ]; then
  printf 'FAIL lint-python: Ruff %s is required but not installed. Run scripts/bootstrap.sh on the main checkout.\n' "$RUFF_VERSION" >&2
  exit 1
fi

ruff_version="$("$ruff_bin" --version)"
if [ "$ruff_version" != "ruff $RUFF_VERSION" ]; then
  printf 'FAIL lint-python: Ruff %s is required; selected %s. Run scripts/bootstrap.sh on the main checkout.\n' "$RUFF_VERSION" "$ruff_version" >&2
  exit 1
fi

# Rust: clippy with -D warnings (treat warnings as errors)
run_capped lint-rust cargo clippy --workspace --all-targets --quiet -- -D warnings

# Rust: format check
run_capped lint-rustfmt cargo fmt --all --check

# Migration authoring policy
run_capped lint-migrations "$SCRIPT_DIR/agent-lint-migrations.sh"

# Python
run_capped lint-python "$ruff_bin" check src/python

# TypeScript: ESLint not configured yet
skip_notice lint-ts "ESLint not configured"

# Workflows: actionlint is the canonical validator per feedback_workflow_validation
# (yaml.safe_load passes schema-invalid syntax GitHub silently rejects).
if command -v actionlint >/dev/null 2>&1; then
  run_capped lint-actions actionlint .github/workflows/*.yml
else
  skip_notice lint-actions "actionlint not installed (run scripts/bootstrap.sh)"
fi

# Markdown: structural + format + link integrity
"$SCRIPT_DIR/agent-lint-md.sh"
