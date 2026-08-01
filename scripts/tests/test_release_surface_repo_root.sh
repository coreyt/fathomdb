#!/usr/bin/env bash
# Regression guard for the emitted TypeScript release-surface test.  `tsc`
# emits it at src/ts/dist/tests, so the repository root is exactly four
# parents above its runtime __dirname.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
RELEASE_SURFACE_TEST="$REPO_ROOT/src/ts/tests/release-surface.test.ts"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

if [ ! -f "$RELEASE_SURFACE_TEST" ]; then
  fail "release-surface TypeScript test must exist"
elif grep -qF 'const REPO_ROOT = resolve(__dirname, "..", "..", "..", "..");' "$RELEASE_SURFACE_TEST"; then
  pass "release-surface test resolves its emitted runtime directory four levels to repo root"
else
  fail "release-surface test must resolve __dirname four levels up to repo root"
fi

compiled_test_dir="$REPO_ROOT/src/ts/dist/tests"
computed_root="$(realpath -m "$compiled_test_dir/../../../..")"
if [ "$computed_root" = "$REPO_ROOT" ]; then
  pass "four parents from the emitted TypeScript test directory reach this repository"
else
  fail "compiled-layout fixture resolved $computed_root, expected $REPO_ROOT"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nRelease-surface repository-root test passed\n'
