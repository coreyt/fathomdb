#!/usr/bin/env bash
# Lint all language surfaces. Pass-through diagnostics unparaphrased on failure.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/agent-output.sh
. "$SCRIPT_DIR/lib/agent-output.sh"
# shellcheck source=lib/actionlint-version.sh
. "$SCRIPT_DIR/lib/actionlint-version.sh"
cd_repo_root

# Python preflight: use the project's exact pinned version, so local prework
# cannot report a false green from version drift.
readonly RUFF_VERSION="0.15.17"
ruff_bin=""
if [ -x .venv/bin/ruff ]; then
  ruff_bin=".venv/bin/ruff"
elif command -v ruff >/dev/null 2>&1; then
  ruff_bin="$(command -v ruff)"
fi

if [ -z "$ruff_bin" ]; then
  printf 'FAIL lint-python: Ruff %s is required but not installed. Run scripts/bootstrap.sh in a clean non-worktree checkout.\n' "$RUFF_VERSION" >&2
  exit 1
fi

ruff_version="$("$ruff_bin" --version)"
if [ "$ruff_version" != "ruff $RUFF_VERSION" ]; then
  printf 'FAIL lint-python: Ruff %s is required; selected %s. Run scripts/bootstrap.sh in a clean non-worktree checkout.\n' "$RUFF_VERSION" "$ruff_version" >&2
  exit 1
fi

# Workflow preflight: actionlint is the canonical validator per
# feedback_workflow_validation (yaml.safe_load passes schema-invalid syntax
# GitHub silently rejects). Match the CI pin before other lints can hide a
# workflow-tooling mismatch.
readonly ACTIONLINT_VERSION="1.7.12"
actionlint_bin="$(find_actionlint_bin || true)"
if [ -z "$actionlint_bin" ]; then
  printf 'FAIL lint-actions: actionlint %s is required but not installed. Run scripts/bootstrap.sh in a clean non-worktree checkout.\n' "$ACTIONLINT_VERSION" >&2
  exit 1
fi

actionlint_version="$(read_actionlint_version "$actionlint_bin")"
if [ "$actionlint_version" != "$ACTIONLINT_VERSION" ]; then
  printf 'FAIL lint-actions: actionlint %s is required; selected %s. Run scripts/bootstrap.sh in a clean non-worktree checkout.\n' "$ACTIONLINT_VERSION" "$actionlint_version" >&2
  exit 1
fi

# Rust: clippy with -D warnings (treat warnings as errors)
run_capped lint-rust cargo clippy --workspace --all-targets --quiet -- -D warnings

# Rust: format check
run_capped lint-rustfmt cargo fmt --all --check

# Migration authoring policy
run_capped lint-migrations "$SCRIPT_DIR/agent-lint-migrations.sh"
run_capped platform-capabilities "$SCRIPT_DIR/check-platform-capabilities.sh"
run_capped public-doc-truth "$SCRIPT_DIR/check-public-doc-truth.py"

# Python
run_capped lint-python "$ruff_bin" check src/python

# TypeScript: ESLint not configured yet
skip_notice lint-ts "ESLint not configured"

# Workflows: the exact version was checked before Rust/Python lint.
run_capped lint-actions "$actionlint_bin" .github/workflows/*.yml

# Markdown: structural + format + link integrity
"$SCRIPT_DIR/agent-lint-md.sh"
