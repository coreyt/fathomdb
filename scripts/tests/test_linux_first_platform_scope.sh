#!/usr/bin/env bash
# Static contract guard for the HITL's 0.8.20 Linux-first native scope
# (steward seq-234).  It deliberately uses text assertions only: actionlint
# remains the workflow YAML syntax/schema authority.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CI="$REPO_ROOT/.github/workflows/ci.yml"
RELEASE="$REPO_ROOT/.github/workflows/release.yml"
BRIEF="$REPO_ROOT/dev/plans/runs/0.8.20-slice-40-commission-brief.md"
PLAN="$REPO_ROOT/dev/plans/plan-0.8.20.md"
MASTER="$REPO_ROOT/dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md"
STATUS="$REPO_ROOT/dev/plans/runs/STATUS-0.8.20.md"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

for workflow in "$CI" "$RELEASE"; do
  name="$(basename "$workflow")"
  if grep -nEi '^[[:space:]]*runs-on:[[:space:]]*(macos|windows)-' "$workflow" >/dev/null; then
    fail "$name schedules a macOS or Windows runner"
  else
    pass "$name has no macOS or Windows runner selection"
  fi

  if grep -nEi '^[[:space:]]*target:[[:space:]]*(aarch64-unknown-linux-gnu|[^[:space:]#]*apple-darwin|[^[:space:]#]*windows[^[:space:]#]*)' "$workflow" >/dev/null; then
    fail "$name contains a deferred native target in an active matrix"
  else
    pass "$name has no non-x86_64-Linux active native target"
  fi

  if grep -nEi '^[[:space:]]*label:[[:space:]]*(linux-aarch64|darwin|win32)' "$workflow" >/dev/null; then
    fail "$name contains a deferred native artifact label in an active matrix"
  else
    pass "$name has no deferred native artifact label"
  fi
done

literal_targets="$(grep -hE '^[[:space:]]+target:[[:space:]]+' "$CI" "$RELEASE" | grep -vF '${{ matrix.target }}' || true)"
if [ -n "$literal_targets" ] && printf '%s\n' "$literal_targets" | grep -vE '^[[:space:]]*target:[[:space:]]*x86_64-unknown-linux-gnu[[:space:]]*(#.*)?$' >/dev/null; then
  fail "native artifact target matrices contain a value other than x86_64-unknown-linux-gnu"
else
  pass "native artifact target matrices are Linux x86_64 only"
fi

if grep -qE '^[[:space:]]*target:[[:space:]]*x86_64-unknown-linux-gnu[[:space:]]*$' "$RELEASE" && \
  grep -qE '^[[:space:]]*label:[[:space:]]*linux-x64-gnu[[:space:]]*$' "$RELEASE"; then
  pass "release retains the Linux x86_64 Python and N-API artifact path"
else
  fail "release must retain the Linux x86_64 Python and N-API artifact path"
fi

for required_job in changes verify security; do
  if grep -qE "^[[:space:]]{2}${required_job}:" "$CI"; then
    pass "CI retains the Linux ${required_job} safety gate"
  else
    fail "CI must retain the Linux ${required_job} safety gate"
  fi
done

if grep -qiE 'macOS/Windows.*0\.8\.22|0\.8\.22.*macOS/Windows' "$RELEASE"; then
  pass "release workflow explicitly defers macOS/Windows native work to 0.8.22"
else
  fail "release workflow must explicitly defer macOS/Windows native work to 0.8.22"
fi

for doc in "$BRIEF" "$PLAN" "$MASTER" "$STATUS"; do
  if grep -qiE 'B4.*(cancelled|canceled).*0\.8\.22|0\.8\.22.*B4.*(cancelled|canceled)' "$doc"; then
    pass "$(basename "$doc") records B4 as cancelled and deferred to 0.8.22"
  else
    fail "$(basename "$doc") must record B4 as cancelled and deferred to 0.8.22"
  fi
  if grep -qiE 'five.*(relevant )?Linux CI.*TC-91|TC-91.*five.*(relevant )?Linux CI' "$doc"; then
    pass "$(basename "$doc") requires five relevant Linux CI TC-91 greens"
  else
    fail "$(basename "$doc") must require five relevant Linux CI TC-91 greens"
  fi
done

if grep -Fq 'seq-233' "$BRIEF"; then
  pass "brief preserves the unresolved B5 binding-route authority"
else
  fail "brief must preserve B5 seq-233 authority"
fi
if grep -Fq 'gate (ii)' "$BRIEF"; then
  pass "brief preserves the explicit publish gate (ii) stop"
else
  fail "brief must preserve the explicit publish gate (ii) stop"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll Linux-first platform-scope tests passed\n'
