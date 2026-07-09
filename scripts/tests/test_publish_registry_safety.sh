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
# The same split-brain exists for cargo (Fix-2): cargo-publish-if-new.sh queries
# CARGO_PUBLISH_IF_NEW_REGISTRY, but `cargo publish` has no URL knob and defaults
# to prod crates.io. The fix mirrors npm/PyPI: when the query is overridden the
# publish must EITHER route to a mapped alt-registry (--registry <name>) OR fail
# closed — it may NEVER run a default-crates.io `cargo publish` for a redirected
# query.
#
# These are RED-first assertions: they FAIL against the pre-Fix-1/Fix-2 helpers
# (no routing flags) and PASS only when the publish command targets the SAME host
# that was queried (or fails closed). Recording shims are used ON PURPOSE here —
# the property under test is the argv routing of the publish command, not
# idempotency (the real publish->install round-trip is in
# test_idempotent_republish.sh).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
NPM_HELPER="$REPO_ROOT/scripts/release/npm-publish-if-new.sh"
PYPI_HELPER="$REPO_ROOT/scripts/release/pypi-publish-if-new.sh"
CARGO_HELPER="$REPO_ROOT/scripts/release/cargo-publish-if-new.sh"
CARGO_CRATE="fathomdb-safety-crate"

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
# crates.io index API for an absent version: valid JSON whose versions[] omits
# the target, so cargo-publish-if-new's query returns "absent" and proceeds.
mkdir -p "$SERVE_DIR/api/v1/crates"
printf '{"versions":[]}' >"$SERVE_DIR/api/v1/crates/$CARGO_CRATE"

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
for tool in npm twine cargo; do
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

# --- cargo: overridden query + mapped publish-registry routes via --registry --
# CARGO_PUBLISH_IF_NEW_LOCAL_VERSION skips the manifest read; the fixture server
# answers the version query as "absent" so the helper proceeds to publish.
: >"$TOOL_LOG"
CARGO_PUBLISH_IF_NEW_REGISTRY="$STAGING" \
  CARGO_PUBLISH_IF_NEW_PUBLISH_REGISTRY="staging-alt" \
  CARGO_PUBLISH_IF_NEW_LOCAL_VERSION="9.9.9" \
  CARGO_REGISTRY_TOKEN="fake-token" \
  PATH="$SHIM_DIR:$PATH" "$CARGO_HELPER" "$CARGO_CRATE" >/dev/null 2>&1 || true
argv="$(cat "$TOOL_LOG")"
# The publish must carry --registry staging-alt (the mapped alt-registry). A bare
# `cargo publish` with no --registry (the pre-Fix-2 behaviour) has no such token
# and fails this assertion — it would default to prod crates.io.
if printf '%s' "$argv" | grep -q -- 'publish' \
   && printf '%s' "$argv" | grep -q -- '--registry staging-alt'; then
  pass "cargo publish routes to mapped alt-registry (staging-alt), not default crates.io"
else
  fail "cargo publish did NOT route to mapped alt-registry; argv='$argv'"
fi

# --- cargo: overridden query + NO publish-registry map → FAIL CLOSED ----------
# The query is redirected but no safe publish target is provided: the helper must
# exit non-zero and NEVER invoke `cargo publish` (which would default to prod).
: >"$TOOL_LOG"
if CARGO_PUBLISH_IF_NEW_REGISTRY="$STAGING" \
     CARGO_PUBLISH_IF_NEW_LOCAL_VERSION="9.9.9" \
     CARGO_REGISTRY_TOKEN="fake-token" \
     PATH="$SHIM_DIR:$PATH" "$CARGO_HELPER" "$CARGO_CRATE" >/dev/null 2>&1; then
  helper_rc=0
else
  helper_rc=$?
fi
argv="$(cat "$TOOL_LOG")"
if [ "$helper_rc" -ne 0 ] && ! printf '%s' "$argv" | grep -q -- 'publish'; then
  pass "cargo fails closed (rc=$helper_rc, no cargo publish) when query is redirected without a publish-registry map"
else
  fail "cargo did NOT fail closed on redirected query without a map; rc=$helper_rc argv='$argv'"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d safety test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll publish-registry-safety tests passed\n'
