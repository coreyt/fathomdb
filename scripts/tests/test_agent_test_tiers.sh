#!/usr/bin/env bash
# Regression guard for the mechanically-total fast/heavy agent-test partition.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
AGENT_TEST="$REPO_ROOT/scripts/agent-test.sh"
AGENT_VERIFY="$REPO_ROOT/scripts/agent-verify.sh"
CHECKER="$REPO_ROOT/scripts/check-agent-test-tier-totality.sh"
CI="$REPO_ROOT/.github/workflows/ci.yml"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

TMPROOT="$(mktemp -d)"
cleanup() {
  case "$TMPROOT" in
    "${TMPDIR:-/tmp}"/*|/tmp/*) rm -rf "$TMPROOT" ;;
    *) printf 'refusing to remove unexpected temp path: %s\n' "$TMPROOT" >&2 ;;
  esac
}
trap cleanup EXIT

if [ ! -x "$CHECKER" ]; then
  fail "the totality checker is executable at scripts/check-agent-test-tier-totality.sh"
elif "$CHECKER" --script "$AGENT_TEST"; then
  pass "the real agent-test registrations form an exact fast/heavy partition"
else
  fail "the real agent-test registrations must form an exact fast/heavy partition"
fi

UNASSIGNED="$TMPROOT/agent-test-unassigned.sh"
cp "$AGENT_TEST" "$UNASSIGNED"
sed -i '0,/^[[:space:]]*run_tier_suite fast /s//run_tier_suite neither /' "$UNASSIGNED"
set +e
UNASSIGNED_OUT="$("$CHECKER" --script "$UNASSIGNED" 2>&1)"
UNASSIGNED_RC=$?
set -e
if [ "$UNASSIGNED_RC" -ne 0 ] && grep -qF 'unassigned' <<<"$UNASSIGNED_OUT"; then
  pass "mutation: an unassigned registered suite hard-fails"
else
  fail "mutation: an unassigned registered suite must hard-fail: $UNASSIGNED_OUT"
fi

DUPLICATE="$TMPROOT/agent-test-duplicate.sh"
cp "$AGENT_TEST" "$DUPLICATE"
sed -i '0,/^[[:space:]]*run_tier_suite fast test-release-version-surfaces /s//run_tier_suite fast test-set-version /' "$DUPLICATE"
set +e
DUPLICATE_OUT="$("$CHECKER" --script "$DUPLICATE" 2>&1)"
DUPLICATE_RC=$?
set -e
if [ "$DUPLICATE_RC" -ne 0 ] && grep -qF 'duplicate suite label' <<<"$DUPLICATE_OUT"; then
  pass "mutation: a duplicate suite label in the same tier hard-fails"
else
  fail "mutation: a duplicate suite label in the same tier must hard-fail: $DUPLICATE_OUT"
fi

RAW="$TMPROOT/agent-test-raw-registration.sh"
cp "$AGENT_TEST" "$RAW"
sed -i '/suite_summary_and_exit/i run_suite silently-unassigned true' "$RAW"
set +e
RAW_OUT="$("$CHECKER" --script "$RAW" 2>&1)"
RAW_RC=$?
set -e
if [ "$RAW_RC" -ne 0 ] && grep -qF 'raw run_suite registration' <<<"$RAW_OUT"; then
  pass "mutation: a raw unpartitioned registration hard-fails"
else
  fail "mutation: a raw unpartitioned registration must hard-fail: $RAW_OUT"
fi

INDIRECT="$TMPROOT/agent-test-indirect-registration.sh"
cp "$AGENT_TEST" "$INDIRECT"
sed -i '$a\
indirect_run() {\
  run_suite "$@"\
}\
indirect_run silently-unassigned true' "$INDIRECT"
set +e
INDIRECT_OUT="$("$CHECKER" --script "$INDIRECT" 2>&1)"
INDIRECT_RC=$?
set -e
if [ "$INDIRECT_RC" -ne 0 ] && grep -qF 'raw run_suite registration' <<<"$INDIRECT_OUT"; then
  pass "mutation: a local raw-registration wrapper hard-fails"
else
  fail "mutation: a local raw-registration wrapper must hard-fail: $INDIRECT_OUT"
fi

run_argv() {
  set +e
  ARGV_OUT="$(cd "$REPO_ROOT" && timeout 20 bash scripts/agent-test.sh "$@" 2>&1)"
  ARGV_RC=$?
  set -e
}

run_argv --tier=invalid
if [ "$ARGV_RC" -eq 2 ] && grep -qF 'Usage:' <<<"$ARGV_OUT"; then
  pass "an invalid --tier exits 2 before any suite runs"
else
  fail "an invalid --tier must exit 2 before any suite runs: rc=$ARGV_RC out=$ARGV_OUT"
fi

set +e
VERIFY_OUT="$(cd "$REPO_ROOT" && timeout 20 bash "$AGENT_VERIFY" --tier=invalid 2>&1)"
VERIFY_RC=$?
set -e
if [ "$VERIFY_RC" -eq 2 ] && grep -qF 'Usage:' <<<"$VERIFY_OUT"; then
  pass "agent-verify rejects an invalid tier before any verifier step runs"
else
  fail "agent-verify must reject an invalid tier before any verifier step runs: rc=$VERIFY_RC out=$VERIFY_OUT"
fi

run_argv --tier=fast --exclude-suite=test-set-version
if [ "$ARGV_RC" -eq 2 ] && grep -qF 'cannot combine' <<<"$ARGV_OUT"; then
  pass "a tier gate refuses an exclusion instead of silently omitting a suite"
else
  fail "a tier gate must refuse an exclusion: rc=$ARGV_RC out=$ARGV_OUT"
fi

if grep -qF 'agent_test_tier="all"' "$AGENT_TEST" \
  && grep -qF -- '--tier=fast|heavy|all' "$AGENT_TEST"; then
  pass "agent-test defaults to all and documents the stable tier interface"
else
  fail "agent-test must default to --tier=all and document fast|heavy|all"
fi

verify_fast_block=""
verify_heavy_block=""
if [ -f "$CI" ]; then
  verify_fast_block="$(awk '/^  verify-fast:$/ { active = 1; next } active && /^  [[:alnum:]_-]+:$/ { exit } active { print }' "$CI")"
  verify_heavy_block="$(awk '/^  verify:$/ { active = 1; next } active && /^  [[:alnum:]_-]+:$/ { exit } active { print }' "$CI")"
fi
if [ -f "$CI" ] \
  && grep -qF 'bash scripts/agent-verify.sh --tier=fast' <<<"$verify_fast_block" \
  && grep -qF 'bash scripts/agent-verify.sh --tier=heavy' <<<"$verify_heavy_block"; then
  pass "CI requires explicit fast and heavy verifier tiers"
else
  fail "CI must run explicit --tier=fast and --tier=heavy verifier gates"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nall agent-test tier totality tests passed\n'
