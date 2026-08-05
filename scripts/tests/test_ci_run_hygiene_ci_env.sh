#!/usr/bin/env bash
# The conventional CI=true environment marker must not replace the ci.yml path.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET="$SCRIPT_DIR/test_ci_run_hygiene.sh"

set +e
OUT="$(CI=true bash "$TARGET" 2>&1)"
RC=$?
set -e

printf '%s\n' "$OUT"
printf 'exit=%d\n' "$RC"

if [ "$RC" -ne 0 ]; then
  printf 'FAIL  CI=true must not replace the ci.yml path: %s\n' "$OUT" >&2
  exit 1
fi
if ! grep -Fq 'CI run-hygiene test passed' <<<"$OUT"; then
  printf 'FAIL  CI=true run did not exercise the run-hygiene assertions\n' >&2
  exit 1
fi
printf 'PASS  CI=true does not replace the ci.yml path\n'
