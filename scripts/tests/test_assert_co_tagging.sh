#!/usr/bin/env bash
# scripts/tests/test_assert_co_tagging.sh — coverage for the sibling-package
# co-tagging assert per AC-052. Runs offline against a python3 -m http.server
# fixture so the test never hits crates.io.
#
# Verifies the script asserts that, for a given Axis-W version, all three
# sibling crates (fathomdb, fathomdb-embedder at Axis W; fathomdb-embedder-api
# at Axis E) exist in the registry.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ASSERT="$REPO_ROOT/scripts/release/assert-co-tagging.sh"
FIX="$REPO_ROOT/dev/release/fixtures/co-tagging"

FAILED=0
SERVE_DIR="$(mktemp -d)"
PORT=0
PID=0

cleanup() {
  if [ "$PID" -ne 0 ]; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
  fi
  rm -rf "$SERVE_DIR"
}
trap cleanup EXIT

pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

# Mirror the crates.io URL shape: /api/v1/crates/<name>. http.server will
# serve these paths as regular files when no directory of that name exists.
mkdir -p "$SERVE_DIR/api/v1/crates"

start_server() {
  local layout="$1"  # 'ok' or 'missing-embedder'
  cp "$FIX/fathomdb-ok.json"               "$SERVE_DIR/api/v1/crates/fathomdb"
  cp "$FIX/fathomdb-embedder-api-ok.json"  "$SERVE_DIR/api/v1/crates/fathomdb-embedder-api"
  case "$layout" in
    ok)               cp "$FIX/fathomdb-embedder-ok.json"      "$SERVE_DIR/api/v1/crates/fathomdb-embedder" ;;
    missing-embedder) cp "$FIX/fathomdb-embedder-missing.json" "$SERVE_DIR/api/v1/crates/fathomdb-embedder" ;;
  esac
  ( cd "$SERVE_DIR" && python3 -u -m http.server 0 ) >"$SERVE_DIR/server.log" 2>&1 &
  PID=$!
  # Wait for the server to print its bound port.
  for _ in $(seq 1 50); do
    if grep -qE 'Serving HTTP on .* port [0-9]+' "$SERVE_DIR/server.log" 2>/dev/null; then
      PORT="$(sed -nE 's/.*port ([0-9]+).*/\1/p' "$SERVE_DIR/server.log" | head -1)"
      break
    fi
    sleep 0.05
  done
  if [ "$PORT" -eq 0 ]; then
    fail "fixture http server failed to bind"
    return 1
  fi
}

stop_server() {
  if [ "$PID" -ne 0 ]; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
    PID=0
  fi
  # Reset served files between fixture layouts.
  rm -rf "$SERVE_DIR/api"
  mkdir -p "$SERVE_DIR/api/v1/crates"
}

# Regression guard: script must send a User-Agent header (crates.io
# returns HTTP 403 without one).
grep -q 'User-Agent:' "$ASSERT" \
  && pass "assert-co-tagging.sh sets User-Agent for crates.io" \
  || fail "assert-co-tagging.sh missing User-Agent header"

# Positive case: all three crates present at 0.6.0 (Axis W) /
# 0.6.0-rc.1 (Axis E read from src/rust/crates/fathomdb-embedder-api/Cargo.toml).
start_server ok
if ASSERT_CO_TAGGING_REGISTRY="http://127.0.0.1:${PORT}" "$ASSERT" 0.6.0 >/dev/null 2>&1; then
  pass "all three sibling crates present at 0.6.0 → pass"
else
  fail "all-present case should succeed"
fi
stop_server

# Negative case: fathomdb-embedder is missing 0.6.0 → fail with
# structured `co-tagging-violation:` diagnostic naming the package.
start_server missing-embedder
if out="$(ASSERT_CO_TAGGING_REGISTRY="http://127.0.0.1:${PORT}" "$ASSERT" 0.6.0 2>&1)"; then
  fail "missing-embedder case should fail"
else
  printf '%s' "$out" | grep -q 'co-tagging-violation:' \
    && printf '%s' "$out" | grep -q 'fathomdb-embedder' \
    && pass "missing-embedder produces structured co-tagging-violation diagnostic" \
    || fail "wrong diagnostic for missing-embedder; got: $out"
fi
stop_server

# Bad version argument → exit non-zero with usage-like diagnostic.
if out="$("$ASSERT" not-a-semver 2>&1)"; then
  fail "non-semver version should be rejected"
else
  printf '%s' "$out" | grep -qiE 'usage|invalid|version' \
    && pass "non-semver version rejected" \
    || fail "wrong diagnostic for non-semver; got: $out"
fi

# Missing argument → exit non-zero with usage.
if out="$("$ASSERT" 2>&1)"; then
  fail "missing version arg should fail"
else
  printf '%s' "$out" | grep -qi 'usage' \
    && pass "missing version arg → usage" \
    || fail "wrong diagnostic for missing arg; got: $out"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll assert-co-tagging tests passed\n'
