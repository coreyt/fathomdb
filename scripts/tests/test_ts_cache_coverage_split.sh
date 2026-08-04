#!/usr/bin/env bash
# Static contract for the cache-aware TypeScript test split.  The ordinary
# agent loop must remain local/preworkable by setting the network skip gate;
# the default-embedder CI job owns the same seven network-gated files after it
# has warmed (or deliberately failed to warm) the BGE cache.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
AGENT_TEST="$REPO_ROOT/scripts/agent-test.sh"
CI="$REPO_ROOT/.github/workflows/ci.yml"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

# Line number (1-based, within $2) of the first line containing fixed string
# $1; EMPTY when there is genuinely no match.
#
# This replaces `grep -nF … | head -n1 | cut -d: -f1 || true`. That idiom has
# two defects and the `|| true` is the worse one: `head -n1` closes the pipe
# while grep is still scanning, so grep can be SIGPIPEd, and `|| true` then
# converts that crash into an EMPTY STRING — indistinguishable from "the CI job
# does not warm the cache". The ordering assertions downstream simply skip
# themselves on an empty value, so a broken pipe would report a silent
# non-result. A wrong answer is worse than an abort.
#
# Here `grep -m1` stops the producer itself (nothing exits early, so there is
# nothing to race) and the rc is read explicitly: 0 = match, 1 = no match, and
# anything else is a real failure that returns non-zero and, under `set -e`,
# aborts loudly instead of being laundered into "".
first_match_line_no() {
  local needle="$1" haystack="$2" hit rc
  set +e
  hit="$(grep -nF -m1 -e "$needle" <<<"$haystack")"
  rc=$?
  set -e
  case "$rc" in
    0) printf '%s' "${hit%%:*}" ;;
    1) : ;;
    *)
      printf 'test_ts_cache_coverage_split: grep failed (rc=%d) searching for `%s`\n' \
        "$rc" "$needle" >&2
      return "$rc"
      ;;
  esac
}

if [ ! -f "$AGENT_TEST" ] || [ ! -f "$CI" ]; then
  fail "agent-test.sh and ci.yml must exist"
  exit 1
fi

# These are every and only TypeScript test files whose bodies honor the shared
# network gate.  Keep the set explicit: a new network-gated file must be
# consciously routed through the dedicated cache-owning CI job too.
expected_gated_files=(
  src/ts/tests/embed-batch-cls.test.ts
  src/ts/tests/embedder-event-narrowing.test.ts
  src/ts/tests/functional-embed.test.ts
  src/ts/tests/release-surface.test.ts
  src/ts/tests/slice20c-flush-barrier.test.ts
  src/ts/tests/tc67-unsupported-vector-kind-report.test.ts
  src/ts/tests/use-default-embedder.test.ts
)
mapfile -t actual_gated_files < <(
  cd "$REPO_ROOT"
  find src/ts/tests -type f -name '*.test.ts' -exec grep -l 'FATHOMDB_SKIP_NETWORK_TESTS' {} + | sort
)
if [ "${actual_gated_files[*]}" = "${expected_gated_files[*]}" ]; then
  pass "exactly the seven audited TypeScript files honor the network gate"
else
  fail "network-gated TypeScript files drifted: expected [${expected_gated_files[*]}], got [${actual_gated_files[*]}]"
fi

# The generic agent loop owns all TypeScript tests, but must make the seven
# network-dependent arms skip rather than require a live model download.
if grep -qxE "[[:space:]]*run_suite test-ts env FATHOMDB_SKIP_NETWORK_TESTS=1 bash -c 'cd src/ts && npm test --silent'" "$AGENT_TEST"; then
  pass "generic agent-test routes TypeScript through the network skip gate"
else
  fail "generic test-ts must set FATHOMDB_SKIP_NETWORK_TESTS=1 exactly once before npm test"
fi
if grep -q 'RELEASE_SURFACE_TESTS' "$AGENT_TEST"; then
  fail "generic agent-test must not enable the dedicated release-surface suite"
else
  pass "generic agent-test leaves release-surface coverage to its dedicated job"
fi

default_embedder_block="$(awk '
  /^  default-embedder-tests:$/ { in_job = 1; next }
  in_job && /^  [[:alnum:]_-]+:$/ { exit }
  in_job { print }
' "$CI")"
if [ -z "$default_embedder_block" ]; then
  fail "ci.yml must retain default-embedder-tests"
else
  if grep -qE '^[[:space:]]*timeout-minutes:[[:space:]]*60[[:space:]]*$' <<<"$default_embedder_block"; then
    pass "default-embedder CI job has the cache plus full TypeScript budget"
  else
    fail "default-embedder CI job must allow 60 minutes"
  fi
  if grep -q 'actions/setup-node@48b55a011bda9f5d6aeb4c2d9c7362e8dae4041e' <<<"$default_embedder_block" && \
    grep -qE '^[[:space:]]*node-version: "25\.9\.0"[[:space:]]*$' <<<"$default_embedder_block"; then
    pass "default-embedder CI job pins Node 25.9.0"
  else
    fail "default-embedder CI job must set up pinned Node 25.9.0"
  fi
  if grep -qE '^[[:space:]]*run: cd src/ts && npm ci[[:space:]]*$' <<<"$default_embedder_block"; then
    pass "default-embedder CI job installs the locked TypeScript dependencies"
  else
    fail "default-embedder CI job must run npm ci in src/ts"
  fi
  if grep -qE '^[[:space:]]*run: cd src/ts && npm run build:native:debug[[:space:]]*$' <<<"$default_embedder_block"; then
    pass "default-embedder CI job builds the native debug binding"
  else
    fail "default-embedder CI job must run the native debug build"
  fi
  if grep -qE '^[[:space:]]*run: cd src/ts && npm exec -- tsc -p tsconfig.json[[:space:]]*$' <<<"$default_embedder_block"; then
    pass "default-embedder CI job emits the TypeScript test files"
  else
    fail "default-embedder CI job must emit TypeScript tests with tsc"
  fi

  expected_emitted_files=(
    dist/tests/embed-batch-cls.test.js
    dist/tests/embedder-event-narrowing.test.js
    dist/tests/functional-embed.test.js
    dist/tests/release-surface.test.js
    dist/tests/slice20c-flush-barrier.test.js
    dist/tests/tc67-unsupported-vector-kind-report.test.js
    dist/tests/use-default-embedder.test.js
  )
  mapfile -t actual_emitted_files < <(grep -oE 'dist/tests/[[:alnum:]_.-]+\.test\.js' <<<"$default_embedder_block" | sort -u)
  if [ "${actual_emitted_files[*]}" = "${expected_emitted_files[*]}" ]; then
    pass "default-embedder CI job runs exactly the seven audited emitted TypeScript files"
  else
    fail "dedicated emitted TypeScript files drifted: expected [${expected_emitted_files[*]}], got [${actual_emitted_files[*]}]"
  fi
  if grep -qE 'RELEASE_SURFACE_TESTS=1[[:space:]]+node[[:space:]]+--test' <<<"$default_embedder_block"; then
    pass "default-embedder CI job enables release-surface assertions"
  else
    fail "default-embedder CI job must run emitted tests with RELEASE_SURFACE_TESTS=1"
  fi
  if grep -qE '(^|[[:space:]])npm[[:space:]]+test([[:space:]]|$)' <<<"$default_embedder_block"; then
    fail "dedicated default-embedder job must not rerun the full npm test suite"
  else
    pass "default-embedder CI job does not rerun the generic full npm test suite"
  fi
  if grep -qE 'FATHOMDB_SKIP_NETWORK_TESTS=1[[:space:]]+(npm|bash)' <<<"$default_embedder_block"; then
    fail "dedicated default-embedder test command must not force network tests to skip"
  else
    pass "dedicated default-embedder test inherits only the warm-cache failure gate"
  fi

  warm_line="$(first_match_line_no 'Warm BGE embedder cache' "$default_embedder_block")"
  npm_ci_line="$(first_match_line_no 'npm ci' "$default_embedder_block")"
  native_build_line="$(first_match_line_no 'npm run build:native:debug' "$default_embedder_block")"
  tsc_line="$(first_match_line_no 'npm exec -- tsc -p tsconfig.json' "$default_embedder_block")"
  emitted_test_line="$(first_match_line_no 'RELEASE_SURFACE_TESTS=1 node --test' "$default_embedder_block")"
  if [ -n "$warm_line" ] && [ -n "$npm_ci_line" ] && [ -n "$native_build_line" ] && \
    [ -n "$tsc_line" ] && [ -n "$emitted_test_line" ] && \
    [ "$warm_line" -lt "$npm_ci_line" ] && [ "$npm_ci_line" -lt "$native_build_line" ] && \
    [ "$native_build_line" -lt "$tsc_line" ] && [ "$tsc_line" -lt "$emitted_test_line" ]; then
    pass "cache warm precedes npm ci, native build, tsc, and the targeted emitted TypeScript run"
  else
    fail "default-embedder ordering must be warm cache -> npm ci -> native build -> tsc -> targeted run"
  fi
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nTypeScript cache-coverage split test passed\n'
