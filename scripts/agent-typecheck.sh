#!/usr/bin/env bash
# Type-check all language surfaces.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/agent-output.sh
. "$SCRIPT_DIR/lib/agent-output.sh"
cd_repo_root

# Rust: cargo check is the type-only gate (clippy already does this in lint, but check is cheaper).
run_capped typecheck-rust cargo check --workspace --quiet

# Python preflight: use the project's exact pinned version, so local prework
# cannot report a false green from version drift or an absent type checker.
readonly PYRIGHT_VERSION="1.1.410"
pyright_bin=""
if [ -x .venv/bin/pyright ]; then
  pyright_bin=".venv/bin/pyright"
elif command -v pyright >/dev/null 2>&1; then
  pyright_bin="$(command -v pyright)"
fi

if [ -z "$pyright_bin" ]; then
  printf 'FAIL typecheck-python: Pyright %s is required but not installed. Run scripts/bootstrap.sh in a clean non-worktree checkout.\n' "$PYRIGHT_VERSION" >&2
  exit 1
fi

pyright_version="$("$pyright_bin" --version)"
if [ "$pyright_version" != "pyright $PYRIGHT_VERSION" ]; then
  printf 'FAIL typecheck-python: Pyright %s is required; selected %s. Run scripts/bootstrap.sh in a clean non-worktree checkout.\n' "$PYRIGHT_VERSION" "$pyright_version" >&2
  exit 1
fi

run_capped typecheck-python "$pyright_bin" -p src/python

# TypeScript: tsc --noEmit if installed
if [ -d src/ts/node_modules ]; then
  run_capped typecheck-ts bash -c 'cd src/ts && npm run --silent typecheck'
else
  skip_notice typecheck-ts "src/ts/node_modules not installed"
fi
