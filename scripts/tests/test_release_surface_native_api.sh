#!/usr/bin/env bash
# The release-surface arm loads the raw N-API .node, not the TypeScript
# wrapper. Its default-embedder smoke must therefore use the native static
# Engine.open factory rather than a wrapper-only helper.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
RELEASE_SURFACE_TEST="$REPO_ROOT/src/ts/tests/release-surface.test.ts"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

if [ ! -f "$RELEASE_SURFACE_TEST" ]; then
  fail "release-surface TypeScript test must exist"
else
  if grep -qF 'const rawEngine = loaded.Engine as' "$RELEASE_SURFACE_TEST" && \
    grep -qF 'const open = rawEngine?.open;' "$RELEASE_SURFACE_TEST" && \
    grep -qF 'typeof open === "function"' "$RELEASE_SURFACE_TEST" && \
    grep -qF 'await open(path, { useDefaultEmbedder: true })' "$RELEASE_SURFACE_TEST"; then
    pass "release-surface default-embedder smoke uses raw Engine.open"
  else
    fail "release-surface smoke must assert and invoke raw Engine.open"
  fi
  if grep -qF 'loaded.engineOpen' "$RELEASE_SURFACE_TEST"; then
    fail "release-surface raw N-API smoke must not call wrapper-only engineOpen"
  else
    pass "release-surface smoke does not rely on wrapper-only engineOpen"
  fi
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nRelease-surface native API test passed\n'
