#!/usr/bin/env bash
# Static contract guard for the HITL's 0.8.20 Linux-first native scope
# (steward seq-234).  It deliberately uses text assertions only: actionlint
# remains the workflow YAML syntax/schema authority.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
CI="$REPO_ROOT/.github/workflows/ci.yml"
RELEASE="$REPO_ROOT/.github/workflows/release.yml"
BRIEF="$REPO_ROOT/dev/plans/runs/0.8.20-slice-40-commission-brief.md"
PLAN="$REPO_ROOT/dev/plans/plan-0.8.20.md"
MASTER="$REPO_ROOT/dev/plans/0.8.6-0.8.16-PROGRAM-SEQUENCING.md"
STATUS="$REPO_ROOT/dev/plans/runs/STATUS-0.8.20.md"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

# Text-only runner/matrix contract (not a YAML parser): every active literal
# `runs-on` must select Ubuntu; every `${{ matrix.runner }}` route must derive
# from an active Ubuntu `runner` value; and active CI/release native `target`
# and `label` values must be the sole Linux x86_64 routes. Expressions that
# *consume* matrix.target/label are deliberately excluded so they cannot be
# mistaken for matrix input values. actionlint remains workflow validation.
active_matrix_values() {
  local workflow="$1"
  local field="$2"
  grep -hE "^[[:space:]]*(-[[:space:]]+)?${field}:[[:space:]]+" "$workflow" \
    | sed -E "s/^[[:space:]]*(-[[:space:]]+)?${field}:[[:space:]]*//; s/[[:space:]]+#.*$//" \
    | grep -vE "^\\$\\{\\{[[:space:]]*matrix\\.${field}[[:space:]]*\\}\\}$" || true
}

assert_only_active_values() {
  local workflow="$1"
  local field="$2"
  local allowed="$3"
  local description="$4"
  local values bad
  values="$(active_matrix_values "$workflow" "$field")"
  bad="$(printf '%s\n' "$values" | sed '/^$/d' | grep -vE "$allowed" || true)"
  if [ -n "$bad" ]; then
    fail "$description: $bad"
  else
    pass "$description"
  fi
}

require_active_values() {
  local workflow="$1"
  local field="$2"
  local description="$3"
  if [ -n "$(active_matrix_values "$workflow" "$field")" ]; then
    pass "$description"
  else
    fail "$description"
  fi
}

for workflow in "$CI" "$RELEASE"; do
  name="$(basename "$workflow")"
  literal_runners="$(grep -hE '^[[:space:]]*runs-on:[[:space:]]+' "$workflow" \
    | sed -E 's/^[[:space:]]*runs-on:[[:space:]]*//; s/[[:space:]]+#.*$//' \
    | grep -vE '^\$\{\{[[:space:]]*matrix\.runner[[:space:]]*\}\}$' || true)"
  bad_literal_runners="$(printf '%s\n' "$literal_runners" | sed '/^$/d' \
    | grep -vE '^ubuntu(-[[:alnum:].-]+)?$' || true)"
  if [ -n "$bad_literal_runners" ]; then
    fail "$name has a non-Linux literal runs-on value: $bad_literal_runners"
  else
    pass "$name has only Linux literal runs-on values"
  fi

  matrix_runs_on="$(grep -hE '^[[:space:]]*runs-on:[[:space:]]*\$\{\{[[:space:]]*matrix\.' "$workflow" || true)"
  unguarded_matrix_runs_on="$(printf '%s\n' "$matrix_runs_on" | grep -vF '${{ matrix.runner }}' || true)"
  if [ -n "$unguarded_matrix_runs_on" ]; then
    fail "$name routes runs-on through an unguarded matrix field"
  else
    pass "$name has no unguarded matrix runs-on route"
  fi

  if grep -qE '^[[:space:]]*runs-on:[[:space:]]*\$\{\{[[:space:]]*matrix\.runner[[:space:]]*\}\}' "$workflow" && \
    [ -z "$(active_matrix_values "$workflow" runner)" ]; then
    fail "$name routes runs-on through matrix.runner without active runner values"
  else
    pass "$name matrix.runner routes have active runner values"
  fi

  assert_only_active_values "$workflow" runner '^ubuntu(-[[:alnum:].-]+)?$' \
    "$name matrix runner values are Linux"
  assert_only_active_values "$workflow" target '^x86_64-unknown-linux-gnu$' \
    "$name native artifact target values are Linux x86_64 only"
done

require_active_values "$CI" target "ci.yml retains an active Linux x86_64 artifact target"
require_active_values "$CI" label "ci.yml retains an active Linux x86_64 artifact label"
require_active_values "$RELEASE" target "release.yml retains an active Linux x86_64 native target"
require_active_values "$RELEASE" label "release.yml retains an active Linux x86_64 native label"
assert_only_active_values "$CI" label '^linux-x64$' \
  "ci.yml artifact label values are Linux x86_64 only"
assert_only_active_values "$RELEASE" label '^linux-x64-gnu$' \
  "release.yml native artifact label values are Linux x86_64 only"

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

# Matrix-runner regression fixture. The contract covers literal `runs-on`
# labels, `${{ matrix.runner }}` routes and their include values, CI artifact
# target/label values, and release native artifact target/label values. This
# fixture specifically proves that a macOS or Windows matrix runner cannot hide
# behind the otherwise-valid `runs-on: ${{ matrix.runner }}` expression.
if [ "${LINUX_FIRST_SCOPE_MATRIX_FIXTURE:-0}" != "1" ]; then
  MATRIX_RUNNER_FIXTURE="$SCRIPT_DIR/fixtures/linux_first_matrix_runner_macos_windows.yml"
  matrix_fixture_root="$(mktemp -d)"
  trap 'rm -rf "$matrix_fixture_root"' EXIT
  mkdir -p "$matrix_fixture_root/.github/workflows" \
    "$matrix_fixture_root/dev/plans/runs" \
    "$matrix_fixture_root/scripts/tests/fixtures"
  cp "$CI" "$matrix_fixture_root/.github/workflows/ci.yml"
  cp "$RELEASE" "$matrix_fixture_root/.github/workflows/release.yml"
  cp "$BRIEF" "$matrix_fixture_root/dev/plans/runs/"
  cp "$PLAN" "$matrix_fixture_root/dev/plans/"
  cp "$MASTER" "$matrix_fixture_root/dev/plans/"
  cp "$STATUS" "$matrix_fixture_root/dev/plans/runs/"
  cp "$0" "$matrix_fixture_root/scripts/tests/"
  cat "$MATRIX_RUNNER_FIXTURE" >> "$matrix_fixture_root/.github/workflows/ci.yml"

  if LINUX_FIRST_SCOPE_MATRIX_FIXTURE=1 \
    bash "$matrix_fixture_root/scripts/tests/test_linux_first_platform_scope.sh"; then
    fail "guard accepts macOS/Windows values routed through matrix.runner"
  else
    pass "guard rejects macOS/Windows values routed through matrix.runner"
  fi
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll Linux-first platform-scope tests passed\n'
