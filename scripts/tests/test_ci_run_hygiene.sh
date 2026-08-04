#!/usr/bin/env bash
# CI must cancel only superseded pull-request runs. Every landed main commit
# needs an independent result, and the group must not collide across workflows.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CI="${CI:-$REPO_ROOT/.github/workflows/ci.yml}"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

if [ ! -f "$CI" ]; then
  fail "ci.yml not found at $CI"
  exit 1
fi

concurrency_block="$(awk '
  /^concurrency:$/ { in_concurrency = 1; next }
  in_concurrency && /^[^[:space:]]/ { exit }
  in_concurrency { print }
' "$CI")"

if [ -z "$concurrency_block" ]; then
  fail "ci.yml must define top-level workflow concurrency"
elif grep -Fqx '  group: ${{ github.workflow }}-${{ github.ref }}' <<<"$concurrency_block"; then
  pass "CI concurrency group is workflow/ref scoped"
else
  fail "CI concurrency group must be workflow/ref scoped"
fi

if grep -Fqx "  cancel-in-progress: \${{ github.event_name == 'pull_request' }}" <<<"$concurrency_block"; then
  pass "CI cancels only superseded pull-request runs"
else
  fail "CI must never cancel push or release runs"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nCI run-hygiene test passed\n'
