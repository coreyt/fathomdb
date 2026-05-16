#!/usr/bin/env bash
# scripts/tests/test_set_version.sh — coverage for set-version.sh
# two-axis enforcement per dev/design/release.md § Version axes.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SV="$REPO_ROOT/scripts/set-version.sh"

CARGO="$REPO_ROOT/Cargo.toml"
PYPROJ="$REPO_ROOT/src/python/pyproject.toml"
NPMPKG="$REPO_ROOT/src/ts/package.json"
EMBAPI="$REPO_ROOT/src/rust/crates/fathomdb-embedder-api/Cargo.toml"

FAILED=0
TMPDIR_ROOT="$(mktemp -d)"
trap 'restore; rm -rf "$TMPDIR_ROOT"' EXIT

# Snapshot the four manifests so the test never leaves the tree dirty.
SNAP="$TMPDIR_ROOT/snap"
mkdir -p "$SNAP"
cp "$CARGO" "$SNAP/Cargo.toml"
cp "$PYPROJ" "$SNAP/pyproject.toml"
cp "$NPMPKG" "$SNAP/package.json"
cp "$EMBAPI" "$SNAP/embedder-api.toml"

restore() {
  cp "$SNAP/Cargo.toml" "$CARGO" 2>/dev/null || true
  cp "$SNAP/pyproject.toml" "$PYPROJ" 2>/dev/null || true
  cp "$SNAP/package.json" "$NPMPKG" 2>/dev/null || true
  cp "$SNAP/embedder-api.toml" "$EMBAPI" 2>/dev/null || true
}

pass() { printf 'PASS  %s\n' "$1"; }
fail() { printf 'FAIL  %s\n' "$1" >&2; FAILED=$((FAILED + 1)); }

# Extract Axis W workspace version from Cargo.toml [workspace.package].
ws_version() {
  awk '
    /^\[workspace\.package\]/ { in_block = 1; next }
    /^\[/                     { in_block = 0 }
    in_block && /^version[[:space:]]*=/ {
      n = split($0, parts, "\"")
      if (n >= 3) { print parts[2] }
      exit
    }
  ' "$CARGO"
}

# Extract Axis E embedder-api version from its [package] block.
emb_version() {
  awk '
    /^\[package\]/ { in_block = 1; next }
    /^\[/          { in_block = 0 }
    in_block && /^version[[:space:]]*=/ {
      n = split($0, parts, "\"")
      if (n >= 3) { print parts[2] }
      exit
    }
  ' "$EMBAPI"
}

py_version() {
  awk '
    /^\[project\]/ { in_block = 1; next }
    /^\[/          { in_block = 0 }
    in_block && /^version[[:space:]]*=/ {
      n = split($0, parts, "\"")
      if (n >= 3) { print parts[2] }
      exit
    }
  ' "$PYPROJ"
}

npm_version() {
  sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$NPMPKG" | head -1
}

# 1. --check-files clean tree → 0.
restore
if "$SV" --check-files >/dev/null 2>&1; then
  pass "check-files clean tree exits 0"
else
  fail "check-files clean tree should exit 0"
fi

# 2. --workspace 9.9.9 updates all Axis W manifests; Axis E untouched.
restore
ORIG_EMB="$(emb_version)"
"$SV" --workspace 9.9.9 >/dev/null
if [ "$(ws_version)" = "9.9.9" ] && [ "$(py_version)" = "9.9.9" ] && [ "$(npm_version)" = "9.9.9" ]; then
  pass "workspace 9.9.9 updates Cargo+pyproject+package.json"
else
  fail "workspace 9.9.9 did not propagate: ws=$(ws_version) py=$(py_version) npm=$(npm_version)"
fi
if [ "$(emb_version)" = "$ORIG_EMB" ]; then
  pass "workspace 9.9.9 leaves Axis E ($ORIG_EMB) untouched"
else
  fail "workspace 9.9.9 mutated Axis E: $ORIG_EMB → $(emb_version)"
fi
if "$SV" --check-files >/dev/null 2>&1; then
  pass "check-files clean after --workspace 9.9.9"
else
  fail "check-files should be clean after --workspace 9.9.9"
fi

# 3. --embedder-api 8.8.8 updates only Axis E; Axis W untouched.
restore
ORIG_WS="$(ws_version)"
ORIG_PY="$(py_version)"
ORIG_NPM="$(npm_version)"
"$SV" --embedder-api 8.8.8 >/dev/null
if [ "$(emb_version)" = "8.8.8" ]; then
  pass "embedder-api 8.8.8 updates Axis E"
else
  fail "embedder-api 8.8.8 did not update Axis E: $(emb_version)"
fi
if [ "$(ws_version)" = "$ORIG_WS" ] && [ "$(py_version)" = "$ORIG_PY" ] && [ "$(npm_version)" = "$ORIG_NPM" ]; then
  pass "embedder-api 8.8.8 leaves Axis W untouched"
else
  fail "embedder-api 8.8.8 leaked into Axis W"
fi
if "$SV" --check-files >/dev/null 2>&1; then
  pass "check-files clean after --embedder-api 8.8.8"
else
  fail "check-files should be clean after --embedder-api 8.8.8"
fi

# 4. Manual drift on one Axis W file → --check-files exits 1 + names file.
restore
# Pyproject is the easiest to drift without disturbing Cargo TOML structure.
sed -i 's/^version = "[^"]*"/version = "7.7.7"/' "$PYPROJ"
if out="$("$SV" --check-files 2>&1)"; then
  fail "check-files should fail on drifted pyproject"
else
  case "$out" in
    *pyproject.toml*) pass "check-files names drifted file" ;;
    *) fail "check-files error must name pyproject.toml; got: $out" ;;
  esac
fi

# 5. Idempotent: --workspace <current> twice in a row → no second-pass change.
restore
CUR="$(ws_version)"
"$SV" --workspace "$CUR" >/dev/null
HASH1="$(sha256sum "$CARGO" "$PYPROJ" "$NPMPKG" | sha256sum)"
"$SV" --workspace "$CUR" >/dev/null
HASH2="$(sha256sum "$CARGO" "$PYPROJ" "$NPMPKG" | sha256sum)"
if [ "$HASH1" = "$HASH2" ]; then
  pass "workspace re-run is idempotent (hash stable)"
else
  fail "workspace re-run mutated content"
fi

# 6. Unknown flag → exit 2 with usage on stderr.
restore
if out="$("$SV" --bogus-flag 2>&1)"; then
  fail "unknown flag should exit non-zero"
else
  rc=$?
  if [ "$rc" -eq 2 ] && printf '%s' "$out" | grep -q -i 'usage'; then
    pass "unknown flag → exit 2 + usage"
  else
    fail "unknown flag wrong exit ($rc) or no usage; got: $out"
  fi
fi

# 7. Missing argument to --workspace → exit 2 with usage.
restore
if out="$("$SV" --workspace 2>&1)"; then
  fail "missing arg should exit non-zero"
else
  rc=$?
  if [ "$rc" -eq 2 ] && printf '%s' "$out" | grep -q -i 'usage'; then
    pass "missing arg → exit 2 + usage"
  else
    fail "missing arg wrong exit ($rc) or no usage; got: $out"
  fi
fi

# 8. Drift on Axis E (explicit version replaced with workspace inheritance) → fail.
restore
sed -i 's/^version = "0.6.0"$/version.workspace = true/' "$EMBAPI"
if out="$("$SV" --check-files 2>&1)"; then
  fail "check-files should fail when Axis E inherits workspace"
else
  case "$out" in
    *fathomdb-embedder-api*) pass "check-files flags Axis E inheritance regression" ;;
    *) fail "check-files must name fathomdb-embedder-api; got: $out" ;;
  esac
fi

restore

if [ "$FAILED" -gt 0 ]; then
  printf '\n%d test(s) failed\n' "$FAILED" >&2
  exit 1
fi
printf '\nAll set-version.sh tests passed\n'
