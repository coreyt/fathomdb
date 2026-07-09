#!/usr/bin/env bash
# scripts/tests/test_publish_registry_safety.sh — Fix-1 SAFETY guard.
#
# A staging/test run (one that sets *_PUBLISH_IF_NEW_REGISTRY at a non-prod
# index) must NEVER be able to publish to the PUBLIC registry. Before Fix-1 the
# helpers queried the override registry but the actual publish command defaulted
# to prod:
#   • npm-publish-if-new.sh  invoked `npm publish` with NO --registry;
#   • pypi-publish-if-new.sh invoked `twine upload` with NO --repository-url.
# Either would ship a test/staging run to the real registry.
#
# These are RED-first assertions: they FAIL against the pre-Fix-1 helpers (no
# routing flags) and PASS only when the publish command targets the SAME host
# that was queried. Recording shims are used ON PURPOSE here — the property
# under test is the argv routing of the publish command, not idempotency (the
# real publish->install round-trip is in test_idempotent_republish.sh).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
NPM_HELPER="$REPO_ROOT/scripts/release/npm-publish-if-new.sh"
PYPI_HELPER="$REPO_ROOT/scripts/release/pypi-publish-if-new.sh"

FAILED=0
SERVE_DIR="$(mktemp -d)"
SHIM_DIR="$(mktemp -d)"
WORK_DIR="$(mktemp -d)"
PORT=0
PID=0
cleanup() {
  if [ "$PID" -ne 0 ]; then kill "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true; fi
  rm -rf "$SERVE_DIR" "$SHIM_DIR" "$WORK_DIR"
}
trap cleanup EXIT
pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

# npm packument for an absent version (valid JSON, no matching version) so the
# helper's query returns "absent" and proceeds to the publish command.
printf '{"name":"fathomdb-safety-pkg","versions":{}}' >"$SERVE_DIR/fathomdb-safety-pkg"
# PyPI JSON API for an absent version is simply a 404 (missing path).

( cd "$SERVE_DIR" && python3 -u -m http.server 0 ) >"$SERVE_DIR/server.log" 2>&1 &
PID=$!
for _ in $(seq 1 50); do
  if grep -qE 'Serving HTTP on .* port [0-9]+' "$SERVE_DIR/server.log" 2>/dev/null; then
    PORT="$(sed -nE 's/.*port ([0-9]+).*/\1/p' "$SERVE_DIR/server.log" | head -1)"; break
  fi
  sleep 0.05
done
[ "$PORT" -ne 0 ] || { fail "fixture http server failed to bind"; exit 1; }
STAGING="http://127.0.0.1:${PORT}"

export TOOL_LOG="$SHIM_DIR/tool.log"
for tool in npm twine; do
  cat >"$SHIM_DIR/$tool" <<SHIM
#!/usr/bin/env bash
printf '$tool %s\n' "\$*" >>"\$TOOL_LOG"
exit 0
SHIM
  chmod +x "$SHIM_DIR/$tool"
done
: >"$TOOL_LOG"

# --- npm: publish must target --registry $STAGING, never prod --------------
mkdir -p "$WORK_DIR/npm"
printf '{"name":"fathomdb-safety-pkg","version":"9.9.9"}' >"$WORK_DIR/npm/package.json"
: >"$TOOL_LOG"
( cd "$WORK_DIR/npm" && NPM_PUBLISH_IF_NEW_REGISTRY="$STAGING" NPM_BIN=npm \
    PATH="$SHIM_DIR:$PATH" "$NPM_HELPER" --tag next >/dev/null 2>&1 ) || true
argv="$(cat "$TOOL_LOG")"
if printf '%s' "$argv" | grep -q -- "--registry $STAGING" \
   && ! printf '%s' "$argv" | grep -q 'registry.npmjs.org'; then
  pass "npm publish routes to the queried registry ($STAGING), not prod"
else
  fail "npm publish did NOT target the override registry; argv='$argv'"
fi

# --- pypi: upload must target --repository-url derived from $STAGING --------
mkdir -p "$WORK_DIR/dist"
printf 'stub' >"$WORK_DIR/dist/fathomdb-9.9.9.tar.gz"
: >"$TOOL_LOG"
# Only the query registry is overridden (no explicit upload URL). The upload
# endpoint MUST be derived from the same host — never prod upload.pypi.org.
PYPI_PUBLISH_IF_NEW_REGISTRY="$STAGING" TWINE_BIN=twine PATH="$SHIM_DIR:$PATH" \
  "$PYPI_HELPER" fathomdb 9.9.9 "$WORK_DIR/dist" >/dev/null 2>&1 || true
argv="$(cat "$TOOL_LOG")"
if printf '%s' "$argv" | grep -q -- "--repository-url $STAGING" \
   && ! printf '%s' "$argv" | grep -q 'upload.pypi.org'; then
  pass "twine upload routes to the queried host ($STAGING), not prod upload.pypi.org"
else
  fail "twine upload did NOT target the override host; argv='$argv'"
fi

# --- pypi: an explicit upload URL is honoured ------------------------------
: >"$TOOL_LOG"
PYPI_PUBLISH_IF_NEW_REGISTRY="$STAGING" \
  PYPI_PUBLISH_IF_NEW_UPLOAD_URL="$STAGING/legacy/" TWINE_BIN=twine \
  PATH="$SHIM_DIR:$PATH" "$PYPI_HELPER" fathomdb 9.9.9 "$WORK_DIR/dist" >/dev/null 2>&1 || true
argv="$(cat "$TOOL_LOG")"
if printf '%s' "$argv" | grep -q -- "--repository-url $STAGING/legacy/"; then
  pass "twine upload honours an explicit PYPI_PUBLISH_IF_NEW_UPLOAD_URL"
else
  fail "twine upload ignored explicit upload URL; argv='$argv'"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d safety test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll publish-registry-safety tests passed\n'
