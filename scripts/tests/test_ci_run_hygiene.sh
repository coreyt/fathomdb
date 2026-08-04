#!/usr/bin/env bash
# CI must coalesce only superseded pull-request runs. Every non-PR run needs a
# unique group, otherwise GitHub also cancels an older pending run in that group
# even when `cancel-in-progress` is false.
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
elif grep -Fqx "  group: \${{ github.event_name == 'pull_request' && format('{0}-{1}', github.workflow, github.ref) || format('{0}-{1}', github.workflow, github.run_id) }}" <<<"$concurrency_block"; then
  pass "CI shares a workflow/ref group only for pull requests and gives other runs unique IDs"
else
  fail "CI concurrency group must share only pull requests and use github.run_id for other runs"
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
