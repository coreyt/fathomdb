#!/usr/bin/env bash
# The cache-coverage guard is exercised on GitHub-hosted runners, where rg is
# not part of the guaranteed shell-tool baseline. Re-run it with a minimal
# PATH containing only the commands it is allowed to need.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
GUARD="$REPO_ROOT/scripts/tests/test_ts_cache_coverage_split.sh"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

tool_dir="$(mktemp -d)"
cleanup() { rm -rf "$tool_dir"; }
trap cleanup EXIT

for tool in bash awk cut dirname find grep head sort; do
  tool_path="$(command -v "$tool" || true)"
  if [ -z "$tool_path" ] || [ ! -x "$tool_path" ]; then
    fail "required baseline command $tool must be available"
  else
    ln -s "$tool_path" "$tool_dir/$tool"
  fi
done

if [ -e "$tool_dir/rg" ]; then
  fail "minimal PATH must exclude rg"
elif [ "$FAILED" -eq 0 ] && PATH="$tool_dir" "$tool_dir/bash" "$GUARD"; then
  pass "cache-coverage guard passes without rg on PATH"
else
  fail "cache-coverage guard must not require rg"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nTypeScript cache-coverage no-rg test passed\n'
