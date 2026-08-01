#!/usr/bin/env bash
# Structural guard for the CI agent-loop timeout budget.  A clean bootstrap
# plus the documented full local `scripts/agent-verify.sh` gate needs more
# than the former 30-minute CI allowance; retain the approved 60-minute
# budget so CI can complete the same prework it asks developers to run.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
CI="$REPO_ROOT/.github/workflows/ci.yml"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

if [ ! -f "$CI" ]; then
  fail "ci.yml not found at $CI"
  exit 1
fi

# Restrict the assertion to the top-level verify job, so another job's
# timeout cannot accidentally satisfy this contract.
verify_block="$(awk '
  /^  verify:$/ { in_verify = 1; next }
  in_verify && /^  [[:alnum:]_-]+:$/ { exit }
  in_verify { print }
' "$CI")"

if [ -z "$verify_block" ]; then
  fail "ci.yml must retain the top-level verify job"
elif grep -qE '^[[:space:]]*timeout-minutes:[[:space:]]*60[[:space:]]*$' <<<"$verify_block"; then
  pass "CI verify job preserves the 60-minute bootstrap plus agent-verify budget"
else
  fail "CI verify job must allow 60 minutes for bootstrap plus agent-verify"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nVerify CI timeout-budget test passed\n'
