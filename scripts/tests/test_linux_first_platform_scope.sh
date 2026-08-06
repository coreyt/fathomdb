#!/usr/bin/env bash
# Static contract guard for 0.8.22's five-target stable native scope. The
# filename is retained because agent-test.sh registers it; the assertions are
# deliberately no longer Linux-first. actionlint remains YAML's authority.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
CI="$REPO_ROOT/.github/workflows/ci.yml"
RELEASE="$REPO_ROOT/.github/workflows/release.yml"
PLAN="$REPO_ROOT/dev/plans/plan-0.8.22.md"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

job_block() {
  awk -v job="$2" '
    $0 == "  " job ":" { in_job = 1 }
    in_job && /^  [[:alnum:]_-]+:$/ && $0 != "  " job ":" { exit }
    in_job { print }
  ' "$1"
}

matrix_rows() {
  local rows="$1" with_label="$2"
  awk -v with_label="$with_label" '
    /^[[:space:]]*-[[:space:]]+runner:[[:space:]]+/ {
      runner = $3; target = ""; label = ""; next
    }
    /^[[:space:]]+target:[[:space:]]+/ { target = $2 }
    /^[[:space:]]+label:[[:space:]]+/ {
      label = $2
      if (with_label && runner != "" && target != "") {
        print runner "|" target "|" label
      }
      next
    }
    !with_label && target != "" && runner != "" {
      print runner "|" target
      runner = ""; target = ""
    }
  ' <<<"$rows"
}

assert_exact_rows() {
  local actual="$1" expected="$2" description="$3"
  local got want
  got="$(printf '%s\n' "$actual" | sed '/^$/d' | sort -u)"
  want="$(printf '%s\n' "$expected" | sed '/^$/d' | sort -u)"
  if [ "$got" = "$want" ]; then
    pass "$description"
  else
    printf 'FAIL  %s\nexpected:\n%s\nactual:\n%s\n' "$description" "$want" "$got" >&2
    FAILED=$((FAILED + 1))
  fi
}

assert_job_runner() {
  local job="$1" runner="$2" description="$3" block
  block="$(job_block "$RELEASE" "$job")"
  if grep -Fqx "    runs-on: $runner" <<<"$block"; then
    pass "$description"
  else
    fail "$description"
  fi
}

assert_matrix_runner_route() {
  local workflow="$1" job="$2" description="$3" block
  block="$(job_block "$workflow" "$job")"
  if grep -Fqx '    runs-on: ${{ matrix.runner }}' <<<"$block"; then
    pass "$description"
  else
    fail "$description"
  fi
}

five_python_rows=$'ubuntu-latest|x86_64-unknown-linux-gnu\nubuntu-24.04-arm|aarch64-unknown-linux-gnu\nmacos-15-intel|x86_64-apple-darwin\nmacos-14|aarch64-apple-darwin\nwindows-latest|x86_64-pc-windows-msvc'
ci_wheel_rows=$'ubuntu-latest|x86_64-unknown-linux-gnu|linux-x64\nubuntu-24.04-arm|aarch64-unknown-linux-gnu|linux-arm64\nmacos-15-intel|x86_64-apple-darwin|darwin-x64\nmacos-14|aarch64-apple-darwin|darwin-arm64\nwindows-latest|x86_64-pc-windows-msvc|win32-x64'
release_napi_rows=$'ubuntu-latest|x86_64-unknown-linux-gnu|linux-x64-gnu\nubuntu-24.04-arm|aarch64-unknown-linux-gnu|linux-arm64-gnu\nmacos-15-intel|x86_64-apple-darwin|darwin-x64\nmacos-14|aarch64-apple-darwin|darwin-arm64\nwindows-latest|x86_64-pc-windows-msvc|win32-x64-msvc'

ci_wheel_block="$(job_block "$CI" wheel-size-gate)"
release_python_block="$(job_block "$RELEASE" build-python)"
release_napi_block="$(job_block "$RELEASE" build-napi)"
ci_wheel_actual="$(matrix_rows "$ci_wheel_block" 1)"
release_python_actual="$(matrix_rows "$release_python_block" 0)"
release_napi_actual="$(matrix_rows "$release_napi_block" 1)"

assert_matrix_runner_route "$CI" wheel-size-gate \
  "ci.yml wheel-size matrix runs on each selected target runner"
assert_matrix_runner_route "$RELEASE" build-python \
  "release.yml Python matrix runs on each selected target runner"
assert_matrix_runner_route "$RELEASE" build-napi \
  "release.yml N-API matrix runs on each selected target runner"
assert_exact_rows "$ci_wheel_actual" "$ci_wheel_rows" \
  "ci.yml wheel-size gate covers exactly the five supported actual runners"
assert_exact_rows "$release_python_actual" "$five_python_rows" \
  "release.yml Python build covers exactly the five supported actual runners"
assert_exact_rows "$release_napi_actual" "$release_napi_rows" \
  "release.yml N-API build covers exactly the five supported actual runners and labels"

for mapping in \
  'publish-npm-platform-linux-x64-gnu:ubuntu-latest' \
  'publish-npm-platform-linux-arm64-gnu:ubuntu-24.04-arm' \
  'publish-npm-platform-darwin-x64:macos-15-intel' \
  'publish-npm-platform-darwin-arm64:macos-14' \
  'publish-npm-platform-win32-x64-msvc:windows-latest' \
  'post-publish-smoke:ubuntu-latest' \
  'post-publish-smoke-aarch64:ubuntu-24.04-arm' \
  'post-publish-smoke-darwin-x64:macos-15-intel' \
  'post-publish-smoke-darwin-arm64:macos-14' \
  'post-publish-smoke-win32-x64:windows-latest'; do
  job="${mapping%%:*}"
  runner="${mapping#*:}"
  assert_job_runner "$job" "$runner" "$job runs on its supported target runner"
done

if rg -q 'unknown-linux-musl|apple-ios|aarch64-pc-windows|i686-pc-windows|win32-arm64' "$CI" "$RELEASE"; then
  fail "CI or release workflow declares an unsupported musl or platform target"
else
  pass "CI and release workflow exclude musl and other unsupported target triples"
fi

promotion_block="$(job_block "$RELEASE" promote-npm-latest)"
for required_need in \
  post-publish-smoke \
  post-publish-smoke-aarch64 \
  post-publish-smoke-darwin-x64 \
  post-publish-smoke-darwin-arm64 \
  post-publish-smoke-win32-x64 \
  co-tagging-assert; do
  if ! grep -Fqx "      - $required_need" <<<"$promotion_block"; then
    fail "latest promotion waits for $required_need"
  fi
done
if grep -Fqx '        run: npm dist-tag add "fathomdb@${RELEASE_TAG#v}" latest' <<<"$promotion_block"; then
  pass "only the main fathomdb package is promoted after all five smokes"
else
  fail "promotion must add latest only to the main fathomdb package"
fi

if grep -Fqx '  NPM_DIST_TAG: "next"' "$RELEASE" \
  && grep -q 'Linux musl, Windows ARM/32-bit' "$PLAN"; then
  pass "platform packages publish under next and the plan keeps unsupported targets explicit"
else
  fail "release truth must retain next-first publication and explicit unsupported targets"
fi

# The runner route must reject a sixth unsupported matrix entry rather than
# merely accepting the three non-Linux runners now intentionally in scope.
if [ "${CROSS_PLATFORM_SCOPE_MATRIX_FIXTURE:-0}" != "1" ]; then
  fixture_root="$(mktemp -d)"
  trap 'rm -rf "$fixture_root"' EXIT
  mkdir -p "$fixture_root/.github/workflows" "$fixture_root/dev/plans" "$fixture_root/scripts/tests"
  cp "$CI" "$fixture_root/.github/workflows/ci.yml"
  cp "$RELEASE" "$fixture_root/.github/workflows/release.yml"
  cp "$PLAN" "$fixture_root/dev/plans/plan-0.8.22.md"
  cp "$0" "$fixture_root/scripts/tests/"
  cat >> "$fixture_root/.github/workflows/ci.yml" <<'EOF'
          - runner: ubuntu-latest
            target: x86_64-unknown-linux-musl
            label: linux-x64-musl
EOF
  if CROSS_PLATFORM_SCOPE_MATRIX_FIXTURE=1 \
    bash "$fixture_root/scripts/tests/test_linux_first_platform_scope.sh"; then
    fail "five-target guard accepts an unsupported sixth matrix row"
  else
    pass "five-target guard rejects an unsupported sixth matrix row"
  fi
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll five-target platform-scope tests passed\n'
