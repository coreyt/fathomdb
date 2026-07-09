#!/usr/bin/env bash
# scripts/tests/test_idempotent_republish.sh — R-REL-4c coordinated-publish
# resilience: per-registry retry/no-op idempotency across ALL THREE registries
# (crates.io, npm, PyPI). This is the prerequisite for the 0.8.20 OPP-12
# breaking-pair publish, where a partial-landing retry must NOT double-publish.
#
# Headline case `idempotent_republish_noops_all_registries`: with every target
# version ALREADY present on its registry, each helper exits 0 and does NOT
# invoke the underlying publish tool. Positive controls confirm an absent
# version DOES publish (so the no-op is not vacuous).
#
# Hermetic: python3 -m http.server serves fake registry responses; cargo / npm
# / twine are PATH shims that only record argv. No real registry is contacted.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CARGO_HELPER="$REPO_ROOT/scripts/release/cargo-publish-if-new.sh"
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

# --- fake registry --------------------------------------------------------
mkdir -p "$SERVE_DIR/api/v1/crates"      # crates.io shape: /api/v1/crates/<name>
mkdir -p "$SERVE_DIR/pypi"               # PyPI shape:      /pypi/<proj>/<ver>/json

# crates.io: fathomdb-engine already at 9.9.9 (the version under test).
cat >"$SERVE_DIR/api/v1/crates/fathomdb-engine" <<'JSON'
{"crate":{"name":"fathomdb-engine"},"versions":[{"num":"9.9.9","yanked":false}]}
JSON
# npm: main package "fathomdb" already has 9.9.9.
cat >"$SERVE_DIR/fathomdb" <<'JSON'
{"name":"fathomdb","versions":{"9.9.9":{"name":"fathomdb","version":"9.9.9"}}}
JSON
# crates.io: fathomdb-query EXISTS but lacks 9.9.9 (version-absent -> publish).
cat >"$SERVE_DIR/api/v1/crates/fathomdb-query" <<'JSON'
{"crate":{"name":"fathomdb-query"},"versions":[{"num":"1.0.0","yanked":false}]}
JSON
# npm: "fathomdb-absent" packument EXISTS but lacks 9.9.9 (version-absent).
cat >"$SERVE_DIR/fathomdb-absent" <<'JSON'
{"name":"fathomdb-absent","versions":{"1.0.0":{"name":"fathomdb-absent","version":"1.0.0"}}}
JSON
# PyPI: fathomdb 9.9.9 already released (file present -> 200).
mkdir -p "$SERVE_DIR/pypi/fathomdb/9.9.9"
printf '{"info":{"version":"9.9.9"}}' >"$SERVE_DIR/pypi/fathomdb/9.9.9/json"
# PyPI absent version 9.9.9 for project "fathomdb-absent" -> no file -> 404.

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
  [ "$PORT" -ne 0 ] || { fail "fixture http server failed to bind"; exit 1; }
}

# --- publish-tool shims (record argv, never do real work) -----------------
export TOOL_LOG="$SHIM_DIR/tool.log"
for tool in cargo npm twine; do
  cat >"$SHIM_DIR/$tool" <<SHIM
#!/usr/bin/env bash
printf '$tool %s\n' "\$*" >>"\$TOOL_LOG"
exit 0
SHIM
  chmod +x "$SHIM_DIR/$tool"
done
reset_log() { : >"$TOOL_LOG"; }

start_server
REG="http://127.0.0.1:${PORT}"

# ==========================================================================
# idempotent_republish_noops_all_registries
# ==========================================================================

# crates.io: version already present -> skip, cargo NOT invoked.
reset_log
if out="$(CARGO_PUBLISH_IF_NEW_REGISTRY="$REG" CARGO_REGISTRY_TOKEN=t \
          CARGO_PUBLISH_IF_NEW_LOCAL_VERSION=9.9.9 \
          PATH="$SHIM_DIR:$PATH" "$CARGO_HELPER" fathomdb-engine 2>&1)"; then
  if printf '%s' "$out" | grep -q 'already published; skipping' && ! grep -q '^cargo' "$TOOL_LOG" 2>/dev/null; then
    pass "crates.io re-run no-ops (cargo not invoked)"
  else
    fail "crates.io no-op: out='$out' tool='$(cat "$TOOL_LOG")'"
  fi
else
  fail "crates.io helper exited non-zero: $out"
fi

# npm: version already present -> skip, npm NOT invoked.
reset_log
mkdir -p "$WORK_DIR/main"
printf '{"name":"fathomdb","version":"9.9.9"}' >"$WORK_DIR/main/package.json"
if out="$(cd "$WORK_DIR/main" && NPM_PUBLISH_IF_NEW_REGISTRY="$REG" NPM_BIN=npm \
          PATH="$SHIM_DIR:$PATH" "$NPM_HELPER" --tag next 2>&1)"; then
  if printf '%s' "$out" | grep -q 'already published; skipping' && ! grep -q '^npm' "$TOOL_LOG" 2>/dev/null; then
    pass "npm re-run no-ops (npm not invoked)"
  else
    fail "npm no-op: out='$out' tool='$(cat "$TOOL_LOG")'"
  fi
else
  fail "npm helper exited non-zero: $out"
fi

# PyPI: version already present -> skip, twine NOT invoked.
reset_log
if out="$(PYPI_PUBLISH_IF_NEW_REGISTRY="$REG" TWINE_BIN=twine \
          PATH="$SHIM_DIR:$PATH" "$PYPI_HELPER" fathomdb 9.9.9 "$WORK_DIR" 2>&1)"; then
  if printf '%s' "$out" | grep -q 'already released; skipping' && ! grep -q '^twine' "$TOOL_LOG" 2>/dev/null; then
    pass "PyPI re-run no-ops (twine not invoked)"
  else
    fail "PyPI no-op: out='$out' tool='$(cat "$TOOL_LOG")'"
  fi
else
  fail "PyPI helper exited non-zero: $out"
fi

# ==========================================================================
# positive controls — an ABSENT version DOES publish (no-op is not vacuous)
# ==========================================================================

# crates.io: schema absent (registry query 404s) -> publish invoked.
reset_log
if out="$(CARGO_PUBLISH_IF_NEW_REGISTRY="$REG" CARGO_REGISTRY_TOKEN=t \
          CARGO_PUBLISH_IF_NEW_LOCAL_VERSION=9.9.9 \
          PATH="$SHIM_DIR:$PATH" "$CARGO_HELPER" fathomdb-query 2>&1)"; then
  grep -q '^cargo .*publish' "$TOOL_LOG" 2>/dev/null \
    && pass "crates.io absent version publishes (cargo invoked)" \
    || fail "crates.io absent: expected cargo publish; tool='$(cat "$TOOL_LOG")' out='$out'"
else
  fail "crates.io absent helper exited non-zero: $out"
fi

# npm: absent package -> publish invoked.
reset_log
mkdir -p "$WORK_DIR/absent"
printf '{"name":"fathomdb-absent","version":"9.9.9"}' >"$WORK_DIR/absent/package.json"
if out="$(cd "$WORK_DIR/absent" && NPM_PUBLISH_IF_NEW_REGISTRY="$REG" NPM_BIN=npm \
          PATH="$SHIM_DIR:$PATH" "$NPM_HELPER" --tag next 2>&1)"; then
  grep -q '^npm publish' "$TOOL_LOG" 2>/dev/null \
    && grep -q -- '--tag next' "$TOOL_LOG" 2>/dev/null \
    && pass "npm absent version publishes with dist-tag (npm invoked)" \
    || fail "npm absent: expected npm publish --tag next; tool='$(cat "$TOOL_LOG")' out='$out'"
else
  fail "npm absent helper exited non-zero: $out"
fi

# PyPI: absent version -> upload invoked with --skip-existing.
reset_log
if out="$(PYPI_PUBLISH_IF_NEW_REGISTRY="$REG" TWINE_BIN=twine \
          PATH="$SHIM_DIR:$PATH" "$PYPI_HELPER" fathomdb-absent 9.9.9 "$WORK_DIR" 2>&1)"; then
  grep -q '^twine upload' "$TOOL_LOG" 2>/dev/null \
    && grep -q -- '--skip-existing' "$TOOL_LOG" 2>/dev/null \
    && pass "PyPI absent version uploads with --skip-existing (twine invoked)" \
    || fail "PyPI absent: expected twine upload --skip-existing; tool='$(cat "$TOOL_LOG")' out='$out'"
else
  fail "PyPI absent helper exited non-zero: $out"
fi

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll idempotent-republish tests passed\n'
