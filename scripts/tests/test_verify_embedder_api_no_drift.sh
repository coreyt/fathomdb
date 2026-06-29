#!/usr/bin/env bash
# scripts/tests/test_verify_embedder_api_no_drift.sh — offline coverage for
# the Axis-E published-API drift guard (scripts/release/verify-embedder-api-
# -no-drift.sh). Mirrors the fixture pattern in test_cargo_publish_if_new.sh:
# a python3 http server stands in for crates.io so no real registry is hit.
#
# Unlike the cargo-publish-if-new fixture, this guard fetches TWO routes
# (the version-list JSON and the /<version>/download tarball) whose paths
# collide on a plain static-file server, so we route them with a tiny
# BaseHTTPRequestHandler. The served tarballs are built at test time from
# the REAL working-tree embedder-api src (GREEN) and a code-mutated copy
# (RED), so the normalization + diff is exercised against actual source.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
GUARD="$REPO_ROOT/scripts/release/verify-embedder-api-no-drift.sh"
EMB_SRC="$REPO_ROOT/src/rust/crates/fathomdb-embedder-api/src"

# Sentinel version that can never collide with the live Axis-E version.
VER="9.9.9-fixture"

FAILED=0
WORK="$(mktemp -d)"
SERVE="$WORK/serve"
PID=0
PORT=0

cleanup() {
  if [ "$PID" -ne 0 ]; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

# Build a minimal .crate tarball (gzip tar with the cargo top-level dir
# layout: fathomdb-embedder-api-<ver>/src/...) from a src directory.
build_crate() {
  local srcdir="$1" out="$2"
  local stage pkg
  stage="$(mktemp -d)"
  pkg="$stage/fathomdb-embedder-api-$VER"
  mkdir -p "$pkg"
  cp -r "$srcdir" "$pkg/src"
  tar -czf "$out" -C "$stage" "fathomdb-embedder-api-$VER"
  rm -rf "$stage"
}

mkdir -p "$SERVE"

# GREEN tarball: byte-for-byte the real working-tree src.
build_crate "$EMB_SRC" "$SERVE/match.crate"

# RED tarball: a code-level mutation (rename a public method) so the
# normalized surface differs from the working tree — stands in for the
# v0.8.9 case where the published surface lacked a method the tree had.
MUT_SRC="$WORK/mut-src"
cp -r "$EMB_SRC" "$MUT_SRC"
sed -i 's/fn embed(/fn embed_renamed(/' "$MUT_SRC/lib.rs"
build_crate "$MUT_SRC" "$SERVE/drift.crate"

# Versions-list JSON: VER is present (so the guard proceeds to the diff).
cat >"$SERVE/versions-present.json" <<JSON
{"crate":{"name":"fathomdb-embedder-api"},"versions":[{"num":"$VER","yanked":false},{"num":"0.6.0","yanked":false}]}
JSON
# Versions-list JSON: VER absent (legitimate new unpublished version).
cat >"$SERVE/versions-absent.json" <<'JSON'
{"crate":{"name":"fathomdb-embedder-api"},"versions":[{"num":"0.6.0","yanked":false}]}
JSON

# Router. MODE (env) selects which fixtures to serve:
#   match   -> versions-present + GREEN tarball
#   drift   -> versions-present + RED tarball
#   absent  -> versions-absent  (download never reached)
#   malformed -> non-JSON body for the versions list (fail-closed exit 2)
cat >"$WORK/router.py" <<'PY'
import os, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

SERVE = sys.argv[1]
MODE = os.environ.get("MODE", "match")
CRATE = "fathomdb-embedder-api"

def read(name):
    with open(os.path.join(SERVE, name), "rb") as fh:
        return fh.read()

class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass
    def do_GET(self):
        path = self.path
        if path == f"/api/v1/crates/{CRATE}":
            if MODE == "malformed":
                body = b"<html>not json</html>"
            elif MODE == "absent":
                body = read("versions-absent.json")
            else:
                body = read("versions-present.json")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if path.endswith("/download"):
            tarball = "drift.crate" if MODE == "drift" else "match.crate"
            body = read(tarball)
            self.send_response(200)
            self.send_header("Content-Type", "application/x-tar")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_response(404)
        self.end_headers()

srv = HTTPServer(("127.0.0.1", 0), H)
sys.stderr.write("PORT=%d\n" % srv.server_address[1])
sys.stderr.flush()
srv.serve_forever()
PY

start_server() {
  MODE="$1" python3 -u "$WORK/router.py" "$SERVE" 2>"$WORK/server.log" &
  PID=$!
  PORT=0
  for _ in $(seq 1 100); do
    if grep -qE '^PORT=[0-9]+' "$WORK/server.log" 2>/dev/null; then
      PORT="$(sed -nE 's/^PORT=([0-9]+).*/\1/p' "$WORK/server.log" | head -1)"
      break
    fi
    sleep 0.05
  done
  [ "$PORT" -ne 0 ] || { fail "fixture http server failed to bind"; return 1; }
}

stop_server() {
  if [ "$PID" -ne 0 ]; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
    PID=0
  fi
}

run_guard() {
  EMB_API_DRIFT_REGISTRY="http://127.0.0.1:$PORT" \
  EMB_API_DRIFT_LOCAL_VERSION="$VER" \
    bash "$GUARD" >"$WORK/out.log" 2>&1
}

# --- Test 1: published version, surface MATCHES → exit 0 --------------------
start_server match || true
if run_guard; then
  pass "no-drift: matching published surface → exit 0"
else
  fail "no-drift: expected exit 0, got $? — $(cat "$WORK/out.log")"
fi
stop_server

# --- Test 2: published version, surface DRIFTS → exit 1 + actionable msg ----
start_server drift || true
set +e
run_guard
rc=$?
set -e
if [ "$rc" -eq 1 ] && grep -q "AXIS-E DRIFT DETECTED" "$WORK/out.log"; then
  pass "drift: mismatched published surface → exit 1 with guidance"
else
  fail "drift: expected exit 1 + 'AXIS-E DRIFT DETECTED', got $rc — $(cat "$WORK/out.log")"
fi
stop_server

# --- Test 3: version NOT yet published → exit 0 (new Axis-E version) ---------
start_server absent || true
if run_guard && grep -q "not yet on the registry" "$WORK/out.log"; then
  pass "new-version: unpublished version → exit 0 (verify-compiled at publish)"
else
  fail "new-version: expected exit 0 + 'not yet on the registry' — $(cat "$WORK/out.log")"
fi
stop_server

# --- Test 4: malformed registry body → fail-closed exit 2 (never skip-pass) -
start_server malformed || true
set +e
run_guard
rc=$?
set -e
if [ "$rc" -eq 2 ]; then
  pass "fail-closed: malformed registry JSON → exit 2 (no silent pass)"
else
  fail "fail-closed: expected exit 2, got $rc — $(cat "$WORK/out.log")"
fi
stop_server

# --- Test 5: registry unreachable → fail-closed (non-zero, never pass) -------
# Point at a closed port; curl --fail errors, guard must NOT pass.
set +e
EMB_API_DRIFT_REGISTRY="http://127.0.0.1:1" \
EMB_API_DRIFT_LOCAL_VERSION="$VER" \
  bash "$GUARD" >"$WORK/out.log" 2>&1
rc=$?
set -e
if [ "$rc" -ne 0 ]; then
  pass "fail-closed: unreachable registry → non-zero exit ($rc)"
else
  fail "fail-closed: unreachable registry must not pass — got exit 0"
fi

if [ "$FAILED" -ne 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nall tests passed\n'
