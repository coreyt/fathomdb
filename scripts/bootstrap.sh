#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

echo "FathomDB scaffold bootstrap"
echo "Public docs live in docs/ and build with MkDocs."
echo "Internal engineering docs live in dev/."
echo "Rust workspace members live under src/rust/crates/."
echo "Run scripts/agent-verify.sh during the agent loop, scripts/check.sh as the broader CI gate."

# Python dev tooling — pytest, hypothesis, ruff, pyright.
if [ -f src/python/pyproject.toml ]; then
  echo "Installing Python dev tooling (pytest + hypothesis + ruff + pyright)..."
  pip install --quiet -e 'src/python[dev]'
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
