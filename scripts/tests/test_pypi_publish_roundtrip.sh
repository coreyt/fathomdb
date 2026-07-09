#!/usr/bin/env bash
# scripts/tests/test_pypi_publish_roundtrip.sh — R-REL-4b/4c REAL PyPI round-trip.
#
# A genuine twine upload -> index-sees-it -> re-run-no-op cycle against a REAL
# local index (NOT argv-recording shims). What is real here:
#   • a minimal but faithful PyPI index (stdlib http.server) implements the
#     legacy upload API twine POSTs to AND the JSON API the helper queries;
#   • `twine` REALLY uploads a REAL sdist over HTTP through the actual
#     pypi-publish-if-new.sh helper (with Fix-1's --repository-url routing);
#   • the re-run NO-OPS via the helper's query-sees-it path (twine NOT
#     re-invoked — asserted by a server-side upload counter, so not vacuous).
#
# twine dependency: twine 6.x refuses --skip-existing against a non-prod
# repository, so the helper's first (real) upload needs twine<6. This test uses
# an existing twine<6 if present, else self-provisions one in a throwaway venv.
# If neither a compatible twine nor pip/venv is available (e.g. a fully offline
# CI), the test SKIPS LOUDLY (exit 0) rather than pretending — the residual is
# reported to the orchestrator. In CI the LIVE PyPI leg publishes via
# pypa/gh-action-pypi-publish (OIDC), which this script's idempotency mirrors.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PYPI_HELPER="$REPO_ROOT/scripts/release/pypi-publish-if-new.sh"

TMP="$(mktemp -d)"
SRV_PID=0
cleanup() {
  if [ "$SRV_PID" -ne 0 ]; then kill "$SRV_PID" 2>/dev/null || true; wait "$SRV_PID" 2>/dev/null || true; fi
  rm -rf "$TMP"
}
trap cleanup EXIT
pass() { printf 'PASS  %s\n' "$1"; }
skip() { printf 'SKIP  %s\n' "$1"; }
FAILED=0
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

twine_major_lt6() {  # $1 = twine binary
  local v
  v="$("$1" --version 2>/dev/null | sed -nE 's/.*twine version ([0-9]+).*/\1/p' | head -1)"
  [ -n "$v" ] && [ "$v" -lt 6 ]
}

# --- locate or provision a twine<6 ----------------------------------------
TWINE=""
if command -v twine >/dev/null 2>&1 && twine_major_lt6 twine; then
  TWINE="$(command -v twine)"
else
  if command -v python3 >/dev/null 2>&1 && python3 -m venv "$TMP/venv" >/dev/null 2>&1; then
    if "$TMP/venv/bin/pip" install -q 'twine<6' >"$TMP/pip.log" 2>&1 \
       && twine_major_lt6 "$TMP/venv/bin/twine"; then
      TWINE="$TMP/venv/bin/twine"
    fi
  fi
fi
if [ -z "$TWINE" ]; then
  skip "no twine<6 available and could not provision one (pip/venv/network) — PyPI REAL round-trip NOT exercised in this environment (documented residual; LIVE path is pypa/gh-action-pypi-publish)"
  exit 0
fi

# --- minimal REAL PyPI index ----------------------------------------------
cat >"$TMP/minipypi.py" <<'PY'
import sys, cgi, io, os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
STORE = set()  # (name, version)
POST_LOG = os.environ.get("POST_LOG")
class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        parts = self.path.strip("/").split("/")
        if len(parts) == 4 and parts[0] == "pypi" and parts[3] == "json":
            name, ver = parts[1].lower().replace("_", "-"), parts[2]
            ok = (name, ver) in STORE
            self.send_response(200 if ok else 404)
            self.send_header("content-type", "application/json"); self.end_headers()
            self.wfile.write(b'{"info":{}}' if ok else b'{}')
            return
        self.send_response(404); self.end_headers(); self.wfile.write(b'{}')
    def do_POST(self):
        ln = int(self.headers.get("content-length", 0))
        body = self.rfile.read(ln)
        fs = cgi.FieldStorage(fp=io.BytesIO(body), headers=self.headers,
            environ={"REQUEST_METHOD": "POST",
                     "CONTENT_TYPE": self.headers.get("content-type", ""),
                     "CONTENT_LENGTH": str(ln)})
        name = (fs.getvalue("name") or "").lower().replace("_", "-")
        ver = fs.getvalue("version") or ""
        if POST_LOG:
            with open(POST_LOG, "a") as fh: fh.write("%s %s\n" % (name, ver))
        if (name, ver) in STORE:
            self.send_response(409); self.end_headers(); self.wfile.write(b"exists"); return
        STORE.add((name, ver))
        self.send_response(200); self.end_headers(); self.wfile.write(b"ok")
if __name__ == "__main__":
    srv = ThreadingHTTPServer(("127.0.0.1", 0), H)
    sys.stdout.write("PORT=%d\n" % srv.server_address[1]); sys.stdout.flush()
    srv.serve_forever()
PY

export POST_LOG="$TMP/post.log"; : >"$POST_LOG"
python3 "$TMP/minipypi.py" >"$TMP/srv.log" 2>&1 &
SRV_PID=$!
PORT=""
for _ in $(seq 1 60); do
  PORT="$(sed -nE 's/PORT=([0-9]+)/\1/p' "$TMP/srv.log" | head -1)"
  [ -n "$PORT" ] && break
  sleep 0.05
done
[ -n "$PORT" ] || { fail "minimal PyPI index failed to bind"; exit 1; }
REG="http://127.0.0.1:${PORT}"

# --- build a REAL minimal sdist -------------------------------------------
PKGROOT="fathomdb-9.9.9"; mkdir -p "$TMP/build/$PKGROOT" "$TMP/dist"
printf 'Metadata-Version: 2.1\nName: fathomdb\nVersion: 9.9.9\nSummary: s\n' \
  >"$TMP/build/$PKGROOT/PKG-INFO"
tar -C "$TMP/build" -czf "$TMP/dist/${PKGROOT}.tar.gz" "$PKGROOT"

export TWINE_USERNAME=x TWINE_PASSWORD=x

# --- run #1: absent -> REAL twine upload -----------------------------------
if out="$(PYPI_PUBLISH_IF_NEW_REGISTRY="$REG" PYPI_PUBLISH_IF_NEW_UPLOAD_URL="$REG/" \
          TWINE_BIN="$TWINE" "$PYPI_HELPER" fathomdb 9.9.9 "$TMP/dist" 2>&1)"; then
  status="$(curl -s -o /dev/null -w '%{http_code}' "$REG/pypi/fathomdb/9.9.9/json")"
  if printf '%s' "$out" | grep -q "uploading to $REG/" \
     && [ "$status" = "200" ] && [ "$(wc -l <"$POST_LOG")" -ge 1 ]; then
    pass "PyPI REAL upload through helper (real twine POST landed; index now 200)"
  else
    fail "PyPI upload: out='$out' json_status=$status posts=$(cat "$POST_LOG")"
  fi
else
  fail "PyPI helper (upload) exited non-zero: $out"
fi

# --- run #2: present -> query-sees-it -> skip, twine NOT re-invoked ---------
POSTS_BEFORE="$(wc -l <"$POST_LOG")"
if out="$(PYPI_PUBLISH_IF_NEW_REGISTRY="$REG" PYPI_PUBLISH_IF_NEW_UPLOAD_URL="$REG/" \
          TWINE_BIN="$TWINE" "$PYPI_HELPER" fathomdb 9.9.9 "$TMP/dist" 2>&1)"; then
  POSTS_AFTER="$(wc -l <"$POST_LOG")"
  if printf '%s' "$out" | grep -q 'already released; skipping' \
     && [ "$POSTS_AFTER" -eq "$POSTS_BEFORE" ]; then
    pass "PyPI re-run NO-OPS (query-sees-it; zero new uploads -> not vacuous)"
  else
    fail "PyPI no-op: out='$out' posts_before=$POSTS_BEFORE posts_after=$POSTS_AFTER"
  fi
else
  fail "PyPI helper (no-op) exited non-zero: $out"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d PyPI round-trip test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll PyPI REAL round-trip tests passed\n'
