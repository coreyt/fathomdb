#!/usr/bin/env bash
# scripts/tests/test_actionlint_fixture.sh — proves actionlint is
# installed, runnable, and rejects the deliberately-broken fixture under
# scripts/tests/fixtures/. Existence of this test is the contract that
# scripts/agent-lint.sh's workflow-validation step is non-trivial.
#
# WHY this fixture and not a .github/workflows/* file: the agent-lint glob
# is `.github/workflows/*.yml` and would catch a broken file there as a
# real failure. The fixture lives outside that glob so the suite can
# exercise the bad-input path without breaking the canonical workflow
# directory.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIX="$SCRIPT_DIR/fixtures/actionlint-bad.yml"

if ! command -v actionlint >/dev/null 2>&1; then
  printf 'SKIP  actionlint not installed (run scripts/bootstrap.sh)\n'
  exit 0
fi

if actionlint "$FIX" >/dev/null 2>&1; then
  printf 'FAIL  actionlint accepted the deliberately-broken fixture\n' >&2
  exit 1
fi

printf 'PASS  actionlint rejects deliberately-broken fixture\n'
