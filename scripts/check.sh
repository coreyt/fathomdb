#!/usr/bin/env bash
set -euo pipefail

cargo fmt --check
cargo check --workspace
cargo test --workspace
python3 -m compileall src/python/fathomdb src/python/tests

if [ -x src/ts/node_modules/.bin/tsc ]; then
  (cd src/ts && npm run typecheck)
else
  echo "skipping TypeScript typecheck (run 'cd src/ts && npm install' to enable)"
fi

if command -v mkdocs >/dev/null 2>&1; then
  mkdocs build --strict
else
  echo "skipping MkDocs build (mkdocs not installed)"
fi
