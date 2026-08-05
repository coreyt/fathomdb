#!/usr/bin/env bash
# Lint all language surfaces. Pass-through diagnostics unparaphrased on failure.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/agent-output.sh
. "$SCRIPT_DIR/lib/agent-output.sh"
# shellcheck source=lib/actionlint-version.sh
. "$SCRIPT_DIR/lib/actionlint-version.sh"
# shellcheck source=lib/shellcheck-version.sh
. "$SCRIPT_DIR/lib/shellcheck-version.sh"
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

# Shell preflight: shellcheck was invoked NOWHERE before 0.8.21 Slice 30 — 140
# tracked *.sh, 123 of them under `set -euo pipefail`, zero linting, and four
# hand-fixes of the same SIGPIPE/masked-return class. Same hard-fail-on-drift
# posture as ruff/actionlint: a green from the wrong shellcheck version is a
# false green, and an ABSENT shellcheck is a FAILED lint, never a skip (TC-37).
# The message shape comes from scripts/lib/shellcheck-version.sh so the
# preflight and the lint leg cannot disagree.
if ! require_shellcheck_bin lint-shell >/dev/null; then
  exit 1
fi

# Shell lint runs BEFORE the language toolchains, for the same reason the
# actionlint pin does: every other leg below is executed BY shell, and shell
# lint costs ~1s. A broken script must not be reported as "clippy failed".
# Two legs (full default ruleset repo-wide + ratcheted SC2312) — see
# scripts/agent-lint-shell.sh.
run_capped lint-shell "$SCRIPT_DIR/agent-lint-shell.sh"

# Rust: clippy with -D warnings (treat warnings as errors)
run_capped lint-rust cargo clippy --workspace --all-targets --quiet -- -D warnings

# Rust: format check
run_capped lint-rustfmt cargo fmt --all --check

# Migration authoring policy
run_capped lint-migrations "$SCRIPT_DIR/agent-lint-migrations.sh"
run_capped platform-capabilities "$SCRIPT_DIR/check-platform-capabilities.sh"
run_capped public-doc-truth "$SCRIPT_DIR/check-public-doc-truth.py"
run_capped pinned-override-rot "$SCRIPT_DIR/check-pinned-override-rot.sh"

# Python
run_capped lint-python "$ruff_bin" check src/python

# TypeScript: ESLint not configured yet
skip_notice lint-ts "ESLint not configured"

# Workflows: the exact version was checked before Rust/Python lint.
run_capped lint-actions "$actionlint_bin" .github/workflows/*.yml

# Markdown: structural + format + link integrity
"$SCRIPT_DIR/agent-lint-md.sh"
