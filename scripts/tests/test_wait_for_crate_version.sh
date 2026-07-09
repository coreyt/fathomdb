#!/usr/bin/env bash
# scripts/tests/test_wait_for_crate_version.sh — R-REL-4c poll-for-resolvability.
# The tiered cargo publish replaced the fixed `sleep 60` index-propagation
# heuristic with this poll: it returns as soon as the just-published version is
# resolvable, and TIMES OUT loudly if propagation genuinely stalls. Hermetic:
# a python3 -m http.server fixture stands in for the crates.io index.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
POLL="$REPO_ROOT/scripts/release/wait-for-crate-version.sh"

FAILED=0
SERVE_DIR="$(mktemp -d)"
PORT=0
PID=0
cleanup() {
  if [ "$PID" -ne 0 ]; then kill "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true; fi
  rm -rf "$SERVE_DIR"
}
trap cleanup EXIT

pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

mkdir -p "$SERVE_DIR/api/v1/crates"
# Present crate at the workspace/axis versions under test.
cat >"$SERVE_DIR/api/v1/crates/fathomdb-schema" <<'JSON'
{"crate":{"name":"fathomdb-schema"},"versions":[{"num":"0.8.9","yanked":false}]}
JSON
# fathomdb-embedder-api present at its Axis-E version 0.6.1 (manifest-resolution).
cat >"$SERVE_DIR/api/v1/crates/fathomdb-embedder-api" <<'JSON'
{"crate":{"name":"fathomdb-embedder-api"},"versions":[{"num":"0.6.1","yanked":false}]}
JSON

( cd "$SERVE_DIR" && python3 -u -m http.server 0 ) >"$SERVE_DIR/server.log" 2>&1 &
PID=$!
for _ in $(seq 1 50); do
  if grep -qE 'Serving HTTP on .* port [0-9]+' "$SERVE_DIR/server.log" 2>/dev/null; then
    PORT="$(sed -nE 's/.*port ([0-9]+).*/\1/p' "$SERVE_DIR/server.log" | head -1)"
    break
  fi
  sleep 0.05
done
[ "$PORT" -ne 0 ] || { fail "fixture http server failed to bind"; exit 1; }
REG="http://127.0.0.1:${PORT}"

# 1) explicit version present -> exit 0 fast.
if out="$(WAIT_FOR_CRATE_REGISTRY="$REG" WAIT_FOR_CRATE_TIMEOUT=5 WAIT_FOR_CRATE_INTERVAL=1 \
          "$POLL" fathomdb-schema 0.8.9 2>&1)"; then
  printf '%s' "$out" | grep -q 'resolvable' && pass "present version resolves (exit 0)" \
    || fail "present-version: wrong output: $out"
else
  fail "present-version should exit 0: $out"
fi

# 2) absent version -> timeout exit 1 (short budget so the test is fast).
set +e
out="$(WAIT_FOR_CRATE_REGISTRY="$REG" WAIT_FOR_CRATE_TIMEOUT=1 WAIT_FOR_CRATE_INTERVAL=1 \
       "$POLL" fathomdb-schema 9.9.9 2>&1)"
rc=$?
set -e
if [ "$rc" -eq 1 ] && printf '%s' "$out" | grep -qi 'timeout'; then
  pass "absent version times out loudly (exit 1)"
else
  fail "absent-version: expected exit 1 + TIMEOUT; got rc=$rc out='$out'"
fi

# 3) manifest version resolution: omit version -> reads Axis-E 0.6.1 from the
#    fathomdb-embedder-api manifest and finds it present.
if out="$(WAIT_FOR_CRATE_REGISTRY="$REG" WAIT_FOR_CRATE_TIMEOUT=5 WAIT_FOR_CRATE_INTERVAL=1 \
          "$POLL" fathomdb-embedder-api 2>&1)"; then
  printf '%s' "$out" | grep -q '0.6.1' && pass "resolves version from manifest (Axis-E 0.6.1)" \
    || fail "manifest-resolution: wrong output: $out"
else
  fail "manifest-resolution should exit 0: $out"
fi

# 4) usage error on no args.
if "$POLL" >/dev/null 2>&1; then
  fail "no-args should exit non-zero"
else
  pass "no args -> usage exit"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll wait-for-crate-version tests passed\n'
