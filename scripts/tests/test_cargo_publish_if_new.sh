#!/usr/bin/env bash
# scripts/tests/test_cargo_publish_if_new.sh — offline coverage for the
# idempotent cargo publish helper (release.yml T1..T7). Mirrors the fixture
# pattern in test_assert_co_tagging.sh: python3 -m http.server serves a
# fake crates.io JSON index, and `cargo` is a shim in PATH so no real
# registry or toolchain work happens.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HELPER="$REPO_ROOT/scripts/release/cargo-publish-if-new.sh"
FIX_DIR="$REPO_ROOT/dev/release/fixtures/cargo-publish-if-new"

FAILED=0
SERVE_DIR="$(mktemp -d)"
SHIM_DIR="$(mktemp -d)"
export CARGO_LOG="$SHIM_DIR/cargo.log"
PORT=0
PID=0

cleanup() {
  if [ "$PID" -ne 0 ]; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
  fi
  rm -rf "$SERVE_DIR" "$SHIM_DIR"
}
trap cleanup EXIT

pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

mkdir -p "$SERVE_DIR/api/v1/crates"

start_server() {
  ( cd "$SERVE_DIR" && python3 -u -m http.server 0 ) >"$SERVE_DIR/server.log" 2>&1 &
  PID=$!
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
}

# Install a fake `cargo` shim that records its argv and exits with the
# code from $CARGO_SHIM_EXIT (default 0). The shim does NOT touch the
# real cargo binary so tests are hermetic.
install_cargo_shim() {
  cat >"$SHIM_DIR/cargo" <<'SHIM'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CARGO_LOG"
exit "${CARGO_SHIM_EXIT:-0}"
SHIM
  chmod +x "$SHIM_DIR/cargo"
}

reset_shim_log() { : >"$CARGO_LOG"; }

install_cargo_shim
reset_shim_log

# Fixture JSON for the fake registry — one crate at 0.6.0 (matches the
# Axis-E held-version scenario the slice exists to fix).
mkdir -p "$FIX_DIR"
cat >"$FIX_DIR/fathomdb-embedder-api.json" <<'JSON'
{
  "crate": {"name": "fathomdb-embedder-api"},
  "versions": [
    {"num": "0.6.0", "yanked": false},
    {"num": "0.5.1", "yanked": false}
  ]
}
JSON
# Pre-release fixture: a crate whose registry has 0.6.1-rc.1 already.
cat >"$FIX_DIR/fathomdb-engine.json" <<'JSON'
{
  "crate": {"name": "fathomdb-engine"},
  "versions": [
    {"num": "0.6.1-rc.1", "yanked": false},
    {"num": "0.6.0", "yanked": false}
  ]
}
JSON
# Crate with NO matching version — should publish.
cat >"$FIX_DIR/fathomdb-schema.json" <<'JSON'
{
  "crate": {"name": "fathomdb-schema"},
  "versions": [
    {"num": "0.6.0", "yanked": false}
  ]
}
JSON

cp "$FIX_DIR/fathomdb-embedder-api.json" "$SERVE_DIR/api/v1/crates/fathomdb-embedder-api"
cp "$FIX_DIR/fathomdb-engine.json"       "$SERVE_DIR/api/v1/crates/fathomdb-engine"
cp "$FIX_DIR/fathomdb-schema.json"       "$SERVE_DIR/api/v1/crates/fathomdb-schema"
# Malformed JSON fixture — HTTP 200 but body is not valid JSON (truncated).
printf '{bad json' >"$SERVE_DIR/api/v1/crates/fathomdb-broken"

start_server

REG="http://127.0.0.1:${PORT}"
run_helper() {
  PATH="$SHIM_DIR:$PATH" \
  CARGO_PUBLISH_IF_NEW_REGISTRY="$REG" \
  CARGO_REGISTRY_TOKEN="test-token" \
  "$HELPER" "$@"
}

# 1) Crate version already on the registry → skip; cargo NOT invoked.
reset_shim_log
if out="$(CARGO_PUBLISH_IF_NEW_LOCAL_VERSION=0.6.0 \
            run_helper fathomdb-embedder-api 2>&1)"; then
  if printf '%s' "$out" | grep -q 'already published; skipping' \
     && ! [ -s "$CARGO_LOG" ]; then
    pass "skip when registry already has the version"
  else
    fail "skip-case: log='$out' shim_log='$(cat "$CARGO_LOG")'"
  fi
else
  fail "skip-case: helper exited non-zero: $out"
fi

# 2) Crate version not on the registry → cargo publish invoked, exit 0.
reset_shim_log
if out="$(CARGO_PUBLISH_IF_NEW_LOCAL_VERSION=0.6.1 \
            run_helper fathomdb-schema 2>&1)"; then
  if grep -q -- '-p fathomdb-schema' "$CARGO_LOG" \
     && grep -q 'publish' "$CARGO_LOG" \
     && ! grep -q -- '--dry-run' "$CARGO_LOG"; then
    pass "publish when registry lacks the version"
  else
    fail "publish-case: shim_log='$(cat "$CARGO_LOG")' helper='$out'"
  fi
else
  fail "publish-case: helper exited non-zero: $out"
fi

# 3) Pre-release version present in registry → still skip.
reset_shim_log
if out="$(CARGO_PUBLISH_IF_NEW_LOCAL_VERSION=0.6.1-rc.1 \
            run_helper fathomdb-engine 2>&1)"; then
  if printf '%s' "$out" | grep -q 'already published; skipping' \
     && ! [ -s "$CARGO_LOG" ]; then
    pass "skip pre-release version already on registry"
  else
    fail "rc-skip-case: log='$out' shim='$(cat "$CARGO_LOG")'"
  fi
else
  fail "rc-skip-case: helper exited non-zero: $out"
fi

# 4) --dry-run forwards to cargo publish --dry-run regardless of registry state.
reset_shim_log
if out="$(CARGO_PUBLISH_IF_NEW_LOCAL_VERSION=0.6.0 \
            run_helper --dry-run fathomdb-embedder-api 2>&1)"; then
  if grep -q -- '--dry-run' "$CARGO_LOG" \
     && grep -q -- '-p fathomdb-embedder-api' "$CARGO_LOG"; then
    pass "--dry-run forwards regardless of registry state"
  else
    fail "dry-run-case: shim='$(cat "$CARGO_LOG")' helper='$out'"
  fi
else
  fail "dry-run-case: helper exited non-zero: $out"
fi

# 5) cargo publish failure must propagate non-zero.
reset_shim_log
if CARGO_SHIM_EXIT=101 CARGO_PUBLISH_IF_NEW_LOCAL_VERSION=0.6.1 \
     run_helper fathomdb-schema >/dev/null 2>&1; then
  fail "publish-failure-case: should have exited non-zero"
else
  pass "publish failure propagates non-zero exit"
fi

# 6) Missing arg → usage exit.
if out="$("$HELPER" 2>&1)"; then
  fail "missing-arg-case: should have failed"
else
  if printf '%s' "$out" | grep -qi 'usage'; then
    pass "missing arg → usage"
  else
    fail "missing-arg-case: wrong diagnostic: $out"
  fi
fi

# 7) Regression guard: helper must set a User-Agent header (crates.io
# returns HTTP 403 without one — same constraint as assert-co-tagging.sh).
if grep -q 'User-Agent:' "$HELPER"; then
  pass "helper sets User-Agent for crates.io"
else
  fail "helper missing User-Agent header"
fi

# 8) [FINDING 1] Malformed JSON from registry → exit 2 (fail-closed); cargo NOT
#    invoked. Before the fix the jq lookup silently falls through to "not found",
#    the helper returns 1 and invokes cargo publish — a fail-open bug.
reset_shim_log
set +e
out="$(CARGO_PUBLISH_IF_NEW_LOCAL_VERSION=0.6.1 run_helper fathomdb-broken 2>&1)"
malformed_rc=$?
set -e
if [ "$malformed_rc" -eq 2 ] \
   && printf '%s' "$out" | grep -qi 'malformed' \
   && ! [ -s "$CARGO_LOG" ]; then
  pass "malformed registry JSON → exit 2, fail-closed, no cargo call"
else
  fail "malformed-json-case: expected exit 2 + 'malformed' diagnostic + no cargo call; got rc=$malformed_rc cargo='$(cat "$CARGO_LOG")' out='$out'"
fi

# 9) [FINDING 2a] Manifest reader — inline version. Read version directly from
#    the real fathomdb-embedder-api Cargo.toml (version = "0.6.0"); no
#    CARGO_PUBLISH_IF_NEW_LOCAL_VERSION bypass. Registry has 0.6.0 → skip.
reset_shim_log
if out="$(run_helper fathomdb-embedder-api 2>&1)"; then
  if printf '%s' "$out" | grep -q 'already published; skipping' \
     && ! [ -s "$CARGO_LOG" ]; then
    pass "manifest reader: inline version (0.6.0) → skip already-published"
  else
    fail "manifest-inline-case: log='$out' shim_log='$(cat "$CARGO_LOG")'"
  fi
else
  fail "manifest-inline-case: helper exited non-zero: $out"
fi

# 10) [FINDING 2b] Manifest reader — workspace-inherited version. Read version
#     from the real fathomdb-engine Cargo.toml (version.workspace = true →
#     0.6.1 per [workspace.package]); no CARGO_PUBLISH_IF_NEW_LOCAL_VERSION
#     bypass. Registry has only 0.6.1-rc.1 and 0.6.0 → publish.
reset_shim_log
if out="$(run_helper fathomdb-engine 2>&1)"; then
  if grep -q -- '-p fathomdb-engine' "$CARGO_LOG" \
     && grep -q 'publish' "$CARGO_LOG" \
     && ! grep -q -- '--dry-run' "$CARGO_LOG"; then
    pass "manifest reader: workspace-inherited version (0.6.1) → cargo publish"
  else
    fail "manifest-workspace-case: shim_log='$(cat "$CARGO_LOG")' helper='$out'"
  fi
else
  fail "manifest-workspace-case: helper exited non-zero: $out"
fi

stop_server

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll cargo-publish-if-new tests passed\n'
