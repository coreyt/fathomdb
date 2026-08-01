#!/usr/bin/env bash
# Static contract for the cache-aware TypeScript test split.  The ordinary
# agent loop must remain local/preworkable by setting the network skip gate;
# the default-embedder CI job owns the same six network-gated files after it
# has warmed (or deliberately failed to warm) the BGE cache.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${REPO_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
AGENT_TEST="$REPO_ROOT/scripts/agent-test.sh"
CI="$REPO_ROOT/.github/workflows/ci.yml"

FAILED=0
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

if [ ! -f "$AGENT_TEST" ] || [ ! -f "$CI" ]; then
  fail "agent-test.sh and ci.yml must exist"
  exit 1
fi

# These are every and only TypeScript test files whose bodies honor the shared
# network gate.  Keep the set explicit: a new network-gated file must be
# consciously routed through the dedicated cache-owning CI job too.
expected_gated_files=(
  src/ts/tests/embedder-event-narrowing.test.ts
  src/ts/tests/functional-embed.test.ts
  src/ts/tests/release-surface.test.ts
  src/ts/tests/slice20c-flush-barrier.test.ts
  src/ts/tests/tc67-unsupported-vector-kind-report.test.ts
  src/ts/tests/use-default-embedder.test.ts
)
mapfile -t actual_gated_files < <(
  cd "$REPO_ROOT"
  rg -l 'FATHOMDB_SKIP_NETWORK_TESTS' src/ts/tests --glob '*.test.ts' | sort
)
if [ "${actual_gated_files[*]}" = "${expected_gated_files[*]}" ]; then
  pass "exactly the six audited TypeScript files honor the network gate"
else
  fail "network-gated TypeScript files drifted: expected [${expected_gated_files[*]}], got [${actual_gated_files[*]}]"
fi

# The generic agent loop owns all TypeScript tests, but must make the six
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
  if grep -q 'actions/setup-node@48b55a011bda9f5d6aeb4c2d9c7362e8dae4041' <<<"$default_embedder_block" && \
    grep -qE '^[[:space:]]*node-version: "22"[[:space:]]*$' <<<"$default_embedder_block"; then
    pass "default-embedder CI job pins Node 22"
  else
    fail "default-embedder CI job must set up pinned Node 22"
  fi
  if grep -qE '^[[:space:]]*run: cd src/ts && npm ci[[:space:]]*$' <<<"$default_embedder_block"; then
    pass "default-embedder CI job installs the locked TypeScript dependencies"
  else
    fail "default-embedder CI job must run npm ci in src/ts"
  fi
  if grep -qE "^[[:space:]]*run: cd src/ts && RELEASE_SURFACE_TESTS=1 npm test --silent[[:space:]]*$" <<<"$default_embedder_block"; then
    pass "default-embedder CI job runs the full release-surface TypeScript suite"
  else
    fail "default-embedder CI job must run RELEASE_SURFACE_TESTS=1 npm test --silent"
  fi
  if grep -qE 'FATHOMDB_SKIP_NETWORK_TESTS=1[[:space:]]+(npm|bash)' <<<"$default_embedder_block"; then
    fail "dedicated default-embedder test command must not force network tests to skip"
  else
    pass "dedicated default-embedder test inherits only the warm-cache failure gate"
  fi

  warm_line="$(grep -nF 'Warm BGE embedder cache' <<<"$default_embedder_block" | head -n1 | cut -d: -f1 || true)"
  npm_ci_line="$(grep -nF 'npm ci' <<<"$default_embedder_block" | head -n1 | cut -d: -f1 || true)"
  npm_test_line="$(grep -nF 'RELEASE_SURFACE_TESTS=1 npm test --silent' <<<"$default_embedder_block" | head -n1 | cut -d: -f1 || true)"
  if [ -n "$warm_line" ] && [ -n "$npm_ci_line" ] && [ -n "$npm_test_line" ] && \
    [ "$warm_line" -lt "$npm_ci_line" ] && [ "$npm_ci_line" -lt "$npm_test_line" ]; then
    pass "cache warm precedes npm ci, which precedes the dedicated full TypeScript run"
  else
    fail "default-embedder ordering must be warm cache -> npm ci -> full TypeScript run"
  fi
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nTypeScript cache-coverage split test passed\n'
