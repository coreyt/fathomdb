#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

echo "FathomDB scaffold bootstrap"
echo "Public docs live in docs/ and build with MkDocs."
echo "Internal engineering docs live in dev/."
echo "Rust workspace members live under src/rust/crates/."
echo "Run scripts/agent-verify.sh during the agent loop, scripts/check.sh as the broader CI gate."

# Repo-tracked git hooks (pre-commit fmt/lint, pre-push agent-verify).
scripts/install-hooks.sh

# Python dev tooling — pytest, hypothesis, ruff, pyright.
if [ -f src/python/pyproject.toml ]; then
  echo "Installing Python dev tooling into .venv (pytest + hypothesis + ruff + pyright)..."
  python3 -m venv .venv
  .venv/bin/python -m pip install --quiet --upgrade pip
  .venv/bin/python -m pip install --quiet -e 'src/python[dev]'
  .venv/bin/python -c 'import pytest, hypothesis'
  .venv/bin/pyright -p src/python >/dev/null
fi

# TypeScript dev tooling.
if [ -f src/ts/package.json ] && [ ! -d src/ts/node_modules ]; then
  echo "Installing TypeScript dev tooling..."
  (cd src/ts && npm install --silent)
fi

# Repo-wide markdown tooling (markdownlint-cli2 + prettier).
if [ -f package.json ] && [ ! -d node_modules ]; then
  echo "Installing markdown dev tooling (markdownlint-cli2 + prettier)..."
  npm install --silent
fi

# Lychee link checker (Rust binary).
if ! command -v lychee >/dev/null 2>&1; then
  echo "Installing lychee link checker..."
  cargo install --locked --quiet lychee
fi

# actionlint — workflow validator. Pinned: yaml.safe_load passes
# schema-invalid syntax that GitHub silently rejects, so we need a real
# linter for .github/workflows/*.yml. Version pin matches a recent stable
# release; bump deliberately, not drifted.
if ! command -v actionlint >/dev/null 2>&1; then
  if command -v go >/dev/null 2>&1; then
    echo "Installing actionlint v1.7.7 via go install..."
    GO111MODULE=on go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.7
  else
    echo "actionlint not installed and go toolchain unavailable; install actionlint manually" >&2
    echo "  see https://github.com/rhysd/actionlint/releases (pin v1.7.7)" >&2
  fi
fi
