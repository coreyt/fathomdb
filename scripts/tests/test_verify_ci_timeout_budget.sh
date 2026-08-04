#!/usr/bin/env bash
# Structural guard for the split CI verifier budgets. The heavy language tier
# retains the approved 60-minute allowance; the fast diagnostic tier gets its
# own bounded 30-minute budget instead of hiding behind that slow tail.
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

# Restrict each assertion to its own top-level job, so another job's timeout
# cannot accidentally satisfy this contract.
verify_block="$(awk '
  /^  verify:$/ { in_verify = 1; next }
  in_verify && /^  [[:alnum:]_-]+:$/ { exit }
  in_verify { print }
' "$CI")"

fast_block="$(awk '
  /^  verify-fast:$/ { in_fast = 1; next }
  in_fast && /^  [[:alnum:]_-]+:$/ { exit }
  in_fast { print }
' "$CI")"

if [ -z "$verify_block" ]; then
  fail "ci.yml must retain the top-level verify job"
elif grep -qE '^[[:space:]]*timeout-minutes:[[:space:]]*60[[:space:]]*$' <<<"$verify_block"; then
  pass "CI heavy verify job preserves the 60-minute language-suite budget"
else
  fail "CI heavy verify job must allow 60 minutes"
fi

if [ -z "$fast_block" ]; then
  fail "ci.yml must define the top-level verify-fast job"
elif grep -qE '^[[:space:]]*timeout-minutes:[[:space:]]*30[[:space:]]*$' <<<"$fast_block"; then
  pass "CI fast verify job has its own 30-minute diagnostic budget"
else
  fail "CI fast verify job must allow 30 minutes"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nVerify CI timeout-budget test passed\n'
