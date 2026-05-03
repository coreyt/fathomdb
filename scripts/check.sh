#!/usr/bin/env bash
set -euo pipefail

cargo fmt --check
cargo check --workspace
# `AGENT_LONG=1` exercises the spec-conforming long-running variants
# (e.g. AC-021's 60 s schema-error window) as part of the broad CI gate.
AGENT_LONG=1 cargo test --workspace
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
